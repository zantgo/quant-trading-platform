// v8 BTE — archive backfill + coverage endpoint tests.
//
// Validation paths (depth bounds, instance-not-found / not-running,
// duplicate-job 409) are fully covered without network. The 200 path
// spawns the production pager as a detached task; the assertions only
// cover the endpoint contract (job id + live progress), never the
// network result.

use api_gateway::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use portfolio_supervisor::instance::{Instance, InstanceStatus};
use std::sync::atomic::AtomicBool;
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

    let instance = Arc::new(Instance::new_test(
        "inst_bte".to_string(),
        ("BTC".to_string(), "USDC".to_string()),
        portfolio_supervisor::instance::TimeframeBuffers::new(),
    ));

    let workspace = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace
        .insert("BTC-USDC".to_string(), instance.clone())
        .await;

    Arc::new(AppState {
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
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: tokio::sync::broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

async fn post_json(
    state: Arc<AppState>,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_000_000)
        .await
        .unwrap_or_default();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn get_json(state: Arc<AppState>, uri: &str) -> (StatusCode, serde_json::Value) {
    let router = api_gateway::build_router(state);
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_000_000)
        .await
        .unwrap_or_default();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn backfill_validates_depth_bounds() {
    let state = build_state().await;
    for bad in [0u32, 366u32] {
        let (status, json) = post_json(
            state.clone(),
            "/api/backtest/archive/backfill",
            serde_json::json!({ "instance_id": "inst_bte", "depth_days": bad }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "depth {bad} must be rejected"
        );
        assert_eq!(json["code"], "invalid_depth");
    }
}

#[tokio::test]
async fn backfill_requires_running_instance() {
    let state = build_state().await;

    // Unknown instance.
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/archive/backfill",
        serde_json::json!({ "instance_id": "nope", "depth_days": 7 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "instance_not_found");

    // Known instance, stopped.
    if let Some(inst) = state
        .workspace
        .list()
        .await
        .into_iter()
        .find(|i| i.id == "inst_bte")
    {
        inst.set_status(InstanceStatus::Stopped).await;
    }
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/archive/backfill",
        serde_json::json!({ "instance_id": "inst_bte", "depth_days": 7 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "instance_not_running");
}

#[tokio::test]
async fn backfill_rejects_duplicate_active_job() {
    let state = build_state().await;

    // Pre-register a running job in the in-memory registry (no network).
    let progress = Arc::new(tokio::sync::Mutex::new(
        backtesting_engine::backfill::BackfillProgress::new(
            1,
            "inst_bte".into(),
            "BTC-USDC".into(),
            "Hyperliquid".into(),
            7,
        ),
    ));
    {
        let mut map = state.backtest.backfills.write().await;
        map.insert(
            1,
            backtesting_engine::registry::TrackedBackfill {
                progress,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
    }

    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/archive/backfill",
        serde_json::json!({ "instance_id": "inst_bte", "depth_days": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["code"], "backfill_busy");
}

#[tokio::test]
async fn backfill_start_and_progress_contract() {
    let state = build_state().await;

    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/archive/backfill",
        serde_json::json!({ "instance_id": "inst_bte", "depth_days": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", json);
    let job_id = json["job_id"].as_i64().expect("job id");

    // Live progress endpoint answers while the detached job runs.
    let (pstatus, pjson) = get_json(
        state.clone(),
        &format!("/api/backtest/archive/progress/{job_id}"),
    )
    .await;
    assert_eq!(pstatus, StatusCode::OK);
    assert_eq!(pjson["job_id"], job_id);
    assert_eq!(pjson["instance_id"], "inst_bte");
    assert_eq!(pjson["depth_days"], 1);
    assert!(
        matches!(
            pjson["status"].as_str(),
            Some("running") | Some("done") | Some("failed")
        ),
        "unexpected status: {}",
        pjson["status"]
    );

    // Cancel endpoint answers.
    let (cstatus, _) = post_json(
        state.clone(),
        &format!("/api/backtest/archive/cancel/{job_id}"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(cstatus, StatusCode::OK);

    // Unknown job → 404.
    let (nstatus, _) = get_json(state.clone(), "/api/backtest/archive/progress/999999").await;
    assert_eq!(nstatus, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn coverage_returns_extended_shape() {
    let state = build_state().await;

    let (status, json) =
        get_json(state.clone(), "/api/backtest/coverage?instance_id=inst_bte").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["archive_depth_days"].as_i64().unwrap_or(0) >= 1);
    assert!(
        json["snapshots"].is_array(),
        "recorded-snapshot rows present"
    );
    assert!(json["archive"].is_array(), "archive rows present");
    assert!(json["backfill_jobs"].is_array(), "job list present");
    // v8.1: the data-prep contract — burn-in + the fixed 10-slot ladder.
    assert!(
        json["burn_in_secs"].as_i64().unwrap_or(0) > 0,
        "burn_in_secs present"
    );
    let ladder = json["ladder"].as_array().expect("ladder present");
    assert_eq!(ladder.len(), 10, "fixed 10-slot ladder");
    for (v, expected) in ladder.iter().zip(config_models::FIXED_TF_LADDER) {
        assert_eq!(v.as_u64(), Some(expected), "ladder entry {expected}s");
    }
}

#[tokio::test]
async fn run_is_rejected_while_another_run_holds_the_lock() {
    let state = build_state().await;

    // v8.2: the single-run lock is the registry's running-run status.
    let tracked = std::sync::Arc::new(backtesting_engine::registry::TrackedRun::new());
    state.backtest.runs.write().await.insert(1, tracked);
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "symbol": "BTC-USDC",
            "timeframe_secs": 60,
            "from_ms": 0,
            "to_ms": 10_000,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", json);
    assert_eq!(json["code"], "backtest_busy");
}

#[tokio::test]
async fn historical_mode_requires_instance_id() {
    let state = build_state().await;

    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "symbol": "BTC-USDC",
            "timeframe_secs": 60,
            "from_ms": 1_000_000,
            "to_ms": 1_010_000,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "instance_required");
}

#[tokio::test]
async fn recorded_run_persists_ds_rows() {
    let state = build_state().await;

    // Seed one recorded snapshot so the recorded path passes validation.
    let snap = core_domain::models::MarketSnapshot {
        symbol: "BTC-USDC".to_string(),
        timeframe_secs: 60,
        timestamp: 1_000_000,
        is_completed: Some(true),
        mid_price: rust_decimal::Decimal::from_f64_retain(100.0).unwrap_or_default(),
        bid_price: rust_decimal::Decimal::from_f64_retain(100.0).unwrap_or_default(),
        ask_price: rust_decimal::Decimal::from_f64_retain(100.0).unwrap_or_default(),
        close: Some(rust_decimal::Decimal::from_f64_retain(100.0).unwrap_or_default()),
        ..core_domain::models::MarketSnapshot::default()
    };
    database_storage::insert_snapshot_internal(&state.pool, &snap).await;

    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "symbol": "BTC-USDC",
            "timeframe_secs": 60,
            "from_ms": 1_000_000_000,
            "to_ms": 1_010_000_000,
            "mode": "recorded",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", json);
    // v8.2 async contract: poll progress until completed.
    let run_id = json["run_id"].as_i64().expect("run_id");
    assert_eq!(json["status"], "running");
    let mut backtest_id = None;
    for _ in 0..200 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (s, p) = get_json(state.clone(), &format!("/api/backtest/progress/{run_id}")).await;
        assert_eq!(s, StatusCode::OK, "{}", p);
        if p["status"].as_str() != Some("running") {
            assert_eq!(p["status"], "completed", "{}", p);
            backtest_id = p["backtest_id"].as_i64();
            break;
        }
    }
    let id = backtest_id.expect("persisted id");

    // DS read endpoints answer for the run.
    let (s, t) = get_json(state.clone(), &format!("/api/backtest/{id}/trades")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(t["trades"].is_array());
    let (s, e) = get_json(state.clone(), &format!("/api/backtest/{id}/equity")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(e["equity"].is_array());
    let (s, m) = get_json(state.clone(), &format!("/api/backtest/{id}/metrics")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(m["metrics"].is_array());
    // The run row carries the mode via the list endpoint.
    let (s, list) = get_json(state.clone(), "/api/backtest/list?limit=5").await;
    assert_eq!(s, StatusCode::OK);
    let first = &list[0];
    assert_eq!(first["mode"], "recorded");
}

// ─── v8.2 standalone runs + progress/cancel ───────────────────────────

async fn seed_archive_row(state: &Arc<AppState>, symbol: &str, tf: u64, ts_secs: u64, close: f64) {
    let candle = core_domain::normalized::NormalizedCandle {
        exchange: core_domain::normalized::Exchange::Hyperliquid,
        symbol: symbol.to_string(),
        start_time_ms: ts_secs * 1000,
        duration_ms: tf * 1000,
        open: rust_decimal::Decimal::from_f64_retain(close - 1.0).unwrap_or_default(),
        high: rust_decimal::Decimal::from_f64_retain(close + 2.0).unwrap_or_default(),
        low: rust_decimal::Decimal::from_f64_retain(close - 2.0).unwrap_or_default(),
        close: rust_decimal::Decimal::from_f64_retain(close).unwrap_or_default(),
        volume: rust_decimal_macros::dec!(100),
        trades_count: 5,
        reconstructed: Some(core_domain::normalized::ReconstructionMethod::ExchangeHistorical),
    };
    database_storage::queries::archive::upsert_archive_candles(&state.pool, &[candle], "backfill")
        .await;
}

#[tokio::test]
async fn standalone_run_requires_exchange() {
    let state = build_state().await;
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "symbols": [{ "symbol": "BTC-USDC", "timeframes": [60, 180, 300, 900] }],
            "from_ms": 1_000_000,
            "to_ms": 1_010_000,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "exchange_required");
}

#[tokio::test]
async fn standalone_allocation_sum_violation_returns_400() {
    let state = build_state().await;
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "exchange": "Hyperliquid",
            "symbols": [
                { "symbol": "BTC-USDC", "timeframes": [60, 180, 300, 900], "allocation_pct": 60.0 },
                { "symbol": "ETH-USDC", "timeframes": [60, 180, 300, 900], "allocation_pct": 60.0 }
            ],
            "from_ms": 1_000_000,
            "to_ms": 1_010_000,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "allocation_sum_exceeded");
}

#[tokio::test]
async fn standalone_invalid_ladders_rejected() {
    let state = build_state().await;
    // Sub-minute slot (archive floor).
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "exchange": "Hyperliquid",
            "symbols": [{ "symbol": "BTC-USDC", "timeframes": [15, 60, 300, 900] }],
            "from_ms": 1_000_000,
            "to_ms": 1_010_000,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_timeframes");
    // Non-ascending ladder.
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "exchange": "Hyperliquid",
            "symbols": [{ "symbol": "BTC-USDC", "timeframes": [900, 60, 300, 180] }],
            "from_ms": 1_000_000,
            "to_ms": 1_010_000,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_timeframes");
}

#[tokio::test]
async fn standalone_hyperliquid_ceiling_validation_400() {
    let state = build_state().await;
    // 4 days exceeds the 5,000-candle ceiling for the 60s TF (max ≈ 3.4d).
    let to_ms = 1_760_000_000_000i64;
    let from_ms = to_ms - 4 * 86400 * 1000;
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "exchange": "Hyperliquid",
            "symbols": [{ "symbol": "BTC-USDC", "timeframes": [60, 180, 300, 900] }],
            "from_ms": from_ms,
            "to_ms": to_ms,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "depth_exceeds_ceiling");
    assert!(json["error"].as_str().unwrap().contains("60s"));
}

#[tokio::test]
async fn standalone_coverage_reports_max_depth_ceiling() {
    let state = build_state().await;
    seed_archive_row(&state, "BTC-USDC", 60, 1_760_000_000, 100.0).await;
    seed_archive_row(&state, "BTC-USDC", 900, 1_760_000_000, 100.0).await;
    let (status, json) = get_json(
        state.clone(),
        "/api/backtest/coverage?symbol=BTC-USDC&exchange=Hyperliquid",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Hyperliquid: per-TF ceiling = 5,000 × tf.
    let rows = json["archive"].as_array().unwrap();
    let tf60 = rows
        .iter()
        .find(|r| r["timeframe_secs"] == 60)
        .expect("60s coverage row");
    assert_eq!(tf60["max_depth_secs"].as_i64(), Some(5000 * 60));
}

#[tokio::test]
async fn progress_and_cancel_404_for_unknown_runs() {
    let state = build_state().await;
    let (status, _) = get_json(state.clone(), "/api/backtest/progress/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_json(
        state.clone(),
        "/api/backtest/cancel/999999",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn standalone_run_completes_and_persists() {
    let state = build_state().await;
    // Small warmup keeps the debug-build replay fast (30 bars × 900s =
    // ~7.5h burn-in instead of the shipped 300 × 900s ≈ 3.1d).
    {
        let mut ws = state.workspace.config().await;
        ws.backtest.warmup_bars = 30;
        state.workspace.set_config(ws).await;
    }
    // Seed archive rows for ALL FOUR ladder TFs over the burn-in window
    // (30 bars × 900s ≈ 7.5h) plus a short scored window. The scored
    // window stays well inside the Hyperliquid 60s ceiling (5,000 × 60s
    // ≈ 3.47d incl. burn-in), so the scored window is the last 6 hours.
    let to_secs = 1_760_000_000u64;
    let burn_span = 86400u64;
    let mut candles = Vec::new();
    for tf in [180u64, 300, 900, 1800] {
        let mut ts = to_secs - burn_span;
        while ts <= to_secs {
            let close = 100.0 + (ts as f64 % 1000.0) * 0.001;
            candles.push(core_domain::normalized::NormalizedCandle {
                exchange: core_domain::normalized::Exchange::Hyperliquid,
                symbol: "BTC-USDC".to_string(),
                start_time_ms: ts * 1000,
                duration_ms: tf * 1000,
                open: rust_decimal::Decimal::from_f64_retain(close - 0.5).unwrap_or_default(),
                high: rust_decimal::Decimal::from_f64_retain(close + 0.5).unwrap_or_default(),
                low: rust_decimal::Decimal::from_f64_retain(close - 0.5).unwrap_or_default(),
                close: rust_decimal::Decimal::from_f64_retain(close).unwrap_or_default(),
                volume: rust_decimal_macros::dec!(100),
                trades_count: 5,
                reconstructed: Some(
                    core_domain::normalized::ReconstructionMethod::ExchangeHistorical,
                ),
            });
            ts += tf;
        }
    }
    database_storage::queries::archive::upsert_archive_candles(&state.pool, &candles, "backfill")
        .await;

    let from_secs = to_secs - 6 * 3600;
    let from_ms = (from_secs as i64) * 1000;
    let to_ms = (to_secs as i64) * 1000;
    let (status, json) = post_json(
        state.clone(),
        "/api/backtest/run",
        serde_json::json!({
            "exchange": "Hyperliquid",
            "symbols": [{ "symbol": "BTC-USDC", "timeframes": [180, 300, 900, 1800], "allocation_pct": 10.0 }],
            "from_ms": from_ms,
            "to_ms": to_ms,
            "portfolio_capital_usd": 1000.0,
            "mode": "historical",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", json);
    let run_id = json["run_id"].as_i64().expect("run_id");
    assert_eq!(json["status"], "running");

    // Poll progress until completion.
    let mut backtest_id = None;
    for _ in 0..1200 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (s, p) = get_json(state.clone(), &format!("/api/backtest/progress/{run_id}")).await;
        assert_eq!(s, StatusCode::OK, "{}", p);
        assert!(matches!(
            p["phase"].as_str().unwrap_or(""),
            "fetching" | "warming" | "replaying" | "analyzing" | "cancelled"
        ));
        if p["status"].as_str() != Some("running") {
            assert_eq!(p["status"], "completed", "{}", p);
            backtest_id = p["backtest_id"].as_i64();
            break;
        }
    }
    let id = backtest_id.expect("run completed with a persisted id");
    // The persisted run answers the DS read endpoints.
    let (s, t) = get_json(state.clone(), &format!("/api/backtest/{id}/equity")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(t["equity"].is_array());
    let _ = &seed_archive_row;
}
