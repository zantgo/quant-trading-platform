//! Regression: sub-minute `/api/history` returns the bootstrap data.
//!
//! Before the frontend `isSubMinute` guard was removed, the chart never
//! called `setData()` for ≤60s timeframes. Now that the guard is gone
//! the backend must faithfully return whatever snapshot data the warm
//! bootstrap deposited in the per-pipeline queue — even for `timeframe_secs=1`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

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
use tokio::net::TcpListener;
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
        oi_delta_1h: None,
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

async fn serve_for(state: Arc<AppState>) -> std::net::SocketAddr {
    let router = api_gateway::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_returns_candles_for_sub_minute_timeframe() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (_router, state) = build_router_with_snapshots(
            1,
            vec![
                make_snapshot(1, 1_718_000_001, 65000.0),
                make_snapshot(1, 1_718_000_002, 65001.0),
            ],
        )
        .await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol=BTC-USDT&timeframe_secs=1&limit=100"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array");
        assert!(
            !candles.is_empty(),
            "expected >= 1 candles for sub-minute history, got {candles:?}"
        );

        // Every returned candle must be a JSON object with `time`, `open`,
        // `high`, `low`, `close`.
        for candle in candles {
            assert!(candle.get("time").is_some());
            assert!(candle.get("close").is_some());
        }
    })
    .await
    .expect("sub-minute history test timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_trims_leading_none_close_for_sub_minute() {
    tokio::time::timeout(Duration::from_secs(10), async {
        // Seed three snapshots: the first has no close → must be trimmed
        // by the handler's leading-None filter (history.rs:42-43). The
        // response should contain only the two real ones.
        let (_router, state) = build_router_with_snapshots(
            1,
            vec![
                {
                    let mut s = make_snapshot(1, 1_718_000_001, 65000.0);
                    s.close = None; // leading None
                    s
                },
                make_snapshot(1, 1_718_000_002, 65001.0),
                make_snapshot(1, 1_718_000_003, 65002.0),
            ],
        )
        .await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol=BTC-USDT&timeframe_secs=1&limit=100"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array");
        assert_eq!(
            candles.len(),
            2,
            "leading close-None must be trimmed, got {candles:?}"
        );
    })
    .await
    .expect("sub-minute leading-trim test timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_returns_empty_on_unknown_pair() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (_router, state) = build_router_with_snapshots(1, vec![]).await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol=NONEXISTENT&timeframe_secs=1"
            ))
            .send()
            .await
            .expect("history request");
        assert!(!res.status().is_server_error());
    })
    .await
    .expect("unknown-pair test timed out");
}

/// Regression: when a sub-minute TF has no in-memory snapshot_history AND no
/// DB rows, the endpoint must return an empty candle array — it must NOT
/// synthesise flat-close candles from the next-larger TF (this was the
/// "line of about 1 minute" rendering bug). The chart's live WS stream
/// fills in real candles within seconds anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_returns_empty_candles_on_cold_sub_minute_start() {
    tokio::time::timeout(Duration::from_secs(10), async {
        // Empty in-memory snapshot_history, empty DB (no rows inserted).
        let (_router, state) = build_router_with_snapshots(1, vec![]).await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol={PAIR_KEY}&timeframe_secs=1&limit=200"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array");
        let prices = body
            .get("prices")
            .and_then(|v| v.as_array())
            .expect("prices array");

        // The historical payload must be empty — no synthetic flat-line
        // candles synthesised from any other TF.
        assert_eq!(
            candles.len(),
            0,
            "expected zero candles on cold sub-minute start, got {candles:?}"
        );
        assert_eq!(
            prices.len(),
            0,
            "expected zero prices on cold sub-minute start, got {prices:?}"
        );
    })
    .await
    .expect("cold sub-minute start test timed out");
}

