//! # Advisory Matrix — Decision Guidance Layer
//!
//! The Advisory Matrix transforms complete market intelligence and risk
//! assessment into structured human-facing guidance. It consumes the Analysis
//! Matrix and Risk Matrix to provide an explainable recommendation framework.
//! It does not execute trades — it explains and recommends.
//!
//! Layer: L4.75 in the architecture (Decision Guidance).

use crate::analysis::{AnalysisMatrix, OpportunityType};
use crate::decision_params::DecisionParams;
use crate::opportunity::OpportunityMatrix;
use crate::risk::{RiskDimension, RiskMatrix};
use serde::{Deserialize, Serialize};

/// Directional guidance classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectionalGuidance {
    StrongLong,
    Long,
    Neutral,
    Short,
    StrongShort,
    AvoidDirectionalExposure,
}

/// Market stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStance {
    Aggressive,
    Constructive,
    Neutral,
    Cautious,
    Avoid,
}

impl std::fmt::Display for MarketStance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Opportunity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunityClass {
    TrendContinuation,
    Breakout,
    Pullback,
    MeanReversion,
    Reversal,
    LiquiditySqueeze,
    Scalp,
    NoClearOpportunity,
}

/// Strategy environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyEnvironment {
    TrendFollowing,
    Breakout,
    MeanReversion,
    HighVolatility,
    LowActivity,
    Unfavorable,
}

/// Entry guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryGuidance {
    Immediate,
    WaitForConfirmation,
    Pullback,
    Breakout,
    NoEntryContext,
}

/// Exit guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitGuidance {
    TrendWeakening,
    MomentumExhaustion,
    StructureBreakdown,
    RiskIncreasing,
    NoWarning,
}

/// Protection strategy guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectionStrategy {
    StructureBased,
    VolatilityBased,
    ATRBased,
    SRBased,
    NoRecommendation,
}

/// Target strategy guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetStrategy {
    ResistanceBased,
    RRBased,
    VolatilityBased,
    TrailingMethod,
    NoRecommendation,
}

/// Advisory Matrix — human-facing guidance per symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryMatrix {
    pub symbol: String,
    pub directional_guidance: DirectionalGuidance,
    pub market_stance: MarketStance,
    pub opportunity_classification: OpportunityClass,
    pub strategy_environment: StrategyEnvironment,
    pub entry_guidance: EntryGuidance,
    pub exit_guidance: ExitGuidance,
    pub protection_strategy: ProtectionStrategy,
    pub target_strategy: TargetStrategy,
    pub confidence_assessment: f64,
    pub stop_loss_distance_pct: f64,
    /// Cross-symbol cascade risk index — per-symbol cascade risk score
    /// from the L5 Risk Matrix, carried through to L7 Overview aggregation.
    pub cascade_risk_score: f64,
    /// Synoptic favorability of entering a position — semantic successor
    /// of the old `Risk.reward_risk` (removed in the institutional redesign).
    /// Synthesized from L3 `market_quality` and the L4 opportunity type —
    /// lower score = more favorable environment.
    pub environment_favorability: RiskDimension,
    /// Setup-efficiency metric: `analysis.market_quality_score ÷
    /// risk.overall_risk.score` (both unipolar 0-100). Higher = the setup's
    /// quality mathematically justifies the risk. `None` when overall risk
    /// is 0 (division guard).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_to_risk_ratio: Option<f64>,
    /// v9: `true` when the strategy's risk ceiling
    /// (`l6.risk_ceiling.max_overall_risk`) is breached — the advisory
    /// soft-blocks (readiness floors at WATCH) and the recommendation
    /// sentence carries the stand-aside note. `false` = no ceiling or not
    /// breached (backward-safe wire default).
    #[serde(default)]
    pub risk_blocked: bool,
    pub final_recommendation: String,
}

impl AdvisoryMatrix {
    pub fn empty(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            directional_guidance: DirectionalGuidance::Neutral,
            market_stance: MarketStance::Neutral,
            opportunity_classification: OpportunityClass::NoClearOpportunity,
            strategy_environment: StrategyEnvironment::LowActivity,
            entry_guidance: EntryGuidance::NoEntryContext,
            exit_guidance: ExitGuidance::NoWarning,
            protection_strategy: ProtectionStrategy::NoRecommendation,
            target_strategy: TargetStrategy::NoRecommendation,
            confidence_assessment: 0.0,
            stop_loss_distance_pct: 0.0,
            cascade_risk_score: 30.0,
            environment_favorability: RiskDimension::default(),
            quality_to_risk_ratio: None,
            risk_blocked: false,
            final_recommendation: "Insufficient data to provide guidance.".into(),
        }
    }
}

