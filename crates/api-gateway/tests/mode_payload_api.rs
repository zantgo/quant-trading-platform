// v7.2 — per-instance execution-mode payload tests: the instance list,
// the portfolio payload, and the automation payload all carry the
// instance's own fixed-at-launch mode. The automation payload also
// flags `ghost: true` for observe instances so the frontend can label
// tracked setups / projections as would-be previews.

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use config_models::ExecutionMode;
use portfolio_supervisor::instance::Instance;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn build_state(mode: ExecutionMode) -> (Arc<AppState>, Arc<Instance>) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    database_storage::crypto::init_master_key("mode-payload-secret");

    let instance = Arc::new(Instance::new_test(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::instance::TimeframeBuffers::new(),
    ));
    instance.set_execution_mode(mode).await;

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

async fn get_json(state: Arc<AppState>, uri: &str) -> serde_json::Value {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {uri} failed");
    let body = axum::body::to_bytes(resp.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn mode_str(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Observe => "observe",
        ExecutionMode::Paper => "paper",
        ExecutionMode::Live => "live",
    }
}

#[tokio::test]
async fn instance_list_carries_per_instance_mode() {
    for mode in [
        ExecutionMode::Observe,
        ExecutionMode::Paper,
        ExecutionMode::Live,
    ] {
        let (state, _inst) = build_state(mode).await;
        let json = get_json(state, "/api/instances").await;
        let first = &json["instances"][0];
        assert_eq!(first["id"], "inst_test");
        assert_eq!(first["mode"], mode_str(mode), "list mode mismatch");
    }
}

#[tokio::test]
async fn portfolio_payload_carries_mode() {
    for mode in [
        ExecutionMode::Observe,
        ExecutionMode::Paper,
        ExecutionMode::Live,
    ] {
        let (state, _inst) = build_state(mode).await;
        let json = get_json(state, "/api/instances/inst_test/portfolio").await;
        assert_eq!(json["instance_id"], "inst_test");
        assert_eq!(json["mode"], mode_str(mode), "portfolio mode mismatch");
    }
}

#[tokio::test]
async fn automation_payload_reports_instance_mode_and_ghost_flag() {
    for mode in [
        ExecutionMode::Observe,
        ExecutionMode::Paper,
        ExecutionMode::Live,
    ] {
        let (state, _inst) = build_state(mode).await;
        let json = get_json(state, "/api/instances/inst_test/automation").await;
        assert_eq!(json["mode"], mode_str(mode), "automation mode mismatch");
        let expect_ghost = mode == ExecutionMode::Observe;
        assert_eq!(
            json["ghost"], expect_ghost,
            "automation ghost flag mismatch for {mode:?}"
        );
    }
}
