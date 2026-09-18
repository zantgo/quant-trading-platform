//! # Market Context Synthesis Implementation
//!
//! The `MarketContext` DTO lives in `core-domain`; this module holds the
//! `synthesize` constructor and its helpers because they need access to the
//! indicator registry (`INDICATORS`) to group indicators by functional category.
//!
//! v9: the strategy's `l1` section drives the synthesis knobs
//! (`indicator_weights`, `monitor_only`, `trend_momentum_blend`,
//! `regime_gate_damp`, `regime_rule`, `volatility_sources`). `None` (or the
//! default strategy) reproduces the v8.2 output byte-for-byte.

use crate::indicators::registry::{IndicatorGroup, INDICATORS};
use config_models::L1Params;
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::{ContextDimension, MarketContext};
use std::collections::HashMap;

fn dir_label(
    score: f64,
    strong: &str,
    weak: &str,
    bear_strong: &str,
    bear_weak: &str,
    neutral: &str,
) -> String {
    if score >= 0.6 {
        strong
    } else if score >= 0.15 {
        weak
    } else if score <= -0.6 {
        bear_strong
    } else if score <= -0.15 {
        bear_weak
    } else {
        neutral
    }
    .to_string()
}

/// The strategy's per-indicator trust weight (0–5, default 1.0 = today) or
/// `None` when the key is `monitor_only` (compute + display + signals, but
/// zero contribution to MarketContext/Alignment). No strategy = all keys at
/// weight 1.0 (v8.2 behavior).
fn l1_weight(l1: Option<&L1Params>, key: &str) -> Option<f64> {
    let Some(l1) = l1 else {
        return Some(1.0);
    };
    if l1.monitor_only.iter().any(|k| k == key) {
        return None;
    }
    Some(l1.indicator_weights.get(key).copied().unwrap_or(1.0))
}

/// Aggregate the enabled directional indicators of a functional group into a
/// weighted-mean signed score + mean confidence.
fn group_dimension(
    map: &HashMap<String, NormalizedIndicatorValue>,
    group: IndicatorGroup,
    directional_only: bool,
    l1: Option<&L1Params>,
) -> ContextDimension {
    let mut sum = 0.0;
    let mut conf = 0.0;
    let mut n = 0.0;
    for meta in INDICATORS {
        if meta.group != group {
            continue;
        }
        if directional_only && !meta.directional {
            continue;
        }
        let Some(v) = map.get(meta.key) else {
            continue;
        };
        // v9: `monitor_only` mutes the key entirely; `indicator_weights`
        // scales the contribution.
        let Some(w) = l1_weight(l1, meta.key) else {
            continue;
        };
        // Guarded: `normalized` is clamp_unit-sanitized, but a NaN
        // `confidence` from any future calculator would poison the
        // weighted mean — collapse non-finite entries to neutral.
        let norm_i = finite(v.normalized, 0.0);
        let conf_i = finite(v.confidence, 0.0);
        sum += norm_i * conf_i * w;
        conf += conf_i;
        n += 1.0;
    }
    if n < f64::EPSILON {
        return ContextDimension::neutral();
    }
    // AUDIT-AIU-060: the docs (02-07 §5.1) specify a confidence-WEIGHTED
    // mean `Σ(score×confidence)/Σ(confidence)`. The previous code computed
    // `Σ(score×confidence)/n` — the mean of the products — which shrank
    // every score toward zero as confidence dropped. The weighted mean is
    // invariant to confidence scale: an indicator with confidence 0.9
    // contributes 9× the weight of one with confidence 0.1.
    let score = if conf > 1e-9 { sum / conf } else { 0.0 };
    let conf = conf / n;
    let label = dir_label(
        score,
        "STRONG_BULL",
        "WEAK_BULL",
        "STRONG_BEAR",
        "WEAK_BEAR",
        "NEUTRAL",
    );
    ContextDimension {
        score,
        confidence: conf,
        label,
    }
}

