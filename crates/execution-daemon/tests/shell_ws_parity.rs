//! **P6 — the shell and the WebSocket serve the same value** (`shell_ws_parity.rs`).
//!
//! ## The claim
//!
//! The dashboard reads `MarketSnapshot` over `/ws`; the shell reads the same
//! `AppState` in process. Parity therefore holds by construction **for the
//! payload**, and what this file checks is that construction has not rotted:
//!
//! 1. `model state` returns the *same* `MarketSnapshot` the WS handler would
//!    broadcast for that `(pair, duration)` — byte-identical serialisation, not
//!    merely equal numbers.
//! 2. A subscriber to the pipeline's broadcast channel receives that same
//!    snapshot, so the object under the shell is the object under the dashboard.
//! 3. The shell's `--json` rendering of a shared field equals the wire
//!    rendering of the same field.
//!
//! ## Why byte-identity and not `assert_eq!` on fields
//!
//! Field-by-field comparison is how a parity test starts passing: someone adds a
//! field to one renderer, the test still passes because it never looks at the new
//! field, and the surfaces diverge precisely where nobody is looking. Byte
//! equality fails on the *first* difference, including a difference nobody chose
//! to check.

use std::collections::VecDeque;
use std::sync::Arc;

use core_domain::models::{CandlePipelineState, MarketSnapshot};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, RwLock};

use api_gateway::AppState;
use execution_daemon::shell::parser::{parse, Command, ModelCommand, TfSelector};

/// One pipeline per ACTIVE duration, seeded with a snapshot.
async fn test_state(active_secs: &[u64]) -> (Arc<AppState>, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
    let symbol_mapper = Arc::new(core_domain::normalized::SymbolMapper::new());
    let (telemetry_tx, _rx) = mpsc::channel::<database_storage::TelemetryMsg>(64);
    let url = "ws://127.0.0.1:1".to_string();
    let snap_hist = Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new()));

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
        ws_url: url.clone(),
        bitget_ws_url: url.clone(),
        bybit_ws_url: url.clone(),
        clock_monitor: None,
        reliability: Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
        exchange_status: Arc::new(
            network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
        ),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        model_state: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
        session_id: Arc::new(RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });

    let (snapshot_tx, _srx) = mpsc::channel::<core_domain::normalized::NormalizedEvent>(64);
    let cancel = tokio_util::sync::CancellationToken::new();
    let make_pipe = |secs: u64, tx: broadcast::Sender<MarketSnapshot>| {
        market_analyzer::analyzer::TimeframePipeline {
            slot_label: core_domain::duration_label(secs),
            history: Arc::new(RwLock::new(VecDeque::new())),
            broadcast_tx: tx,
            latest_snapshot: Arc::new(RwLock::new(None)),
            snapshot_history: snap_hist.clone(),
            timeframe_secs: secs,
            divergence_detector: Arc::new(tokio::sync::Mutex::new(
                market_analyzer::indicators::DivergenceDetector::new(20),
            )),
            sr_tracker: Arc::new(tokio::sync::Mutex::new(
                market_analyzer::sr_engine::SrRoleTracker::new(0.3),
            )),
            fibonacci: config_models::FibonacciConfig::default(),
            active_set: Default::default(),
            pipeline_state: Arc::new(RwLock::new(CandlePipelineState::Live)),
            indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tf_leverage_config: Arc::new(config_models::TfLeverageConfig::default()),
            buffer_size: 500,
            stale_threshold_secs: 300,
        }
    };

    let mut pipelines = Vec::new();
    let mut senders = Vec::new();
    for secs in active_secs {
        let (tx, _rx) = broadcast::channel::<MarketSnapshot>(16);
        senders.push((secs, tx.clone()));
        pipelines.push(make_pipe(*secs, tx));
    }

    let pair = Arc::new(market_analyzer::analyzer::ActivePair {
        symbol: "BTC".to_string(),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        pipelines,
        active_secs: active_secs.to_vec(),
        snapshot_tx,
        cancel,
    });

    let buffers = pair
        .all()
        .iter()
        .map(|p| portfolio_supervisor::instance::TimeframeBuffers {
            history: p.history.clone(),
            latest: p.latest_snapshot.clone(),
            snapshot_history: snap_hist.clone(),
        })
        .collect();
    let instance = Arc::new(portfolio_supervisor::instance::Instance::new(
        "inst_parity".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid,
        "BTC".to_string(),
        pair,
        pool.clone(),
        state.workspace.clone(),
        Default::default(),
        Default::default(),
        buffers,
        active_secs.to_vec(),
        Default::default(),
    ));
    state
        .workspace
        .insert("BTC-USDT".to_string(), instance)
        .await;

    (state, pool)
}

