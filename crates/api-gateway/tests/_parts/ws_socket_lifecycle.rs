//! AUDIT-M11 regression: dashboard WebSocket sockets must have liveness
//! detection, so a departed peer cannot pin server resources indefinitely.
//!
//! The defect: the per-socket loop in `api-gateway/src/ws.rs` only ever WROTE.
//! There was no `socket.recv()`, so a close frame was never observed, and no
//! ping, so the only thing that ever noticed a departed peer was the NEXT
//! outgoing frame failing.
//!
//! Outgoing frames are not a usable liveness signal: shadow frames are throttled
//! to `tf_secs * 1000 / 4` (see `market_analyzer::analyzer`), which is
//! 3,600,000 ms — ONE HOUR — for a 4h slot, and they only fire on market events
//! at all. So a dead 1h/4h socket lived on for up to an hour per tab-close,
//! holding the `Arc<ActivePair>` (and every one of its durations' retained
//! history) via `current_pair`, and counting against `MAX_WS_CONNECTIONS`.
//! Roughly 40 live sockets were pinned per tab-close; after ~13 tab closes
//! inside one hour new upgrades were refused with a bare 429, which the client
//! cannot distinguish from a server fault.
//!
//! These are LOAD-style tests against the real router over a real TCP listener,
//! not unit tests: the failure mode only exists in the socket task's lifetime.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use api_gateway::{self, AppState};
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
use std::sync::OnceLock;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

const PAIR_KEY: &str = "BTC-USDT";

/// AUDIT-WS-TEST-ISOLATION: serialise the tests in this binary.
///
/// `connected_client_count()` reads a PROCESS-GLOBAL counter
/// (`ws::ACTIVE_WS_CONNECTIONS`), and each test asserts on an EXACT count
/// (`wait_for_connections(1, ..)`). Cargo runs test functions in parallel
/// within one binary, so a socket opened by `churned_slow_duration_..` was
/// counted against `a_healthy_socket_is_counted_and_stays_open`'s expectation of
/// exactly 1 — the assertion failed for reasons that had nothing to do with the
/// liveness behaviour under test.
///
/// These tests passed only when the harness happened to run them serially;
/// `cargo test -- --test-threads=1` makes all three pass deterministically. A
/// global resource needs a global lock; `tokio::sync::Mutex` (not `std`) so the
/// guard can be held across `.await` inside the async test bodies.
static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

async fn serial_guard() -> tokio::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| Mutex::new(())).lock().await
}

/// Build a state with one instance whose pipelines all share a broadcast channel
/// that nothing ever writes to — i.e. a socket that receives NO frames. That is
/// exactly the condition that made liveness undetectable before the fix: with
/// no shadow or completed frames, the only write is the initial cached snapshot.
async fn app_state() -> Arc<AppState> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("sqlite");
    let symbol_mapper = Arc::new(SymbolMapper::new());
    symbol_mapper
        .register(Exchange::Hyperliquid, "BTC", PAIR_KEY)
        .await;
    let (telemetry_tx, _) = mpsc::channel::<database_storage::TelemetryMsg>(100);

    let workspace = WorkspaceState::empty();
    let (bcast_tx, _keep) = broadcast::channel::<MarketSnapshot>(32);

    let secs_list: Vec<u64> = vec![1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400];
    let pipes: Vec<TimeframePipeline> = secs_list
        .iter()
        .map(|&secs| TimeframePipeline {
            slot_label: core_domain::duration_label(secs),
            history: Arc::new(RwLock::new(Default::default())),
            broadcast_tx: bcast_tx.clone(),
            latest_snapshot: Arc::new(RwLock::new(None)),
            snapshot_history: Arc::new(RwLock::new(Default::default())),
            timeframe_secs: secs,
            divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
            sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
            fibonacci: Default::default(),
            latest_oi: Arc::new(RwLock::new(None)),
            latest_funding: Arc::new(RwLock::new(None)),
            latest_mark_px: Arc::new(RwLock::new(None)),
            latest_index_px: Arc::new(RwLock::new(None)),
            active_set: Default::default(),
            cluster_matrix: Arc::new(RwLock::new(None)),
            cluster_status: Arc::new(RwLock::new(
                core_domain::liquidity::ClusterStatusSnapshot::pending("T", "t"),
            )),
            pipeline_state: Arc::new(RwLock::new(
                core_domain::models::CandlePipelineState::Initializing,
            )),
            indicator_lifecycle: Arc::new(RwLock::new(Default::default())),
            tf_leverage_config: Arc::new(Default::default()),
            buffer_size: 500,
            stale_threshold_secs: 300,
        })
        .collect();

    let buffers: Vec<TimeframeBuffers> = pipes
        .iter()
        .map(|p| TimeframeBuffers {
            history: p.history.clone(),
            latest: p.latest_snapshot.clone(),
            snapshot_history: p.snapshot_history.clone(),
        })
        .collect();

    let active_pair = Arc::new(ActivePair {
        symbol: PAIR_KEY.to_string(),
        pipelines: pipes,
        active_secs: secs_list.clone(),
        snapshot_tx: mpsc::channel::<core_domain::normalized::NormalizedEvent>(50).0,
        cancel: CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(Default::default())),
        funding_history: Arc::new(RwLock::new(Default::default())),
        latency_tracker: Arc::new(Default::default()),
    });

    let instance = Arc::new(Instance::new(
        "inst_ws".into(),
        ("BTC".into(), "USDT".into()),
        ExchangeChoice::Hyperliquid,
        "BTC".into(),
        active_pair,
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        secs_list,
        Default::default(),
    ));
    workspace.insert(PAIR_KEY.to_string(), instance).await;
    workspace.set_config(Default::default()).await;

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
        snapshot_export_manual_tick: notify_handle(),
        session_id: Arc::new(RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 0),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

