use crate::types::{
    AddInstanceRequest, InstanceConfigPayload, InstanceDetailQuery, InstanceIntervalsRequest,
    InstanceListResponse,
};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};

/// E3: log a single-operator safety audit event (`operator_id = "local"`).
async fn log_risk_event(
    pool: &sqlx::SqlitePool,
    instance_id: &str,
    symbol: &str,
    gate_id: i64,
    decision: &str,
    reason: &str,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    if let Err(e) = sqlx::query(
        "INSERT INTO risk_control_events \
         (instance_id, symbol, gate_id, decision, reason, timestamp_ms, operator_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'local')",
    )
    .bind(instance_id)
    .bind(symbol)
    .bind(gate_id)
    .bind(decision)
    .bind(reason)
    .bind(now)
    .execute(pool)
    .await
    {
        eprintln!("risk event persist failed for {instance_id}: {e}");
    }
}

use portfolio_supervisor::registry;
use rust_decimal_macros::dec;
use std::sync::Arc;

pub async fn serve_start_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    match registry::start_instance(&state.registry_context(), &instance_id).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            format!("Instance {} started", instance_id),
        )
            .into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn serve_list_instances(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InstanceDetailQuery>,
) -> impl IntoResponse {
    let all_summaries = registry::list_instances(&state.registry_context()).await;
    let summaries: Vec<_> = if let Some(ref pk) = query.pair_key {
        all_summaries
            .into_iter()
            .filter(|s| s.pair == *pk)
            .collect()
    } else {
        all_summaries
    };
    Json(InstanceListResponse {
        total_count: summaries.len(),
        instances: summaries,
    })
}

