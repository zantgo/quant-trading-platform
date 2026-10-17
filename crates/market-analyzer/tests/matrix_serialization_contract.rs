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
        liquidity: dim.clone(),
        regime: "TRENDING".into(),
        overall_score: 50,
        overall_label: "BULLISH".into(),
    };

    MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1700000000,
        symbol: "BTC-USDT".into(),
        is_completed: Some(true),
        mid_price: dec!(50000.0),
        bid_price: dec!(49995.0),
        ask_price: dec!(50005.0),
        bid_size: Some(dec!(1.5)),
        ask_size: Some(dec!(2.0)),
        funding_rate: Some(dec!(0.0001)),
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
        open_interest: Some(dec!(15000000.0)),
        oi_delta_pct: Some(dec!(500000.0)),
        oi_delta_window_secs: None,
        mark_price: Some(dec!(50002.0)),
        index_price: Some(dec!(50000.0)),
        mark_index_spread_pct: Some(0.004),
        prev_day_px: Some(dec!(49500.0)),
        statistical_context: None,
        liquidity_signals: vec![],
        metrics_config: None,
        risk_profile: Some(1),
        liquidity: None,
        cluster: None,
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
#[test]
#[test]
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
        "open_interest",
        "oi_delta_pct",
        "mark_price",
        "index_price",
        "mark_index_spread_pct",
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

// AUDIT-TEST: the liquidity/cluster/volume-profile payloads (the newest,
// most complex matrices) previously had NO serialization key contract —
// a serde rename in those structs would pass CI and break the dashboard
// silently. Each test below pins the exact wire key set the frontend
// (`ui/src/types.ts`) consumes.

#[test]
#[test]
fn liquidity_flow_keys_match_frontend_contract() {
    let mut flow = core_domain::liquidity::LiquidityFlow::default();
    // `recent_real_buckets` is skipped when empty — populate one bucket
    // so the wire shape (string-keyed BTreeMap) is pinned.
    flow.recent_real_buckets.insert(
        42,
        core_domain::liquidity::RealLiquidationBucket {
            bucket_index: 42,
            side: core_domain::normalized::LiquidationSide::Long,
            price_low: 49900.0,
            price_high: 50100.0,
            peak_price: 50000.0,
            notional_usd: 1000.0,
            event_count: 2,
            last_updated_ms: 1700000000000,
        },
    );
    let v = serde_json::to_value(&flow).unwrap();
    assert_keys_eq(
        &v,
        &[
            "long_liquidations_usd",
            "short_liquidations_usd",
            "net_liquidation_usd",
            "event_count",
            "largest_event_usd",
            "largest_event_price",
            "largest_event_side",
            "cascade_state",
            "cascade_intensity",
            "recent_real_buckets",
        ],
        "LiquidityFlow",
    );
    assert_eq!(
        v["recent_real_buckets"]["42"]["side"],
        json!("LONG"),
        "RealLiquidationBucket side serializes SCREAMING_SNAKE_CASE (serde rename_all)"
    );
}

#[test]
fn liquidation_cluster_matrix_keys_match_frontend_contract() {
    let m = core_domain::liquidity::LiquidationClusterMatrix::empty("BTC-USDT", 50000.0);
    let v = serde_json::to_value(&m).unwrap();
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "generated_at_ms",
            "valid_until_ms",
            "mid_price",
            "leverage_assumptions",
            "short_clusters",
            "long_clusters",
            "cascade_asymmetry",
            "total_long_oi_usd",
            "total_short_oi_usd",
            "estimation_confidence",
        ],
        "LiquidationClusterMatrix",
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
