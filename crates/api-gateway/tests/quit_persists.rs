//! Regression for Bug 2 — `quit_session` silently leaves the workspace
//! persisted on disk, so the next daemon start auto-respawns every
//! "deleted" instance.
//!
//! Prior to this fix, `AppState::quit_session` cleared
//! `WorkspaceConfig.instances` in memory but never called
//! `config_models::save_workspace(&ws)`. On the next cold start the
//! `execution-daemon` boot loop (`crates/execution-daemon/src/main.rs`,
//! the `for entry in &workspace.instances` block) read the stale TOML
//! and re-spawned every entry. The user-visible symptom was that
//! closing the dashboard and reopening it brought the old instances
//! back, even though the dashboard's "Quit" dialog promised they would
//! be terminated.
//!
//! The fix: `quit_session` now calls `save_workspace(&ws)` after
//! clearing the in-memory list, so `config.toml` reflects the empty
//! workspace. This test exercises the full happy path:
//!   1. Write a `config.toml` with two pre-existing `InstanceEntry`s
//!      under a unique path; set `MARKET_MONITOR_CONFIG` to point at
//!      it via `config_models::config_path()`'s env override.
//!   2. Construct the full `AppState` and `init_session(...)`.
//!   3. Drive `state.quit_session().await`.
//!   4. Re-parse the TOML via `load_platform()` + `load_workspace()` and
//!      assert `instances.is_empty()`.
//!   5. Verify the in-memory state also reflects the cleared workspace,
//!      so the next `init_session` does not auto-respawn anything.
//!
//! Companion regression for the **live-map drift** bug: `WorkspaceState`
//! keeps TWO state holders (`config` and the live `instances` map).
//! Before the fix, `quit_session` only cleared `config`. The dashboard
//! reads `/api/instances` which returns from the live map, so the
//! user-visible symptom was: quit "succeeds", the TOML clears, but on
//! re-entry `/api/instances` still returns the old instances because
//! the live map wasn't dropped. `quit_session_clears_live_workspace_map`
//! (added below) is the targeted regression lock for this class of bug.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use api_gateway::AppState;
use config_models::{CandleBufferConfig, InstanceEntry, PlatformConfig, WorkspaceConfig};
use core_domain::liquidity::ClusterStatusSnapshot;
use core_domain::models::{CandlePipelineState, MarketSnapshot, TimeframeSlot};
use core_domain::normalized::{NormalizedEvent, SymbolMapper};
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::session::{Currency, ExchangeChoice};
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use std::collections::VecDeque;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const HL_PAIR: &str = "BTC-USDC";
const BG_PAIR: &str = "ETH-USDC";

/// Atomic counter so two concurrent test invocations never collide on the
/// same temp file path (Cargo runs tests in parallel by default).
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Serialises every test in this binary that touches the
/// `MARKET_MONITOR_CONFIG` env var. The variable is process-global and
/// `config_models::config_path()` reads it without locking, so
/// concurrent tests would otherwise trample each other's paths and
/// produce spurious `NotFound` failures on reload.
static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Build a unique temp file path for this test. The caller must
/// `std::fs::remove_file(...)` it afterwards; we deliberately do not
/// rely on a `tempfile` crate to avoid adding a new dev-dependency for
/// a single use-site.
fn unique_config_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    p.push(format!(
        "quant_trading_platform_quit_persists_{pid}_{n}.toml"
    ));
    p
}

/// Mirror of the private `OnDiskConfig` shape. The struct is
/// intentionally minimal — every optional field uses `Default::default()`
/// or `None`, and `workspace` carries the seeded entries. The TOML
/// produced by `toml::to_string` is round-trippable by
/// `config_models::load_platform()` + `load_workspace()`.
#[derive(serde::Serialize)]
struct TestOnDiskConfig {
    #[serde(default)]
    hyperliquid: config_models::HyperliquidConfig,
    #[serde(default)]
    bitget: config_models::BitgetConfig,
    #[serde(default)]
    clock_monitor: Option<config_models::ClockMonitorTomlConfig>,
    #[serde(default)]
    quality: Option<config_models::QualityConfig>,
    #[serde(default)]
    reconnect: config_models::ReconnectConfig,
    #[serde(default)]
    candle_buffer: CandleBufferConfig,
    workspace: WorkspaceConfig,
}

fn make_instance(id: &str, pair: &str) -> InstanceEntry {
    InstanceEntry {
        id: id.to_string(),
        symbol: pair.to_string(),
        quote: "USDT".to_string(),
        status: config_models::InstanceStatus::Running,
        micro_term: config_models::TimeframeConfig::new(60, Default::default()),
        fast_term: config_models::TimeframeConfig::new(180, Default::default()),
        slow_term: None,
        macro_term: None,
        automation: Default::default(),
        operational_mode: Default::default(),
        mode: config_models::ExecutionMode::Paper,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
        custom_pipelines: std::collections::HashMap::new(),
    }
}

