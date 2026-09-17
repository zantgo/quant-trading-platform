//! Regression for Bug 2 — ExchangeStatus `Pairs: 0` despite a live instance.
//!
//! The legacy `register_exchange` used `entry(name).or_insert(...)`, which
//! silently dropped the count when the daemon had pre-seeded the
//! exchange with `active_pairs: 0`. The fix: `add_instance`,
//! `delete_instance`, and `recharge_instance` each call
//! `sync_exchange_status_active_pairs`, which walks the live workspace
//! and rewrites each exchange's count.
//!
//! This test exercises the helper directly: it inserts dummy `Instance`
//! objects into the workspace, invokes the sync helper, and asserts the
//! `ExchangeStatusTracker::report()` reflects the correct count per
//! exchange. We don't need to drive a full pipeline (DB, candles,
//! websockets) because the helper reads only the in-memory `WorkspaceState`
//! and writes only the `ExchangeStatusTracker` — both pure data.

use std::sync::Arc;

use core_domain::models::MarketSnapshot;
use core_domain::normalized::SymbolMapper;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::registry::sync_exchange_status_active_pairs;
use portfolio_supervisor::registry_context::RegistryContext;
use portfolio_supervisor::session::SessionState;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use tokio::sync::{mpsc, RwLock};

const HL_PAIR: &str = "BTC-USDC";
const _BG_PAIR: &str = "ETH-USDC";

async fn build_context() -> (Arc<RegistryContext>, Arc<ExchangeStatusTracker>) {
    let pool: SqlitePool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    let symbol_mapper = Arc::new(SymbolMapper::new());
    let exchange_status = Arc::new(ExchangeStatusTracker::new());
    // Seed both supported exchanges with the legacy `or_insert`-style
    // behaviour so the helper has to actually rewrite the count.
    exchange_status
        .seed_single("Hyperliquid", "wss://api.hyperliquid.xyz/ws")
        .await;
    exchange_status
        .seed_single("Bitget", "wss://ws.bitget.com/v2/ws/public")
        .await;

    let workspace = WorkspaceState::empty();
    let ctx = RegistryContext {
        workspace: workspace.clone(),
        session: Arc::new(SessionState::new()),
        platform: Arc::new(RwLock::new(Default::default())),
        pool,
        symbol_mapper,
        telemetry_tx: mpsc::channel(8).0,
        latency_tracker: Arc::new(Default::default()),
        ws_url: String::new(),
        bitget_ws_url: String::new(),
        exchange_status: exchange_status.clone(),
        reliability: Arc::new(ReliabilityTracker::new()),
        connection_quality: Arc::new(Default::default()),
    };
    (Arc::new(ctx), exchange_status)
}

fn active_pairs_by_name(
    report: &network_adapters::exchange_status_tracker::ExchangeStatusReport,
) -> std::collections::HashMap<String, u32> {
    report
        .exchanges
        .iter()
        .map(|e| (e.name.clone(), e.active_pairs))
        .collect()
}

