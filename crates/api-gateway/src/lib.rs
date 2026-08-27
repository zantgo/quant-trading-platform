use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect},
    routing::{delete, get, post, put},
    Router,
};
use core_domain::normalized::SymbolMapper;
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use config_models::PlatformConfig;
use network_adapters::clock_monitor::ClockMonitor;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::execution::ExecutionEngine;
use portfolio_supervisor::instance::Instance;
use portfolio_supervisor::session::{Currency, ExchangeChoice, SessionState};
use portfolio_supervisor::workspace_state::WorkspaceState;

pub mod handlers;
pub mod helpers;
pub mod math;
pub mod telemetry;
pub mod types;
pub mod ws;

pub use math::compute_support_resistance;
pub use telemetry::compile_deterministic_telemetry;
pub use types::IndicatorSnapshot;

use portfolio_supervisor::registry_context::RegistryContext;

/// Notification emitted after `registry::recharge_instance` has swapped the
/// workspace entry for a pair. The WS handler subscribes to this channel so
/// it can re-subscribe to the new `ActivePair`'s broadcast channel when the
/// previous one is silently orphaned by the swap.
///
/// Without this notification the WS handler would hold a stale
/// `Arc<ActivePair>` whose embedded `broadcast::Sender` is kept alive by the
/// handler itself, so `Receiver::recv()` blocks forever (no `Closed` error),
/// the TCP socket never emits `onclose`, the frontend sees a frozen chart.
#[derive(Clone, Debug)]
pub struct RechargeNotice {
    pub pair_key: String,
}

pub struct AppState {
    /// The single workspace state (workspace config + live `Arc<Instance>`
    /// map). The binary supports one workspace per deployment; the field is
    /// named `workspace` not `instances` to make the hierarchy explicit
    /// (PLATFORM > WORKSPACE > INSTANCE — see
    /// `docs/conceptual-foundations/01-07-data-model-hierarchy.md`).
    pub workspace: WorkspaceState,
    pub session: Arc<SessionState>,
    /// Platform-level config (exchange endpoints, clock monitor). Separate
    /// from the workspace.
    pub platform: Arc<RwLock<PlatformConfig>>,
    pub pool: SqlitePool,
    pub symbol_mapper: Arc<SymbolMapper>,
    pub telemetry_tx: mpsc::Sender<database_storage::TelemetryMsg>,
    /// Per-(pair_key, timeframe_secs) connection-quality scopes (08-05).
    pub connection_quality: Arc<ConnectionQualityRegistry>,
    pub clock_monitor: Option<Arc<ClockMonitor>>,
    pub reliability: Arc<ReliabilityTracker>,
    pub exchange_status: Arc<ExchangeStatusTracker>,
    pub latency_tracker: core_domain::SharedLatencyTracker,
    pub ws_url: String,
    pub bitget_ws_url: String,
    /// v10.1: browser-origin allowlist for the served HTTP/WS surface —
    /// built from the resolved bind/port (per-folder sessions) plus the
    /// Vite dev origins. K1 boundary: any other origin is refused.
    pub allowed_origins: Vec<String>,
    /// L7 cross-symbol market overview, refreshed periodically.
    pub overview: Arc<RwLock<Option<core_domain::overview::OverviewMatrix>>>,
    pub execution_engine: Arc<ExecutionEngine>,
    /// v7 TAE setup executor (when automation is enabled). Serves the
    /// `/api/instances/:id/automation` surface.
    pub automation: Option<Arc<portfolio_supervisor::setup_executor::SetupExecutor>>,
    /// Notification channel fired by HTTP handlers after a successful
    /// `recharge_instance`. Subscribed by the WS handler so it can swap its
    /// broadcast subscription off the orphaned `ActivePair` onto the new one.
    pub recharge_tx: tokio::sync::broadcast::Sender<RechargeNotice>,

    // ── Snapshot Export (v6.10.4+) ─────────────────────────────────
    /// Runtime state of the periodic snapshot-export task. Shared between
    /// the task (reads config every tick), the HTTP handlers
    /// (`/api/snapshot-export/status`, `.../config`, `.../run-now`), and
    /// the CLI `setup` flow (which writes the same data into
    /// `config.toml`).
    pub snapshot_export: Arc<RwLock<core_domain::snapshot_export::SnapshotExportRuntime>>,
    /// `Notify` fired by `POST /api/snapshot-export/run-now` to wake the
    /// scheduler task for an immediate tick (the next scheduled tick is
    /// unaffected).
    pub snapshot_export_manual_tick: Arc<tokio::sync::Notify>,

