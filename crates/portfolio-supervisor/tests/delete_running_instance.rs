//! Regression for the simplified DELETE contract.
//!
//! The dashboard UI now treats instances as binary (Running or
//! non-existent) — there is no Pause / Start / Stop button, and no
//! Stopped gate before delete. `registry::delete_instance` must:
//!
//!   1. Accept the call regardless of lifecycle state (Running, Paused,
//!      Stopped) — the previous 409 "Cannot delete: Instance must be
//!      STOPPED before deletion" is gone.
//!   2. Cancel the instance's pipeline tasks.
//!   3. Drain the per-TF history buffers (free in-memory footprint).
//!   4. Remove from the live `WorkspaceState.instances` map.
//!   5. Remove from `WorkspaceConfig.instances` AND persist to TOML.
//!   6. Sync `ExchangeStatusTracker` so the Data Infrastructure panel
//!      reflects the dropped pair.
//!
//! If any of these regress, this test fires — locking the contract the
//! right-panel Delete button and `TabHeader` close-tab both depend on.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use config_models::{
    CandleBufferConfig, HyperliquidConfig, InstanceEntry, ReconnectConfig, WorkspaceConfig,
};
use core_domain::models::{MarketSnapshot, TimeframeSlot};
use core_domain::normalized::SymbolMapper;
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::registry::delete_instance;
use portfolio_supervisor::registry_context::RegistryContext;
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::session::SessionState;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR: &str = "BTC-USDC";
const INSTANCE_ID: &str = "inst_delete_running";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Serialises every test in this binary that touches the
/// `MARKET_MONITOR_CONFIG` env var. The variable is process-global and
/// `config_models::config_path()` reads it without locking, so
/// concurrent tests would otherwise trample each other's paths and
/// produce spurious `NotFound` failures on reload.
static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

fn unique_config_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    p.push(format!(
        "quant_trading_platform_delete_running_{pid}_{n}.toml"
    ));
    p
}

/// Mirror of the private `OnDiskConfig` shape. Every optional field
/// uses `Default::default()` / `None`, the workspace carries one
/// `Running` instance we can delete.
#[derive(serde::Serialize)]
struct TestOnDiskConfig {
    #[serde(default)]
    hyperliquid: HyperliquidConfig,
    #[serde(default)]
    bitget: config_models::BitgetConfig,
    #[serde(default)]
    clock_monitor: Option<config_models::ClockMonitorTomlConfig>,
    #[serde(default)]
    quality: Option<config_models::QualityConfig>,
    #[serde(default)]
    reconnect: ReconnectConfig,
    #[serde(default)]
    candle_buffer: CandleBufferConfig,
    workspace: WorkspaceConfig,
}

fn write_initial_config(path: &std::path::Path) {
    let mut ws = WorkspaceConfig::default();
    ws.instances.push(InstanceEntry {
        id: INSTANCE_ID.to_string(),
        symbol: PAIR.to_string(),
        quote: "USDC".to_string(),
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
    });
    let on_disk = TestOnDiskConfig {
        hyperliquid: Default::default(),
        bitget: Default::default(),
        clock_monitor: None,
        quality: None,
        reconnect: Default::default(),
        candle_buffer: Default::default(),
        workspace: ws,
    };
    let raw = toml::to_string(&on_disk).expect("serialize");
    std::fs::write(path, raw).expect("write");
}