/// Compute `environment_favorability` — synoptic favorability of entering
/// a position. Synthesizes L3 `market_quality` and L4 `opportunity_score`.
/// Lower score = more favorable environment.
///
/// Bug-fix #18: the legacy implementation read the L3
/// deprecated opportunity enum (a coarse OpportunityType bucketed as
/// 20 vs 80) instead of the L4 `OpportunityMatrix.opportunity_score`
/// (a continuous 0-100 score). The 60-point discretization collapsed
/// every non-NoClearOpportunity case to 80, masking the difference
/// between a prime TrendContinuation (opportunity_score ≈ 90) and a
/// marginal Pullback (opportunity_score ≈ 45). We consume the L4
/// numeric score directly; when the opportunity matrix is absent
/// (early warmup), we fall back to the neutral 50 sentinel (v9 F-03:
/// the deprecated L3 enum mirror is erased entirely).
fn compute_environment_favorability(
    analysis: &AnalysisMatrix,
    opportunity: Option<&crate::opportunity::OpportunityMatrix>,
    params: &DecisionParams,
) -> RiskDimension {
    let quality_penalty: f64 = match analysis.market_quality {
        crate::analysis::QualityLevel::Excellent => params.quality_penalty(0),
        crate::analysis::QualityLevel::Good => params.quality_penalty(1),
        crate::analysis::QualityLevel::Average => params.quality_penalty(2),
        crate::analysis::QualityLevel::Weak => params.quality_penalty(3),
        crate::analysis::QualityLevel::Poor => params.quality_penalty(4),
    };
    let opportunity_score: f64 = if let Some(opp) = opportunity {
        opp.opportunity_score
    } else {
        // Pre-warmup fallback (neutral sentinel, not optimistic).
        params.opportunity_fallback
    };
    let score: f64 = ((quality_penalty + (100.0 - opportunity_score)) / 2.0).clamp(0.0, 100.0);
    RiskDimension {
        score,
        ..RiskDimension::default()
    }
}

