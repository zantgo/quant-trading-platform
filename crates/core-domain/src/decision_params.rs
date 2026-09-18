//! # Decision Params — the L6 parameter struct (v9 F-05)
//!
//! Single source of truth for every numeric constant in the Decision
//! Layer synthesis. `advisory.rs` (AdvisoryMatrix) and
//! `decision_context.rs` (DecisionContext + probabilities) previously
//! duplicated the stance grid, direction risk gates, entry-wait mirrors,
//! and the confidence formula — kept in sync by convention and audit
//! fixes. Both consumers now read ONE `&DecisionParams`.
//!
//! Every default reproduces the pre-v9 constants byte-for-byte. This
//! struct is the seed of the strategy's `l6` section (the strategy
//! param struct wires non-default values in the strategy phase).
//!
//! The confluence-score blend also lives here: the analyzer and the BTE
//! historical runner previously hardcoded the same `0.50/0.30/0.20`
//! blend independently of the Decision layer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionParams {
    // ── Synthesis core ──────────────────────────────────────────────
    /// Unsigned confluence blend `[L2 tradability, L3 quality, L4 opportunity]`.
    pub confluence_weights: [f64; 3],
    /// `confidence = state_confidence × (1 − k·risk/100)` — applied in
    /// `confidence_assessment`, `expected_reward_risk_ratio`, and the
    /// probability path.
    pub risk_discount_k: f64,
    /// Pre-warmup opportunity-score sentinel.
    pub opportunity_fallback: f64,

    // ── MarketStance grid (quality × risk, ordered rules) ───────────
    pub stance_risk_avoid: f64,
    pub stance_risk_cautious: f64,
    pub stance_risk_neutral: f64,
    pub stance_risk_constructive: f64,
    pub stance_risk_aggressive: f64,

    // ── Directional guidance risk gates ─────────────────────────────
    pub direction_risk_strong: f64,
    pub direction_risk_plain: f64,

    // ── Entry guidance (volatility-risk borders) ────────────────────
    pub entry_vol_no_entry: f64,
    pub entry_vol_immediate: f64,
    pub entry_vol_breakout: f64,

    // ── Exit guidance (overall-risk borders) ────────────────────────
    pub exit_risk_increasing: f64,
    pub exit_trend_weakening: f64,

    // ── Protection strategy ─────────────────────────────────────────
    /// VOLATILITY_BASED trigger (volatility-risk score).
    pub protection_vol_risk: f64,
    /// `SR_BASED` proximity precondition: `distance_to_SR < mult · ATR`
    /// (v9 F-02 divergence fix).
    pub sr_proximity_atr_mult: f64,

    // ── Target strategy (overall-risk borders) ──────────────────────
    pub target_rr_based: f64,
    pub target_trailing: f64,

    // ── Stop-loss distance formula ──────────────────────────────────
    pub stop_base_mult_strong: f64,
    pub stop_base_mult_weak: f64,
    pub stop_base_pct: f64,
    pub stop_base_clamp_min: f64,
    pub stop_base_clamp_max: f64,
    pub stop_vol_bump_scale: f64,
    pub stop_final_clamp_min: f64,
    pub stop_final_clamp_max: f64,

    // ── entry_danger ────────────────────────────────────────────────
    /// `[EXCELLENT, GOOD, AVERAGE, WEAK, POOR]` quality penalties.
    pub quality_penalties: [f64; 5],
    /// Weight of the quality penalty vs `100 − opportunity_score`
    /// (0.5 = the canonical mean).
    pub entry_danger_quality_weight: f64,

    // ── Trade readiness ─────────────────────────────────────────────
    pub readiness_aside_max: f64,
    pub readiness_ready_min: f64,

    // ── Long/short/hold probability split ───────────────────────────
    pub prob_guidance_amp: f64,
    pub prob_guidance_atten: f64,
    pub prob_stance_amp: f64,
    pub prob_avoid_atten: f64,
    pub prob_avoid_hold_amp: f64,
    pub prob_rr_penalty: f64,
    pub prob_min_pct: f64,
    pub prob_hold_cap: f64,
    pub prob_arm_floor: f64,
    pub prob_geometric_offset: f64,
    pub prob_eff_conf_floor: f64,
    pub prob_hold_scale: f64,
    pub prob_contributing_conf_min: f64,

    // ── Risk ceiling (v9 — soft-block) ──────────────────────────────
    /// `None` = no ceiling (today's behavior). When set, readiness
    /// floors at WATCH and the recommendation carries the
    /// `risk_blocked` stamp.
    pub risk_ceiling_max_overall_risk: Option<f64>,
}