fn notify_handle() -> Arc<tokio::sync::Notify> {
    Arc::new(tokio::sync::Notify::new())
}

/// Serve the real router on a real port and return its base URL.
async fn serve(state: Arc<AppState>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let app = api_gateway::build_router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

async fn open_socket(
    addr: SocketAddr,
    secs: u64,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{addr}/ws?symbol={PAIR_KEY}&tf={secs}");
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect");
    ws
}

/// Poll until the server-side connection count reaches `want`, or time out.
async fn wait_for_connections(want: usize, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if api_gateway::ws::connected_client_count() == want {
            return true;
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_no_connections(within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if api_gateway::ws::connected_client_count() == 0 {
            return true;
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// THE load test: churn 12 sockets on a SLOW duration, abandoning each batch
/// abruptly (drop the transport with no close frame). The server must reclaim
/// every slot rather than accumulating them up to the connection cap.
///
/// Before the fix a 1h slot received no shadow frames at all, so the socket task
/// never wrote anything after the (absent) cached snapshot and never noticed the
/// peer was gone — the slot stayed counted until the cap was hit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn churned_slow_duration_sockets_do_not_exhaust_the_connection_cap() {
    let _guard = serial_guard().await;
    let state = app_state().await;
    let addr = serve(state).await;

    // Start from a clean slate: other tests in this binary share the global
    // connection counter.
    assert!(
        wait_for_no_connections(Duration::from_secs(5)).await,
        "test suite must start with no live sockets, found {}",
        api_gateway::ws::connected_client_count()
    );

    // Three rounds of open-4 / abandon-all, on the 1h duration.
    for round in 0..3 {
        let mut sockets = Vec::new();
        for _ in 0..4 {
            sockets.push(open_socket(addr, 3600).await);
        }
        assert!(
            wait_for_connections(4, Duration::from_secs(5)).await,
            "round {round}: sockets should register"
        );

        // ABANDON: drop the transport without a close handshake.
        drop(sockets);

        // The server must reclaim them via its keepalive ping, which is bounded
        // by WS_KEEPALIVE_INTERVAL (30 s) and is independent of the shadow-frame
        // cadence (which is an HOUR at this duration).
        assert!(
            wait_for_no_connections(Duration::from_secs(45)).await,
            "round {round}: abandoned sockets must be reclaimed, {} still counted",
            api_gateway::ws::connected_client_count()
        );
    }
}

/// A graceful close must still be reclaimed — guards against the keepalive
/// regressing the good path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_graceful_close_is_reclaimed_immediately() {
    let _guard = serial_guard().await;
    let state = app_state().await;
    let addr = serve(state).await;
    assert!(wait_for_no_connections(Duration::from_secs(5)).await);

    let mut ws = open_socket(addr, 60).await;
    assert!(
        wait_for_connections(1, Duration::from_secs(5)).await,
        "socket should register"
    );

    // A proper close frame.
    let _ = ws.close(None).await;
    drop(ws);

    assert!(
        wait_for_no_connections(Duration::from_secs(5)).await,
        "a graceful close must be reclaimed promptly, {} still counted",
        api_gateway::ws::connected_client_count()
    );
}

/// A socket whose duration pipeline exists must serve normally and be counted —
/// the liveness work must not break the happy path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_healthy_socket_is_counted_and_stays_open() {
    let _guard = serial_guard().await;
    let state = app_state().await;
    let addr = serve(state).await;
    assert!(wait_for_no_connections(Duration::from_secs(5)).await);

    let _ws = open_socket(addr, 60).await;
    assert!(
        wait_for_connections(1, Duration::from_secs(5)).await,
        "a healthy socket must be counted"
    );

    // Hold it across a keepalive interval; it must survive.
    tokio::time::sleep(Duration::from_secs(35)).await;
    assert_eq!(
        api_gateway::ws::connected_client_count(),
        1,
        "a connected client must not be reaped by its own keepalive"
    );
}
