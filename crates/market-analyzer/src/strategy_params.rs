//! # Strategy → runtime-params conversions (v9)
//!
//! `config-models::StrategyConfig` is the persisted single source of
//! truth; `core-domain::DecisionParams` and `synthesis::OpportunityParams`
//! are the runtime structs the hot path reads. This module converts the
//! strategy's `l6` section into the shared `DecisionParams` (the seed the
//! F-05 unification created). Defaults reproduce the pre-v9 constants.

/// Build the shared L6 `DecisionParams` from the strategy's `l6` section.
pub fn decision_params_from_strategy(
    l6: &config_models::L6Params,
) -> core_domain::decision_params::DecisionParams {
    use core_domain::decision_params::DecisionParams;
    let mut p = DecisionParams::default();
    p.confluence_weights = l6.synthesis.confluence_weights;
    p.risk_discount_k = l6.synthesis.risk_discount_k;
    p.opportunity_fallback = l6.synthesis.opportunity_fallback;
    p.stance_risk_avoid = l6.stance.risk.avoid;
    p.stance_risk_cautious = l6.stance.risk.cautious;
    p.stance_risk_neutral = l6.stance.risk.neutral;
    p.stance_risk_constructive = l6.stance.risk.constructive;
    p.stance_risk_aggressive = l6.stance.risk.aggressive;
    p.direction_risk_strong = l6.direction.risk_strong;
    p.direction_risk_plain = l6.direction.risk_plain;
    p.entry_vol_no_entry = l6.entry.vol_risk_no_entry;
    p.entry_vol_immediate = l6.entry.vol_risk_immediate;
    p.entry_vol_breakout = l6.entry.vol_risk_breakout;
    p.exit_risk_increasing = l6.exit.risk_increasing;
    p.exit_trend_weakening = l6.exit.trend_weakening;
    p.protection_vol_risk = l6.protection.vol_risk;
    p.sr_proximity_atr_mult = l6.protection.sr_proximity_atr_mult;
    p.target_rr_based = l6.target.rr_based;
    p.target_trailing = l6.target.trailing;
    p.stop_base_mult_strong = l6.stop.base_multiplier.strong;
    p.stop_base_mult_weak = l6.stop.base_multiplier.weak;
    p.stop_base_pct = l6.stop.base_pct;
    p.stop_base_clamp_min = l6.stop.base_clamp[0];
    p.stop_base_clamp_max = l6.stop.base_clamp[1];
    p.stop_vol_bump_scale = l6.stop.vol_bump_scale;
    p.stop_final_clamp_min = l6.stop.final_clamp[0];
    p.stop_final_clamp_max = l6.stop.final_clamp[1];
    if let Some(q) = l6.entry_danger.quality_penalties.get("Excellent") {
        p.quality_penalties[0] = *q;
    }
    if let Some(q) = l6.entry_danger.quality_penalties.get("Good") {
        p.quality_penalties[1] = *q;
    }
    if let Some(q) = l6.entry_danger.quality_penalties.get("Average") {
        p.quality_penalties[2] = *q;
    }
    if let Some(q) = l6.entry_danger.quality_penalties.get("Weak") {
        p.quality_penalties[3] = *q;
    }
    if let Some(q) = l6.entry_danger.quality_penalties.get("Poor") {
        p.quality_penalties[4] = *q;
    }
    p.entry_danger_quality_weight = l6.entry_danger.blend[0];
    p.readiness_aside_max = l6.readiness.aside_max;
    p.readiness_ready_min = l6.readiness.ready_min;
    let prob = &l6.probability;
    p.prob_guidance_amp = prob.guidance_amp;
    p.prob_guidance_atten = prob.guidance_atten;
    p.prob_stance_amp = prob.stance_amp;
    p.prob_avoid_atten = prob.avoid_atten;
    p.prob_avoid_hold_amp = prob.avoid_hold_amp;
    p.prob_rr_penalty = prob.rr_penalty;
    p.prob_min_pct = prob.min_pct;
    p.prob_hold_cap = prob.hold_cap;
    p.prob_arm_floor = prob.arm_floor;
    p.prob_geometric_offset = prob.geometric_offset;
    p.prob_eff_conf_floor = prob.eff_conf_floor;
    p.prob_hold_scale = prob.hold_scale;
    p.prob_contributing_conf_min = prob.contributing_conf_min;
    p.risk_ceiling_max_overall_risk = l6.risk_ceiling.max_overall_risk;
    p
}

