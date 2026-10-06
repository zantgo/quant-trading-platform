//! v11.12.24 (memory): `/api/history` output parity under the retained-history
//! projection.
//!
//! The retained `snapshot_history` window now stores
//! `MarketSnapshot::history_projection()` — the subset `serve_history`
//! actually reads (`timestamp`, OHLCV, `quality_envelope.is_gap_filled`,
//! `indicators.{raw_value, normalized, state_label, values}`) — because the
//! daemon was OOM-killed at ~1.8–2.1 GB RSS retaining 1 000 FULL frames per
//! (instance × duration).
//!
//! Dropping 54.9 KB of matrix payload per retained frame is only safe if the
//! HTTP response is **byte-identical** to what the unprojected frames
//! produced. That is exactly what this file pins: the same fixture seeded
//! twice — once raw, once projected — must yield the same `prices`, the same
//! `candles`, and the same `indicator_history` (times, raw / normalized /
//! state_label arrays and every multi-line `values.*` sub-series).
//!
//! A mismatch here means the projection dropped something a chart renders.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use config_models::WorkspaceConfig;
use core_domain::indicator_dtos::{
    IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus,
};
use core_domain::models::{
    CandlePipelineState, CandleQualityEnvelope, MarketSnapshot, SequenceIntegrity,
};
use core_domain::normalized::{Exchange, SymbolMapper};
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::workspace_state::WorkspaceState;
use rust_decimal::Decimal;
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR_KEY: &str = "BTC-USDT";
const INSTANCE_ID: &str = "inst_projection_parity";
const TF_SECS: u64 = 60;
const BARS: u64 = 40;

fn dec(v: f64) -> Decimal {
    Decimal::from_f64_retain(v).unwrap_or_default()
}

