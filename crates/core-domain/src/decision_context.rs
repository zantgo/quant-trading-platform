//! # Decision Context — Quantitative Decision Metadata
//!
//! Computes quantitative decision-support metadata from the Analysis matrix (L3),
//! the Opportunity matrix (L4), and the Risk matrix (L5). The three matrices are
//! always consumed together — they form the canonical L6 synthesis triad.
//!
//! ## Canonical reference
//!
//! - [Decision Matrix §2.1](../matrices/02-04-decision-matrix.md) for the
//!   field-by-field contract.
//! - [Risk Matrix §2.1](../matrices/02-11-risk-matrix.md) for the
//!   `overall_risk.score ∈ [0, 100]` unit convention (and the `/ 100.0`
//!   divisor in `expected_reward_risk_ratio`).

use crate::analysis::AnalysisMatrix;
use crate::risk::{RiskDimension, RiskMatrix};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::indicator_dtos::NormalizedIndicatorValue;

/// Quantitative decision-metadata matrix published on `MarketSnapshot.decision_context`.
///
/// Synthesizes the canonical L3 (state) + L4 (forecast) + L5 (danger) triad. Distinct
/// from the human-facing `AdvisoryMatrix` (which carries the `trade_readiness`,
/// `directional_guidance`, `entry_guidance`, etc. guidance fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionContext {
    /// Quantitative confluence score in `[-100, +100]` — signed product of
    /// `analysis.market_bias_score` (sign) and the 3-factor quality blend
    /// (magnitude). Mirrors `Analysis.bias` family on the sign axis.
    pub score: f64,
    /// 5-state `MarketBias` family. Mirrors `Analysis.bias` exactly; wire
    /// values are PascalCase (`StrongBullish` / `Bullish` / `Neutral` /
    /// `Bearish` / `StrongBearish`).
    pub bias: String,
    /// `[0.0, 1.0]` derived as `|score| / 100.0`. Canonical name in the wire
    /// schema (renamed from `confidence` in the institutional redesign; see
    /// [02-00b-confidence-hierarchy.md](../matrices/02-00b-confidence-hierarchy.md)).
    /// The legacy `confidence` field has been removed; the audit identified
    /// the duplicate as breaking the canonical field-ownership contract.
    pub score_confidence: f64,
    /// Synoptic entry-danger in `[0, 100]` (high = dangerous). `RiskDimension`
    /// shape matching the wire contract documented at
    /// [02-00-matrix-field-ownership.md](../matrices/02-00-matrix-field-ownership.md).
    /// Synthesized from L3 `market_quality` and L4 `opportunity_score` via
    /// the formula in Decision Matrix §3.8.
    pub entry_danger: RiskDimension,
    /// Risk-discounted R:R — `(active-side R:R) × (1 − L5.overall_risk / 100.0)`.
    /// The active-side R:R is `L4.long_expected_rr_internal` for bullish
    /// bias, `L4.short_expected_rr_internal` for bearish, 0 for Neutral.
    /// The legacy matrix-level `L4.expected_rr_internal` was removed in v6.9.
    pub expected_reward_risk_ratio: f64,
    /// Gate label — one of `READY` / `FORMING` / `WATCH` / `STAND_ASIDE`.
    pub trade_readiness: String,
    /// Indicator keys whose `confidence ≥ 0.6` contribute to the score.
    pub contributing_indicators: Vec<String>,
    /// Long-side normalized probability (0–100, integer). Sums to 100 with
    /// `short_probability` + `hold_probability`. Canonical source of truth
    /// for the "X% long" display; replaces the frontend-local computation
    /// that previously lived in `ui/src/lib/decisionRank.ts`.
    #[serde(default)]
    pub long_probability: f64,
    /// Short-side normalized probability (0–100, integer).
    #[serde(default)]
    pub short_probability: f64,
    /// Hold (no-position) normalized probability (0–100, integer).
    #[serde(default)]
    pub hold_probability: f64,
    /// Net directional bias in percentage points (`long − short`),
    /// range `[−100, +100]`. Positive = net long-leaning, negative = net
    /// short-leaning.
    #[serde(default)]
    pub net_bias_pct: f64,
    /// v6.10.19 (P6): `true` when the graded-lean floors actually
    /// adjusted the split (HOLD was capped at 60% and/or the directional
    /// arm was raised to 15%) — the operator-facing LEAN annotation
    /// ("floor-boosted") tells the trader this is a structurally boosted
    /// low-confidence read, not a deep consensus.
    #[serde(default)]
    pub lean_floor_applied: bool,
}

