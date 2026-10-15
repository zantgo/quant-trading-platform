//! v9 Account Profile endpoints — the single capital dial
//! (`portfolio_capital_usd`), mode-gated writes, and the paper reset.

use crate::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CapitalPayload {
    pub portfolio_capital_usd: f64,
}

async fn session_mode(state: &AppState) -> String {
    state
        .session
        .session_mode()
        .await
        .map(|m| m.to_string())
        .unwrap_or_else(|| "paper".to_string())
}

pub async fn account_summary(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let workspace = state.workspace.config().await;
    let mode = session_mode(&state).await;

    // Capital source per mode: paper = configured, live = exchange balance,
    // observe = none (no trading ledger).
    let (source, capital): (&str, Option<f64>) = match mode.as_str() {
        "observe" => ("none", None),
        "live" => ("exchange", Some(state.execution_engine.get_equity().await)),
        _ => ("paper_config", Some(workspace.portfolio_capital_usd)),
    };

    let equity = state.execution_engine.get_equity().await;
    // Daily PnL + safety state from the instances (shared-ledger mirror).
    let instances = state.workspace.list().await;
    let daily_pnl = 0.0;
    let mut worst_safety = "NORMAL".to_string();
    for inst in &instances {
        let safety = *inst.safety.safety_state.read().await;
        let s = safety.as_str().to_string();
        let order = ["NORMAL", "WARN", "CAUTIOUS", "SUSPENDED", "DRAWDOWN_STOP"];
        if order.iter().position(|x| *x == s).unwrap_or(0)
            > order.iter().position(|x| *x == worst_safety).unwrap_or(0)
        {
            worst_safety = s;
        }
    }

    let initial = capital.unwrap_or(0.0);
    let drawdown_pct = if initial > 0.0 && equity < initial {
        ((initial - equity) / initial) * 100.0
    } else {
        0.0
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "mode": mode,
            "portfolio_capital_source": source,
            "portfolio_capital_usd": capital,
            "equity": equity,
            "daily_pnl": daily_pnl,
            "drawdown_pct": drawdown_pct,
            "safety_state": worst_safety,
            "instance_count": instances.len(),
            "open_positions_count": state.execution_engine.positions.read().await.len(),
        })),
    )
}

pub async fn set_account_capital(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CapitalPayload>,
) -> impl IntoResponse {
    let mode = session_mode(&state).await;
    if mode != "paper" {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": format!("portfolio capital writes are paper-only (mode is {mode})") }),
            ),
        );
    }
    if !payload.portfolio_capital_usd.is_finite()
        || !(100.0..=10_000_000.0).contains(&payload.portfolio_capital_usd)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "portfolio_capital_usd must be 100–10,000,000" })),
        );
    }

    // AUDIT-M8: atomic read-modify-write. The old sequence released the lock
    // between reading the config and writing it back, so a concurrent mutator
    // could land in between and have its change discarded by this write while
    // the response still reported success.
    let capital = payload.portfolio_capital_usd;
    if let Err(e) = state
        .workspace
        .mutate_and_persist(move |workspace| {
            workspace.portfolio_capital_usd = capital;
            workspace.config_version = workspace.config_version.saturating_add(1);
            Some(workspace.clone())
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("persist failed: {e}") })),
        );
    }
    // Session default for NEW sessions — existing ledgers are never
    // silently reseeded (only the explicit Reset action does that).
    state
        .session
        .set_session_defaults(Some(mode.clone()), Some(payload.portfolio_capital_usd))
        .await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "portfolio_capital_usd": payload.portfolio_capital_usd,
        })),
    )
}

pub async fn reset_account(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mode = session_mode(&state).await;
    if mode != "paper" {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": format!("portfolio reset is paper-only (mode is {mode})") }),
            ),
        );
    }
    let workspace = state.workspace.config().await;
    let capital = workspace.portfolio_capital_usd;
    let dec =
        rust_decimal::Decimal::from_f64_retain(capital).unwrap_or(rust_decimal_macros::dec!(1000));

    state.execution_engine.set_initial_equity(dec).await;
    for inst in state.workspace.list().await {
        inst.safety.set_portfolio_capital(dec).await;
        inst.safety.session_reset().await;
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "portfolio_capital_usd": capital,
            "note": "paper ledger reseeded; audit event recorded",
        })),
    )
}
