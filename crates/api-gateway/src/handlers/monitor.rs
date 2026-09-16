use crate::helpers::{default_pair_key, get_active_pair};
use crate::types::{
    HistoryQuery, MonitorResponse, MonitorTimeframe, MtfConfirmation, MtfIndicatorRow,
};
use crate::AppState;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use core_domain::models::MarketSnapshot;
use market_analyzer::indicators::registry::INDICATORS;
use portfolio_supervisor::profile_evaluation::{
    calculate_registry_confluence, evaluate_mtf_alignment, indicator_to_snapshot_values,
    SnapshotValues,
};
use std::sync::Arc;

fn snap_values(s: &Option<MarketSnapshot>) -> Option<SnapshotValues> {
    s.as_ref().map(|m| {
        let price = m.mid_price.to_string().parse::<f64>().unwrap_or(0.0);
        indicator_to_snapshot_values(&m.indicators, price)
    })
}

fn dir_bucket(sv: &SnapshotValues, key: &str) -> i8 {
    if !sv.indicators.contains_key(key) {
        return 0;
    }
    let n = sv.norm(key);
    if n > 0.10 {
        1
    } else if n < -0.10 {
        -1
    } else {
        0
    }
}

fn tf_summary(
    label: &str,
    secs: u64,
    snap: &Option<MarketSnapshot>,
    sv: &Option<SnapshotValues>,
) -> MonitorTimeframe {
    let (regime, overall_score, overall_label) = snap
        .as_ref()
        .and_then(|m| m.context.as_ref())
        .map(|c| (c.regime.clone(), c.overall_score, c.overall_label.clone()))
        .unwrap_or_else(|| ("RANGE".to_string(), 0, "NEUTRAL".to_string()));
    // Bull-bias confluence for display; sign shows net directional pressure.
    let confluence_score = sv
        .as_ref()
        .map(|s| calculate_registry_confluence("BULLISH", s).score)
        .unwrap_or(0);
    MonitorTimeframe {
        label: label.to_string(),
        timeframe_secs: secs,
        regime,
        overall_score,
        overall_label,
        confluence_score,
    }
}

/// GET /api/monitor?symbol= — cross-timeframe meta-intelligence synthesis for
/// the Terminal Monitor: per-timeframe context + confluence, an MTF confirmation
/// matrix, and the macro market-context summary.
pub async fn serve_monitor(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let pair_key = if query.symbol.is_empty() {
        let first = state
            .workspace
            .config()
            .await
            .declared_symbols()
            .first()
            .cloned()
            .unwrap_or_default();
        default_pair_key(&state, &first).await
    } else {
        query.symbol.clone()
    };

    let Some(pair) = get_active_pair(&state, &pair_key).await else {
        return Json(MonitorResponse {
            symbol: pair_key,
            timeframes: vec![],
            mtf: MtfConfirmation {
                trend_agreement_pct: 0.0,
                structural_trend: "NEUTRAL".into(),
                rows: vec![],
            },
            market_context: None,
        })
        .into_response();
    };

    // Fixed 10-slot ladder (fastest → slowest): latest snapshot + derived
    // snapshot values per slot, positionally aligned with
    // `core_domain::models::FIXED_TF_SLOTS`.
    let snaps = pair.latest_snapshots_all_tf().await;
    let svs: Vec<Option<SnapshotValues>> = snaps.iter().map(snap_values).collect();

    // v11.2: report only the ACTIVE ladder slots (fastest N).
    let active = pair.active_count.min(10).max(1);
    let timeframes: Vec<MonitorTimeframe> = core_domain::models::FIXED_TF_SLOTS[..active]
        .iter()
        .zip(snaps[..active].iter())
        .zip(svs[..active].iter())
        .enumerate()
        .map(|(i, ((slot, snap), sv))| {
            let secs = pair
                .pipeline_for_slot(*slot)
                .map(|p| p.timeframe_secs)
                .unwrap_or(config_models::FIXED_TF_LADDER[i]);
            tf_summary(&slot.display_name(), secs, snap, sv)
        })
        .collect();

    // MTF per-indicator agreement matrix (directional registry indicators).
    let empty = SnapshotValues::from_map(Default::default(), 0.0);
    let svs_arr: Vec<&SnapshotValues> =
        svs.iter().map(|sv| sv.as_ref().unwrap_or(&empty)).collect();
    let mut rows: Vec<MtfIndicatorRow> = Vec::new();
    let mut agree_accum = 0.0;
    let mut agree_n = 0.0;
    for meta in INDICATORS {
        if !meta.directional || meta.render == market_analyzer::indicators::RenderKind::Marker {
            continue;
        }
        let per_tf: Vec<i8> = svs_arr.iter().map(|sv| dir_bucket(sv, meta.key)).collect();
        let present: Vec<i8> = per_tf.iter().copied().filter(|&d| d != 0).collect();
        if present.is_empty() {
            continue;
        }
        let bulls = present.iter().filter(|&&d| d > 0).count() as f64;
        let bears = present.iter().filter(|&&d| d < 0).count() as f64;
        let dominant = bulls.max(bears);
        let agreement = dominant / present.len() as f64;
        agree_accum += agreement;
        agree_n += 1.0;
        rows.push(MtfIndicatorRow {
            key: meta.key.to_string(),
            display_name: meta.display_name.to_string(),
            per_tf,
            agreement,
        });
    }
    let trend_agreement_pct = if agree_n > 0.0 {
        (agree_accum / agree_n) * 100.0
    } else {
        0.0
    };

    // Ladder-ordered slice (fastest → slowest): `evaluate_mtf_alignment`
    // reads first (fastest), second, and last (slowest); missing slots
    // degrade to the shared `empty` values exactly as before.
    let mtf_align = evaluate_mtf_alignment(&svs_arr);

    // Macro context preferred; fall back down the ladder toward micro.
    let market_context = snaps
        .iter()
        .rev()
        .find_map(|s| s.as_ref().and_then(|m| m.context.clone()));

    Json(MonitorResponse {
        symbol: pair_key,
        timeframes,
        mtf: MtfConfirmation {
            trend_agreement_pct,
            structural_trend: mtf_align.structural_trend,
            rows,
        },
        market_context,
    })
    .into_response()
}