/// Compute Advisory Matrix from Analysis + Risk + Opportunity + Cluster.
/// `opportunity` may be `None` during early warm-up.
///
/// `sr_distance_atr` (v9 F-02): the distance from the reference price to
/// the nearest structural level, expressed in ATR multiples, computed by
/// the caller from the indicator map. `None` = no structural levels
/// available. The documented `SR_BASED` protection rule requires
/// `distance_to_nearest_SR < params.sr_proximity_atr_mult · ATR`;
/// without the distance input the branch cannot fire and falls through
/// to `ATR_BASED` (fail-closed).
///
/// `params` (v9 F-05): the shared Decision-layer parameter struct —
/// `advisory.rs` and `decision_context.rs` previously duplicated these
/// grids; both now read the single source of truth.
pub fn compute_advisory(
    analysis: &AnalysisMatrix,
    risk: &RiskMatrix,
    opportunity: Option<&OpportunityMatrix>,
    sr_distance_atr: Option<f64>,
    params: &DecisionParams,
    // v9: the strategy's `l3` section (bias-lifted gate).
    analysis_params: &crate::analysis::AnalysisParams,
) -> AdvisoryMatrix {
    if analysis.timeframes_considered == 0 {
        return AdvisoryMatrix::empty(&analysis.symbol);
    }

    // Capture the L4 OpportunityMatrix reference before the local
    // `opportunity: OpportunityClass` shadow at line ~264. Bug-fix
    // #18 needs the L4 numeric `opportunity_score` (not the L3
    // bucketed enum), so we must hold onto the original `Option<&OpportunityMatrix>`
    // through the rest of the function.
    let opp_matrix = opportunity;

    // v6.10 (Phase 1 / A5): Market stance from quality × risk, per the
    // 6-rule canonical table at
    // `docs/matrices/02-04-decision-matrix.md §3.2`. We compute stance
    // BEFORE directional guidance so the AVOID short-circuit can run first
    // (see v6.10 A3 below).
    let stance = match analysis.market_quality {
        // Rule 1: POOR quality OR overall_risk ≥ avoid → AVOID.
        // Rule 2: POOR/WEAK quality OR overall_risk ≥ cautious → CAUTIOUS.
        // Rule 3: AVERAGE quality AND overall_risk < neutral → NEUTRAL.
        // Rule 4: EXCELLENT quality AND overall_risk < aggressive → AGGRESSIVE.
        // Rule 5: GOOD/EXCELLENT quality AND overall_risk < constructive → CONSTRUCTIVE.
        // Rule 6 (default): everything else → CAUTIOUS.
        //
        // NOTE: rules 1 and 2 share the POOR/WEAK triggers with rule 6;
        // ordering matters. We model them as a single match against quality
        // with risk-band sub-matches. v9 F-05: all borders come from
        // `DecisionParams` (the single source shared with
        // `decision_context.rs`).
        crate::analysis::QualityLevel::Poor => {
            if risk.overall_risk.score >= params.stance_risk_avoid {
                MarketStance::Avoid
            } else {
                // POOR + any other risk band → CAUTIOUS (rule 2)
                MarketStance::Cautious
            }
        }
        crate::analysis::QualityLevel::Weak => {
            if risk.overall_risk.score >= params.stance_risk_cautious {
                MarketStance::Cautious
            } else {
                // WEAK + risk < cautious → still CAUTIOUS (rule 2)
                MarketStance::Cautious
            }
        }
        crate::analysis::QualityLevel::Average => {
            if risk.overall_risk.score < params.stance_risk_neutral {
                MarketStance::Neutral // rule 3
            } else {
                MarketStance::Cautious // default rule 6
            }
        }
        crate::analysis::QualityLevel::Good => {
            if risk.overall_risk.score < params.stance_risk_constructive {
                MarketStance::Constructive // rule 5 (with EXCELLENT below)
            } else {
                MarketStance::Cautious // default rule 6
            }
        }
        crate::analysis::QualityLevel::Excellent => {
            if risk.overall_risk.score >= params.stance_risk_avoid {
                MarketStance::Avoid // rule 1
            } else if risk.overall_risk.score < params.stance_risk_aggressive {
                MarketStance::Aggressive // rule 4
            } else if risk.overall_risk.score < params.stance_risk_constructive {
                MarketStance::Constructive // rule 5
            } else {
                MarketStance::Cautious // default rule 6
            }
        }
    };

    // v6.10 (Phase 1 / A3): DirectionalGuidance short-circuits on AVOID
    // stance. Per spec rule 1: `market_stance == AVOID → AVOID_DIRECTIONAL_EXPOSURE`
    // must precede bias derivation. Without this guard, a POOR-quality
    // setup with StrongBullish bias would emit `StrongLong`, which is a
    // dangerous contradiction (operator should not be told to go long when
    // the matrix says "stay out").
    let directional = if stance == MarketStance::Avoid {
        DirectionalGuidance::AvoidDirectionalExposure
    } else {
        // v6.10.17: a LIFTED bias (grace / hysteresis hold / LEAN tier —
        // `bias_lifted`) is always directional regardless of the risk gate,
        // mirroring `DecisionContext::compute` so the advisory guidance and
        // the probability split can never contradict each other.
        let lifted = crate::analysis::bias_lifted(
            analysis.bias,
            analysis.market_bias_score,
            analysis_params,
        );
        match analysis.bias {
            crate::analysis::MarketBias::StrongBullish => {
                if risk.overall_risk.score < params.direction_risk_strong {
                    DirectionalGuidance::StrongLong
                } else {
                    DirectionalGuidance::Long
                }
            }
            crate::analysis::MarketBias::Bullish => {
                if lifted || risk.overall_risk.score < params.direction_risk_plain {
                    DirectionalGuidance::Long
                } else {
                    DirectionalGuidance::Neutral
                }
            }
            crate::analysis::MarketBias::StrongBearish => {
                if risk.overall_risk.score < params.direction_risk_strong {
                    DirectionalGuidance::StrongShort
                } else {
                    DirectionalGuidance::Short
                }
            }
            crate::analysis::MarketBias::Bearish => {
                if lifted || risk.overall_risk.score < params.direction_risk_plain {
                    DirectionalGuidance::Short
                } else {
                    DirectionalGuidance::Neutral
                }
            }
            crate::analysis::MarketBias::Neutral => DirectionalGuidance::Neutral,
        }
    };

    // Opportunity classification from the L4 OpportunityMatrix. v9 (F-03):
    // the deprecated L3 `opportunity_analysis` fallback is ERASED — the
    // classification belongs to L4 alone. When the L4 matrix is absent
    // (pre-warmup), the honest classification is NoClearOpportunity.
    let opportunity = if let Some(opp) = opportunity {
        match opp.primary_opportunity {
            OpportunityType::TrendContinuation => OpportunityClass::TrendContinuation,
            OpportunityType::Breakout => OpportunityClass::Breakout,
            OpportunityType::Pullback => OpportunityClass::Pullback,
            OpportunityType::MeanReversion => OpportunityClass::MeanReversion,
            OpportunityType::Reversal => OpportunityClass::Reversal,
            OpportunityType::LiquiditySqueeze => OpportunityClass::LiquiditySqueeze,
            OpportunityType::Scalp => OpportunityClass::Scalp,
            OpportunityType::NoClearOpportunity => OpportunityClass::NoClearOpportunity,
        }
    } else {
        OpportunityClass::NoClearOpportunity
    };

    // Strategy environment from regime (canonical mapping over all 8 MarketRegime values)
    let strategy_env = match analysis.market_regime {
        crate::analysis::MarketRegime::TrendingBull
        | crate::analysis::MarketRegime::TrendingBear => StrategyEnvironment::TrendFollowing,
        crate::analysis::MarketRegime::Accumulation
        | crate::analysis::MarketRegime::Distribution => StrategyEnvironment::Breakout,
        crate::analysis::MarketRegime::Range => StrategyEnvironment::MeanReversion,
        crate::analysis::MarketRegime::Expansion => StrategyEnvironment::HighVolatility,
        crate::analysis::MarketRegime::Contraction => StrategyEnvironment::LowActivity,
        _ => StrategyEnvironment::Unfavorable,
    };

    // Entry guidance: ordered first-match rules (spec §3.4)
    let entry = if risk.volatility_risk.score >= params.entry_vol_no_entry {
        EntryGuidance::NoEntryContext
    } else if matches!(
        analysis.trend_assessment,
        crate::analysis::TrendAssessment::Strong | crate::analysis::TrendAssessment::Healthy
    ) && risk.volatility_risk.score < params.entry_vol_immediate
    {
        EntryGuidance::Immediate
    } else if matches!(
        analysis.trend_assessment,
        crate::analysis::TrendAssessment::Strong | crate::analysis::TrendAssessment::Healthy
    ) {
        EntryGuidance::Pullback
    } else if analysis.trend_assessment == crate::analysis::TrendAssessment::Developing
        && risk.volatility_risk.score < params.entry_vol_breakout
    {
        EntryGuidance::Breakout
    } else if analysis.trend_assessment == crate::analysis::TrendAssessment::Developing {
        EntryGuidance::WaitForConfirmation
    } else {
        EntryGuidance::NoEntryContext
    };

    // Exit guidance: ordered first-match rules (spec §3.5)
    let exit = if risk.overall_risk.score >= params.exit_risk_increasing {
        ExitGuidance::RiskIncreasing
    } else if matches!(
        analysis.structure_assessment,
        crate::analysis::StructureAssessment::Broken
            | crate::analysis::StructureAssessment::Unknown
    ) {
        ExitGuidance::StructureBreakdown
    } else if analysis.momentum_assessment == crate::analysis::MomentumAssessment::Reversing {
        ExitGuidance::MomentumExhaustion
    } else if analysis.momentum_assessment == crate::analysis::MomentumAssessment::Weakening
        || risk.overall_risk.score >= params.exit_trend_weakening
    {
        ExitGuidance::TrendWeakening
    } else {
        ExitGuidance::NoWarning
    };

    // Protection strategy: ordered first-match rules (spec §3.6)
    let protection =
        if analysis.volatility_assessment == crate::analysis::VolatilityAssessment::Compressed {
            ProtectionStrategy::StructureBased
        } else if risk.volatility_risk.score > params.protection_vol_risk
            && matches!(
                analysis.volatility_assessment,
                crate::analysis::VolatilityAssessment::Expanding
                    | crate::analysis::VolatilityAssessment::Extreme
            )
        {
            ProtectionStrategy::VolatilityBased
        } else if analysis.market_regime == crate::analysis::MarketRegime::Range
            && matches!(
                analysis.structure_assessment,
                crate::analysis::StructureAssessment::Strong
                    | crate::analysis::StructureAssessment::Healthy
            )
            && sr_distance_atr.is_some_and(|d| d < params.sr_proximity_atr_mult)
        {
            ProtectionStrategy::SRBased
        } else {
            ProtectionStrategy::ATRBased
        };

    // Target strategy: ordered first-match rules (spec §3.7)
    let target = if matches!(
        analysis.structure_assessment,
        crate::analysis::StructureAssessment::Strong
            | crate::analysis::StructureAssessment::Healthy
    ) {
        TargetStrategy::ResistanceBased
    } else if risk.overall_risk.score < params.target_rr_based {
        TargetStrategy::RRBased
    } else if risk.overall_risk.score < params.target_trailing {
        TargetStrategy::TrailingMethod
    } else {
        TargetStrategy::VolatilityBased
    };

    // Environment favorability (synoptic L3+L4 favorability for entering).
    // Pass the original `opportunity` parameter (Option<&OpportunityMatrix>)
    // — the local `opportunity: OpportunityClass` shadow at line 264 is
    // not the L4 matrix, it's the L3 bucketed enum used by the rest of
    // `compute_advisory`. Bug-fix #18 needs the L4 numeric
    // `opportunity_score`, so we reference the parameter directly.
    let environment_favorability = compute_environment_favorability(analysis, opp_matrix, params);

    // Quality-to-Risk ratio — setup efficiency. Both inputs are unipolar
    // [0, 100] with opposite semantics (high quality good, high risk bad);
    // the ratio is higher = better. `None` when overall risk is 0.
    let quality_to_risk_ratio = if risk.overall_risk.score > 0.0 {
        Some((analysis.market_quality_score / risk.overall_risk.score * 100.0).round() / 100.0)
    } else {
        None
    };

    // Confidence: analysis.state_confidence × (1 - k·risk/100) —
    // v9 F-05: shared DecisionParams helper (same formula the
    // probability path uses).
    let confidence =
        params.confidence_assessment(analysis.state_confidence, risk.overall_risk.score);

    // Stop-loss distance: percent-scale output for the TAE type-boundary
    // handoff, `[0.5, 15.0]` percent (e.g. `2.5` means 2.5%).
    //
    // v6.10 (Phase 1 / A1): wire format is RAW PERCENTAGES in `[0.5, 15.0]`
    // (e.g. `2.5` means 2.5%). The TAE position-sizing path in
    // `crates/portfolio-supervisor/src/execution/order.rs:54-58` divides the
    // raw value by 100 to get the fractional stop distance used in the
    // sizing formula `size_quote = (E × R) / (D_sl / 100)`. The legacy
    // implementation produced fractional values in `[0.005, 0.15]`; when
    // the TAE divided by 100, the effective stop distance was 0.005%–0.15%,
    // which inflated position sizes ~100×. We now multiply the entire
    // expression by 100 so the output range is `[0.5, 15.0]` percent.
    //
    // NOTE (v2026-08 audit): the actual implementation below does NOT use
    // ATR — the relative-ATR approach documented in earlier revisions was
    // never wired (ATR is not available at this call site). The live
    // formula is a stance-keyed base (`1.0|1.5 × 2.0%`) plus a
    // `volatility_risk.score / 10` bump, clamped to `[0.5, 15.0]`. Docs
    // 02-04 §3.6 and 03-02-07 §4 describe this actual formula.
    // v9 F-05: the formula lives in `DecisionParams::stop_loss_distance_pct`.
    let strong_structure = matches!(
        analysis.structure_assessment,
        crate::analysis::StructureAssessment::Strong
            | crate::analysis::StructureAssessment::Healthy
    );
    let stop_loss_distance_pct =
        params.stop_loss_distance_pct(strong_structure, risk.volatility_risk.score);

    // v6.10.19 (T5): under a Neutral or Avoid directional read there is no
    // actionable entry — the "Entry: …. Stop: …" clauses are execution
    // instructions that must not ride on a "no directional call" sentence
    // (a trader could read them as pending-order guidance under a HOLD).
    let entry_stop_clause = if matches!(
        directional,
        DirectionalGuidance::Neutral | DirectionalGuidance::AvoidDirectionalExposure
    ) {
        String::new()
    } else {
        format!(
            " Entry: {}. Stop: {}.",
            match entry {
                EntryGuidance::Immediate => "immediate",
                EntryGuidance::WaitForConfirmation => "wait for confirmation",
                EntryGuidance::Pullback => "on pullback",
                EntryGuidance::Breakout => "on breakout",
                EntryGuidance::NoEntryContext => "no entry context",
            },
            match protection {
                ProtectionStrategy::StructureBased => "structure-based",
                ProtectionStrategy::VolatilityBased => "volatility-based",
                ProtectionStrategy::ATRBased => "ATR-based",
                ProtectionStrategy::SRBased => "S/R-based",
                ProtectionStrategy::NoRecommendation => "no recommendation",
            }
        )
    };
    // v6.10.19a (D2b): the sentence previously read "Neutral — no
    // directional edge: NEUTRAL bias with 13% confidence, …" — the claim
    // already states the read, so the "X bias with N% confidence" fragment
    // duplicated it behind a colon. Neutral/Avoid claims now carry only the
    // confidence; directional claims keep "LONG bias with 72% confidence".
    let claim = match directional {
        DirectionalGuidance::StrongLong => "Strong long bias",
        DirectionalGuidance::Long => "Long bias",
        DirectionalGuidance::StrongShort => "Strong short bias",
        DirectionalGuidance::Short => "Short bias",
        DirectionalGuidance::Neutral => "Neutral — no directional edge",
        DirectionalGuidance::AvoidDirectionalExposure => "Avoid directional exposure",
    };
    let bias_fragment = match directional {
        DirectionalGuidance::Neutral | DirectionalGuidance::AvoidDirectionalExposure => {
            format!("{:.0}% confidence", confidence)
        }
        _ => format!("{} bias with {:.0}% confidence", analysis.bias, confidence),
    };
    let recommendation = format!(
        "{}: {}, {} stance in a {} environment. {} opportunity.{}",
        claim,
        bias_fragment,
        match stance {
            MarketStance::Aggressive => "aggressive",
            MarketStance::Constructive => "constructive",
            MarketStance::Neutral => "neutral",
            MarketStance::Cautious => "cautious",
            MarketStance::Avoid => "avoid",
        },
        match strategy_env {
            StrategyEnvironment::TrendFollowing => "trend-following",
            StrategyEnvironment::Breakout => "breakout",
            StrategyEnvironment::MeanReversion => "mean-reversion",
            StrategyEnvironment::HighVolatility => "high-volatility",
            StrategyEnvironment::LowActivity => "low-activity",
            StrategyEnvironment::Unfavorable => "unfavorable",
        },
        match opportunity {
            OpportunityClass::TrendContinuation => "Trend continuation",
            OpportunityClass::Breakout => "Breakout",
            OpportunityClass::Pullback => "Pullback",
            OpportunityClass::MeanReversion => "Mean reversion",
            OpportunityClass::Reversal => "Reversal",
            OpportunityClass::LiquiditySqueeze => "Liquidity squeeze",
            OpportunityClass::Scalp => "Scalp",
            OpportunityClass::NoClearOpportunity => "No clear",
        },
        entry_stop_clause
    );

    // v9: the risk-ceiling soft-block. When the strategy sets
    // `l6.risk_ceiling.max_overall_risk` and the environment exceeds it,
    // the advisory carries the `risk_blocked` stamp and the
    // recommendation sentence gains the stand-aside note. The
    // probability split stays descriptive (locked decision); the
    // readiness floors at WATCH (decision_context.rs).
    let risk_blocked = params
        .risk_ceiling_max_overall_risk
        .is_some_and(|ceiling| risk.overall_risk.score > ceiling);
    let recommendation = if risk_blocked {
        let ceiling = params.risk_ceiling_max_overall_risk.unwrap_or(0.0);
        format!(
            "{recommendation} Environment risk {:.0} exceeds the strategy ceiling ({:.0}) — standing aside.",
            risk.overall_risk.score, ceiling
        )
    } else {
        recommendation
    };

    AdvisoryMatrix {
        symbol: analysis.symbol.clone(),
        directional_guidance: directional,
        market_stance: stance,
        opportunity_classification: opportunity,
        strategy_environment: strategy_env,
        entry_guidance: entry,
        exit_guidance: exit,
        protection_strategy: protection,
        target_strategy: target,
        confidence_assessment: confidence,
        stop_loss_distance_pct,
        cascade_risk_score: risk.cascade_risk.score,
        environment_favorability,
        quality_to_risk_ratio,
        risk_blocked,
        final_recommendation: recommendation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_analysis_returns_empty_advisory() {
        let analysis = AnalysisMatrix::empty("BTC-USD");
        let risk = RiskMatrix::empty("BTC-USD");
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(matches!(
            adv.directional_guidance,
            DirectionalGuidance::Neutral
        ));
        assert_eq!(adv.confidence_assessment, 0.0);
    }

    // v6.10.19a (D2b): the Neutral claim must not duplicate the bias behind
    // a colon ("…no directional edge: NEUTRAL bias with 13% confidence, …"),
    // and no execution clause may ride on a no-directional-call sentence.
    #[test]
    fn neutral_claim_sentence_has_no_duplicated_bias_fragment() {
        use crate::analysis::MarketBias;
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Neutral;
        let risk = RiskMatrix::empty("BTC-USD");
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(matches!(
            adv.directional_guidance,
            DirectionalGuidance::Neutral
        ));
        assert!(
            adv.final_recommendation
                .contains("Neutral — no directional edge"),
            "claim missing: {}",
            adv.final_recommendation
        );
        assert!(
            !adv.final_recommendation.contains("NEUTRAL bias with"),
            "bias duplicated behind the claim: {}",
            adv.final_recommendation
        );
        assert!(
            !adv.final_recommendation.contains("Entry:"),
            "no execution clause under a neutral read: {}",
            adv.final_recommendation
        );
    }

    // v6.10.19a (D2b): directional claims keep the "X bias with N%
    // confidence" fragment — only the Neutral/Avoid claims drop it.
    #[test]
    fn directional_claim_sentence_keeps_bias_fragment() {
        use crate::analysis::MarketBias;
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::StrongBullish;
        let risk = RiskMatrix::empty("BTC-USD");
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        // `RiskMatrix::empty` carries overall risk 50.0, so the StrongBullish
        // gate demotes to a plain LONG — the fragment shape is what matters.
        assert!(matches!(
            adv.directional_guidance,
            DirectionalGuidance::Long
        ));
        assert!(
            adv.final_recommendation
                .starts_with("Long bias: STRONG_BULLISH bias with"),
            "sentence: {}",
            adv.final_recommendation
        );
        assert!(adv.final_recommendation.contains("% confidence"));
        assert!(adv.final_recommendation.contains("Entry:"));
    }

    #[test]
    fn quality_to_risk_ratio_computed_from_quality_and_overall_risk() {
        use crate::analysis::{MarketBias, QualityLevel};
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::StrongBullish;
        analysis.market_quality = QualityLevel::Good;
        analysis.market_quality_score = 80.0;
        let mut risk = RiskMatrix::empty("BTC-USD");
        risk.overall_risk.score = 20.0;
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(adv.quality_to_risk_ratio, Some(4.0));
    }

    #[test]
    fn quality_to_risk_ratio_none_when_overall_risk_zero() {
        use crate::analysis::MarketBias;
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::StrongBullish;
        analysis.market_quality_score = 90.0;
        let mut risk = RiskMatrix::empty("BTC-USD");
        risk.overall_risk.score = 0.0;
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        assert_eq!(adv.quality_to_risk_ratio, None);
    }

    #[test]
    fn advisory_serde_skips_none_quality_to_risk_ratio() {
        let analysis = AnalysisMatrix::empty("BTC-USD");
        let risk = RiskMatrix::empty("BTC-USD");
        let adv = compute_advisory(
            &analysis,
            &risk,
            None,
            None,
            &DecisionParams::default(),
            &crate::analysis::AnalysisParams::default(),
        );
        let json = serde_json::to_string(&adv).unwrap();
        assert!(!json.contains("quality_to_risk_ratio"));
        let back: AdvisoryMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(back.quality_to_risk_ratio, None);
    }
}