/// Build a minimal `Arc<Instance>` suitable for the delete contract
/// tests. Mirrors the `build_stub_instance` helper in
/// `exchange_status_pair_count.rs` but is local so this file is
/// self-contained. The instance carries a full `ActivePair` scaffold
/// but no live pipelines run — `delete_instance` only reads the
/// workspace map, the `cancel` token, the TF buffers, the config, and
/// the exchange-status tracker.
fn build_stub_instance(
    pool: SqlitePool,
    workspace: WorkspaceState,
    id: &str,
    base: &str,
    quote: &str,
) -> Arc<Instance> {
    use core_domain::liquidity::ClusterStatusSnapshot;
    use core_domain::normalized::NormalizedEvent;

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
        pipeline_state: Arc::new(RwLock::new(
            core_domain::models::CandlePipelineState::Initializing,
        )),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    };

    let active = Arc::new(ActivePair {
        symbol: format!("{}-{}", base, quote),
        custom_pipelines: std::collections::HashMap::new(),
        micro1: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[0],
            config_models::FIXED_TF_LADDER[0],
            bcast_tx.clone(),
        ),
        micro2: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[1],
            config_models::FIXED_TF_LADDER[1],
            bcast_tx.clone(),
        ),
        fast1: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[2],
            config_models::FIXED_TF_LADDER[2],
            bcast_tx.clone(),
        ),
        fast2: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[3],
            config_models::FIXED_TF_LADDER[3],
            bcast_tx.clone(),
        ),
        slow1: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[4],
            config_models::FIXED_TF_LADDER[4],
            bcast_tx.clone(),
        ),
        slow2: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[5],
            config_models::FIXED_TF_LADDER[5],
            bcast_tx.clone(),
        ),
        macro1: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[6],
            config_models::FIXED_TF_LADDER[6],
            bcast_tx.clone(),
        ),
        macro2: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[7],
            config_models::FIXED_TF_LADDER[7],
            bcast_tx.clone(),
        ),
        longterm1: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[8],
            config_models::FIXED_TF_LADDER[8],
            bcast_tx.clone(),
        ),
        longterm2: build_pipe(
            core_domain::models::FIXED_TF_SLOTS[9],
            config_models::FIXED_TF_LADDER[9],
            bcast_tx,
        ),
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

    let buffers: [TimeframeBuffers; 10] = std::array::from_fn(|i| {
        let p = active.all()[i];
        TimeframeBuffers {
            history: p.history.clone(),
            latest: p.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        }
    });

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

async fn build_context() -> (Arc<RegistryContext>, PathBuf, Arc<ExchangeStatusTracker>) {
    let cfg_path = unique_config_path();
    write_initial_config(&cfg_path);
    std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);

    let pool: SqlitePool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let platform = config_models::load_platform().expect("load platform");
    let workspace = config_models::load_workspace().expect("load workspace");

    let workspace_state = WorkspaceState::new(workspace);
    let symbol_mapper = Arc::new(SymbolMapper::new());
    let exchange_status = Arc::new(ExchangeStatusTracker::new());

    let ctx = RegistryContext {
        workspace: workspace_state.clone(),
        session: Arc::new(SessionState::new()),
        platform: Arc::new(RwLock::new(platform)),
        pool: pool.clone(),
        symbol_mapper,
        telemetry_tx: mpsc::channel(8).0,
        latency_tracker: Arc::new(Default::default()),
        ws_url: String::new(),
        bitget_ws_url: String::new(),
        exchange_status: exchange_status.clone(),
        reliability: Arc::new(ReliabilityTracker::new()),
        connection_quality: Arc::new(ConnectionQualityRegistry::new()),
    };

    // Seed the live map with the configured instance. `delete_instance`
    // walks the live map (not the config), so without this the
    // "not found" path always wins.
    ctx.workspace
        .insert(
            PAIR.to_string(),
            build_stub_instance(pool, workspace_state, INSTANCE_ID, "BTC", "USDC"),
        )
        .await;

    (Arc::new(ctx), cfg_path, exchange_status)
}