/// Regression for a 3s sub-minute TF: same as the 1s case but exercises a
/// different factor. Without the fix, the server would synthesise 20
/// identical OHLC candles per minute (3s × 20 = 60s) from the 60s source,
/// producing exactly the "line of about 1 minute" render artefact.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_returns_empty_candles_on_cold_3s_start() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (_router, state) = build_router_with_snapshots(3, vec![]).await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol={PAIR_KEY}&timeframe_secs=3&limit=200"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array");

        assert_eq!(
            candles.len(),
            0,
            "expected zero candles on cold 3s start (no synthetic 60s-derived flat lines), got {candles:?}"
        );
    })
    .await
    .expect("cold 3s start test timed out");
}

/// Regression: gap-fill Doji candles inserted between real snapshots to
/// keep the chart time-axis continuous must be tagged with `reconstructed`
/// so the frontend can filter them out of the persistent candle cache —
/// they're not real OHLCV data, just visual scaffolding. Real (in-memory)
/// snapshots must NOT carry the flag.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_marks_gap_fill_dojis_as_synthetic() {
    tokio::time::timeout(Duration::from_secs(10), async {
        // Two snapshots 60 seconds apart on a 5s TF — the gap-fill logic
        // must insert 12 Doji entries (60 / 5 - 1 = 11) into the response.
        let (_router, state) = build_router_with_snapshots(
            5,
            vec![
                make_snapshot(5, 1_718_000_000, 65000.0),
                make_snapshot(5, 1_718_000_060, 65010.0),
            ],
        )
        .await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol={PAIR_KEY}&timeframe_secs=5&limit=100"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array")
            .clone();

        // 2 real snapshots + 11 gap-fill Doji entries = 13 candles.
        assert_eq!(
            candles.len(),
            13,
            "expected 2 real + 11 gap-fill candles, got {candles:?}"
        );

        // First and last candles are the real snapshots — no reconstructed flag.
        let first = &candles[0];
        let last = &candles[candles.len() - 1];
        assert!(
            first.get("reconstructed").is_none(),
            "first real candle must NOT have reconstructed flag, got {first:?}"
        );
        assert!(
            last.get("reconstructed").is_none(),
            "last real candle must NOT have reconstructed flag, got {last:?}"
        );

        // Every middle entry is a gap-fill Doji — must be SYNTHETIC.
        let mut synthetic_count = 0;
        for candle in &candles[1..candles.len() - 1] {
            let r = candle
                .get("reconstructed")
                .and_then(|v| v.as_str())
                .expect("middle gap-fill candles must have reconstructed flag");
            assert_eq!(
                r, "SYNTHETIC",
                "gap-fill Doji must be tagged SYNTHETIC, got {r}"
            );
            synthetic_count += 1;
        }
        assert_eq!(synthetic_count, 11, "expected 11 gap-fill Doji entries");
    })
    .await
    .expect("gap-fill synthetic tagging test timed out");
}

/// AUDIT-V8-004 (history continuity): snapshots whose
/// `quality_envelope.is_gap_filled == true` (idle-bucket heartbeat dojis
/// pushed into the in-memory snapshot history) must surface on the wire as
/// `reconstructed: "SYNTHETIC"` so the frontend's `candleReconstructed`
/// filter keeps them out of its persistent candle cache — while real
/// snapshots keep `reconstructed: null`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_endpoint_marks_heartbeat_dojis_as_reconstructed() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut real = make_snapshot(1, 1_718_000_001, 65000.0);
        real.quality_envelope = Some(core_domain::models::CandleQualityEnvelope {
            quality_score: 100.0,
            is_valid: true,
            is_gap_filled: false,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: core_domain::models::SequenceIntegrity::Valid,
            gap_since_last: 1,
            validated_at: 0,
        });
        let mut doji = make_snapshot(1, 1_718_000_002, 65000.0);
        doji.volume = Some(rust_decimal::Decimal::ZERO);
        doji.open = doji.close;
        doji.high = doji.close;
        doji.low = doji.close;
        doji.quality_envelope = Some(core_domain::models::CandleQualityEnvelope {
            quality_score: 80.0,
            is_valid: true,
            is_gap_filled: true,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: core_domain::models::SequenceIntegrity::Valid,
            gap_since_last: 1,
            validated_at: 0,
        });

        let (_router, state) = build_router_with_snapshots(1, vec![real, doji]).await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol=BTC-USDT&timeframe_secs=1&limit=100"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let candles = body
            .get("candles")
            .and_then(|v| v.as_array())
            .expect("candles array");

        let real_candle = candles
            .iter()
            .find(|c| c.get("time").and_then(|t| t.as_u64()) == Some(1_718_000_001_000))
            .expect("real candle present");
        assert!(
            real_candle.get("reconstructed").is_none()
                || real_candle
                    .get("reconstructed")
                    .and_then(|v| v.as_str())
                    .is_none(),
            "real snapshot must NOT carry a reconstructed flag: {:?}",
            real_candle.get("reconstructed")
        );

        let doji_candle = candles
            .iter()
            .find(|c| c.get("time").and_then(|t| t.as_u64()) == Some(1_718_000_002_000))
            .expect("doji candle present");
        assert_eq!(
            doji_candle.get("reconstructed").and_then(|v| v.as_str()),
            Some("SYNTHETIC"),
            "gap-filled heartbeat snapshot must carry reconstructed=\"SYNTHETIC\": {:?}",
            doji_candle
        );
    })
    .await
    .expect("history provenance test timed out");
}