/// Build the L3 runtime `AnalysisParams` from the strategy's `l3` section.
pub fn analysis_params_from_strategy(
    l3: &config_models::L3Params,
) -> core_domain::analysis::AnalysisParams {
    use core_domain::analysis::AnalysisParams;
    let mut p = AnalysisParams::default();
    p.bias_strong = l3.bias.bands.strong;
    p.bias_plain = l3.bias.bands.plain;
    let g = &l3.bias.grace;
    p.grace_band_min = g.band[0];
    p.grace_band_max = g.band[1];
    p.grace_vote_min = g.vote_min;
    p.grace_flat_tf = g.flat_tf as i32;
    p.grace_agreement_min = g.agreement_min;
    p.grace_signals_min = g.signals_min;
    p.grace_haircut = g.haircut;
    p.grace_hold_band_min = g.hold.band_min;
    p.grace_hold_vote_min = g.hold.vote_min;
    p.grace_skip_regime = g.skip_regime.clone();
    p.lean_tolerance = l3.bias.lean.tolerance;
    p.lean_haircut = l3.bias.lean.haircut;
    let c = &l3.confidence;
    p.conf_agreement_bonus = c.agreement.bonus;
    p.conf_agreement_min = c.agreement.min;
    p.conf_conflict_cap = c.conflict.bonus;
    p.conf_conflict_max = c.conflict.min;
    p.conf_signal_bonus = c.signals.bonus;
    p.conf_signals_min = c.signals.min as u32;
    p.conf_single_tf_cap = c.single_tf_cap;
    let r = &l3.regime;
    p.regime_bbwp_expansion = r.bbwp.expansion;
    p.regime_bbwp_contraction = r.bbwp.contraction;
    p.regime_adx = r.adx;
    p.regime_trend_score = r.trend_score;
    p.regime_missing_bbwp = r.missing.bbwp;
    p.regime_missing_adx = r.missing.adx;
    p.assessment_trend = l3.assessments.trend;
    p.assessment_momentum = l3.assessments.momentum;
    p.assessment_structure = l3.assessments.structure;
    p.assessment_volatility = l3.assessments.volatility;
    p.assessment_volume = l3.assessments.volume;
    p.quality_bands = l3.quality_bands;
    p.phase_low_vol_max = l3.phase.low_vol_max;
    p.phase_trend_score = l3.phase.trend_score;
    p.phase_volume_strong = l3.phase.volume_strong;
    p.phase_structure_healthy = l3.phase.structure_healthy;
    p.phase_volume_delta = l3.phase.volume_delta;
    p
}

/// Build the L2 runtime `AlignmentParams` from the strategy's `l2` section.
pub fn alignment_params_from_strategy(
    l2: &config_models::L2Params,
) -> core_domain::alignment::AlignmentParams {
    use core_domain::alignment::AlignmentParams;
    let mut p = AlignmentParams::default();
    p.tf_weight_mode = l2.tf_weighting.mode.clone();
    p.tf_weights = l2.tf_weighting.weights.clone();
    p.tf_weight_floor = l2.tf_weighting.floor;
    p.tf_weight_ceil = l2.tf_weighting.ceil;
    p.blend_trend = l2.overall_blend.trend;
    p.blend_momentum = l2.overall_blend.momentum;
    p.blend_volume = l2.overall_blend.volume;
    p.blend_volatility = l2.overall_blend.volatility;
    p.thin_volume_enabled = l2.thin_volume.enabled;
    p.thin_volume_threshold = l2.thin_volume.threshold;
    p.thin_blend_trend = l2.thin_volume.blend.trend;
    p.thin_blend_momentum = l2.thin_volume.blend.momentum;
    p.thin_blend_volume = l2.thin_volume.blend.volume;
    p.thin_blend_volatility = l2.thin_volume.blend.volatility;
    p.min_confluence_tfs = l2.confluence.min_tfs as u32;
    p.trend_agreement_weighted = l2.trend_agreement_weighted;
    p.dimension_mask = l2.dimension_mask.clone();
    p.overall_label_bands = l2.states.overall_label_bands;
    p
}

