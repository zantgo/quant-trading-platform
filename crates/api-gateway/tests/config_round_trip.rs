//! Config round-trip tests: `GET /api/config` → mutate → `POST /api/config`
//! must persist a bootable `config.toml` (workspace wrapped in `[workspace]`
//! with platform sections preserved) and re-serve the merged values.

use api_gateway::AppState;
use axum::{body::Body, http::Request, Router};
use core_domain::LatencyTracker;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::execution::ExecutionEngine;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, Notify, RwLock};
use tower::ServiceExt;

/// The on-disk config path is process-global (`MARKET_MONITOR_CONFIG`), so
/// the two tests must not interleave their sandbox setup/teardown.
static CONFIG_ENV_LOCK: Mutex<()> = Mutex::const_new(());

/// Run `f` on a dedicated 8 MiB thread. The POST /api/config path polls the
/// full tower middleware chain and then re-parses `config.toml` inside
/// `save_workspace` — in debug builds the combined winnow/tower stack
/// frames exceed libtest's default 2 MiB test-thread stack (the winnow
/// recursion alone reaches megabytes of frames), so these tests must run
/// with a larger stack. Release builds are unaffected; this is purely a
/// test-harness accommodation.
fn run_on_big_stack<F, T>(name: &str, f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("failed to spawn big-stack test thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

async fn setup_state_with_config(
    workspace: config_models::WorkspaceConfig,
) -> (Router, Arc<AppState>) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");
    let ws_url = "ws://127.0.0.1:1".to_string();
    let workspace_state = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace_state.set_config(workspace).await;
    let state = Arc::new(AppState {
        workspace: workspace_state,
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        pool: pool.clone(),
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(ConnectionQualityRegistry::new()),
        ws_url: ws_url.clone(),
        bitget_ws_url: ws_url,
        clock_monitor: None,
        reliability: Arc::new(ReliabilityTracker::new()),
        exchange_status: Arc::new(ExchangeStatusTracker::new()),
        latency_tracker: Arc::new(LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(Notify::new()),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });
    (api_gateway::build_router(state.clone()), state)
}

fn sample_workspace() -> config_models::WorkspaceConfig {
    let mut ws = config_models::WorkspaceConfig::default();
    ws.instances.push(config_models::InstanceEntry {
        id: "btc".into(),
        symbol: "BTC-USDT".into(),
        quote: "USDT".into(),
        status: config_models::InstanceStatus::Running,
        timeframes: [(
            60u64,
            config_models::TimeframeConfig::new(60, config_models::IndicatorsConfig::default()),
        )]
        .into_iter()
        .collect(),
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode: config_models::ExecutionMode::Paper,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    });
    ws.api_failover = config_models::ApiFailoverConfig {
        max_retries_per_call: 7,
        retry_delay_seconds: 12,
        max_consecutive_failures: 42,
    };
    ws
}

#[test]
fn config_post_round_trip_persists_merged_workspace() {
    run_on_big_stack("config_round_trip_main", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(config_post_round_trip_persists_merged_workspace_inner())
    })
}

async fn config_post_round_trip_persists_merged_workspace_inner() {
    let _guard = CONFIG_ENV_LOCK.lock().await;
    // Sandbox the on-disk config path so the test cannot clobber the
    // developer's real config.toml.
    let sandbox = std::env::temp_dir().join(format!("config_post_test_{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let cfg_path = sandbox.join("config.toml");
    // Seed a full on-disk file: platform sections + [workspace] wrapper.
    let seed = r#"
[hyperliquid]
ws_url = "wss://api.hyperliquid.xyz/ws"
rest_url = "https://api.hyperliquid.xyz/info"

[bitget]
ws_url = "wss://ws.bitget.com/v2/ws/public"
rest_url = "https://api.bitget.com"

[clock_monitor]
ntp_url = "pool.ntp.org"

[workspace]
id = "main"
name = "Test Workspace"
default_currency = "USDC"
default_exchange = "Hyperliquid"

[workspace.candles]
duration_seconds = 60
analysis_limit = 100

[workspace.indicators]
ema_fast = 8
"#;
    std::fs::write(&cfg_path, seed).unwrap();
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);

    let (router, _state) = setup_state_with_config(sample_workspace()).await;

    // GET the current config.
    let get_res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(get_res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let mut cfg: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(cfg["api_failover"]["max_consecutive_failures"], 42);

    // Mutate a couple of editable fields and POST the whole body back.
    cfg["candles"]["duration_seconds"] = json!(120);
    cfg["api_failover"]["max_retries_per_call"] = json!(9);

    let post_res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config")
                .header("content-type", "application/json")
                .body(Body::from(cfg.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = post_res.status();
    if status != axum::http::StatusCode::OK {
        let body = axum::body::to_bytes(post_res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        panic!(
            "POST /api/config failed: {} body={:?}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    // The persisted file must remain a bootable full config: platform
    // sections intact + [workspace] wrapper present + merged values.
    let persisted = std::fs::read_to_string(&cfg_path).unwrap();
    assert!(
        persisted.contains("[hyperliquid]"),
        "platform section destroyed: {}",
        persisted
    );
    assert!(
        persisted.contains("[workspace]"),
        "[workspace] wrapper missing: {}",
        persisted
    );
    assert!(
        persisted.contains("duration_seconds = 120"),
        "candles merge not persisted: {}",
        persisted
    );
    assert!(
        persisted.contains("max_retries_per_call = 9"),
        "api_failover merge not persisted: {}",
        persisted
    );

    // The file must parse back through the canonical loader.
    let (_platform, ws) = config_models::load().expect("persisted config must be bootable");
    assert_eq!(ws.candles.duration_seconds, 120);
    assert_eq!(ws.api_failover.max_retries_per_call, 9);
    assert_eq!(ws.instances.len(), 1);

    std::env::remove_var("MARKET_MONITOR_CONFIG");
    let _ = std::fs::remove_dir_all(&sandbox);
}

#[test]
fn config_post_without_platform_fields_keeps_runtime_config() {
    run_on_big_stack("config_round_trip_min", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(config_post_without_platform_fields_keeps_runtime_config_inner())
    })
}

async fn config_post_without_platform_fields_keeps_runtime_config_inner() {
    let _guard = CONFIG_ENV_LOCK.lock().await;
    let sandbox = std::env::temp_dir().join(format!("config_post_min_{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let cfg_path = sandbox.join("config.toml");
    let seed = "[workspace]\nid = \"main\"\nname = \"M\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n";
    std::fs::write(&cfg_path, seed).unwrap();
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);

    let (router, state) = setup_state_with_config(sample_workspace()).await;

    // A minimal partial body (no id/name, no instances) must not error and
    // must not clobber the loaded workspace.
    let post_res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "api_failover": { "max_retries_per_call": 3 } }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post_res.status(), axum::http::StatusCode::OK);

    let loaded = state.workspace.config().await;
    assert_eq!(loaded.api_failover.max_retries_per_call, 3);
    assert_eq!(
        loaded.instances.len(),
        1,
        "instances must survive a partial POST"
    );
    assert!(loaded.config_version > 1, "config_version must increment");

    std::env::remove_var("MARKET_MONITOR_CONFIG");
    let _ = std::fs::remove_dir_all(&sandbox);
}