pub async fn serve_add_instance(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddInstanceRequest>,
) -> impl IntoResponse {
    let base = payload.base.trim().to_uppercase();
    let quote = payload.quote.trim().to_uppercase();

    if base.is_empty() || quote.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Base and quote currency required" })),
        )
            .into_response();
    }
    if base.len() > 10 || quote.len() > 10 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Symbol too long" })),
        )
            .into_response();
    }

    match registry::add_instance(&state.registry_context(), (base, quote)).await {
        Ok(instance) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({
                "id": instance.id,
                "pair": instance.pair_display(),
                "message": format!("Instance {} created", instance.pair_display()),
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

pub async fn serve_delete_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    // DELETE accepts any state (Running, Paused, Stopped) — the
    // dashboard UI is binary (instance exists or it doesn't), so we
    // cancel the pipeline, drain the buffers, and remove from the
    // workspace in one shot. The only 4xx we ever emit is 404 if the
    // instance_id is unknown to the workspace.
    match registry::delete_instance(&state.registry_context(), &instance_id).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            format!("Instance {} deleted", instance_id),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

pub async fn serve_delete_instance_by_pair(
    State(state): State<Arc<AppState>>,
    Path(pair_key): Path<String>,
) -> impl IntoResponse {
    let instance_id = state.workspace.get(&pair_key).await.map(|i| i.id.clone());

    match instance_id {
        Some(id) => match registry::delete_instance(&state.registry_context(), &id).await {
            Ok(()) => (
                axum::http::StatusCode::OK,
                format!("Instance {} deleted", pair_key),
            )
                .into_response(),
            Err(e) => (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response(),
        },
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

pub async fn serve_get_instance_detail(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => {
            let status = inst.config_state.read().await.status.as_str().to_string();
            let trading = inst.trading.read().await;
            let losses: u32 = inst.safety.consecutive_losses.read().await.values().sum();
            let safety_state = inst.safety.safety_state.read().await.as_str().to_string();
            Json(serde_json::json!({
                "id": inst.id,
                "pair": inst.pair_display(),
                "symbol": inst.symbol(),
                "status": status,
                "portfolio_capital": trading.portfolio_capital,
                "current_equity": trading.current_equity,
                "consecutive_losses": losses,
                "safety_state": safety_state,
            }))
            .into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

pub async fn serve_update_instance_config(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
    Json(payload): Json<InstanceConfigPayload>,
) -> impl IntoResponse {
    // The route parameter is the instance UUID (e.g. "inst_018e4a6d3f5a1b2c").
    // We no longer fall back to using the path parameter as a pair key so an
    // accidental POST that points at a stale slug returns 404 instead of
    // mutating state on the wrong pair.
    let pair_key = match state.get_instance_by_id(&instance_id).await {
        Some(inst) => inst.pair_key(),
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("Instance {} not found", instance_id),
            )
                .into_response();
        }
    };

    let mut config = state.workspace.config().await;
    let symbol = pair_key.clone();

    // Locate the existing entry by symbol (workspace.instances is a
    // Vec<InstanceEntry>, not a HashMap).
    let mut existing = config
        .instances
        .iter()
        .find(|i| i.symbol == symbol)
        .cloned();
    if existing.is_none() {
        existing = Some(config_models::InstanceEntry {
            id: symbol.clone(),
            symbol: symbol.clone(),
            quote: String::new(),
            status: config_models::InstanceStatus::Running,
            strategy: None,
            timeframes: std::collections::BTreeMap::new(),
            automation: Default::default(),
            operational_mode: Default::default(),
            mode: match state.session.session_mode().await.as_deref() {
                Some("live") => config_models::ExecutionMode::Live,
                Some("paper") => config_models::ExecutionMode::Paper,
                _ => config_models::ExecutionMode::Observe,
            },
            allocation_pct: None,
            weight_overrides: None,
            activation: None,
        });
    }
    let mut entry = existing.expect("entry created above");
    entry.id = instance_id.clone();
    if let Some(timeframes) = payload.timeframes {
        entry.timeframes = timeframes;
    }
    entry.automation = payload.automation.unwrap_or(entry.automation);
    entry.operational_mode = payload
        .operational_mode
        .as_deref()
        .and_then(|s| match s {
            "advisory" | "ManualOnly" => Some(config_models::OperationalMode::Advisory),
            "paper_trading" | "DeterministicHeuristics" => {
                Some(config_models::OperationalMode::PaperTrading)
            }
            "live_trading" => Some(config_models::OperationalMode::LiveTrading),
            _ => None,
        })
        .unwrap_or(entry.operational_mode);
    entry.weight_overrides = payload.weight_overrides.or(entry.weight_overrides);
    entry.activation = payload.activation.or(entry.activation);
    // v9: strategy binding — validated against the registry; the recharge
    // below applies it at the next candle boundary.
    if let Some(strategy) = &payload.strategy {
        let strategy = strategy.clone();
        let workspace_check = state.workspace.config().await;
        if workspace_check.resolve_strategy(&strategy).is_err() {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("strategy '{strategy}' not found"),
            )
                .into_response();
        }
        entry.strategy = Some(strategy);
    }

    // Replace or insert the entry in workspace.instances.
    if let Some(slot) = config.instances.iter_mut().find(|i| i.symbol == symbol) {
        *slot = entry;
    } else {
        config.instances.push(entry);
    }
    if let Err(e) = config_models::save_workspace(&config) {
        eprintln!("⚠️  Failed to persist workspace config: {}", e);
    }
    // Bridge in-memory state: workspace.config() returns a clone, so the
    // mutation above is invisible to subsequent readers until we publish it.
    // Without this the next call (and in particular `recharge_instance`)
    // would see a stale Vec<InstanceEntry> and fail with
    // "No saved config for pair ...", exactly the bug the caller reported.
    state.workspace.set_config(config).await;
    println!(
        "Instance config saved: {} — triggering pipeline recharge",
        pair_key
    );

    match registry::recharge_instance(&state.registry_context(), &pair_key).await {
        Ok(()) => {
            // Notify WS handlers so they re-subscribe to the new
            // `ActivePair`'s broadcast channel. Without this notification
            // any WS handler that had cached the OLD `Arc<ActivePair>`
            // would silently freeze (its `Receiver` would block forever on
            // a channel whose Sender is now kept alive only by the
            // handler itself — no `Closed`, no `onclose`, no reconnect).
            let _ = state.recharge_tx.send(crate::RechargeNotice {
                pair_key: pair_key.clone(),
            });
            (
                axum::http::StatusCode::OK,
                "Instance configuration saved and pipelines recharged",
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Pipeline recharge failed for {}: {}", pair_key, e);
            // AUDIT-F16: the old code returned 200 with a plaintext note —
            // the client could not distinguish saved-but-stale from success,
            // silently diverging persisted vs live config.
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Config saved but pipeline recharge failed: {}", e),
            )
                .into_response()
        }
    }
}

pub async fn serve_pause_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    match registry::pause_instance(&state.registry_context(), &instance_id).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            format!("Instance {} paused", instance_id),
        )
            .into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn serve_stop_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    match registry::stop_instance(&state.registry_context(), &instance_id).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            format!("Instance {} stopped", instance_id),
        )
            .into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn serve_reset_safety(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => {
            inst.safety.reset_consecutive_losses(None).await;
            log_risk_event(
                &state.pool,
                &instance_id,
                &inst.symbol(),
                99,
                "SAFETY_RESET",
                "operator reset consecutive-loss counters",
            )
            .await;
            (
                axum::http::StatusCode::OK,
                format!("Safety counter reset for instance {}", instance_id),
            )
                .into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

// ─── Instance Intervals ───────────────────────────────────────────

pub async fn serve_instance_intervals(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
    Json(payload): Json<InstanceIntervalsRequest>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => {
            let mut config_state = inst.config_state.write().await;
            config_state.intervals.slow_seconds = payload.slow_seconds as u64;
            config_state.intervals.normal_seconds = payload.normal_seconds as u64;
            config_state.intervals.fast_seconds = payload.fast_seconds as u64;

            println!(
                "Instance intervals updated for {} ({}) slow={}s normal={}s fast={}s",
                inst.pair_display(),
                instance_id,
                payload.slow_seconds as u64,
                payload.normal_seconds as u64,
                payload.fast_seconds as u64
            );

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "instance_id": instance_id,
                    "intervals": {
                        "slow_seconds": payload.slow_seconds as u64,
                        "normal_seconds": payload.normal_seconds as u64,
                        "fast_seconds": payload.fast_seconds as u64,
                    },
                })),
            )
                .into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct ReleaseVetoBody {
    #[serde(default)]
    reset_peak: bool,
}

pub async fn serve_release_veto(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
    Json(body): Json<ReleaseVetoBody>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => match inst.safety.release_veto(body.reset_peak).await {
            Ok(()) => {
                log_risk_event(
                    &state.pool,
                    &instance_id,
                    &inst.symbol(),
                    99,
                    "SAFETY_RELEASE",
                    &format!(
                        "operator released safety state (reset_peak={})",
                        body.reset_peak
                    ),
                )
                .await;
                (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!({
                        "success": true,
                        "message": format!("Safety veto released for instance {}", instance_id),
                        "instance_id": instance_id,
                        "reset_peak": body.reset_peak,
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "success": false,
                    "error": e,
                    "message": "Veto condition still active — cannot release"
                })),
            )
                .into_response(),
        },
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

pub async fn serve_get_safety(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => {
            let safety_state = inst.safety.safety_state.read().await.as_str().to_string();
            let losses_map = inst.safety.consecutive_losses.read().await.clone();
            let peak_eq = inst.safety.peak_equity.read().await.to_string();
            let current_eq = inst.trading.read().await.current_equity;
            let initial_cap = inst.trading.read().await.portfolio_capital;
            let context = inst.safety.get_safety_context().await;
            let daily_pnl = inst.safety.daily_pnl.read().await.to_string();
            let equity = state.execution_engine.get_equity_decimal().await;
            let max_drawdown_pct = {
                let peak = *inst.safety.peak_equity.read().await;
                if peak > rust_decimal::Decimal::ZERO {
                    ((dec!(1) - equity / peak) * dec!(100))
                        .max(dec!(0))
                        .to_string()
                } else {
                    "0".to_string()
                }
            };
            let margin_usage = {
                let position = state.execution_engine.get_position(&inst.symbol()).await;
                let leverage = *state.execution_engine.cross_leverage.read().await;
                if let Some(pos) = position {
                    if leverage > 0 {
                        let notional = pos.size * pos.entry_price;
                        (notional / rust_decimal::Decimal::from(leverage) / equity).to_string()
                    } else {
                        "0".to_string()
                    }
                } else {
                    "0".to_string()
                }
            };

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "instance_id": instance_id,
                    "safety_state": safety_state,
                    "consecutive_losses": losses_map,
                    "peak_equity": peak_eq,
                    "current_equity": current_eq,
                    "portfolio_capital": initial_cap,
                    "context": context,
                    "daily_pnl": daily_pnl,
                    "max_drawdown_pct": max_drawdown_pct,
                    "margin_usage_ratio": margin_usage,
                })),
            )
                .into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

pub async fn serve_get_portfolio(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let instance = state.get_instance_by_id(&instance_id).await;

    match instance {
        Some(inst) => {
            let trading = inst.trading.read().await;
            let safety_state = *inst.safety.safety_state.read().await;
            let losses_map = inst.safety.consecutive_losses.read().await.clone();
            let peak_eq = *inst.safety.peak_equity.read().await;
            let daily_pnl = *inst.safety.daily_pnl.read().await;
            let session_eq = *inst.safety.starting_session_equity.read().await;
            let context = inst.safety.get_safety_context().await;

            let symbol = inst.symbol();
            let engine = &state.execution_engine;
            let mid = {
                let guard = inst.fastest_buffer().latest.read().await;
                guard.as_ref().map(|s| s.mid_price).unwrap_or_default()
            };
            let equity = engine.get_equity_decimal().await;

            // Position matrix (mark-to-market) + unrealized PnL.
            let position = engine.get_position(&symbol).await;
            let positions: Vec<core_domain::portfolio::PositionMatrix> =
                if let Some(pos) = &position {
                    let direction = match pos.direction {
                        config_models::Direction::Long => "LONG",
                        config_models::Direction::Short => "SHORT",
                    };
                    vec![
                        portfolio_supervisor::position_layer::compute_position_matrix_with_config(
                            &symbol,
                            direction,
                            pos.entry_price,
                            pos.size,
                            if mid > rust_decimal::Decimal::ZERO {
                                mid
                            } else {
                                pos.entry_price
                            },
                            pos.opened_at_ms,
                            0,
                            engine.fee_config.maker_fee_pct,
                            engine.fee_config.taker_fee_pct,
                        ),
                    ]
                } else {
                    vec![]
                };

            let exposure =
                portfolio_supervisor::exposure_layer::compute_exposure_matrix(&positions, equity);
            // v7.3: the enforced concentration limits come from
            // `[workspace.risk_limits]` — rendered by the PME Exposure tab.
            let risk_limits = {
                let ws = state.workspace.config().await;
                portfolio_supervisor::exposure_layer::ConcentrationLimits::from_config(
                    &ws.risk_limits,
                )
            };
            let capital = portfolio_supervisor::capital_layer::compute_capital_matrix(
                rust_decimal::Decimal::from_f64_retain(trading.portfolio_capital)
                    .unwrap_or_default(),
                dec!(0),
                &positions,
                rust_decimal::Decimal::from(*engine.cross_leverage.read().await),
                session_eq,
                daily_pnl,
                rust_decimal::Decimal::from_f64_retain(inst.safety_config.max_daily_drawdown_pct)
                    .unwrap_or_default(),
            );
            let margin_alert = portfolio_supervisor::capital_layer::check_margin_alerts(
                capital.margin_usage_ratio,
            )
            .map(|a| match a {
                portfolio_supervisor::capital_layer::MarginAlert::Warning => "WARNING",
                portfolio_supervisor::capital_layer::MarginAlert::CloseOnly => "CLOSE_ONLY",
                portfolio_supervisor::capital_layer::MarginAlert::Emergency => "EMERGENCY",
            });

            let peak = peak_eq;
            let max_drawdown_pct = if peak > rust_decimal::Decimal::ZERO {
                ((dec!(1) - equity / peak) * dec!(100)).max(dec!(0))
            } else {
                dec!(0)
            };

            let systemic_risk_score = state
                .overview
                .read()
                .await
                .as_ref()
                .map(|o| o.systemic_risk_score)
                .unwrap_or(0.0);

            let lifecycle_state = {
                let lc = inst.lifecycle.read().await;
                lc.state.as_str().to_string()
            };

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "instance_id": instance_id,
                    "symbol": symbol,
                    "mode": match inst.execution_mode().await {
                        config_models::ExecutionMode::Observe => "observe",
                        config_models::ExecutionMode::Paper => "paper",
                        config_models::ExecutionMode::Live => "live",
                    },
                    "portfolio_capital": trading.portfolio_capital,
                    "current_equity": equity.to_string(),
                    "peak_equity": peak.to_string(),
                    "max_drawdown_pct": max_drawdown_pct.to_string(),
                    "realized_pnl": capital.realized_pnl.to_string(),
                    "unrealized_pnl": capital.unrealized_pnl.to_string(),
                    "daily_pnl": daily_pnl.to_string(),
                    "starting_session_equity": session_eq.to_string(),
                    "safety_state": safety_state.as_str(),
                    "safety_context": context,
                    "consecutive_losses": losses_map,
                    "systemic_risk_score": systemic_risk_score,
                    "lifecycle": lifecycle_state,
                    "exposure": {
                        "gross_exposure": exposure.gross_exposure.to_string(),
                        "net_exposure": exposure.net_exposure.to_string(),
                        "net_exposure_pct": exposure.net_exposure_pct.to_string(),
                        "long_exposure": exposure.long_exposure.to_string(),
                        "short_exposure": exposure.short_exposure.to_string(),
                        "symbol_concentration": exposure.symbol_concentration.iter().map(|(k, v)| (k.clone(), v.to_string())).collect::<std::collections::HashMap<_, _>>(),
                        "max_single_pair_pct": exposure.max_single_pair_pct.to_string(),
                        "limits": {
                            "max_single_pair_exposure_pct": risk_limits.max_single_pair_pct,
                            "max_portfolio_exposure_pct": risk_limits.max_portfolio_pct,
                            "max_correlation": risk_limits.max_correlation,
                        },
                    },
                    "capital": {
                        "available_margin": capital.available_margin.to_string(),
                        "committed_margin": capital.committed_margin.to_string(),
                        "margin_usage_ratio": capital.margin_usage_ratio.to_string(),
                        "leverage_ratio": capital.leverage_ratio.to_string(),
                        "margin_alert": margin_alert,
                    },
                    "position_count": positions.len(),
                    "positions": positions.iter().map(|p| {
                        serde_json::json!({
                            "symbol": p.symbol,
                            "direction": p.direction,
                            "size": p.size.to_string(),
                            "entry_price": p.entry_price.to_string(),
                            "mark_price": p.current_price.to_string(),
                            "unrealized_pnl": p.unrealized_pnl.to_string(),
                            "roi_pct": p.roi_pct.to_string(),
                            "stop_loss_price": p.stop_loss_price.map(|v| v.to_string()),
                            "take_profit_price": p.take_profit_price.map(|v| v.to_string()),
                        })
                    }).collect::<Vec<_>>(),
                })),
            )
                .into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response(),
    }
}