fn write_initial_config(path: &std::path::Path) {
    let mut ws = WorkspaceConfig::default();
    // Hyperliquid settles exclusively in USDC (see
    // `init_session`'s `supports_currency` check in
    // `crates/api-gateway/src/lib.rs`); the test fixture matches that
    // contract so `init_session(USDC, Hyperliquid)` succeeds.
    ws.instances.push(make_instance("inst_btc", "BTC-USDC"));
    ws.instances.push(make_instance("inst_eth", "ETH-USDC"));
    let on_disk = TestOnDiskConfig {
        hyperliquid: Default::default(),
        bitget: Default::default(),
        clock_monitor: None,
        quality: None,
        reconnect: Default::default(),
        candle_buffer: Default::default(),
        workspace: ws,
    };
    let raw = toml::to_string(&on_disk).expect("serialize initial config");
    std::fs::write(path, raw).expect("write initial config.toml");
}

/// Build a minimal `Arc<Instance>` suitable for seeding the live
/// workspace map. Mirrors `build_stub_instance` in
/// `delete_running_instance.rs` — the two test files live in separate
/// binaries, so each carries its own copy.
fn build_stub_instance(
    pool: SqlitePool,
    workspace: WorkspaceState,
    id: &str,
    base: &str,
    quote: &str,
) -> Arc<Instance> {
    let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(8);
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let build_pipe = |slot: TimeframeSlot, secs: u64, tx| TimeframePipeline {
        slot,
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist.clone(),
        timeframe_secs: secs,
        timeframe_label: "Test",
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
        fibonacci: config_models::FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(None)),
        cluster_status: Arc::new(RwLock::new(ClusterStatusSnapshot::pending(
            "TEST-USD",
            &slot.as_str(),
        ))),
        pipeline_state: Arc::new(RwLock::new(CandlePipelineState::Initializing)),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    };
    let active = Arc::new(ActivePair {
        symbol: format!("{}-{}", base, quote),
        custom_pipelines: std::collections::HashMap::new(),
        micro1: build_pipe(TimeframeSlot::Micro1, 1, bcast_tx.clone()),
        micro2: build_pipe(TimeframeSlot::Micro2, 3, bcast_tx.clone()),
        fast1: build_pipe(TimeframeSlot::Fast1, 5, bcast_tx.clone()),
        fast2: build_pipe(TimeframeSlot::Fast2, 15, bcast_tx.clone()),
        slow1: build_pipe(TimeframeSlot::Slow1, 30, bcast_tx.clone()),
        slow2: build_pipe(TimeframeSlot::Slow2, 60, bcast_tx.clone()),
        macro1: build_pipe(TimeframeSlot::Macro1, 180, bcast_tx.clone()),
        macro2: build_pipe(TimeframeSlot::Macro2, 300, bcast_tx.clone()),
        longterm1: build_pipe(TimeframeSlot::Longterm1, 900, bcast_tx.clone()),
        longterm2: build_pipe(TimeframeSlot::Longterm2, 3600, bcast_tx),
        snapshot_tx: mpsc::channel::<NormalizedEvent>(8).0,
        cancel: CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(Default::default()),
        active_indices: (0..10).collect(),
    });
    let buffers: [TimeframeBuffers; 10] = active
        .all()
        .iter()
        .map(|pipe| TimeframeBuffers {
            history: pipe.history.clone(),
            latest: pipe.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or_else(|_| panic!("expected ten fixed-ladder buffers"));
    Arc::new(Instance::new(
        id.to_string(),
        (base.to_string(), quote.to_string()),
        ExchangeChoice::Hyperliquid,
        active,
        pool,
        workspace,
        Default::default(),
        Default::default(),
        buffers,
        config_models::FIXED_TF_LADDER.to_vec(), // v11.2 active ladder
        Default::default(),
    ))
}

async fn build_state_with_config(_config_path: PathBuf) -> Arc<AppState> {
    let pool: SqlitePool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    database_storage::run_migrations(&pool).await.unwrap();

    let symbol_mapper = Arc::new(SymbolMapper::new());

    // `MARKET_MONITOR_CONFIG` must already be set by the caller so the
    // loaders read the right path. The caller keeps the env var set
    // until after `quit_session` runs (so `save_workspace` writes back
    // to the same temp file).
    let platform = config_models::load_platform().expect("load platform config");
    let workspace = config_models::load_workspace().expect("load workspace config");

    let workspace_state = WorkspaceState::new(workspace);
    let session = Arc::new(portfolio_supervisor::session::SessionState::new());
    let (recharge_tx, _) = broadcast::channel::<api_gateway::RechargeNotice>(8);
    let snapshot_export_runtime = Arc::new(RwLock::new(
        core_domain::snapshot_export::SnapshotExportRuntime::default(),
    ));
    let snapshot_export_manual_tick = Arc::new(tokio::sync::Notify::new());
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(8);

    Arc::new(AppState {
        workspace: workspace_state,
        session,
        platform: Arc::new(RwLock::new(platform)),
        pool,
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(ConnectionQualityRegistry::new()),
        clock_monitor: None,
        reliability: Arc::new(ReliabilityTracker::new()),
        exchange_status: Arc::new(ExchangeStatusTracker::new()),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        ws_url: String::new(),
        bitget_ws_url: String::new(),
        overview: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx,
        snapshot_export: snapshot_export_runtime.clone(),
        snapshot_export_manual_tick: snapshot_export_manual_tick.clone(),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// The test holds `CONFIG_ENV_LOCK` (a `std::sync::Mutex`) across
// `.await` calls because `MARKET_MONITOR_CONFIG` is a process-global
// env var that `save_workspace` and `load_workspace` read without
// going through any per-instance state. The lock prevents concurrent
// tests in this binary from repointing the env var mid-test. The lint
// is suppressed for these four tests because the alternative (an
// async-aware mutex + retry loop) would add complexity for no real
// benefit — `flavor = "multi_thread"` ensures the lock never deadlocks.
#[allow(clippy::await_holding_lock)]
async fn quit_session_persists_empty_workspace_to_disk() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let cfg_path = unique_config_path();
    write_initial_config(&cfg_path);

    // `save_workspace` resolves its target via `MARKET_MONITOR_CONFIG`
    // (see `config_models::config_path()`); keep it pointed at the
    // temp file for the entire quit cycle so the rewrite lands where
    // the test expects it.
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);
    let state = build_state_with_config(cfg_path.clone()).await;

    // Seed the LIVE workspace map too — without this the live-map
    // assertion below is vacuously true and we miss the regression
    // where `quit_session` clears `config.instances` but leaves the
    // live `WorkspaceState.instances` map populated (which is what
    // `/api/instances` actually reads).
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    state
        .workspace
        .insert(
            HL_PAIR.to_string(),
            build_stub_instance(
                pool.clone(),
                state.workspace.clone(),
                "inst_btc",
                "BTC",
                "USDC",
            ),
        )
        .await;
    state
        .workspace
        .insert(
            BG_PAIR.to_string(),
            build_stub_instance(
                pool.clone(),
                state.workspace.clone(),
                "inst_eth",
                "ETH",
                "USDC",
            ),
        )
        .await;
    assert_eq!(
        state.workspace.len().await,
        2,
        "fixture must seed 2 live instances"
    );

    // Sanity: the in-memory config starts with both entries.
    let initial = state.workspace.config().await;
    assert_eq!(initial.instances.len(), 2, "fixture must seed 2 instances");

    // Initialize the session — quit must work without an active
    // session, but the boot loop only re-spawns when `init_session`
    // has been called at least once, so this mirrors the real flow.
    state
        .init_session(Currency::USDC, ExchangeChoice::Hyperliquid)
        .await
        .expect("init_session");

    // Quit.
    state
        .quit_session()
        .await
        .expect("quit_session must succeed");

    // (a) In-memory workspace reflects the deletion — BOTH sides:
    //     the declarative config AND the live runtime map. The latter
    //     is what `/api/instances` reads; clearing only `config` was
    //     the original bug.
    let after_mem = state.workspace.config().await;
    assert!(
        after_mem.instances.is_empty(),
        "in-memory instances must be cleared; got {} entries",
        after_mem.instances.len()
    );
    let live_after = state.workspace.list().await;
    assert!(
        live_after.is_empty(),
        "live workspace map must be empty after quit; /api/instances otherwise returns stale rows. \
         Got {} live entries",
        live_after.len()
    );
    assert_eq!(
        state.workspace.len().await,
        0,
        "live workspace map must be empty; the next init_session should NOT auto-spawn"
    );
    assert!(
        state.workspace.get(HL_PAIR).await.is_none(),
        "live entry for {} must be removed",
        HL_PAIR
    );
    assert!(
        state.workspace.get(BG_PAIR).await.is_none(),
        "live entry for {} must be removed",
        BG_PAIR
    );

    // (b) On-disk TOML was rewritten to match. Re-parse via the real
    // `MARKET_MONITOR_CONFIG` loader and assert `instances.is_empty()`.
    let reloaded = config_models::load_workspace().expect("reload after quit");
    assert!(
        reloaded.instances.is_empty(),
        "config.toml must be empty after quit; next daemon start must NOT auto-spawn; got {:?}",
        reloaded.instances
    );
    let on_disk = std::fs::read_to_string(&cfg_path).unwrap();
    // `WorkspaceConfig.instances` is `#[serde(default)]` so the TOML
    // round-trip uses the literal `instances = []` (an inline empty
    // array) rather than `[[workspace.instances]]` entries. The
    // contract is therefore "TOML must contain `instances = []` after
    // quit", which both (a) makes the empty state explicit and
    // (b) keeps the file diff-able across quits.
    assert!(
        on_disk.contains("instances = []"),
        "config.toml must declare `instances = []` after quit so future reloads see zero entries; got:\n{on_disk}"
    );
    assert!(
        !on_disk.contains("[[workspace.instances]]"),
        "config.toml must not retain any [[workspace.instances]] entries after quit; got:\n{on_disk}"
    );

    // Cleanup: drop the temp file but leave the env var in place —
    // subsequent tests in this binary re-set the lock-guarded
    // `MARKET_MONITOR_CONFIG` to point at their own `cfg_path`.
    let _ = std::fs::remove_file(&cfg_path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn quit_session_clears_live_workspace_map() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Standalone regression lock for the live-map drift bug. Without
    // the fix, this fails: the live `WorkspaceState.instances` map
    // still has the entry after `quit_session` returns Ok, so the
    // very next `GET /api/instances` re-emits the row to the
    // dashboard's right panel.
    let cfg_path = unique_config_path();
    write_initial_config(&cfg_path);
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);
    let state = build_state_with_config(cfg_path.clone()).await;

    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    state
        .workspace
        .insert(
            HL_PAIR.to_string(),
            build_stub_instance(pool, state.workspace.clone(), "inst_btc", "BTC", "USDC"),
        )
        .await;
    assert_eq!(
        state.workspace.len().await,
        1,
        "fixture must seed 1 live instance"
    );

    state
        .init_session(Currency::USDC, ExchangeChoice::Hyperliquid)
        .await
        .expect("init_session");
    state
        .quit_session()
        .await
        .expect("quit_session must succeed");

    // The LIVE map is what `/api/instances` reads. It must be empty.
    let live_after = state.workspace.list().await;
    assert!(
        live_after.is_empty(),
        "live workspace map must be empty after quit; /api/instances otherwise returns stale rows. \
         Got {} live entries",
        live_after.len()
    );
    assert_eq!(
        state.workspace.len().await,
        0,
        "WorkspaceState::len() must be 0 after quit"
    );
    assert!(
        state.workspace.get(HL_PAIR).await.is_none(),
        "WorkspaceState::get({}) must return None after quit",
        HL_PAIR
    );

    // Belt-and-braces: the declarative config is also cleared.
    let cfg = state.workspace.config().await;
    assert!(cfg.instances.is_empty());

    // Cleanup: see test above for rationale on leaving the env var.
    let _ = std::fs::remove_file(&cfg_path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn quit_session_then_init_session_does_not_respawn() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Belt-and-braces: after `quit_session` we call `init_session`
    // again and verify the workspace is still empty. This is the
    // scenario the user actually hits when they click "Quit" and then
    // re-select their exchange to start a fresh session.
    let cfg_path = unique_config_path();
    write_initial_config(&cfg_path);

    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);
    let state = build_state_with_config(cfg_path.clone()).await;
    state
        .init_session(Currency::USDC, ExchangeChoice::Hyperliquid)
        .await
        .expect("init 1");
    state.quit_session().await.expect("quit");
    state
        .init_session(Currency::USDC, ExchangeChoice::Hyperliquid)
        .await
        .expect("init 2");

    let cfg = state.workspace.config().await;
    assert!(
        cfg.instances.is_empty(),
        "fresh session must not auto-respawn anything"
    );

    // The on-disk version matches.
    let reloaded = config_models::load_workspace().expect("reload");
    assert!(reloaded.instances.is_empty());

    // Cleanup.
    std::env::remove_var("MARKET_MONITOR_CONFIG");
    let _ = std::fs::remove_file(&cfg_path);
}

/// Confirm `PlatformConfig` round-trips through `load_platform()` for
/// the `MARKET_MONITOR_CONFIG` path. Keeps the import of `PlatformConfig`
/// honest and documents the helper.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn platform_config_round_trips_through_env_override() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let cfg_path = unique_config_path();
    write_initial_config(&cfg_path);
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);

    let loaded: PlatformConfig = config_models::load_platform().expect("load platform");
    assert_eq!(loaded, PlatformConfig::default());

    std::env::remove_var("MARKET_MONITOR_CONFIG");
    let _ = std::fs::remove_file(&cfg_path);
}
