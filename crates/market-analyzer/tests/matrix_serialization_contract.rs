//! Wire-contract key parity (Phase 2 of the final verification).
//!
//! Every matrix the MME dashboard consumes is serialized to JSON with
//! snake_case keys — the frontend store (`ui/src/types.ts`) parses these
//! verbatim. This test locks the exact key sets so a struct refactor or a
//! new serde rename can never silently drift the wire shape the
//! dashboards and export builders depend on.
//!
//! Expected key lists are derived from `ui/src/types.ts` interfaces and
//! the `docs/ui-ux/07-05-export-data-payload-schema.md` payload schema.

use std::collections::HashMap;

use core_domain::alignment::AlignmentMatrix;
use core_domain::analysis::AnalysisMatrix;
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::{ContextDimension, MarketContext};
use core_domain::models::{CandleQualityEnvelope, MarketSnapshot, SequenceIntegrity};
use core_domain::normalized::Exchange;
use rust_decimal_macros::dec;
use serde_json::{json, Value};

fn neutral_dimension() -> ContextDimension {
    ContextDimension {
        score: 0.0,
        confidence: 0.0,
        label: "NEUTRAL".into(),
    }
}

fn build_realistic_snapshot() -> MarketSnapshot {
    let mut indicators: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
    indicators.insert(
        "rsi".to_string(),
        NormalizedIndicatorValue::scalar(62.4, 0.24, "BULLISH"),
    );
    indicators.insert(
        "adx".to_string(),
        NormalizedIndicatorValue::scalar(34.99, 0.7, "STRONG_BULL_TREND"),
    );

    let dim = neutral_dimension();
    let context = MarketContext {
        trend: dim.clone(),
        momentum: dim.clone(),
        volatility: dim.clone(),
        volume: dim.clone(),
        // `MarketContext.liquidity` is a ContextDimension — NOT the deleted
        // `MarketSnapshot.liquidity` flow. Same name, different type.
        liquidity: dim.clone(),
        regime: "TRENDING".into(),
        overall_score: 50,
        overall_label: "BULLISH".into(),
    };

    MarketSnapshot {
        // CR-10: populated, not `None` — a skipped-serialization field proves
        // nothing about its wire shape, and `state_bands` is the one key this
        // purge ADDED. Its inner key set is pinned below.
        state_bands: Some(core_domain::state_bands::StateBandMatrix::empty(
            "BTC-USDT", "60s", 60, 50_000.0,
        )),
        timeframe_label: Some("1s".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1700000000,
        symbol: "BTC-USDT".into(),
        is_completed: Some(true),
        mid_price: dec!(50000.0),
        bid_price: dec!(49995.0),
        ask_price: dec!(50005.0),
        open: Some(dec!(49800.0)),
        high: Some(dec!(50200.0)),
        low: Some(dec!(49750.0)),
        close: Some(dec!(50000.0)),
        volume: Some(dec!(100.0)),
        average_volume: Some(dec!(85.0)),
        indicators,
        context: Some(context),
        alignment: Some(AlignmentMatrix::empty("BTC-USDT")),
        analysis: Some(AnalysisMatrix::empty("BTC-USDT")),
        prev_day_px: Some(dec!(49500.0)),
        statistical_context: None,
        metrics_config: None,
        risk_profile: Some(1),
        volume_profile: None,
        quality_envelope: Some(CandleQualityEnvelope {
            quality_score: 95.0,
            is_valid: true,
            is_gap_filled: false,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: SequenceIntegrity::Valid,
            gap_since_last: 60,
            validated_at: 1700000000000,
        }),
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    }
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
    keys.sort();
    keys
}

fn assert_keys_eq(v: &Value, expected: &[&str], what: &str) {
    let mut exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    exp.sort();
    assert_eq!(
        sorted_keys(v),
        exp,
        "{what} JSON keys must match the wire contract"
    );
}

#[test]
fn alignment_matrix_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.alignment.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `AlignmentMatrix` (12 fields + v6.10.16
    // `blend_weights`).
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "timeframes_present",
            "dimensions",
            "mtf_trend_alignment",
            "mtf_momentum_alignment",
            "mtf_volume_alignment",
            "mtf_volatility_alignment",
            "mtf_overall_score",
            "mtf_overall_label",
            "blend_weights",
            "timeframe_alignments",
            "signal_cross_tf_count",
            "trend_agreement_pct",
        ],
        "AlignmentMatrix",
    );
    let row = &v["timeframe_alignments"];
    assert!(row.is_array() && row.as_array().map(|a| a.is_empty()).unwrap_or(false));
}

#[test]
fn analysis_matrix_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.analysis.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `AnalysisMatrix` — the full wire shape (v9 F-03:
    // `opportunity_analysis` erased — the classification is L4-owned).
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "bias",
            "market_bias_score",
            "state_confidence",
            "confidence",
            "market_regime",
            "trend_assessment",
            "momentum_assessment",
            "structure_assessment",
            "volatility_assessment",
            "volume_assessment",
            "market_quality",
            "market_quality_score",
            "market_phase",
            "market_interpretation",
            "rationale",
            "supporting_signals",
            "contradicting_signals",
            "timeframes_considered",
        ],
        "AnalysisMatrix",
    );
}

#[test]
fn market_context_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.context.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `MarketContext` — 5 dimensions + regime + score + label.
    assert_keys_eq(
        &v,
        &[
            "trend",
            "momentum",
            "volatility",
            "volume",
            "liquidity",
            "regime",
            "overall_score",
            "overall_label",
        ],
        "MarketContext",
    );
}

