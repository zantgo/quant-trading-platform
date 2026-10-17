use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use tokio::sync::RwLock;

use core_domain::alignment::{self, AlignmentMatrix};
use core_domain::analysis::{self, AnalysisMatrix};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::MarketContext;
use core_domain::models::MarketSnapshot;
use std::collections::HashMap;

/// What `synthesize_cross_tf` returns. **The model, and nothing else.**
///
/// CR-15: the L4 opportunity, L5 risk and L6 advisory layers are strategy.
/// They are no longer computed here and are no longer carried on the wire
/// ([01-21](../../../conceptual-foundations/01-21-engine-split-and-model-boundary.md)
/// §3). The types still live in `core-domain` because TAE names them, but
/// nothing in this crate produces them.
pub struct CrossTfSynthesisResult {
    pub alignment: AlignmentMatrix,
    pub analysis: AnalysisMatrix,
}

pub fn synthesize_cross_tf(
    symbol: &str,
    tf_snapshots: &[(u64, &MarketSnapshot)],
    previous_score: Option<f64>,
    previous_regime: Option<core_domain::analysis::MarketRegime>,
    previous_volume_dim: Option<f64>,
    previous_bias: Option<core_domain::analysis::MarketBias>,
    // v9: the strategy's L3 params.
    analysis_params: &core_domain::analysis::AnalysisParams,
    // v9: the strategy's L2 params.
    alignment_params: &core_domain::alignment::AlignmentParams,
    // CR-15: `ladder_roles` decides WHICH duration leads the per-duration
    // indicator union. That is a model-side question — it changes which
    // readings the axes see — so it survives the strategy cut even though it
    // lives in the strategy config.
    ladder_roles: &config_models::strategy::LadderRoles,
) -> CrossTfSynthesisResult {
    // AUDIT-C2: labels are collected as owned `String`s first, then borrowed
    // for the alignment call. Previously the label was created *inside* the
    // filter_map closure and coerced to `&'static str` via `Box::leak` — one
    // leaked allocation per timeframe per candle close, unbounded over the
    // daemon lifetime (10+ MB/day on 1 s TFs). No allocation escapes here.
    type TfLabelInput<'a> = (
        String,
        u64,
        f64,
        &'a HashMap<String, NormalizedIndicatorValue>,
        &'a MarketContext,
    );
    type TimeframeInput<'a> = (
        &'a str,
        u64,
        f64,
        &'a HashMap<String, NormalizedIndicatorValue>,
        &'a MarketContext,
    );
    let tf_data_owned: Vec<TfLabelInput<'_>> = tf_snapshots
        .iter()
        .filter_map(|(secs, snap)| {
            let ctx = snap.context.as_ref()?;
            let price = snap.close.as_ref().and_then(|d| d.to_f64()).unwrap_or(0.0);
            Some((slot_label(snap), *secs, price, &snap.indicators, ctx))
        })
        .collect();
    let tf_data: Vec<TimeframeInput<'_>> = tf_data_owned
        .iter()
        .map(|(label, secs, price, map, ctx)| (label.as_str(), *secs, *price, *map, *ctx))
        .collect();

    let alignment = alignment::compute_alignment(symbol, &tf_data, alignment_params);

    // Build per-key union of indicators across all 4 TFs. The previous
    // implementation took the FIRST non-empty TF's indicator map as the
    // "representative" set, which meant: if TF1 had no Fibonacci / Volume
    // Profile / Pivot Points (e.g. macro still warming up) the confluent
    // level surface stayed empty even though TF3 / TF4 had the data.
    //
    // This per-key merge matches the cross-TF pattern already used by
    // `alignment::compute_alignment` (line 1286 above). Each indicator
    // key is filled from the FIRST TF that has it; subsequent TFs don't
    // overwrite. The "first wins" rule is deterministic and matches the
    // iteration order of `tf_snapshots` (micro, fast, slow, macro —
    // fastest candle first, so a populated faster TF shadows a stale
    // slower TF).
    //
    // v11 ladder_roles: when enabled, the `decision_tf` snapshot is
    // merged FIRST so its indicators drive bias/regime/stance/risk —
    // the frequency lever for short ladders (macro decides, micro
    // executes). Zones still read the merged level surface.
    let mut representative_indicators: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
    if ladder_roles.enabled {
        let mut ordered: Vec<(u64, &MarketSnapshot)> = Vec::new();
        let decision_label = ladder_roles.decision_tf.as_str();
        let mut rest: Vec<(u64, &MarketSnapshot)> = Vec::new();
        for item in tf_snapshots {
            // Case-insensitive: role values and derived duration labels
            // both come from the canonical pool ("1s"…"1d").
            if slot_label(item.1).eq_ignore_ascii_case(decision_label) {
                ordered.push(*item);
            } else {
                rest.push(*item);
            }
        }
        // v11.2 active-extremes fallback: when the configured decision
        // duration is NOT in the active set (e.g. decision_tf = "1d" with
        // the default fastest-8 ladder), the SLOWEST ACTIVE snapshot leads
        // the merge instead — the decision role stays anchored to the slow
        // horizon the operator asked for, never to the fastest.
        if ordered.is_empty() {
            if let Some(slowest) = tf_snapshots.iter().max_by_key(|(secs, _)| *secs) {
                ordered.push(*slowest);
            }
        }
        ordered.extend(rest);
        for (_, snap) in ordered {
            for (k, v) in &snap.indicators {
                representative_indicators
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
        }
    } else {
        for (_, snap) in tf_snapshots {
            for (k, v) in &snap.indicators {
                representative_indicators
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
        }
    }

    let bbwp = representative_indicators.get("bbwp").map(|v| v.raw_value);
    let adx = representative_indicators.get("adx").map(|v| v.raw_value);

    let analysis = analysis::derive_analysis(
        &alignment,
        bbwp,
        adx,
        previous_score,
        previous_regime,
        previous_volume_dim,
        previous_bias,
        // v9: the strategy's L3 params.
        analysis_params,
    );

    CrossTfSynthesisResult {
        alignment,
        analysis,
    }
}

fn slot_label(snap: &MarketSnapshot) -> String {
    core_domain::duration_label(snap.timeframe_secs)
}

#[allow(dead_code)]
fn empty_map() -> &'static HashMap<String, NormalizedIndicatorValue> {
    static MAP: std::sync::LazyLock<HashMap<String, NormalizedIndicatorValue>> =
        std::sync::LazyLock::new(HashMap::new);
    &MAP
}

