//! `/api/liquidity/cluster-status` endpoint.
//!
//! Returns the per-TF cluster-refresh status snapshot for one pair (or
//! all pairs when `symbol` is omitted). The frontend `LiquidityStatusPanel`
//! polls this every few seconds to render the colored pill next to the
//! LIQ HEATMAP toggle — operators can hover to see the exact skip reason
//! when the heatmap is empty because the refresh task is failing.
//!
//! Without this endpoint, a failing cluster refresh is invisible: the
//! heatmap just stays empty and the operator has no signal that
//! `compute_cluster_for_tf` is returning `NoOpenInterest` /
//! `InsufficientHistory` etc. on every tick.

use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use core_domain::liquidity::ClusterStatusSnapshot;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ClusterStatusParams {
    /// `BTC-USDT` (hyphen, internal symbol). When absent, returns the
    /// status of every active pair.
    pub symbol: Option<String>,
    /// v11.9: a duration label (`1s`, `30s`, `1m`, `15m`, `1h`, …) or raw
    /// seconds. When absent (paired with a symbol), returns every ACTIVE
    /// duration for that pair in one payload.
    pub slot: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ClusterStatusResponse {
    /// Single (symbol, slot) pair — flat object with the snapshot fields.
    Single(ClusterStatusSnapshot),
    /// All slots for one symbol — object keyed by slot name.
    BySymbol {
        symbol: String,
        slots: std::collections::BTreeMap<String, ClusterStatusSnapshot>,
    },
    /// No symbol filter — array of per-symbol payloads.
    All(Vec<SymbolClusterStatus>),
}

#[derive(Debug, Serialize)]
pub struct SymbolClusterStatus {
    pub symbol: String,
    pub slots: std::collections::BTreeMap<String, ClusterStatusSnapshot>,
}

/// Validate the slot query parameter (a supported duration label or raw
/// seconds). Returns the resolved duration in seconds, or `Err` with a
/// stable error string on invalid input.
fn parse_slot(slot: &str) -> Result<u64, String> {
    for &secs in core_domain::SUPPORTED_DURATIONS.iter() {
        if core_domain::duration_label(secs).eq_ignore_ascii_case(slot.trim()) {
            return Ok(secs);
        }
    }
    if let Ok(secs) = slot.trim().parse::<u64>() {
        if core_domain::is_supported_duration(secs) {
            return Ok(secs);
        }
    }
    Err(format!(
        "invalid slot '{}' (expected a supported duration label like 1s|5s|30s|1m|5m|15m|30m|1h|4h|12h|1d, or raw seconds)",
        slot
    ))
}

pub async fn serve_cluster_status(
    Query(params): Query<ClusterStatusParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClusterStatusResponse>, (StatusCode, String)> {
    let pairs = state.workspace.list().await;

    match (params.symbol.as_deref(), params.slot.as_deref()) {
        (Some(symbol), Some(slot)) => {
            let slot = parse_slot(slot).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
            let pair = pairs
                .into_iter()
                .find(|p| p.pair_key() == symbol)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        format!("symbol '{}' not found in workspace", symbol),
                    )
                })?;
            let snap = read_slot_status_secs(&pair, slot).await?;
            Ok(Json(ClusterStatusResponse::Single(snap)))
        }
        (Some(symbol), None) => {
            let pair = pairs
                .into_iter()
                .find(|p| p.pair_key() == symbol)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        format!("symbol '{}' not found in workspace", symbol),
                    )
                })?;
            let mut slots = std::collections::BTreeMap::new();
            for pipe in pair.active_pair.all() {
                slots.insert(
                    pipe.slot_label.clone(),
                    read_slot_status_secs(&pair, pipe.timeframe_secs).await?,
                );
            }
            Ok(Json(ClusterStatusResponse::BySymbol {
                symbol: symbol.to_string(),
                slots,
            }))
        }
        (None, _) => {
            let mut out = Vec::with_capacity(pairs.len());
            for pair in pairs {
                let mut slots = std::collections::BTreeMap::new();
                for pipe in pair.active_pair.all() {
                    slots.insert(
                        pipe.slot_label.clone(),
                        read_slot_status_secs(&pair, pipe.timeframe_secs).await?,
                    );
                }
                out.push(SymbolClusterStatus {
                    symbol: pair.pair_key(),
                    slots,
                });
            }
            Ok(Json(ClusterStatusResponse::All(out)))
        }
    }
}

async fn read_slot_status_secs(
    pair: &Arc<portfolio_supervisor::instance::Instance>,
    tf_secs: u64,
) -> Result<ClusterStatusSnapshot, (StatusCode, String)> {
    let pipe = pair.active_pair.pipeline_for_secs(tf_secs).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "slot '{}' not configured for {}",
                core_domain::duration_label(tf_secs),
                pair.pair_key()
            ),
        )
    })?;
    let guard = pipe.cluster_status.read().await;
    // Derive `Stale` on the fly: a successful refresh whose TTL has
    // elapsed indicates the refresh task has crashed or stalled. The
    // raw handle stores `ttl_remaining_ms` (negative when expired);
    // we surface `Stale` here so operators see a yellow pill instead
    // of a misleading green one. `ttl_remaining_ms` is left as stored
    // (the value at refresh time); recomputing it precisely requires
    // the matrix's `valid_until_ms`, which isn't carried in the
    // snapshot — keeping the value-at-refresh gives a stable,
    // monotonically-decreasing number that operators can correlate
    // with the refresh cadence.
    let mut snap = guard.clone();
    if snap.status == core_domain::liquidity::ClusterRefreshStatus::Ok && snap.ttl_remaining_ms < 0
    {
        snap.status = core_domain::liquidity::ClusterRefreshStatus::Stale;
    }
    Ok(snap)
}
