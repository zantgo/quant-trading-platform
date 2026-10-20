//! AUDIT-C5 regression: a recharge must never strand an instance.
//!
//! The defect: `recharge_instance` cancelled the live instance and flushed its
//! buffers at the TOP of the function, then did all its fallible work (config
//! lookup, venue resolve, REST bootstrap, `build_pipelines`) before the single
//! `workspace.insert` at the end. Any error or abort in that window left the
//! workspace map holding the OLD instance whose `CancellationToken` was already
//! cancelled and whose buffers were cleared — so it still reported
//! `status: running`, its WS socket stayed open (the `broadcast::Sender` is
//! owned by the `Arc<TimeframePipeline>` the map still holds) and it delivered
//! NOTHING, forever. Every indicator read "Live" while the chart was frozen,
//! `/api/system/pipelines` reported healthy, and the only recovery was
//! delete + re-add.
//!
//! The trigger was mundane: the recharges run inside a `tokio::task::JoinSet`
//! (handlers/config.rs), and `JoinSet`'s Drop ABORTS every contained task. Axum
//! drops the handler future when the client disconnects, so closing the browser
//! tab three seconds after pressing SAVE was enough.
//!
//! The fix is build-then-swap: the live instance runs untouched through the
//! whole build and is only cancelled and drained at the swap point. These tests
//! pin that invariant without needing a real venue, by driving the failure
//! paths that occur BEFORE the swap.

use api_gateway::AppState;
use core_domain::LatencyTracker;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::execution::ExecutionEngine;
use portfolio_supervisor::registry::recharge_instance;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Notify, RwLock};

/// `run_single`'s future is ~2 MB in debug builds; the registry paths that
/// construct it overflow the default 2 MiB worker stack.
fn run_on_big_stack<F, T>(name: &str, f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("failed to spawn big-stack test thread")
        .join()
        .unwrap_or_else(|p| std::panic::resume_unwind(p))
}

fn entry(symbol: &str) -> config_models::InstanceEntry {
    config_models::InstanceEntry {
        id: symbol.to_lowercase(),
        symbol: symbol.into(),
        quote: symbol
            .rsplit_once('-')
            .map(|(_, q)| q.into())
            .unwrap_or_default(),
        status: config_models::InstanceStatus::Running,
        timeframes: std::collections::BTreeMap::new(),
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode: config_models::ExecutionMode::Observe,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    }
}