#[test]
fn snapshot_top_level_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(&snap).unwrap();
    // Top-level `MarketSnapshot` keys consumed by the store
    // (`ui/src/lib/websocket.svelte.ts`) and the metrics/MTF builders.
    let keys = sorted_keys(&v);
    for required in [
        "timeframe_label",
        "exchange",
        "timeframe_secs",
        "timestamp",
        "symbol",
        "is_completed",
        "mid_price",
        "bid_price",
        "ask_price",
        "open",
        "high",
        "low",
        "close",
        "volume",
        "indicators",
        "context",
        "alignment",
        "analysis",
        "prev_day_px",
        "quality_envelope",
        "pipeline_state",
        "indicator_lifecycle",
    ] {
        assert!(
            keys.contains(&required.to_string()),
            "MarketSnapshot must carry `{required}` on the wire"
        );
    }
    // CR-15: the four strategy keys must be GONE from the wire, not merely
    // unused. A field left in place but always null is worse than an absent
    // one: every consumer has to decide what "null strategy" means, and the
    // dashboard would render it as a real reading of "no opportunity".
    // CR-10: the nine non-candle envelope keys are asserted ABSENT for the
    // same reason the four strategy keys are — a field left in place but always
    // null is worse than an absent one. This is the wire half of CH-1: the
    // model boundary is not just "we don't read them", it is "they are not
    // published".
    // CR-10: `state_bands` is the one ADDED top-level key — the candle-derived
    // replacement for the cluster matrix. Asserted present, then pinned by
    // `state_band_keys_match_frontend_contract` below.
    assert!(
        keys.contains(&"state_bands".to_string()),
        "MarketSnapshot must carry `state_bands` on the wire"
    );
    for removed in [
        "open_interest",
        "oi_delta_pct",
        "oi_delta_window_secs",
        "mark_price",
        "index_price",
        "mark_index_spread_pct",
        "funding_rate",
        "liquidity",
        "liquidity_signals",
        "cluster",
        "bid_size",
        "ask_size",
    ] {
        assert!(
            !keys.contains(&removed.to_string()),
            "CR-10: `{removed}` must not appear on the MarketSnapshot wire"
        );
    }
    for removed in ["risk", "advisory", "opportunity", "decision_context"] {
        assert!(
            !keys.contains(&removed.to_string()),
            "CR-15: `{removed}` must not appear on the MarketSnapshot wire"
        );
    }
    // `liquidity_signals` is skipped when empty
    // (`#[serde(skip_serializing_if = "Vec::is_empty")]` in models.rs) —
    // when present it must be an array.
    if keys.contains(&"liquidity_signals".to_string()) {
        assert!(v["liquidity_signals"].is_array());
    }
    // The export builders read `snapshot.context` blocks verbatim — the
    // JSON here must be object-shaped, not a string.
    assert!(
        v["context"].is_object(),
        "context must serialize as an object"
    );
    assert!(
        v["indicators"].is_object(),
        "indicators must serialize as an object"
    );
    assert_eq!(v["symbol"], json!("BTC-USDT"));
}

// AUDIT-TEST: the candle-derived band payload (the newest derived matrix)
// previously had NO serialization key contract — a serde rename would pass CI
// and break the dashboard silently. The test below pins the exact wire key set
// the frontend (`ui/src/types.ts`) consumes.
//
// CR-10 REPLACED this block. It pinned `LiquidityFlow` and
// `LiquidationClusterMatrix`, both of which are deleted — they were built from
// the venue's forced-close stream and from open interest under an assumed
// leverage distribution. The band set that took their place derives from
// candle structure alone, and its key set is pinned here instead.
#[test]
fn state_band_keys_match_frontend_contract() {
    let mut m = core_domain::state_bands::StateBandMatrix::empty("BTC-USDT", "60s", 60, 50_000.0);
    m.has_profile = true;
    m.has_bands = true;
    m.bands.push(core_domain::state_bands::StateBand {
        price_low: 49_900.0,
        price_high: 50_100.0,
        band_atr_multiple: 0.5,
        volume_percentile: 90.0,
        proximity_percentile: 40.0,
        intensity_pct: 36.0,
        basis: core_domain::state_bands::BandBasis::HighVolumeNode,
        volume_asymmetry: 0.12,
    });
    let v = serde_json::to_value(&m).unwrap();
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "timeframe_label",
            "timeframe_secs",
            "poc_price",
            "value_area_high",
            "value_area_low",
            "has_profile",
            "has_bands",
            "bands",
            "summary",
            "timestamp_ms",
        ],
        "StateBandMatrix",
    );
    let band = v["bands"]
        .as_array()
        .expect("bands is an array")
        .first()
        .expect("one band pushed");
    assert_keys_eq(
        band,
        &[
            "price_low",
            "price_high",
            "band_atr_multiple",
            "volume_percentile",
            "proximity_percentile",
            "intensity_pct",
            "basis",
            "volume_asymmetry",
        ],
        "StateBand",
    );
}

#[test]
fn volume_profile_snapshot_keys_match_frontend_contract() {
    let vp =
        core_domain::volume_profile::VolumeProfileSnapshot::empty("BTC-USDT", "micro", 60, 50000.0);
    let v = serde_json::to_value(&vp).unwrap();
    // No `mid_price` / `buy_volume` / `sell_volume` on the wire — the
    // buy/sell split is derived from the per-bin sides; `timestamp_ms`
    // is the bucket close timestamp.
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "timeframe_label",
            "timeframe_secs",
            "timestamp_ms",
            "bins",
            "num_bins",
            "poc_price",
            "value_area_high",
            "value_area_low",
            "range_low",
            "range_high",
            "total_volume",
        ],
        "VolumeProfileSnapshot",
    );
}
