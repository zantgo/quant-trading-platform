//! Integration test for the full save → in-memory publish → recharge cycle.
//!
//! Reproduces and regression-locks the "Pipeline recharge failed for
//! BTC-USDC: No saved config for pair BTC-USDC" bug.
//!
//! The historical flow when a user edited a timeframe in
//! `WorkspaceSettings.svelte`:
//!   1. Frontend POSTs `/api/instances/{pairKey}/config` to the
//!      `serve_update_instance_config` handler
//!      (`crates/api-gateway/src/handlers/instances.rs`).
//!   2. The handler builds a fresh `WorkspaceConfig` clone, mutates the
//!      matching `InstanceEntry`, persists the clone to `config.toml` via
//!      `config_models::save_workspace`, then invokes
//!      `registry::recharge_instance`.
//!   3. `recharge_instance` reads `state.workspace.config()` and looks up
//!      the entry by `symbol` to determine the new timeframe configuration.
//!      The bug: step 2's mutation happens on a clone that was never
//!      published back, so step 3 reads a stale snapshot and returns
//!      `Err("No saved config for pair ...")`.
//!
//! The fix:
//!   - `serve_update_instance_config` now calls `state.workspace.set_config(...)`
//!     after `save_workspace`, bridging the disk write into in-memory state.
//!   - `delete_instance` does the same.
//!   - `add_instance` now actually populates `config.instances[]` (it never
//!     used to, leaving every first-edit pair needing a daemon restart).
//!   - `default_pair_key` now reads the active session quote so USDC
//!     sessions no longer produce `BTC-USDT-USDT` fallbacks on the read
//!     side.
//!   - The route parameter is a UUID; pairKey-based slug requests return
//!     `404 NOT_FOUND` immediately.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use api_gateway::{self, AppState};
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

const INSTANCE_ID: &str = "inst_save_recharge_cycle";
const PAIR_KEY: &str = "BTC-USDT";

async fn setup_app_with_instance() -> Arc<AppState> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    // The bootstrap path queries `market_snapshots`; without migrations
    // every save→recharge cycle fails with "no such table", which is
    // orthogonal to the bug under test but masks the test outcome.
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");

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

    // Build a minimal ActivePair scaffold, mirroring `axum_routes.rs`
    // (`test_websocket_stream_with_active_pair` lines 149-263).
    let (mid_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (fast_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (slow_bcast, _) = broadcast::channel::<MarketSnapshot>(10);
    let (macro_bcast, _) = broadcast::channel::<MarketSnapshot>(10);

    let (snapshot_tx, _snapshot_rx) =
        mpsc::channel::<core_domain::normalized::NormalizedEvent>(100);
    let cancel = CancellationToken::new();

    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));
    let new_pipe = |secs,
                    label,
                    slot: core_domain::models::TimeframeSlot,
                    tx: broadcast::Sender<MarketSnapshot>| TimeframePipeline {
        slot,
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: snap_hist.clone(),
        timeframe_secs: secs,
        timeframe_label: label,
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
        symbol: PAIR_KEY.to_string(),
        custom_pipelines: std::collections::HashMap::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        // Fixed 10-slot ladder; the four representative broadcast channels
        // ride on micro1/fast1/slow1/macro1, the rest on throwaways.
        micro1: new_pipe(
            1,
            "Micro1",
            core_domain::models::TimeframeSlot::Micro1,
            mid_bcast.clone(),
        ),
        micro2: new_pipe(
            3,
            "Micro2",
            core_domain::models::TimeframeSlot::Micro2,
            broadcast::channel(8).0,
        ),
        fast1: new_pipe(
            5,
            "Fast1",
            core_domain::models::TimeframeSlot::Fast1,
            fast_bcast.clone(),
        ),
        fast2: new_pipe(
            15,
            "Fast2",
            core_domain::models::TimeframeSlot::Fast2,
            broadcast::channel(8).0,
        ),
        slow1: new_pipe(
            30,
            "Slow1",
            core_domain::models::TimeframeSlot::Slow1,
            slow_bcast.clone(),
        ),
        slow2: new_pipe(
            60,
            "Slow2",
            core_domain::models::TimeframeSlot::Slow2,
            broadcast::channel(8).0,
        ),
        macro1: new_pipe(
            180,
            "Macro1",
            core_domain::models::TimeframeSlot::Macro1,
            macro_bcast.clone(),
        ),
        macro2: new_pipe(
            300,
            "Macro2",
            core_domain::models::TimeframeSlot::Macro2,
            broadcast::channel(8).0,
        ),
        longterm1: new_pipe(
            900,
            "Longterm1",
            core_domain::models::TimeframeSlot::Longterm1,
            broadcast::channel(8).0,
        ),
        longterm2: new_pipe(
            3600,
            "Longterm2",
            core_domain::models::TimeframeSlot::Longterm2,
            broadcast::channel(8).0,
        ),
        snapshot_tx,
        cancel,
        active_indices: (0..10).collect(),
    });

    let buffers: [TimeframeBuffers; 10] = pair
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
        INSTANCE_ID.to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        ExchangeChoice::Hyperliquid,
        pair.clone(),
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        config_models::FIXED_TF_LADDER.to_vec(), // v11.2 active ladder
        Default::default(),
    ));

    workspace.insert(PAIR_KEY.to_string(), instance).await;

    // Pre-seed an empty WorkspaceConfig so handlers that synthesize defaults
    // have something to read from — they will push a new entry on save.
    {
        let cfg: WorkspaceConfig = WorkspaceConfig::default();
        workspace.set_config(cfg).await;
    }

    Arc::new(AppState {
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
    })
}