    // ── v10 session identity ───────────────────────────────────────
    /// The current session id (monotonic, persisted). `None` before the
    /// session is created at boot.
    pub session_id: Arc<RwLock<Option<i64>>>,

    // ── Backtesting Engine (BTE, v8) ───────────────────────────────
    /// Single-run lock + live backfill progress registry. The BTE runs one
    /// backtest at a time; concurrent runs return 409.
    pub backtest: Arc<backtesting_engine::registry::BacktestRegistry>,
}

impl AppState {
    /// Build the `RegistryContext` view that portfolio-supervisor's registry
    /// functions take. Extracted here to keep the registry decoupled from
    /// `api-gateway`'s `AppState` type.
    pub fn registry_context(&self) -> RegistryContext {
        RegistryContext {
            workspace: self.workspace.clone(),
            session: self.session.clone(),
            platform: self.platform.clone(),
            pool: self.pool.clone(),
            symbol_mapper: self.symbol_mapper.clone(),
            telemetry_tx: self.telemetry_tx.clone(),
            latency_tracker: self.latency_tracker.clone(),
            ws_url: self.ws_url.clone(),
            bitget_ws_url: self.bitget_ws_url.clone(),
            exchange_status: self.exchange_status.clone(),
            reliability: self.reliability.clone(),
            connection_quality: self.connection_quality.clone(),
        }
    }

    pub async fn instance_count(&self) -> usize {
        self.workspace.len().await
    }

    pub async fn get_all_instances(&self) -> Vec<Arc<Instance>> {
        self.workspace.list().await
    }

    pub async fn get_active_pair(
        &self,
        pair_key: &str,
    ) -> Option<Arc<market_analyzer::analyzer::ActivePair>> {
        self.workspace
            .get(pair_key)
            .await
            .map(|inst| inst.active_pair.clone())
    }

    pub async fn get_instance_by_id(&self, id: &str) -> Option<Arc<Instance>> {
        for inst in self.workspace.list().await {
            if inst.id == id {
                return Some(inst);
            }
        }
        None
    }

    pub async fn init_session(
        &self,
        currency: Currency,
        exchange: ExchangeChoice,
    ) -> Result<(), String> {
        if exchange != ExchangeChoice::Hyperliquid && exchange != ExchangeChoice::Bitget {
            return Err("Unsupported exchange selected.".to_string());
        }
        if !exchange.supports_currency(&currency) {
            return Err(format!(
                "{} does not support {} settlement. {}",
                exchange.as_str(),
                currency.as_str(),
                match exchange {
                    ExchangeChoice::Hyperliquid => "Hyperliquid perpetuals settle in USDC only.",
                    ExchangeChoice::Bitget => "Select USDT or USDC.",
                }
            ));
        }

        *self.session.base_currency.write().await = Some(currency);
        *self.session.exchange.write().await = Some(exchange);
        self.session
            .active
            .store(true, std::sync::atomic::Ordering::Relaxed);

        println!(
            "✅ Session initialized: {} on {}",
            currency.as_str(),
            exchange.as_str(),
        );
        Ok(())
    }

