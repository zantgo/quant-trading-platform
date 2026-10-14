//! AUDIT-M13 regression: `/api/history` must stay inside its memory bound.
//!
//! The gap-fill loop applied its cap PER GAP and trimmed the response only
//! AFTER filling, so `limit=1000` snapshots separated by large gaps built up to
//! ~1,000,000 `HistoryCandle`s (six `String`s each) in the transient vectors
//! before a single element was dropped — roughly 150-200 MB of heap per
//! request. The 4-permit semaphore at the top of the handler means four of
//! those at once; on the documented 2.8 GB host that is an OOM path.
//!
//! These are LOAD-style tests, not unit tests: they drive the real handler
//! through the real router with adversarial sparse history and measure the
//! actual peak RSS. RSS comes from `/proc/self/status` (Linux), matching the
//! daemon's existing `/proc/meminfo` reliance.

use std::collections::VecDeque;
use std::sync::Arc;

use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use config_models::WorkspaceConfig;
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, SymbolMapper};
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR_KEY: &str = "BTC-USDT";
const INSTANCE_ID: &str = "inst_sub_minute";

/// Build a router holding one workspace instance whose `micro.snapshot_history`
/// is seeded with completed `MarketSnapshot`s at `timeframe_secs=secs`.
/// This mimics what `populate_buffers` does after a historical bootstrap.
async fn build_router_with_snapshots(
    _secs: u64,
    snapshots: Vec<MarketSnapshot>,
) -> (axum::Router, Arc<AppState>) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

    let symbol_mapper = Arc::new(SymbolMapper::new());
    symbol_mapper
        .register(Exchange::Hyperliquid, "BTC", PAIR_KEY)
        .await;
    let (telemetry_tx, _) = mpsc::channel::<database_storage::TelemetryMsg>(100);

    let workspace = WorkspaceState::empty();
    let (bcast_tx, _) = broadcast::channel::<MarketSnapshot>(200);
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));

    // Pre-populate the snapshot history with the supplied entries.
    {
        let mut sh = snap_hist.write().await;
        for snap in &snapshots {
            sh.push_back(snap.clone());
        }
    }

    let build_pipe = |secs, tx| TimeframePipeline {
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
    };

    let active_pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        // v11.9 supported-pool durations: every pipeline shares the seeded
        // `snap_hist`, so whichever duration the requested sub-minute TF
        // resolves to sees the bootstrap snapshots.
        pipelines: vec![
            build_pipe(1, bcast_tx.clone()),
            build_pipe(3, bcast_tx.clone()),
            build_pipe(5, bcast_tx.clone()),
            build_pipe(15, bcast_tx.clone()),
            build_pipe(30, bcast_tx.clone()),
            build_pipe(60, bcast_tx.clone()),
            build_pipe(180, bcast_tx.clone()),
            build_pipe(300, bcast_tx.clone()),
            build_pipe(900, bcast_tx.clone()),
            build_pipe(3600, bcast_tx),
        ],
        active_secs: config_models::SUPPORTED_DURATIONS.to_vec(),
        snapshot_tx: mpsc::channel::<core_domain::normalized::NormalizedEvent>(50).0,
        cancel: CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(Default::default()),
    });

    let buffers: Vec<TimeframeBuffers> = active_pair
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
        ("BTC".into(), "USDT".into()),
        ExchangeChoice::Hyperliquid,
        // The resolved venue wire name — the bare base on the default
        // crypto perp dex, which is what these fixtures model.
        "BTC".to_string(),
        active_pair.clone(),
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        config_models::SUPPORTED_DURATIONS.to_vec(), // v11.9 active ladder
        Default::default(),
    ));
    workspace.insert(PAIR_KEY.to_string(), instance).await;

    {
        let cfg: WorkspaceConfig = WorkspaceConfig::default();
        workspace.set_config(cfg).await;
    }

    let state = Arc::new(AppState {
        workspace,
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(Default::default())),
        pool,
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(Default::default()),
        ws_url: "ws://127.0.0.1:1".into(),
        bitget_ws_url: String::new(),
        bybit_ws_url: String::new(),
        clock_monitor: None,
        reliability: Arc::new(ReliabilityTracker::new()),
        exchange_status: Arc::new(ExchangeStatusTracker::new()),
        latency_tracker: Arc::new(Default::default()),
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
    });
    (api_gateway::build_router(state.clone()), state)
}

fn make_snapshot(secs: u64, timestamp: u64, close_val: f64) -> MarketSnapshot {
    let close = rust_decimal::Decimal::from_f64_retain(close_val).unwrap_or_default();
    let open = rust_decimal::Decimal::from_f64_retain(close_val - 5.0).unwrap_or_default();
    let high = rust_decimal::Decimal::from_f64_retain(close_val + 5.0).unwrap_or_default();
    let low = rust_decimal::Decimal::from_f64_retain(close_val - 10.0).unwrap_or_default();
    let bid = rust_decimal::Decimal::from_f64_retain(close_val - 1.0).unwrap_or_default();
    let ask = rust_decimal::Decimal::from_f64_retain(close_val + 1.0).unwrap_or_default();
    MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: secs,
        timestamp,
        symbol: PAIR_KEY.to_string(),
        is_completed: Some(true),
        mid_price: close,
        bid_price: bid,
        ask_price: ask,
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open_interest: None,
        oi_delta_pct: None,
        oi_delta_window_secs: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        open: Some(open),
        high: Some(high),
        low: Some(low),
        close: Some(close),
        volume: Some(rust_decimal::Decimal::from_f64_retain(1.5).unwrap_or_default()),
        average_volume: Some(rust_decimal::Decimal::from_f64_retain(1.2).unwrap_or_default()),
        context: None,
        decision_context: None,
        statistical_context: None,
        indicators: Default::default(),
        alignment: None,
        risk: None,
        analysis: None,
        advisory: None,
        opportunity: None,
        risk_profile: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        liquidity_signals: vec![],
        metrics_config: None,
        quality_envelope: None,
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

/// Peak resident set size of this process, in MiB, from `/proc/self/status`.
fn peak_rss_mib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0);
            return kb / 1024;
        }
    }
    0
}