/// GET /api/instances/:id/exposure — Exposure Matrix (informational).
pub async fn serve_get_exposure(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };
    let symbol = inst.symbol();
    let engine = &state.execution_engine;
    let equity = engine.get_equity_decimal().await;
    let mid = {
        let guard = inst.fastest_buffer().latest.read().await;
        guard.as_ref().map(|s| s.mid_price).unwrap_or_default()
    };

    let position = engine.get_position(&symbol).await;
    let positions: Vec<core_domain::portfolio::PositionMatrix> = if let Some(pos) = &position {
        let direction = match pos.direction {
            config_models::Direction::Long => "LONG",
            config_models::Direction::Short => "SHORT",
        };
        vec![
            portfolio_supervisor::position_layer::compute_position_matrix_with_config(
                &symbol,
                direction,
                pos.entry_price,
                pos.size,
                if mid > rust_decimal::Decimal::ZERO {
                    mid
                } else {
                    pos.entry_price
                },
                pos.opened_at_ms,
                0,
                engine.fee_config.maker_fee_pct,
                engine.fee_config.taker_fee_pct,
            ),
        ]
    } else {
        vec![]
    };

    let exposure =
        portfolio_supervisor::exposure_layer::compute_exposure_matrix(&positions, equity);
    // v7.3: the enforced concentration limits from `[workspace.risk_limits]`.
    let risk_limits = {
        let ws = state.workspace.config().await;
        portfolio_supervisor::exposure_layer::ConcentrationLimits::from_config(&ws.risk_limits)
    };

    Json(serde_json::json!({
        "instance_id": instance_id,
        "symbol": symbol,
        "gross_exposure": exposure.gross_exposure.to_string(),
        "net_exposure": exposure.net_exposure.to_string(),
        "net_exposure_pct": exposure.net_exposure_pct.to_string(),
        "long_exposure": exposure.long_exposure.to_string(),
        "short_exposure": exposure.short_exposure.to_string(),
        "symbol_concentration": exposure.symbol_concentration.iter().map(|(k, v)| (k.clone(), v.to_string())).collect::<std::collections::HashMap<_, _>>(),
        "max_single_pair_pct": exposure.max_single_pair_pct.to_string(),
        "limits": {
            "max_single_pair_exposure_pct": risk_limits.max_single_pair_pct,
            "max_portfolio_exposure_pct": risk_limits.max_portfolio_pct,
            "max_correlation": risk_limits.max_correlation,
        },
    }))
    .into_response()
}