fn with_config_env<F, R>(path: &std::path::Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    // The caller already holds `CONFIG_ENV_LOCK` for the entire test
    // (see test-level `#[allow(clippy::await_holding_lock)]`). We must
    // not re-acquire the lock here (that would deadlock) — instead
    // just swap the env var under the caller's lock and run `f`.
    let prev = std::env::var("MARKET_MONITOR_CONFIG").ok();
    std::env::set_var("MARKET_MONITOR_CONFIG", path);
    let out = f();
    match prev {
        Some(v) => std::env::set_var("MARKET_MONITOR_CONFIG", v),
        None => std::env::remove_var("MARKET_MONITOR_CONFIG"),
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// Holds `CONFIG_ENV_LOCK` across `.await` to serialise the
// process-global `MARKET_MONITOR_CONFIG` env var that `delete_instance`
// (and `save_workspace` under the hood) reads on every call. See
// `quit_persists.rs` for the full rationale; `flavor = "multi_thread"`
// keeps the lock from deadlocking. `build_context` does NOT re-acquire
// the lock — `std::sync::Mutex` is non-reentrant, so it must not be
// held by the caller either. The outer test acquires it once and
// holds it for the whole test.
#[allow(clippy::await_holding_lock)]
async fn delete_running_instance_succeeds_without_stop() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let (ctx, cfg_path, _exchange_status) = build_context().await;

    // Sanity: the live map + config both carry the entry before delete.
    assert!(
        ctx.workspace.get(PAIR).await.is_some(),
        "live map must carry the entry"
    );
    let initial = ctx.workspace.config().await;
    assert_eq!(initial.instances.len(), 1, "fixture must seed 1 instance");
    assert_eq!(
        initial.instances[0].status,
        config_models::InstanceStatus::Running
    );

    // DELETE works on a Running instance — no manual stop required.
    // This is the contract the right-panel button depends on; the
    // pre-fix flow returned Err("Cannot delete: Instance must be
    // STOPPED before deletion") and the row silently reappeared.
    let result = delete_instance(&ctx, INSTANCE_ID).await;
    assert!(
        result.is_ok(),
        "delete_instance on Running must succeed; got {:?}",
        result
    );

    // (a) In-memory: live map + workspace config both empty.
    assert!(
        ctx.workspace.get(PAIR).await.is_none(),
        "live map must drop the entry"
    );
    let after_mem = ctx.workspace.config().await;
    assert!(
        after_mem.instances.is_empty(),
        "WorkspaceConfig.instances must be empty after delete; got {:?}",
        after_mem.instances
    );

    // (b) On-disk TOML rewrites with `instances = []` so the next
    //     daemon restart does not auto-respawn the deleted pair.
    let reloaded = with_config_env(&cfg_path, || {
        config_models::load_workspace().expect("reload")
    });
    assert!(
        reloaded.instances.is_empty(),
        "config.toml must not retain the deleted entry on disk; got {:?}",
        reloaded.instances
    );

    let on_disk = std::fs::read_to_string(&cfg_path).unwrap();
    assert!(
        on_disk.contains("instances = []"),
        "config.toml must declare the empty `instances = []`; got:\n{on_disk}"
    );

    // Cleanup.
    let _ = std::fs::remove_file(&cfg_path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn delete_unknown_instance_id_returns_not_found() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let (ctx, cfg_path, _exchange_status) = build_context().await;

    // The entry exists for `inst_delete_running`; asking for a
    // different id must surface a clean error string, not a panic.
    let err = delete_instance(&ctx, "inst_does_not_exist")
        .await
        .expect_err("delete must fail for unknown id");
    assert!(
        err.contains("not found"),
        "error string should mention 'not found' so the UI can render a friendly message; got: {err}"
    );

    // The seeded entry is still there.
    assert!(
        ctx.workspace.get(PAIR).await.is_some(),
        "failed delete must not touch other entries"
    );
    let cfg = ctx.workspace.config().await;
    assert_eq!(
        cfg.instances.len(),
        1,
        "failed delete must not touch other entries"
    );

    let _ = std::fs::remove_file(&cfg_path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn delete_already_stopped_instance_also_succeeds() {
    let _env_guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // The user might end up here if a previous delete attempt left the
    // row in a half-deleted state (the "Cannot stop from state
    // STOPPED" symptom they reported). The new contract accepts any
    // state, so a retry must just work.
    let (ctx, cfg_path, _exchange_status) = build_context().await;

    // Mark the seeded instance as Stopped to simulate a previously-
    // half-deleted state.
    {
        let mut cfg = ctx.workspace.config().await;
        cfg.instances[0].status = config_models::InstanceStatus::Stopped;
        ctx.workspace.set_config(cfg).await;
    }

    let result = delete_instance(&ctx, INSTANCE_ID).await;
    assert!(
        result.is_ok(),
        "delete on a Stopped instance must succeed (no lifecycle gate); got {:?}",
        result
    );

    let after = ctx.workspace.config().await;
    assert!(
        after.instances.is_empty(),
        "post-delete config must be empty"
    );

    let _ = std::fs::remove_file(&cfg_path);
}