/// Collapse non-finite indicator inputs to a safe fallback. `f64::clamp`
/// passes NaN through (and NaN comparisons are all false), so every raw
/// value entering the synthesis is sanitized first — one NaN anywhere
/// would otherwise poison the dimension scores, confidence fields and
/// `overall_score` on the wire (serde_json emits NaN as `null`).
fn finite(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Synthesize the context from a normalized indicator map.
///
/// Lives in `market-analyzer` because it reads `INDICATORS` to group
/// contributions by functional category.
///
/// v9: `l1` (the strategy's L1 section) drives weights, monitor-only keys,
/// the regime rule, the regime gate, the directional blend and the
/// volatility-dimension source mix. `None` = the v8.2 defaults.
pub fn synthesize_market_context(
    map: &HashMap<String, NormalizedIndicatorValue>,
    l1: Option<&L1Params>,
) -> MarketContext {
    let trend = group_dimension(map, IndicatorGroup::Trend, true, l1);
    let momentum = group_dimension(map, IndicatorGroup::Momentum, true, l1);

    // Volatility: magnitude from BBWP/HV (expansion vs compression), non-directional.
    // v9: the source blend is the strategy's `l1.context.volatility_sources`
    // (bbwp / hv / atr_pct weights; default bbwp-only = v8.2).
    let vol_sources = l1
        .map(|c| &c.context.volatility_sources)
        .cloned()
        .unwrap_or_default();
    let w_bbwp = vol_sources.bbwp;
    let w_hv = vol_sources.hv;
    let w_atr = vol_sources.atr_pct;
    let w_sum = (w_bbwp + w_hv + w_atr).max(1e-9);
    let bbwp = finite(map.get("bbwp").map(|v| v.raw_value).unwrap_or(50.0), 50.0);
    let src_bbwp = ((bbwp - 50.0) / 50.0).clamp(-1.0, 1.0);
    let src_hv = finite(map.get("hv").map(|v| v.normalized).unwrap_or(0.0), 0.0).abs();
    let src_atr = finite(map.get("atr").map(|v| v.normalized).unwrap_or(0.0), 0.0).abs();
    let vol_score =
        ((src_bbwp * w_bbwp + src_hv * w_hv + src_atr * w_atr) / w_sum).clamp(-1.0, 1.0);
    let conf_bbwp = (bbwp / 100.0).clamp(0.0, 1.0);
    let vol_confidence =
        ((conf_bbwp * w_bbwp + src_hv * w_hv + src_atr * w_atr) / w_sum).clamp(0.0, 1.0);
    let volatility = ContextDimension {
        score: vol_score,
        confidence: vol_confidence,
        label: if bbwp >= 90.0 {
            "EXPANSION_CLIMAX".into()
        } else if bbwp >= 60.0 {
            "EXPANDING".into()
        } else if bbwp <= 10.0 {
            "MAX_COMPRESSION".into()
        } else if bbwp <= 30.0 {
            "CONTRACTING".into()
        } else {
            "NORMAL".into()
        },
    };

    // Volume/participation: RVOL magnitude gate.
    let rvol = finite(map.get("rvol").map(|v| v.raw_value).unwrap_or(1.0), 1.0);
    let volume = ContextDimension {
        score: (rvol - 1.0).clamp(-1.0, 1.0),
        confidence: (rvol / 3.0).clamp(0.0, 1.0),
        label: if rvol >= 3.0 {
            "CLIMACTIC".into()
        } else if rvol >= 1.5 {
            "HIGH".into()
        } else if rvol < 0.7 {
            "THIN".into()
        } else {
            "NORMAL".into()
        },
    };

    // Liquidity proxy: VWAP proximity + volume participation.
    //
    // AUDIT-AIU-061: the previous code scored the VWAP contribution as
    // `vwap_score × 50` where `vwap_score = vwap.normalized` — a SIGNED
    // premium/discount reading. The comment claimed "near fair value →
    // higher liquidity", but `confidence = |normalized|` means high
    // confidence = FAR from fair value, so the mapping contradicted its own
    // intent (extreme premium −0.8 → −40 liquidity; equilibrium → 0).
    // The corrected contribution is proximity-based: `(1 − |normalized|)`
    // so price AT fair value contributes +50 and a stretched price
    // contributes ~0.
    let vwap_conf = finite(map.get("vwap").map(|v| v.confidence).unwrap_or(0.0), 0.0);
    let vwap_score = finite(map.get("vwap").map(|v| v.normalized).unwrap_or(0.0), 0.0);
    // AUDIT-AIU-111: `ContextDimension.score` is contractually [-1, 1]
    // (`core-domain/src/market_context.rs`, `02-07-metrics-matrix.md` §5.2)
    // — the previous ±100 clamp emitted `+40.00` where every sibling
    // dimension rendered `+0.40`, and any consumer doing scale math on the
    // dimension would be off by 100×. The two contributions are now
    // normalized to half-widths that sum to [-1, 1].
    let rvol_contrib = ((rvol - 1.0) * 0.5).clamp(-0.5, 0.5);
    let vwap_contrib = ((1.0 - vwap_score.abs()).clamp(0.0, 1.0)) * 0.5;
    let liquidity_score = (rvol_contrib + vwap_contrib).clamp(-1.0, 1.0);
    let liquidity = ContextDimension {
        score: liquidity_score,
        confidence: ((vwap_conf + volume.confidence) / 2.0).clamp(0.0, 1.0),
        label: if rvol >= 1.2 {
            "GOOD".into()
        } else if rvol < 0.6 {
            "LOW".into()
        } else {
            "ADEQUATE".into()
        },
    };

    // Regime from ADX strength + BBWP compression + trend agreement.
    // v9: the thresholds come from `l1.context.regime_rule`.
    let regime_rule = l1
        .map(|c| &c.context.regime_rule)
        .cloned()
        .unwrap_or_default();
    let adx = finite(map.get("adx").map(|v| v.raw_value).unwrap_or(0.0), 0.0);
    let chop = finite(
        map.get("choppiness").map(|v| v.raw_value).unwrap_or(50.0),
        50.0,
    );
    let regime = if bbwp <= regime_rule.bbwp_compression || chop >= regime_rule.chop_compression {
        "COMPRESSION"
    } else if bbwp >= regime_rule.bbwp_expansion {
        "EXPANSION"
    } else if adx >= regime_rule.adx_trending || chop <= regime_rule.chop_trending {
        "TRENDING"
    } else {
        "RANGE"
    }
    .to_string();

    // Overall = confidence-weighted blend of trend + momentum (directional),
    // dampened when the regime is range/compression.
    // v9: blend + gate come from the strategy's `l1.context`.
    let (w_trend, w_momentum) = l1
        .map(|c| {
            (
                c.context.trend_momentum_blend[0],
                c.context.trend_momentum_blend[1],
            )
        })
        .unwrap_or((0.6, 0.4));
    let damp = l1.map(|c| &c.context.regime_gate_damp);
    let regime_gate = match regime.as_str() {
        "TRENDING" => damp.map(|d| d.trending).unwrap_or(1.0),
        "EXPANSION" => damp.map(|d| d.expansion).unwrap_or(1.0),
        "RANGE" => damp.map(|d| d.range).unwrap_or(0.6),
        _ => damp.map(|d| d.other).unwrap_or(0.5),
    };
    let w_sum_directional = (w_trend + w_momentum).max(1e-9);
    let blended =
        (trend.score * w_trend + momentum.score * w_momentum) / w_sum_directional * regime_gate;
    let overall_score = (blended * 100.0).round() as i32;
    let overall_label = dir_label(
        blended,
        "STRONG_BULL",
        "WEAK_BULL",
        "STRONG_BEAR",
        "WEAK_BEAR",
        "NEUTRAL",
    );

    MarketContext {
        trend,
        momentum,
        volatility,
        volume,
        liquidity,
        regime,
        overall_score,
        overall_label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_snapshot_inputs() -> HashMap<String, NormalizedIndicatorValue> {
        HashMap::new()
    }

    fn scalar(key: &str, raw: f64, norm: f64) -> HashMap<String, NormalizedIndicatorValue> {
        let mut m = empty_snapshot_inputs();
        m.insert(
            key.to_string(),
            NormalizedIndicatorValue::scalar(raw, norm, "TEST"),
        );
        m
    }

    #[test]
    fn empty_map_is_neutral() {
        let ctx = synthesize_market_context(&empty_snapshot_inputs(), None);
        assert_eq!(ctx.regime, "RANGE");
        assert_eq!(ctx.overall_label, "NEUTRAL");
        assert_eq!(ctx.overall_score, 0);
    }

    #[test]
    fn high_bbwp_is_expansion() {
        let map = scalar("bbwp", 95.0, 0.9);
        let ctx = synthesize_market_context(&map, None);
        assert_eq!(ctx.regime, "EXPANSION");
        assert_eq!(ctx.volatility.label, "EXPANSION_CLIMAX");
    }

    #[test]
    fn liquidity_dimension_stays_within_unit_interval() {
        // AUDIT-AIU-111: `ContextDimension.score` is contractually [-1, 1]
        // (02-07 §5.2) — the legacy ±100 clamp emitted `+40.00` next to
        // `+0.40` siblings. Sweep the extremes: heavy participation
        // (rvol 3.0) must not exceed +1.0, dead participation (rvol 0.4)
        // must not fall below −1.0.
        let mut heavy = empty_snapshot_inputs();
        heavy.insert(
            "rvol".into(),
            NormalizedIndicatorValue::scalar(3.0, 0.0, "EXHAUSTION_CLIMAX_VOLUME"),
        );
        heavy.insert(
            "vwap".into(),
            NormalizedIndicatorValue::scalar(100.0, 0.0, "EQUILIBRIUM"),
        );
        let ctx = synthesize_market_context(&heavy, None);
        assert!(
            ctx.liquidity.score >= -1.0 && ctx.liquidity.score <= 1.0,
            "liquidity.score {} outside [-1, 1]",
            ctx.liquidity.score
        );
        assert!(
            ctx.liquidity.score > 0.0,
            "high participation must read positive"
        );

        let mut thin = empty_snapshot_inputs();
        thin.insert(
            "rvol".into(),
            NormalizedIndicatorValue::scalar(0.4, 0.0, "LOW_PARTICIPATION_VOLUME"),
        );
        // Stretched away from fair value: vwap proximity ≈ 0 on top of the
        // thin-participation penalty → the dimension must read negative.
        thin.insert(
            "vwap".into(),
            NormalizedIndicatorValue::scalar(104.0, -0.8, "EXTREME_PREMIUM_REVERSION_ZONE"),
        );
        let ctx2 = synthesize_market_context(&thin, None);
        assert!(
            ctx2.liquidity.score >= -1.0 && ctx2.liquidity.score <= 1.0,
            "liquidity.score {} outside [-1, 1]",
            ctx2.liquidity.score
        );
        assert!(
            ctx2.liquidity.score < 0.0,
            "thin participation must read negative"
        );
    }

    #[test]
    fn bullish_trend_and_momentum_positive_overall() {
        let mut map = empty_snapshot_inputs();
        // Inject a few indicators so trend+momentum groups have something.
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(75.0, 0.8, "OVERBOUGHT"),
        );
        map.insert(
            "macd".into(),
            NormalizedIndicatorValue::scalar(1.5, 0.7, "BULLISH_EXPANDING"),
        );
        let ctx = synthesize_market_context(&map, None);
        assert!(ctx.overall_score > 0, "expected positive overall score");
    }

    // ── v9 L1-strategy tests ──

    #[test]
    fn default_l1_reproduces_legacy_output() {
        let mut map = scalar("bbwp", 95.0, 0.9);
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(75.0, 0.8, "OVERBOUGHT"),
        );
        let legacy = synthesize_market_context(&map, None);
        let with_default = synthesize_market_context(&map, Some(&L1Params::default()));
        assert_eq!(legacy.overall_score, with_default.overall_score);
        assert_eq!(legacy.regime, with_default.regime);
        assert_eq!(legacy.volatility.score, with_default.volatility.score);
        assert_eq!(
            legacy.volatility.confidence,
            with_default.volatility.confidence
        );
        assert_eq!(legacy.trend.score, with_default.trend.score);
        assert_eq!(legacy.momentum.score, with_default.momentum.score);
    }

    #[test]
    fn monitor_only_mutes_key_from_context() {
        let l1 = L1Params {
            monitor_only: vec!["rsi".to_string()],
            ..Default::default()
        };
        let mut map = empty_snapshot_inputs();
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(75.0, 0.8, "OVERBOUGHT"),
        );
        map.insert(
            "macd".into(),
            NormalizedIndicatorValue::scalar(1.5, 0.7, "BULLISH_EXPANDING"),
        );
        let muted = synthesize_market_context(&map, Some(&l1));
        let loud = synthesize_market_context(&map, None);
        assert!(muted.momentum.score.abs() < loud.momentum.score.abs());
    }

    #[test]
    fn indicator_weight_scales_contribution() {
        let mut l1 = L1Params::default();
        l1.indicator_weights.insert("rsi".to_string(), 0.0);
        let mut map = empty_snapshot_inputs();
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(75.0, 0.9, "OVERBOUGHT"),
        );
        map.insert(
            "macd".into(),
            NormalizedIndicatorValue::scalar(1.5, 0.7, "BULLISH_EXPANDING"),
        );
        let zeroed = synthesize_market_context(&map, Some(&l1));
        let loud = synthesize_market_context(&map, None);
        assert!(zeroed.momentum.score.abs() < loud.momentum.score.abs());
    }

    #[test]
    fn custom_regime_rule_shifts_classification() {
        let mut l1 = L1Params::default();
        l1.context.regime_rule.bbwp_compression = 40.0;
        l1.context.regime_rule.bbwp_expansion = 45.0;
        let map = scalar("bbwp", 50.0, 0.5);
        let ctx = synthesize_market_context(&map, Some(&l1));
        assert_eq!(ctx.regime, "EXPANSION");
        let legacy = synthesize_market_context(&map, None);
        assert_ne!(legacy.regime, ctx.regime);
    }

    #[test]
    fn vol_source_blend_uses_hv_and_atr() {
        let mut l1 = L1Params::default();
        l1.context.volatility_sources.bbwp = 0.0;
        l1.context.volatility_sources.hv = 1.0;
        l1.context.volatility_sources.atr_pct = 1.0;
        let mut map = scalar("bbwp", 50.0, 0.0);
        map.insert(
            "hv".into(),
            NormalizedIndicatorValue::scalar(0.04, 0.9, "HIGH_VOLATILITY"),
        );
        map.insert(
            "atr".into(),
            NormalizedIndicatorValue::scalar(120.0, 0.7, "ELEVATED_RANGE"),
        );
        let ctx = synthesize_market_context(&map, Some(&l1));
        let expected = (0.9 * 1.0 + 0.7 * 1.0) / 2.0;
        assert!(
            (ctx.volatility.score - expected).abs() < 1e-9,
            "volatility.score {} != {}",
            ctx.volatility.score,
            expected
        );
    }
}
