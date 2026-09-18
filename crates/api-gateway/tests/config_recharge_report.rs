//! v11.11: a `POST /api/config` timeframes change must SURFACE pipeline
//! recharge failures in the response body (the config itself is still
//! persisted, so the status stays 200). Regression for the silent-200 that
//! let the UI show a ladder the running instances never adopted — the
//! "timeframes appear then disappear" bug's backend half.

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
use tokio::sync::{broadcast, mpsc, Notify, RwLock};
use tower::ServiceExt;

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

async fn setup_state_with_running_entry() -> Router {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    let workspace_state = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    let mut ws = config_models::WorkspaceConfig::default();
    // Running entry with NO live instance behind it — recharge_instance
    // fails with "Instance for pair ... not found" (fast error path A).
    ws.instances.push(config_models::InstanceEntry {
        id: "btc".into(),
        symbol: "BTC-USDT".into(),
        quote: "USDT".into(),
        status: config_models::InstanceStatus::Running,
        timeframes: std::collections::BTreeMap::new(),
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode: config_models::ExecutionMode::Observe,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    });
    workspace_state.set_config(ws).await;
    let state = Arc::new(AppState {
        workspace: workspace_state,
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        pool,
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(ConnectionQualityRegistry::new()),
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: String::new(),
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
    api_gateway::build_router(state)
}

#[test]
fn recharge_failure_is_surfaced_and_config_still_persists() {
    run_on_big_stack("recharge_report", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(async {
                // Sandbox the on-disk config path.
                let sandbox =
                    std::env::temp_dir().join(format!("recharge_report_{}", std::process::id()));
                std::fs::create_dir_all(&sandbox).unwrap();
                let cfg_path = sandbox.join("config.toml");
                std::fs::write(
                    &cfg_path,
                    "[workspace]\nid = \"main\"\nname = \"T\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n[workspace.candles]\nduration_seconds = 60\nanalysis_limit = 100\n",
                )
                .unwrap();
                std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);

                let router = setup_state_with_running_entry().await;

                let res = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/config")
                            .header("content-type", "application/json")
                            .body(Body::from(json!({ "timeframes": [60, 300] }).to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(res.status(), axum::http::StatusCode::OK);
                let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
                    .await
                    .unwrap();
                let body = String::from_utf8_lossy(&body).to_string();
                assert!(
                    body.contains("Configuration successfully saved."),
                    "saved prefix must stay: {body:?}"
                );
                assert!(
                    body.contains("pipeline recharge failed"),
                    "recharge failure must be surfaced: {body:?}"
                );

                // The ladder change itself must still be persisted and served.
                let get_res = router
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
                let cfg: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(
                    cfg["timeframes"],
                    json!([60, 300]),
                    "ladder change must persist even when recharge failed"
                );

                let _ = std::fs::remove_file(&cfg_path);
            })
    });
}