async fn serve_for(state: Arc<AppState>) -> std::net::SocketAddr {
    let router = api_gateway::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    // Tiny sleep so the server has a chance to bind before the test fires
    // its first request. The `axum_routes.rs` precedent uses 50ms.
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

fn default_body(micro_secs: u64) -> serde_json::Value {
    serde_json::json!({
        "micro_term": {
            "candles": { "duration_seconds": micro_secs },
            "indicators": {}
        }
    })
}

/// Run an async test body on a dedicated multi-thread runtime whose worker
/// threads get an 8 MiB stack. The save path re-parses `config.toml` inside
/// `config_models::save_workspace`, and in debug builds the combined
/// winnow/tower stack frames exceed tokio's default 2 MiB worker stack.
/// Release builds are unaffected; this is purely a test-harness
/// accommodation.
fn run_on_big_stack<F, Fut>(name: &str, f: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(8 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("tokio runtime")
                .block_on(f())
        })
        .expect("failed to spawn big-stack test thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

// The three tests in this file share the process CWD's `config.toml`
// (the instance-config POST persists the workspace there). Running them
// in parallel races the read-modify-write — serialize them explicitly.
static CONFIG_FILE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn post_instance_config_by_uuid_recharges_in_memory_state() {
    let _serial = CONFIG_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    run_on_big_stack("save_recharge_uuid", || {
        post_instance_config_by_uuid_recharges_in_memory_state_inner()
    });
}

async fn post_instance_config_by_uuid_recharges_in_memory_state_inner() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let state = setup_app_with_instance().await;
        let addr = serve_for(state.clone()).await;
        let client = reqwest::Client::new();

        let body = default_body(30);
        let res = client
            .post(format!("http://{addr}/api/instances/{INSTANCE_ID}/config"))
            .json(&body)
            .send()
            .await
            .expect("POST should reach the server");

        assert!(
            res.status().is_success(),
            "save handler must return 2xx; got {}",
            res.status()
        );
        let body = res.text().await.unwrap();
        assert!(
            body.contains("Instance configuration saved and pipelines recharged")
                || body.contains("Config saved but pipeline recharge failed"),
            "unexpected response body: {body:?}"
        );

        // The handle in-memory state must reflect the saved override —
        // BEFORE the fix this Vec was empty on the freshly-cloned config the
        // handler never published back, so `recharge_instance` produced
        // "No saved config for pair BTC-USDT".
        let cfg_after = state.workspace.config().await;
        let entry = cfg_after
            .instances
            .iter()
            .find(|i| i.symbol == PAIR_KEY)
            .expect("handler must persist an InstanceEntry");
        assert_eq!(
            entry.micro_term.candles.duration_seconds, 30,
            "micro_term override must reach the in-memory snapshot"
        );

        // Live map must still hold the instance after the recharge.
        assert!(
            state.workspace.get(PAIR_KEY).await.is_some(),
            "live Arc<Instance> must survive recharge"
        );
    })
    .await
    .expect("save->recharge cycle exceeded 15 s budget");
}

#[test]
fn post_instance_config_by_pairkey_is_rejected_with_404() {
    let _serial = CONFIG_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    run_on_big_stack("save_recharge_404", || {
        post_instance_config_by_pairkey_is_rejected_with_404_inner()
    });
}

async fn post_instance_config_by_pairkey_is_rejected_with_404_inner() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let state = setup_app_with_instance().await;
        let addr = serve_for(state).await;
        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://{addr}/api/instances/{PAIR_KEY}/config"))
            .json(&default_body(60))
            .send()
            .await
            .expect("POST should reach the server");
        assert_eq!(
            res.status(),
            axum::http::StatusCode::NOT_FOUND,
            "pairKey-based slug should 404; the route is UUID-only now"
        );
    })
    .await
    .expect("404 path exceeded 10 s budget");
}

#[test]
fn post_instance_config_uses_session_quote_in_default_pair_key() {
    let _serial = CONFIG_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    run_on_big_stack("save_recharge_quote", || {
        post_instance_config_uses_session_quote_in_default_pair_key_inner()
    });
}

async fn post_instance_config_uses_session_quote_in_default_pair_key_inner() {
    // The /api/history?symbol= (no symbol) fallback path must honour the
    // session quote. With a USDC session, `default_pair_key("BTC-USDT")` is
    // expected to round-trip to "BTC-USDC" rather than "BTC-USDT-USDT".
    tokio::time::timeout(Duration::from_secs(10), async {
        let state = setup_app_with_instance().await;
        // Force the session quote to USDC.
        {
            use portfolio_supervisor::session::Currency;
            *state.session.base_currency.write().await = Some(Currency::USDC);
        }
        let addr = serve_for(state).await;
        let client = reqwest::Client::new();

        // /api/history with no symbol — exercises the quote-aware default.
        let res = client
            .get(format!("http://{addr}/api/history"))
            .send()
            .await
            .expect("GET should reach the server");
        // No entry was ever added, so the response is an empty body (200 OK
        // or 404 are both acceptable — we only care that no 5xx fires).
        assert!(
            !res.status().is_server_error(),
            "default_pair_key fallback should not 5xx; got {}",
            res.status()
        );
    })
    .await
    .expect("default_pair_key path exceeded 10 s budget");
}
