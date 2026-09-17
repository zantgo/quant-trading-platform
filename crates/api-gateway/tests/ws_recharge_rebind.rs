//! Regression tests for the "WS broadcast freezes after instance recharge" bug.
//!
//! Symptom (pre-fix): when the user reconfigured an instance's timeframes via
//! `WorkspaceSettings` / `TimeframeSettings`, the dashboard chart stopped
//! updating even though the backend kept logging "🕯️ Candle Aggregated".
//!
//! Root cause (pre-fix): the WS handler in `crates/api-gateway/src/ws.rs`
//! cached an `Arc<ActivePair>` at connection time and bound a
//! `tokio::sync::broadcast::Receiver` to the channel inside that snapshot.
//! When `recharge_instance` rebuilt the pipeline it allocated a brand-new
//! `Arc<ActivePair>` with new broadcast channels and swapped the workspace
//! map. The WS handler kept the OLD `Arc<ActivePair>` alive (its embedded
//! `Sender` therefore never dropped), so the OLD channel never reported
//! `Closed` and `rx_stream.recv().await` blocked forever. The TCP socket
//! stayed open, the frontend never received `onclose`, and
//! `shouldReconnect` saw all 4 sockets still in `WebSocket.OPEN` state and
//! refused to reconnect. The chart froze silently.
//!
//! Fix: a process-wide `recharge_tx: broadcast::Sender<RechargeNotice>`
//! lives on `AppState`. `serve_update_instance_config` publishes a notice
//! after `recharge_instance` returns. The WS handler subscribes via
//! `tokio::select!` and, on receiving a notice for its own pair, drops its
//! cached `Receiver` and rebinds to the freshly installed
//! `ActivePair`'s broadcast channel.
//!
//! These tests exercise:
//!   1. The orphan behaviour (pre-fix root cause) is reproducible in
//!      isolation against the `tokio::sync::broadcast` API.
//!   2. The full WS handler rebinds to the new `ActivePair` after a
//!      recharge notice is fired, so a live browser sees the post-recharge
//!      snapshots instead of freezing.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use api_gateway::{self, AppState, RechargeNotice};
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
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR_KEY: &str = "BTC-USDT";
const INSTANCE_ID: &str = "inst_recharge_rebind";

fn make_snapshot(timeframe_secs: u64, mid_price: f64) -> MarketSnapshot {
    MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
        timeframe_secs,
        timestamp: 1_700_000_000,
        symbol: PAIR_KEY.to_string(),
        is_completed: Some(true),
        mid_price: rust_decimal::Decimal::from(mid_price as i64),
        bid_price: rust_decimal::Decimal::from(mid_price as i64),
        ask_price: rust_decimal::Decimal::from(mid_price as i64),
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
    }
}