impl Default for DecisionParams {
    fn default() -> Self {
        Self {
            confluence_weights: [0.50, 0.30, 0.20],
            risk_discount_k: 1.0,
            opportunity_fallback: 50.0,
            stance_risk_avoid: 90.0,
            stance_risk_cautious: 75.0,
            stance_risk_neutral: 50.0,
            stance_risk_constructive: 55.0,
            stance_risk_aggressive: 45.0,
            direction_risk_strong: 50.0,
            direction_risk_plain: 40.0,
            entry_vol_no_entry: 75.0,
            entry_vol_immediate: 40.0,
            entry_vol_breakout: 30.0,
            exit_risk_increasing: 80.0,
            exit_trend_weakening: 60.0,
            protection_vol_risk: 60.0,
            sr_proximity_atr_mult: 0.5,
            target_rr_based: 40.0,
            target_trailing: 60.0,
            stop_base_mult_strong: 1.0,
            stop_base_mult_weak: 1.5,
            stop_base_pct: 2.0,
            stop_base_clamp_min: 0.5,
            stop_base_clamp_max: 5.0,
            stop_vol_bump_scale: 10.0,
            stop_final_clamp_min: 0.5,
            stop_final_clamp_max: 15.0,
            quality_penalties: [10.0, 25.0, 50.0, 70.0, 80.0],
            entry_danger_quality_weight: 0.5,
            readiness_aside_max: 20.0,
            readiness_ready_min: 20.0,
            prob_guidance_amp: 1.2,
            prob_guidance_atten: 0.5,
            prob_stance_amp: 1.15,
            prob_avoid_atten: 0.5,
            prob_avoid_hold_amp: 1.5,
            prob_rr_penalty: 0.6,
            prob_min_pct: 2.0,
            prob_hold_cap: 60.0,
            prob_arm_floor: 15.0,
            prob_geometric_offset: 0.15,
            prob_eff_conf_floor: 0.5,
            prob_hold_scale: 50.0,
            prob_contributing_conf_min: 0.6,
            risk_ceiling_max_overall_risk: None,
        }
    }
}

impl DecisionParams {
    /// Quality penalty for a `QualityLevel` index into
    /// `[EXCELLENT, GOOD, AVERAGE, WEAK, POOR]`.
    pub fn quality_penalty(&self, idx: usize) -> f64 {
        self.quality_penalties.get(idx).copied().unwrap_or(50.0)
    }

    /// `confidence_assessment = state_confidence × (1 − k·risk/100) × 100`.
    pub fn confidence_assessment(&self, state_confidence: f64, overall_risk: f64) -> f64 {
        (state_confidence * (1.0 - self.risk_discount_k * overall_risk / 100.0) * 100.0)
            .clamp(0.0, 100.0)
    }

    /// The risk-discount factor `(1 − k·risk/100)`.
    pub fn risk_discount(&self, overall_risk: f64) -> f64 {
        (1.0 - self.risk_discount_k * overall_risk / 100.0).max(0.0)
    }

