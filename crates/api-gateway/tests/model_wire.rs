//! **The model reachable on the wire** (P10-D, V-217/V-219/V-250).
//!
//! `GET /api/model` serves the stored `ModelState` envelope; `/api/overview`
//! carries the same pair's `cross_pair_state`. The test builds the payload with
//! the production assemblers — not hand-written JSON — so a change in the
//! reduction shape surfaces here rather than in the dashboard.

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use core_domain::axes::{
    AxisId, AxisValue, DiagnosticBasis, DiagnosticState, Diagnostics, Domain, Level, LevelBasis,
};
use core_domain::overview::OverviewMatrix;
use market_analyzer::model::{
    assemble_cross_tf_state, assemble_model_state, assemble_timeframe_state, Reading,
    TimeframeInput,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn state_for(secs: u64, atr: f64) -> core_domain::axes::TimeframeState {
    assemble_timeframe_state(TimeframeInput {
        identity: core_domain::axes::TimeframeIdentity {
            instance_id: "inst-1".into(),
            pair: "BTC-USDT".into(),
            exchange: "hyperliquid".into(),
            timeframe_secs: secs,
            timeframe_label: core_domain::duration_label(secs),
            candle_count: 500,
            last_closed_at: "2026-10-18T00:00:00Z".into(),
            available_at: "500".into(),
            spec_version: core_domain::SPEC_VERSION.into(),
            build_version: core_domain::BUILD_VERSION.into(),
        },
        readings: vec![Reading::produced(
            "d1",
            AxisId::Direction,
            AxisValue::signed(0.5, Domain::DirectionalBias),
            true,
        )],
        events: core_domain::event::EventCounts::default(),
        levels: vec![Level::new(
            "support",
            100.0,
            101.0,
            LevelBasis::DeclaredBand {
                indicator: "structure".into(),
            },
        )
        .expect("a real band")],
        atr: Some(atr),
        diagnostics: Diagnostics {
            state: DiagnosticState::Coiling,
            volatility_change_pct: 0.0,
            participation_change_pct: 0.0,
            basis: DiagnosticBasis {
                bbwp_pct: 20.0,
                rvol_pct: 100.0,
            },
        },
        progression_stage: None,
        progression_progress: core_domain::leg::Progress::Value { value: 0.0 },
        target_version: 0,
        target_stable: false,
        available_at_bar: 500,
        direction_history: vec![0.5; 50],
        axis_signals: Default::default(),
        legs: Vec::new(),
    })
}

fn state() -> Arc<AppState> {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy pool");
    Arc::new(AppState {
        workspace: portfolio_supervisor::workspace_state::WorkspaceState::empty(),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        pool,
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: tokio::sync::mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".into(),
        bitget_ws_url: String::new(),
        bybit_ws_url: String::new(),
        clock_monitor: None,
        reliability: Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
        exchange_status: Arc::new(
            network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
        ),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        model_state: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: tokio::sync::broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

async fn get(router: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

#[tokio::test]
async fn the_envelope_and_the_cross_pair_state_are_published() {
    let app = state();
    let ladder = vec![1u64, 60u64];
    let durations = vec![(1u64, state_for(1, 0.1)), (60u64, state_for(60, 0.2))];
    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &durations);
    let instances = vec![("BTC-USDT".to_string(), cross, durations)];
    let model = assemble_model_state(
        core_domain::SPEC_VERSION,
        core_domain::BUILD_VERSION,
        1_760_000_000_000,
        &ladder,
        &instances,
        1,
    );
    *app.model_state.write().await = Some(model.clone());
    let mut overview = OverviewMatrix::empty();
    overview.cross_pair_state = Some(model.cross_pair_state.clone());
    overview.instance_count = 1;
    overview.active_symbols = vec!["BTC-USDT".into()];
    *app.overview.write().await = Some(overview);

    let router = api_gateway::build_router(app);
    let (status, body) = get(router.clone(), "/api/model").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ladder_secs"], serde_json::json!([1, 60]));
    assert_eq!(body["version"], core_domain::SPEC_VERSION);
    let pair = &body["instances"]["BTC-USDT"];
    assert!(pair["cross_tf_state"].is_object(), "level 2 is on the wire");
    assert!(
        pair["durations"]["60"]["axes"].is_array(),
        "every duration's timeframe_state is stored, not recomputed"
    );
    assert!(
        body["cross_pair_state"]["breadth"]["breadth_pct"].is_number(),
        "level 3 rides in the same envelope"
    );

    let (status, overview) = get(router, "/api/overview").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        overview["cross_pair_state"]["breadth"]["breadth_pct"],
        body["cross_pair_state"]["breadth"]["breadth_pct"],
        "one producer: the overview embeds the stored reduction"
    );
}

#[tokio::test]
async fn before_the_first_aggregation_the_model_is_null_not_invented() {
    let (status, body) = get(api_gateway::build_router(state()), "/api/model").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.is_null(),
        "not yet aggregated is not an empty document"
    );
}
