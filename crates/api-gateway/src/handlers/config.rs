use crate::types::{ConfigResponse, RulesResponse, SetRulesRequest};
use crate::AppState;
use axum::{extract::State, http::header, response::IntoResponse, Json};
use config_models::WorkspaceConfig;
use std::sync::Arc;

pub async fn serve_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let current_config = state.workspace.config().await;
    let response_body = ConfigResponse {
        api_key_configured: true,
        symbols: current_config.declared_symbols(),
        candles: current_config.candles.clone(),
        indicators: current_config.indicators.clone(),
        instances: current_config.instances.clone(),
        indicator_registry: market_analyzer::indicators::registry::all(),
        api_failover: current_config.api_failover,
        slow_timeframe: Some(current_config.slow_timeframe.clone()),
        macro_timeframe: Some(current_config.macro_timeframe.clone()),
        liquidity: Some(current_config.liquidity.clone()),
        minimal_tae: Some(current_config.minimal_tae.clone()),
        analytics: Some(current_config.analytics.clone()),
        risk_limits: Some(current_config.risk_limits.clone()),
        safety: Some(current_config.safety.clone()),
        fees: Some(current_config.fees.clone()),
        leverage: Some(current_config.leverage.clone()),
        execution: Some(current_config.execution.clone()),
        activation: Some(current_config.activation.clone()),
        backtest: Some(current_config.backtest.clone()),
    };
    let json = axum::Json(response_body);
    let mut response = json.into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response
}

/// A partial config update: the dashboard's `GET /api/config` response
/// round-trips through `POST /api/config` with only the operator-editable
/// fields mutated (`candles`, `indicators`, `instances`, `api_failover`,
/// and since v7.4 the engine-settings sections: `minimal_tae`, `safety`,
/// `risk_limits`, `analytics`, `execution`, `fees`, `leverage`).
/// The body therefore does NOT carry `WorkspaceConfig`'s mandatory
/// `id`/`name`/`default_currency`/`default_exchange` — merge the provided
/// fields into the currently loaded config rather than demanding a full
/// document (fixes the previous permanent-422 round-trip failure).
#[derive(Debug, serde::Deserialize)]
pub struct ConfigUpdateRequest {
    #[serde(default)]
    pub candles: Option<config_models::CandlesConfig>,
    /// v11.2: active-timeframe count (fastest N of the fixed pool, 1..=10).
    #[serde(default)]
    pub active_timeframes: Option<usize>,
    #[serde(default)]
    pub indicators: Option<config_models::IndicatorsConfig>,
    #[serde(default)]
    pub instances: Option<Vec<config_models::InstanceEntry>>,
    #[serde(default)]
    pub api_failover: Option<config_models::ApiFailoverConfig>,
    // v7.4: engine-settings sections (editable via the dashboard Settings
    // tabs). All optional — a partial update touches only what it sends.
    #[serde(default)]
    pub minimal_tae: Option<config_models::MinimalTaeConfig>,
    #[serde(default)]
    pub safety: Option<config_models::SafetyConfig>,
    #[serde(default)]
    pub risk_limits: Option<config_models::RiskLimitsConfig>,
    #[serde(default)]
    pub analytics: Option<config_models::AnalyticsConfig>,
    #[serde(default)]
    pub execution: Option<config_models::ExecutionConfig>,
    #[serde(default)]
    pub fees: Option<config_models::FeesConfig>,
    #[serde(default)]
    pub leverage: Option<config_models::LeverageConfig>,
    #[serde(default)]
    pub activation: Option<config_models::ActivationConfig>,
    // v8: Backtesting Engine settings section.
    #[serde(default)]
    pub backtest: Option<config_models::BacktestConfig>,
    // Accept-and-ignore: derived or read-only fields echoed by the GET
    // response that must not clobber the loaded config on save.
    #[serde(default)]
    pub api_key_configured: Option<bool>,
    #[serde(default)]
    pub symbols: Option<Vec<String>>,
    #[serde(default)]
    pub indicator_registry: Option<serde_json::Value>,
}