/// GET /api/instances/:id/capital — Capital Matrix + margin alert.
pub async fn serve_get_capital(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };
    let symbol = inst.symbol();
    let engine = &state.execution_engine;
    let trading = inst.trading.read().await;

    let position = engine.get_position(&symbol).await;
    let mid = {
        let guard = inst.fastest_buffer().latest.read().await;
        guard.as_ref().map(|s| s.mid_price).unwrap_or_default()
    };
    let positions: Vec<core_domain::portfolio::PositionMatrix> = if let Some(pos) = &position {
        let direction = match pos.direction {
            config_models::Direction::Long => "LONG",
            config_models::Direction::Short => "SHORT",
        };
        vec![
            portfolio_supervisor::position_layer::compute_position_matrix_with_config(
                &symbol,
                direction,
                pos.entry_price,
                pos.size,
                if mid > rust_decimal::Decimal::ZERO {
                    mid
                } else {
                    pos.entry_price
                },
                pos.opened_at_ms,
                0,
                engine.fee_config.maker_fee_pct,
                engine.fee_config.taker_fee_pct,
            ),
        ]
    } else {
        vec![]
    };

    let daily_pnl = *inst.safety.daily_pnl.read().await;
    let session_eq = *inst.safety.starting_session_equity.read().await;
    let capital = portfolio_supervisor::capital_layer::compute_capital_matrix(
        rust_decimal::Decimal::from_f64_retain(trading.portfolio_capital).unwrap_or_default(),
        dec!(0),
        &positions,
        rust_decimal::Decimal::from(*engine.cross_leverage.read().await),
        session_eq,
        daily_pnl,
        rust_decimal::Decimal::from_f64_retain(inst.safety_config.max_daily_drawdown_pct)
            .unwrap_or_default(),
    );
    let margin_alert = portfolio_supervisor::capital_layer::check_margin_alerts(
        capital.margin_usage_ratio,
    )
    .map(|a| match a {
        portfolio_supervisor::capital_layer::MarginAlert::Warning => "WARNING",
        portfolio_supervisor::capital_layer::MarginAlert::CloseOnly => "CLOSE_ONLY",
        portfolio_supervisor::capital_layer::MarginAlert::Emergency => "EMERGENCY",
    });

    Json(serde_json::json!({
        "instance_id": instance_id,
        "symbol": symbol,
        "initial_balance": capital.initial_balance.to_string(),
        "current_equity": capital.current_equity.to_string(),
        "available_margin": capital.available_margin.to_string(),
        "committed_margin": capital.committed_margin.to_string(),
        "realized_pnl": capital.realized_pnl.to_string(),
        "unrealized_pnl": capital.unrealized_pnl.to_string(),
        "margin_usage_ratio": capital.margin_usage_ratio.to_string(),
        "leverage_ratio": capital.leverage_ratio.to_string(),
        "max_daily_drawdown_pct": capital.max_daily_drawdown_pct.to_string(),
        "daily_pnl": capital.daily_pnl.to_string(),
        "margin_alert": margin_alert,
    }))
    .into_response()
}