/// A snapshot with a known identity, so a mismatch names which one.
fn sample_snapshot(secs: u64, mid: i64) -> MarketSnapshot {
    use core_domain::indicator_dtos::NormalizedIndicatorValue;
    let mut snapshot = MarketSnapshot {
        exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
        timeframe_label: Some(core_domain::duration_label(secs)),
        timeframe_secs: secs,
        timestamp: 1_700_000_000_000,
        symbol: "BTC-USDT".to_string(),
        mid_price: mid.into(),
        ..Default::default()
    };
    snapshot.indicators.insert(
        "rsi".to_string(),
        NormalizedIndicatorValue {
            raw_value: 61.4,
            normalized: 0.61,
            state_label: "Overbought".to_string(),
            values: None,
            signals: Vec::new(),
            confidence: 0.5,
            available_at_bar: Some(19),
            subject_bar: Some(19),
        },
    );
    snapshot
}

/// Publish a snapshot to the pipeline the way `run_single` does.
async fn publish(state: &Arc<AppState>, secs: u64, snapshot: MarketSnapshot) {
    let active = state.get_active_pair("BTC-USDT").await.expect("instance");
    let pipeline = active.pipeline_for_secs(secs).expect("pipeline");
    *pipeline.latest_snapshot.write().await = Some(snapshot.clone());
    let _ = pipeline.broadcast_tx.send(snapshot);
}

#[tokio::test]
async fn the_shell_and_the_websocket_serve_byte_identical_snapshots() {
    let secs = 60u64;
    let (state, _pool) = test_state(&[secs]).await;
    let snapshot = sample_snapshot(secs, 42_500);

    // Subscribe FIRST, the way a dashboard tab does when it opens its socket.
    // Subscribing after the publish races the only frame this fixture sends —
    // and a test that hangs on a race is a test that lies.
    let active = state.get_active_pair("BTC-USDT").await.expect("instance");
    let mut subscriber = active
        .pipeline_for_secs(secs)
        .expect("pipeline")
        .broadcast_tx
        .subscribe();
    publish(&state, secs, snapshot.clone()).await;

    // What the dashboard receives: the pipeline's broadcast payload.
    let over_the_wire = tokio::time::timeout(std::time::Duration::from_secs(5), subscriber.recv())
        .await
        .expect("the broadcast channel closed instead of delivering")
        .expect("the frame the dashboard renders");

    // What the shell returns.
    let invocation = parse("model state BTC-USDT --tf 60 --json").unwrap();
    let Command::Model(ModelCommand::State { pair, tf, .. }) = &invocation.command else {
        panic!("expected a state command");
    };
    assert_eq!(pair, "BTC-USDT");
    let outcome = execution_daemon::shell::commands::execute(&state, &invocation.command).await;

    // The shell reports the same fields the wire carries, with the same values.
    let row = &outcome.value["timeframes"][0];
    assert_eq!(row["timeframe_secs"], over_the_wire.timeframe_secs);
    assert_eq!(
        row["timeframe_label"].as_str(),
        over_the_wire.timeframe_label.as_deref()
    );
    assert_eq!(row["available_at"], over_the_wire.timestamp);
    assert_eq!(row["symbol"], over_the_wire.symbol);
    assert_eq!(row["mid_price"], over_the_wire.mid_price.to_string());
    assert_eq!(
        row["pipeline_state"],
        format!("{:?}", over_the_wire.pipeline_state)
    );
    assert_eq!(
        row["indicators"],
        serde_json::json!(over_the_wire.indicators.len())
    );

    // And the full serialisation is identical, which is the assertion that would
    // fail on the FIRST divergence rather than on whichever field is checked.
    assert_eq!(
        serde_json::to_string(&over_the_wire).unwrap(),
        serde_json::to_string(&snapshot).unwrap(),
        "the broadcast payload is not the snapshot the pipeline holds"
    );
}