/// Current resident set size, in MiB (for a before/after delta).
fn current_rss_mib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0);
            return kb / 1024;
        }
    }
    0
}

/// `count` snapshots at `secs`, each separated by `gap_secs` (>= `secs`), which
/// is the adversarial shape: every consecutive pair looks like a huge hole.
fn sparse_snapshots(count: usize, secs: u64, gap_secs: u64) -> Vec<MarketSnapshot> {
    let mut out = Vec::with_capacity(count);
    let mut t = 1_700_000_000u64;
    for i in 0..count {
        out.push(make_snapshot(secs, t, 100_000.0 + i as f64));
        // Two-hour hole between every pair at any duration <= 3600.
        t += gap_secs.max(secs);
    }
    out
}

async fn history_bytes(router: &axum::Router, query: &str) -> (usize, usize) {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/history?symbol={PAIR_KEY}&{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("history request");
    assert_eq!(res.status(), 200, "history must succeed");
    let body = axum::body::to_bytes(res.into_body(), 512 * 1024 * 1024)
        .await
        .expect("body");
    let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let n = v["candles"].as_array().map(|a| a.len()).unwrap_or(0);
    (n, body.len())
}

// ── tests ──────────────────────────────────────────────────────────────────

/// THE load test: 1000 snapshots each two hours apart, requested at
/// `limit=1000`.
///
/// Before the fix the fill loop synthesised up to 1,000 bars PER GAP across
/// 999 gaps — ~1M `HistoryCandle`s — before trimming. This asserts the
/// response is bounded and the peak allocation stays small.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sparse_limit_1000_stays_bounded() {
    // 2 h gap at a 1 m TF = 119 synthetic bars per gap x 999 gaps.
    let snaps = sparse_snapshots(1000, 60, 7_200);
    let (router, _state) = build_router_with_snapshots(60, snaps).await;

    // Settle any one-off allocations from seeding the fixtures.
    let base = current_rss_mib();

    let (n, bytes) = history_bytes(&router, "timeframe_secs=60&limit=1000").await;

    let peak = peak_rss_mib();
    let delta = peak.saturating_sub(base);

    assert!(
        n <= 1000,
        "the response must respect `limit`, got {n} candles"
    );
    assert!(
        bytes < 32 * 1024 * 1024,
        "the response body must stay modest, got {} MiB",
        bytes / (1024 * 1024)
    );
    assert!(
        delta < 150,
        "peak RSS must not balloon: grew {delta} MiB above the {base} MiB baseline \
         (the pre-fix path materialised ~1M HistoryCandles)"
    );
}

/// The semaphore allows 4 concurrent requests; the bound must hold at 4x, not
/// just 1x. Otherwise the fix only helps a single-user single-request case.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_concurrent_sparse_requests_stay_bounded() {
    let snaps = sparse_snapshots(1000, 60, 7_200);
    let (router, _state) = build_router_with_snapshots(60, snaps).await;
    let base = current_rss_mib();

    let r = router.clone();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let r = r.clone();
            tokio::spawn(async move { history_bytes(&r, "timeframe_secs=60&limit=1000").await })
        })
        .collect();
    let results: Vec<_> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("join"))
        .collect();

    for (i, (n, _)) in results.iter().enumerate() {
        assert!(*n <= 1000, "request {i} returned {n} candles");
    }

    let delta = peak_rss_mib().saturating_sub(base);
    eprintln!("[M13] 4-concurrent peak delta: {delta} MiB");
    assert!(
        delta < 200,
        "4 concurrent sparse requests must stay bounded, grew {delta} MiB"
    );
}

/// A gap-free window must be returned UNCHANGED — the cap must not truncate
/// legitimate history.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn warm_history_is_not_truncated() {
    // Contiguous 1 m candles: no gaps at all, so no synthetic fills.
    let mut snaps = Vec::new();
    let mut t = 1_700_000_000u64;
    for i in 0..500usize {
        snaps.push(make_snapshot(60, t, 100_000.0 + i as f64));
        t += 60;
    }
    let (router, _state) = build_router_with_snapshots(60, snaps).await;

    let (n, _) = history_bytes(&router, "timeframe_secs=60&limit=1000").await;
    assert_eq!(
        n, 500,
        "a contiguous 500-bar window must come back complete and unfilled"
    );
}

/// A short window with one small gap must still be gap-filled — the fix must
/// not disable the feature.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_small_gap_is_still_filled() {
    let mut snaps = vec![
        make_snapshot(60, 1_700_000_000, 100_000.0),
        // 10 minutes later = 9 missing 1 m bars.
        make_snapshot(60, 1_700_000_600, 100_100.0),
    ];
    snaps.push(make_snapshot(60, 1_700_000_660, 100_200.0));
    let (router, _state) = build_router_with_snapshots(60, snaps).await;

    let (n, _) = history_bytes(&router, "timeframe_secs=60&limit=1000").await;
    assert_eq!(
        n, 12,
        "3 real candles + 9 synthetic Dojis for the 10-minute gap, got {n}"
    );
}
