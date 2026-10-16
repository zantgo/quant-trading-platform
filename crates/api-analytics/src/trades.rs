use api_core::types::{
    AddTradeRequest, TradeJournalQuery, TradeLedgerQuery, TradeTelemetryRequest,
    UpdateJournalNotesRequest,
};
use api_core::AppState;
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

pub async fn serve_add_trade(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddTradeRequest>,
) -> impl IntoResponse {
    let outcome_upper = payload.outcome.trim().to_uppercase();
    if outcome_upper != "WIN" && outcome_upper != "LOSS" {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Outcome must be WIN or LOSS",
        )
            .into_response();
    }

    match database_storage::insert_user_trade(
        &state.pool,
        &payload.symbol,
        &payload.direction,
        &outcome_upper,
        payload.risk_multiplier,
        payload.reward_multiplier,
    )
    .await
    {
        Ok(id) => (
            axum::http::StatusCode::CREATED,
            format!("Trade logged with ID {}", id),
        )
            .into_response(),
        Err(e) => {
            eprintln!("Web API Error: Failed to log trade record: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to persist trade record",
            )
                .into_response()
        }
    }
}

pub async fn serve_get_trades(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let trades = database_storage::query_user_trades(&state.pool, 100).await;
    Json(trades)
}

// ─── Trade Journal ───────────────────────────────────────────────

pub async fn serve_trade_journal(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TradeJournalQuery>,
) -> impl IntoResponse {
    // AUDIT-F4: cap the unbounded limit (was `?limit=2_000_000_000`).
    let limit = query.limit.min(api_core::types::API_MAX_LIMIT);
    let records = database_storage::query_trade_journal(&state.pool, limit).await;
    Json(records)
}

pub async fn serve_update_journal_notes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateJournalNotesRequest>,
) -> impl IntoResponse {
    let score = payload.execution_score.clamp(0.0, 10.0);
    let ok =
        database_storage::update_journal_notes(&state.pool, id, &payload.human_notes, score).await;
    if ok {
        (axum::http::StatusCode::OK, "Journal notes updated").into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            "Journal record not found",
        )
            .into_response()
    }
}

/// Neutralize spreadsheet formula injection in CSV output. Any cell whose
/// (trimmed) content starts with `=`, `+`, `-`, `@` — or a tab/CR variant
/// of those — is prefixed with a single quote so Excel/Sheets treat it as
/// text. Also escapes embedded double quotes. Applied to every
/// operator-authored / exchange-sourced string field (AUDIT-H9: notes and
/// analysis are free text; `=HYPERLINK(...)`/`=cmd|...` cells executed
/// when the file was opened).
fn csv_cell(s: &str) -> String {
    let trimmed = s.trim_start();
    let dangerous = trimmed
        .chars()
        .next()
        .map(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'))
        .unwrap_or(false);
    let mut out = String::with_capacity(s.len() + 2);
    if dangerous {
        out.push('\'');
    }
    out.push_str(s);
    out
}

fn csv_quoted(s: &str) -> String {
    format!("\"{}\"", csv_cell(s).replace('"', "\"\""))
}

pub async fn serve_export_journal_csv(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let records = database_storage::query_trade_journal(&state.pool, 1000).await;
    let mut csv = String::from("id,trade_id,entry_date,exit_date,asset,direction,entry_reason,roi_pct,final_analysis,execution_score,human_notes,symbol,realized_pnl,t_roi_pct\n");
    for r in &records {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{:.2},{},{:.1},{},{},{:.2},{:.2}\n",
            r.id,
            r.trade_id,
            r.entry_date,
            r.exit_date,
            csv_quoted(&r.asset),
            csv_quoted(&r.direction),
            csv_quoted(&r.entry_reason),
            r.roi_pct,
            csv_quoted(&r.final_analysis),
            r.execution_score,
            csv_quoted(&r.human_notes),
            csv_quoted(&r.symbol),
            r.realized_pnl,
            r.t_roi_pct,
        ));
    }
    ([(header::CONTENT_TYPE, "text/csv; charset=utf-8")], csv)
}

pub async fn serve_export_journal_json(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let records = database_storage::query_trade_journal(&state.pool, 1000).await;
    Json(records)
}

// ─── Trade Ledger ─────────────────────────────────────────────────

pub async fn serve_trade_ledger(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TradeLedgerQuery>,
) -> impl IntoResponse {
    // AUDIT-F4: cap the unbounded limit.
    let limit = query.limit.min(api_core::types::API_MAX_LIMIT);
    let trades = database_storage::trade_telemetry_query_all(&state.pool, limit).await;
    Json(trades)
}

// ─── Trade Telemetry ──────────────────────────────────────────────

pub async fn serve_trade_telemetry_add(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TradeTelemetryRequest>,
) -> impl IntoResponse {
    let id = database_storage::trade_telemetry_insert(
        &state.pool,
        &payload.exchange,
        &payload.symbol,
        &payload.direction,
        payload.entry_timestamp,
        payload.exit_timestamp,
        payload.entry_price,
        payload.exit_price,
        payload.size,
        payload.commission_fees,
        payload.funding_fees,
        payload.realized_pnl,
        payload.roi_pct,
        &payload.trigger_source,
    )
    .await;
    if id > 0 {
        (
            axum::http::StatusCode::CREATED,
            format!("Trade logged with id {}", id),
        )
            .into_response()
    } else {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to log trade",
        )
            .into_response()
    }
}
