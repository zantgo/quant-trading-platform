// v11.11 — crash recovery must land in the session posture the persisted
// instances describe (v7.3 doctrine: instances ARE the truth), never in the
// stale boot-time `mode` column of the interrupted row. An empty workspace
// recovers as the default observe posture — the reported bug was a fresh
// observe session recovering as paper because the boot row guessed "paper".

use api_gateway::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn build_state() -> Arc<AppState> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    database_storage::crypto::init_master_key("session-recovery-secret");

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

fn stale_row(mode: &str) -> api_gateway::InterruptedSessionInfo {
    api_gateway::InterruptedSessionInfo {
        id: 1,
        mode: Some(mode.to_string()),
        exchange: Some("hyperliquid".to_string()),
        currency: Some("USDT".to_string()),
        started_at_ms: 0,
        instance_count: 0,
    }
}

fn workspace_with_instance(mode: config_models::ExecutionMode) -> config_models::WorkspaceConfig {
    let mut ws = config_models::WorkspaceConfig::default();
    ws.instances.push(config_models::InstanceEntry {
        id: "btc".into(),
        symbol: "BTC-USDT".into(),
        quote: "USDT".into(),
        status: config_models::InstanceStatus::Running,
        timeframes: std::collections::BTreeMap::new(),
        strategy: None,
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    });
    ws
}

#[tokio::test]
async fn stale_paper_row_with_empty_workspace_recovers_as_observe() {
    let state = build_state().await;
    *state.interrupted_session.write().await = Some(stale_row("paper"));

    state
        .recover_interrupted_session()
        .await
        .expect("recovery must succeed");

    assert_eq!(
        state.session.session_mode().await.as_deref(),
        Some("observe")
    );
    assert_eq!(state.session.session_capital().await, None);
    assert!(state
        .session
        .active
        .load(std::sync::atomic::Ordering::Relaxed));
}

#[tokio::test]
async fn stale_paper_row_with_persisted_observe_instance_recovers_as_observe() {
    let state = build_state().await;
    state
        .workspace
        .set_config(workspace_with_instance(
            config_models::ExecutionMode::Observe,
        ))
        .await;
    *state.interrupted_session.write().await = Some(stale_row("paper"));

    state
        .recover_interrupted_session()
        .await
        .expect("recovery must succeed");

    assert_eq!(
        state.session.session_mode().await.as_deref(),
        Some("observe")
    );
}

#[tokio::test]
async fn persisted_paper_instance_wins_over_stale_observe_row() {
    let state = build_state().await;
    state
        .workspace
        .set_config(workspace_with_instance(config_models::ExecutionMode::Paper))
        .await;
    *state.interrupted_session.write().await = Some(stale_row("observe"));

    state
        .recover_interrupted_session()
        .await
        .expect("recovery must succeed");

    assert_eq!(state.session.session_mode().await.as_deref(), Some("paper"));
    // Paper sessions recover with the workspace capital default.
    assert!(state.session.session_capital().await.is_some());
}

#[tokio::test]
async fn persisted_live_instance_recovers_as_live() {
    let state = build_state().await;
    state
        .workspace
        .set_config(workspace_with_instance(config_models::ExecutionMode::Live))
        .await;
    *state.interrupted_session.write().await = Some(stale_row("paper"));

    state
        .recover_interrupted_session()
        .await
        .expect("recovery must succeed");

    assert_eq!(state.session.session_mode().await.as_deref(), Some("live"));
    assert_eq!(state.session.session_capital().await, None);
}