/// M8-style range validation for the v7.4 engine-settings sections.
/// Returns `None` when every provided value is within range, otherwise the
/// first offending field description (the dashboard mirrors these ranges).
fn validate_ranges(payload: &ConfigUpdateRequest) -> Option<String> {
    let f = |v: f64, min: f64, max: f64| v >= min && v <= max;
    if let Some(n) = payload.active_timeframes {
        if !(1..=10).contains(&n) {
            return Some("active_timeframes must be 1–10".into());
        }
    }
    if let Some(fees) = &payload.fees {
        if !f(fees.maker_fee_pct, 0.0, 5.0) {
            return Some("fees.maker_fee_pct must be 0–5".into());
        }
        if !f(fees.taker_fee_pct, 0.0, 5.0) {
            return Some("fees.taker_fee_pct must be 0–5".into());
        }
        if !f(fees.funding_rate_8h, 0.0, 2.0) {
            return Some("fees.funding_rate_8h must be 0–2".into());
        }
    }
    if let Some(lev) = &payload.leverage {
        if lev.cross_leverage < 1 || lev.cross_leverage > 150 {
            return Some("leverage.cross_leverage must be 1–150".into());
        }
    }
    if let Some(tae) = &payload.minimal_tae {
        if !f(tae.allocation_pct, 1.0, 100.0) {
            return Some("minimal_tae.allocation_pct must be 1–100".into());
        }
        if !f(tae.min_net_rr, 0.0, 20.0) {
            return Some("minimal_tae.min_net_rr must be 0–20".into());
        }
        if tae.max_open_positions < 1 || tae.max_open_positions > 20 {
            return Some("minimal_tae.max_open_positions must be 1–20".into());
        }
        if let Some(cap) = tae.max_position_size_pct_of_equity {
            if cap <= 0.0 || cap > 1_000_000_000.0 {
                return Some("minimal_tae.max_position_size_pct_of_equity out of range".into());
            }
        }
    }
    if let Some(s) = &payload.safety {
        if s.consecutive_loss_caution < 1 || s.consecutive_loss_caution > 20 {
            return Some("safety.consecutive_loss_caution must be 1–20".into());
        }
        if s.consecutive_loss_dropout < 2 || s.consecutive_loss_dropout > 20 {
            return Some("safety.consecutive_loss_dropout must be 2–20".into());
        }
        if s.consecutive_loss_dropout <= s.consecutive_loss_caution {
            return Some(
                "safety.consecutive_loss_dropout must exceed consecutive_loss_caution".into(),
            );
        }
        if s.dropout_duration_hours < 1 || s.dropout_duration_hours > 168 {
            return Some("safety.dropout_duration_hours must be 1–168".into());
        }
        if !f(s.drawdown_limit_pct, 1.0, 100.0) {
            return Some("safety.drawdown_limit_pct must be 1–100".into());
        }
        if !f(s.max_daily_drawdown_pct, 0.1, 50.0) {
            return Some("safety.max_daily_drawdown_pct must be 0.1–50".into());
        }
        if !f(s.systemic_risk_threshold, 0.0, 100.0) {
            return Some("safety.systemic_risk_threshold must be 0–100".into());
        }
    }
    if let Some(rl) = &payload.risk_limits {
        if !f(rl.max_single_pair_exposure_pct, 1.0, 100.0) {
            return Some("risk_limits.max_single_pair_exposure_pct must be 1–100".into());
        }
        if !f(rl.max_portfolio_exposure_pct, 1.0, 100.0) {
            return Some("risk_limits.max_portfolio_exposure_pct must be 1–100".into());
        }
        if !f(rl.max_correlation, 0.0, 1.0) {
            return Some("risk_limits.max_correlation must be 0–1".into());
        }
    }
    if let Some(a) = &payload.analytics {
        if !f(a.alpha, 0.001, 0.5) {
            return Some("analytics.alpha must be 0.001–0.5".into());
        }
        if a.monte_carlo_runs < 100 || a.monte_carlo_runs > 1_000_000 {
            return Some("analytics.monte_carlo_runs must be 100–1,000,000".into());
        }
        if a.min_trades_for_verdict < 1 || a.min_trades_for_verdict > 10_000 {
            return Some("analytics.min_trades_for_verdict must be 1–10,000".into());
        }
    }
    // v8: the BTE depth contract — 1..=365, mirroring the run-form slider.
    if let Some(bt) = &payload.backtest {
        if bt.archive_depth_days < 1 || bt.archive_depth_days > 365 {
            return Some("backtest.archive_depth_days must be 1–365".into());
        }
        if bt.warmup_bars < 30 || bt.warmup_bars > 10_000 {
            return Some("backtest.warmup_bars must be 30–10,000".into());
        }
        if bt.max_equity_points < 10 || bt.max_equity_points > 100_000 {
            return Some("backtest.max_equity_points must be 10–100,000".into());
        }
        for (exchange, limits) in [("hyperliquid", &bt.hyperliquid), ("bitget", &bt.bitget)] {
            if limits.page_cap == 0 || limits.page_cap > 10_000 {
                return Some(format!("backtest.{exchange}.page_cap must be 1–10,000"));
            }
            if limits.max_pages_per_run == 0 || limits.max_pages_per_run > 100_000 {
                return Some(format!(
                    "backtest.{exchange}.max_pages_per_run must be 1–100,000"
                ));
            }
        }
    }
    if let Some(ex) = &payload.execution {
        if !f(ex.slippage_ceiling_pct, 0.0, 5.0) {
            return Some("execution.slippage_ceiling_pct must be 0–5".into());
        }
    }
    None
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    // Reject out-of-range engine-settings values BEFORE touching disk —
    // the dashboards mirror these ranges, so a breach means a stale client.
    if let Some(msg) = validate_ranges(&payload) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid configuration: {}", msg),
        )
            .into_response();
    }

    let runtime_fields_present = payload.minimal_tae.is_some()
        || payload.safety.is_some()
        || payload.risk_limits.is_some()
        || payload.analytics.is_some()
        || payload.execution.is_some()
        || payload.fees.is_some()
        || payload.leverage.is_some()
        || payload.activation.is_some()
        || payload.backtest.is_some()
        // v11.2: the active-timeframe count reshapes every running
        // instance's pipeline set — recharge so the change applies live.
        || payload.active_timeframes.is_some();

    let mut merged = state.workspace.config().await;
    if let Some(candles) = payload.candles {
        merged.candles = candles;
    }
    if let Some(n) = payload.active_timeframes {
        merged.active_timeframes = n;
    }
    if let Some(indicators) = payload.indicators {
        merged.indicators = indicators;
    }
    if let Some(instances) = payload.instances {
        merged.instances = instances;
    }
    if let Some(api_failover) = payload.api_failover {
        merged.api_failover = api_failover;
    }
    if let Some(minimal_tae) = payload.minimal_tae {
        merged.minimal_tae = minimal_tae;
    }
    if let Some(safety) = payload.safety {
        merged.safety = safety;
    }
    if let Some(risk_limits) = payload.risk_limits {
        merged.risk_limits = risk_limits;
    }
    if let Some(analytics) = payload.analytics {
        merged.analytics = analytics;
    }
    if let Some(execution) = payload.execution {
        merged.execution = execution;
    }
    if let Some(fees) = payload.fees {
        merged.fees = fees;
    }
    if let Some(leverage) = payload.leverage {
        merged.leverage = leverage;
    }
    if let Some(activation) = payload.activation {
        merged.activation = activation;
    }
    if let Some(backtest) = payload.backtest {
        merged.backtest = backtest;
    }
    merged.config_version = merged.config_version.saturating_add(1);

    // M8: full numeric validation before persisting — `validate_ranges` above
    // covers engine-settings, but indicator periods / liquidity guards live in
    // `validate_workspace`.
    if let Err(e) = config_models::validate_workspace(&merged) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid configuration: {}", e),
        )
            .into_response();
    }

    // Persist through `save_workspace` (NOT a bare `toml::to_string_pretty`
    // of the WorkspaceConfig): the on-disk shape wraps the workspace in a
    // `[workspace]` table and keeps the platform sections (`[hyperliquid]`,
    // `[bitget]`, `[clock_monitor]`, `[reconnect]`, `[candle_buffer]`,
    // `[snapshot_export]`) intact — a bare write previously produced a file
    // the daemon could not boot from (`load_platform` panics without the
    // required `OnDiskConfig.workspace`).
    if let Err(e) = config_models::save_workspace(&merged) {
        eprintln!(
            "Failed to write configuration updates to config.toml: {}",
            e
        );
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to persist configuration file",
        )
            .into_response();
    }
    state.workspace.set_config(merged).await;

    // v7.4: engine-settings edits (TAE / PME / PAE / Profile) are applied
    // LIVE — recharge every running instance so the executors, safety
    // ladder and sizing read the new values on their next cycle. Recharge
    // is idempotent; failures are logged, never fatal (the config is saved).
    if runtime_fields_present {
        let ctx = state.registry_context();
        let symbols: Vec<String> = state
            .workspace
            .config()
            .await
            .instances
            .iter()
            .filter(|i| i.status == config_models::InstanceStatus::Running)
            .map(|i| i.symbol.clone())
            .collect();
        for symbol in symbols {
            if let Err(e) = portfolio_supervisor::registry::recharge_instance(&ctx, &symbol).await {
                eprintln!(
                    "Config update: pipeline recharge failed for {}: {}",
                    symbol, e
                );
            } else {
                let _ = state
                    .recharge_tx
                    .send(crate::RechargeNotice { pair_key: symbol });
            }
        }
    }

    println!("Configuration Updated: successfully synchronized config.toml dynamically.");
    (
        axum::http::StatusCode::OK,
        "Configuration successfully saved.",
    )
        .into_response()
}