    pub async fn quit_session(&self) -> Result<(), String> {
        println!("🛑 Initiating graceful shutdown of all instances...");

        // (1) Capture the live pair keys BEFORE we touch anything. The
        // `WorkspaceState` keeps two separate state holders — `config`
        // (declarative) and `instances` (live runtime map) — and
        // `set_config` does NOT reconcile the live map. We have to drop
        // every entry from both sides, otherwise the next
        // `/api/instances` call (which reads the live map) returns
        // rows that the persisted TOML says shouldn't exist.
        let live_pair_keys: Vec<String> = {
            self.workspace
                .list()
                .await
                .iter()
                .map(|i| i.pair_key())
                .collect()
        };

        // (2) Cancel every running pipeline task. `cancel.cancel()` is
        // idempotent so it's safe even when an instance is already
        // Stopped.
        for pair_key in &live_pair_keys {
            if let Some(instance) = self.workspace.get(pair_key).await {
                instance.cancel.cancel();
            }
        }

        // Let cancellation propagate so the orphaned tasks can observe
        // it before we drop their `Arc<Instance>` references.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // (3) Drop every live instance from the runtime map. Without
        // this, `/api/instances` (which reads `WorkspaceState::list()`,
        // NOT `WorkspaceState::config()`) keeps returning the just-quit
        // instances and the dashboard shows them on re-entry.
        for pair_key in &live_pair_keys {
            self.workspace.remove(pair_key).await;
        }

        // (4) Clear the declarative config + persist to TOML.
        let mut ws = self.workspace.config().await;
        ws.instances.clear();
        self.workspace.set_config(ws.clone()).await;

        if let Err(e) = config_models::save_workspace(&ws) {
            eprintln!("⚠️  Failed to persist workspace after quit: {}", e);
            // Don't fail the quit — the in-memory state is already
            // correct and the operator can recover with `destroy` if
            // the TOML is unreadable.
        } else {
            println!(
                "💾 Workspace persisted: 0 instances after quit (deleted {} entries from config.toml)",
                live_pair_keys.len()
            );
        }

        // (5) Mark session inactive.
        self.session
            .active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        *self.session.base_currency.write().await = None;
        *self.session.exchange.write().await = None;

        println!("✅ Session terminated. All instances stopped.");
        Ok(())
    }
}

// ── Stratified state types for Axum FromRef ──────────────────────

#[derive(Clone)]
pub struct DbState(pub SqlitePool);

impl axum::extract::FromRef<Arc<AppState>> for DbState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.pool.clone())
    }
}

#[derive(Clone)]
pub struct WsState(pub mpsc::Sender<database_storage::TelemetryMsg>);

impl axum::extract::FromRef<Arc<AppState>> for WsState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.telemetry_tx.clone())
    }
}

#[derive(Clone)]
pub struct ConfigState(pub Arc<RwLock<PlatformConfig>>);

