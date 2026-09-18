//! v11.12 regression: growing the ACTIVE ladder past 10 durations used to
//! PANIC the recharge mid-build ("one rx per active duration" — the event
//! channels were hardcoded to the pre-v11.9 10-slot world), killing the
//! recharge after the old pipelines were already cancelled: the operator's
//! SAVE hung forever and the new columns never loaded.
//!
//! This test drives the operator's exact flow offline: a live scaffold
//! instance + `POST /api/config { timeframes: [all 14] }` with UNREACHABLE
//! REST (every ≥60s slot cold-starts via the per-slot bootstrap tolerance)
//! → the recharge must complete, install a 14-slot instance, and report 14
//! `active_secs` on `/api/instances`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use config_models::WorkspaceConfig;
use core_domain::models::MarketSnapshot;
use core_domain::normalized::SymbolMapper;
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

const INSTANCE_ID: &str = "inst_full_ladder";
const PAIR_KEY: &str = "BTC-USDT";
const FULL_LADDER: [u64; 14] = [
    1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
];

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_config_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    p.push(format!(
        "quant_trading_platform_full_ladder_{}_{}.toml",
        std::process::id(),
        n
    ));
    p
}

fn isolate_config() -> PathBuf {
    let path = unique_config_path();
    let seed = r#"
[workspace]
id = "main"
name = "Test"
default_currency = "USDT"
default_exchange = "Hyperliquid"
timeframes = [1, 3, 5, 15, 30, 60, 180, 300]
"#;
    std::fs::write(&path, seed).expect("seed isolated config");
    std::env::set_var("MARKET_MONITOR_CONFIG", &path);
    path
}

fn scaffold_pipeline(
    secs: u64,
    tx: broadcast::Sender<MarketSnapshot>,
    snap_hist: Arc<RwLock<VecDeque<MarketSnapshot>>>,
) -> TimeframePipeline {
    TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist,
        timeframe_secs: secs,
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.3))),
        fibonacci: FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(None)),
        cluster_status: Arc::new(RwLock::new(
            core_domain::liquidity::ClusterStatusSnapshot::pending("TEST", "test"),
        )),
        pipeline_state: Arc::new(RwLock::new(
            core_domain::models::CandlePipelineState::Initializing,
        )),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    }
}

async fn setup_app_with_instance() -> Arc<AppState> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

    let symbol_mapper = Arc::new(SymbolMapper::new());
    symbol_mapper
        .register(
            core_domain::normalized::Exchange::Hyperliquid,
            "BTC",
            PAIR_KEY,
        )
        .await;
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);

    let workspace = WorkspaceState::empty();

    let (mid_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (snapshot_tx, _snapshot_rx) =
        mpsc::channel::<core_domain::normalized::NormalizedEvent>(100);
    let cancel = CancellationToken::new();

    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let pipelines: Vec<TimeframePipeline> = FULL_LADDER
        .iter()
        .map(|&secs| {
            if secs == 1 {
                scaffold_pipeline(secs, mid_bcast.clone(), snap_hist.clone())
            } else {
                scaffold_pipeline(secs, broadcast::channel(8).0, snap_hist.clone())
            }
        })
        .collect();

    let pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        pipelines,
        active_secs: FULL_LADDER.to_vec(),
        snapshot_tx,
        cancel,
    });

    let buffers: Vec<TimeframeBuffers> = pair
        .all()
        .iter()
        .map(|pipe| TimeframeBuffers {
            history: pipe.history.clone(),
            latest: pipe.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        })
        .collect();

    let instance = Arc::new(Instance::new(
        INSTANCE_ID.to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid,
        pair.clone(),
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        FULL_LADDER.to_vec(),
        Default::default(),
    ));

    workspace.insert(PAIR_KEY.to_string(), instance).await;

    // A Running persisted entry so `POST /api/config` recharges this pair.
    let mut cfg = WorkspaceConfig::default();
    cfg.instances.push(config_models::InstanceEntry {
        id: INSTANCE_ID.into(),
        symbol: PAIR_KEY.into(),
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
    workspace.set_config(cfg).await;

    // Unreachable REST → every ≥60s bootstrap slot cold-starts (per-slot
    // tolerance) instead of reaching the live exchange.
    let mut platform = config_models::PlatformConfig::default();
    platform.hyperliquid.ws_url = "ws://127.0.0.1:9/ws".to_string();

    Arc::new(AppState {
        workspace,
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(platform)),
        pool,
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: String::new(),
        clock_monitor: None,
        reliability: Arc::new(ReliabilityTracker::new()),
        exchange_status: Arc::new(ExchangeStatusTracker::new()),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: broadcast::channel::<api_gateway::RechargeNotice>(64).0,
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

static CONFIG_FILE_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` on a dedicated 8 MiB thread — the recharge path re-parses
/// `config.toml` inside `save_workspace`, and in debug builds the winnow
/// frames exceed libtest's 2 MiB stack (see config_round_trip.rs).
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

#[test]
fn recharge_to_the_full_14_duration_ladder_survives() {
    let _serial = CONFIG_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cfg_path = isolate_config();
    run_on_big_stack("full_ladder_recharge", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(async {
                let state = setup_app_with_instance().await;
                let router = api_gateway::build_router(state.clone());

                let res = router
                    .clone()
                    .oneshot(
                        axum::http::Request::builder()
                            .method("POST")
                            .uri("/api/config")
                            .header("content-type", "application/json")
                            .body(axum::body::Body::from(
                                serde_json::json!({ "timeframes": FULL_LADDER }).to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .expect(
                        "POST must reach the server (the old code PANICKED the request task here)",
                    );
                assert!(
                    res.status().is_success(),
                    "save must succeed; got {}",
                    res.status()
                );

                // The recharged instance must report the FULL ladder.
                let list = router
                    .oneshot(
                        axum::http::Request::builder()
                            .uri("/api/instances")
                            .body(axum::body::Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert!(list.status().is_success());
                let body = axum::body::to_bytes(list.into_body(), 1024 * 1024)
                    .await
                    .unwrap();
                let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let active = &parsed["instances"][0]["active_secs"];
                assert_eq!(
                    active.as_array().map(|a| a.len()),
                    Some(14),
                    "instance must run all 14 durations; got {active}"
                );
            })
    });
    let _ = std::fs::remove_file(&cfg_path);
}