/// Build the L5 runtime `RiskParams` from the strategy's `l5` section.
pub fn risk_params_from_strategy(l5: &config_models::L5Params) -> core_domain::risk::RiskParams {
    use core_domain::risk::*;
    let mut p = RiskParams::default();
    let w = &l5.overall_weights;
    p.weights_market = w.get("market").copied().unwrap_or(0.14);
    p.weights_volatility = w.get("volatility").copied().unwrap_or(0.14);
    p.weights_execution_liquidity = w.get("execution_liquidity").copied().unwrap_or(0.14);
    p.weights_structure = w.get("structure").copied().unwrap_or(0.10);
    p.weights_momentum = w.get("momentum").copied().unwrap_or(0.14);
    p.weights_signal = w.get("signal").copied().unwrap_or(0.10);
    p.weights_execution = w.get("execution").copied().unwrap_or(0.10);
    p.weights_cascade = w.get("cascade").copied().unwrap_or(0.14);
    p.bands = l5.bands;
    p.state_delta = l5.state_delta;
    let d = &l5.dimensions;
    p.market = RiskMarketParams {
        baseline: d.market.baseline,
        weak_trend: d.market.weak_trend,
        broken_structure: d.market.broken_structure,
        poor_quality: d.market.poor_quality,
        low_conf_max: d.market.low_conf_max,
        low_conf: d.market.low_conf,
        contradicting: d.market.contradicting,
        strong_trend: d.market.strong_trend,
        high_conf_min: d.market.high_conf_min,
        high_conf: d.market.high_conf,
    };
    p.volatility = RiskVolatilityParams {
        baseline: d.volatility.baseline,
        bbwp_extreme: d.volatility.bbwp_extreme,
        bbwp_extreme_add: d.volatility.bbwp_extreme_add,
        bbwp_elevated: d.volatility.bbwp_elevated,
        bbwp_elevated_add: d.volatility.bbwp_elevated_add,
        squeeze_add: d.volatility.squeeze_add,
        micro_fast_blend: d.volatility.micro_fast_blend,
        atr_pct_floor: d.volatility.atr_pct_floor,
        atr_pct_max: d.volatility.atr_pct_max,
    };
    p.execution_liquidity = RiskExecLiquidityParams {
        baseline: d.execution_liquidity.baseline,
        rvol_very_low: d.execution_liquidity.rvol_very_low,
        rvol_very_low_add: d.execution_liquidity.rvol_very_low_add,
        rvol_low: d.execution_liquidity.rvol_low,
        rvol_low_add: d.execution_liquidity.rvol_low_add,
        rvol_high: d.execution_liquidity.rvol_high,
        rvol_high_add: d.execution_liquidity.rvol_high_add,
        spread_wide: d.execution_liquidity.spread_wide,
        spread_wide_add: d.execution_liquidity.spread_wide_add,
        spread_tight: d.execution_liquidity.spread_tight,
        spread_tight_add: d.execution_liquidity.spread_tight_add,
    };
    p.structure = RiskStructureParams {
        baseline: d.structure.baseline,
        broken: d.structure.broken,
        weak: d.structure.weak,
        healthy: d.structure.healthy,
        flip: d.structure.flip,
    };
    p.momentum = RiskMomentumParams {
        baseline: d.momentum.baseline,
        exhausted: d.momentum.exhausted,
        reversing: d.momentum.reversing,
        weakening: d.momentum.weakening,
        increasing: d.momentum.increasing,
    };
    p.signal = RiskSignalParams {
        baseline: d.signal.baseline,
        per_contradicting: d.signal.per_contradicting,
        contradicting_cap: d.signal.contradicting_cap,
        none_active: d.signal.none_active,
        low_conf_max: d.signal.low_conf_max,
        low_conf: d.signal.low_conf,
    };
    p.execution = RiskExecutionParams {
        baseline: d.execution.baseline,
        spread_wide: d.execution.spread_wide,
        spread_wide_add: d.execution.spread_wide_add,
        spread_moderate: d.execution.spread_moderate,
        spread_moderate_add: d.execution.spread_moderate_add,
        rvol_low: d.execution.rvol_low,
        rvol_add: d.execution.rvol_add,
        ratio_tiers: d
            .execution
            .ratio_tiers
            .iter()
            .map(|t| (t.max, t.min, t.add))
            .collect(),
    };
    p.cascade = RiskCascadeParams {
        baseline: d.cascade.baseline,
        sustained: d.cascade.sustained,
        detected: d.cascade.detected,
        asymmetry_min: d.cascade.asymmetry_min,
        asymmetry_scale: d.cascade.asymmetry_scale,
        oi_divergence_max: d.cascade.oi_divergence_max,
        funding_flip_max: d.cascade.funding_flip_max,
    };
    p
}