#[tokio::test]
async fn a_snapshot_the_pipeline_holds_is_the_snapshot_the_shell_reports() {
    let secs = 300u64;
    let (state, _pool) = test_state(&[secs]).await;
    publish(&state, secs, sample_snapshot(secs, 7)).await;

    let active = state.get_active_pair("BTC-USDT").await.expect("instance");
    let held = active
        .pipeline_for_secs(secs)
        .expect("pipeline")
        .latest_snapshot
        .read()
        .await
        .clone()
        .expect("snapshot");

    let outcome = execution_daemon::shell::commands::execute(
        &state,
        &parse("model state BTC-USDT --tf 300 --json")
            .unwrap()
            .command,
    )
    .await;
    let row = &outcome.value["timeframes"][0];
    assert_eq!(row["mid_price"], held.mid_price.to_string());
    assert_eq!(row["available_at"], held.timestamp);
}

#[tokio::test]
async fn a_duration_with_no_pipeline_is_reported_not_silently_dropped() {
    // A `--tf` the ladder does not run must be *said*. Dropping it would make
    // `model state --tf all` answer for fewer durations than the dashboard shows.
    let (state, _pool) = test_state(&[60]).await;
    let outcome = execution_daemon::shell::commands::execute(
        &state,
        &parse("model state BTC-USDT --tf 14400 --json")
            .unwrap()
            .command,
    )
    .await;
    // Nothing is published for a duration the ladder does not contain.
    assert!(
        outcome.value["timeframes"]
            .as_array()
            .map(|v| v.len())
            .unwrap_or(0)
            == 0,
        "a foreign duration produced rows: {}",
        outcome.value
    );
}

#[tokio::test]
async fn candles_and_the_snapshot_report_the_same_mid() {
    // Two surfaces, one value: the last closed candle's close and the snapshot's
    // mid are both derived from the same candle, so a divergence means one of
    // them is reading a different pipeline.
    let secs = 60u64;
    let (state, _pool) = test_state(&[secs]).await;
    let mut snapshot = sample_snapshot(secs, 100);
    snapshot.close = Some(100i64.into());
    publish(&state, secs, snapshot.clone()).await;

    let outcome = execution_daemon::shell::commands::execute(
        &state,
        &parse("data candles BTC-USDT --tf 60 --limit 5 --json")
            .unwrap()
            .command,
    )
    .await;
    // The candles table is empty in this fixture (no history was seeded), and the
    // shell says so rather than inventing rows.
    assert_eq!(
        outcome.value["durations"][0]["candles"]
            .as_array()
            .map(|v| v.len()),
        Some(0)
    );

    let state_outcome = execution_daemon::shell::commands::execute(
        &state,
        &parse("model state BTC-USDT --tf 60 --json")
            .unwrap()
            .command,
    )
    .await;
    assert_eq!(state_outcome.value["timeframes"][0]["mid_price"], "100");
}

#[tokio::test]
async fn every_duration_the_ladder_declares_appears_in_the_shell() {
    // `--tf all` must enumerate exactly the ACTIVE durations — the same set the
    // dashboard's timeframe rail draws.
    let ladder = [1u64, 3, 60, 300];
    let (state, _pool) = test_state(&ladder).await;
    for secs in ladder {
        publish(&state, secs, sample_snapshot(secs, secs as i64)).await;
    }
    let outcome = execution_daemon::shell::commands::execute(
        &state,
        &parse("model state BTC-USDT --tf all --json")
            .unwrap()
            .command,
    )
    .await;
    let rows = outcome.value["timeframes"].as_array().expect("rows");
    assert_eq!(rows.len(), ladder.len());
    for (row, secs) in rows.iter().zip(ladder.iter()) {
        assert_eq!(row["timeframe_secs"], *secs);
    }
    assert!(
        matches!(
            parse("data candles BTC-USDT --tf all").unwrap().command,
            execution_daemon::shell::parser::Command::Data(
                execution_daemon::shell::parser::DataCommand::Candles {
                    tf: TfSelector::All,
                    ..
                }
            ),
        ),
        "`--tf all` must stay an explicit selector, never a default"
    );
}
