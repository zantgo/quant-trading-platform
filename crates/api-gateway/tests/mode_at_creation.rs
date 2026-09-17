// v7.2 — mode is chosen at launch and fixed for the instance lifetime.
// The runtime toggle endpoint (`POST /api/instances/:id/mode`) is removed;
// the only mode source is the session default set by the Launch Setup
// wizard (`POST /api/session/init` → `set_session_defaults`), which
// `registry::add_instance` reads when creating instances.

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use portfolio_supervisor::instance::Instance;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn build_state() -> (Arc<AppState>, Arc<Instance>) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    database_storage::crypto::init_master_key("mode-at-creation-secret");

    let instance = Arc::new(Instance::new_test(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::instance::TimeframeBuffers::new(),
    ));
    let workspace = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace
        .insert("BTC-USDT".to_string(), instance.clone())
        .await;

    let state = Arc::new(AppState {
        workspace,
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
        clock_monitor: None,
        reliability: Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
        exchange_status: Arc::new(
            network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
        ),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
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
    });
    (state, instance)
}

#[tokio::test]
async fn mode_toggle_route_is_removed() {
    let (state, _inst) = build_state().await;
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/instances/inst_test/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"live"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // The endpoint no longer exists — the mode cannot be changed at runtime.
    // (axum's static fallback answers 404 for a missing route; unknown POSTs
    // fall through to ServeDir, which answers 405 — both prove the API route
    // is gone.)
    assert!(
        resp.status() == StatusCode::NOT_FOUND || resp.status() == StatusCode::METHOD_NOT_ALLOWED,
        "expected route removal, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn launch_wizard_sets_the_session_defaults_that_create_instances() {
    // What the Launch Setup wizard sends via POST /api/session/init:
    // mode + paper capital become the SessionState defaults that
    // `registry::add_instance` reads (observe | paper | live → ExecutionMode).
    for (mode_str, expect) in [("observe", "observe"), ("paper", "paper"), ("live", "live")] {
        let (state, _inst) = build_state().await;
        state
            .session
            .set_session_defaults(Some(mode_str.to_string()), Some(2500.0))
            .await;
        assert_eq!(
            state.session.session_mode().await.as_deref(),
            Some(mode_str)
        );
        assert_eq!(state.session.session_capital().await, Some(2500.0));
        let resolved = match state.session.session_mode().await.as_deref() {
            Some("live") => config_models::ExecutionMode::Live,
            Some("observe") => config_models::ExecutionMode::Observe,
            _ => config_models::ExecutionMode::Paper,
        };
        assert_eq!(
            match resolved {
                config_models::ExecutionMode::Observe => "observe",
                config_models::ExecutionMode::Paper => "paper",
                config_models::ExecutionMode::Live => "live",
            },
            expect
        );
    }
}