// ─── TOML export/import (config-sharing workflow) ───────────────────

/// Returns the raw `config.toml` file as `text/plain` so the operator can
/// download it and `scp` / upload it to another machine. This is the
/// canonical "share my config" endpoint.
pub async fn serve_workspace_toml(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let raw = std::fs::read_to_string("config.toml")
        .unwrap_or_else(|_| "# config.toml not found on disk".to_string());
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"config.toml\"",
            ),
        ],
        raw,
    )
}

/// Import a raw TOML workspace. The `[workspace]` section is parsed and
/// applied to the running state. **Platform-level fields** (`[hyperliquid]`,
/// `[bitget]`, `[clock_monitor]`) are preserved from the current machine
/// so that exchange URLs are not overwritten by a shared config from
/// another host.
pub async fn serve_workspace_toml_import(
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    let workspace: WorkspaceConfig = match toml::from_str(&body) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Invalid TOML: {}", e),
            )
                .into_response()
        }
    };

    // Validate workspace has required fields.
    if workspace.id.is_empty() || workspace.instances.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid workspace: missing id or instances",
        )
            .into_response();
    }
    if let Err(e) = config_models::validate_workspace(&workspace) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid workspace: {}", e),
        )
            .into_response();
    }

    // Apply the imported workspace to memory.
    state.workspace.set_config(workspace.clone()).await;

    // Persist: read current file, swap workspace table, write back.
    let _ = std::fs::read_to_string("config.toml")
        .ok()
        .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
        .and_then(|disk_val| {
            let mut table = disk_val.as_table()?.clone();
            let ws_toml = toml::to_string_pretty(&workspace)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
                .unwrap_or(toml::Value::Table(toml::Table::new()));
            table.insert("workspace".to_string(), ws_toml);
            toml::to_string_pretty(&toml::Value::Table(table))
                .ok()
                .map(|serialised| std::fs::write("config.toml", serialised))
        });

    println!(
        "Workspace '{}' imported from TOML ({} instances). Platform-level fields preserved.",
        workspace.name,
        workspace.instances.len(),
    );
    (
        axum::http::StatusCode::OK,
        format!(
            "Workspace '{}' imported ({} instances). Platform-level fields preserved. Restart recommended to reconcile instances.",
            workspace.name,
            workspace.instances.len(),
        ),
    ).into_response()
}