impl axum::extract::FromRef<Arc<AppState>> for ConfigState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.platform.clone())
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/api/session/status",
            get(handlers::session::serve_session_status),
        )
        .route("/api/sessions", get(handlers::session::serve_sessions_list))
        .route(
            "/api/sessions/:id/analytics",
            get(handlers::analytics::serve_session_analytics),
        )
        .route(
            "/api/analytics/comparison",
            get(handlers::analytics::serve_analytics_comparison),
        )
        .route(
            "/api/session/init",
            post(handlers::session::serve_session_init),
        )
        .route(
            "/api/session/quit",
            post(handlers::session::serve_session_quit),
        )
        .route(
            "/api/config",
            get(handlers::config::serve_config).post(handlers::config::update_config),
        )
        .route(
            "/api/workspace/toml",
            get(handlers::config::serve_workspace_toml)
                .post(handlers::config::serve_workspace_toml_import),
        )
        .route(
            "/api/rules",
            get(handlers::config::serve_get_rules).post(handlers::config::serve_set_rules),
        )
        .route("/api/history", get(handlers::history::serve_history))
        .route(
            "/api/connection-quality",
            get(handlers::connection_quality::get_connection_quality),
        )
        .route(
            "/api/liquidity/cluster-status",
            get(handlers::cluster_status::serve_cluster_status),
        )
        .route("/api/overview", get(handlers::overview::serve_overview))
        .route(
            "/api/snapshot-export/status",
            get(handlers::snapshot_export::serve_snapshot_export_status),
        )
        .route(
            "/api/snapshot-export/config",
            put(handlers::snapshot_export::serve_update_snapshot_export_config),
        )
        .route(
            "/api/snapshot-export/run-now",
            post(handlers::snapshot_export::serve_run_snapshot_export_now),
        )
        .route("/api/monitor", get(handlers::monitor::serve_monitor))
        .route(
            "/api/trades",
            get(handlers::trades::serve_get_trades).post(handlers::trades::serve_add_trade),
        )
        .route(
            "/api/trade-ledger",
            get(handlers::trades::serve_trade_ledger),
        )
        .route(
            "/api/trade-journal",
            get(handlers::trades::serve_trade_journal),
        )
        .route(
            "/api/trade-journal/:id/notes",
            post(handlers::trades::serve_update_journal_notes),
        )
        .route(
            "/api/trade-journal/export/csv",
            get(handlers::trades::serve_export_journal_csv),
        )
        .route(
            "/api/trade-journal/export/json",
            get(handlers::trades::serve_export_journal_json),
        )
        .route(
            "/api/trades/telemetry",
            post(handlers::trades::serve_trade_telemetry_add),
        )
        .route(
            "/api/instances",
            get(handlers::instances::serve_list_instances)
                .post(handlers::instances::serve_add_instance),
        )
        .route(
            "/api/instances/:instance_id",
            get(handlers::instances::serve_get_instance_detail)
                .delete(handlers::instances::serve_delete_instance),
        )
        .route(
            "/api/instances/by-pair/:pair_key",
            delete(handlers::instances::serve_delete_instance_by_pair),
        )
        .route(
            "/api/instances/:instance_id/config",
            post(handlers::instances::serve_update_instance_config),
        )
        .route(
            "/api/instances/:instance_id/pause",
            post(handlers::instances::serve_pause_instance),
        )
        .route(
            "/api/instances/:instance_id/start",
            post(handlers::instances::serve_start_instance),
        )
        .route(
            "/api/instances/:instance_id/stop",
            post(handlers::instances::serve_stop_instance),
        )
        .route(
            "/api/instances/:instance_id/safety/reset",
            post(handlers::instances::serve_reset_safety),
        )
        .route(
            "/api/instances/:instance_id/lifecycle",
            post(handlers::instances::serve_instance_lifecycle),
        )
        .route(
            "/api/strategies",
            get(handlers::strategies::list_strategies).post(handlers::strategies::create_strategy),
        )
        .route(
            "/api/strategies/:name",
            get(handlers::strategies::get_strategy)
                .put(handlers::strategies::update_strategy)
                .delete(handlers::strategies::delete_strategy),
        )
        .route(
            "/api/strategies/:name/clone",
            post(handlers::strategies::clone_strategy),
        )
        .route(
            "/api/account/summary",
            get(handlers::account::account_summary),
        )
        .route(
            "/api/account/capital",
            post(handlers::account::set_account_capital),
        )
        .route("/api/account/reset", post(handlers::account::reset_account))
        .route(
            "/api/instances/:instance_id/safety/release-veto",
            post(handlers::instances::serve_release_veto),
        )
        .route(
            "/api/instances/:instance_id/safety",
            get(handlers::instances::serve_get_safety),
        )
        .route(
            "/api/instances/:instance_id/portfolio",
            get(handlers::instances::serve_get_portfolio),
        )
        .route(
            "/api/instances/:instance_id/exposure",
            get(handlers::instances::serve_get_exposure),
        )
        .route(
            "/api/instances/:instance_id/capital",
            get(handlers::instances::serve_get_capital),
        )
        .route(
            "/api/instances/:instance_id/safety/session-reset",
            post(handlers::instances::serve_session_reset),
        )
        .route(
            "/api/instances/:instance_id/automation",
            get(handlers::instances::serve_get_automation),
        )
        .route(
            "/api/instances/:instance_id/automation/close",
            post(handlers::instances::serve_automation_close),
        )
        .route(
            "/api/instances/:instance_id/intervals",
            post(handlers::instances::serve_instance_intervals),
        )
        .route(
            "/api/instances/:instance_id/activation",
            get(handlers::instances::serve_get_activation),
        )
        .route(
            "/api/instances/:instance_id/reload",
            post(handlers::instances::serve_reload_timeframe),
        )
        .route(
            "/api/risk-profiles",
            get(handlers::profiles::serve_risk_profiles_list)
                .post(handlers::profiles::serve_risk_profile_create),
        )
        .route(
            "/api/risk-profiles/:id",
            delete(handlers::profiles::serve_risk_profile_delete)
                .post(handlers::profiles::serve_risk_profile_update),
        )
        .route(
            "/api/risk/calculate",
            post(handlers::profiles::serve_risk_calculate),
        )
        .route(
            "/api/risk/fee-table",
            get(handlers::profiles::serve_fee_table),
        )
        .route(
            "/api/risk/commission-projection",
            post(handlers::profiles::serve_commission_projection),
        )
        .route(
            "/api/dashboard/stats",
            get(handlers::dashboard::serve_dashboard_stats),
        )
        .route(
            "/api/analytics/strategy",
            get(handlers::analytics::serve_strategy_analytics),
        )
        .route(
            "/api/analytics/strategy/history",
            get(handlers::analytics::serve_strategy_analytics_history),
        )
        .route(
            "/api/analytics/risk",
            get(handlers::analytics::serve_risk_analytics),
        )
        .route(
            "/api/analytics/performance",
            get(handlers::analytics::serve_performance_matrix),
        )
        .route(
            "/api/analytics/optimization",
            get(handlers::analytics::serve_optimization_report),
        )
        .route(
            "/api/analytics/trades",
            get(handlers::analytics::serve_trade_analytics),
        )
        .route(
            "/api/analytics/summary",
            get(handlers::analytics::serve_performance_summary),
        )
        .route(
            "/api/backtest/run",
            post(handlers::analytics::serve_backtest_run),
        )
        .route(
            "/api/backtest/list",
            get(handlers::analytics::serve_backtest_list),
        )
        .route(
            "/api/backtest/progress/:id",
            get(handlers::analytics::serve_backtest_progress),
        )
        .route(
            "/api/backtest/cancel/:id",
            post(handlers::analytics::serve_backtest_cancel),
        )
        .route(
            "/api/backtest/coverage",
            get(handlers::backtest::serve_backtest_coverage),
        )
        .route(
            "/api/backtest/archive/backfill",
            post(handlers::backtest::serve_backfill_start),
        )
        .route(
            "/api/backtest/archive/progress/:id",
            get(handlers::backtest::serve_backfill_progress),
        )
        .route(
            "/api/backtest/archive/cancel/:id",
            post(handlers::backtest::serve_backfill_cancel),
        )
        .route(
            "/api/backtest/:id/input_bars",
            get(handlers::analytics::serve_backtest_input_bars),
        )
        .route(
            "/api/backtest/:id/trades",
            get(handlers::analytics::serve_backtest_trades),
        )
        .route(
            "/api/backtest/:id/equity",
            get(handlers::analytics::serve_backtest_equity),
        )
        .route(
            "/api/backtest/:id/portfolio",
            get(handlers::analytics::serve_backtest_portfolio),
        )
        .route(
            "/api/backtest/:id/signals",
            get(handlers::analytics::serve_backtest_signals),
        )
        .route(
            "/api/backtest/:id/metrics",
            get(handlers::analytics::serve_backtest_metrics),
        )
        .route(
            "/api/backtest/:id",
            get(handlers::analytics::serve_backtest_get),
        )
        .route(
            "/api/keys",
            get(handlers::keys::list_keys).post(handlers::keys::add_key),
        )
        .route("/api/keys/rotate", post(handlers::keys::rotate_keys))
        .route(
            "/api/keys/backup",
            get(handlers::keys::backup_keys).post(handlers::keys::backup_keys_post),
        )
        .route("/api/keys/:key_id", delete(handlers::keys::delete_key))
        .route(
            "/api/system/status",
            get(handlers::system::serve_system_status),
        )
        .route(
            "/api/system/observability",
            get(handlers::system::serve_observability_buffers),
        )
        .route(
            "/api/system/clock",
            get(handlers::clock::serve_clock_status),
        )
        .route(
            "/api/system/platform-config",
            get(handlers::system::serve_platform_config),
        )
        .route(
            "/api/system/pipelines",
            get(handlers::system::serve_system_pipelines),
        )
        .route(
            "/api/system/distribution",
            get(handlers::system::serve_system_distribution),
        )
        .route(
            "/api/exchange-status",
            get(handlers::exchange_status::serve_exchange_status),
        )
        .route(
            "/api/data-quality",
            get(handlers::data_quality::serve_data_quality),
        )
        .route("/ws", get(ws::ws_handler))
        .route(
            "/favicon.ico",
            get(|| async { Redirect::to("/favicon.svg") }),
        )
        .layer(axum::middleware::from_fn(rate_limit_middleware))
        .layer(
            // K1 (production audit): CORS is locked to the dashboard's own
            // origins — previously `allow_origin(Any)` let ANY website
            // drive every unauthenticated endpoint (config rewrite,
            // instance lifecycle, safety-veto release) from the operator's
            // browser. Same-origin dashboard fetches need no CORS headers
            // at all; the allowlist only keeps direct-origin bookmarks and
            // the Vite dev proxy working.
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(
                    state
                        .allowed_origins
                        .iter()
                        .filter_map(|o| HeaderValue::from_str(o).ok())
                        .collect::<Vec<_>>(),
                ))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::PATCH,
                    Method::OPTIONS,
                ])
                .allow_headers(AllowHeaders::list(vec![CONTENT_TYPE, AUTHORIZATION])),
        )
        // Cross-site rejection: placed OUTERMOST so the 403 is issued
        // before any handler runs. Modern browsers always attach
        // `Sec-Fetch-Site` to cross-origin fetches and WS upgrades; a
        // `cross-site` value (or a foreign `Origin` header) is refused
        // outright. Same-origin dashboard fetches and Vite-dev proxied
        // requests carry `same-origin`/the app origin and pass.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            reject_cross_site,
        ))
        .fallback_service(ServeDir::new("ui/dist"))
        .with_state(state)
}

