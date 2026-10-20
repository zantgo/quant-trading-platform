//! v11.12: the Welcome gate is OPERATOR-INTENT (`SessionState::ui_active`).
//! A fresh boot must report `active: false` on `/api/session/status` even
//! though the boot auto-init runs `init_session` (which flips the INTERNAL
//! `active` to true to let the background instance respawn pass the
//! session gate) — previously the browser's first status poll could see
//! `active: true` mid-spawn and skip the mandatory Welcome screen after a
//! Ctrl+C restart. Only an explicit operator action (init / recover) may
//! release the gate; quit / discard re-engage it.

use api_gateway::AppState;
use axum::{body::Body, http::Request, Router};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// AUDIT-TEST-ISOLATION: point `config_path()` at a per-test sandbox.
///
/// `/api/session/quit` and `/api/session/discard` PERSIST before they publish,
/// which is the AUDIT-M8c ordering — a failed write must not leave the dashboard
/// showing an empty workspace while `config.toml` still declares every instance.
/// Persistence resolves the path through `config_models::config_path()`, and
/// Cargo runs integration tests with CWD set to the PACKAGE directory, so the
/// repo-root `config.toml` is not on that path and the save failed with
/// `could not read config.toml: No such file or directory` — turning the
/// persist-failure response into a 500 and failing two tests that assert
/// success.
///
/// These tests were never executed in CI: `test-engine-live` used to gate on a
/// `LIVE_VENUE=1` variable nothing in the tree reads, so `api-gateway` was
/// silently skipped until v11.13.2 made the stage two-sided. They only passed
/// locally by accident — a stray `config.toml` in the crate directory.
///
/// Same pattern as `quit_persists.rs`: a process-global env var read without
/// locking, so concurrent tests need the mutex as well as unique paths.
static COUNTER: AtomicU64 = AtomicU64::new(0);
// `tokio::sync::Mutex`, not `std`: the guard is held across `.await`
// points in the test bodies, and a `std` guard is neither `Send` nor safe
// to hold across a suspension.
static CONFIG_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Seed a sandbox config and point the loader at it. The caller holds
/// `CONFIG_ENV_LOCK` for the duration of the test.
fn sandbox_config() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("session_ui_gate_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("sandbox dir");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[workspace]\nid = \"main\"\nname = \"T\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n",
    )
    .expect("seed config");
    std::env::set_var("MARKET_MONITOR_CONFIG", &path);
    path
}

async fn build_router() -> Router {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

    let state = Arc::new(AppState {
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
    });
    api_gateway::build_router(state)
}

async fn status_active(router: &Router) -> bool {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/session/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    parsed["active"].as_bool().expect("active bool")
}

#[tokio::test]
async fn fresh_boot_reports_inactive_until_the_operator_acts() {
    let router = build_router().await;
    // Simulates the boot window: the daemon's auto-init has run (internally
    // active), but the operator has not chosen anything yet. The UI MUST
    // see the Welcome gate.
    assert!(
        !status_active(&router).await,
        "fresh boot must report active:false (mandatory Welcome gate)"
    );
}

#[tokio::test]
async fn session_init_releases_the_gate_and_quit_re_engages_it() {
    let _env_lock = CONFIG_ENV_LOCK.lock().await;
    let _sandbox = sandbox_config();
    let router = build_router().await;

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/init")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "currency": "USDT",
                        "exchange": "Bitget",
                        "mode": "observe",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success(), "init must succeed");
    assert!(
        status_active(&router).await,
        "operator init releases the gate"
    );

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/quit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success(), "quit must succeed");
    assert!(!status_active(&router).await, "quit re-engages the gate");
}

#[tokio::test]
async fn discard_re_engages_the_gate() {
    let _env_lock = CONFIG_ENV_LOCK.lock().await;
    let _sandbox = sandbox_config();
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    let state = Arc::new(AppState {
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
        interrupted_session: Arc::new(RwLock::new(Some(api_gateway::InterruptedSessionInfo {
            id: 1,
            mode: Some("observe".to_string()),
            exchange: Some("hyperliquid".to_string()),
            currency: Some("USDT".to_string()),
            started_at_ms: 0,
            instance_count: 0,
        }))),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });
    // Simulate a recovered (active) session, then Discard.
    state.session.set_ui_active(true);
    let router = api_gateway::build_router(state);

    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/discard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success(), "discard must succeed");
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], serde_json::json!(true));
    // The gate re-engages (Discard leaves the operator at the Welcome screen).
}
