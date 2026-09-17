//! Integration tests for the `/api/liquidity/cluster-status` endpoint.
//!
//! Verifies:
//!   - default response (no filters) returns an empty array when no
//!     pairs are configured
//!   - 400 on invalid slot query param
//!   - 404 on unknown symbol query param
//!   - single-symbol response includes all 10 fixed-ladder TF slots
//!   - single-(symbol, slot) response shape
//!   - Stale status derivation: a successful refresh whose TTL elapsed
//!     surfaces as Stale (not Ok)
//!
//! Note: the pipeline status handle is shared between the cluster
//! refresh task (which writes it) and this handler (which reads it),
//! so we can simulate a successful refresh by writing directly into
//! `pipe.cluster_status`.

use api_gateway::{self, AppState};
use config_models::PlatformConfig;
use core_domain::liquidity::{ClusterRefreshStatus, ClusterStatusSnapshot};
use core_domain::normalized::SymbolMapper;
use market_analyzer::analyzer::{ActivePair, TimeframePipeline};
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::instance::TimeframeBuffers;
use portfolio_supervisor::session::ExchangeChoice;
use portfolio_supervisor::workspace_state::WorkspaceState;
use rust_decimal::Decimal;
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower::ServiceExt;

async fn setup_test_state() -> (Arc<AppState>, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");
    let symbol_mapper = Arc::new(SymbolMapper::new());
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);
    let ws_url = "ws://127.0.0.1:1".to_string();
    let state = Arc::new(AppState {
        workspace: portfolio_supervisor::workspace_state::WorkspaceState::empty(),
        platform: Arc::new(RwLock::new(PlatformConfig::default())),
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

fn make_pipe(secs: u64) -> TimeframePipeline {
    let (bcast_tx, _) = broadcast::channel::<core_domain::models::MarketSnapshot>(8);
    TimeframePipeline {
        slot_label: core_domain::duration_label(secs),
        history: Arc::new(RwLock::new(VecDeque::new())),
        broadcast_tx: bcast_tx,
        latest_snapshot: Arc::new(RwLock::new(None)),
        snapshot_history: Arc::new(RwLock::new(VecDeque::new())),
        timeframe_secs: secs,
        divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
        sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
        fibonacci: config_models::FibonacciConfig::default(),
        latest_oi: Arc::new(RwLock::new(Some(Decimal::from(1_000_000)))),
        latest_funding: Arc::new(RwLock::new(Some(
            Decimal::from_f64_retain(0.0001).unwrap_or_default(),
        ))),
        latest_mark_px: Arc::new(RwLock::new(Some(Decimal::from(50_000)))),
        latest_index_px: Arc::new(RwLock::new(Some(Decimal::from(50_000)))),
        active_set: Default::default(),
        cluster_matrix: Arc::new(RwLock::new(None)),
        cluster_status: Arc::new(RwLock::new(ClusterStatusSnapshot::pending(
            "BTC-USDC",
            &core_domain::duration_label(secs),
        ))),
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

async fn register_btc_usdc(state: &Arc<AppState>) {
    let (snapshot_tx, _snapshot_rx) = mpsc::channel(8);
    let active_pair = Arc::new(ActivePair {
        symbol: "BTC-USDC".to_string(),
        pipelines: vec![
            make_pipe(1),
            make_pipe(3),
            make_pipe(5),
            make_pipe(15),
            make_pipe(30),
            make_pipe(60),
            make_pipe(180),
            make_pipe(300),
            make_pipe(900),
            make_pipe(3600),
        ],
        active_secs: config_models::SUPPORTED_DURATIONS.to_vec(),
        snapshot_tx,
        cancel: tokio_util::sync::CancellationToken::new(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))),
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
    });

    let buffers = TimeframeBuffers {
        history: Arc::new(RwLock::new(VecDeque::new())),
        latest: Arc::new(RwLock::new(None)),
        snapshot_history: Arc::new(RwLock::new(VecDeque::new())),
    };
    let instance = Arc::new(portfolio_supervisor::instance::Instance::new(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDC".to_string()),
        ExchangeChoice::Hyperliquid,
        active_pair,
        state.pool.clone(),
        state.workspace.clone(),
        config_models::IntervalsConfig::default(),
        config_models::SafetyConfig::default(),
        std::iter::repeat(buffers)
            .take(config_models::SUPPORTED_DURATIONS.len())
            .collect(),
        config_models::SUPPORTED_DURATIONS.to_vec(), // v11.9 active ladder
        config_models::OperationalMode::Advisory,
    ));
    state
        .workspace
        .insert("BTC-USDC".to_string(), instance)
        .await;
}

#[tokio::test]
async fn cluster_status_no_filter_returns_empty_array_when_no_pairs() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn cluster_status_invalid_slot_returns_400() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=invalid")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("invalid slot"));
}

#[tokio::test]
async fn cluster_status_unknown_symbol_returns_404() {
    let (state, _pool) = setup_test_state().await;
    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=UNKNOWN-X")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cluster_status_by_symbol_returns_all_10_slots() {
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;
    let router = api_gateway::build_router(state);

    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 16_384).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["symbol"], "BTC-USDC");
    let slots = parsed["slots"].as_object().unwrap();
    for name in [
        "1s", "3s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "1h",
    ] {
        assert!(slots.contains_key(name), "missing slot key {name}");
    }
    // Default state is Pending.
    assert_eq!(slots["1s"]["status"], "PENDING");
}