impl DecisionContext {
    /// Compute `DecisionContext` from the L3/L4/L5 triad plus the indicator map.
    ///
    /// `opportunity` may be `None` if the L4 Opportunity Matrix is not yet
    /// populated for this symbol (early-warmup state). The function still
    /// produces a valid `DecisionContext` in this case.
    ///
    /// v9 F-05: `params` is the shared Decision-layer parameter struct —
    /// every grid below reads the same source `compute_advisory` uses.
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        indicators: &HashMap<String, NormalizedIndicatorValue>,
        close: f64,
        atr: f64,
        confluence_score: f64,
        analysis: &AnalysisMatrix,
        opportunity: Option<&super::opportunity::OpportunityMatrix>,
        risk: &RiskMatrix,
        params: &super::decision_params::DecisionParams,
        analysis_params: &super::analysis::AnalysisParams,
    ) -> Self {
        let _ = (close, atr); // kept for API symmetry / future volatility-aware extensions

        // Mirror `analysis.bias` exactly (5-state MarketBias family).
        // The unsigned confluence_score cannot encode direction; the
        // canonical source of directional bias is Analysis.bias.
        // Uses the same PascalCase as `AnalysisMatrix.bias` serialisation
        // (MarketBias derives `Serialize` without `rename_all`), NOT the
        // SCREAMING_SNAKE_CASE from the Display impl. The frontend type
        // `MarketBias` only covers PascalCase.
        let bias = match analysis.bias {
            crate::analysis::MarketBias::StrongBullish => "StrongBullish".to_string(),
            crate::analysis::MarketBias::Bullish => "Bullish".to_string(),
            crate::analysis::MarketBias::Neutral => "Neutral".to_string(),
            crate::analysis::MarketBias::Bearish => "Bearish".to_string(),
            crate::analysis::MarketBias::StrongBearish => "StrongBearish".to_string(),
        };

        // Score carries the directional sign from analysis.bias so the
        // frontend `computeDecisionRank` can split probability between
        // LONG (positive score) and SHORT (negative score) arms.
        let signed_confluence =
            match analysis.bias {
                crate::analysis::MarketBias::StrongBearish
                | crate::analysis::MarketBias::Bearish => -confluence_score,
                crate::analysis::MarketBias::Neutral => 0.0,
                crate::analysis::MarketBias::Bullish
                | crate::analysis::MarketBias::StrongBullish => confluence_score,
            };

        // -- expected_reward_risk_ratio = active-side R:R × (1 − overall_risk/100)
        let active_rr = opportunity
            .map(|o| match analysis.bias {
                crate::analysis::MarketBias::StrongBullish
                | crate::analysis::MarketBias::Bullish => o.long_expected_rr_internal,
                crate::analysis::MarketBias::StrongBearish
                | crate::analysis::MarketBias::Bearish => o.short_expected_rr_internal,
                crate::analysis::MarketBias::Neutral => 0.0,
            })
            .unwrap_or(0.0);
        let risk_disc = params.risk_discount(risk.overall_risk.score);
        let expected_reward_risk_ratio = active_rr * risk_disc;

        // -- entry_danger = mean(quality_penalty, 100 − opportunity_score)
        let quality_penalty: f64 = match analysis.market_quality {
            crate::analysis::QualityLevel::Excellent => params.quality_penalty(0),
            crate::analysis::QualityLevel::Good => params.quality_penalty(1),
            crate::analysis::QualityLevel::Average => params.quality_penalty(2),
            crate::analysis::QualityLevel::Weak => params.quality_penalty(3),
            crate::analysis::QualityLevel::Poor => params.quality_penalty(4),
        };
        let opportunity_score = opportunity
            .map(|o| o.opportunity_score)
            .unwrap_or(params.opportunity_fallback);
        let entry_danger_score =
            ((quality_penalty + (100.0 - opportunity_score)) / 2.0).clamp(0.0, 100.0);
        let entry_danger = RiskDimension::from_score_with_confidence(
            entry_danger_score,
            analysis.state_confidence,
        );

        // -- confidence_assessment (shared DecisionParams helper)
        let confidence_assessment =
            params.confidence_assessment(analysis.state_confidence, risk.overall_risk.score);

        let score_confidence = (confluence_score.abs() / 100.0).min(1.0);

        // -- Market stance (exact replication of AdvisoryMatrix A5 / §3.2)
        //    Returns (is_avoid, is_aggressive_or_constructive) so the
        //    percentage modulation matches `compute_advisory()` exactly.
        //    v9 F-05: the same DecisionParams borders.
        let (stance_is_avoid, stance_is_aggressive_or_constructive) = match analysis.market_quality
        {
            crate::analysis::QualityLevel::Poor => {
                if risk.overall_risk.score >= params.stance_risk_avoid {
                    (true, false)
                } else {
                    (false, false) // Cautious
                }
            }
            crate::analysis::QualityLevel::Weak => {
                (false, false) // always Cautious
            }
            crate::analysis::QualityLevel::Average => {
                (false, false) // Neutral or Cautious — neither avoid nor aggressive
            }
            crate::analysis::QualityLevel::Good => {
                if risk.overall_risk.score < params.stance_risk_constructive {
                    (false, true) // Constructive
                } else {
                    (false, false) // Cautious
                }
            }
            crate::analysis::QualityLevel::Excellent => {
                if risk.overall_risk.score >= params.stance_risk_avoid {
                    (true, false) // Avoid
                } else if risk.overall_risk.score < params.stance_risk_constructive {
                    (false, true) // Aggressive or Constructive
                } else {
                    (false, false) // Cautious
                }
            }
        };

        // -- Directional guidance (exact replication of AdvisoryMatrix A3 / §3.1)
        //    Returns (is_long, is_short) booleans for the percentage modulation.
        // v6.10.17: a LIFTED bias (grace / hysteresis hold / LEAN tier —
        // `bias_lifted`) is always treated as directional regardless of the
        // risk gate, so a minimal bullish/bearish confirmation produces a
        // graded directional probability split instead of a 96% HOLD. The
        // risk gate still applies to plain directional biases (unchanged).
        let lifted = crate::analysis::bias_lifted(
            analysis.bias,
            analysis.market_bias_score,
            analysis_params,
        );
        let (direction_is_long, direction_is_short) = if stance_is_avoid {
            (false, false) // AvoidDirectionalExposure — no directional lean
        } else {
            match analysis.bias {
                crate::analysis::MarketBias::StrongBullish => (true, false), // StrongLong (risk gate bypassed: identical outcome)
                crate::analysis::MarketBias::Bullish => {
                    if lifted || risk.overall_risk.score < params.direction_risk_plain {
                        (true, false) // Long (lifted reads bypass the risk gate)
                    } else {
                        (false, false) // Neutral
                    }
                }
                crate::analysis::MarketBias::StrongBearish => (false, true), // StrongShort (risk gate bypassed: identical outcome)
                crate::analysis::MarketBias::Bearish => {
                    if lifted || risk.overall_risk.score < params.direction_risk_plain {
                        (false, true) // Short (lifted reads bypass the risk gate)
                    } else {
                        (false, false) // Neutral
                    }
                }
                crate::analysis::MarketBias::Neutral => (false, false),
            }
        };

        let directional_is_non_neutral = direction_is_long || direction_is_short;

        // -- entry_guidance (mirrors AdvisoryMatrix `compute_advisory` §3.4).
        //    v6.10.17 (P1): the READY gate must not fire when the advisory
        //    would tell the operator to WAIT — the legacy volatility-only
        //    proxy let a READY badge coexist with "Entry: Wait for
        //    confirmation" (a Developing trend with vol ≥ 20). The full
        //    mirror: NoEntryContext (vol ≥ 60, or a weak/exhausted trend)
        //    and WaitForConfirmation (Developing with vol ≥ 20) are both
        //    wait-states; only Immediate/Pullback/Breakout entries pass.
        let entry_guidance_is_wait = risk.volatility_risk.score >= params.entry_vol_no_entry
            || matches!(
                analysis.trend_assessment,
                crate::analysis::TrendAssessment::Weak
                    | crate::analysis::TrendAssessment::Exhausted
            )
            || (analysis.trend_assessment == crate::analysis::TrendAssessment::Developing
                && risk.volatility_risk.score >= params.entry_vol_breakout);

        // v6.10.19 (T4): "FORMING" means a setup is actually coiling — it
        // requires at least one QUALIFYING profile (preconditions_met > 0,
        // non-NoClear). A directional read with zero qualifying profiles
        // is a dead no-clear market: the lean stays visible (v6.10.17
        // decoupling) but the readiness honest WATCH, never a misleading
        // "FORMING" next to "NO CLEAR SETUP".
        let has_qualifying_profile = opportunity
            .map(|o| {
                o.profiles.iter().any(|p| {
                    p.preconditions_met > 0
                        && p.opportunity_type
                            != crate::analysis::OpportunityType::NoClearOpportunity
                })
            })
            .unwrap_or(false);

        let trade_readiness =
            if stance_is_avoid || confidence_assessment < params.readiness_aside_max {
                "STAND_ASIDE"
            } else if directional_is_non_neutral
                && confidence_assessment >= params.readiness_ready_min
                && stance_is_aggressive_or_constructive
                && !entry_guidance_is_wait
            {
                "READY"
            } else if directional_is_non_neutral && has_qualifying_profile {
                "FORMING"
            } else {
                "WATCH"
            }
            .to_string();

        // v9: the risk-ceiling soft-block — when the strategy's
        // `l6.risk_ceiling.max_overall_risk` is breached, readiness floors
        // at WATCH (the setup still surfaces, visibly blocked; TAE requires
        // READY so it can never execute).
        let trade_readiness = if params
            .risk_ceiling_max_overall_risk
            .is_some_and(|ceiling| risk.overall_risk.score > ceiling)
            && (trade_readiness == "READY" || trade_readiness == "FORMING")
        {
            "WATCH".to_string()
        } else {
            trade_readiness
        };

        // Contributing indicators: any indicator with confidence >= 0.6
        let contributing_indicators: Vec<String> = indicators
            .iter()
            .filter(|(_, v)| v.confidence >= params.prob_contributing_conf_min)
            .map(|(k, _)| k.clone())
            .collect();

        // ── Probability fields ─────────────────────────────────────────
        // Replicate the frontend `computeDecisionRank()` algorithm
        // (ui/src/lib/decisionRank.ts:66–202) so the canonical percentage
        // numbers (e.g. "+7% long") live server-side and are the source
        // of truth for all consumers.

        // Geometric offset: when bias is Neutral (score=0), inspect the
        // top-scored profile for a latent directional lean.
        let effective_score = if signed_confluence.abs() < 1e-9 {
            let mut offset = 0.0;
            if let Some(opp) = opportunity {
                let has_long = opp.long_entry_zone.low > 0.0;
                let has_short = opp.short_entry_zone.low > 0.0;
                let qualifying: Vec<_> = opp
                    .profiles
                    .iter()
                    .filter(|p| {
                        p.preconditions_met > 0
                            && p.opportunity_type
                                != crate::analysis::OpportunityType::NoClearOpportunity
                    })
                    .collect();
                if let Some(top) = qualifying.iter().max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) {
                    if has_long && !has_short {
                        offset = top.score * params.prob_geometric_offset;
                    } else if has_short && !has_long {
                        offset = -top.score * params.prob_geometric_offset;
                    } else if has_long && has_short {
                        if opp.long_expected_rr_internal >= opp.short_expected_rr_internal {
                            offset = top.score * params.prob_geometric_offset;
                        } else {
                            offset = -top.score * params.prob_geometric_offset;
                        }
                    }
                }
            }
            offset
        } else {
            signed_confluence
        };

        let effective_confidence = if score_confidence >= params.prob_eff_conf_floor {
            score_confidence
        } else {
            params.prob_eff_conf_floor
        };

        let base_long = (effective_score.max(0.0) * effective_confidence).clamp(0.0, 100.0);
        let base_short = ((-effective_score).max(0.0) * effective_confidence).clamp(0.0, 100.0);
        let base_hold = ((entry_danger_score / 100.0) * params.prob_hold_scale).clamp(0.0, 100.0);

        let mut long = base_long;
        let mut short = base_short;
        let mut hold = base_hold;

        // Modulation by directional guidance
        if direction_is_long {
            long *= params.prob_guidance_amp;
            short *= params.prob_guidance_atten;
        } else if direction_is_short {
            short *= params.prob_guidance_amp;
            long *= params.prob_guidance_atten;
        }

        // Modulation by market stance
        if stance_is_aggressive_or_constructive {
            if direction_is_long {
                long *= params.prob_stance_amp;
            } else if direction_is_short {
                short *= params.prob_stance_amp;
            }
        }
        if stance_is_avoid {
            long *= params.prob_avoid_atten;
            short *= params.prob_avoid_atten;
            hold *= params.prob_avoid_hold_amp;
        }

        // R:R modulation. v6.10.17: the ×0.6 penalty applies only when an
        // ACTUAL sub-1.0 R:R exists (`expected_reward_risk_ratio > 0`). A
        // missing R:R (0 — no-clear matrices, warmup) is unknown, not bad,
        // and must not further punish a vote-driven lifted lean: the
        // aggregated bracket surfaced by the panel carries the real
        // geometric R:R for the operator.
        // v6.10.18 (I-1b): the penalty is keyed to the BIAS direction, not
        // the risk-gate guidance — when the §3.1 gate produces Neutral
        // guidance at elevated risk (plain Bullish, risk ≥ 40), the poor
        // bracket must STILL cap the directional conviction (a trader
        // rejects "worse R:R → higher conviction"). The read direction is
        // the bias; the guidance only governs the gate.
        let bias_is_long = matches!(
            analysis.bias,
            crate::analysis::MarketBias::StrongBullish | crate::analysis::MarketBias::Bullish
        );
        let bias_is_short = matches!(
            analysis.bias,
            crate::analysis::MarketBias::StrongBearish | crate::analysis::MarketBias::Bearish
        );
        if expected_reward_risk_ratio > 0.0 && expected_reward_risk_ratio < 1.0 {
            if bias_is_long {
                long *= params.prob_rr_penalty;
            } else if bias_is_short {
                short *= params.prob_rr_penalty;
            }
        }

        long = long.clamp(0.0, 100.0);
        short = short.clamp(0.0, 100.0);
        hold = hold.clamp(0.0, 100.0);

        // Renormalize to sum to 100 (largest absorbs rounding residual)
        let sum = long + short + hold;
        let (mut l, mut s, mut h) = if sum <= 0.0 {
            (34.0, 33.0, 33.0)
        } else {
            let l_rounded = ((long / sum) * 100.0).round();
            let s_rounded = ((short / sum) * 100.0).round();
            let h_rounded = 100.0 - l_rounded - s_rounded;
            (l_rounded, s_rounded, h_rounded)
        };

        // MIN_PCT floor (prevents degenerate 100/0/0 distributions)
        l = l.max(params.prob_min_pct);
        s = s.max(params.prob_min_pct);
        h = h.max(params.prob_min_pct);

        // v6.10.17 graded-lean floors: whenever a directional read exists
        // the distribution must stay GRADED — HOLD is capped at 60% and
        // the directional arm never sinks below 15% — so the verdict can
        // never collapse into a 96% HOLD next to a minimal bearish or
        // bullish confirmation. The 2% floor applies only to the truly
        // flat (Neutral-bias) state. v6.10.18 (I-1b): keyed to the BIAS
        // direction (a directional read exists whenever the bias is
        // directional), matching the R:R modulation.
        let directional_active = bias_is_long || bias_is_short;
        let mut lean_floor_applied = false;
        if directional_active {
            let mut dl = l;
            let mut ds = s;
            let mut dh = h;
            // v6.10.19 (P6): track whether the floors actually moved the
            // split — the operator must SEE that this is a boosted lean.
            let mut floor_moved = false;
            if dh > params.prob_hold_cap {
                floor_moved = true;
                dh = params.prob_hold_cap;
            }
            if bias_is_long && dl < params.prob_arm_floor {
                floor_moved = true;
                dl = params.prob_arm_floor;
            } else if bias_is_short && ds < params.prob_arm_floor {
                floor_moved = true;
                ds = params.prob_arm_floor;
            }
            lean_floor_applied = floor_moved;
            l = dl;
            s = ds;
            h = dh;
        }

        let re_sum = l + s + h;
        let long_probability = ((l / re_sum) * 100.0).round();
        let short_probability = ((s / re_sum) * 100.0).round();
        let hold_probability = 100.0 - long_probability - short_probability;
        let net_bias_pct = long_probability - short_probability;

        Self {
            score: signed_confluence,
            bias,
            score_confidence,
            entry_danger,
            expected_reward_risk_ratio,
            trade_readiness,
            contributing_indicators,
            long_probability,
            short_probability,
            hold_probability,
            net_bias_pct,
            lean_floor_applied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        AnalysisMatrix, MarketBias, MarketPhase, MarketRegime, MomentumAssessment, OpportunityType,
        QualityLevel, StructureAssessment, TrendAssessment, VolatilityAssessment, VolumeAssessment,
    };
    use crate::risk::{RiskDimension, RiskLevel, RiskMatrix, RiskState};

    fn make_analysis_with_quality(q: QualityLevel) -> AnalysisMatrix {
        AnalysisMatrix {
            symbol: "BTC-USD".to_string(),
            bias: MarketBias::Neutral,
            market_bias_score: 0.0,
            state_confidence: 0.82,
            confidence: 0.82,
            market_regime: MarketRegime::Range,
            trend_assessment: TrendAssessment::Healthy,
            momentum_assessment: MomentumAssessment::Stable,
            structure_assessment: StructureAssessment::Healthy,
            volatility_assessment: VolatilityAssessment::Normal,
            volume_assessment: VolumeAssessment::Normal,
            market_quality: q,
            market_quality_score: 0.0,
            trend_score: None,
            momentum_score: None,
            structure_score: None,
            volatility_score: None,
            volume_score: None,
            representative_bbwp: None,
            representative_adx: None,
            market_phase: MarketPhase::Unknown,
            market_interpretation: "Test".into(),
            rationale: String::new(),
            supporting_signals: Vec::new(),
            contradicting_signals: Vec::new(),
            timeframes_considered: 4,
        }
    }

    fn make_risk_with_overall(score: f64) -> RiskMatrix {
        let dim = RiskDimension {
            score,
            level: RiskLevel::Low,
            state: RiskState::Stable,
            confidence: 50.0,
            evidence: Vec::new(),
            volatility_to_spread_ratio: None,
        };
        RiskMatrix {
            symbol: "BTC-USD".to_string(),
            market_risk: dim.clone(),
            volatility_risk: dim.clone(),
            execution_liquidity_risk: dim.clone(),
            structure_risk: dim.clone(),
            momentum_risk: dim.clone(),
            signal_risk: dim.clone(),
            execution_risk: dim.clone(),
            cascade_risk: dim.clone(),
            overall_risk: dim,
        }
    }

    #[test]
    fn bullish_bias_produces_positive_score() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Good);
        analysis.bias = MarketBias::Bullish;
        let risk = make_risk_with_overall(20.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Breakout,
            opportunity_score: 85.0,
            setup_quality: crate::analysis::SetupQuality::Prime,
            profiles: vec![],
            forecast_confidence: 0.85,
            long_expected_rr_internal: 2.5,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            30.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Bullish");
        assert!((ctx.score - 30.0).abs() < 1e-9);
        assert!((ctx.expected_reward_risk_ratio - 2.0).abs() < 1e-9);
        assert!((ctx.entry_danger.score - 20.0).abs() < 1e-9);
        assert_eq!(ctx.trade_readiness, "READY");
        assert!(ctx.long_probability > ctx.short_probability);
        assert!(
            (ctx.long_probability + ctx.short_probability + ctx.hold_probability - 100.0).abs()
                < 1e-9
        );
        assert!((ctx.net_bias_pct - (ctx.long_probability - ctx.short_probability)).abs() < 1e-9);
    }

    #[test]
    fn bearish_bias_produces_negative_score() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Good);
        analysis.bias = MarketBias::Bearish;
        let risk = make_risk_with_overall(20.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::TrendContinuation,
            opportunity_score: 70.0,
            setup_quality: crate::analysis::SetupQuality::Prime,
            profiles: vec![],
            forecast_confidence: 0.7,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 2.0,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            50.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Bearish");
        assert!((ctx.score - -50.0).abs() < 1e-9);
        // active side is Bearish → reads short_expected_rr_internal
        assert!((ctx.expected_reward_risk_ratio - (2.0 * 0.8)).abs() < 1e-9);
        assert!(ctx.short_probability > ctx.long_probability);
        assert!(
            (ctx.long_probability + ctx.short_probability + ctx.hold_probability - 100.0).abs()
                < 1e-9
        );
    }

    #[test]
    fn neutral_bias_produces_zero_score() {
        let analysis = make_analysis_with_quality(QualityLevel::Good);
        let risk = make_risk_with_overall(20.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 0.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            30.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Neutral");
        assert!((ctx.score - 0.0).abs() < 1e-9);
        // Neutral bias with no directional offset should produce a balanced distribution
        assert!(
            (ctx.long_probability + ctx.short_probability + ctx.hold_probability - 100.0).abs()
                < 1e-9
        );
        assert!(ctx.net_bias_pct >= -5.0 && ctx.net_bias_pct <= 5.0);
    }

    #[test]
    fn graced_bias_produces_nonzero_signed_confluence() {
        // v6.10.16 sensitivity lever: a Bullish bias produced by the L3
        // grace band must carry a positive signed confluence (the Neutral
        // zeroing at decision_context.rs is driven by bias, so rescuing the
        // bias rescues the direction) — this is the exact cascade that
        // turned the user's 4:0-vote capture into a HOLD.
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bullish;
        let risk = make_risk_with_overall(43.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Pullback,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::Moderate,
            profiles: vec![],
            forecast_confidence: 0.44,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.0,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        // Unsigned blend ≈ 0.5×75 + 0.3×quality + 0.2×60 ≈ 63 (as the
        // capture's L6). With a Bullish bias the signed score must be
        // positive, not zeroed.
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            63.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Bullish");
        assert!((ctx.score - 63.0).abs() < 1e-9);
        assert!(ctx.long_probability > ctx.hold_probability);
        assert!(
            (ctx.long_probability + ctx.short_probability + ctx.hold_probability - 100.0).abs()
                < 1e-9
        );
    }

    #[test]
    fn strong_bullish_mirrors_analysis_bias() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Excellent);
        analysis.bias = MarketBias::StrongBullish;
        let risk = make_risk_with_overall(10.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::TrendContinuation,
            opportunity_score: 100.0,
            setup_quality: crate::analysis::SetupQuality::Prime,
            profiles: vec![],
            forecast_confidence: 0.85,
            long_expected_rr_internal: 3.0,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            90.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "StrongBullish");
        assert!((ctx.score - 90.0).abs() < 1e-9);
        assert!(ctx.long_probability > ctx.short_probability);
    }

    #[test]
    fn strong_bearish_mirrors_analysis_bias() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Weak);
        analysis.bias = MarketBias::StrongBearish;
        let risk = make_risk_with_overall(10.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Breakout,
            opportunity_score: 80.0,
            setup_quality: crate::analysis::SetupQuality::Strong,
            profiles: vec![],
            forecast_confidence: 0.8,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 1.8,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            65.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "StrongBearish");
        assert!((ctx.score - -65.0).abs() < 1e-9);
        assert!(ctx.short_probability > ctx.long_probability);
    }

    #[test]
    fn high_risk_blocks_at_70() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bullish;
        let risk = make_risk_with_overall(80.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Breakout,
            opportunity_score: 70.0,
            setup_quality: crate::analysis::SetupQuality::Strong,
            profiles: vec![],
            forecast_confidence: 0.7,
            long_expected_rr_internal: 2.5,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            30.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!((ctx.expected_reward_risk_ratio - 0.5).abs() < 1e-9);
        assert_eq!(ctx.trade_readiness, "STAND_ASIDE");
        assert!(ctx.long_probability > 0.0);
        assert!(ctx.short_probability > 0.0);
        assert!(ctx.hold_probability > 0.0);
    }

    #[test]
    fn dangerous_setup_blocks_at_70() {
        let analysis = make_analysis_with_quality(QualityLevel::Weak);
        let risk = make_risk_with_overall(85.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Breakout,
            opportunity_score: 30.0,
            setup_quality: crate::analysis::SetupQuality::Marginal,
            profiles: vec![],
            forecast_confidence: 0.3,
            long_expected_rr_internal: 2.5,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            30.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!((ctx.entry_danger.score - 70.0).abs() < 1e-9);
        assert_eq!(ctx.trade_readiness, "STAND_ASIDE");
        assert!(ctx.long_probability > 0.0);
    }

    #[test]
    fn lifted_lean_bias_grades_probabilities() {
        // v6.10.17: the 03:40-style capture — Bearish bias lifted by the
        // LEAN tier (composite 2.6, 3:1 bearish vote), risk 41.87 (> 40,
        // would silence a plain Bearish), blend 55, no-clear matrix (R:R 0
        // → no ×0.6 penalty). The probability split must be GRADED
        // directional — never a 96% HOLD.
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bearish;
        analysis.market_bias_score = 0.026; // wire fraction of the 2.6 composite
        let risk = make_risk_with_overall(41.87);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.0,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            55.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Bearish");
        assert!(ctx.short_probability > ctx.long_probability);
        assert!(ctx.short_probability > ctx.hold_probability);
        assert!(ctx.hold_probability <= 60.0);
        assert!(ctx.short_probability >= 15.0);
        assert!(ctx.net_bias_pct < -30.0);
        let total = ctx.long_probability + ctx.short_probability + ctx.hold_probability;
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn lifted_lean_bias_mirror_is_sign_symmetric() {
        // v6.10.17 equal long/short possibility: the exact mirror of the
        // LEAN-bearish capture (Bullish, composite −2.6) must swap the
        // long/short arms exactly and keep hold identical.
        let mut bear = make_analysis_with_quality(QualityLevel::Average);
        bear.bias = MarketBias::Bearish;
        bear.market_bias_score = 0.026;
        let mut bull = make_analysis_with_quality(QualityLevel::Average);
        bull.bias = MarketBias::Bullish;
        bull.market_bias_score = -0.026;
        let risk = make_risk_with_overall(41.87);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.0,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx_bear = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            55.0,
            &bear,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        let ctx_bull = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            55.0,
            &bull,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx_bear.long_probability, ctx_bull.short_probability);
        assert_eq!(ctx_bear.short_probability, ctx_bull.long_probability);
        assert_eq!(ctx_bear.hold_probability, ctx_bull.hold_probability);
        assert_eq!(ctx_bear.net_bias_pct, -ctx_bull.net_bias_pct);
    }

    #[test]
    fn flat_neutral_state_keeps_hold_dominant() {
        // v6.10.17: the genuinely flat state (Neutral bias, no qualifying
        // profiles, no directional offset) keeps its hold-dominant split —
        // HOLD ≥ 90 is the EXCEPTION reserved for real no-direction.
        let analysis = make_analysis_with_quality(QualityLevel::Average);
        let risk = make_risk_with_overall(41.87);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.0,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            55.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Neutral");
        assert!(ctx.hold_probability >= 90.0);
        assert!(ctx.long_probability >= 2.0);
        assert!(ctx.short_probability >= 2.0);
        assert!(
            (ctx.long_probability + ctx.short_probability + ctx.hold_probability - 100.0).abs()
                < 1e-9
        );
    }

    #[test]
    fn sub_1_real_rr_still_penalizes_directional_arm() {
        // v6.10.17: a REAL sub-1.0 R:R (0.5, not 0) still applies the ×0.6
        // penalty — only a MISSING R:R (0) is exempt.
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bearish;
        analysis.market_bias_score = 0.026; // wire fraction of the 2.6 composite
        let risk = make_risk_with_overall(41.87);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.5,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            55.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        // 0.5 × (1 − 0.4187) = 0.2907 < 1.0 → penalty applies: the short
        // arm is compressed, so HOLD overtakes it while the split stays
        // graded (hold ≤ 60, short ≥ 15).
        assert!(ctx.hold_probability > ctx.short_probability);
        assert!(ctx.hold_probability <= 60.0);
        assert!(ctx.short_probability >= 15.0);
        assert!(ctx.short_probability > ctx.long_probability);
        let total = ctx.long_probability + ctx.short_probability + ctx.hold_probability;
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn lean_floor_applied_flag_tracks_the_boost() {
        // v6.10.19 (P6): when the graded-lean floors actually move the
        // split (HOLD capped at 60% / arm raised to 15%), the flag tells
        // the UI to render the LEAN annotation.
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bearish;
        analysis.market_bias_score = 0.026; // lifted
        let risk = make_risk_with_overall(41.87);
        let indicators = HashMap::new();
        // Low opportunity score → high entry danger → hold > 60 pre-cap.
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 20.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.3,
            long_expected_rr_internal: 0.0,
            short_expected_rr_internal: 0.0,
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            30.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(
            ctx.lean_floor_applied,
            "floors must be flagged when they adjust the split"
        );
        // The cap fires pre-renormalization; the final renormalization
        // lets HOLD absorb the rounding residual (≤ ~62), never the
        // pre-cap value.
        assert!(ctx.hold_probability <= 62.0);
    }

    #[test]
    fn lean_floor_applied_false_for_a_natural_split() {
        // A strong directional read whose split already satisfies the
        // floors needs no boost — no annotation.
        let mut analysis = make_analysis_with_quality(QualityLevel::Good);
        analysis.bias = MarketBias::Bullish;
        analysis.market_bias_score = 0.0; // lifted
        let risk = make_risk_with_overall(20.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Scalp,
            opportunity_score: 85.0,
            setup_quality: crate::analysis::SetupQuality::Prime,
            profiles: vec![],
            forecast_confidence: 0.85,
            long_expected_rr_internal: 2.5,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            90.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(!ctx.lean_floor_applied);
    }

    #[test]
    fn forming_requires_a_qualifying_profile() {
        // v6.10.19 (T4): a directional read with NO qualifying profile is
        // a dead no-clear market — readiness WATCH (the lean stays
        // visible via the decoupling), never a misleading "FORMING".
        let mut analysis = make_analysis_with_quality(QualityLevel::Good);
        analysis.bias = MarketBias::Bullish;
        // Lifted (score 0 → guidance Long despite the risk gate) with a
        // confidence that keeps the state BELOW the READY arm (conf 32.8).
        analysis.market_bias_score = 0.0;
        let risk = make_risk_with_overall(60.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 40.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.4,
            long_expected_rr_internal: 2.0,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            40.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.trade_readiness, "WATCH");
    }

    #[test]
    fn forming_fires_with_a_qualifying_profile() {
        // The same directional read WITH a qualifying profile (2/3
        // preconditions) is a real coiling setup → FORMING.
        let mut analysis = make_analysis_with_quality(QualityLevel::Good);
        analysis.bias = MarketBias::Bullish;
        analysis.market_bias_score = 0.0;
        let risk = make_risk_with_overall(60.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Scalp,
            opportunity_score: 60.0,
            setup_quality: crate::analysis::SetupQuality::Moderate,
            profiles: vec![crate::analysis::OpportunityProfile {
                opportunity_type: OpportunityType::Scalp,
                score: 58.0,
                preconditions_met: 2,
                preconditions_total: 3,
                notes: "Scalp".into(),
                direction_family: Some(crate::analysis::DirectionFamily::TrendRiding),
                long_entry_zone: None,
                long_target_zone: None,
                long_invalidation_level: None,
                long_expected_rr_internal: 0.0,
                short_entry_zone: None,
                short_target_zone: None,
                short_invalidation_level: None,
                short_expected_rr_internal: 0.0,
                long_geometry_consistent: false,
                short_geometry_consistent: false,
                trade_viability: None,
                scoring_factors: None,
                display_score: Some(39.0),
            }],
            forecast_confidence: 0.6,
            long_expected_rr_internal: 2.0,
            time_horizon: "SCALP".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            40.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.trade_readiness, "FORMING");
    }

    #[test]
    fn bias_keyed_rr_penalty_applies_even_when_the_risk_gate_says_neutral() {
        // v6.10.18 (I-1 + I-1b): plain Bullish (NOT lifted — composite
        // 21.77 > 20) at risk 41.07 ≥ 40 → the §3.1 risk gate produces
        // Neutral guidance — but the sub-1.0 bracket must STILL cap the
        // directional conviction (bias-keyed penalty), never inflate it.
        let mut analysis = make_analysis_with_quality(QualityLevel::Average);
        analysis.bias = MarketBias::Bullish;
        analysis.market_bias_score = 0.2177; // wire fraction of 21.77
        let risk = make_risk_with_overall(41.07);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::Scalp,
            opportunity_score: 58.69,
            setup_quality: crate::analysis::SetupQuality::Moderate,
            profiles: vec![],
            forecast_confidence: 0.71,
            long_expected_rr_internal: 0.55,
            short_expected_rr_internal: 0.0,
            time_horizon: "SCALP".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            71.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(ctx.bias, "Bullish");
        // The split stays GRADED with the penalty: long ≈ 57, hold ≈ 41,
        // net ≈ +55 — versus ~75/23 unpenalized (the 0.55 R:R must cap).
        assert!(ctx.long_probability > ctx.hold_probability);
        assert!(ctx.hold_probability <= 60.0);
        assert!(ctx.long_probability < 75.0);
        assert!(ctx.net_bias_pct > 30.0);
        let total = ctx.long_probability + ctx.short_probability + ctx.hold_probability;
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn no_clear_opportunity_active_side_rr_is_zero_propagated() {
        let analysis = make_analysis_with_quality(QualityLevel::Good);
        let risk = make_risk_with_overall(20.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::NoClearOpportunity,
            opportunity_score: 0.0,
            setup_quality: crate::analysis::SetupQuality::None,
            profiles: vec![],
            forecast_confidence: 0.0,
            long_expected_rr_internal: 0.0, // explicit Neutral sentinel
            time_horizon: "INTRADAY".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            0.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!((ctx.expected_reward_risk_ratio - 0.0).abs() < 1e-9);
        assert!(ctx.long_probability + ctx.short_probability + ctx.hold_probability > 0.0);
    }

    #[test]
    fn probabilities_sum_to_100_for_strong_bullish_ready() {
        let mut analysis = make_analysis_with_quality(QualityLevel::Excellent);
        analysis.bias = MarketBias::StrongBullish;
        let risk = make_risk_with_overall(10.0);
        let indicators = HashMap::new();
        let opp = crate::opportunity::OpportunityMatrix {
            symbol: "BTC-USD".to_string(),
            primary_opportunity: OpportunityType::TrendContinuation,
            opportunity_score: 100.0,
            setup_quality: crate::analysis::SetupQuality::Prime,
            profiles: vec![],
            forecast_confidence: 0.85,
            long_expected_rr_internal: 3.0,
            time_horizon: "SWING".to_string(),
            ..Default::default()
        };
        let ctx = DecisionContext::compute(
            &indicators,
            100.0,
            1.0,
            90.0,
            &analysis,
            Some(&opp),
            &risk,
            &crate::decision_params::DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        let total = ctx.long_probability + ctx.short_probability + ctx.hold_probability;
        assert!(
            (total - 100.0).abs() < 1e-9,
            "probabilities must sum to 100, got {}",
            total
        );
        assert_eq!(
            ctx.net_bias_pct,
            ctx.long_probability - ctx.short_probability
        );
        assert!(ctx.long_probability >= 2.0);
        assert!(ctx.short_probability >= 2.0);
        assert!(ctx.hold_probability >= 2.0);
        // High-conviction bullish → long should dominate
        assert!(ctx.long_probability > ctx.short_probability);
        assert!(ctx.net_bias_pct > 20.0);
    }
}