/// POST /api/instances/:id/safety/session-reset — informational rebaseline.
pub async fn serve_session_reset(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };
    inst.safety.session_reset().await;
    log_risk_event(
        &state.pool,
        &instance_id,
        &inst.symbol(),
        99,
        "SAFETY_SESSION_RESET",
        "operator rebaselined peak equity + daily PnL",
    )
    .await;
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Session reset for {} — peak equity and daily PnL re-baselined", instance_id),
        })),
    )
        .into_response()
}

/// GET /api/instances/:id/automation — the v7 TAE surface.
pub async fn serve_get_automation(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };
    let symbol = inst.symbol();

    let engine = &state.execution_engine;

    // Per-instance execution mode (fixed at launch), not the engine-wide
    // backend. An observe instance carries `ghost: true` so the frontend
    // labels tracked setups / projections as would-be previews.
    let mode = inst.execution_mode().await;
    let ghost = mode == config_models::ExecutionMode::Observe;

    // Tracked setup + executor phase.
    let exec_state = match &state.automation {
        Some(ex) => Some(ex.state(&symbol).await),
        None => None,
    };

    // Orders (entry + bracket) with statuses.
    let orders = engine.orders.read().await;
    let entry_order = exec_state
        .as_ref()
        .and_then(|s| s.entry_order_id.as_ref())
        .and_then(|id| orders.get(id))
        .map(order_json);
    let tp_order = exec_state
        .as_ref()
        .and_then(|s| s.tp_order_id.as_ref())
        .and_then(|id| orders.get(id))
        .map(order_json);
    let sl_order = exec_state
        .as_ref()
        .and_then(|s| s.sl_order_id.as_ref())
        .and_then(|id| orders.get(id))
        .map(order_json);
    drop(orders);

    let position = engine.get_position(&symbol).await;
    let equity = engine.get_equity_decimal().await.to_string();
    let activity = engine.activity_for(&instance_id).await;

    let lifecycle_state = {
        let lc = inst.lifecycle.read().await;
        lc.state.as_str().to_string()
    };
    let safety_state = {
        let st = *inst.safety.safety_state.read().await;
        st.as_str().to_string()
    };
    let safety_blocked = matches!(safety_state.as_str(), "DRAWDOWN_STOP" | "SUSPENDED");

    let invalidation = match exec_state.as_ref().map(|s| s.phase) {
        // A pending entry can still be invalidated (LEVEL price-cross or
        // SIGNAL direction flip); an open position is managed by its
        // TP/SL bracket instead.
        Some(portfolio_supervisor::setup_executor::ExecutorPhase::PendingEntry) => "pending",
        _ => "none",
    };

    let open_positions_count = {
        let positions = engine.positions.read().await;
        positions.len() as u32
    };

    Json(serde_json::json!({
        "instance_id": instance_id,
        "symbol": symbol,
        "mode": match mode {
            config_models::ExecutionMode::Observe => "observe",
            config_models::ExecutionMode::Paper => "paper",
            config_models::ExecutionMode::Live => "live",
        },
        "ghost": ghost,
        "enabled": state.automation.is_some(),
        "phase": exec_state.as_ref().map(|s| phase_str(s.phase)),
        "fingerprint": exec_state.as_ref().map(|s| s.fingerprint.clone()),
        "tracked_setup": exec_state.as_ref().and_then(|s| s.tracked_setup.clone()),
        "projection": exec_state.as_ref().and_then(|s| s.projection.clone()),
        "entry_order": entry_order,
        "bracket": {
            "tp_order": tp_order,
            "sl_order": sl_order,
        },
        "position": position.as_ref().map(|p| {
            serde_json::json!({
                "symbol": p.symbol,
                "direction": match p.direction {
                    config_models::Direction::Long => "LONG",
                    config_models::Direction::Short => "SHORT",
                },
                "size": p.size.to_string(),
                "entry_price": p.entry_price.to_string(),
                "unrealized_pnl": p.unrealized_pnl.to_string(),
            })
        }),
        "invalidation": { "state": invalidation, "detail": "" },
        "activity_log": activity.iter().map(|a| {
            serde_json::json!({
                "ts": a.ts_ms,
                "event": a.event,
                "detail": a.detail,
            })
        }).collect::<Vec<_>>(),
        "safety_gate": {
            "blocked": safety_blocked,
            "reason": if safety_blocked { serde_json::Value::String(safety_state) } else { serde_json::Value::Null },
        },
        "lifecycle": lifecycle_state,
        "equity": equity,
        "open_positions_count": open_positions_count,
    }))
    .into_response()
}