    /// `stop_loss_distance_pct` per 02-04 §3.6: stance-keyed base plus a
    /// volatility-risk bump, clamped to the final band.
    pub fn stop_loss_distance_pct(
        &self,
        strong_structure: bool,
        volatility_risk_score: f64,
    ) -> f64 {
        let base_multiplier = if strong_structure {
            self.stop_base_mult_strong
        } else {
            self.stop_base_mult_weak
        };
        let base_pct = (base_multiplier * self.stop_base_pct)
            .clamp(self.stop_base_clamp_min, self.stop_base_clamp_max);
        let risk_bump = (volatility_risk_score / 100.0) * self.stop_vol_bump_scale;
        (base_pct + risk_bump).clamp(self.stop_final_clamp_min, self.stop_final_clamp_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_analysis() -> crate::analysis::AnalysisMatrix {
        let mut a = crate::analysis::AnalysisMatrix::empty("BTC-USD");
        a.timeframes_considered = 4;
        a.state_confidence = 0.8;
        a.bias = crate::analysis::MarketBias::StrongBullish;
        a.market_quality = crate::analysis::QualityLevel::Excellent;
        a.market_quality_score = 90.0;
        a.trend_assessment = crate::analysis::TrendAssessment::Healthy;
        a.structure_assessment = crate::analysis::StructureAssessment::Healthy;
        a
    }

    fn low_risk() -> crate::risk::RiskMatrix {
        let mut r = crate::risk::RiskMatrix::empty("BTC-USD");
        r.overall_risk.score = 20.0;
        r.volatility_risk.score = 20.0;
        r
    }

    fn high_risk() -> crate::risk::RiskMatrix {
        let mut r = crate::risk::RiskMatrix::empty("BTC-USD");
        r.overall_risk.score = 75.0;
        r.volatility_risk.score = 50.0;
        r
    }

    #[test]
    fn default_params_have_no_ceiling() {
        assert!(DecisionParams::default()
            .risk_ceiling_max_overall_risk
            .is_none());
    }

    #[test]
    fn confidence_assessment_uses_risk_discount_k() {
        let p = DecisionParams {
            risk_discount_k: 0.5,
            ..Default::default()
        };
        let c = p.confidence_assessment(0.8, 50.0);
        // 0.8 × (1 − 0.5·0.5) × 100 = 60
        assert!((c - 60.0).abs() < 1e-9);
    }

    #[test]
    fn stop_formula_strong_vs_weak_structure() {
        let p = DecisionParams::default();
        let strong = p.stop_loss_distance_pct(true, 0.0);
        let weak = p.stop_loss_distance_pct(false, 0.0);
        assert!((strong - 2.0).abs() < 1e-9);
        assert!((weak - 3.0).abs() < 1e-9);
        let bumped = p.stop_loss_distance_pct(true, 50.0);
        // 2.0 + 0.5×10 = 7.0
        assert!((bumped - 7.0).abs() < 1e-9);
        assert_eq!(p.stop_loss_distance_pct(true, 100.0), 12.0);
    }

    #[test]
    fn risk_ceiling_floors_readiness_in_compute() {
        let p = DecisionParams {
            risk_ceiling_max_overall_risk: Some(60.0),
            ..Default::default()
        };
        let analysis = sample_analysis();
        let risk = high_risk();
        let ctx = crate::decision_context::DecisionContext::compute(
            &std::collections::HashMap::new(),
            100.0,
            1.0,
            30.0,
            &analysis,
            None,
            &risk,
            &p,
            &crate::analysis::AnalysisParams::default(),
        );
        // Risk 75 > ceiling 60 → readiness must never exceed WATCH.
        assert_eq!(ctx.trade_readiness, "WATCH");

        // Below the ceiling the ceiling does not interfere.
        let p2 = DecisionParams {
            risk_ceiling_max_overall_risk: Some(60.0),
            ..Default::default()
        };
        let ctx2 = crate::decision_context::DecisionContext::compute(
            &std::collections::HashMap::new(),
            100.0,
            1.0,
            30.0,
            &analysis,
            None,
            &low_risk(),
            &p2,
            &crate::analysis::AnalysisParams::default(),
        );
        assert_ne!(ctx2.trade_readiness, "WATCH");
    }

    #[test]
    fn risk_ceiling_stamps_advisory() {
        let p = DecisionParams {
            risk_ceiling_max_overall_risk: Some(60.0),
            ..Default::default()
        };
        let analysis = sample_analysis();
        let adv = crate::advisory::compute_advisory(
            &analysis,
            &high_risk(),
            None,
            None,
            None,
            &p,
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(adv.risk_blocked);
        assert!(adv.final_recommendation.contains("strategy ceiling"));
        let adv2 = crate::advisory::compute_advisory(
            &analysis,
            &low_risk(),
            None,
            None,
            None,
            &p,
            &crate::analysis::AnalysisParams::default(),
        );
        assert!(!adv2.risk_blocked);
    }
}
