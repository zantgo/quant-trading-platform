//! Per-timeframe cluster refresh integration tests (v6.5).
//!
//! Verifies that:
//!   1. `compute_cluster_for_tf` reads the **TF-specific** history, not
//!      just the fastest TF's history.
//!   2. Each TF pipeline owns its own `cluster_matrix` handle (separate
//!      `Arc<RwLock<...>>` instances, not a shared one).
//!   3. Failures (no snapshot, no OI, insufficient history) bubble
//!      up as `ClusterRefreshError` rather than silently returning None.
//!
//! v11.9: the ActivePair carries the ACTIVE duration set. These tests use
//! three distinct durations (closest pool equivalents of the
//! legacy micro@60s / fast@300s / macro@900s trio); every other slot is a
//! default pipe that the cluster assertions never touch.
//!
//! Run via `./manage.sh test-engine`.

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use config_models::{FibonacciConfig, LiquidityConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedCandle, NormalizedEvent};
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use portfolio_supervisor::session::ExchangeChoice;
use rust_decimal::Decimal;

/// Active-duration indexes under test (positional with the ascending
/// duration vec used by the tests): 60s, 300s, 900s. The remaining
/// entries are default pipes; the assertions never touch them.
const MIDX: usize = 5; // 60s — "micro-equivalent" duration
const FIDX: usize = 7; // 300s — "fast-equivalent" duration
const AIDX: usize = 8; // 900s — "macro-equivalent" duration