fn order_json(o: &portfolio_supervisor::execution::OrderLifecycle) -> serde_json::Value {
    serde_json::json!({
        "id": o.exchange_order_id,
        "client_order_id": o.packet.client_order_id,
        "side": match o.packet.side {
            config_models::OrderSide::Buy => "BUY",
            config_models::OrderSide::Sell => "SELL",
        },
        "order_type": format!("{:?}", o.packet.order_type),
        "price": o.packet.price.map(|p| p.to_string()),
        "size": o.packet.size.to_string(),
        "status": format!("{:?}", o.status),
        "filled_size": o.filled_size.to_string(),
        "fill_price": o.fill_price.map(|p| p.to_string()),
        "reduce_only": o.packet.reduce_only,
        "created_at": o.created_at,
    })
}

fn phase_str(p: portfolio_supervisor::setup_executor::ExecutorPhase) -> &'static str {
    match p {
        portfolio_supervisor::setup_executor::ExecutorPhase::Idle => "idle",
        portfolio_supervisor::setup_executor::ExecutorPhase::PendingEntry => "pending_entry",
        portfolio_supervisor::setup_executor::ExecutorPhase::PositionOpen => "position_open",
    }
}

/// POST /api/instances/:id/automation/close — manual override: cancels
/// pending/bracket orders and closes the open position at market.
pub async fn serve_automation_close(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };
    let symbol = inst.symbol();

    let mid = {
        let guard = inst.fastest_buffer().latest.read().await;
        guard.as_ref().map(|s| s.mid_price).unwrap_or_default()
    };
    if mid <= rust_decimal::Decimal::ZERO {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "No market data available for manual close",
        )
            .into_response();
    }

    match state
        .execution_engine
        .close_position(&symbol, mid, "manual")
        .await
    {
        Ok(()) => {
            log_risk_event(
                &state.pool,
                &instance_id,
                &symbol,
                99,
                "MANUAL_CLOSE",
                "operator manually closed the position at market",
            )
            .await;
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({ "success": true, "message": format!("Position {} closed at market", symbol) })),
            )
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "error": e })),
        )
            .into_response(),
    }
}

