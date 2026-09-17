// v7.1 follow-up — session init with mode + paper capital:
//   - paper mode stores the capital as the session default
//   - live mode without an active key is rejected with a clear error
//   - live mode with a key is accepted
//   - session status echoes mode + capital

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn build_state() -> Arc<AppState> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    database_storage::run_migrations(&pool).await.unwrap();
    database_storage::crypto::init_master_key("session-test-secret");

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
    })
}

async fn init_session(
    state: Arc<AppState>,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/session/init")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn session_status(state: Arc<AppState>) -> serde_json::Value {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/session/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn paper_session_stores_capital_and_mode() {
    let state = build_state().await;

    let (status, body) = init_session(
        state.clone(),
        serde_json::json!({
            "exchange": "Hyperliquid",
            "currency": "USDC",
            "mode": "paper",
            "portfolio_capital_usd": 2500.0,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mode"], "paper");
    assert_eq!(body["portfolio_capital_usd"], 2500.0);

    // Status echoes the defaults.
    let status_json = session_status(state.clone()).await;
    assert_eq!(status_json["active"], true);
    assert_eq!(status_json["mode"], "paper");
    assert_eq!(status_json["capital"], 2500.0);
}

#[tokio::test]
async fn live_session_without_key_is_rejected() {
    let state = build_state().await;
    let (status, body) = init_session(
        state.clone(),
        serde_json::json!({
            "exchange": "Hyperliquid",
            "currency": "USDC",
            "mode": "live",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("API key"));
}

#[tokio::test]
async fn live_session_with_key_is_accepted() {
    let state = build_state().await;
    let secret_enc = database_storage::crypto::encrypt_field("0xdeadbeef").unwrap();
    sqlx::query(
        "INSERT INTO exchange_keys (exchange, account_name, api_key, api_secret, is_active) \
         VALUES ('Hyperliquid', 'main', '0xabc', ?, 1)",
    )
    .bind(&secret_enc)
    .execute(&state.pool)
    .await
    .unwrap();

    let (status, body) = init_session(
        state.clone(),
        serde_json::json!({
            "exchange": "Hyperliquid",
            "currency": "USDC",
            "mode": "live",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mode"], "live");
}

#[tokio::test]
async fn invalid_mode_is_rejected() {
    let state = build_state().await;
    let (status, _body) = init_session(
        state.clone(),
        serde_json::json!({
            "exchange": "Hyperliquid",
            "currency": "USDC",
            "mode": "swing",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
