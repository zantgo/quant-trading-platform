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

use core_domain::advisory::AdvisoryMatrix;
use core_domain::alignment::AlignmentMatrix;
use core_domain::analysis::AnalysisMatrix;
use core_domain::decision_context::DecisionContext;
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::{ContextDimension, MarketContext};
use core_domain::models::{CandleQualityEnvelope, MarketSnapshot, SequenceIntegrity};
use core_domain::normalized::Exchange;
use core_domain::risk::RiskMatrix;
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

    let decision = DecisionContext {
        score: 45.0,
        bias: "Bullish".into(),
        score_confidence: 0.68,
        entry_danger: core_domain::risk::RiskDimension::from_score(35.0),
        expected_reward_risk_ratio: 2.1,
        trade_readiness: "WATCH".into(),
        contributing_indicators: vec!["rsi".into(), "adx".into()],
        long_probability: 0.0,
        short_probability: 0.0,
        hold_probability: 0.0,
        net_bias_pct: 0.0,
        lean_floor_applied: false,
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
        risk: Some(RiskMatrix::empty("BTC-USDT")),
        advisory: Some(AdvisoryMatrix::empty("BTC-USDT")),
        open_interest: Some(dec!(15000000.0)),
        oi_delta_1h: Some(dec!(500000.0)),
        mark_price: Some(dec!(50002.0)),
        index_price: Some(dec!(50000.0)),
        mark_index_spread_pct: Some(0.004),
        prev_day_px: Some(dec!(49500.0)),
        statistical_context: None,
        decision_context: Some(decision),
        opportunity: None,
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
fn risk_matrix_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.risk.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `RiskMatrix` — includes the Phase-3 rename
    // `execution_liquidity_risk` and `cascade_risk`.
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "market_risk",
            "volatility_risk",
            "execution_liquidity_risk",
            "structure_risk",
            "momentum_risk",
            "signal_risk",
            "execution_risk",
            "cascade_risk",
            "overall_risk",
        ],
        "RiskMatrix",
    );
}

#[test]
fn advisory_matrix_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.advisory.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `AdvisoryMatrix` — 14 required fields (incl.
    // `cascade_risk_score` and the `environment_favorability` band)
    // + the v9 `risk_blocked` ceiling stamp.
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "directional_guidance",
            "market_stance",
            "opportunity_classification",
            "strategy_environment",
            "entry_guidance",
            "exit_guidance",
            "protection_strategy",
            "target_strategy",
            "confidence_assessment",
            "stop_loss_distance_pct",
            "cascade_risk_score",
            "environment_favorability",
            "risk_blocked",
            "final_recommendation",
        ],
        "AdvisoryMatrix",
    );
}

#[test]
fn decision_context_keys_match_frontend_contract() {
    let snap = build_realistic_snapshot();
    let v = serde_json::to_value(snap.decision_context.as_ref().unwrap()).unwrap();
    // `ui/src/types.ts` `DecisionContext`.
    assert_keys_eq(
        &v,
        &[
            "score",
            "bias",
            "score_confidence",
            "entry_danger",
            "expected_reward_risk_ratio",
            "trade_readiness",
            "contributing_indicators",
            "long_probability",
            "short_probability",
            "hold_probability",
            "net_bias_pct",
            "lean_floor_applied",
        ],
        "DecisionContext",
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
        "risk",
        "advisory",
        "decision_context",
        "open_interest",
        "oi_delta_1h",
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
fn opportunity_matrix_keys_match_frontend_contract() {
    let m = core_domain::opportunity::OpportunityMatrix {
        symbol: "BTC-USDT".into(),
        primary_opportunity: core_domain::analysis::OpportunityType::TrendContinuation,
        opportunity_score: 78.0,
        setup_quality: core_domain::analysis::SetupQuality::Strong,
        profiles: vec![],
        forecast_confidence: 0.72,
        contributing_signals: vec!["BULLISH_CROSSOVER".into()],
        invalidation_note: "".into(),
        entry_zone: core_domain::analysis::PriceRange {
            low: 63100.0,
            high: 63400.0,
        },
        target_zone: core_domain::analysis::PriceRange {
            low: 66000.0,
            high: 67000.0,
        },
        invalidation_level: 62800.0,
        long_entry_zone: core_domain::analysis::PriceRange {
            low: 63100.0,
            high: 63400.0,
        },
        long_target_zone: core_domain::analysis::PriceRange {
            low: 66000.0,
            high: 67000.0,
        },
        long_invalidation_level: 62800.0,
        short_entry_zone: core_domain::analysis::PriceRange::default(),
        short_target_zone: core_domain::analysis::PriceRange::default(),
        short_invalidation_level: 0.0,
        long_expected_rr_internal: 2.5,
        short_expected_rr_internal: 0.0,
        long_gross_rr_internal: 2.5,
        short_gross_rr_internal: 0.0,
        time_horizon: "SWING".into(),
        // Populate the skip_serializing_if fields so the wire keys are
        // pinned (a serde rename would otherwise go unnoticed).
        confluent_entry_levels: vec![core_domain::opportunity::ConfluentLevel {
            price: 63300.0,
            confluence_count: 3,
            sources: vec![core_domain::opportunity::LevelSource::Fibonacci],
            strength: 1.0,
            side: Some("LONG".into()),
        }],
        confluent_target_levels: vec![core_domain::opportunity::ConfluentLevel {
            price: 66000.0,
            confluence_count: 2,
            sources: vec![core_domain::opportunity::LevelSource::PivotPoints],
            strength: 0.8,
            side: Some("LONG".into()),
        }],
        confluent_invalidation_levels: vec![core_domain::opportunity::ConfluentLevel {
            price: 62800.0,
            confluence_count: 2,
            sources: vec![core_domain::opportunity::LevelSource::SupportResistance],
            strength: 0.9,
            side: Some("LONG".into()),
        }],
        neutral_reference_bracket: Some(core_domain::opportunity::NeutralBracket {
            entry_zone: core_domain::analysis::PriceRange {
                low: 63100.0,
                high: 63400.0,
            },
            target_zone: core_domain::analysis::PriceRange {
                low: 66000.0,
                high: 67000.0,
            },
            invalidation_level: 62800.0,
            expected_rr_internal: 2.5,
            geometry_consistent: true,
            rationale: "range reference".into(),
        }),
        direction_family: Some(core_domain::analysis::DirectionFamily::TrendRiding),
        long_geometry_consistent: true,
        short_geometry_consistent: false,
    };
    let v = serde_json::to_value(&m).unwrap();
    assert_keys_eq(
        &v,
        &[
            "symbol",
            "primary_opportunity",
            "opportunity_score",
            "setup_quality",
            "profiles",
            "forecast_confidence",
            "contributing_signals",
            "invalidation_note",
            "entry_zone",
            "target_zone",
            "invalidation_level",
            "long_entry_zone",
            "long_target_zone",
            "long_invalidation_level",
            "short_entry_zone",
            "short_target_zone",
            "short_invalidation_level",
            "long_expected_rr_internal",
            "short_expected_rr_internal",
            "long_gross_rr_internal",
            "short_gross_rr_internal",
            "time_horizon",
            "confluent_entry_levels",
            "confluent_target_levels",
            "confluent_invalidation_levels",
            "direction_family",
            "long_geometry_consistent",
            "short_geometry_consistent",
            "neutral_reference_bracket",
        ],
        "OpportunityMatrix",
    );
}

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