/// Path of the indicators rulebook served by `GET /api/rules`. The file
/// must exist in the repo — it is the MME indicators guide spec, mirrored
/// verbatim by `03-02-09-mme-indicators-guide.md`.
pub const RULES_GUIDE_PATH: &str =
    "docs/engines/market-monitoring-engine/03-02-09-mme-indicators-guide.md";

pub async fn serve_get_rules() -> impl IntoResponse {
    match std::fs::read_to_string(RULES_GUIDE_PATH) {
        Ok(content) => Json(RulesResponse { content }).into_response(),
        Err(e) => {
            eprintln!(
                "Failed to read indicators guide ({}): {}",
                RULES_GUIDE_PATH, e
            );
            (
                axum::http::StatusCode::NOT_FOUND,
                "Indicators guide not found",
            )
                .into_response()
        }
    }
}

pub async fn serve_set_rules(Json(payload): Json<SetRulesRequest>) -> impl IntoResponse {
    // M6 (production audit): the rules endpoint serves the MME indicators
    // guide — a git-tracked spec doc. Writing attacker-controlled (or
    // operator-edited) content into it was destructive (and, combined
    // with the pre-K1 CORS posture, let any website overwrite the spec).
    // The guide is now read-only over HTTP; edit it in the repo.
    let _ = payload;
    (
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        "The indicators guide is read-only over HTTP (edit docs/engines/market-monitoring-engine/03-02-09-mme-indicators-guide.md in the repo).",
    )
        .into_response()
}
