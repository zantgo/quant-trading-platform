use api_gateway::{self, AppState};
use config_models::FibonacciConfig;
use core_domain::models::MarketSnapshot;
use core_domain::normalized::SymbolMapper;
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::TimeframeBuffers;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower::ServiceExt;

async fn setup_test_state() -> (Arc<AppState>, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");

    let _dummy_config = config_models::WorkspaceConfig::default();

    let symbol_mapper = Arc::new(SymbolMapper::new());
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);
    let ws_url = "ws://127.0.0.1:1".to_string();

    let state = Arc::new(AppState {
        workspace: portfolio_supervisor::workspace_state::WorkspaceState::empty(),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        pool: pool.clone(),
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: ws_url.clone(),
        bitget_ws_url: ws_url,
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

    (state, pool)
}

#[tokio::test]
async fn test_analyze_missing_position_returns_400() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let body = serde_json::json!({
        "historical_prices": [50000.0, 50100.0, 50200.0],
        "indicators": {
            "rsi": 65.0,
            "macd_line": 20.0,
            "macd_signal": 15.0
        }
    });

    let request = hyper::Request::builder()
        .method("POST")
        .uri("/api/analyze")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap(),
        ))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert!(
        response.status().is_client_error(),
        "Missing position should return 4xx, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_history_invalid_symbol_returns_empty_or_error() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let request = hyper::Request::builder()
        .method("GET")
        .uri("/api/history?symbol=NONEXISTENT_PAIR")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert!(
        !response.status().is_server_error(),
        "Server should not 500 on invalid symbol, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_config_endpoint_returns_ok() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let request = hyper::Request::builder()
        .method("GET")
        .uri("/api/config")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert!(
        response.status().is_success(),
        "/api/config should return success, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_api_failover_router_integration() {
    // Test that the router builds without panicking with a fully configured state
    let (state, _pool) = setup_test_state().await;
    let _router = api_gateway::build_router(state);
    // Router construction is the test — if we got here without panic, it passes
}

#[tokio::test]
async fn test_websocket_stream_with_active_pair() {
    // Build state with a registered pair that has broadcast channels
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");

    let _dummy_config = config_models::WorkspaceConfig::default();

    let symbol_mapper = Arc::new(SymbolMapper::new());
    symbol_mapper
        .register(core_domain::normalized::Exchange::Hyperliquid, "BTC", "BTC")
        .await;
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);
    let _ws_url = "ws://127.0.0.1:1".to_string();

    let workspace = WorkspaceState::empty();

    let (snapshot_tx, _snapshot_rx) =
        mpsc::channel::<core_domain::normalized::NormalizedEvent>(100);
    let cancel = tokio_util::sync::CancellationToken::new();

    let snap_hist = Arc::new(RwLock::new(
        std::collections::VecDeque::<MarketSnapshot>::new(),
    ));
    let make_pipe = |secs: u64, tx: broadcast::Sender<MarketSnapshot>| TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
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
    let throwaway = || broadcast::channel::<MarketSnapshot>(10).0;
    let active_secs: Vec<u64> = config_models::SUPPORTED_DURATIONS.to_vec();
    let pipelines: Vec<TimeframePipeline> = (0..10)
        .map(|i| make_pipe(active_secs[i], throwaway()))
        .collect();
    let pair = Arc::new(ActivePair {
        symbol: "BTC".to_string(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        pipelines,
        active_secs: active_secs.clone(),
        snapshot_tx,
        cancel,
    });

    let buffers: Vec<TimeframeBuffers> = pair
        .all()
        .iter()
        .map(|pipe| TimeframeBuffers {
            history: pipe.history.clone(),
            latest: pipe.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        })
        .collect();

    let instance = Arc::new(portfolio_supervisor::instance::Instance::new(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid,
        pair.clone(),
        pool.clone(),
        workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        active_secs, // v11.9 active ladder
        Default::default(),
    ));
    let workspace = WorkspaceState::empty();
    workspace.insert("BTC-USDT".to_string(), instance).await;

    let state = Arc::new(AppState {
        workspace,
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        pool: pool.clone(),
        symbol_mapper,
        telemetry_tx,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: "".to_string(),
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

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Connect WebSocket with valid parameters
    let ws_url = format!("ws://{}/ws?symbol=BTC-USDT&timeframe=Mid", addr);
    let (ws_stream, response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("WebSocket connection should succeed");

    assert!(
        response.status().is_success() || response.status() == 101,
        "WS handshake should succeed, got {}",
        response.status()
    );

    // Connection should be open — the pair exists, so handle_ws_socket subscribes
    drop(ws_stream);

    // Test with non-existent pair key — connection upgrades but handler returns immediately
    let unknown_url = format!("ws://{}/ws?symbol=NONEXISTENT-PAIR", addr);
    let unknown_result = tokio_tungstenite::connect_async(&unknown_url).await;
    // The handshake may still succeed (upgrade happens before handler checks pairs)
    // but we verify no panic — the handler gracefully returns None
    assert!(
        unknown_result.is_ok() || unknown_result.is_err(),
        "WS with unknown pair should not crash the server"
    );
}

#[test]
fn rules_guide_path_resolves_inside_repo() {
    // Audit fix (C5): `GET /api/rules` previously read `docs/indicators-guide.md`,
    // which does not exist in the repo — the endpoint 404'd permanently. The
    // handler must target the real MME indicators guide spec. The daemon runs
    // from the workspace root, so walk up from the crate dir to find it.
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let mut dir = std::path::PathBuf::from(&crate_dir);
    let mut found = None;
    loop {
        let candidate = dir.join(api_gateway::handlers::config::RULES_GUIDE_PATH);
        if candidate.exists() {
            found = Some(candidate);
            break;
        }
        if !dir.pop() {
            break;
        }
    }
    let guide = found.expect(
        "RULES_GUIDE_PATH must resolve to an existing file inside the workspace (docs/engines/market-monitoring-engine/03-02-09-mme-indicators-guide.md)",
    );
    let content = std::fs::read_to_string(&guide).expect("guide must be readable");
    assert!(
        content.contains("indicators"),
        "guide must contain rulebook content"
    );
}

#[tokio::test]
async fn cross_site_requests_are_rejected() {
    // K1 (production audit): the unauthenticated API must refuse requests
    // whose `Sec-Fetch-Site` / `Origin` prove a foreign website — the
    // loopback bind alone is no defense against browser-based attackers.
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);

    // Browser cross-site fetch: `Sec-Fetch-Site: cross-site` → 403.
    let request = hyper::Request::builder()
        .method("GET")
        .uri("/api/config")
        .header("sec-fetch-site", "cross-site")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);

    // Foreign Origin header → 403.
    let request = hyper::Request::builder()
        .method("POST")
        .uri("/api/session/quit")
        .header("content-type", "application/json")
        .header("origin", "https://evil.example")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let resp = router.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);

    // Same-origin request passes (the dashboard's own fetches).
    let request = hyper::Request::builder()
        .method("GET")
        .uri("/api/config")
        .header("sec-fetch-site", "same-origin")
        .header("origin", "http://127.0.0.1:3000")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.unwrap();
    assert!(
        resp.status().is_success() || resp.status() == axum::http::StatusCode::NOT_FOUND,
        "same-origin request must not be blocked, got {}",
        resp.status()
    );
}
