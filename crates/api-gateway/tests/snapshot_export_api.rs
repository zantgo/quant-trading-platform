//! Tests for the snapshot-export HTTP handlers
//! (`GET /api/snapshot-export/status`,
//! `PUT /api/snapshot-export/config`,
//! `POST /api/snapshot-export/run-now`).
//!
//! These cover the API contract end-to-end — the handlers share a
//! single `Arc<RwLock<SnapshotExportRuntime>>` with the daemon's
//! periodic task, so the API surface is the same one the GUI and the
//! CLI both consume.

use api_gateway::{self, AppState};
use core_domain::snapshot_export::SnapshotExportRuntime;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::workspace_state::WorkspaceState;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower::ServiceExt;

async fn setup_test_state() -> Arc<AppState> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");

    let symbol_mapper = Arc::new(core_domain::normalized::SymbolMapper::new());
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(100);
    let ws_url = "ws://127.0.0.1:1".to_string();

    Arc::new(AppState {
        workspace: WorkspaceState::empty(),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        pool,
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
        snapshot_export: Arc::new(RwLock::new(SnapshotExportRuntime::default())),
        snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

async fn body_to_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .expect("body read");
    serde_json::from_slice(&bytes).expect("json parse")
}

#[tokio::test]
async fn status_returns_default_runtime_when_unconfigured() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let req = hyper::Request::builder()
        .method("GET")
        .uri("/api/snapshot-export/status")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "GET status should succeed");

    let json = body_to_json(resp).await;
    assert_eq!(json["enabled"], false);
    assert_eq!(json["interval_secs"], 60);
    assert_eq!(json["max_snapshots_retained"], 1000);
    assert_eq!(json["last_snapshot_at"], serde_json::Value::Null);
    assert_eq!(json["total_snapshots_written"], 0);
    let tabs = json["tabs"].as_array().unwrap();
    assert_eq!(tabs.len(), 9);
}

#[tokio::test]
async fn put_config_enables_scheduler_and_persists_to_runtime() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let body = serde_json::json!({
        "enabled": true,
        "output_path": "/tmp/snapshots",
        "interval_secs": 30,
        "max_snapshots_retained": 250,
    });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "PUT config should succeed");

    let json = body_to_json(resp).await;
    assert_eq!(json["enabled"], true);
    assert_eq!(json["output_path"], "/tmp/snapshots");
    assert_eq!(json["interval_secs"], 30);
    assert_eq!(json["max_snapshots_retained"], 250);
}

#[tokio::test]
async fn put_config_clamps_interval_to_allowed_range() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    // Interval too small → clamped to 5s (floor).
    let body = serde_json::json!({ "interval_secs": 1 });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_to_json(resp).await;
    assert_eq!(json["interval_secs"], 5);

    // Interval too big → clamped to 3600s (ceiling).
    let body = serde_json::json!({ "interval_secs": 100_000 });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let json = body_to_json(resp).await;
    assert_eq!(json["interval_secs"], 3600);
}

#[tokio::test]
async fn put_config_rejects_empty_output_path() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let body = serde_json::json!({ "output_path": "   " });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 400, "Empty path should 400");
}

#[tokio::test]
async fn put_config_filters_unknown_tabs_and_deduplicates() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let body = serde_json::json!({
        "tabs": ["alignment", "not_a_tab", "risk", "alignment"]
    });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let json = body_to_json(resp).await;
    let tabs: Vec<&str> = json["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(tabs, vec!["alignment", "risk"]);
}

#[tokio::test]
async fn put_config_empty_tabs_falls_back_to_all() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let body = serde_json::json!({ "tabs": [] });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let json = body_to_json(resp).await;
    let tabs = json["tabs"].as_array().unwrap();
    assert_eq!(tabs.len(), 9);
}

#[tokio::test]
async fn run_now_returns_acknowledgement() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state);

    let req = hyper::Request::builder()
        .method("POST")
        .uri("/api/snapshot-export/run-now")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let json = body_to_json(resp).await;
    assert_eq!(json["triggered"], true);
    assert!(json["path"].is_string());
}

#[tokio::test]
async fn status_reflects_put_config_changes() {
    let state = setup_test_state().await;
    let router = api_gateway::build_router(state.clone());

    // PUT first.
    let body = serde_json::json!({ "enabled": true, "interval_secs": 90 });
    let req = hyper::Request::builder()
        .method("PUT")
        .uri("/api/snapshot-export/config")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let _ = router.clone().oneshot(req).await.unwrap();

    // GET status.
    let req = hyper::Request::builder()
        .method("GET")
        .uri("/api/snapshot-export/status")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let json = body_to_json(resp).await;
    assert_eq!(json["enabled"], true);
    assert_eq!(json["interval_secs"], 90);
}