async fn app_state(pool: SqlitePool) -> Arc<AppState> {
    let workspace = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace
        .set_config(config_models::WorkspaceConfig {
            instances: vec![entry("BTC-USDT")],
            ..Default::default()
        })
        .await;
    Arc::new(AppState {
        workspace,
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        pool,
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(ConnectionQualityRegistry::new()),
        ws_url: "ws://127.0.0.1:1".into(),
        bitget_ws_url: String::new(),
        bybit_ws_url: String::new(),
        clock_monitor: None,
        reliability: Arc::new(ReliabilityTracker::new()),
        exchange_status: Arc::new(ExchangeStatusTracker::new()),
        latency_tracker: Arc::new(LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        model_state: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(Notify::new()),
        session_id: Arc::new(RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    })
}

/// THE load-bearing test: when the recharge fails EARLY (before the swap), the
/// old instance must be left completely untouched.
///
/// Before the fix the old instance's token was cancelled and its buffers
/// flushed at the top of the function, so this left a permanent zombie: still
/// listed as `running`, socket open, delivering nothing, no diagnostic anywhere.
#[test]
fn a_failed_recharge_leaves_the_old_instance_untouched() {
    run_on_big_stack("recharge_abort_safety", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(async {
                let pool = SqlitePool::connect("sqlite::memory:")
                    .await
                    .expect("sqlite");
                let state = app_state(pool).await;

                // Install a real instance into the live map so the early-failure
                // path has something to strand if the fix regresses.
                let ctx = state.registry_context();
                *state.session.base_currency.write().await =
                    Some(portfolio_supervisor::session::Currency::USDT);
                *state.session.exchange.write().await =
                    Some(portfolio_supervisor::session::ExchangeChoice::Bitget);
                state
                    .session
                    .active
                    .store(true, std::sync::atomic::Ordering::Relaxed);

                let inst = portfolio_supervisor::registry::add_instance(
                    &ctx,
                    ("BTC".into(), "USDT".into()),
                )
                .await
                .expect("add_instance must succeed with no live venue");

                let before_token = inst.cancel.clone();
                assert!(
                    !before_token.is_cancelled(),
                    "the freshly spawned instance must be live"
                );

                // Now force the recharge to fail BEFORE the swap. Removing the
                // saved config entry makes `pair_cfg` lookup fail, which is the
                // first fallible step after the (now removed) top-of-function
                // cancel.
                {
                    let mut cfg = state.workspace.config().await;
                    cfg.instances.clear();
                    state.workspace.set_config(cfg).await;
                }

                let res = recharge_instance(&ctx, "BTC-USDT").await;
                assert!(res.is_err(), "the recharge was made to fail; got {res:?}");

                // The old instance must still be in the workspace map...
                let survivor = state.workspace.get("BTC-USDT").await.expect(
                    "the old instance must survive a failed recharge -- \
                         otherwise it is a zombie: listed as running but dead",
                );

                // ...with a token that is NOT cancelled...
                assert!(
                    !survivor.cancel.is_cancelled(),
                    "a failed recharge must not cancel the live instance's pipelines \
                     (before the fix this was the first thing it did)"
                );

                // ...and still declaring its active durations (a flushed
                // zombie would report a stale or empty ladder).
                assert!(
                    !survivor.active_secs.is_empty(),
                    "the survivor must still declare its active durations, got {:?}",
                    survivor.active_secs
                );
            });
    });
}

/// A recharge for a pair that is not in the workspace must fail cleanly and
/// must not touch any OTHER instance.
#[test]
fn a_recharge_for_an_unknown_pair_does_not_disturb_others() {
    run_on_big_stack("recharge_unknown_pair", || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(async {
                let pool = SqlitePool::connect("sqlite::memory:")
                    .await
                    .expect("sqlite");
                let state = app_state(pool).await;
                let ctx = state.registry_context();

                let res = recharge_instance(&ctx, "NOPE-USDT").await;
                assert!(res.is_err(), "unknown pair must fail: {res:?}");
                assert!(
                    state.workspace.is_empty().await,
                    "a failed recharge must not install anything"
                );
            });
    });
}

/// The real tab-close scenario: a recharge that is actually RUNNING and then
/// aborted mid-flight must leave the old instance fully intact AND must not
/// orphan the half-built pipelines.
///
/// This must genuinely poll the future (a bare `drop(fut)` never runs it, so it
/// would pass against the unfixed code too). The task is spawned, allowed to
/// reach its network-bound bootstrap, then aborted exactly as `JoinSet`'s Drop
/// aborts every contained task when the HTTP client disconnects.
#[test]
fn an_aborted_recharge_task_leaves_the_old_instance_running() {
    run_on_big_stack("recharge_abort", || {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(async {
                let pool = SqlitePool::connect("sqlite::memory:")
                    .await
                    .expect("sqlite");
                let state = app_state(pool).await;
                let ctx = state.registry_context();
                *state.session.base_currency.write().await =
                    Some(portfolio_supervisor::session::Currency::USDT);
                *state.session.exchange.write().await =
                    Some(portfolio_supervisor::session::ExchangeChoice::Bitget);
                state
                    .session
                    .active
                    .store(true, std::sync::atomic::Ordering::Relaxed);

                let inst = portfolio_supervisor::registry::add_instance(
                    &ctx,
                    ("BTC".into(), "USDT".into()),
                )
                .await
                .expect("add_instance");

                let ctx2 = ctx.clone();
                let handle =
                    tokio::spawn(async move { recharge_instance(&ctx2, "BTC-USDT").await });

                // Let it get past the (now removed) top-of-function cancel and
                // into the network-bound bootstrap.
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                handle.abort();
                let _ = handle.await;

                // The old instance must be untouched.
                let survivor = state
                    .workspace
                    .get("BTC-USDT")
                    .await
                    .expect("the old instance must survive an aborted recharge");
                assert!(
                    !survivor.cancel.is_cancelled(),
                    "an aborted recharge must not cancel the live instance -- \
                     this is exactly the zombie: listed as running, socket open, \
                     delivering nothing, no diagnostic anywhere"
                );
                assert!(!inst.cancel.is_cancelled());
            });
    });
}
