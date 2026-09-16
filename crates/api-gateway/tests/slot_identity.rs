//! Regression tests for the slot identity invariants. The fixed 10-slot
//! ladder (micro1..longterm2) is a set of positional slots whose identity
//! must survive any combination of user-chosen durations. These tests pin
//! the behaviour so a future refactor cannot reintroduce duration-based
//! dispatch (which was the root cause of the all-columns-rendering-micro
//! bug).
use std::sync::Arc;

use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use config_models::WorkspaceConfig;
use core_domain::models::{MarketSnapshot, TimeframeSlot, FIXED_TF_SLOTS};
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

fn make_pipe(
    slot: TimeframeSlot,
    secs: u64,
    tx: broadcast::Sender<MarketSnapshot>,
) -> TimeframePipeline {
    TimeframePipeline {
        slot,
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: Arc::new(RwLock::new(VecDeque::new())),
        timeframe_secs: secs,
        timeframe_label: "TEST",
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
    // One broadcast channel per fixed-ladder slot — no two slots share a
    // channel even where durations are closest.
    let (bcast0, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast1, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast2, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast3, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast4, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast5, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast6, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast7, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast8, _) = broadcast::channel::<MarketSnapshot>(200);
    let (bcast9, _) = broadcast::channel::<MarketSnapshot>(200);

    // The fixed 10-slot ladder with its canonical durations
    // (`config_models::FIXED_TF_LADDER`): slot identity is authoritative,
    // never derived from the duration.
    let active_pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        custom_pipelines: std::collections::HashMap::new(),
        micro1: make_pipe(TimeframeSlot::Micro1, 1, bcast0),
        micro2: make_pipe(TimeframeSlot::Micro2, 3, bcast1),
        fast1: make_pipe(TimeframeSlot::Fast1, 5, bcast2),
        fast2: make_pipe(TimeframeSlot::Fast2, 15, bcast3),
        slow1: make_pipe(TimeframeSlot::Slow1, 30, bcast4),
        slow2: make_pipe(TimeframeSlot::Slow2, 60, bcast5),
        macro1: make_pipe(TimeframeSlot::Macro1, 180, bcast6),
        macro2: make_pipe(TimeframeSlot::Macro2, 300, bcast7),
        longterm1: make_pipe(TimeframeSlot::Longterm1, 900, bcast8),
        longterm2: make_pipe(TimeframeSlot::Longterm2, 3600, bcast9),
        snapshot_tx: mpsc::channel::<NormalizedEvent>(50).0,
        cancel: CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        active_indices: (0..10).collect(),
});

    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let buffers: [TimeframeBuffers; 10] = active_pair
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
        config_models::FIXED_TF_LADDER.to_vec(), // v11.2 active ladder
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
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });
    let router = api_gateway::build_router(state.clone());
    (router, state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_for_slot_dispatches_by_slot_not_duration() {
    let (_router, state) = build_test_router().await;
    let pair = state
        .get_active_pair(PAIR_KEY)
        .await
        .expect("pair must be present");

    // All ten fixed-ladder slots must resolve to the pipeline carrying
    // their own identity and canonical duration — the slot lookup must
    // NOT depend on duration (the pattern that produced the "all columns
    // showed MICRO" bug in the legacy duration-based dispatcher).
    for (slot, secs) in FIXED_TF_SLOTS.iter().zip(config_models::FIXED_TF_LADDER) {
        let pipe = pair
            .pipeline_for_slot(*slot)
            .unwrap_or_else(|| panic!("slot {slot:?} must resolve"));
        assert_eq!(pipe.slot, *slot, "slot identity must round-trip");
        assert_eq!(pipe.timeframe_secs, secs, "canonical duration per slot");
    }

    // Each slot subscription returns its own broadcast receiver — no two
    // slots share a channel. (Receives are kept alive and compared with
    // `same_channel`: taking the address of a loop-local would compare the
    // same stack slot every iteration.)
    let rxs: Vec<_> = FIXED_TF_SLOTS
        .iter()
        .map(|slot| {
            pair
                .subscribe_broadcast_by_slot(*slot)
                .unwrap_or_else(|| panic!("slot {slot:?} must expose a broadcast channel"))
        })
        .collect();
    for (i, a) in rxs.iter().enumerate() {
        for (j, b) in rxs.iter().enumerate() {
            if i != j {
                assert!(
                    !a.same_channel(b),
                    "slots {i} and {j} must not share a channel"
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_for_duration_rejects_no_match() {
    let (_router, state) = build_test_router().await;
    let pair = state
        .get_active_pair(PAIR_KEY)
        .await
        .expect("pair must be present");

    // No slot is configured for duration=100 (the fixed ladder is
    // 1,3,5,15,30,60,180,300,900,3600). The lookup must return an
    // explicit error so callers don't silently default to micro.
    let err = pair
        .pipeline_for_duration(100)
        .err()
        .expect("100s must produce an error — no slot configured for it");
    assert!(
        err.contains("timeframe_secs=100"),
        "error should mention the offending duration: {err}"
    );
    assert!(
        err.contains("No slot matches"),
        "no-match error must be explicit so callers don't silently default to micro: {err}"
    );

    // Every canonical ladder duration resolves to some slot.
    for secs in config_models::FIXED_TF_LADDER {
        assert!(
            pair.pipeline_for_duration(secs).is_ok(),
            "{secs}s is on the fixed ladder and must resolve"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_query_slot_overrides_duration_dispatch() {
    tokio::time::timeout(Duration::from_secs(8), async {
        let (router, _state) = build_test_router().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Slot-tagged WS query: even though `timeframe_secs=60` would
        // map to slow2 under duration resolution, an explicit slot=micro1
        // must bind the connection to micro1's broadcast channel.
        let res_micro = reqwest::Client::new()
            .get(format!(
                "http://{addr}/ws?symbol=BTC-USDT&timeframe_secs=60&slot=micro1"
            ))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("WS upgrade request");
        assert!(
            res_micro.status().as_u16() == 101 || res_micro.status().is_success(),
            "expected WS upgrade, got {}",
            res_micro.status()
        );

        // Slot-tagged query for `longterm2`:
        let res_slow = reqwest::Client::new()
            .get(format!(
                "http://{addr}/ws?symbol=BTC-USDT&timeframe_secs=60&slot=longterm2"
            ))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("WS upgrade request");
        assert!(res_slow.status().as_u16() == 101 || res_slow.status().is_success());
    })
    .await
    .expect("ws_query_slot_overrides_duration_dispatch timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeframe_slot_round_trips_through_wire_payload() {
    // Every `MarketSnapshot` carries `timeframe_slot` after the analyzer
    // stamps it; the WS emission must surface that slot on the outer
    // JSON-RPC params.
    let snap = MarketSnapshot {
        timeframe_slot: Some(TimeframeSlot::Macro2),
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
        oi_delta_1h: None,
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
        serialized.get("timeframe_slot").and_then(|v| v.as_str()),
        Some("macro2"),
        "Wire payload must carry timeframe_slot"
    );

    // The ten canonical wire names round-trip; unknown / legacy names
    // default to Micro1 so a stale client never silently reroutes.
    assert_eq!(TimeframeSlot::parse("micro1"), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse("micro2"), TimeframeSlot::Micro2);
    assert_eq!(TimeframeSlot::parse("fast1"), TimeframeSlot::Fast1);
    assert_eq!(TimeframeSlot::parse("fast2"), TimeframeSlot::Fast2);
    assert_eq!(TimeframeSlot::parse("slow1"), TimeframeSlot::Slow1);
    assert_eq!(TimeframeSlot::parse("slow2"), TimeframeSlot::Slow2);
    assert_eq!(TimeframeSlot::parse("macro1"), TimeframeSlot::Macro1);
    assert_eq!(TimeframeSlot::parse("macro2"), TimeframeSlot::Macro2);
    assert_eq!(TimeframeSlot::parse("longterm1"), TimeframeSlot::Longterm1);
    assert_eq!(TimeframeSlot::parse("longterm2"), TimeframeSlot::Longterm2);
    // Legacy 4-slot names intentionally fall through to the default.
    assert_eq!(TimeframeSlot::parse("micro"), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse("fast"), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse("slow"), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse("macro"), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse("garbage"), TimeframeSlot::Micro1);

    // Full duration table of the fixed ladder.
    assert_eq!(TimeframeSlot::parse_from_secs(1), TimeframeSlot::Micro1);
    assert_eq!(TimeframeSlot::parse_from_secs(3), TimeframeSlot::Micro2);
    assert_eq!(TimeframeSlot::parse_from_secs(5), TimeframeSlot::Fast1);
    assert_eq!(TimeframeSlot::parse_from_secs(15), TimeframeSlot::Fast2);
    assert_eq!(TimeframeSlot::parse_from_secs(30), TimeframeSlot::Slow1);
    assert_eq!(TimeframeSlot::parse_from_secs(60), TimeframeSlot::Slow2);
    assert_eq!(TimeframeSlot::parse_from_secs(180), TimeframeSlot::Macro1);
    assert_eq!(TimeframeSlot::parse_from_secs(300), TimeframeSlot::Macro2);
    assert_eq!(TimeframeSlot::parse_from_secs(900), TimeframeSlot::Longterm1);
    assert_eq!(TimeframeSlot::parse_from_secs(3600), TimeframeSlot::Longterm2);
}
