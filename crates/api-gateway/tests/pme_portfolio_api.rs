// PME v7 — informational portfolio API integration test.
//
// Exercises the read-only surface: rich /portfolio (equity, exposure,
// capital, positions, safety), /exposure, /capital, extended /safety,
// and the informational /safety/session-reset. Nothing here executes
// trades through the daemon; the harness builds an Instance via
// `Instance::new_test` and drives the unified execution engine directly.

use api_gateway::AppState;
use config_models::OrderSide;
use portfolio_supervisor::paper_trading::FeesConfig;
use portfolio_supervisor::{execution::ExecutionEngine, instance::Instance};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let body = resp.into_body();
    let bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("valid json")
}

async fn build_state() -> (Arc<AppState>, Arc<Instance>, Arc<ExecutionEngine>) {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy memory pool");

    let instance = Arc::new(Instance::new_test(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::instance::TimeframeBuffers::new(),
    ));
    // A live mid for mark-to-market + fills.
    let snap = core_domain::models::MarketSnapshot {
        symbol: "BTC-USDT".to_string(),
        mid_price: dec!(110),
        bid_price: dec!(110),
        ask_price: dec!(110),
        close: Some(dec!(110)),
        ..core_domain::models::MarketSnapshot::default()
    };
    *instance.micro1.latest.write().await = Some(snap);

    let workspace = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace
        .insert("BTC-USDT".to_string(), instance.clone())
        .await;

    let engine = Arc::new(ExecutionEngine::new(FeesConfig::default()));
    engine.set_initial_equity(dec!(1000)).await;

    // Open a 1-unit long at 100 (fills at mid+spread ≈ 100.005).
    engine
        .submit_order(
            config_models::OrderPacket {
                client_order_id: "t1".to_string(),
                symbol: "BTC-USDT".to_string(),
                side: OrderSide::Buy,
                order_type: config_models::OrderType::Market,
                price: None,
                size: dec!(1),
                reduce_only: false,
                is_emergency_liquidation: false,
                associated_position_id: None,
                metadata: Default::default(),
            },
            dec!(100),
        )
        .await
        .expect("open position");

    let state = Arc::new(AppState {
        workspace,
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        pool,
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: tokio::sync::mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: "".to_string(),
        clock_monitor: None,
        reliability: Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
        exchange_status: Arc::new(
            network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
        ),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: engine.clone(),
        recharge_tx: tokio::sync::broadcast::channel::<api_gateway::RechargeNotice>(64).0,
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

    (state, instance, engine)
}

async fn get(state: Arc<AppState>, path: &str) -> (StatusCode, serde_json::Value) {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = json_body(resp).await;
    (status, body)
}

#[tokio::test]
async fn portfolio_returns_rich_state_with_position() {
    let (state, _inst, _engine) = build_state().await;
    let (status, body) = get(state, "/api/instances/inst_test/portfolio").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["symbol"], "BTC-USDT");
    assert!(body["current_equity"].as_str().is_some());
    assert!(body["peak_equity"].as_str().is_some());
    assert!(body["max_drawdown_pct"].as_str().is_some());
    assert_eq!(body["safety_state"], "NORMAL");
    assert_eq!(body["position_count"], 1);
    assert_eq!(body["positions"][0]["direction"], "LONG");
    // Mark price 110 vs entry ~100.005 → positive uPnL.
    let upnl: f64 = body["positions"][0]["unrealized_pnl"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(upnl > 9.0);
    // Exposure block present with gross exposure ≈ 100.
    assert!(body["exposure"]["gross_exposure"].as_str().is_some());
    assert!(body["capital"]["available_margin"].as_str().is_some());
}

#[tokio::test]
async fn exposure_and_capital_endpoints_serve_matrices() {
    let (state, _inst, _engine) = build_state().await;

    let (s1, exp) = get(state.clone(), "/api/instances/inst_test/exposure").await;
    assert_eq!(s1, StatusCode::OK);
    assert!(exp["gross_exposure"].as_str().is_some());
    assert!(exp["net_exposure"].as_str().is_some());

    let (s2, cap) = get(state, "/api/instances/inst_test/capital").await;
    assert_eq!(s2, StatusCode::OK);
    assert!(cap["available_margin"].as_str().is_some());
    assert!(cap["margin_usage_ratio"].as_str().is_some());
}

#[tokio::test]
async fn safety_endpoint_reports_extended_fields() {
    let (state, inst, _engine) = build_state().await;
    inst.safety.set_portfolio_capital(dec!(1000)).await;
    inst.safety.update(dec!(1000)).await;

    let (status, body) = get(state, "/api/instances/inst_test/safety").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["safety_state"], "NORMAL");
    assert!(body["max_drawdown_pct"].as_str().is_some());
    assert!(body["daily_pnl"].as_str().is_some());
    assert!(body["margin_usage_ratio"].as_str().is_some());
}

#[tokio::test]
async fn session_reset_rebaselines_informational_state() {
    let (state, inst, _engine) = build_state().await;
    inst.safety.set_portfolio_capital(dec!(1000)).await;
    inst.safety.update(dec!(1200)).await;
    inst.safety.update(dec!(1000)).await;

    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/instances/inst_test/safety/session-reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let peak = *inst.safety.peak_equity.read().await;
    let session = *inst.safety.starting_session_equity.read().await;
    let daily = *inst.safety.daily_pnl.read().await;
    assert_eq!(peak, dec!(1000));
    assert_eq!(session, dec!(1000));
    assert_eq!(daily, dec!(0));
}

#[tokio::test]
async fn portfolio_404_for_unknown_instance() {
    let (state, _inst, _engine) = build_state().await;
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/instances/does_not_exist/portfolio")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