// ─── Activation + reload (V6-212 / V7-310..314) ─────────────────────

/// GET /api/instances/:id/activation — the effective activation set
/// (global `[activation]` ∪ per-instance) at the current config_version.
pub async fn serve_get_activation(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(inst) = state.get_instance_by_id(&instance_id).await else {
        return (axum::http::StatusCode::NOT_FOUND, "Instance not found").into_response();
    };

    let workspace = state.workspace.config().await;
    let instance_cfg = workspace
        .instances
        .iter()
        .find(|i| i.id == instance_id)
        .and_then(|i| i.activation.clone());

    let active = market_analyzer::active_set::ActiveSet::from_config(
        &workspace.activation,
        instance_cfg.as_ref(),
        workspace.config_version,
        workspace.liquidity.enabled,
    );

    let mut signals: Vec<String> = active
        .disabled_signals
        .iter()
        .map(|(kind, name)| {
            if name.is_empty() {
                kind.clone()
            } else {
                format!("{}:{}", kind, name)
            }
        })
        .collect();
    signals.sort();

    let mut indicators: Vec<String> = active.disabled_indicators.iter().cloned().collect();
    indicators.sort();
    let mut kinds: Vec<String> = active.disabled_signal_kinds.iter().cloned().collect();
    kinds.sort();

    Json(serde_json::json!({
        "instance_id": instance_id,
        "symbol": inst.symbol(),
        "config_version": active.config_version,
        "disabled_indicators": indicators,
        "disabled_signals": signals,
        "disabled_signal_kinds": kinds,
        "liquidity": {
            "enabled": active.liquidity_enabled,
            "liquidation_feed": active.liquidation_feed,
            "cluster_estimation": active.cluster_estimation,
            "signals": active.liquidity_signals_enabled,
        },
    }))
    .into_response()
}

