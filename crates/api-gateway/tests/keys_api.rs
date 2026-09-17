// E2 — exchange-keys API integration test (registration, encryption,
// rotation round-trip, backup export).

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn build_state() -> Arc<AppState> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    database_storage::crypto::init_master_key("test-master-secret");

    Arc::new(AppState {
        workspace: portfolio_supervisor::workspace_state::WorkspaceState::empty(),
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
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
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
    })
}

#[tokio::test]
async fn keys_crud_encryption_rotation_and_backup() {
    let state = build_state().await;
    let router = api_gateway::build_router(state.clone());

    // Add a key (encrypted at rest).
    let add = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/keys")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "exchange": "Hyperliquid",
                        "account_name": "main",
                        "api_key": "0xabc123",
                        "api_secret": "supersecret",
                        "is_active": true,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::CREATED);

    // List keys — secret must NOT be echoed.
    let list = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/keys")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(list.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = json["keys"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["account_name"], "main");
    assert!(items[0].get("api_secret").is_none());
    assert!(items[0].get("api_key").is_none());

    // Rotation round-trip: decrypt with the new key must recover the secret.
    let rotate = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/keys/rotate")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "new_master_secret": "new-master-secret" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(rotate.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["rotated"], 1);

    // The stored secret decrypts under the NEW master key.
    let row: (String,) = sqlx::query_as("SELECT api_secret FROM exchange_keys LIMIT 1")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    let plain = database_storage::crypto::decrypt_field(&row.0).unwrap();
    assert_eq!(plain, "supersecret");

    // Backup export with a passphrase.
    let backup = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/keys/backup?passphrase=my-backup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(backup.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let enc = json["items"][0]["api_secret_encrypted"].as_str().unwrap();
    let backup_key = database_storage::crypto::backup_key_from_passphrase("my-backup");
    let plain_backup = database_storage::crypto::decrypt_with_key(enc, &backup_key).unwrap();
    assert_eq!(plain_backup, "supersecret");

    // Delete.
    let del = router
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/keys/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::OK);
}