/// Origins the dashboard itself can be served from — built from the
/// resolved bind/port (v10.1, per-folder sessions) plus the Vite dev
/// server origins so operator bookmarks and `bun run dev` keep working
/// on any port. Any other origin is refused.
pub fn default_allowed_origins(bind: &str, port: u16) -> Vec<String> {
    let mut out = vec![
        format!("http://{bind}:{port}"),
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        "http://127.0.0.1:5173".to_string(),
        "http://localhost:5173".to_string(),
    ];
    out.sort();
    out.dedup();
    out
}

pub fn origin_allowed(origin: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == origin)
}

/// K1 (production audit): the API is unauthenticated and binds loopback
/// only — the one remaining boundary against remote attackers is the
/// browser's same-origin policy. `Sec-Fetch-Site` is present on every
/// browser-originated cross-site request; `Origin` is present on all
/// browser POSTs. Either header proving a foreign site → 403.
async fn reject_cross_site(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let headers = req.headers();
    if let Some(fetch_site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if fetch_site != "same-origin" && fetch_site != "same-site" && fetch_site != "none" {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    if let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
    {
        if !origin_allowed(origin, &state.allowed_origins) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

/// Global rate limiter (single-operator loopback, 30 req/s).
/// Uses a sliding window of 1 s kept in a process-wide `Mutex<VecDeque>`.
/// `429 Too Many Requests` when the window is full; no per-IP tracking
/// needed because the server binds loopback-only (one operator).
/// Burst of 5-10 parallel dashboard polls (overview + instances + coverage)
/// previously hit the old 10/s ceiling and surfaced as `Backfill failed`.
async fn rate_limit_middleware(req: Request, next: Next) -> axum::response::Response {
    static WINDOW: Duration = Duration::from_secs(1);
    const LIMIT: usize = 30;
    static STATE: LazyLock<Mutex<VecDeque<Instant>>> =
        LazyLock::new(|| Mutex::new(VecDeque::new()));

    let now = Instant::now();
    let should_limit = {
        let mut guard = STATE.lock().expect("rate-limit mutex poisoned");
        // evict entries outside the 1 s window
        while guard
            .front()
            .is_some_and(|t| now.duration_since(*t) >= WINDOW)
        {
            guard.pop_front();
        }
        if guard.len() >= LIMIT {
            true
        } else {
            guard.push_back(now);
            false
        }
    };
    if should_limit {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, "1")],
            "Rate limit exceeded: 30 req/s",
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IndicatorSnapshot;

    #[test]
    fn test_support_resistance_calculations() {
        let prices = vec![
            3110.0, 3135.0, 3105.0, 3140.0, 3100.0, 3150.0, 3115.0, 3145.0, 3120.0, 3130.0,
        ];
        let current_price = 3125.0;

        let (support, resistance) = compute_support_resistance(&prices, current_price);

        for s in &support {
            let s_val: f64 = s.parse().unwrap();
            assert!(
                s_val < current_price,
                "Support {} should be below current price",
                s_val
            );
        }

        for r in &resistance {
            let r_val: f64 = r.parse().unwrap();
            assert!(
                r_val > current_price,
                "Resistance {} should be above current price",
                r_val
            );
        }

        assert!(support.len() <= 3);
        assert!(resistance.len() <= 3);
    }

    #[test]
    fn test_compile_deterministic_telemetry() {
        use market_analyzer::indicators::normalized::NormalizedIndicatorValue;
        use std::collections::HashMap;

        let mut map: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(25.0, 0.8, "OVERSOLD_ACCUMULATION"),
        );
        map.insert(
            "bbwp".into(),
            NormalizedIndicatorValue::scalar(5.0, 0.0, "MAX_VOLATILITY_COMPRESSION"),
        );
        // RVOL fallback: non-directional gate (normalized=0.0, band in values.rvol_band).
        map.insert(
            "rvol".into(),
            NormalizedIndicatorValue::with_values(
                0.8,
                0.0,
                "LOW_PARTICIPATION_VOLUME",
                [("rvol_band".to_string(), -0.5)].into_iter().collect(),
            ),
        );
        map.insert(
            "squeeze".into(),
            NormalizedIndicatorValue::scalar(-0.05, -0.2, "BEARISH_MOMENTUM_EXHAUSTING"),
        );
        map.insert("macd".into(), {
            let mut v = HashMap::new();
            v.insert("line".to_string(), -0.5);
            v.insert("signal".to_string(), -0.3);
            v.insert("histogram".to_string(), -0.2);
            NormalizedIndicatorValue::with_values(-0.2, -0.5, "BEARISH_MOMENTUM_EXPANDING", v)
        });
        map.insert("adx".into(), {
            let mut v = HashMap::new();
            v.insert("adx".to_string(), 15.0);
            v.insert("plus_di".to_string(), 12.0);
            v.insert("minus_di".to_string(), 18.0);
            NormalizedIndicatorValue::with_values(15.0, 0.0, "TRENDLESS_CONGESTION", v)
        });
        map.insert("ema_stack".into(), {
            let mut v = HashMap::new();
            v.insert("long".to_string(), 3200.0);
            NormalizedIndicatorValue::with_values(3125.0, -1.0, "ESTABLISHED_BEARISH_STACK", v)
        });
        map.insert("vwap".into(), {
            let mut v = HashMap::new();
            v.insert("vwap".to_string(), 3130.0);
            NormalizedIndicatorValue::with_values(3130.0, 0.8, "EXTREME_DISCOUNT_REVERSION_ZONE", v)
        });
        // Push a Divergence signal onto the RSI entry (divergence lives on parent).
        if let Some(rsi_entry) = map.get_mut("rsi") {
            rsi_entry.signals.push(
                market_analyzer::indicators::normalized::IndicatorSignal::new(
                    market_analyzer::indicators::normalized::SignalKind::Divergence,
                    market_analyzer::indicators::normalized::SignalDirection::Bullish,
                    market_analyzer::indicators::normalized::SignalStatus::Potential,
                    "POTENTIAL_BULLISH_DIVERGENCE",
                ),
            );
        }
        map.insert("atr".into(), {
            let mut v = HashMap::new();
            v.insert("atr_14".to_string(), 1.5);
            NormalizedIndicatorValue::with_values(1.5, 0.0, "ATR_RAW", v)
        });
        let indicators = IndicatorSnapshot::new(map, Some(3125.0));

        let support_levels: Vec<String> = vec!["3100.00".to_string(), "3050.00".to_string()];
        let resistance_levels: Vec<String> = vec!["3150.00".to_string(), "3200.00".to_string()];

        let telemetry = telemetry::compile_deterministic_telemetry(
            &indicators,
            &support_levels,
            &resistance_levels,
        );

        assert_eq!(telemetry.market_regime, "COMPRESSION");
        assert!(telemetry.total_confluence_score < 0);
        assert_eq!(telemetry.rvol, 0.8);
        assert_eq!(telemetry.adx_value, 15.0);
        assert_eq!(telemetry.adx_regime, "congestion");
        assert!((telemetry.bbwp_percentile - 5.0).abs() < 0.001);
        assert!(!telemetry.squeeze_on);
        assert_eq!(telemetry.vwap_bias, "discount");
        assert_eq!(telemetry.rsi_divergence_state, "potential_bullish");
        assert_eq!(telemetry.macd_divergence_state, "none");
        assert_eq!(telemetry.macd_crossover_state, "none");
        assert_eq!(telemetry.squeeze_release_state, "none");
        assert_eq!(telemetry.support_levels.len(), 2);
        assert_eq!(telemetry.resistance_levels.len(), 2);
    }
}