pub struct SynthesisContext {
    pub micro_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub fast_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub slow_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub macro_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
}

impl SynthesisContext {
    pub async fn gather_snapshots(&self) -> Vec<(u64, MarketSnapshot)> {
        let mut out = Vec::with_capacity(4);
        if let Some(s) = self.micro_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.fast_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.slow_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        if let Some(s) = self.macro_snapshot.read().await.clone() {
            out.push((s.timeframe_secs, s));
        }
        out
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    //! Model-side synthesis tests. CR-15 removed the L4/L5/L6 tests with the
    //! layers themselves: they exercised `compute_opportunity`, zone geometry
    //! and risk/reward, none of which this function produces any more. What is
    //! left is the model's own contract — alignment across durations, the bias
    //! collapse, and the representative-indicator union.
    use super::*;
    use core_domain::analysis::MarketBias;
    use core_domain::indicator_dtos::NormalizedIndicatorValue;
    use core_domain::market_context::{ContextDimension, MarketContext};
    use core_domain::models::MarketSnapshot;
    use rust_decimal::Decimal;

    fn ctx(trend: f64, momentum: f64, overall: i32) -> MarketContext {
        MarketContext {
            trend: ContextDimension {
                score: trend,
                confidence: 0.7,
                label: "WEAK_BULL".into(),
            },
            momentum: ContextDimension {
                score: momentum,
                confidence: 0.6,
                label: "WEAK_BULL".into(),
            },
            volatility: ContextDimension {
                score: 0.0,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            volume: ContextDimension {
                score: 0.0,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            liquidity: ContextDimension::neutral(),
            regime: "TRENDING_BULL".into(),
            overall_score: overall,
            overall_label: if overall > 20 {
                "BULLISH".into()
            } else {
                "NEUTRAL".into()
            },
        }
    }

    fn snap(secs: u64, price: f64, context: MarketContext) -> MarketSnapshot {
        let mut m = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            timeframe_secs: secs,
            timeframe_label: Some(format!("{secs}s")),
            mid_price: Decimal::from_f64_retain(price).unwrap(),
            close: Decimal::from_f64_retain(price),
            indicators: Default::default(),
            ..MarketSnapshot::default()
        };
        m.context = Some(context);
        m
    }

    fn synth(snaps: &[&MarketSnapshot]) -> CrossTfSynthesisResult {
        let pairs: Vec<(u64, &MarketSnapshot)> =
            snaps.iter().map(|s| (s.timeframe_secs, *s)).collect();
        let params = core_domain::analysis::AnalysisParams::default();
        let alignment = core_domain::alignment::AlignmentParams::default();
        let roles = config_models::strategy::LadderRoles::default();
        synthesize_cross_tf(
            "BTC-USDC", &pairs, None, None, None, None, &params, &alignment, &roles,
        )
    }

    #[test]
    fn no_durations_yields_neutral_model_state() {
        let r = synth(&[]);
        assert_eq!(r.alignment.timeframes_present, 0);
        assert_eq!(r.analysis.bias, MarketBias::Neutral);
    }

    #[test]
    fn one_duration_still_produces_a_bias() {
        let s = snap(60, 100.0, ctx(0.6, 0.4, 40));
        let r = synth(&[(&s)]);
        assert_eq!(r.alignment.timeframes_present, 1);
        assert!(
            matches!(
                r.analysis.bias,
                MarketBias::Bullish | MarketBias::StrongBullish
            ),
            "one bullish duration should read bullish, got {:?}",
            r.analysis.bias
        );
    }

    #[test]
    fn four_aligned_bullish_durations_agree() {
        let a = snap(1, 100.0, ctx(0.8, 0.5, 60));
        let b = snap(60, 100.0, ctx(0.8, 0.5, 60));
        let c = snap(300, 100.0, ctx(0.8, 0.5, 60));
        let d = snap(3600, 100.0, ctx(0.8, 0.5, 60));
        let r = synth(&[(&a), (&b), (&c), (&d)]);
        assert_eq!(r.alignment.timeframes_present, 4);
        assert!(
            matches!(
                r.analysis.bias,
                MarketBias::Bullish | MarketBias::StrongBullish
            ),
            "four aligned bullish durations should agree bullish, got {:?}",
            r.analysis.bias
        );
    }

    #[test]
    fn opposing_durations_never_produce_a_strong_bias() {
        // The invariant is NOT "opposing durations read neutral" — the bias-grace
        // bands can still land on a plain directional value. The invariant is
        // that cross-duration disagreement can never produce a STRONG claim:
        // one fast duration disagreeing with one slow one is not enough evidence
        // for a confident direction.
        let a = snap(60, 100.0, ctx(0.8, 0.5, 60));
        let b = snap(3600, 100.0, ctx(-0.8, -0.5, -60));
        let r = synth(&[(&a), (&b)]);
        assert!(
            !matches!(
                r.analysis.bias,
                MarketBias::StrongBullish | MarketBias::StrongBearish
            ),
            "two opposing durations must not produce a strong bias, got {:?}",
            r.analysis.bias
        );
    }

    #[test]
    fn representative_indicators_merge_across_durations_when_the_fastest_lacks_a_key() {
        // The union must draw a key from ANY duration, not only the first —
        // a fast duration still warming up used to blank the whole surface.
        let mut slow = snap(3600, 100.0, ctx(0.5, 0.5, 30));
        slow.indicators.insert(
            "fibonacci".into(),
            NormalizedIndicatorValue::scalar(0.618, 0.0, "0.618"),
        );
        let fast = snap(1, 100.0, ctx(0.5, 0.5, 30));
        let r = synth(&[(&fast), (&slow)]);
        assert_eq!(r.analysis.symbol, "BTC-USDC");
        // The analysis is produced from the union; a missing key must not panic
        // or blank the bias.
        assert_ne!(r.analysis.bias, MarketBias::Neutral);
    }
}
