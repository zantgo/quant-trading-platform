use crate::types::{SessionInitRequest, SessionStatusResponse};
use crate::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use std::sync::Arc;

pub async fn serve_session_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let active = state
        .session
        .active
        .load(std::sync::atomic::Ordering::Relaxed);
    let currency = *state.session.base_currency.read().await;
    let exchange = *state.session.exchange.read().await;
    let instance_count = state.instance_count().await;
    let mode = state.session.session_mode().await;
    let capital = state.session.session_capital().await;
    // v10: the persisted session number.
    let session_id = *state.session_id.read().await;
    // v11.6 crash recovery: the interrupted session detected at boot.
    let interrupted_info = state.interrupted_session.read().await.clone();

    Json(SessionStatusResponse {
        active,
        currency: currency.map(|c| c.as_str().to_string()),
        exchange: exchange.map(|e| e.as_str().to_string()),
        instance_count,
        mode,
        capital,
        session_id,
        interrupted: interrupted_info.is_some(),
        interrupted_session: interrupted_info.map(|i| crate::types::InterruptedSessionInfoWire {
            id: i.id,
            mode: i.mode,
            exchange: i.exchange,
            currency: i.currency,
            started_at_ms: i.started_at_ms,
            instance_count: i.instance_count,
        }),
    })
}

/// v11.6 crash recovery — RECOVER the interrupted session (activate with
/// the persisted defaults; instances already respawned at boot).
pub async fn serve_session_recover(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.recover_interrupted_session().await {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "message": "Session recovered — instances resumed (paper/live boot PAUSED).",
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "error": e })),
        )
            .into_response(),
    }
}

/// v11.6 crash recovery — DISCARD the interrupted session (instances and
/// settings wiped to defaults; telemetry history kept).
pub async fn serve_session_discard(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.discard_interrupted_session().await {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "message": "Interrupted session discarded — fresh start.",
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "error": e })),
        )
            .into_response(),
    }
}

/// v10: list persisted sessions (newest first).
pub async fn serve_sessions_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match database_storage::queries::sessions::list_sessions(&state.pool).await {
        Ok(rows) => Json(serde_json::json!({
            "sessions": rows
                .iter()
                .map(|r| crate::types::SessionListRow {
                    id: r.id,
                    mode: r.mode.clone(),
                    exchange: r.exchange.clone(),
                    currency: r.currency.clone(),
                    portfolio_capital_usd: r.portfolio_capital_usd,
                    started_at_ms: r.started_at_ms,
                    ended_at_ms: r.ended_at_ms,
                    status: r.status.clone(),
                })
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn serve_session_init(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SessionInitRequest>,
) -> impl IntoResponse {
    let currency = match payload.currency.to_uppercase().as_str() {
        "USDT" => portfolio_supervisor::session::Currency::USDT,
        "USDC" => portfolio_supervisor::session::Currency::USDC,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Invalid currency. Use 'USDT' or 'USDC'.",
                })),
            )
                .into_response()
        }
    };

    let exchange = match payload.exchange.to_lowercase().as_str() {
        "hyperliquid" => portfolio_supervisor::session::ExchangeChoice::Hyperliquid,
        "bitget" => portfolio_supervisor::session::ExchangeChoice::Bitget,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Invalid exchange. Use 'Hyperliquid' or 'Bitget'.",
                })),
            )
                .into_response()
        }
    };

    // v7.1 follow-up: mode + paper capital defaults for created instances.
    let mode = match payload.mode.as_deref() {
        None => None,
        Some("observe") | Some("paper") | Some("live") => Some(payload.mode.clone().unwrap()),
        Some(other) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!(
                        "Invalid mode '{}'. Use 'observe', 'paper' or 'live'.",
                        other
                    ),
                })),
            )
                .into_response();
        }
    };
    if let Some(cap) = payload.portfolio_capital_usd {
        if !cap.is_finite() || cap <= 0.0 {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": "portfolio_capital_usd must be a positive number.",
                })),
            )
                .into_response();
        }
    }

    // Live session requires an active key for the chosen exchange.
    if mode.as_deref() == Some("live") {
        let exchange_name = exchange.as_str();
        let key_row = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM exchange_keys WHERE exchange = ?1 AND is_active = 1",
        )
        .bind(exchange_name)
        .fetch_one(&state.pool)
        .await;
        let key_count = key_row.map(|r| r.0).unwrap_or(0);
        if key_count == 0 {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!(
                        "Live session requires an active {} API key — add one in Settings → Exchange API Keys (with EXCHANGE_SECRET_KEY set).",
                        exchange_name
                    ),
                })),
            )
                .into_response();
        }
    }

    state
        .session
        .set_session_defaults(mode.clone(), payload.portfolio_capital_usd)
        .await;

    match state.init_session(currency, exchange).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "Session initialized successfully.",
                "mode": mode,
                "portfolio_capital_usd": payload.portfolio_capital_usd,
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": e,
            })),
        )
            .into_response(),
    }
}

pub async fn serve_session_quit(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // v10: close the persisted session row before tearing down.
    if let Some(sid) = *state.session_id.read().await {
        let ended = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let _ = database_storage::queries::sessions::close_session(&state.pool, sid, ended).await;
    }
    match state.quit_session().await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "Session terminated. All instances stopped."
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e,
            })),
        )
            .into_response(),
    }
}
