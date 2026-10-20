// PAE v7 — backtest API integration test.
//
// Seeds a memory DB with recorded decision matrices, POSTs
// /api/backtest/run, asserts the simulated trade + NHST stats, then
// round-trips GET /api/backtest/:id.

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use core_domain::axes::{
    AxisId, AxisState, AxisValue, DiagnosticBasis, DiagnosticState, Diagnostics, Domain,
    EventCounts, Readiness, TimeframeIdentity, TimeframeState,
};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::models::MarketSnapshot;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn bull_state(secs: u64) -> TimeframeState {
    let axis = |axis, position| AxisState {
        axis,
        position,
        size: AxisValue::rank(100.0),
        readiness: Readiness::Solid,
        readings: Vec::new(),
    };
    TimeframeState {
        identity: TimeframeIdentity {
            instance_id: "inst".into(),
            pair: "BTC-USDC".into(),
            exchange: "hyperliquid".into(),
            timeframe_secs: secs,
            timeframe_label: core_domain::duration_label(secs),
            candle_count: 500,
            last_closed_at: "2026-10-20T00:00:00Z".into(),
            available_at: "500".into(),
            spec_version: core_domain::SPEC_VERSION.into(),
            build_version: core_domain::BUILD_VERSION.into(),
        },
        axes: vec![
            axis(
                AxisId::Direction,
                AxisValue::signed(0.8, Domain::DirectionalBias),
            ),
            axis(
                AxisId::Momentum,
                AxisValue::signed(0.4, Domain::Acceleration),
            ),
            axis(
                AxisId::Structure,
                AxisValue::signed(0.3, Domain::NetEventBias),
            ),
            axis(
                AxisId::Progression,
                AxisValue::ordinal(core_domain::axes::Stage::Mid),
            ),
        ],
        events: EventCounts::default(),
        levels: Vec::new(),
        atr: Some(20.0),
        legs: Vec::new(),
        diagnostics: Diagnostics {
            state: DiagnosticState::Expanding,
            volatility_change_pct: 0.0,
            participation_change_pct: 0.0,
            basis: DiagnosticBasis {
                bbwp_pct: 20.0,
                rvol_pct: 100.0,
            },
        },
        derived: core_domain::axes::DerivedState {
            consensus: core_domain::axes::Consensus {
                agreeing_axes: 0,
                of: 4,
                axes: Vec::new(),
                weakest_axis: None,
            },
            stability: core_domain::axes::Stability {
                state: core_domain::axes::StabilityState::Holding,
                changes_last_50_bars: 0,
                window: 50,
            },
        },
    }
}

fn long_snapshot(ts: u64, mid: f64) -> MarketSnapshot {
    let mut snap = MarketSnapshot::default();
    snap.symbol = "BTC-USDC".to_string();
    snap.timeframe_secs = 60;
    snap.timestamp = ts;
    snap.is_completed = Some(true);
    snap.mid_price = Decimal::from_f64_retain(mid).unwrap_or_default();
    snap.bid_price = snap.mid_price;
    snap.ask_price = snap.mid_price;
    snap.close = Some(snap.mid_price);

    snap.timeframe_state = Some(bull_state(60));
    snap.indicators.insert(
        "atr".into(),
        NormalizedIndicatorValue::scalar(20.0, 0.0, "X"),
    );
    snap
}

async fn build_state() -> Arc<AppState> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

    // Seed: accept at 105 → the 94 print fills the resting long → TP at 170.
    for (ts, mid) in [(1000u64, 105.0f64), (1001, 94.0), (1002, 170.0)] {
        database_storage::insert_snapshot_internal(&pool, &long_snapshot(ts, mid)).await;
    }

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
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: "".to_string(),
        bybit_ws_url: "".to_string(),
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

#[tokio::test]
async fn backtest_run_and_get_round_trip() {
    let state = build_state().await;

    // The seeded snapshots carry second-valued timestamps (1000..=1002).
    // The request sends real millisecond timestamps — the ms→s conversion
    // must land the window on those rows (regression for the unit-mismatch
    // bug that made UI-driven backtests always return "not enough data").
    let body = serde_json::json!({
        "symbol": "BTC-USDC",
        "timeframe_secs": 60,
        "from_ms": 1_000_000,
        "to_ms": 1_010_000,
        "portfolio_capital_usd": 1000.0,
    });

    let router = api_gateway::build_router(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/backtest/run")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // v8.2 async contract: the POST returns { run_id, status } — poll
    // progress until the run completes, then load the persisted result.
    let run_id = json["run_id"].as_i64().unwrap();
    assert!(run_id > 0);
    assert_eq!(json["status"], "running");

    let mut backtest_id: Option<i64> = None;
    for _ in 0..200 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let router = api_gateway::build_router(state.clone());
        let resp = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/backtest/progress/{run_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let status = pjson["status"].as_str().unwrap_or("running");
        if status != "running" {
            assert_eq!(status, "completed", "{}", pjson);
            backtest_id = pjson["backtest_id"].as_i64();
            break;
        }
    }
    let id = backtest_id.expect("run completed with a persisted backtest_id");

    // Round-trip via GET /api/backtest/:id.
    let router = api_gateway::build_router(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/backtest/{}", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["summary"]["total_trades"], 1);
    assert_eq!(json["summary"]["win_count"], 1);
    assert_eq!(json["trades"][0]["exit_reason"], "tp");
    assert_eq!(json["stats"]["alpha"], 0.05);
    assert!(json["equity_curve"].as_array().unwrap().len() >= 3);

    // Round-trip via GET /api/backtest/:id.
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/backtest/{}", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["backtest_id"], id);
    assert_eq!(json["summary"]["total_trades"], 1);
    assert_eq!(json["summary"]["win_count"], 1);
    assert!(json["stats"]["p_value"].as_f64().is_some());
}

#[tokio::test]
async fn backtest_run_400_not_enough_data_for_empty_window() {
    let state = build_state().await;

    // A window far away from the seeded rows (seconds 1000..=1002) must
    // produce a loud 400 with coverage numbers, not a silent 200.
    let body = serde_json::json!({
        "symbol": "BTC-USDC",
        "timeframe_secs": 60,
        "from_ms": 50_000_000_000_i64,
        "to_ms": 50_000_100_000_i64,
        "portfolio_capital_usd": 1000.0,
    });

    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/backtest/run")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["code"], "not_enough_data");
    assert_eq!(json["snapshot_count"], 0);
    assert_eq!(json["earliest_secs"], 1000);
    assert_eq!(json["latest_secs"], 1002);
}

#[tokio::test]
async fn backtest_run_400_invalid_window_and_timeframe() {
    let state = build_state().await;

    for (body, expected_code) in [
        (
            serde_json::json!({
                "symbol": "BTC-USDC", "timeframe_secs": 60,
                "from_ms": 2_000_000, "to_ms": 1_000_000,
            }),
            "invalid_window",
        ),
        (
            serde_json::json!({
                "symbol": "BTC-USDC", "timeframe_secs": 0,
                "from_ms": 1_000_000, "to_ms": 1_010_000,
            }),
            "invalid_timeframe",
        ),
    ] {
        let router = api_gateway::build_router(state.clone());
        let resp = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/backtest/run")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], expected_code);
    }
}

#[tokio::test]
async fn backtest_get_404_for_unknown_id() {
    let state = build_state().await;
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/backtest/999999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