/// AUDIT-V8-006 (axis alignment): the `indicator_history` arrays must be
/// aligned to the (possibly gap-filled) `times` axis. When the snapshot
/// history has a hole, the API inserts a scaffold Doji into `times` and
/// must push `null` into every indicator series at that index — otherwise
/// `times[i]` pairs with the wrong `values[i]` and every EMA point after
/// the gap is plotted at a shifted timestamp (the "lines don't correspond
/// with the price" symptom).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_aligns_indicator_arrays_to_gap_filled_axis() {
    tokio::time::timeout(Duration::from_secs(10), async {
        use core_domain::indicator_dtos::NormalizedIndicatorValue;

        let mk = |ts: u64, ema: f64| {
            let mut s = make_snapshot(1, ts, ema);
            let mut vals = std::collections::HashMap::new();
            vals.insert("fast".to_string(), ema);
            let mut entry =
                NormalizedIndicatorValue::scalar(ema, 0.0, "CONSOLIDATED_TANGLED_STACK");
            entry.values = Some(vals);
            let mut indicators = std::collections::HashMap::new();
            indicators.insert("ema_stack".to_string(), entry);
            s.indicators = indicators;
            s
        };

        // Snapshot history with a HOLE at ts=1_718_000_002.
        let (_router, state) = build_router_with_snapshots(
            1,
            vec![mk(1_718_000_001, 65000.0), mk(1_718_000_003, 65002.0)],
        )
        .await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "http://{addr}/api/history?symbol=BTC-USDT&timeframe_secs=1&limit=100"
            ))
            .send()
            .await
            .expect("history request");
        assert!(res.status().is_success());

        let body: serde_json::Value = res.json().await.expect("json");
        let times = body
            .get("indicator_history")
            .and_then(|v| v.get("times"))
            .and_then(|v| v.as_array())
            .expect("indicator_history.times");
        let fast = body
            .get("indicator_history")
            .and_then(|v| v.get("indicators"))
            .and_then(|v| v.get("ema_stack"))
            .and_then(|v| v.get("values"))
            .and_then(|v| v.get("fast"))
            .and_then(|v| v.as_array())
            .expect("ema_stack.values.fast");

        assert_eq!(
            times.len(),
            3,
            "gap-filled axis must include the scaffold Doji: {times:?}"
        );
        assert_eq!(
            fast.len(),
            times.len(),
            "indicator arrays must be aligned to the gap-filled axis (got {fast:?} for {times:?})"
        );
        assert!(
            fast[0].is_number(),
            "first real snapshot value present at index 0: {fast:?}"
        );
        assert!(
            fast[1].is_null(),
            "scaffold Doji must carry null indicators at index 1: {fast:?}"
        );
        assert_eq!(
            fast[2].as_f64(),
            Some(65002.0),
            "second real snapshot value at index 2: {fast:?}"
        );
    })
    .await
    .expect("history axis alignment test timed out");
}