/// Build a minimal `Arc<Instance>` suitable for the exchange-count test.
/// The instance carries a full `ActivePair` but no live pipelines run —
/// only the workspace map and the `pair` tuple are read by the sync
/// helper, so the scaffold is intentionally bare.
fn build_stub_instance(
    pool: SqlitePool,
    workspace: WorkspaceState,
    id: &str,
    base: &str,
    quote: &str,
) -> Arc<portfolio_supervisor::instance::Instance> {
    use config_models::FibonacciConfig;
    use core_domain::normalized::NormalizedEvent;
    use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
    use market_analyzer::indicators::DivergenceDetector;
    use market_analyzer::sr_engine::SrRoleTracker;
    use portfolio_supervisor::instance::TimeframeBuffers;
    use std::collections::VecDeque;

    let (bcast_tx, _) = tokio::sync::broadcast::channel::<MarketSnapshot>(8);
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let active_secs: Vec<u64> = config_models::SUPPORTED_DURATIONS.to_vec();
    let build_pipe = |secs: u64, tx| TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist.clone(),
        timeframe_secs: secs,
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
        fibonacci: FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(None)),
        cluster_status: Arc::new(RwLock::new(
            core_domain::liquidity::ClusterStatusSnapshot::pending(
                "TEST-USD",
                &core_domain::duration_label(secs),
            ),
        )),
        pipeline_state: Arc::new(RwLock::new(
            core_domain::models::CandlePipelineState::Initializing,
        )),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    };

    // Full supported-pool ActivePair (10 durations, fastest → slowest).
    let pipelines: Vec<TimeframePipeline> = (0..10)
        .map(|i| build_pipe(active_secs[i], bcast_tx.clone()))
        .collect();
    let active = Arc::new(ActivePair {
        symbol: format!("{}-{}", base, quote),
        pipelines,
        active_secs: active_secs.clone(),
        snapshot_tx: mpsc::channel::<NormalizedEvent>(8).0,
        cancel: tokio_util::sync::CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(Default::default()),
    });

    let buffers: Vec<TimeframeBuffers> = (0..10)
        .map(|i| {
            let p = &active.all()[i];
            TimeframeBuffers {
                history: p.history.clone(),
                latest: p.latest_snapshot.clone(),
                snapshot_history: snap_hist.clone(),
            }
        })
        .collect();

    Arc::new(portfolio_supervisor::instance::Instance::new(
        id.to_string(),
        (base.to_string(), quote.to_string()),
        // The previous hardcoded `Exchange::Hyperliquid` initial value at
        // `instance.rs:300` is overridden by this argument. Per-pair
        // exchange tagging drives the per-bucket counts in the
        // ExchangeStatusPanel; defaulting to Hyperliquid for tests that
        // don't care keeps the helper signature stable.
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid,
        active,
        pool,
        workspace,
        Default::default(),
        Default::default(),
        buffers,
        active_secs, // v11.9 active ladder
        Default::default(),
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_updates_hyperliquid_count_after_instance_added() {
    let (ctx, exchange_status) = build_context().await;

    // Baseline: both seeded with 0.
    let initial = active_pairs_by_name(&exchange_status.report().await);
    assert_eq!(initial.get("Hyperliquid"), Some(&0));
    assert_eq!(initial.get("Bitget"), Some(&0));

    // Seed the workspace directly with one BTC-USDC instance via the
    // low-level WorkspaceState::insert. The exchange-status helper counts
    // any Arc<Instance> in the live map whose pair base starts with one
    // of the registered exchange names; manually inserting keeps the test
    // free of the full `add_instance` pipeline machinery.
    ctx.workspace
        .insert(
            HL_PAIR.to_string(),
            build_stub_instance(
                ctx.pool.clone(),
                ctx.workspace.clone(),
                "inst_btc",
                "BTC",
                "USDC",
            ),
        )
        .await;
    sync_exchange_status_active_pairs(&ctx).await;
    let after = active_pairs_by_name(&exchange_status.report().await);
    assert_eq!(
        after.get("Hyperliquid"),
        Some(&1),
        "Hyperliquid count must reflect the live workspace"
    );
    assert_eq!(after.get("Bitget"), Some(&0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_handles_multiple_exchanges_independently() {
    let (ctx, exchange_status) = build_context().await;

    for (id, base) in [
        ("inst_btc", "BTC"),
        ("inst_eth", "ETH"),
        ("inst_sol", "SOL"),
    ] {
        let pair = format!("{}-USDC", base);
        ctx.workspace
            .insert(
                pair,
                build_stub_instance(ctx.pool.clone(), ctx.workspace.clone(), id, base, "USDC"),
            )
            .await;
    }
    sync_exchange_status_active_pairs(&ctx).await;

    let after = active_pairs_by_name(&exchange_status.report().await);
    // BTC, ETH, and SOL all settle on Hyperliquid in our test setup.
    // The helper doesn't filter by quote, only by base, so all three
    // roll up into the Hyperliquid bucket.
    assert_eq!(after.get("Hyperliquid"), Some(&3));
    assert_eq!(after.get("Bitget"), Some(&0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_resets_count_after_instance_removed() {
    let (ctx, exchange_status) = build_context().await;

    ctx.workspace
        .insert(
            "BTC-USDC".into(),
            build_stub_instance(
                ctx.pool.clone(),
                ctx.workspace.clone(),
                "inst_a",
                "BTC",
                "USDC",
            ),
        )
        .await;
    ctx.workspace
        .insert(
            "BTC2-USDC".into(),
            build_stub_instance(
                ctx.pool.clone(),
                ctx.workspace.clone(),
                "inst_b",
                "BTC",
                "USDC",
            ),
        )
        .await;
    sync_exchange_status_active_pairs(&ctx).await;
    assert_eq!(
        active_pairs_by_name(&exchange_status.report().await).get("Hyperliquid"),
        Some(&2)
    );

    ctx.workspace.remove("BTC-USDC").await;
    sync_exchange_status_active_pairs(&ctx).await;
    assert_eq!(
        active_pairs_by_name(&exchange_status.report().await).get("Hyperliquid"),
        Some(&1),
        "removing a workspace instance must drop the count by one"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_is_idempotent() {
    let (ctx, exchange_status) = build_context().await;
    ctx.workspace
        .insert(
            HL_PAIR.to_string(),
            build_stub_instance(
                ctx.pool.clone(),
                ctx.workspace.clone(),
                "inst_x",
                "BTC",
                "USDC",
            ),
        )
        .await;

    sync_exchange_status_active_pairs(&ctx).await;
    let first = exchange_status.report().await;

    sync_exchange_status_active_pairs(&ctx).await;
    let second = exchange_status.report().await;

    assert_eq!(
        first
            .exchanges
            .iter()
            .find(|e| e.name == "Hyperliquid")
            .map(|e| e.active_pairs),
        second
            .exchanges
            .iter()
            .find(|e| e.name == "Hyperliquid")
            .map(|e| e.active_pairs),
        "calling sync twice in a row must be a no-op"
    );
}

/// Regression: prior to the parity sweep, the helper hardcoded the
/// exchange label to "Hyperliquid" so Bitget's bucket always read 0
/// even when Bitget instances were live. The fix tags each instance
/// with its `ExchangeChoice` at construction and the helper buckets
/// accordingly. This test inserts one HL + one BG instance and asserts
/// each bucket carries the right count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_buckets_per_exchange() {
    use portfolio_supervisor::session::ExchangeChoice;
    let (ctx, exchange_status) = build_context().await;

    // HL: 1 instance
    ctx.workspace
        .insert(
            HL_PAIR.to_string(),
            build_stub_instance_v2(
                ctx.pool.clone(),
                ctx.workspace.clone(),
                "inst_btc",
                "BTC",
                "USDC",
                ExchangeChoice::Hyperliquid,
            ),
        )
        .await;
    // BG: 2 instances
    for (id, base) in [("inst_eth", "ETH"), ("inst_sol_bg", "SOL")] {
        ctx.workspace
            .insert(
                format!("{}-USDT", base),
                build_stub_instance_v2(
                    ctx.pool.clone(),
                    ctx.workspace.clone(),
                    id,
                    base,
                    "USDT",
                    ExchangeChoice::Bitget,
                ),
            )
            .await;
    }

    sync_exchange_status_active_pairs(&ctx).await;
    let after = active_pairs_by_name(&exchange_status.report().await);
    assert_eq!(
        after.get("Hyperliquid"),
        Some(&1),
        "Hyperliquid bucket must carry exactly 1 instance"
    );
    assert_eq!(
        after.get("Bitget"),
        Some(&2),
        "Bitget bucket must carry exactly 2 instances (was hardcoded 0 before parity sweep)"
    );
}

/// Variant of `build_stub_instance` that lets the caller pick the
/// `ExchangeChoice` so the per-bucket test can stamp HL vs BG tags.
fn build_stub_instance_v2(
    pool: SqlitePool,
    workspace: WorkspaceState,
    id: &str,
    base: &str,
    quote: &str,
    exchange: portfolio_supervisor::session::ExchangeChoice,
) -> Arc<portfolio_supervisor::instance::Instance> {
    use config_models::FibonacciConfig;
    use core_domain::normalized::NormalizedEvent;
    use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
    use market_analyzer::indicators::DivergenceDetector;
    use market_analyzer::sr_engine::SrRoleTracker;
    use portfolio_supervisor::instance::TimeframeBuffers;
    use std::collections::VecDeque;

    let (bcast_tx, _) = tokio::sync::broadcast::channel::<MarketSnapshot>(8);
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let active_secs: Vec<u64> = config_models::SUPPORTED_DURATIONS.to_vec();
    let build_pipe = |secs: u64, tx| TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist.clone(),
        timeframe_secs: secs,
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
        fibonacci: FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(None)),
        cluster_status: Arc::new(RwLock::new(
            core_domain::liquidity::ClusterStatusSnapshot::pending(
                "TEST-USD",
                &core_domain::duration_label(secs),
            ),
        )),
        pipeline_state: Arc::new(RwLock::new(
            core_domain::models::CandlePipelineState::Initializing,
        )),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    };

    let pipelines: Vec<TimeframePipeline> = (0..10)
        .map(|i| build_pipe(active_secs[i], bcast_tx.clone()))
        .collect();
    let active = Arc::new(ActivePair {
        symbol: format!("{}-{}", base, quote),
        pipelines,
        active_secs: active_secs.clone(),
        snapshot_tx: mpsc::channel::<NormalizedEvent>(8).0,
        cancel: tokio_util::sync::CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(Default::default()),
    });

    let buffers: Vec<TimeframeBuffers> = (0..10)
        .map(|i| {
            let p = &active.all()[i];
            TimeframeBuffers {
                history: p.history.clone(),
                latest: p.latest_snapshot.clone(),
                snapshot_history: snap_hist.clone(),
            }
        })
        .collect();

    Arc::new(portfolio_supervisor::instance::Instance::new(
        id.to_string(),
        (base.to_string(), quote.to_string()),
        exchange,
        active,
        pool,
        workspace,
        Default::default(),
        Default::default(),
        buffers,
        active_secs, // v11.9 active ladder
        Default::default(),
    ))
}