/// A realistic COMPLETED frame: a multi-line indicator map (the shape
/// `/api/history` flattens into `values.<key>.<sub>` series), a synthetic
/// gap-fill envelope, and every heavyweight field the projection drops.
fn make_snapshot(timestamp: u64) -> MarketSnapshot {
    let base = 60_000.0 + timestamp as f64;
    let mut snap = MarketSnapshot {
        timeframe_label: Some("1m".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: TF_SECS,
        timestamp,
        symbol: PAIR_KEY.to_string(),
        is_completed: Some(true),
        mid_price: dec(base),
        bid_price: dec(base - 1.0),
        ask_price: dec(base + 1.0),
        bid_size: Some(dec(1.5)),
        ask_size: Some(dec(2.0)),
        funding_rate: Some(dec(0.0001)),
        open: Some(dec(base - 20.0)),
        high: Some(dec(base + 12.0)),
        low: Some(dec(base - 24.0)),
        close: Some(dec(base)),
        volume: Some(dec(3.25)),
        average_volume: Some(dec(2.75)),
        context: Some(core_domain::market_context::MarketContext {
            trend: core_domain::market_context::ContextDimension {
                score: 60.0,
                confidence: 0.7,
                label: "BULLISH".into(),
            },
            momentum: core_domain::market_context::ContextDimension {
                score: 55.0,
                confidence: 0.6,
                label: "BULLISH".into(),
            },
            volatility: core_domain::market_context::ContextDimension {
                score: 40.0,
                confidence: 0.5,
                label: "MODERATE".into(),
            },
            volume: core_domain::market_context::ContextDimension {
                score: 52.0,
                confidence: 0.65,
                label: "NEUTRAL".into(),
            },
            liquidity: core_domain::market_context::ContextDimension {
                score: 48.0,
                confidence: 0.6,
                label: "NEUTRAL".into(),
            },
            regime: "TRENDING".into(),
            overall_score: 52,
            overall_label: "BULLISH".into(),
        }),
        decision_context: None,
        statistical_context: Some(core_domain::models::StatisticalContext {
            close_z: Some(0.4),
            rsi_z: Some(-0.2),
            macd_z: Some(0.9),
            monte_carlo_expected: Some(0.01),
            monte_carlo_stdev: Some(0.03),
        }),
        alignment: Some(core_domain::alignment::AlignmentMatrix::empty(PAIR_KEY)),
        risk: Some(core_domain::risk::RiskMatrix::empty(PAIR_KEY)),
        analysis: Some(core_domain::analysis::AnalysisMatrix::empty(PAIR_KEY)),
        advisory: Some(core_domain::advisory::AdvisoryMatrix::empty(PAIR_KEY)),
        opportunity: Some(core_domain::opportunity::OpportunityMatrix {
            symbol: PAIR_KEY.into(),
            opportunity_score: 60.0,
            ..Default::default()
        }),
        risk_profile: Some(1),
        liquidity: None,
        cluster: None,
        volume_profile: None,
        liquidity_signals: vec![],
        metrics_config: None,
        quality_envelope: Some(CandleQualityEnvelope {
            quality_score: 95.0,
            is_valid: true,
            // Alternate real / synthetic so the frontend's
            // `candleReconstructed` filter is exercised on both paths.
            is_gap_filled: timestamp % 7 == 0,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: SequenceIntegrity::Valid,
            gap_since_last: 0,
            validated_at: timestamp * 1000,
        }),
        pipeline_state: CandlePipelineState::Live,
        indicator_lifecycle: std::collections::HashMap::new(),
        ..Default::default()
    };

    // Single-line indicator with a state label and NO WARMING placeholder —
    // `push_value` filters WARMING entries out of the history arrays.
    snap.indicators.insert(
        "rsi".to_string(),
        NormalizedIndicatorValue {
            raw_value: 62.4,
            normalized: 0.24,
            state_label: "BULLISH".into(),
            values: None,
            signals: vec![IndicatorSignal::new(
                SignalKind::Threshold,
                SignalDirection::Bullish,
                SignalStatus::Active,
                "crossed 60",
            )],
            confidence: 0.9,
        },
    );
    // Multi-line indicator: `values` become separate chart series, so the
    // projection MUST preserve every sub-key.
    snap.indicators.insert(
        "bollinger".to_string(),
        NormalizedIndicatorValue {
            raw_value: 60_010.0,
            normalized: 0.1,
            state_label: "UPPER_BAND".into(),
            values: Some(
                [
                    ("upper", 60_050.0),
                    ("middle", 60_000.0),
                    ("lower", 59_950.0),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            ),
            signals: vec![],
            confidence: 0.5,
        },
    );
    // A WARMING placeholder — must be skipped (pushed as null) on BOTH paths.
    snap.indicators.insert(
        "hull_ma".to_string(),
        NormalizedIndicatorValue::scalar(0.0, 0.0, "WARMING"),
    );
    snap
}

fn fixture() -> Vec<MarketSnapshot> {
    let start = 1_718_000_000u64;
    (0..BARS)
        .map(|i| make_snapshot(start + i * TF_SECS))
        .collect()
}

async fn build_router(seeded: Vec<MarketSnapshot>) -> Arc<AppState> {
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
    {
        let mut sh = snap_hist.write().await;
        for snap in &seeded {
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
        pipeline_state: Arc::new(RwLock::new(CandlePipelineState::Initializing)),
        indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
        advisory: Arc::new(RwLock::new(None)),
        tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
        buffer_size: 500,
        stale_threshold_secs: 300,
    };

    let active_pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        pipelines: vec![build_pipe(TF_SECS, bcast_tx)],
        active_secs: vec![TF_SECS],
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
        vec![TF_SECS],
        Default::default(),
    ));
    workspace.insert(PAIR_KEY.to_string(), instance).await;
    workspace.set_config(WorkspaceConfig::default()).await;

    Arc::new(AppState {
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
    })
}

async fn fetch_history(state: Arc<AppState>, limit: usize) -> serde_json::Value {
    let router = api_gateway::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let res = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/history?symbol=BTC-USDT&timeframe_secs={TF_SECS}&limit={limit}"
        ))
        .send()
        .await
        .expect("history request");
    assert!(res.status().is_success(), "history must answer 200");
    res.json().await.expect("json body")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projected_history_is_byte_identical_to_unprojected() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let raw = fixture();
        let projected: Vec<MarketSnapshot> = raw.iter().map(|s| s.history_projection()).collect();

        // Sanity: the fixture really does exercise the stripped fields.
        assert!(raw[0].alignment.is_some() && projected[0].alignment.is_none());
        assert!(raw[0].advisory.is_some() && projected[0].advisory.is_none());
        assert!(!raw[0].indicators["rsi"].signals.is_empty());
        assert!(projected[0].indicators["rsi"].signals.is_empty());

        let raw_body = fetch_history(build_router(raw).await, 100).await;
        let proj_body = fetch_history(build_router(projected).await, 100).await;

        for field in ["prices", "candles", "indicator_history"] {
            let a = serde_json::to_string(&raw_body[field]).unwrap();
            let b = serde_json::to_string(&proj_body[field]).unwrap();
            assert_eq!(
                a, b,
                "`{field}` must be identical whether the retained window holds \
                 full or projected snapshots — the projection may not change \
                 anything a chart renders"
            );
        }
    })
    .await
    .expect("projection parity test timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projected_history_still_carries_every_indicator_series() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let projected: Vec<MarketSnapshot> =
            fixture().iter().map(|s| s.history_projection()).collect();
        let body = fetch_history(build_router(projected).await, 100).await;

        let hist = &body["indicator_history"];
        let times = hist["times"].as_array().expect("times array");
        assert_eq!(times.len(), BARS as usize, "every retained bar is served");

        let indicators = hist["indicators"].as_object().expect("indicators map");
        // Multi-line `values` sub-series must survive the projection — they
        // are the Bollinger bands the chart draws.
        let bands = indicators.get("bollinger").expect("bollinger present");
        for sub in ["upper", "middle", "lower"] {
            let series = bands["values"][sub]
                .as_array()
                .unwrap_or_else(|| panic!("bollinger.values.{sub} must be a series"));
            assert_eq!(
                series.len(),
                BARS as usize,
                "bollinger {sub} must have one point per bar, aligned to `times`"
            );
            assert!(
                series.iter().all(|v| v.is_number()),
                "bollinger {sub} must carry real numbers, not placeholders"
            );
        }
        // `raw` / `normalized` / `state_label` axes.
        for axis in ["raw", "normalized", "state_label"] {
            let arr = bands[axis].as_array().expect("axis array");
            assert_eq!(arr.len(), BARS as usize, "{axis} must align to `times`");
        }
        // The WARMING placeholder is filtered out on both paths.
        let rsi = indicators.get("rsi").expect("rsi present");
        assert_eq!(rsi["raw"].as_array().unwrap().len(), BARS as usize);
        assert!(
            rsi["raw"].as_array().unwrap().iter().all(|v| v.is_number()),
            "rsi raw values are non-WARMING, so every slot must be a number"
        );
    })
    .await
    .expect("indicator-series test timed out");
}

/// The `limit` must bound the work done, not just the response — v11.12.24
/// switched the handler from a whole-window clone to a tail clone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn small_limit_returns_the_newest_bars_only() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let projected: Vec<MarketSnapshot> =
            fixture().iter().map(|s| s.history_projection()).collect();
        let body = fetch_history(build_router(projected).await, 5).await;

        let candles = body["candles"].as_array().expect("candles");
        assert_eq!(candles.len(), 5, "limit=5 must return 5 bars");
        let times = body["indicator_history"]["times"].as_array().unwrap();
        assert_eq!(times.len(), 5, "indicator arrays must match the bar count");

        // The newest five, not the oldest five.
        let newest = body["prices"].as_array().unwrap();
        let last_close = newest[4].as_str().expect("price string");
        assert_eq!(
            last_close,
            fixture().last().unwrap().close.unwrap().to_string(),
            "the tail read must return the NEWEST bars"
        );
    })
    .await
    .expect("limit test timed out");
}