/// Build an `ActivePair` wired to a fresh set of broadcast channels.
/// Returns the pair plus the `broadcast::Sender`s of the four
/// representative active durations (1s/5s/30s/180s) so the test can
/// publish snapshots on any of them; the remaining six carry throwaway
/// channels.
type PairWithSenders = (
    Arc<ActivePair>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
);
fn build_active_pair_with_channels(pair_key: &str) -> PairWithSenders {
    let (micro_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (fast_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (slow_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (macro_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let throwaway = || broadcast::channel::<MarketSnapshot>(10).0;
    let (snapshot_tx, _snapshot_rx) =
        mpsc::channel::<core_domain::normalized::NormalizedEvent>(100);
    let cancel = CancellationToken::new();
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let new_pipe = |secs: u64, tx: broadcast::Sender<MarketSnapshot>| TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist.clone(),
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
    };
    let pair = Arc::new(ActivePair {
        symbol: pair_key.to_string(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        pipelines: vec![
            new_pipe(1, micro_bcast.clone()),
            new_pipe(3, throwaway()),
            new_pipe(5, fast_bcast.clone()),
            new_pipe(15, throwaway()),
            new_pipe(30, slow_bcast.clone()),
            new_pipe(60, throwaway()),
            new_pipe(180, macro_bcast.clone()),
            new_pipe(300, throwaway()),
            new_pipe(900, throwaway()),
            new_pipe(3600, throwaway()),
        ],
        active_secs: config_models::SUPPORTED_DURATIONS.to_vec(),
        snapshot_tx,
        cancel,
    });
    (pair, micro_bcast, fast_bcast, slow_bcast, macro_bcast)
}

fn make_buffers_for(pair: &ActivePair) -> Vec<TimeframeBuffers> {
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    pair.all()
        .iter()
        .map(|pipe| TimeframeBuffers {
            history: pipe.history.clone(),
            latest: pipe.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        })
        .collect()
}

async fn setup_app_with_pair() -> (
    Arc<AppState>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
    broadcast::Sender<MarketSnapshot>,
) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    database_storage::run_migrations(&pool).await.unwrap();
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
    let (pair, micro_bcast, fast_bcast, slow_bcast, macro_bcast) =
        build_active_pair_with_channels(PAIR_KEY);
    let buffers = make_buffers_for(&pair);

    let instance = Arc::new(Instance::new(
        INSTANCE_ID.to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        ExchangeChoice::Hyperliquid,
        pair.clone(),
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
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
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
    });
    (state, micro_bcast, fast_bcast, slow_bcast, macro_bcast)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orphaned_active_pair_receiver_never_sees_new_publisher() {
    // Reproduce the exact pre-fix failure mode in isolation, without the
    // WS layer. The expectation is that the test demonstrates the orphan
    // behaviour the fix must work around: a Receiver bound to the OLD
    // channel stays silent even though the publisher kept firing on the
    // NEW channel.

    let (state, micro_old, _fast_old, _slow_old, _macro_old) = setup_app_with_pair().await;

    // Keep at least one subscriber alive on the recharge channel so the
    // `send` below doesn't fail with `SendError(no receivers)`. In
    // production the WS handler always holds a Receiver; here we mimic
    // that with a long-lived subscriber that the test never consumes.
    let mut _recharge_keepalive = state.recharge_tx.subscribe();

    // 1. Take a Receiver on the OLD ActivePair's micro channel, mimicking
    //    what the WS handler does on connect.
    let old_pair = state.get_active_pair(PAIR_KEY).await.unwrap();
    let mut old_rx = old_pair.subscribe_broadcast_by_secs(1);

    // 2. Sanity: the OLD channel delivers a snapshot before any swap.
    micro_old
        .send(make_snapshot(60, 100.0))
        .expect("send to OLD channel");
    let snap = old_rx
        .as_mut()
        .expect("micro slot must be present")
        .recv()
        .await
        .expect("recv on OLD channel");
    assert_eq!(snap.mid_price, rust_decimal::Decimal::from(100));

    // 3. Simulate `recharge_instance`: build a NEW ActivePair with a NEW
    //    broadcast channel, swap it into the workspace. The OLD pair is
    //    still kept alive by our local `old_pair` clone (exactly the WS
    //    handler's situation).
    let (new_pair, micro_new, _fast_new, _slow_new, _macro_new) =
        build_active_pair_with_channels(PAIR_KEY);
    let buffers = make_buffers_for(&new_pair);
    let new_instance = Arc::new(Instance::new(
        INSTANCE_ID.to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        ExchangeChoice::Hyperliquid,
        new_pair.clone(),
        state.pool.clone(),
        state.workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        config_models::SUPPORTED_DURATIONS.to_vec(), // v11.9 active ladder
        Default::default(),
    ));
    state
        .workspace
        .insert(PAIR_KEY.to_string(), new_instance)
        .await;

    // 4. Fire the post-recharge notice (the fix). Tolerate the absence of
    //    receivers in the unlikely race where the keepalive is dropped.
    let _ = state.recharge_tx.send(RechargeNotice {
        pair_key: PAIR_KEY.to_string(),
    });

    // 5. Publish on the NEW channel — the WS handler (after the fix) is
    //    now reading from it, but our pre-swap `old_rx` is still on the
    //    OLD channel and must NOT see anything. Subscribe a long-lived
    //    receiver first so the `send` calls have someone to deliver to.
    let _new_keepalive = micro_new.clone();
    let mut new_rx_ahead = micro_new.subscribe();
    micro_new
        .send(make_snapshot(60, 200.0))
        .expect("send to NEW channel");
    micro_new
        .send(make_snapshot(60, 201.0))
        .expect("send to NEW channel");

    // 6. The OLD receiver stays silent. We poll briefly to confirm.
    let old_seen_new = tokio::time::timeout(Duration::from_millis(150), async {
        loop {
            match old_rx
                .as_mut()
                .expect("micro slot must be present")
                .recv()
                .await
            {
                Ok(snap)
                    if snap.mid_price == rust_decimal::Decimal::from(200)
                        || snap.mid_price == rust_decimal::Decimal::from(201) =>
                {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        !old_seen_new,
        "OLD receiver must not see NEW-publisher snapshots (orphan behaviour intact)"
    );

    // 7. The NEW receiver (mimicking the WS handler after re-subscribe)
    //    sees both new snapshots. Drain the two frames we just enqueued
    //    on the NEW channel.
    let latest_pair = state.get_active_pair(PAIR_KEY).await.unwrap();
    assert!(
        !Arc::ptr_eq(&latest_pair, &old_pair),
        "workspace entry must point at the NEW ActivePair"
    );
    let snap1 = new_rx_ahead.recv().await.expect("recv 200 on NEW");
    assert_eq!(snap1.mid_price, rust_decimal::Decimal::from(200));
    let snap2 = new_rx_ahead.recv().await.expect("recv 201 on NEW");
    assert_eq!(snap2.mid_price, rust_decimal::Decimal::from(201));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_handler_rebinds_after_recharge_notice() {
    // End-to-end: spin up the real axum router, open a WebSocket client,
    // publish a snapshot before the recharge, swap the workspace entry,
    // publish a RechargeNotice, then publish a snapshot via the NEW
    // channel. The client must receive the new snapshot — which it can
    // only do if the WS handler re-subscribed to the new
    // `ActivePair` after observing the notice.

    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

    let (state, micro_old, _fast_old, _slow_old, _macro_old) = setup_app_with_pair().await;

    let router = api_gateway::build_router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Open a WS client. The handler will cache the OLD ActivePair and
    // subscribe to its micro broadcast channel.
    let req = format!("ws://{addr}/ws?symbol={PAIR_KEY}&timeframe_secs=60&slot=1s")
        .into_client_request()
        .unwrap();
    let (mut ws, _resp) = tokio_tungstenite::connect_async(req).await.unwrap();

    // 1. Pre-recharge snapshot delivered.
    micro_old
        .send(make_snapshot(60, 100.0))
        .expect("send pre-recharge");
    let pre_frame = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("WS frame before recharge timed out")
        .expect("WS stream ended before recharge")
        .expect("WS frame error before recharge");
    let pre_text = match pre_frame {
        Message::Text(t) => t,
        other => panic!("expected Text frame, got {other:?}"),
    };
    assert!(
        pre_text.contains("100") && pre_text.contains("BTC-USDT"),
        "pre-recharge frame must include the price 100; got: {pre_text}"
    );

    // 2. Simulate recharge: NEW ActivePair + NEW channels into workspace.
    let (new_pair, micro_new, _fast_new, _slow_new, _macro_new) =
        build_active_pair_with_channels(PAIR_KEY);
    let buffers = make_buffers_for(&new_pair);
    let new_instance = Arc::new(Instance::new(
        INSTANCE_ID.to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        ExchangeChoice::Hyperliquid,
        new_pair.clone(),
        state.pool.clone(),
        state.workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        config_models::SUPPORTED_DURATIONS.to_vec(), // v11.9 active ladder
        Default::default(),
    ));
    state
        .workspace
        .insert(PAIR_KEY.to_string(), new_instance)
        .await;

    // 3. Fire the RechargeNotice. The WS handler's `tokio::select!` arm
    //    will see it on the next loop iteration and re-subscribe. The
    //    WS handler always holds a Receiver on `recharge_tx`, so this
    //    send is guaranteed to have at least one subscriber. Tolerate
    //    transient races with `let _ =`.
    let _ = state.recharge_tx.send(RechargeNotice {
        pair_key: PAIR_KEY.to_string(),
    });

    // 4. Give the handler a chance to drain its `select!` arm.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5. Publish on the NEW channel. Pre-fix this would be invisible to
    //    the WS handler (orphan). Post-fix the handler is now subscribed
    //    to the NEW channel and forwards the frame.
    micro_new
        .send(make_snapshot(60, 999.0))
        .expect("send post-recharge");

    let post_frame = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap_or_else(|_| panic!("WS frame after recharge timed out — handler is still orphaned"))
        .expect("WS stream ended after recharge")
        .expect("WS frame error after recharge");
    let post_text = match post_frame {
        Message::Text(t) => t,
        other => panic!("expected Text frame, got {other:?}"),
    };
    assert!(
        post_text.contains("999") && post_text.contains("BTC-USDT"),
        "post-recharge frame must include the new price 999 — got: {post_text}"
    );

    // 6. Clean shutdown.
    let _ = ws.close(None).await;
    server.abort();
}