#[tokio::test]
async fn cluster_status_single_slot_returns_flat_snapshot() {
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;
    let router = api_gateway::build_router(state);

    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=1s")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 8192).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["symbol"], "BTC-USDC");
    assert_eq!(parsed["timeframe_label"], "1s");
    assert_eq!(parsed["status"], "PENDING");
}

#[tokio::test]
async fn cluster_status_derives_stale_from_expired_ttl() {
    // Write a successful refresh with a TTL that's already in the past,
    // then verify the handler surfaces it as `Stale` (not `Ok`).
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;

    // Reach into the active pair's micro pipeline and overwrite the
    // cluster_status with a fake "successful but expired" snapshot.
    let pair = state.workspace.get("BTC-USDC").await.unwrap();
    let micro_pipe = pair.active_pair.pipeline_for_secs(1);
    {
        let mut guard = micro_pipe
            .as_ref()
            .expect("fastest active pipeline must be present")
            .cluster_status
            .write()
            .await;
        guard.status = ClusterRefreshStatus::Ok;
        guard.last_success_ms = Some(1_000);
        guard.last_skip_reason = None;
        guard.cluster_count_short = 5;
        guard.cluster_count_long = 3;
        guard.mid_price = 50_000.0;
        guard.ttl_remaining_ms = -60_000; // expired 1 min ago
    }
    let _ = micro_pipe;

    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=1s")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 8192).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Despite the raw handle saying Ok, the handler must derive Stale.
    assert_eq!(parsed["status"], "STALE");
    assert_eq!(parsed["cluster_count_short"], 5);
    assert_eq!(parsed["cluster_count_long"], 3);
}

#[tokio::test]
async fn cluster_status_keeps_ok_for_fresh_ttl() {
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;

    let pair = state.workspace.get("BTC-USDC").await.unwrap();
    let micro_pipe = pair.active_pair.pipeline_for_secs(1);
    {
        let mut guard = micro_pipe
            .as_ref()
            .expect("fastest active pipeline must be present")
            .cluster_status
            .write()
            .await;
        guard.status = ClusterRefreshStatus::Ok;
        guard.last_success_ms = Some(1_000);
        guard.last_skip_reason = None;
        guard.ttl_remaining_ms = 60_000; // 1 min remaining
        guard.mid_price = 50_000.0;
    }
    let _ = micro_pipe;

    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=1s")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 8192).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["status"], "OK");
}

#[tokio::test]
async fn cluster_status_preserves_skip_reason_in_payload() {
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;

    let pair = state.workspace.get("BTC-USDC").await.unwrap();
    let micro_pipe = pair.active_pair.pipeline_for_secs(1);
    let expected_reason =
        "no open_interest yet (HL derivatives poller hasn't populated this symbol)";
    {
        let mut guard = micro_pipe
            .as_ref()
            .expect("fastest active pipeline must be present")
            .cluster_status
            .write()
            .await;
        guard.status = ClusterRefreshStatus::Skipped;
        guard.last_skip_reason = Some(expected_reason.to_string());
    }
    let _ = micro_pipe;

    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=1s")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 8192).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["status"], "SKIPPED");
    assert_eq!(parsed["last_skip_reason"], expected_reason);
}

/// Regression: the cluster-refresh skip reason is templated on the active
/// exchange (v6.6). HL and Bitget produce different messages, and the API
/// payload must preserve whichever one was set on the snapshot.
#[tokio::test]
async fn cluster_status_preserves_bitget_skip_reason_in_payload() {
    let (state, _pool) = setup_test_state().await;
    register_btc_usdc(&state).await;

    let pair = state.workspace.get("BTC-USDC").await.unwrap();
    let micro_pipe = pair.active_pair.pipeline_for_secs(1);
    let expected_reason =
        "no open_interest yet (Bitget ticker channel hasn't delivered holdingAmount)";
    {
        let mut guard = micro_pipe
            .as_ref()
            .expect("fastest active pipeline must be present")
            .cluster_status
            .write()
            .await;
        guard.status = ClusterRefreshStatus::Skipped;
        guard.last_skip_reason = Some(expected_reason.to_string());
    }
    let _ = micro_pipe;

    let router = api_gateway::build_router(state);
    let res = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/liquidity/cluster-status?symbol=BTC-USDC&slot=1s")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 8192).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["status"], "SKIPPED");
    assert_eq!(parsed["last_skip_reason"], expected_reason);
}

// suppress unused-import lint from `WorkspaceState` in dev imports
#[allow(dead_code)]
fn _ensure_compile(_ws: &WorkspaceState) {}
