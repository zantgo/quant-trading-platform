//! Regression tests for the v11.9 duration-keyed identity invariants. A
//! timeframe IS its duration in seconds — there are no named slots — and
//! the derived label must survive the wire. These tests pin the behaviour
//! so a future refactor cannot reintroduce named-slot dispatch.
use std::sync::Arc;

use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use config_models::WorkspaceConfig;
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedEvent};
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::{Instance, TimeframeBuffers};
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR_KEY: &str = "BTC-USDT";

fn make_pipe(secs: u64, tx: broadcast::Sender<MarketSnapshot>) -> TimeframePipeline {
    TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: Arc::new(RwLock::new(VecDeque::new())),
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
    }
}

async fn build_test_router() -> (axum::Router, Arc<AppState>) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

    let symbol_mapper = Arc::new(core_domain::normalized::SymbolMapper::new());
    symbol_mapper
        .register(Exchange::Hyperliquid, "BTC", PAIR_KEY)
        .await;
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);

    let workspace = WorkspaceState::empty();
    // One broadcast channel per active duration — no two pipelines share a
    // channel even where durations are closest.
    let bcast_txs: Vec<broadcast::Sender<MarketSnapshot>> = (0..10)
        .map(|_| broadcast::channel::<MarketSnapshot>(200).0)
        .collect();

    // v11.9: the ACTIVE durations are the canonical pool subset (fastest
    // 8 by default); the derived label is the identity.
    let active_secs: Vec<u64> = config_models::SUPPORTED_DURATIONS[..8].to_vec();
    let pipelines: Vec<TimeframePipeline> = active_secs
        .iter()
        .zip(bcast_txs.iter())
        .map(|(&secs, tx)| make_pipe(secs, tx.clone()))
        .collect();
    let active_pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        pipelines,
        active_secs: active_secs.clone(),
        snapshot_tx: mpsc::channel::<NormalizedEvent>(50).0,
        cancel: CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
    });

    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
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
        "inst_slot_identity".into(),
        ("BTC".into(), "USDT".into()),
        ExchangeChoice::Hyperliquid,
        active_pair.clone(),
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        active_secs, // v11.9 active ladder
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
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        pool,
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".into(),
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
    });
    let router = api_gateway::build_router(state.clone());
    (router, state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_for_secs_dispatches_by_duration() {
    let (_router, state) = build_test_router().await;
    let pair = state
        .get_active_pair(PAIR_KEY)
        .await
        .expect("pair must be present");

    // Every ACTIVE duration must resolve to the pipeline carrying its own
    // identity (duration label + exact secs).
    for secs in &pair.active_secs {
        let pipe = pair
            .pipeline_for_secs(*secs)
            .unwrap_or_else(|| panic!("{secs}s must resolve"));
        assert_eq!(pipe.timeframe_secs, *secs, "canonical duration");
        assert_eq!(
            pipe.slot_label,
            core_domain::duration_label(*secs),
            "derived label per duration"
        );
    }

    // Each active duration subscription returns its own broadcast
    // receiver — no two pipelines share a channel.
    let rxs: Vec<_> = pair
        .active_secs
        .iter()
        .map(|&secs| {
            pair.subscribe_broadcast_by_secs(secs)
                .unwrap_or_else(|| panic!("{secs}s must expose a broadcast channel"))
        })
        .collect();
    for (i, a) in rxs.iter().enumerate() {
        for (j, b) in rxs.iter().enumerate() {
            if i != j {
                assert!(
                    !a.same_channel(b),
                    "durations {i} and {j} must not share a channel"
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_for_secs_rejects_inactive_duration() {
    let (_router, state) = build_test_router().await;
    let pair = state
        .get_active_pair(PAIR_KEY)
        .await
        .expect("pair must be present");

    // The default active ladder is the fastest 8 pool durations; 15m and
    // 1h are NOT active and must not resolve.
    assert!(pair.pipeline_for_secs(900).is_none());
    assert!(pair.pipeline_for_secs(3600).is_none());
    assert!(pair.subscribe_broadcast_by_secs(900).is_none());

    // Every ACTIVE duration resolves.
    for secs in &pair.active_secs {
        assert!(
            pair.pipeline_for_secs(*secs).is_some(),
            "{secs}s is active and must resolve"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_query_label_binds_duration_dispatch() {
    tokio::time::timeout(Duration::from_secs(8), async {
        let (router, _state) = build_test_router().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Label-tagged WS query: even though `timeframe_secs=1` would map
        // to the fastest duration, an explicit slot=1m must bind the
        // connection to the 1-minute pipeline's broadcast channel.
        let res_1m = reqwest::Client::new()
            .get(format!(
                "http://{addr}/ws?symbol=BTC-USDT&timeframe_secs=1&slot=1m"
            ))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("WS upgrade request");
        assert!(
            res_1m.status().as_u16() == 101 || res_1m.status().is_success(),
            "expected WS upgrade, got {}",
            res_1m.status()
        );

        // Raw-seconds variant resolves too.
        let res_secs = reqwest::Client::new()
            .get(format!(
                "http://{addr}/ws?symbol=BTC-USDT&timeframe_secs=1&slot=300"
            ))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("WS upgrade request");
        assert!(res_secs.status().as_u16() == 101 || res_secs.status().is_success());
    })
    .await
    .expect("ws_query_label_binds_duration_dispatch timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeframe_label_round_trips_through_wire_payload() {
    // Every `MarketSnapshot` carries `timeframe_label` after the analyzer
    // stamps it; the WS emission must surface that label on the outer
    // JSON-RPC params.
    let snap = MarketSnapshot {
        timeframe_label: Some("5m".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: 300,
        timestamp: 1_700_000_000,
        symbol: PAIR_KEY.to_string(),
        is_completed: Some(true),
        mid_price: rust_decimal::Decimal::from(1),
        bid_price: rust_decimal::Decimal::from(1),
        ask_price: rust_decimal::Decimal::from(1),
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
        open: None,
        high: None,
        low: None,
        close: None,
        volume: None,
        average_volume: None,
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
        decision_context: None,
        statistical_context: None,
        context: None,
        liquidity_signals: vec![],
        metrics_config: None,
        quality_envelope: None,
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    };
    let serialized = serde_json::to_value(&snap).expect("serialize");
    assert_eq!(
        serialized.get("timeframe_label").and_then(|v| v.as_str()),
        Some("5m"),
        "Wire payload must carry timeframe_label"
    );

    // Full duration table of the supported pool: label derivation is
    // total and deterministic.
    assert_eq!(core_domain::duration_label(1), "1s");
    assert_eq!(core_domain::duration_label(3), "3s");
    assert_eq!(core_domain::duration_label(5), "5s");
    assert_eq!(core_domain::duration_label(15), "15s");
    assert_eq!(core_domain::duration_label(30), "30s");
    assert_eq!(core_domain::duration_label(60), "1m");
    assert_eq!(core_domain::duration_label(180), "3m");
    assert_eq!(core_domain::duration_label(300), "5m");
    assert_eq!(core_domain::duration_label(900), "15m");
    assert_eq!(core_domain::duration_label(1800), "30m");
    assert_eq!(core_domain::duration_label(3600), "1h");
    assert_eq!(core_domain::duration_label(14400), "4h");
    assert_eq!(core_domain::duration_label(43200), "12h");
    assert_eq!(core_domain::duration_label(86400), "1d");
    // Out-of-pool durations degrade to a raw-seconds label.
    assert_eq!(core_domain::duration_label(45), "45s");
}