/// POST /api/instances/:id/reload?tf=<secs|label|all> — rebuild the
/// instance's pipeline(s) (V7-310…314). `all` (the default) delegates to
/// the full recharge, which rebuilds every ACTIVE duration.
pub async fn serve_reload_timeframe(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
    Query(query): Query<InstanceDetailQuery>,
) -> impl IntoResponse {
    // Resolve the instance UUID to its live pair key (the registry's
    // handle is keyed by the unified symbol, not the UUID).
    let pair_key = match state.get_instance_by_id(&instance_id).await {
        Some(inst) => inst.pair_key(),
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("Instance {} not found", instance_id),
            )
                .into_response();
        }
    };
    let requested = query
        .tf
        .as_deref()
        .or(query.slot.as_deref())
        .unwrap_or("all");
    // v11.9: accept a duration in seconds (`tf=60`) or a duration label
    // (`tf=1m`). Unknown durations are rejected loudly.
    let resolved: String = if requested == "all" {
        "all".to_string()
    } else if let Ok(secs) = requested.parse::<u64>() {
        if !core_domain::is_supported_duration(secs) {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!(
                    "Unknown duration '{}s' (valid: {}|all)",
                    secs,
                    core_domain::SUPPORTED_DURATIONS
                        .iter()
                        .map(|d| core_domain::duration_label(*d))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            )
                .into_response();
        }
        core_domain::duration_label(secs)
    } else if core_domain::SUPPORTED_DURATIONS
        .iter()
        .any(|d| core_domain::duration_label(*d).eq_ignore_ascii_case(requested))
    {
        requested.to_string()
    } else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!(
                "Unknown duration '{}' (valid: {}|all)",
                requested,
                core_domain::SUPPORTED_DURATIONS
                    .iter()
                    .map(|d| core_domain::duration_label(*d))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        )
            .into_response();
    };
    let slot = resolved.as_str();

    if slot == "all" {
        return match portfolio_supervisor::registry::recharge_instance(
            &state.registry_context(),
            &pair_key,
        )
        .await
        {
            Ok(()) => (
                axum::http::StatusCode::OK,
                format!("Instance {} recharged (slot=all)", instance_id),
            )
                .into_response(),
            Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
        };
    }

    match registry::reload_timeframe(&state.registry_context(), &pair_key, slot).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            format!("Instance {} reloaded (slot={})", instance_id, slot),
        )
            .into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// v9 unified lifecycle control — `{ "action": "start" | "pause" |
/// "terminate" }`. Maps onto the existing registry lifecycle functions:
/// pause = close-only (no new entries, pending orders cancelled, open
/// positions managed normally); terminate = stop (cancel all orders +
/// flatten at market, exit_reason "stop_flatten"); start = resume.
pub async fn serve_instance_lifecycle(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let action = payload.get("action").and_then(|a| a.as_str()).unwrap_or("");
    let result = match action {
        "pause" => {
            portfolio_supervisor::registry::pause_instance(&state.registry_context(), &instance_id)
                .await
        }
        "terminate" => {
            portfolio_supervisor::registry::stop_instance(&state.registry_context(), &instance_id)
                .await
        }
        "start" => {
            portfolio_supervisor::registry::start_instance(&state.registry_context(), &instance_id)
                .await
        }
        other => Err(format!(
            "unknown lifecycle action '{other}' — use start | pause | terminate"
        )),
    };
    match result {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "action": action,
                "instance_id": instance_id,
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}
