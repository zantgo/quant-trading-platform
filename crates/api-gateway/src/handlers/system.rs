use crate::types::{
    DecisionMemoryBufferRow, ObservabilityBuffersResponse, SystemStatusResponse, WsQuery,
};
use crate::AppState;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

/// GET /api/system/platform-config — the serialized `PlatformConfig`
/// (exchange endpoints, clock monitor, quality, reconnect, candle buffer,
/// snapshot export). DIE Settings renders these real values; the endpoint
/// exists because `GET /api/config` intentionally returns only workspace
/// fields and the platform block lives in `AppState.platform`.
pub async fn serve_platform_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let platform = state.platform.read().await;
    Json(platform.clone())
}

/// GET /api/system/pipelines — per-instance × slot candle-pipeline state
/// (DIE L2 Market Data tab). Reads the live pipeline handles the registry
/// already maintains: state, buffer depth, last completed close, and the
/// reconstructed-candle count for the buffer window.
pub async fn serve_system_pipelines(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let instances = state.get_all_instances().await;
    let mut rows: Vec<serde_json::Value> = Vec::new();

    for inst in &instances {
        let pair = inst.pair_key();
        // Fixed 10-slot ladder (fastest → slowest), positionally aligned
        // with `core_domain::FIXED_TF_SLOTS` / `ActivePair::all()`.
        let pipelines: Vec<(String, &market_analyzer::analyzer::TimeframePipeline)> = {
            let ap = &inst.active_pair;
            // v11.2: report only the ACTIVE ladder slots (fastest N) —
            // slots beyond the count are inert (never spawned).
            let active = inst.active_secs.len().min(10).max(1);
            core_domain::models::FIXED_TF_SLOTS[..active]
                .iter()
                .zip(ap.all()[..active].iter().copied())
                .map(|(slot, pipe)| (slot.as_str(), pipe))
                .collect()
        };
        for (slot_label, pipeline) in pipelines {
            let state_str = {
                let s = pipeline.pipeline_state.read().await;
                format!("{:?}", *s)
            };
            let (buffer_depth, last_close, last_close_ts, reconstructed) = {
                let hist = pipeline.history.read().await;
                let depth = hist.len();
                let mut last: Option<rust_decimal::Decimal> = None;
                let mut last_ts: Option<i64> = None;
                let mut recon: u32 = 0;
                for c in hist.iter() {
                    if c.reconstructed.is_some() {
                        recon += 1;
                    }
                    last = Some(c.close);
                    last_ts = Some(c.start_time_ms as i64);
                }
                (
                    depth,
                    last.map(|d| d.to_string()).unwrap_or_default(),
                    last_ts.unwrap_or(0),
                    recon,
                )
            };
            // Canonical wire identifier ("micro1".."longterm2") — the
            // label came from `FIXED_TF_SLOTS`, the pipeline's own slot
            // matches it for all ten fixed pipelines.
            let slot_key = slot_label.as_str();
            rows.push(serde_json::json!({
                "pair": pair,
                "slot": slot_key,
                "timeframe_secs": pipeline.timeframe_secs,
                "pipeline_state": state_str.to_uppercase(),
                "buffer_depth": buffer_depth,
                "buffer_size": pipeline.buffer_size,
                "last_completed_close": last_close,
                "last_completed_ts": last_close_ts,
                "reconstructed_candles": reconstructed,
            }));
        }
    }

    Json(serde_json::json!({ "pipelines": rows }))
}

/// GET /api/system/distribution — DIE L4 egress telemetry: the latency
/// snapshot (already computed by `LatencyTracker`) plus the connected
/// WebSocket client count maintained by the ws module.
pub async fn serve_system_distribution(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let lat = state.latency_tracker.snapshot();
    Json(serde_json::json!({
        "observation_loop_latency_ms": lat.observation_loop_latency_ms,
        "ingest_skew_ms": lat.ingest_skew_ms,
        "system_heartbeat_latency_ms": lat.system_heartbeat_latency_ms,
        "ws_clients_connected": crate::ws::connected_client_count(),
    }))
}

pub async fn serve_system_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let active_pairs_count = state.instance_count().await;
    let lat = state.latency_tracker.snapshot();

    let report = state.exchange_status.report().await;
    let connected = report.exchanges.iter().any(|e| {
        matches!(
            e.state,
            network_adapters::exchange_status_tracker::ExchangeConnectionState::Connected
        )
    });

    let total_allocated_margin = state
        .execution_engine
        .committed_margin()
        .await
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);

    let response = SystemStatusResponse {
        connected,
        latency_ms: lat.observation_loop_latency_ms,
        ingest_skew_ms: lat.ingest_skew_ms,
        observation_loop_latency_ms: lat.observation_loop_latency_ms,
        system_heartbeat_latency_ms: lat.system_heartbeat_latency_ms,
        journal_mode: "WAL".to_string(),
        total_allocated_margin,
        active_pairs_count,
    };

    Json(response)
}

pub async fn serve_observability_buffers(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    let symbol = if query.symbol.is_empty() {
        let _cfg = state.platform.read().await;
        state
            .workspace
            .config()
            .await
            .declared_symbols()
            .first()
            .cloned()
            .unwrap_or_default()
    } else {
        query.symbol
    };
    let raw_symbol = symbol
        .split_once(':')
        .map(|(_, s)| s)
        .unwrap_or(&symbol)
        .to_string();

    let overview = state.overview.read().await;
    let recent_decisions: Vec<DecisionMemoryBufferRow> = overview
        .as_ref()
        .map(|o| {
            o.asset_ranking
                .iter()
                .take(10)
                .map(|rank| DecisionMemoryBufferRow {
                    id: 0,
                    symbol: rank.symbol.clone(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    regime_classification: rank.regime.clone(),
                    orchestrator_decision: rank.bias.clone(),
                    confidence_score: rank.confidence,
                    eight_factor_score: (rank.score * 8.0) as i32,
                    portfolio_risk_pct: 0.0,
                })
                .collect()
        })
        .unwrap_or_default();

    let completed_trades: Vec<crate::types::CompletedTradesBufferRow> = sqlx::query_as(
        "SELECT \
            t.id, t.symbol, t.direction, t.entry_price, t.exit_price, \
            t.realized_pnl, t.roi_pct, \
            COALESCE(j.execution_score, 0.0) as execution_score, \
            COALESCE(j.final_analysis, '') as primary_mistake, \
            t.exit_timestamp as closed_at \
         FROM trade_telemetry_history t \
         LEFT JOIN trade_learning_journal j ON t.id = j.trade_id \
         WHERE t.symbol = ?1 \
         ORDER BY t.exit_timestamp DESC \
         LIMIT 5",
    )
    .bind(&raw_symbol)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Json(ObservabilityBuffersResponse {
        symbol: raw_symbol,
        recent_decisions,
        completed_trades,
    })
}