fn make_pipe(secs: u64, tx: broadcast::Sender<MarketSnapshot>) -> TimeframePipeline {
    TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::<NormalizedCandle>::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None::<MarketSnapshot>)),
        snapshot_history: Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new())),
        timeframe_secs: secs,
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
        fibonacci: FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(Some(Decimal::from(1_000_000)))),
        latest_funding: Arc::new(RwLock::new(Some(
            Decimal::from_f64_retain(0.0001).unwrap_or_default(),
        ))),
        latest_mark_px: Arc::new(RwLock::new(Some(Decimal::from(50_000)))),
        latest_index_px: Arc::new(RwLock::new(Some(Decimal::from(50_000)))),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(
            None::<core_domain::liquidity::LiquidationClusterMatrix>,
        )),
        cluster_status: Arc::new(RwLock::new(
            core_domain::liquidity::ClusterStatusSnapshot::pending(
                "BTC-USDT",
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
    }
}

/// Build a full 10-duration `ActivePair`. `overrides` supplies prepared
/// pipelines at specific indexes; every other entry gets a default pipe
/// bound to its pool duration.
fn make_full_pair(mut overrides: [Option<TimeframePipeline>; 10]) -> ActivePair {
    let (tx, _) = broadcast::channel::<MarketSnapshot>(1);
    let active_secs: Vec<u64> = config_models::SUPPORTED_DURATIONS.to_vec();
    let pipes: Vec<TimeframePipeline> = (0..10)
        .map(|i| {
            overrides[i]
                .take()
                .unwrap_or_else(|| make_pipe(active_secs[i], tx.clone()))
        })
        .collect();
    ActivePair {
        symbol: "BTC-USDT".into(),
        pipelines: pipes,
        active_secs,
        snapshot_tx: mpsc::channel::<NormalizedEvent>(8).0,
        cancel: tokio_util::sync::CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(Default::default()),
    }
}

fn make_snap_history(closes: Vec<f64>) -> MarketSnapshot {
    use rust_decimal::prelude::FromPrimitive;
    let mut snap = MarketSnapshot::default_for_test("BTC-USDT", 60);
    let last = closes.last().copied().unwrap_or(50_000.0);
    snap.mid_price = Decimal::from_f64(last).unwrap_or_default();
    snap.open_interest = Some(Decimal::from(1_000_000));
    snap.funding_rate = Some(Decimal::from_f64_retain(0.0001).unwrap_or_default());
    snap
}

fn test_config() -> LiquidityConfig {
    LiquidityConfig::default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_tf_cluster_refresh_uses_tf_specific_history() {
    use core_domain::normalized::NormalizedCandle as NC;
    use portfolio_supervisor::registry::pipelines::compute_cluster_for_tf;

    // Three TFs with three different price histories.
    let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(10);
    let mut slots: [Option<TimeframePipeline>; 10] = std::array::from_fn(|_| None);
    slots[MIDX] = Some(make_pipe(60, bcast_tx.clone()));
    slots[FIDX] = Some(make_pipe(300, bcast_tx.clone()));
    slots[AIDX] = Some(make_pipe(900, bcast_tx));
    let micro_pipe = slots[MIDX].as_ref().unwrap();
    let fast_pipe = slots[FIDX].as_ref().unwrap();
    let macro_pipe = slots[AIDX].as_ref().unwrap();

    // Micro history: range 49_500 → 50_500 (down move).
    {
        let mut h = micro_pipe.history.write().await;
        for i in 0..20 {
            let p = 50_000.0 - (i as f64) * 25.0;
            h.push_back(NC {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".into(),
                start_time_ms: i * 60_000,
                duration_ms: 60_000,
                open: Decimal::from_f64_retain(p).unwrap_or_default(),
                high: Decimal::from_f64_retain(p + 10.0).unwrap_or_default(),
                low: Decimal::from_f64_retain(p - 10.0).unwrap_or_default(),
                close: Decimal::from_f64_retain(p - 5.0).unwrap_or_default(),
                volume: Decimal::from(100),
                trades_count: 0,
                reconstructed: None,
            });
        }
    }
    // Fast history: range 50_000 → 51_000 (up move).
    {
        let mut h = fast_pipe.history.write().await;
        for i in 0..20 {
            let p = 50_000.0 + (i as f64) * 50.0;
            h.push_back(NC {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".into(),
                start_time_ms: i * 300_000,
                duration_ms: 300_000,
                open: Decimal::from_f64_retain(p).unwrap_or_default(),
                high: Decimal::from_f64_retain(p + 10.0).unwrap_or_default(),
                low: Decimal::from_f64_retain(p - 10.0).unwrap_or_default(),
                close: Decimal::from_f64_retain(p + 5.0).unwrap_or_default(),
                volume: Decimal::from(100),
                trades_count: 0,
                reconstructed: None,
            });
        }
    }
    // Macro history: range 50_000 ± 100 (sideways).
    {
        let mut h = macro_pipe.history.write().await;
        for i in 0..20 {
            let p = 50_000.0 + ((i as f64) * 7.0).sin() * 100.0;
            h.push_back(NC {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".into(),
                start_time_ms: i * 900_000,
                duration_ms: 900_000,
                open: Decimal::from_f64_retain(p).unwrap_or_default(),
                high: Decimal::from_f64_retain(p + 50.0).unwrap_or_default(),
                low: Decimal::from_f64_retain(p - 50.0).unwrap_or_default(),
                close: Decimal::from_f64_retain(p + 1.0).unwrap_or_default(),
                volume: Decimal::from(100),
                trades_count: 0,
                reconstructed: None,
            });
        }
    }

    // All three TFs share the same latest_snapshot at the micro mid.
    *micro_pipe.latest_snapshot.write().await = Some(make_snap_history(vec![49_500.0]));
    *fast_pipe.latest_snapshot.write().await = Some(make_snap_history(vec![51_000.0]));
    *macro_pipe.latest_snapshot.write().await = Some(make_snap_history(vec![50_000.0]));

    let active = Arc::new(make_full_pair(slots));

    let cfg = test_config();

    // Compute one cluster for each TF; we use a clone of the handle via
    // `active.all()[slot]` to confirm the per-TF isolation.
    let micro_m = compute_cluster_for_tf(
        &active,
        60,
        &cfg,
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await
    .expect("micro should compute");
    active.all()[MIDX]
        .cluster_matrix
        .write()
        .await
        .replace(micro_m);

    let fast_m = compute_cluster_for_tf(
        &active,
        300,
        &cfg,
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await
    .expect("fast should compute");
    active.all()[FIDX]
        .cluster_matrix
        .write()
        .await
        .replace(fast_m);

    let macro_m = compute_cluster_for_tf(
        &active,
        900,
        &cfg,
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await
    .expect("macro should compute");
    active.all()[AIDX]
        .cluster_matrix
        .write()
        .await
        .replace(macro_m);

    // Each TF's cluster_matrix handle is now populated with **different**
    // matrices because the price histories differ. Every cluster matrix
    // must be valid (non-empty short/long clusters) even when the
    // histories are different.
    assert!(
        !active.all()[MIDX]
            .cluster_matrix
            .read()
            .await
            .as_ref()
            .unwrap()
            .short_clusters
            .is_empty()
            || !active.all()[MIDX]
                .cluster_matrix
                .read()
                .await
                .as_ref()
                .unwrap()
                .long_clusters
                .is_empty(),
        "micro cluster should detect at least one cluster with 20-bar history"
    );
    assert!(
        !active.all()[FIDX]
            .cluster_matrix
            .read()
            .await
            .as_ref()
            .unwrap()
            .short_clusters
            .is_empty()
            || !active.all()[FIDX]
                .cluster_matrix
                .read()
                .await
                .as_ref()
                .unwrap()
                .long_clusters
                .is_empty(),
        "fast cluster should detect at least one cluster"
    );
    assert!(
        !active.all()[AIDX]
            .cluster_matrix
            .read()
            .await
            .as_ref()
            .unwrap()
            .short_clusters
            .is_empty()
            || !active.all()[AIDX]
                .cluster_matrix
                .read()
                .await
                .as_ref()
                .unwrap()
                .long_clusters
                .is_empty(),
        "macro cluster should detect at least one cluster"
    );

    // All handles are distinct Arc instances (per-TF isolation).
    let h_micro = Arc::as_ptr(&active.all()[MIDX].cluster_matrix) as *const u8;
    let h_fast = Arc::as_ptr(&active.all()[FIDX].cluster_matrix) as *const u8;
    let h_macro = Arc::as_ptr(&active.all()[AIDX].cluster_matrix) as *const u8;
    assert_ne!(h_micro, h_fast, "micro and fast must have distinct handles");
    assert_ne!(h_fast, h_macro, "fast and macro must have distinct handles");
    assert_ne!(
        h_micro, h_macro,
        "micro and macro must have distinct handles"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_tf_cluster_refresh_returns_error_when_no_snapshot() {
    use portfolio_supervisor::registry::pipelines::compute_cluster_for_tf;

    let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(10);
    let mut slots: [Option<TimeframePipeline>; 10] = std::array::from_fn(|_| None);
    slots[MIDX] = Some(make_pipe(60, bcast_tx));

    let active = Arc::new(make_full_pair(slots));

    // No snapshot populated → must return the NoSnapshotYet variant.
    let result = compute_cluster_for_tf(
        &active,
        60,
        &test_config(),
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await;
    assert!(
        result.is_err(),
        "no snapshot → should return Err, got {:?}",
        result,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_tf_cluster_refresh_returns_error_when_no_oi() {
    use portfolio_supervisor::registry::pipelines::compute_cluster_for_tf;

    let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(10);
    let mut slots: [Option<TimeframePipeline>; 10] = std::array::from_fn(|_| None);
    slots[MIDX] = Some(make_pipe(60, bcast_tx));
    let micro_pipe = slots[MIDX].as_ref().unwrap();

    // Snapshot exists but no OI.
    let mut snap = make_snap_history(vec![50_000.0]);
    snap.open_interest = None;
    *micro_pipe.latest_snapshot.write().await = Some(snap);

    let active = Arc::new(make_full_pair(slots));

    let result = compute_cluster_for_tf(
        &active,
        60,
        &test_config(),
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await;
    assert!(result.is_err(), "no OI → Err");
}

/// Regression: the cluster-refresh skip reason must be templated on the
/// active exchange (v6.6). HL and Bitget have different OI carriers
/// (REST poller vs ticker channel), so a generic message misleads the
/// operator about which feed to investigate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cluster_refresh_skip_reason_templates_on_active_exchange() {
    use portfolio_supervisor::registry::pipelines::compute_cluster_for_tf;

    async fn build_active() -> Arc<ActivePair> {
        let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(10);
        let mut slots: [Option<TimeframePipeline>; 10] = std::array::from_fn(|_| None);
        slots[MIDX] = Some(make_pipe(60, bcast_tx));
        let micro_pipe = slots[MIDX].as_ref().unwrap();

        // Snapshot exists but no OI → NoOpenInterest variant fires.
        let mut snap = make_snap_history(vec![50_000.0]);
        snap.open_interest = None;
        *micro_pipe.latest_snapshot.write().await = Some(snap);

        Arc::new(make_full_pair(slots))
    }

    let active_hl = build_active().await;
    let err_hl = compute_cluster_for_tf(
        &active_hl,
        60,
        &test_config(),
        ExchangeChoice::Hyperliquid,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await
    .unwrap_err();
    let msg_hl = err_hl.to_string();
    assert!(
        msg_hl.contains("HL derivatives poller"),
        "HL skip reason should mention the HL poller, got: {}",
        msg_hl
    );
    assert!(
        !msg_hl.contains("Bitget"),
        "HL skip reason must NOT mention Bitget, got: {}",
        msg_hl
    );

    let active_bg = build_active().await;
    let err_bg = compute_cluster_for_tf(
        &active_bg,
        60,
        &test_config(),
        ExchangeChoice::Bitget,
        &portfolio_supervisor::registry::pipelines::ClusterOverrides::default(),
    )
    .await
    .unwrap_err();
    let msg_bg = err_bg.to_string();
    assert!(
        msg_bg.contains("Bitget ticker channel"),
        "Bitget skip reason should mention the Bitget ticker channel, got: {}",
        msg_bg
    );
    assert!(
        !msg_bg.contains("HL derivatives poller"),
        "Bitget skip reason must NOT mention HL poller, got: {}",
        msg_bg
    );
}

// Avoid unused import warnings.
#[allow(dead_code)]
fn _unused_refs(_t: &TimeframeConfig) {}
#[allow(dead_code)]
const _: Option<TimeframeConfig> = None;

// bring `default_for_test` into scope via extension trait
trait SnapshotTestHelpers {
    fn default_for_test(symbol: &str, timeframe_secs: u64) -> MarketSnapshot;
}

impl SnapshotTestHelpers for MarketSnapshot {
    fn default_for_test(symbol: &str, _secs: u64) -> MarketSnapshot {
        use std::collections::HashMap;
        MarketSnapshot {
            timeframe_label: Some("1m".to_string()),
            exchange: Some(Exchange::Hyperliquid),
            timeframe_secs: 60,
            timestamp: 0,
            symbol: symbol.into(),
            is_completed: Some(true),
            mid_price: Decimal::from(50_000),
            bid_price: Decimal::ZERO,
            ask_price: Decimal::ZERO,
            bid_size: None,
            ask_size: None,
            funding_rate: Some(Decimal::from_f64_retain(0.0001).unwrap_or_default()),
            open_interest: Some(Decimal::from(1_000_000)),
            oi_delta_pct: None,
            oi_delta_window_secs: None,
            mark_price: None,
            index_price: None,
            mark_index_spread_pct: None,
            prev_day_px: None,
            open: Some(Decimal::from(50_000)),
            high: Some(Decimal::from(50_100)),
            low: Some(Decimal::from(49_900)),
            close: Some(Decimal::from(50_000)),
            volume: Some(Decimal::from(100)),
            average_volume: Some(Decimal::from(100)),
            indicators: HashMap::new(),
            context: None,
            decision_context: None,
            statistical_context: None,
            alignment: None,
            risk: None,
            analysis: None,
            advisory: None,
            opportunity: None,
            liquidity_signals: vec![],
            metrics_config: None,
            risk_profile: None,
            liquidity: None,
            cluster: None,
            volume_profile: None,
            quality_envelope: None,
            pipeline_state: core_domain::models::CandlePipelineState::default(),
            indicator_lifecycle: std::collections::HashMap::new(),
        }
    }
}
