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
    /// `micro1`|`micro2`|`fast1`|`fast2`|`slow1`|`slow2`|`macro1`|`macro2`|
    /// `longterm1`|`longterm2`. When absent (paired with a symbol), returns
    /// all 10 fixed-ladder TF slots for that pair in one payload.
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

/// Validate and normalize the slot query parameter (the fixed 10-slot
/// ladder's canonical wire names). Returns `Err` with a stable error
/// string on invalid input.
fn parse_slot(slot: &str) -> Result<core_domain::models::TimeframeSlot, String> {
    match slot {
        "micro1" | "micro2" | "fast1" | "fast2" | "slow1" | "slow2" | "macro1" | "macro2"
        | "longterm1" | "longterm2" => Ok(core_domain::models::TimeframeSlot::parse(slot)),
        other => Err(format!(
            "invalid slot '{}' (expected one of \
             micro1|micro2|fast1|fast2|slow1|slow2|macro1|macro2|longterm1|longterm2)",
            other
        )),
    }
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
            let snap = read_slot_status(&pair, slot).await?;
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
            for s in core_domain::models::FIXED_TF_SLOTS {
                slots.insert(s.as_str(), read_slot_status(&pair, s).await?);
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
                for s in core_domain::models::FIXED_TF_SLOTS {
                    slots.insert(s.as_str(), read_slot_status(&pair, s).await?);
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

async fn read_slot_status(
    pair: &Arc<portfolio_supervisor::instance::Instance>,
    slot: core_domain::models::TimeframeSlot,
) -> Result<ClusterStatusSnapshot, (StatusCode, String)> {
    let pipe = pair
        .active_pair
        .pipeline_for_slot(slot)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("slot '{}' not configured for {}", slot.as_str(), pair.pair_key()),
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
