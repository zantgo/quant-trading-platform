//! HTTP surface assembly — `api-gateway` (v11.14.2 build-time split).
//!
//! This crate used to hold `AppState`, the wire DTOs, the WebSocket handler
//! and all 21 HTTP handlers in one compilation unit. Measured at 247 s on its
//! own — roughly 17x the per-line cost of the engine crates — because rustc
//! compiles one process per crate, so that work was entirely serial while the
//! other cores sat idle.
//!
//! It is now a thin facade: the code lives in
//!
//! * `api-core` — `AppState`, `types`, `ws`, `telemetry`, `math`, `helpers`,
//!   the origin/middleware helpers. The shared kernel every handler needs.
//! * `api-market` / `api-instances` / `api-analytics` / `api-system` — four
//!   sibling handler groups that compile in parallel against `api-core`.
//! * `api-gateway` (here) — `build_router` plus the re-exports below.
//!
//! The re-exports are load-bearing: they keep every existing consumer path
//! (`api_gateway::AppState`, `api_gateway::handlers::analytics::BacktestRequest`,
//! `api_gateway::types::…`, `api_gateway::ws::connected_client_count`, and so
//! on) compiling unchanged, including all 28 integration tests.

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    response::Redirect,
    routing::{delete, get, post, put},
    Router,
};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

pub use api_core::{
    default_allowed_origins, helpers, math, origin_allowed, telemetry, types, ws, AppState,
    ConfigState, DbState, IndicatorSnapshot, InterruptedSessionInfo, RechargeNotice, WsState,
};

pub use api_analytics::{analytics, backtest, trades};
pub use api_instances::{config, instances, keys, profiles, session, strategies};
pub use api_market::{connection_quality, history, model, monitor, overview, snapshot_export};
pub use api_system::{account, clock, dashboard, data_quality, exchange_status, system};

/// Re-assembles the four handler groups under the historical `handlers::`
/// namespace so that route registration — and every consumer — reads exactly
/// as it did when all 21 modules lived in this crate.
pub mod handlers {
    pub use api_analytics::{analytics, backtest, trades};
    pub use api_instances::{config, instances, keys, profiles, session, strategies};
    pub use api_market::{connection_quality, history, model, monitor, overview, snapshot_export};
    pub use api_system::{account, clock, dashboard, data_quality, exchange_status, system};
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/api/session/status",
            get(handlers::session::serve_session_status),
        )
        .route(
            "/api/session/recover",
            post(handlers::session::serve_session_recover),
        )
        .route(
            "/api/session/discard",
            post(handlers::session::serve_session_discard),
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
        .route("/api/overview", get(handlers::overview::serve_overview))
        .route("/api/model", get(handlers::model::serve_model))
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

/// Global rate limiter (single-operator loopback, 60 req/s).
/// Uses a sliding window of 1 s kept in a process-wide `Mutex<VecDeque>`.
/// `429 Too Many Requests` when the window is full; no per-IP tracking
/// needed because the server binds loopback-only (one operator).
/// Burst of 5-10 parallel dashboard polls (overview + instances + coverage)
/// previously hit the old 10/s ceiling and surfaced as `Backfill failed`.
/// v11.3: raised 30 → 60 req/s for multi-tab viewers (each tab polls
/// its own overview/instances cycle).
async fn rate_limit_middleware(req: Request, next: Next) -> axum::response::Response {
    static WINDOW: Duration = Duration::from_secs(1);
    const LIMIT: usize = 60;
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
            "Rate limit exceeded: 60 req/s",
        )
            .into_response();
    }
    next.run(req).await
}