/// Build the L7 runtime `OverviewParams` from the strategy's `l7` section.
pub fn overview_params_from_strategy(
    l7: &config_models::L7Params,
) -> core_domain::overview::OverviewParams {
    use core_domain::overview::OverviewParams;
    let mut p = OverviewParams::default();
    p.breadth_strong = l7.breadth_bands.strong;
    p.breadth_positive = l7.breadth_bands.positive;
    p.breadth_balanced = l7.breadth_bands.balanced;
    p.global_bias_strong_share = l7.global_bias.strong_share;
    p.global_bias_plain_share = l7.global_bias.plain_share;
    p.sync_bands = l7.sync_bands;
    p.risk_low_max = l7.risk.dist_bins.low_max;
    p.risk_high_min = l7.risk.dist_bins.high_min;
    p.env_mean_high = l7.risk.env_mean.high;
    p.env_mean_moderate = l7.risk.env_mean.moderate;
    p.systemic_high_weight = l7.systemic.weights[0];
    p.systemic_sync_weight = l7.systemic.weights[1];
    let sp = &l7.systemic.sync_penalty;
    p.sync_penalty = [
        sp.get("highly_synchronized").copied().unwrap_or(100.0),
        sp.get("synchronized").copied().unwrap_or(60.0),
        sp.get("mixed").copied().unwrap_or(30.0),
        sp.get("fragmented").copied().unwrap_or(10.0),
        sp.get("highly_fragmented").copied().unwrap_or(0.0),
    ];
    let td = &l7.systemic.tf_decay;
    p.tf_decay = [
        td.get("micro1").copied().unwrap_or(0.05),
        td.get("micro2").copied().unwrap_or(0.05),
        td.get("fast1").copied().unwrap_or(0.05),
        td.get("fast2").copied().unwrap_or(0.1),
        td.get("slow1").copied().unwrap_or(0.1),
        td.get("slow2").copied().unwrap_or(0.15),
        td.get("macro1").copied().unwrap_or(0.15),
        td.get("macro2").copied().unwrap_or(0.15),
        td.get("longterm1").copied().unwrap_or(0.1),
        td.get("longterm2").copied().unwrap_or(0.1),
    ];
    p.cascade_index_fallback = l7.systemic.cascade_index_fallback;
    p.entry_veto_threshold = l7.systemic.entry_veto_threshold;
    p.asset_rank_slope = l7.asset_rank.slope;
    p.asset_rank_offset = l7.asset_rank.offset;
    p.low_coverage_min_symbols = l7.low_coverage_min_symbols;
    p.alignment_buckets = l7.alignment_buckets;
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_strategy_l6_round_trips_to_default_decision_params() {
        let l6 = config_models::L6Params::default();
        let p = decision_params_from_strategy(&l6);
        assert_eq!(p, core_domain::decision_params::DecisionParams::default());
    }

    #[test]
    fn custom_stop_and_ceiling_flow_through() {
        let mut l6 = config_models::L6Params::default();
        l6.stop.base_pct = 3.5;
        l6.stop.final_clamp = [1.0, 10.0];
        l6.risk_ceiling.max_overall_risk = Some(60.0);
        l6.stance.risk.avoid = 75.0;
        let p = decision_params_from_strategy(&l6);
        assert_eq!(p.stop_base_pct, 3.5);
        assert_eq!(p.stop_final_clamp_max, 10.0);
        assert_eq!(p.risk_ceiling_max_overall_risk, Some(60.0));
        assert_eq!(p.stance_risk_avoid, 75.0);
    }

    #[test]
    fn quality_penalties_map_in_order() {
        let mut l6 = config_models::L6Params::default();
        l6.entry_danger
            .quality_penalties
            .insert("Poor".into(), 90.0);
        let p = decision_params_from_strategy(&l6);
        assert_eq!(p.quality_penalty(4), 90.0);
        assert_eq!(p.quality_penalty(0), 10.0);
    }
}
