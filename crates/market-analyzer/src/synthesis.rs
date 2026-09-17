use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use tokio::sync::RwLock;

use core_domain::advisory;
use core_domain::alignment::{self, AlignmentMatrix};
use core_domain::analysis::PriceRange;
use core_domain::analysis::{
    self, AnalysisMatrix, OpportunityProfile, OpportunityType, SetupQuality,
};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::liquidity::{LiquidationClusterMatrix, LiquidityFlow};
use core_domain::market_context::MarketContext;
use core_domain::models::MarketSnapshot;
use core_domain::opportunity::{ConfluentLevel, LevelSource, NeutralBracket, OpportunityMatrix};
use core_domain::risk::{self, RiskMatrix};
use std::collections::HashMap;

pub struct CrossTfSynthesisResult {
    pub alignment: AlignmentMatrix,
    pub analysis: AnalysisMatrix,
    pub opportunity: Option<OpportunityMatrix>,
    pub risk: RiskMatrix,
    pub advisory: advisory::AdvisoryMatrix,
}

/// v9 (F-04 + strategy wiring): the full L4 opportunity-layer runtime
/// parameters. Previously the synthesis hardcoded `FALLBACK_ENABLED /
/// K_ENTRY / K_TARGET`, the net-cost model, the setup-selection tree, the
/// per-type precondition thresholds, the scoring blend, the quality bands,
/// the zone geometry, and the confluent source weights. All of them now
/// come from the strategy's `l4` section (defaults = today's behavior).
#[derive(Debug, Clone, PartialEq)]
pub struct OpportunityParams {
    pub confluent_atr_fallback_enabled: bool,
    pub confluent_atr_k_entry: f64,
    pub confluent_atr_k_target: f64,
    pub net_cost: core_domain::risk_reward::NetCostModel,
    /// Selection tree: first-match priority order over the enabled setups.
    pub setup_priority: Vec<OpportunityType>,
    /// Setups considered at all (priority ∩ enabled).
    pub setup_enabled: Vec<OpportunityType>,
    pub precondition: L4PreconditionParams,
    /// Scoring blend `[Q_ctx, S_sig, A_mtf, F_fresh]`.
    pub score_blend: [f64; 4],
    /// SetupQuality bands `[Prime, Strong, Moderate, Marginal]`.
    pub quality_bands: [f64; 4],
    pub zones: L4ZoneParams,
    /// Confluent source weights keyed by `LevelSource` debug name.
    pub confluence_weights: std::collections::HashMap<String, f64>,
    /// v9 (strategy `l1_5.signal_weights`): the 11-kind trust axis,
    /// multiplying each discrete signal's contribution to the L4
    /// signal-strength factor. Empty = all 1.0 (v8.2 behavior).
    pub signal_weights: std::collections::HashMap<String, f64>,
    /// v10.1: strategy-driven viability floor — the net R:R a bracket
    /// must clear to be `Actionable`. Default 1.0 reproduces v8.2;
    /// strategies can loosen to 0.5 etc. via `tae.intake.min_net_rr`.
    pub viability_min_net_rr: f64,
    /// v11: TF-role separation — `decision_tf` feeds the representative
    /// indicator set (bias/regime/stance/risk) when enabled; zones still
    /// derive from the merged level surface. Empty = legacy first-TF-wins.
    pub ladder_roles: config_models::LadderRoles,
}

/// Per-setup-type precondition thresholds (defaults = the pre-v9 tree).
#[derive(Debug, Clone, PartialEq)]
pub struct L4PreconditionParams {
    pub trend_continuation_trend_min: f64,
    pub breakout_vol_min: f64,
    pub breakout_struct_min: f64,
    pub reversal_momentum_exhausted_max: f64,
    pub reversal_structure_broken_max: f64,
    pub pullback_trend_min: f64,
    pub mean_reversion_vol_max: f64,
    pub scalp_bbwp_range: [f64; 2],
    pub scalp_struct_min: f64,
    pub squeeze_asymmetry_min: f64,
    /// Regime name sets (PascalCase enum names).
    pub mean_reversion_regimes: Vec<String>,
    pub scalp_regimes: Vec<String>,
    pub squeeze_regimes: Vec<String>,
}

impl Default for L4PreconditionParams {
    fn default() -> Self {
        Self {
            trend_continuation_trend_min: 75.0,
            breakout_vol_min: 70.0,
            breakout_struct_min: 60.0,
            reversal_momentum_exhausted_max: 25.0,
            reversal_structure_broken_max: 40.0,
            pullback_trend_min: 60.0,
            mean_reversion_vol_max: 30.0,
            scalp_bbwp_range: [70.0, 95.0],
            scalp_struct_min: 70.0,
            squeeze_asymmetry_min: 0.3,
            mean_reversion_regimes: vec!["Range".into(), "Contraction".into()],
            scalp_regimes: vec!["TrendingBull".into(), "TrendingBear".into()],
            squeeze_regimes: vec!["Expansion".into(), "Transition".into()],
        }
    }
}

/// Zone-geometry parameters (defaults = the pre-v9 constants).
#[derive(Debug, Clone, PartialEq)]
pub struct L4ZoneParams {
    /// Confluent-level clustering tolerance.
    pub tolerance_atr_mult: f64,
    pub tolerance_close_pct: f64,
    /// Zone width by setup quality: high ≥ threshold → high, else low.
    pub width_k_high: f64,
    pub width_k_threshold: f64,
    pub width_k_low: f64,
    /// Synthetic ATR-fallback level strength.
    pub fallback_strength: f64,
    /// Invalidation-candidate source weights (fib_0786 / vp_val).
    pub invalidation_weights: std::collections::HashMap<String, f64>,
    /// Range-fade bracket geometry.
    pub range_entry_half_atr: f64,
    pub range_target_k_atr: f64,
    pub range_target_spread_atr: f64,
    pub range_inv_k_atr: f64,
    /// Horizon-appropriate stop budgets (keyed "SCALP"/"SWING"/…).
    pub horizon_stop_budgets: std::collections::HashMap<String, f64>,
}

impl Default for L4ZoneParams {
    fn default() -> Self {
        let mut inv = std::collections::HashMap::new();
        inv.insert("fib_0786".into(), 0.5);
        inv.insert("vp_val".into(), 0.4);
        let mut horizon = std::collections::HashMap::new();
        horizon.insert("SCALP".into(), 1.5);
        horizon.insert("INTRADAY".into(), 2.0);
        horizon.insert("SWING".into(), 3.0);
        horizon.insert("POSITION".into(), 4.0);
        Self {
            tolerance_atr_mult: 0.2,
            tolerance_close_pct: 0.1,
            width_k_high: 2.0,
            width_k_threshold: 70.0,
            width_k_low: 1.5,
            fallback_strength: 35.0,
            invalidation_weights: inv,
            range_entry_half_atr: 0.2,
            range_target_k_atr: 1.5,
            range_target_spread_atr: 0.2,
            range_inv_k_atr: 1.5,
            horizon_stop_budgets: horizon,
        }
    }
}

/// Parse an opportunity-type name (PascalCase) into the enum.
pub fn opportunity_type_from_name(name: &str) -> Option<OpportunityType> {
    match name {
        "LiquiditySqueeze" => Some(OpportunityType::LiquiditySqueeze),
        "Scalp" => Some(OpportunityType::Scalp),
        "TrendContinuation" => Some(OpportunityType::TrendContinuation),
        "Breakout" => Some(OpportunityType::Breakout),
        "Reversal" => Some(OpportunityType::Reversal),
        "Pullback" => Some(OpportunityType::Pullback),
        "MeanReversion" => Some(OpportunityType::MeanReversion),
        "NoClearOpportunity" => Some(OpportunityType::NoClearOpportunity),
        _ => None,
    }
}

/// Compare a `MarketRegime` against a strategy regime name (PascalCase,
/// e.g. `"Range"`). SCREAMING_SNAKE forms are tolerated for robustness.
pub fn regime_matches(regime: analysis::MarketRegime, name: &str) -> bool {
    let pascal = match regime {
        analysis::MarketRegime::TrendingBull => "TrendingBull",
        analysis::MarketRegime::TrendingBear => "TrendingBear",
        analysis::MarketRegime::Range => "Range",
        analysis::MarketRegime::Accumulation => "Accumulation",
        analysis::MarketRegime::Distribution => "Distribution",
        analysis::MarketRegime::Expansion => "Expansion",
        analysis::MarketRegime::Contraction => "Contraction",
        analysis::MarketRegime::Transition => "Transition",
    };
    pascal.eq_ignore_ascii_case(name) || pascal.to_uppercase() == name
}

impl OpportunityParams {
    /// Legacy config path (pre-strategy `[workspace.opportunity_matrix]`).
    pub fn from_config(cfg: &config_models::OpportunityMatrixConfig) -> Self {
        let mut params = Self::default();
        params.confluent_atr_fallback_enabled = cfg.confluent_atr_fallback_enabled;
        params.confluent_atr_k_entry = cfg.confluent_atr_k_entry;
        params.confluent_atr_k_target = cfg.confluent_atr_k_target;
        params.net_cost = core_domain::risk_reward::NetCostModel {
            taker_fee_bps: cfg.net_taker_fee_bps,
            slippage_bps: cfg.net_slippage_bps,
            funding_bps: cfg.net_funding_bps,
        };
        params
    }

    /// The strategy path: build from the strategy's `l4` section.
    pub fn from_strategy(l4: &config_models::L4Params) -> Self {
        let mut params = Self::default();
        params.confluent_atr_fallback_enabled = l4.zones.atr_fallback.enabled;
        params.confluent_atr_k_entry = l4.zones.atr_fallback.k_entry;
        params.confluent_atr_k_target = l4.zones.atr_fallback.k_target;
        params.net_cost = core_domain::risk_reward::NetCostModel {
            taker_fee_bps: l4.costs.taker_fee_bps,
            slippage_bps: l4.costs.slippage_bps,
            funding_bps: l4.costs.funding_bps,
        };
        params.setup_priority = l4
            .setups
            .priority
            .iter()
            .filter_map(|n| opportunity_type_from_name(n))
            .collect();
        params.setup_enabled = l4
            .setups
            .enabled
            .iter()
            .filter_map(|n| opportunity_type_from_name(n))
            .collect();
        let pc = &l4.preconditions;
        params.precondition = L4PreconditionParams {
            trend_continuation_trend_min: pc.trend_continuation.trend_min,
            breakout_vol_min: pc.breakout.vol_min,
            breakout_struct_min: pc.breakout.struct_min,
            reversal_momentum_exhausted_max: pc.reversal.momentum_exhausted_max,
            reversal_structure_broken_max: pc.reversal.structure_broken_max,
            pullback_trend_min: pc.pullback.trend_min,
            mean_reversion_vol_max: pc.mean_reversion.vol_max,
            scalp_bbwp_range: pc.scalp.bbwp_range,
            scalp_struct_min: pc.scalp.struct_min,
            squeeze_asymmetry_min: pc.liquidity_squeeze.asymmetry_min,
            mean_reversion_regimes: pc.mean_reversion.regimes.clone(),
            scalp_regimes: pc.scalp.regimes.clone(),
            squeeze_regimes: pc.liquidity_squeeze.regimes.clone(),
        };
        params.score_blend = l4.scoring.blend;
        params.quality_bands = l4.scoring.quality_bands;
        let z = &l4.zones;
        params.zones = L4ZoneParams {
            tolerance_atr_mult: z.tolerance_atr_mult,
            tolerance_close_pct: z.tolerance_close_pct,
            width_k_high: z.width_k.high,
            width_k_threshold: z.width_k.threshold,
            width_k_low: z.width_k.low,
            fallback_strength: z.fallback_strength,
            invalidation_weights: z.invalidation_weights.clone(),
            range_entry_half_atr: z.range_frame.entry_half_atr,
            range_target_k_atr: z.range_frame.target_k_atr,
            range_target_spread_atr: z.range_frame.target_spread_atr,
            range_inv_k_atr: z.range_frame.inv_k_atr,
            horizon_stop_budgets: z
                .horizon_stop_budgets
                .iter()
                .map(|(k, v)| (k.to_uppercase(), *v))
                .collect(),
        };
        if !l4.confluence_weights.is_empty() {
            params.confluence_weights = l4.confluence_weights.clone();
        }
        params
    }

    /// Whether a setup type is active for this strategy (enabled ∩
    /// priority). `NoClearOpportunity` is always the sentinel, never
    /// user-gated.
    pub fn setup_active(&self, ot: &OpportunityType) -> bool {
        if *ot == OpportunityType::NoClearOpportunity {
            return true;
        }
        self.setup_enabled.contains(ot) && self.setup_priority.contains(ot)
    }
}

impl Default for OpportunityParams {
    fn default() -> Self {
        Self {
            confluent_atr_fallback_enabled: true,
            confluent_atr_k_entry: 1.5,
            confluent_atr_k_target: 2.5,
            net_cost: core_domain::risk_reward::NetCostModel {
                taker_fee_bps: 6.0,
                slippage_bps: 5.0,
                funding_bps: 0.0,
            },
            setup_priority: default_setup_priority(),
            setup_enabled: default_setup_priority(),
            precondition: L4PreconditionParams::default(),
            score_blend: [0.35, 0.30, 0.20, 0.15],
            quality_bands: [75.0, 60.0, 45.0, 25.0],
            zones: L4ZoneParams::default(),
            confluence_weights: default_confluence_weights(),
            signal_weights: std::collections::HashMap::new(),
            viability_min_net_rr: 0.5,
            ladder_roles: config_models::LadderRoles::default(),
        }
    }
}

/// The pre-v9 selection-tree order.
fn default_setup_priority() -> Vec<OpportunityType> {
    vec![
        OpportunityType::LiquiditySqueeze,
        OpportunityType::Scalp,
        OpportunityType::TrendContinuation,
        OpportunityType::Breakout,
        OpportunityType::Reversal,
        OpportunityType::Pullback,
        OpportunityType::MeanReversion,
    ]
}

fn default_confluence_weights() -> std::collections::HashMap<String, f64> {
    let mut m = std::collections::HashMap::new();
    m.insert("volume_profile".into(), 0.30);
    m.insert("fibonacci".into(), 0.25);
    m.insert("support_resistance".into(), 0.20);
    m.insert("pivot_points".into(), 0.15);
    m.insert("liquidation_cluster".into(), 0.10);
    m.insert("atr_fallback".into(), 0.05);
    m
}

fn setup_quality_band_params(score: f64, bands: [f64; 4]) -> SetupQuality {
    if score >= bands[0] {
        SetupQuality::Prime
    } else if score >= bands[1] {
        SetupQuality::Strong
    } else if score >= bands[2] {
        SetupQuality::Moderate
    } else if score >= bands[3] {
        SetupQuality::Marginal
    } else {
        SetupQuality::None
    }
}

fn default_time_horizon(ot: OpportunityType) -> &'static str {
    match ot {
        OpportunityType::Scalp => "SCALP",
        OpportunityType::Breakout
        | OpportunityType::MeanReversion
        | OpportunityType::LiquiditySqueeze
        | OpportunityType::NoClearOpportunity => "INTRADAY",
        OpportunityType::TrendContinuation | OpportunityType::Pullback => "SWING",
        OpportunityType::Reversal => "POSITION",
    }
}

/// v6.10.18 (I-5b): the horizon-appropriate stop budget in ATR units.
/// A 60s scalp with a 4.5×ATR stop (VP-VAL anchored) produces a sub-1
/// R:R bracket — a professional scalp stop belongs at ~1.5×ATR, a swing
/// stop at ~3×ATR. The zone derivation prefers the NEARER of the
/// structural stop and this horizon budget, so brackets carry an
/// R:R the operator can actually trade.
/// v9: the budgets come from the strategy's `l4.zones.horizon_stop_budgets`
/// (defaults = the pre-v9 constants).
fn stop_atr_multiple_for(horizon: &str, params: &OpportunityParams) -> f64 {
    params
        .zones
        .horizon_stop_budgets
        .get(horizon)
        .copied()
        .unwrap_or(match horizon {
            "SCALP" => 1.5,
            "INTRADAY" => 2.0,
            "SWING" => 3.0,
            _ => 4.0, // POSITION
        })
}

fn compute_candidate_score(
    opportunity_type: OpportunityType,
    analysis: &AnalysisMatrix,
    alignment: &AlignmentMatrix,
    signals: &HashMap<String, NormalizedIndicatorValue>,
    preconditions_met: u32,
    preconditions_total: u32,
    params: &OpportunityParams,
) -> (f64, String, f64, f64, f64) {
    // v6.10 (Phase 2 / B2): align L4's QualityLevel → f64 mapping with the
    // canonical L6 fallback table at
    // `docs/matrices/02-04-decision-matrix.md §2.3` (POOR=20, WEAK=40,
    // AVERAGE=55, GOOD=70, EXCELLENT=100). The previous L4 mapping
    // (10/30/55/80/95) drifted from L6 (20/40/55/70/100) and caused the
    // same QualityLevel value to contribute a different f64 score
    // depending on which layer read it. With this change, the same enum
    // yields the same contribution whether consumed by L4 or L6.
    let q_ctx = match analysis.market_quality {
        analysis::QualityLevel::Excellent => 100.0,
        analysis::QualityLevel::Good => 70.0,
        analysis::QualityLevel::Average => 55.0,
        analysis::QualityLevel::Weak => 40.0,
        analysis::QualityLevel::Poor => 20.0,
    };

    let s_sig = {
        let mut total_strength = 0.0;
        let mut count = 0;
        for v in signals.values() {
            for s in &v.signals {
                // v9: the strategy's `l1_5.signal_weights` trust axis
                // multiplies each kind's contribution (default 1.0).
                let kind_weight = params
                    .signal_weights
                    .get(&format!("{:?}", s.kind))
                    .copied()
                    .unwrap_or(1.0);
                total_strength += s.strength.min(1.0) * kind_weight;
                count += 1;
            }
        }
        if count > 0 {
            (total_strength / count as f64 * 100.0).min(100.0)
        } else {
            40.0
        }
    };

    let a_mtf = alignment.trend_agreement_pct;

    let f_fresh = {
        let min_age = signals
            .values()
            .flat_map(|v| v.signals.iter())
            .map(|s| s.age_bars)
            .min()
            .unwrap_or(10);
        100.0 * (1.0 - (min_age as f64 / 20.0).min(1.0))
    };

    // v9: the strategy's `l4.scoring.blend` weights (defaults = the
    // pre-v9 0.35/0.30/0.20/0.15 blend).
    let [w_q, w_s, w_a, w_f] = params.score_blend;
    let raw = (w_q * q_ctx + w_s * s_sig + w_a * a_mtf + w_f * f_fresh).clamp(0.0, 100.0);
    // v6.10.1 (bug-fix): `score` is the raw viability blend, NOT gated by
    // the precondition completion ratio. The previous expression
    // `raw * ratio` collapsed every active-setup-but-inactive-condition
    // candidate to score = 0, hiding the operator's view of how close
    // each setup was to firing (every inactive profile card showed
    // `preconditions 0/N met` AND `score 0`). The activation signal is
    // already published separately on every `OpportunityProfile` as
    // `preconditions_met` / `preconditions_total`, and is also surfaced
    // in `scoring_factors.precondition_ratio` (Rust-only, serde-skipped)
    // for telemetry. The dashboard renders this as a dedicated progress
    // bar (`ui/src/components/OpportunitiesPanel.svelte:430-437`).
    let ratio = if preconditions_total > 0 {
        preconditions_met as f64 / preconditions_total as f64
    } else {
        0.0
    };
    // NoClearOpportunity is the unconditional-zero sentinel: it is the
    // explicit "no setup detected" placeholder and can never surface as an
    // actionable trade. Every other opportunity emits the raw viability
    // score so the operator can compare setups head-to-head even when
    // their preconditions are currently unmet.
    let score = if matches!(opportunity_type, OpportunityType::NoClearOpportunity) {
        0.0
    } else {
        raw.clamp(0.0, 100.0)
    };

    // v6.14: the operator-facing score scales by the precondition ratio
    // (`0/3 → 0 muted, 2/3 → scaled, 3/3 → full`) — published as an
    // ADDITIVE `display_score` on the profile so the raw `score` above
    // stays intact for data-science logging. `round(score × min(1, ratio))`
    // mirrors the frontend's legacy `displayScore` rule exactly (Rust
    // `.round()` = JS `Math.round` half-up on non-negative values), so
    // screen, export, and wire agree without duplicated frontend math.
    let display_score = (score * ratio.min(1.0)).round();

    // User-facing rationale. Precondition count is displayed separately
    // via the structured `preconditions_met` / `preconditions_total`
    // fields on every profile card — keep the `notes` lean.
    let notes = format!("{:?}", opportunity_type);

    (score, notes, raw, ratio, display_score)
}

struct LevelCandidate {
    price: f64,
    source: LevelSource,
    weight: f64,
}

fn indicator_sub_value(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    key: &str,
    sub: &str,
) -> Option<f64> {
    indicators
        .get(key)
        .and_then(|v| v.values.as_ref())
        .and_then(|m| m.get(sub))
        .copied()
}

fn collect_candidate_levels(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    cluster: Option<&LiquidationClusterMatrix>,
    close: f64,
    bias_bullish: bool,
    for_target: bool,
    params: &OpportunityParams,
) -> Vec<LevelCandidate> {
    let mut candidates: Vec<LevelCandidate> = Vec::new();

    let fib = indicators.get("fibonacci").and_then(|v| v.values.as_ref());
    let vp = indicators
        .get("volume_profile")
        .and_then(|v| v.values.as_ref());
    let pp = indicators
        .get("pivot_points")
        .and_then(|v| v.values.as_ref());

    // v9: the strategy's `l4.confluence_weights` (defaults = the pre-v9
    // fixed source weights).
    let source_weight = |s: LevelSource| -> f64 {
        let key = match s {
            LevelSource::Fibonacci => "fibonacci",
            LevelSource::VolumeProfile => "volume_profile",
            LevelSource::PivotPoints => "pivot_points",
            LevelSource::SupportResistance => "support_resistance",
            LevelSource::LiquidityCluster => "liquidation_cluster",
            LevelSource::AtrFallback => "atr_fallback",
        };
        params
            .confluence_weights
            .get(key)
            .copied()
            .unwrap_or(match s {
                LevelSource::Fibonacci => 0.25,
                LevelSource::VolumeProfile => 0.30,
                LevelSource::PivotPoints => 0.15,
                LevelSource::SupportResistance => 0.20,
                LevelSource::LiquidityCluster => 0.10,
                LevelSource::AtrFallback => 0.05,
            })
    };

    if for_target {
        if bias_bullish {
            if let Some(m) = fib {
                for key in &["ext_1272", "ext_1618", "ext_2000", "ext_2618"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::Fibonacci,
                                weight: source_weight(LevelSource::Fibonacci),
                            });
                        }
                    }
                }
            }
            if let Some(m) = vp {
                for key in &["vah"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile),
                            });
                        }
                    }
                }
                for key in &["lvn_0", "lvn_1", "lvn_2"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile) * 0.8,
                            });
                        }
                    }
                }
            }
            if let Some(m) = pp {
                for key in &["r1", "r2", "r3"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::PivotPoints,
                                weight: source_weight(LevelSource::PivotPoints),
                            });
                        }
                    }
                }
            }
            if let Some(c) = cluster {
                for lc in &c.long_clusters {
                    let p = (lc.price_low + lc.price_high) / 2.0;
                    if p > 0.0 && p > close {
                        candidates.push(LevelCandidate {
                            price: p,
                            source: LevelSource::LiquidityCluster,
                            weight: source_weight(LevelSource::LiquidityCluster)
                                * (lc.notional_usd / c.total_short_oi_usd.max(1.0)).min(1.0),
                        });
                    }
                }
            }
        } else {
            if let Some(m) = fib {
                for key in &["ext_1272", "ext_1618", "ext_2000", "ext_2618"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::Fibonacci,
                                weight: source_weight(LevelSource::Fibonacci),
                            });
                        }
                    }
                }
            }
            if let Some(m) = vp {
                for key in &["val"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile),
                            });
                        }
                    }
                }
                for key in &["lvn_0", "lvn_1", "lvn_2"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile) * 0.8,
                            });
                        }
                    }
                }
            }
            if let Some(m) = pp {
                for key in &["s1", "s2", "s3"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::PivotPoints,
                                weight: source_weight(LevelSource::PivotPoints),
                            });
                        }
                    }
                }
            }
            if let Some(c) = cluster {
                for lc in &c.short_clusters {
                    let p = (lc.price_low + lc.price_high) / 2.0;
                    if p > 0.0 && p < close {
                        candidates.push(LevelCandidate {
                            price: p,
                            source: LevelSource::LiquidityCluster,
                            weight: source_weight(LevelSource::LiquidityCluster)
                                * (lc.notional_usd / c.total_long_oi_usd.max(1.0)).min(1.0),
                        });
                    }
                }
            }
        }
    } else {
        if bias_bullish {
            if let Some(m) = fib {
                for key in &["fib_0382", "fib_0500", "fib_0618", "fib_0660", "fib_0786"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::Fibonacci,
                                weight: source_weight(LevelSource::Fibonacci),
                            });
                        }
                    }
                }
            }
            if let Some(m) = vp {
                for key in &["poc", "val"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile),
                            });
                        }
                    }
                }
            }
            if let Some(m) = pp {
                for key in &["s1", "s2", "s3"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v < close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::PivotPoints,
                                weight: source_weight(LevelSource::PivotPoints),
                            });
                        }
                    }
                }
            }
            if let Some(c) = cluster {
                for lc in &c.short_clusters {
                    let p = (lc.price_low + lc.price_high) / 2.0;
                    if p > 0.0 && p < close {
                        candidates.push(LevelCandidate {
                            price: p,
                            source: LevelSource::LiquidityCluster,
                            weight: source_weight(LevelSource::LiquidityCluster)
                                * (lc.notional_usd / c.total_long_oi_usd.max(1.0)).min(1.0),
                        });
                    }
                }
            }
        } else {
            if let Some(m) = fib {
                for key in &["fib_0382", "fib_0500", "fib_0618", "fib_0660", "fib_0786"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::Fibonacci,
                                weight: source_weight(LevelSource::Fibonacci),
                            });
                        }
                    }
                }
            }
            if let Some(m) = vp {
                for key in &["poc", "vah"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::VolumeProfile,
                                weight: source_weight(LevelSource::VolumeProfile),
                            });
                        }
                    }
                }
            }
            if let Some(m) = pp {
                for key in &["r1", "r2", "r3"] {
                    if let Some(&v) = m.get(*key) {
                        if v > 0.0 && v > close {
                            candidates.push(LevelCandidate {
                                price: v,
                                source: LevelSource::PivotPoints,
                                weight: source_weight(LevelSource::PivotPoints),
                            });
                        }
                    }
                }
            }
            if let Some(c) = cluster {
                for lc in &c.long_clusters {
                    let p = (lc.price_low + lc.price_high) / 2.0;
                    if p > 0.0 && p > close {
                        candidates.push(LevelCandidate {
                            price: p,
                            source: LevelSource::LiquidityCluster,
                            weight: source_weight(LevelSource::LiquidityCluster)
                                * (lc.notional_usd / c.total_short_oi_usd.max(1.0)).min(1.0),
                        });
                    }
                }
            }
        }
    }

    candidates
}

fn cluster_levels(candidates: &[LevelCandidate], tolerance: f64) -> Vec<Vec<&LevelCandidate>> {
    if candidates.is_empty() {
        return vec![];
    }
    let mut sorted: Vec<usize> = (0..candidates.len()).collect();
    sorted.sort_by(|&a, &b| {
        candidates[a]
            .price
            .partial_cmp(&candidates[b].price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut clusters: Vec<Vec<&LevelCandidate>> = Vec::new();
    for &idx in &sorted {
        let cand = &candidates[idx];
        let mut found = false;
        for cluster in &mut clusters {
            let cluster_avg = cluster.iter().map(|c| c.price).sum::<f64>() / cluster.len() as f64;
            if (cand.price - cluster_avg).abs() <= tolerance {
                cluster.push(cand);
                found = true;
                break;
            }
        }
        if !found {
            clusters.push(vec![cand]);
        }
    }
    clusters
}

/// The role a confluent level plays in the bracket — the SIDE semantics
/// differ per role (v6.10.18 I-6): entries and invalidation levels sit on
/// the trade's OWN side (below close = LONG entry / LONG stop, above
/// close = SHORT entry / SHORT stop), while TARGETS are the profit zones
/// and sit on the OPPOSITE geometric side (above close = LONG target,
/// below close = SHORT target). One flat "above = SHORT" rule tagged a
/// long's profit zone "SHORT" — misleading for the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfluentRole {
    Entry,
    Target,
    Invalidation,
}

impl ConfluentRole {
    fn side_for(&self, price: f64, close: f64) -> Option<String> {
        let above = price > close;
        let below = price < close;
        if !above && !below {
            return None;
        }
        // Entry / Invalidation: LONG below, SHORT above.
        // Target: reversed — LONG above (profit zone), SHORT below.
        let is_long = match self {
            ConfluentRole::Target => above,
            ConfluentRole::Entry | ConfluentRole::Invalidation => below,
        };
        Some(if is_long {
            "LONG".to_string()
        } else {
            "SHORT".to_string()
        })
    }
}

fn clusters_to_confluent(
    clusters: Vec<Vec<&LevelCandidate>>,
    close: f64,
    role: ConfluentRole,
) -> Vec<ConfluentLevel> {
    let mut out: Vec<ConfluentLevel> = Vec::new();
    for cluster in &clusters {
        let avg_price = cluster.iter().map(|c| c.price).sum::<f64>() / cluster.len() as f64;
        let mut sources: Vec<LevelSource> = cluster.iter().map(|c| c.source).collect();
        sources.sort_by_key(|s| *s as u8);
        sources.dedup_by_key(|s| *s as u8);
        let confluence_count = sources.len() as u32;
        let total_weight: f64 = cluster.iter().map(|c| c.weight).sum();
        let strength = (total_weight * 100.0).min(100.0);
        // F23 (v6.10.17) + I-6 (v6.10.18): role-aware side semantics.
        let side = role.side_for(avg_price, close);
        out.push(ConfluentLevel {
            price: avg_price,
            confluence_count,
            sources,
            strength,
            side,
        });
    }
    out.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn derive_confluent_zones(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    cluster: Option<&LiquidationClusterMatrix>,
    close: f64,
    bias_bullish: bool,
    params: &OpportunityParams,
) -> (
    Vec<ConfluentLevel>,
    Vec<ConfluentLevel>,
    Vec<ConfluentLevel>,
) {
    let atr = indicators
        .get("atr")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("atr_14").copied())
        .unwrap_or(close * 0.01);
    let tolerance = (atr * params.zones.tolerance_atr_mult)
        .max(close * params.zones.tolerance_close_pct / 100.0);

    let entry_candidates =
        collect_candidate_levels(indicators, cluster, close, bias_bullish, false, params);
    let target_candidates =
        collect_candidate_levels(indicators, cluster, close, bias_bullish, true, params);

    let entry_clusters = cluster_levels(&entry_candidates, tolerance);
    let target_clusters = cluster_levels(&target_candidates, tolerance);

    let mut entry_levels = clusters_to_confluent(entry_clusters, close, ConfluentRole::Entry);
    let mut target_levels = clusters_to_confluent(target_clusters, close, ConfluentRole::Target);

    // ── ATR-based fallback for entry/target ──
    // When every structural source (Fibonacci / Volume Profile / Pivot
    // Points / Liquidation Clusters) fails to produce a candidate for
    // entry or target, the surface goes empty. The Opportunities panel
    // then shows "No confluent levels" — which is technically correct
    // but unhelpful in practice: a healthy market with a clear bias
    // should always surface at least one actionable bracket. We
    // therefore fall back to a single ATR-derived level:
    //
    //   bullish: entry = close − k_entry·ATR, target = close + k_target·ATR
    //   bearish: entry = close + k_entry·ATR, target = close − k_target·ATR
    //
    // v9 (F-04): the knobs come from the wired `OpportunityParams`
    // (sourced from `[workspace.opportunity_matrix]`) — previously
    // hardcoded inline.
    if params.confluent_atr_fallback_enabled {
        if entry_levels.is_empty() && atr > 0.0 {
            let entry_price = if bias_bullish {
                close - params.confluent_atr_k_entry * atr
            } else {
                close + params.confluent_atr_k_entry * atr
            };
            entry_levels.push(ConfluentLevel {
                price: entry_price,
                confluence_count: 1,
                sources: vec![LevelSource::AtrFallback],
                strength: params.zones.fallback_strength, // synthetic strength below typical real levels
                side: Some(if bias_bullish {
                    "LONG".to_string()
                } else {
                    "SHORT".to_string()
                }),
            });
        }
        if target_levels.is_empty() && atr > 0.0 {
            let target_price = if bias_bullish {
                close + params.confluent_atr_k_target * atr
            } else {
                close - params.confluent_atr_k_target * atr
            };
            target_levels.push(ConfluentLevel {
                price: target_price,
                confluence_count: 1,
                sources: vec![LevelSource::AtrFallback],
                strength: params.zones.fallback_strength,
                side: Some(if bias_bullish {
                    "LONG".to_string()
                } else {
                    "SHORT".to_string()
                }),
            });
        }
    }

    let invalidation_candidates: Vec<LevelCandidate> = if bias_bullish {
        let mut inval = Vec::new();
        if let Some(v) = indicator_sub_value(indicators, "fibonacci", "fib_0786") {
            if v > 0.0 && v < close {
                inval.push(LevelCandidate {
                    price: v,
                    source: LevelSource::Fibonacci,
                    weight: params
                        .zones
                        .invalidation_weights
                        .get("fib_0786")
                        .copied()
                        .unwrap_or(0.5),
                });
            }
        }
        if let Some(v) = indicator_sub_value(indicators, "volume_profile", "val") {
            if v > 0.0 && v < close {
                inval.push(LevelCandidate {
                    price: v,
                    source: LevelSource::VolumeProfile,
                    weight: params
                        .zones
                        .invalidation_weights
                        .get("vp_val")
                        .copied()
                        .unwrap_or(0.4),
                });
            }
        }
        inval
    } else {
        let mut inval = Vec::new();
        if let Some(v) = indicator_sub_value(indicators, "fibonacci", "fib_0786") {
            if v > 0.0 && v > close {
                inval.push(LevelCandidate {
                    price: v,
                    source: LevelSource::Fibonacci,
                    weight: params
                        .zones
                        .invalidation_weights
                        .get("fib_0786")
                        .copied()
                        .unwrap_or(0.5),
                });
            }
        }
        if let Some(v) = indicator_sub_value(indicators, "volume_profile", "vah") {
            if v > 0.0 && v > close {
                inval.push(LevelCandidate {
                    price: v,
                    source: LevelSource::VolumeProfile,
                    weight: params
                        .zones
                        .invalidation_weights
                        .get("vp_val")
                        .copied()
                        .unwrap_or(0.4),
                });
            }
        }
        inval
    };

    let inval_clusters = cluster_levels(&invalidation_candidates, tolerance);
    let invalidation_levels =
        clusters_to_confluent(inval_clusters, close, ConfluentRole::Invalidation);

    (entry_levels, target_levels, invalidation_levels)
}

/// Build entry/target/invalidation zones for one directional side.
///
/// `bias_long = true` produces a long-oriented bracket (entry below close,
/// target above, invalidation below). `bias_long = false` mirrors that
/// (entry above close, target below, invalidation above). Returns the three
/// zone values together with the per-side confluent level vectors so the
/// matrix-level fields can be sourced from the active side without an extra
/// `derive_confluent_zones` call.
fn derive_side_zones(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    cluster: Option<&LiquidationClusterMatrix>,
    close: f64,
    atr: f64,
    primary_score: f64,
    bias_long: bool,
    stop_atr_multiple: f64,
    params: &OpportunityParams,
) -> (
    core_domain::opportunity::PriceRange,
    core_domain::opportunity::PriceRange,
    f64,
    Vec<ConfluentLevel>,
    Vec<ConfluentLevel>,
    Vec<ConfluentLevel>,
) {
    let (confluent_entry, confluent_target, confluent_inval) =
        derive_confluent_zones(indicators, cluster, close, bias_long, params);

    let has_confluent_entry = confluent_entry.len() >= 2;
    let has_confluent_target = confluent_target.len() >= 2;
    let has_confluent_inval = !confluent_inval.is_empty();

    // ── Entry zone — side-specific clamp ───────────────────────────────
    // LONG:  zone must sit BELOW close (`high ≤ close`).
    // SHORT: zone must sit ABOVE close (`low ≥ close`).
    // The legacy implementation clamped both bounds to `close` in the
    // same direction (`low = low.min(close); high = high.max(close)`)
    // which produced zones straddling close instead of sitting cleanly
    // on one side. Fix: clamp the bound that touches `close`, then
    // widen the other bound away from `close` by ATR.
    let entry_zone = if has_confluent_entry {
        let prices: Vec<f64> = confluent_entry.iter().map(|c| c.price).collect();
        let raw_low = prices.iter().cloned().fold(f64::INFINITY, f64::min);
        let raw_high = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let (low, high) = if bias_long {
            // LONG: high must NOT exceed close; widen low further below.
            let high = raw_high.min(close);
            let low = raw_low.min(high).min(close - atr * 0.1).max(0.0);
            (low, high)
        } else {
            // SHORT: low must NOT go below close; widen high further above.
            let low = raw_low.max(close);
            let high = raw_high.max(low).max(close + atr * 0.1);
            (low, high)
        };
        core_domain::opportunity::PriceRange { low, high }
    } else {
        // ATR fallback — symmetric, side-correct.
        if bias_long {
            core_domain::opportunity::PriceRange {
                low: (close - atr * 0.5).max(0.0),
                high: close,
            }
        } else {
            core_domain::opportunity::PriceRange {
                low: close,
                high: close + atr * 0.5,
            }
        }
    };

    // ── Target zone — side-correct, with min distance from close ────────
    // LONG:  zone must sit ABOVE close (`low ≥ close + δ`).
    // SHORT: zone must sit BELOW close (`high ≤ close − δ`).
    let target_zone = if has_confluent_target {
        let prices: Vec<f64> = confluent_target.iter().map(|c| c.price).collect();
        let raw_low = prices.iter().cloned().fold(f64::INFINITY, f64::min);
        let raw_high = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let (low, high) = if bias_long {
            // LONG: low must be above close; widen high further above.
            let low = raw_low.max(close + atr * 0.1);
            let high = raw_high.max(low);
            // Defensive upper floor: never publish `high ≤ 0` even if every
            // candidate somehow carried a non-positive price (the
            // `collect_candidate_levels` `v > 0.0` filter normally blocks
            // this, but the floor protects against any future source that
            // bypasses that filter).
            let high = high.max(close + atr * 1.5);
            (low, high)
        } else {
            // SHORT: high must be below close; widen low further below.
            let high = raw_high.min(close - atr * 0.1);
            let low = raw_low.min(high);
            // Defensive lower floor: a non-positive candidate (e.g. a
            // `PIVOT_UNAVAILABLE` pivot_points series with s1=s2=s3=0.0)
            // would otherwise drag `raw_low` to 0 and the published
            // `short_target_zone.low` becomes 0, which the frontend
            // surfaces verbatim as `$0–$X` (Bug A, observed BTC-USDT
            // 2026-08-11). Pin low to `close − atr · 1.5` (mirrors the
            // ATR-fallback lower bound at line ~712 below).
            let low = low.max((close - atr * 1.5).max(0.0)).min(high);
            (low, high)
        };
        core_domain::opportunity::PriceRange { low, high }
    } else if bias_long {
        let k = if primary_score >= params.zones.width_k_threshold {
            params.zones.width_k_high
        } else {
            params.zones.width_k_low
        };
        core_domain::opportunity::PriceRange {
            low: close + atr * k,
            high: close + atr * (k + 1.0),
        }
    } else {
        let k = if primary_score >= params.zones.width_k_threshold {
            params.zones.width_k_high
        } else {
            params.zones.width_k_low
        };
        core_domain::opportunity::PriceRange {
            low: close - atr * (k + 1.0),
            high: close - atr * k,
        }
    };

    // ── Invalidation — MUST sit OUTSIDE the entry zone ────────────────
    // LONG:  inv < entry.low  (a stop above entry.high would be a no-op).
    // SHORT: inv > entry.high.
    // The legacy implementation picked `confluent_inval[0].price`
    // regardless of side, which surfaced the screenshot bug where
    // SL = $63937 sat at entry.low (= $63937).
    // AUDIT-H8b: on a perfectly flat series (`atr == 0`) the ATR-fallback
    // stop distance collapses to zero — `entry.low - 0 = entry.low` — and
    // the geometry assertions panicked on legitimate market data. Floor
    // the stop distance and the side margins at a tiny relative epsilon
    // of the reference price so the stop always clears the entry zone.
    let atr_floor = close.abs() * 1e-6;
    let stop_distance = (atr * stop_atr_multiple).max(atr_floor);
    let margin = (atr * 0.05).max(atr_floor);
    let invalidation_level = if has_confluent_inval {
        // Side-prune the candidates: keep only those on the correct
        // side of the entry zone. If none survive, fall through to the
        // ATR fallback below.
        let survivors: Vec<&ConfluentLevel> = confluent_inval
            .iter()
            .filter(|c| {
                if bias_long {
                    c.price < entry_zone.low
                } else {
                    c.price > entry_zone.high
                }
            })
            .collect();
        // v6.10.18 (I-5b): horizon-aware stop preference. The structural
        // stop (e.g. VP VAL / VAH) can sit FAR from the entry — a 60s
        // scalp with a 4.5×ATR stop is a sub-1 R:R bracket nobody can
        // trade. Prefer the NEARER of the structural survivor and the
        // horizon stop (`ATR × multiple` from the entry mid), always
        // staying outside the entry zone. The structural stop still wins
        // when it is already tighter than the horizon budget.
        let entry_mid = (entry_zone.low + entry_zone.high) / 2.0;
        let horizon_stop = if bias_long {
            (entry_mid - stop_distance).max(0.0)
        } else {
            entry_mid + stop_distance
        };
        let structural = survivors.first().map(|c| c.price.max(0.0));
        let chosen = match (structural, bias_long) {
            // LONG: the HIGHER stop is the closer one.
            (Some(s), true) => s.max(horizon_stop),
            // SHORT: the LOWER stop is the closer one.
            (Some(s), false) => s.min(horizon_stop),
            (None, true) => horizon_stop,
            (None, false) => horizon_stop,
        };
        if bias_long {
            chosen.min(entry_zone.low - margin).max(0.0)
        } else {
            chosen.max(entry_zone.high + margin)
        }
    } else if bias_long {
        (entry_zone.low - stop_distance).max(0.0)
    } else {
        entry_zone.high + stop_distance
    };

    // ── Geometry invariant assertions ────────────────────────────────
    // These debug-only checks prevent silent geometry violations from
    // reaching the frontend. In release builds the values are still
    // clamped by the logic above; these are the safety net.
    #[cfg(debug_assertions)]
    {
        if bias_long {
            // LONG: entry below close, target above close, inval below entry.
            debug_assert!(
                entry_zone.high <= close + atr * 0.01,
                "derive_side_zones (LONG): entry_zone.high {:.2} > close {:.2} + epsilon — entry straddles or sits above close",
                entry_zone.high, close
            );
            debug_assert!(
                target_zone.low >= entry_zone.high,
                "derive_side_zones (LONG): target_zone.low {:.2} < entry_zone.high {:.2} — target below entry",
                target_zone.low, entry_zone.high
            );
            debug_assert!(
                invalidation_level < entry_zone.low,
                "derive_side_zones (LONG): invalidation_level {:.2} >= entry_zone.low {:.2} — SL at or above entry",
                invalidation_level, entry_zone.low
            );
        } else {
            // SHORT: entry above close, target below close, inval above entry.
            debug_assert!(
                entry_zone.low >= close,
                "derive_side_zones (SHORT): entry_zone.low {:.2} < close {:.2} — entry sits below close",
                entry_zone.low, close
            );
            debug_assert!(
                target_zone.high <= entry_zone.low,
                "derive_side_zones (SHORT): target_zone.high {:.2} > entry_zone.low {:.2} — target above entry",
                target_zone.high, entry_zone.low
            );
            debug_assert!(
                invalidation_level > entry_zone.high,
                "derive_side_zones (SHORT): invalidation_level {:.2} <= entry_zone.high {:.2} — SL at or below entry",
                invalidation_level, entry_zone.high
            );
        }
    }

    (
        entry_zone,
        target_zone,
        invalidation_level,
        confluent_entry,
        confluent_target,
        confluent_inval,
    )
}

/// v6.10.21 (NBR): direction-agnostic range reference frame for the
/// No-Clear + range regime. The entry band centers on the close
/// (±0.2×ATR), the target rides the upper range-bound proxy
/// (close + 1.5..1.7×ATR), the invalidation sits below the lower
/// range-bound proxy (close − 1.5×ATR) — a symmetric range-fade frame
/// whose gross geometric R:R ≈ 1.07, net ≈ 1.05 after friction.
///
/// Purely informational: the operator sees valid range-fade geometry
/// instead of an empty Range folder, but the frame is never a trade —
/// `TradeViability`/preconditions/`profiles` are untouched, and the
/// caller only emits it when the primary opportunity is
/// `NoClearOpportunity` AND the regime reads as a range. Returns `None`
/// when no valid frame can be derived (non-positive bounds, missing ATR).
fn derive_neutral_bracket(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    close: f64,
    params: &OpportunityParams,
) -> Option<NeutralBracket> {
    if !close.is_finite() || close <= 0.0 {
        return None;
    }
    let atr = indicators
        .get("atr")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("atr_14").copied())
        .unwrap_or(close * 0.01);
    if atr <= 0.0 {
        return None;
    }
    // v9: the strategy's `l4.zones.range_frame` geometry (defaults = the
    // pre-v9 constants).
    let rf = &params.zones;
    let entry_zone = PriceRange {
        low: close - rf.range_entry_half_atr * atr,
        high: close + rf.range_entry_half_atr * atr,
    };
    let target_zone = PriceRange {
        low: close + rf.range_target_k_atr * atr,
        high: close + (rf.range_target_k_atr + rf.range_target_spread_atr) * atr,
    };
    let invalidation_level = close - rf.range_inv_k_atr * atr;
    if entry_zone.low <= 0.0 || target_zone.low <= 0.0 || invalidation_level <= 0.0 {
        return None;
    }
    // The same three-state geometry gate the directional sides use —
    // only a geometrically valid frame is ever published.
    use core_domain::risk_reward::{compute_side_rr_v2, Side, SideRrStatus};
    let status = compute_side_rr_v2(
        entry_zone.low,
        entry_zone.high,
        target_zone.low,
        target_zone.high,
        invalidation_level,
        close,
        Side::Long,
    );
    let gross = match &status {
        SideRrStatus::Value(v) => Some(*v),
        _ => None,
    };
    let (expected_rr_internal, geometry_consistent) = if let Some(_gross) = gross {
        // v9 (F-04): the net-cost model is wired — previously hardcoded
        // `NetCostModel::default()` (6/5/0 bps).
        let cost = params.net_cost.clone();
        let net = cost.net_rr(
            (entry_zone.low + entry_zone.high) / 2.0,
            (target_zone.low + target_zone.high) / 2.0,
            invalidation_level,
            Side::Long,
        );
        (net, true)
    } else {
        (0.0, false)
    };
    if !geometry_consistent {
        return None;
    }
    Some(NeutralBracket {
        entry_zone,
        target_zone,
        invalidation_level,
        expected_rr_internal,
        geometry_consistent,
        rationale: "range reference — no directional setup".to_string(),
    })
}

/// Z-Score magnitude at which a MeanReversion opportunity is considered
/// "extended" enough to resolve its trade side from the deviation
/// (`|z| ≥ threshold`). Below that, the data is ambiguous and the
/// caller falls back to the family × bias mapping.
const ZSCORE_MEAN_REVERSION_SIDE_THRESHOLD: f64 = 0.5;

/// Resolve the directional side of a CounterTrend opportunity
/// (MeanReversion / Reversal) from market data instead of the bare
/// family × bias mapping (4b):
///
/// - **MeanReversion** follows the Z-Score sign — price stretched ABOVE
///   its rolling mean (`z ≥ +threshold`) → `Some(false)` (SHORT,
///   "sell the rip"); stretched BELOW (`z ≤ −threshold`) → `Some(true)`
///   (LONG, "buy the dip").
/// - **Reversal** follows the confirmed divergence direction —
///   `CONFIRMED_BULLISH_DIVERGENCE` → `Some(true)` (LONG),
///   `CONFIRMED_BEARISH_DIVERGENCE` → `Some(false)` (SHORT).
///
/// Returns `None` when the data is ambiguous or absent; the caller
/// falls back to the family × bias mapping (LONG under bearish bias,
/// SHORT under bullish bias).
fn resolve_countertrend_side(
    primary: core_domain::analysis::OpportunityType,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
) -> Option<bool> {
    match primary {
        core_domain::analysis::OpportunityType::MeanReversion => {
            let z = indicators.get("zscore").map(|v| v.raw_value).unwrap_or(0.0);
            if z >= ZSCORE_MEAN_REVERSION_SIDE_THRESHOLD {
                Some(false)
            } else if z <= -ZSCORE_MEAN_REVERSION_SIDE_THRESHOLD {
                Some(true)
            } else {
                None
            }
        }
        core_domain::analysis::OpportunityType::Reversal => {
            let bullish_div = indicators.values().any(|v| {
                v.signals
                    .iter()
                    .any(|s| s.label.contains("BULLISH_DIVERGENCE"))
            });
            let bearish_div = indicators.values().any(|v| {
                v.signals
                    .iter()
                    .any(|s| s.label.contains("BEARISH_DIVERGENCE"))
            });
            match (bullish_div, bearish_div) {
                (true, false) => Some(true),
                (false, true) => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

fn compute_opportunity(
    analysis: &AnalysisMatrix,
    alignment: &AlignmentMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    liquidity_flow: Option<&LiquidityFlow>,
    cluster: Option<&LiquidationClusterMatrix>,
    close: f64,
    params: &OpportunityParams,
) -> Option<OpportunityMatrix> {
    if analysis.timeframes_considered == 0 {
        return None;
    }

    // Bug-fix #20: read the canonical signed `mtf_*_alignment` fields
    // and convert to 0-100 here, instead of reading the 0-100 mapped
    // `dimensions[i].score` (which is `from_signed` output and is
    // indistinguishable from the other 0-100 dimensions in the
    // `AlignmentMatrix.dimensions` vector). The L4 opportunity
    // preconditions and the per-candidate score blend now operate on
    // the same scale the L2 emitted, eliminating the historical
    // "trend_dim is in 0-100 but mtf_trend_alignment is signed"
    // asymmetry that caused `OpportunityType::TrendContinuation` to
    // never fire on a perfectly balanced trend (signed = 0, mapped
    // = 50, but legacy 50 is "Neutral" not "Weak Bull").
    let trend_dim = ((alignment.mtf_trend_alignment + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0);
    let momentum_dim = ((alignment.mtf_momentum_alignment + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0);
    let vol_dim = ((alignment.mtf_volatility_alignment + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0);
    let struct_dim = alignment.dimensions.get(4).map(|d| d.score).unwrap_or(50.0);

    let bbwp = indicators.get("bbwp").map(|v| v.raw_value).unwrap_or(50.0);

    // Divergence detection: the L1.5 signal flow has three label families.
    //   1. RSI/MACD/Stochastic/ChandeMO/MFI/CMF/OBV/Squeeze: the per-indicator
    //      `SeriesDivergence::state` produces `CONFIRMED_BULLISH_DIVERGENCE` /
    //      `CONFIRMED_BEARISH_DIVERGENCE`. The legacy "CONFIRMED + DIVERGENCE"
    //      substring check still matches these.
    //   2. OI-Price divergence: the derivatives-WS path emits
    //      `OI_PRICE_DIVERGENCE` (no "CONFIRMED" prefix). The legacy substring
    //      check would miss this entirely, breaking the L1.5→L4→L6
    //      Reversal flow. We now match any label containing the substring
    //      `DIVERGENCE`, which subsumes both label families.
    let has_confirmed_divergence = indicators
        .values()
        .any(|v| v.signals.iter().any(|s| s.label.contains("DIVERGENCE")));

    // v9: the strategy's `l4.preconditions` thresholds (defaults = the
    // pre-v9 constants).
    let pc = &params.precondition;
    let momentum_exhausted = momentum_dim < pc.reversal_momentum_exhausted_max;
    let structure_broken = struct_dim < pc.reversal_structure_broken_max;
    let momentum_weakening = matches!(
        analysis.momentum_assessment,
        analysis::MomentumAssessment::Weakening
    );

    let bias_bullish = matches!(
        analysis.bias,
        analysis::MarketBias::Bullish | analysis::MarketBias::StrongBullish
    );
    let bias_bearish = matches!(
        analysis.bias,
        analysis::MarketBias::Bearish | analysis::MarketBias::StrongBearish
    );
    let bias_directional = bias_bullish || bias_bearish;

    let cascade_active = liquidity_flow
        .map(|lf| {
            matches!(
                lf.cascade_state,
                core_domain::liquidity::CascadeState::Detected
                    | core_domain::liquidity::CascadeState::Sustained
            )
        })
        .unwrap_or(false);
    let cascade_asymmetry = cluster.map(|c| c.cascade_asymmetry).unwrap_or(0.0);
    let regime_is_expansion_or_transition = pc
        .squeeze_regimes
        .iter()
        .any(|r| regime_matches(analysis.market_regime, r));

    let is_trending = pc
        .scalp_regimes
        .iter()
        .any(|r| regime_matches(analysis.market_regime, r));

    let is_range = pc
        .mean_reversion_regimes
        .iter()
        .any(|r| regime_matches(analysis.market_regime, r));

    let momentum_not_exhausted = !matches!(
        analysis.momentum_assessment,
        analysis::MomentumAssessment::Exhausted | analysis::MomentumAssessment::Reversing
    );

    let mut profiles: Vec<OpportunityProfile> = Vec::new();

    // v9: the selection tree is the strategy's first-match priority order
    // over its ENABLED setups — previously a hardcoded chain. The sentinel
    // `NoClearOpportunity` is never user-gated.
    let primary_opportunity = {
        let mut primary = OpportunityType::NoClearOpportunity;
        for candidate in &params.setup_priority {
            if !params.setup_active(candidate) {
                continue;
            }
            let matched = match candidate {
                OpportunityType::LiquiditySqueeze => {
                    cascade_active
                        && cascade_asymmetry.abs() > pc.squeeze_asymmetry_min
                        && regime_is_expansion_or_transition
                }
                OpportunityType::Scalp => {
                    (pc.scalp_bbwp_range[0]..pc.scalp_bbwp_range[1]).contains(&bbwp)
                        && struct_dim >= pc.scalp_struct_min
                        && bias_directional
                        && is_trending
                }
                OpportunityType::TrendContinuation => {
                    trend_dim >= pc.trend_continuation_trend_min
                        && bias_directional
                        && momentum_not_exhausted
                }
                OpportunityType::Breakout => {
                    vol_dim >= pc.breakout_vol_min && struct_dim >= pc.breakout_struct_min
                }
                OpportunityType::Reversal => {
                    has_confirmed_divergence && structure_broken && momentum_exhausted
                }
                OpportunityType::Pullback => {
                    trend_dim >= pc.pullback_trend_min && momentum_weakening
                }
                OpportunityType::MeanReversion => {
                    // B2: the primary must meet its own profile
                    // preconditions (vol ≤ max AND the range gate).
                    vol_dim <= pc.mean_reversion_vol_max && is_range
                }
                OpportunityType::NoClearOpportunity => false,
            };
            if matched {
                primary = *candidate;
                break;
            }
        }
        primary
    };

    // 4b: CounterTrend direction — deviation-driven side resolution.
    // `Some(true)` → the profile populates LONG zones, `Some(false)` →
    // SHORT zones, `None` → fall back to family × bias. See
    // `resolve_countertrend_side`.
    let countertrend_resolved_long: Option<bool> =
        resolve_countertrend_side(primary_opportunity, indicators);

    let candidates: [OpportunityType; 8] = [
        OpportunityType::LiquiditySqueeze,
        OpportunityType::Scalp,
        OpportunityType::TrendContinuation,
        OpportunityType::Breakout,
        OpportunityType::Reversal,
        OpportunityType::Pullback,
        OpportunityType::MeanReversion,
        OpportunityType::NoClearOpportunity,
    ];

    // First pass: score every candidate so we can resolve `primary_score`
    // BEFORE deriving zones. The zone helper widens its ATR fallback
    // bracket when `primary_score >= 70.0`, so the value must be in hand
    // before `derive_side_zones` is called. We collect everything we need
    // for the second pass into a Vec.
    type ScoredCandidate = (OpportunityType, f64, String, f64, f64, f64, u32, u32);
    let mut scored: Vec<ScoredCandidate> = Vec::with_capacity(candidates.len());
    for ot in &candidates {
        let (met, total) = match ot {
            OpportunityType::LiquiditySqueeze => (
                if cascade_active
                    && cascade_asymmetry.abs() > pc.squeeze_asymmetry_min
                    && regime_is_expansion_or_transition
                {
                    3
                } else {
                    0
                },
                3,
            ),
            OpportunityType::Scalp => (
                if (pc.scalp_bbwp_range[0]..pc.scalp_bbwp_range[1]).contains(&bbwp)
                    && struct_dim >= pc.scalp_struct_min
                    && bias_directional
                    && is_trending
                {
                    3
                } else {
                    0
                },
                3,
            ),
            OpportunityType::TrendContinuation => (
                if trend_dim >= pc.trend_continuation_trend_min
                    && bias_directional
                    && momentum_not_exhausted
                {
                    3
                } else {
                    0
                },
                3,
            ),
            OpportunityType::Breakout => (
                if vol_dim >= pc.breakout_vol_min && struct_dim >= pc.breakout_struct_min {
                    2
                } else {
                    0
                },
                2,
            ),
            OpportunityType::Reversal => (
                if has_confirmed_divergence && structure_broken && momentum_exhausted {
                    3
                } else {
                    0
                },
                3,
            ),
            OpportunityType::Pullback => (
                if trend_dim >= pc.pullback_trend_min && momentum_weakening {
                    2
                } else {
                    0
                },
                2,
            ),
            OpportunityType::MeanReversion => (
                if vol_dim <= pc.mean_reversion_vol_max && is_range {
                    2
                } else {
                    0
                },
                2,
            ),
            // v6.10.19a (N1): NoClearOpportunity is the unconditional
            // "no setup detected" sentinel — it must never read "met".
            // The previous `tradability_dim < 30` gate let a weak-market
            // fallback profile show "1/1 preconditions met" on the strip,
            // which reads as an activated setup next to "informational
            // only". The sentinel always reports 0/1.
            OpportunityType::NoClearOpportunity => (0, 1),
        };

        let (score, notes, raw_score, precondition_ratio, display_score) = compute_candidate_score(
            *ot,
            analysis,
            alignment,
            indicators,
            met as u32,
            total as u32,
            params,
        );
        scored.push((
            *ot,
            score,
            notes,
            raw_score,
            precondition_ratio,
            display_score,
            met as u32,
            total as u32,
        ));
    }

    let primary_score = scored
        .iter()
        .find(|(ot, _, _, _, _, _, _, _)| *ot == primary_opportunity)
        .map(|(_, s, _, _, _, _, _, _)| *s)
        .unwrap_or(0.0);

    let atr = indicators
        .get("atr")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("atr_14").copied())
        .unwrap_or(close * 0.01);

    let (
        long_entry_zone,
        long_target_zone,
        long_invalidation_level,
        long_conf_entry,
        long_conf_target,
        long_conf_inval,
    ) = derive_side_zones(
        indicators,
        cluster,
        close,
        atr,
        primary_score,
        true,
        stop_atr_multiple_for(default_time_horizon(primary_opportunity), params),
        params,
    );
    let (
        short_entry_zone,
        short_target_zone,
        short_invalidation_level,
        short_conf_entry,
        short_conf_target,
        short_conf_inval,
    ) = derive_side_zones(
        indicators,
        cluster,
        close,
        atr,
        primary_score,
        false,
        stop_atr_multiple_for(default_time_horizon(primary_opportunity), params),
        params,
    );

    // Per-side reward/risk computed with the three-state model
    // (`core_domain::risk_reward::compute_side_rr_v2`) which distinguishes:
    //   Value(f64)  — bracket is geometrically valid
    //   NoValue(r)  — bracket exists but geometry is inverted
    //   Error(msg)  — computation failed (NaN, division by zero)
    // The legacy closure conflated NoValue and Error as `None`.
    use core_domain::risk_reward::{compute_side_rr_v2, SideRrStatus};
    let long_rr_status = compute_side_rr_v2(
        long_entry_zone.low,
        long_entry_zone.high,
        long_target_zone.low,
        long_target_zone.high,
        long_invalidation_level,
        close,
        core_domain::risk_reward::Side::Long,
    );
    let short_rr_status = compute_side_rr_v2(
        short_entry_zone.low,
        short_entry_zone.high,
        short_target_zone.low,
        short_target_zone.high,
        short_invalidation_level,
        close,
        core_domain::risk_reward::Side::Short,
    );

    // Extract the f64 from the three-state result (for backward compat
    // with the per-profile `f64` fields). The trade_viability badge
    // reads the three-state status directly; the per-profile R:R
    // fields carry the numeric value.
    fn rr_value(status: &SideRrStatus) -> Option<f64> {
        match status {
            SideRrStatus::Value(v) => Some(*v),
            _ => None,
        }
    }
    fn rr_is_ok(status: &SideRrStatus) -> bool {
        matches!(status, SideRrStatus::Value(_))
    }
    // v6.10.19 (P5): ACTIONABLE requires the NET R:R ≥ 1.0 — the gross
    // geometric ratio minus estimated entry/exit fees + slippage
    // (`NetCostModel`, defaults 6/5/0 bps, config-tunable in a
    // follow-up). A gross 1:1 bracket nets ~0.98 at 22 bps of friction
    // and must demote to Qualifying. The GROSS ratio stays on the wire
    // (`long_gross_rr_internal` / `short_gross_rr_internal`).
    let viability_for =
        |status: &SideRrStatus, net_rr: f64| -> core_domain::opportunity::TradeViability {
            if rr_is_ok(status) && net_rr >= params.viability_min_net_rr {
                core_domain::opportunity::TradeViability::Actionable
            } else if rr_is_ok(status) {
                core_domain::opportunity::TradeViability::Qualifying
            } else {
                core_domain::opportunity::TradeViability::GeometryInverted
            }
        };
    let long_gross_rr_internal = rr_value(&long_rr_status);
    let short_gross_rr_internal = rr_value(&short_rr_status);
    // v9 (F-04): the net-cost model is wired from `OpportunityParams` —
    // previously hardcoded `NetCostModel::default()` (6/5/0 bps).
    let cost_model = params.net_cost.clone();
    let long_net_rr = if rr_is_ok(&long_rr_status) {
        cost_model.net_rr(
            (long_entry_zone.low + long_entry_zone.high) / 2.0,
            (long_target_zone.low + long_target_zone.high) / 2.0,
            long_invalidation_level,
            core_domain::risk_reward::Side::Long,
        )
    } else {
        0.0
    };
    let short_net_rr = if rr_is_ok(&short_rr_status) {
        cost_model.net_rr(
            (short_entry_zone.low + short_entry_zone.high) / 2.0,
            (short_target_zone.low + short_target_zone.high) / 2.0,
            short_invalidation_level,
            core_domain::risk_reward::Side::Short,
        )
    } else {
        0.0
    };
    // v6.10.19 (P5): the published per-side values are NET; the gross
    // rides in the new `long_gross_rr_internal` / `short_gross_rr_internal`.
    let long_expected_rr_internal = long_gross_rr_internal.map(|_| long_net_rr);
    let short_expected_rr_internal = short_gross_rr_internal.map(|_| short_net_rr);

    let long_geometry_consistent = rr_is_ok(&long_rr_status);
    let short_geometry_consistent = rr_is_ok(&short_rr_status);

    // `direction_family`: maps the active bias to a structured tag so
    // the frontend `selectProfileSide` can produce directional
    // arrows on profile cards. The per-profile `direction_family`
    // (TrendRiding/CounterTrend/Neutral) is set per-profile below
    // (each OpportunityType maps to one family via `direction_family_for`).
    let matrix_direction_family: Option<core_domain::opportunity::DirectionFamily> =
        if bias_directional {
            Some(core_domain::opportunity::DirectionFamily::TrendRiding)
        } else {
            Some(core_domain::opportunity::DirectionFamily::Neutral)
        };

    let time_horizon = default_time_horizon(primary_opportunity).to_string();

    let forecast_confidence = (analysis.state_confidence * (primary_score / 100.0)).clamp(0.0, 1.0);

    // Second pass: build each `OpportunityProfile` from precomputed zones,
    // R:R ratios, and the profile's own `direction_family` (which is a
    // function of `OpportunityType`, not the active bias). The per-profile
    // direction family decides which side's zones (long or short) the
    // profile populates:
    //   - TrendRiding  + bullish bias → LONG zones
    //   - TrendRiding  + bearish bias → SHORT zones
    //   - CounterTrend               → deviation-driven side (4b):
    //     MeanReversion follows the Z-Score sign, Reversal follows the
    //     divergence direction; falls back to family × bias
    //     (CounterTrend + bullish → SHORT, + bearish → LONG).
    //   - Neutral      + any bias     → no zones (DirectionalNeutral)
    //   - any family   + neutral bias → no zones (DirectionalNeutral)
    // The audit's bug-fix #1 was: the per-profile
    // `long_expected_rr_internal` / `short_expected_rr_internal` were
    // hardcoded to 0.0, breaking every per-profile card. We now
    // propagate the geometric R:R from the same zones that drive
    // `entry_zone` / `target_zone` / `invalidation_level`.
    for (ot, score, notes, raw_score, precondition_ratio, display_score, met, total) in &scored {
        let profile_family = analysis::direction_family_for(*ot);

        // Resolves the per-profile side based on the family + macro bias.
        // The tuple is (long_ez, long_tz, long_inv, long_rr, short_ez,
        // short_tz, short_inv, short_rr). Sides that don't apply carry
        // `None` for zones and 0.0 for R:R.
        let (
            pf_long_ez,
            pf_long_tz,
            pf_long_inv,
            pf_long_rr,
            pf_short_ez,
            pf_short_tz,
            pf_short_inv,
            pf_short_rr,
        ) = match (profile_family, bias_bullish, bias_bearish) {
            (analysis::DirectionFamily::TrendRiding, true, _) => (
                Some(long_entry_zone.clone()),
                Some(long_target_zone.clone()),
                Some(long_invalidation_level),
                long_expected_rr_internal.unwrap_or(0.0),
                None,
                None,
                None,
                0.0,
            ),
            (analysis::DirectionFamily::TrendRiding, false, true) => (
                None,
                None,
                None,
                0.0,
                Some(short_entry_zone.clone()),
                Some(short_target_zone.clone()),
                Some(short_invalidation_level),
                short_expected_rr_internal.unwrap_or(0.0),
            ),
            (analysis::DirectionFamily::CounterTrend, true, _)
            | (analysis::DirectionFamily::CounterTrend, false, true) => {
                // 4b: CounterTrend profiles surface the side the
                // market data supports (z-score for MeanReversion,
                // divergence direction for Reversal); family × bias
                // is the fallback when the data is ambiguous.
                let ct_long = countertrend_resolved_long.unwrap_or(!bias_bullish);
                if ct_long {
                    (
                        Some(long_entry_zone.clone()),
                        Some(long_target_zone.clone()),
                        Some(long_invalidation_level),
                        long_expected_rr_internal.unwrap_or(0.0),
                        None,
                        None,
                        None,
                        0.0,
                    )
                } else {
                    (
                        None,
                        None,
                        None,
                        0.0,
                        Some(short_entry_zone.clone()),
                        Some(short_target_zone.clone()),
                        Some(short_invalidation_level),
                        short_expected_rr_internal.unwrap_or(0.0),
                    )
                }
            }
            (analysis::DirectionFamily::Neutral, _, _)
            | (analysis::DirectionFamily::TrendRiding, _, _)
            | (analysis::DirectionFamily::CounterTrend, _, _) => {
                (None, None, None, 0.0, None, None, None, 0.0)
            }
        };

        // Per-profile `trade_viability`: only set when the profile is
        // the PRIMARY opportunity. The frontend uses this to highlight
        // actionable setups versus side profiles.
        // v6.10.18 (I-5): Actionable requires the bracket R:R ≥ 1.0;
        // valid sub-1 brackets demote to Qualifying (the trade is real,
        // the edge is not).
        let trade_viability_at_profile = if *ot == primary_opportunity {
            match (profile_family, bias_bullish, bias_bearish) {
                (analysis::DirectionFamily::Neutral, _, _) => {
                    Some(core_domain::opportunity::TradeViability::DirectionalNeutral)
                }
                (analysis::DirectionFamily::TrendRiding, true, _) => {
                    Some(viability_for(&long_rr_status, long_net_rr))
                }
                (analysis::DirectionFamily::TrendRiding, false, true) => {
                    Some(viability_for(&short_rr_status, short_net_rr))
                }
                (analysis::DirectionFamily::CounterTrend, true, _)
                | (analysis::DirectionFamily::CounterTrend, false, true) => {
                    // 4b: viability checks the SAME side the profile
                    // populated (deviation-driven, family × bias fallback).
                    let ct_long = countertrend_resolved_long.unwrap_or(!bias_bullish);
                    if ct_long {
                        Some(viability_for(&long_rr_status, long_net_rr))
                    } else {
                        Some(viability_for(&short_rr_status, short_net_rr))
                    }
                }
                _ => Some(core_domain::opportunity::TradeViability::DirectionalNeutral),
            }
        } else {
            None
        };

        profiles.push(OpportunityProfile {
            opportunity_type: *ot,
            score: *score,
            preconditions_met: *met,
            preconditions_total: *total,
            notes: notes.clone(),
            direction_family: Some(profile_family),
            long_geometry_consistent: pf_long_ez.is_some() && long_geometry_consistent,
            short_geometry_consistent: pf_short_ez.is_some() && short_geometry_consistent,
            long_entry_zone: pf_long_ez,
            long_target_zone: pf_long_tz,
            long_invalidation_level: pf_long_inv,
            long_expected_rr_internal: pf_long_rr,
            short_entry_zone: pf_short_ez,
            short_target_zone: pf_short_tz,
            short_invalidation_level: pf_short_inv,
            short_expected_rr_internal: pf_short_rr,
            trade_viability: trade_viability_at_profile,
            scoring_factors: Some(core_domain::analysis::ScoringFactors {
                raw_score: *raw_score,
                precondition_ratio: *precondition_ratio,
            }),
            display_score: Some(*display_score),
        });
    }

    // ── 4a: actionable side (single effective direction) ────────────────
    // The top qualifying profile's resolved side (zone-presence on the
    // wire — the same rule `selectProfileSide` implements in the
    // frontend) is the canonical direction for the matrix-level
    // surfaces: the legacy scalar zones, the confluent-level display,
    // and the invalidation note. The macro bias side is the fallback
    // when no profile qualifies or no zones resolve. This closes the
    // CounterTrend duality where a profile card could say LONG while
    // the note/confluent/legacy surfaces described the SHORT thesis.
    let top_side_long: Option<bool> = profiles
        .iter()
        .filter(|p| {
            p.preconditions_met > 0
                && p.opportunity_type != core_domain::analysis::OpportunityType::NoClearOpportunity
        })
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|p| {
            let has_long = p
                .long_entry_zone
                .as_ref()
                .map(|z| z.low > 0.0)
                .unwrap_or(false);
            let has_short = p
                .short_entry_zone
                .as_ref()
                .map(|z| z.low > 0.0)
                .unwrap_or(false);
            match (has_long, has_short) {
                (true, false) => Some(true),
                (false, true) => Some(false),
                _ => None,
            }
        });

    let actionable_side_long = top_side_long.unwrap_or(bias_bullish);

    // Legacy scalar fields key off the actionable side so PME/TAE
    // consumers and the Opportunities tab speak with one voice. The
    // per-direction siblings (`long_*_zone` / `short_*_zone`) are always
    // published untouched.
    let (entry_zone, target_zone, invalidation_level) = if actionable_side_long {
        (
            long_entry_zone.clone(),
            long_target_zone.clone(),
            long_invalidation_level,
        )
    } else {
        (
            short_entry_zone.clone(),
            short_target_zone.clone(),
            short_invalidation_level,
        )
    };

    // v7.3: the matrix-level confluent level sets carry the UNION of both
    // sides' pools (`derive_side_zones` computed long AND short confluent
    // levels). The actionable side alone was published before, so a
    // NoClear state whose actionable side fell back to SHORT surfaced
    // only SHORT-tagged levels while the panel showed a LONG reference
    // bracket — the Expected R:R section had no LONG row. Each side's
    // vector is already sorted by strength (desc); merge and stable-sort
    // so ties keep long-before-short determinism. No dedup is needed:
    // long and short levels are disjoint per vector by close-position
    // semantics (a structural level below close is a LONG entry / SHORT
    // target — it lands in different role vectors, never twice in one).
    let confluent_entry = {
        let mut merged: Vec<ConfluentLevel> = long_conf_entry
            .into_iter()
            .chain(short_conf_entry)
            .collect();
        merged.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged
    };
    let confluent_target = {
        let mut merged: Vec<ConfluentLevel> = long_conf_target
            .into_iter()
            .chain(short_conf_target)
            .collect();
        merged.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged
    };
    let confluent_inval = {
        let mut merged: Vec<ConfluentLevel> = long_conf_inval
            .into_iter()
            .chain(short_conf_inval)
            .collect();
        merged.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged
    };

    let contributing_signals: Vec<String> = indicators
        .values()
        .flat_map(|v| v.signals.iter())
        .filter(|s| s.strength > 0.3)
        .map(|s| s.label.clone())
        .collect();

    // Invalidation note — side-aware, and strictly bound to a level the
    // UI surfaces:
    //   - actionable side (4a) → the top qualifying profile's side and
    //     that side's invalidation (long: "A close below", short:
    //     "A close above").
    //   - no qualifying profile → the macro bias side's invalidation,
    //     which is exactly the side the frontend's reference brackets
    //     display (BULLISH ← long, BEARISH ← short).
    //   - neutral bias with no qualifying profile → there is NO
    //     directional thesis to invalidate, so the note is suppressed
    //     (same rule as NoClearOpportunity). The historical fallbacks —
    //     the geometry-consistent side heuristics and the legacy scalar
    //     `invalidation_level` position test — are gone: they emitted
    //     "Close below X" sentences whose level belonged to a side the
    //     frontend card never showed (bug D3, and the unbound-level
    //     class where the note quoted a matrix scalar while the card
    //     displayed a different stop-loss).
    let note = if let Some(long) = top_side_long {
        if long {
            Some((long_invalidation_level, "below"))
        } else {
            Some((short_invalidation_level, "above"))
        }
    } else if bias_bullish {
        Some((long_invalidation_level, "below"))
    } else if bias_bearish {
        Some((short_invalidation_level, "above"))
    } else {
        // v6.10.17 (F21): under NoClearOpportunity there is no thesis to
        // invalidate — "A close above X invalidates the
        // NoClearOpportunity thesis" is nonsense. Under a neutral bias
        // with no qualifying profile the setup card is NEUTRAL (or
        // absent) — same suppression, no directional sentence to emit.
        None
    };

    let invalidation_note = match note {
        Some((note_level, note_side))
            if primary_opportunity
                != core_domain::analysis::OpportunityType::NoClearOpportunity =>
        {
            format!(
                "A close {} {:.1} on the completed candle invalidates the {:?} thesis.",
                note_side, note_level, primary_opportunity
            )
        }
        _ => String::new(),
    };

    // v6.10.21 (NBR): the neutral range reference bracket is emitted only
    // when the primary is NoClearOpportunity AND the regime reads as a
    // range — a valid non-directional frame so the Range folder never
    // sits empty, informational only (never Actionable).
    let neutral_reference_bracket =
        if primary_opportunity == OpportunityType::NoClearOpportunity && is_range {
            derive_neutral_bracket(indicators, close, params)
        } else {
            None
        };

    Some(OpportunityMatrix {
        symbol: analysis.symbol.clone(),
        primary_opportunity,
        opportunity_score: primary_score,
        setup_quality: setup_quality_band_params(primary_score, params.quality_bands),
        profiles,
        forecast_confidence,
        contributing_signals,
        invalidation_note,
        entry_zone,
        target_zone,
        invalidation_level,
        long_entry_zone,
        long_target_zone,
        long_invalidation_level,
        short_entry_zone,
        short_target_zone,
        short_invalidation_level,
        long_expected_rr_internal: long_expected_rr_internal.unwrap_or(0.0),
        short_expected_rr_internal: short_expected_rr_internal.unwrap_or(0.0),
        long_gross_rr_internal: long_gross_rr_internal.unwrap_or(0.0),
        short_gross_rr_internal: short_gross_rr_internal.unwrap_or(0.0),
        time_horizon,
        confluent_entry_levels: confluent_entry,
        confluent_target_levels: confluent_target,
        confluent_invalidation_levels: confluent_inval,
        direction_family: matrix_direction_family,
        long_geometry_consistent,
        short_geometry_consistent,
        neutral_reference_bracket,
    })
}

pub fn synthesize_cross_tf(
    symbol: &str,
    tf_snapshots: &[(u64, &MarketSnapshot)],
    liquidity_flow: Option<&LiquidityFlow>,
    cluster: Option<&LiquidationClusterMatrix>,
    // AUDIT-AIU-062: discrete liquidity signals ride into the L5 cascade
    // risk dimension (previously computed but unused downstream).
    liquidity_signals: &[core_domain::liquidity::LiquiditySignal],
    previous_score: Option<f64>,
    previous_regime: Option<core_domain::analysis::MarketRegime>,
    previous_volume_dim: Option<f64>,
    previous_bias: Option<core_domain::analysis::MarketBias>,
    // v9 (F-04): wired opportunity-layer params (zone ATR fallback +
    // net-cost model).
    params: &OpportunityParams,
    // v9: the shared L6 DecisionParams (strategy `l6` section).
    decision_params: &core_domain::decision_params::DecisionParams,
    // v9: the strategy's L3 params.
    analysis_params: &core_domain::analysis::AnalysisParams,
    // v9: the strategy's L2 params.
    alignment_params: &core_domain::alignment::AlignmentParams,
    // v9: the strategy's L5 params.
    risk_params: &core_domain::risk::RiskParams,
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
    if params.ladder_roles.enabled {
        let mut ordered: Vec<(u64, &MarketSnapshot)> = Vec::new();
        let decision_label = params.ladder_roles.decision_tf.as_str();
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

    let close = tf_snapshots
        .first()
        .and_then(|(_, s)| s.close.as_ref())
        .and_then(|d| d.to_f64())
        .unwrap_or(0.0);

    // v6.10.18 (I-8): the per-TF L2 volatility states feed the L5
    // volatility-risk dimension (micro .7 / fast .3 — the actionable
    // horizons). The L2 score is signed −1..+1 → unipolar 0–100.
    let tf_volatility: Vec<(String, String, f64)> = tf_data
        .iter()
        .map(|(label, _, _, _, ctx)| {
            (
                label.to_string(),
                ctx.volatility.label.clone(),
                (((ctx.volatility.score + 1.0) / 2.0) * 100.0).clamp(0.0, 100.0),
            )
        })
        .collect();
    let risk = risk::compute_risk(
        &analysis.symbol,
        &analysis,
        &representative_indicators,
        liquidity_flow,
        cluster,
        close,
        liquidity_signals,
        // v6.10.9: the previous L2 mtf overall score feeds the derived
        // RiskState trend arm (Increasing/Improving/Stable).
        previous_score,
        &tf_volatility,
        // v9: the strategy's L5 params.
        risk_params,
    );

    let opportunity = compute_opportunity(
        &analysis,
        &alignment,
        &representative_indicators,
        liquidity_flow,
        cluster,
        close,
        params,
    );

    // v9 (F-03): the L3 deprecated `opportunity_analysis` mirror is
    // ERASED — the opportunity classification is L4-owned; the Analysis
    // Matrix no longer carries it and no sync step is needed.

    // v9 F-02: the documented SR_BASED protection precondition requires
    // `distance_to_nearest_SR < 0.5 · ATR`. The nearest structural level is
    // read from the representative indicator map (session pivots, Fibonacci,
    // volume-profile anchors); the distance is expressed in ATR multiples.
    // `None` = no structural levels available → SR_BASED cannot fire
    // (fail-closed, per the core-domain contract).
    let sr_distance_atr = nearest_sr_distance_atr(&representative_indicators, close);

    let advisory = advisory::compute_advisory(
        &analysis,
        &risk,
        opportunity.as_ref(),
        cluster,
        sr_distance_atr,
        decision_params,
        analysis_params,
    );

    CrossTfSynthesisResult {
        alignment,
        analysis,
        opportunity,
        risk,
        advisory,
    }
}

/// v9 F-02: distance from `close` to the nearest structural level in the
/// representative indicator map, expressed in ATR multiples.
///
/// Candidate levels are the session pivot stack (`pivot`, `s1..s3`,
/// `r1..r3`), every positive Fibonacci level value, and the volume-profile
/// anchors (`poc` / `vah` / `val`). `None` when no candidate exists or the
/// ATR reading is missing/zero (the advisory's SR_BASED branch then
/// fails closed to ATR_BASED).
fn nearest_sr_distance_atr(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    close: f64,
) -> Option<f64> {
    let atr = indicators
        .get("atr")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("atr_14").copied())
        .unwrap_or(0.0);
    if close <= 0.0 || atr <= 0.0 {
        return None;
    }
    let mut nearest: Option<f64> = None;
    let mut consider = |level: f64| {
        if level > 0.0 && level.is_finite() {
            let dist = (close - level).abs();
            nearest = Some(nearest.map_or(dist, |n: f64| n.min(dist)));
        }
    };
    // Session pivots: all seven levels are prices in the values sub-map.
    if let Some(pivots) = indicators
        .get("pivot_points")
        .and_then(|v| v.values.as_ref())
    {
        for key in ["pivot", "s1", "s2", "s3", "r1", "r2", "r3"] {
            if let Some(&level) = pivots.get(key) {
                consider(level);
            }
        }
    }
    // Fibonacci: every positive level value (gp_top/gp_bottom/fib_*/ext_*).
    if let Some(fib) = indicators.get("fibonacci").and_then(|v| v.values.as_ref()) {
        for &level in fib.values() {
            consider(level);
        }
    }
    // Volume profile anchors.
    if let Some(vp) = indicators
        .get("volume_profile")
        .and_then(|v| v.values.as_ref())
    {
        for key in ["poc", "vah", "val"] {
            if let Some(&level) = vp.get(key) {
                consider(level);
            }
        }
    }
    nearest.map(|dist| dist / atr)
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
mod tests {
    use super::*;
    use core_domain::indicator_dtos::NormalizedIndicatorValue;
    use core_domain::market_context::{ContextDimension, MarketContext};
    use core_domain::models::MarketSnapshot;
    use rust_decimal::Decimal;

    fn make_context(
        regime: &str,
        trend_score: f64,
        momentum_score: f64,
        vol_score: f64,
        volm_score: f64,
        overall: i32,
    ) -> MarketContext {
        MarketContext {
            trend: ContextDimension {
                score: trend_score,
                confidence: 0.7,
                label: "WEAK_BULL".into(),
            },
            momentum: ContextDimension {
                score: momentum_score,
                confidence: 0.6,
                label: "WEAK_BULL".into(),
            },
            volatility: ContextDimension {
                score: vol_score,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            volume: ContextDimension {
                score: volm_score,
                confidence: 0.5,
                label: "NORMAL".into(),
            },
            liquidity: ContextDimension::neutral(),
            regime: regime.to_string(),
            overall_score: overall,
            overall_label: if overall > 20 {
                "BULLISH".into()
            } else if overall < -20 {
                "BEARISH".into()
            } else {
                "NEUTRAL".into()
            },
        }
    }

    fn find_profile(
        opp: &OpportunityMatrix,
        ot: core_domain::analysis::OpportunityType,
    ) -> &core_domain::analysis::OpportunityProfile {
        opp.profiles
            .iter()
            .find(|p| p.opportunity_type == ot)
            .expect("profile must exist")
    }

    fn make_snapshot(secs: u64, price: f64, ctx: MarketContext) -> MarketSnapshot {
        let mut indicators: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        indicators.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(55.0, 0.5, "NEUTRAL"),
        );
        indicators.insert(
            "adx".into(),
            NormalizedIndicatorValue::scalar(28.0, 0.6, "TRENDING"),
        );
        indicators.insert(
            "rvol".into(),
            NormalizedIndicatorValue::scalar(1.2, 0.3, "NORMAL"),
        );
        indicators.insert(
            "bbwp".into(),
            NormalizedIndicatorValue::scalar(45.0, 0.5, "NORMAL"),
        );
        indicators.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(0.5, 0.2, "NEUTRAL"),
        );
        indicators.insert(
            "support_resistance".into(),
            NormalizedIndicatorValue::scalar(0.0, 0.0, "SUPPORT"),
        );

        let mut atr_values = HashMap::new();
        atr_values.insert("atr_14".into(), price * 0.01);
        indicators.insert(
            "atr".into(),
            NormalizedIndicatorValue {
                raw_value: price * 0.01,
                normalized: 0.0,
                state_label: "NORMAL".into(),
                values: Some(atr_values),
                signals: vec![],
                confidence: 0.5,
            },
        );

        let mut macd_values = HashMap::new();
        macd_values.insert("line".into(), 10.0);
        macd_values.insert("signal".into(), 8.0);
        macd_values.insert("histogram".into(), 2.0);
        indicators.insert(
            "macd".into(),
            NormalizedIndicatorValue {
                raw_value: 2.0,
                normalized: 0.4,
                state_label: "BULLISH".into(),
                values: Some(macd_values),
                signals: vec![],
                confidence: 0.6,
            },
        );

        let close = Decimal::from_f64_retain(price).unwrap_or_default();
        MarketSnapshot {
            timeframe_label: None,
            exchange: None,
            timeframe_secs: secs,
            timestamp: 0,
            symbol: "BTC-USD".into(),
            is_completed: Some(true),
            mid_price: close,
            bid_price: close,
            ask_price: close,
            bid_size: Some(Decimal::ONE),
            ask_size: Some(Decimal::ONE),
            funding_rate: None,
            open_interest: None,
            oi_delta_1h: None,
            mark_price: None,
            index_price: None,
            mark_index_spread_pct: None,
            prev_day_px: None,
            open: Some(close),
            high: Some(close),
            low: Some(close),
            close: Some(close),
            volume: Some(Decimal::ONE_HUNDRED),
            average_volume: Some(Decimal::ONE_HUNDRED),
            context: Some(ctx),
            decision_context: None,
            statistical_context: None,
            indicators,
            alignment: None,
            risk: None,
            analysis: None,
            advisory: None,
            opportunity: None,
            liquidity_signals: vec![],
            metrics_config: None,
            risk_profile: None,
            liquidity: None,
            cluster: None,
            volume_profile: None,
            quality_envelope: None,
            pipeline_state: core_domain::models::CandlePipelineState::default(),
            indicator_lifecycle: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn synthesize_empty_returns_neutral() {
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        assert_eq!(result.alignment.timeframes_present, 0);
        assert_eq!(result.analysis.timeframes_considered, 0);
        assert_eq!(
            result.advisory.directional_guidance,
            advisory::DirectionalGuidance::Neutral
        );
    }

    #[test]
    fn synthesize_single_tf_works() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        assert_eq!(result.alignment.timeframes_present, 1);
        assert_eq!(result.alignment.dimensions.len(), 10);
        assert!(result.analysis.state_confidence <= 0.5);
    }

    #[test]
    fn synthesize_four_tf_aligned_bullish() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap60 = make_snapshot(60, 64000.0, ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        assert_eq!(result.alignment.timeframes_present, 4);
        assert!(result.alignment.mtf_overall_score > 0.0);
        assert!(result.analysis.state_confidence > 0.5);
        assert!(matches!(
            result.analysis.bias,
            analysis::MarketBias::Bullish | analysis::MarketBias::StrongBullish
        ));
        assert!(result.opportunity.is_some());
    }

    #[test]
    fn synthesize_mixed_tf_is_neutral() {
        let bull_ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let bear_ctx = make_context("TRENDING", -0.7, -0.6, 0.2, 0.1, -60);
        let snap60 = make_snapshot(60, 64000.0, bull_ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, bear_ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, bull_ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, bear_ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        assert!(result.alignment.mtf_overall_score.abs() < 40.0);
    }

    #[test]
    fn opportunity_emits_both_directional_zones() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap60 = make_snapshot(60, 64000.0, ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        assert!(opp.long_entry_zone.high >= opp.long_entry_zone.low);
        assert!(opp.long_target_zone.high >= opp.long_target_zone.low);
        assert!(opp.short_entry_zone.high >= opp.short_entry_zone.low);
        assert!(opp.short_target_zone.high >= opp.short_target_zone.low);
        assert!(opp.long_invalidation_level > 0.0);
        assert!(opp.short_invalidation_level > 0.0);
        // v6.10.x: regression-locking invariants — target zones must never
        // publish a non-positive bound (Bug A: pivot_points series with
        // s1=s2=s3=0 leaked through as target candidates and dragged
        // `short_target_zone.low` to 0).
        assert!(
            opp.long_target_zone.low > 0.0,
            "long_target_zone.low must be > 0 (was {})",
            opp.long_target_zone.low
        );
        assert!(
            opp.long_target_zone.high > 0.0,
            "long_target_zone.high must be > 0 (was {})",
            opp.long_target_zone.high
        );
        assert!(
            opp.short_target_zone.low > 0.0,
            "short_target_zone.low must be > 0 (was {})",
            opp.short_target_zone.low
        );
        assert!(
            opp.short_target_zone.high > 0.0,
            "short_target_zone.high must be > 0 (was {})",
            opp.short_target_zone.high
        );
    }

    /// Regression for Bug A — observed on BTC-USDT (Bitget) 2026-08-11.
    /// The `pivot_points` indicator emits `s1=s2=s3=r1=r2=r3=pivot=0.0`
    /// with state_label `PIVOT_UNAVAILABLE` when its window has not yet
    /// accumulated enough bars. The previous SHORT-target candidate
    /// filter (`v < close`) accepted those zeros — every `0 < close` is
    /// true — and they propagated into `short_target_zone.low = 0`,
    /// which the frontend surfaced verbatim as `$0–$X`. This test
    /// reproduces the offending snapshot shape and asserts the
    /// `short_target_zone.low > 0` invariant holds.
    #[test]
    fn target_zone_rejects_zero_candidates_when_pivot_unavailable() {
        let ctx = make_context("RANGE", 0.0, 0.0, 0.2, 0.1, 0);
        let mut snap60 = make_snapshot(60, 63604.0, ctx.clone());
        // Inject the exact buggy shape observed in production: a
        // `pivot_points` entry whose sub-keys are all 0.0 with the
        // `PIVOT_UNAVAILABLE` label.
        let mut pp_vals = std::collections::HashMap::new();
        for k in ["pivot", "r1", "r2", "r3", "s1", "s2", "s3"] {
            pp_vals.insert(k.to_string(), 0.0_f64);
        }
        snap60.indicators.insert(
            "pivot_points".into(),
            core_domain::indicator_dtos::NormalizedIndicatorValue {
                raw_value: 0.0,
                normalized: 0.0,
                state_label: "PIVOT_UNAVAILABLE".into(),
                values: Some(pp_vals),
                signals: vec![],
                confidence: 0.0,
            },
        );
        // And a volume_profile with `val = 0` (another observed shape
        // when the profile window hasn't filled).
        let mut vp_vals = std::collections::HashMap::new();
        vp_vals.insert("poc".to_string(), 0.0_f64);
        vp_vals.insert("vah".to_string(), 0.0_f64);
        vp_vals.insert("val".to_string(), 0.0_f64);
        vp_vals.insert("total_volume".to_string(), 0.0_f64);
        snap60.indicators.insert(
            "volume_profile".into(),
            core_domain::indicator_dtos::NormalizedIndicatorValue {
                raw_value: 0.0,
                normalized: 0.0,
                state_label: "VP_UNAVAILABLE".into(),
                values: Some(vp_vals),
                signals: vec![],
                confidence: 0.0,
            },
        );
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap60)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        assert!(
            opp.short_target_zone.low > 0.0,
            "short_target_zone.low must be > 0 when zero-valued pivot/volume candidates are injected (was {})",
            opp.short_target_zone.low
        );
        assert!(
            opp.short_target_zone.high > 0.0,
            "short_target_zone.high must be > 0 (was {})",
            opp.short_target_zone.high
        );
        assert!(
            opp.long_target_zone.low > 0.0,
            "long_target_zone.low must be > 0 (was {})",
            opp.long_target_zone.low
        );
        assert!(
            opp.long_target_zone.high > 0.0,
            "long_target_zone.high must be > 0 (was {})",
            opp.long_target_zone.high
        );
    }

    #[test]
    fn directional_target_zones_are_geometrically_separated() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap60 = make_snapshot(60, 64000.0, ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        let long_target_mid = (opp.long_target_zone.low + opp.long_target_zone.high) / 2.0;
        let short_target_mid = (opp.short_target_zone.low + opp.short_target_zone.high) / 2.0;
        let close = 64000.0;
        assert!(
            long_target_mid >= close,
            "long target mid {long_target_mid} must be >= close {close}"
        );
        assert!(
            short_target_mid <= close,
            "short target mid {short_target_mid} must be <= close {close}"
        );
    }

    #[test]
    fn directional_invalidation_levels_are_geometrically_separated() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap60 = make_snapshot(60, 64000.0, ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        let close = 64000.0;
        assert!(
            opp.long_invalidation_level < close,
            "long invalidation {} must be < close {close}",
            opp.long_invalidation_level
        );
        assert!(
            opp.short_invalidation_level > close,
            "short invalidation {} must be > close {close}",
            opp.short_invalidation_level
        );
    }

    #[test]
    fn legacy_scalar_fields_mirror_long_side_when_bullish() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let snap60 = make_snapshot(60, 64000.0, ctx.clone());
        let snap180 = make_snapshot(180, 64100.0, ctx.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx.clone());
        let snap900 = make_snapshot(900, 64300.0, ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        assert!(matches!(
            result.analysis.bias,
            analysis::MarketBias::Bullish | analysis::MarketBias::StrongBullish
        ));
        assert_eq!(opp.entry_zone.low, opp.long_entry_zone.low);
        assert_eq!(opp.entry_zone.high, opp.long_entry_zone.high);
        assert_eq!(opp.target_zone.low, opp.long_target_zone.low);
        assert_eq!(opp.target_zone.high, opp.long_target_zone.high);
        assert_eq!(opp.invalidation_level, opp.long_invalidation_level);
    }

    #[test]
    fn legacy_scalar_fields_mirror_short_side_when_bearish() {
        let bear_ctx = make_context("TRENDING", -0.7, -0.6, 0.2, 0.1, -60);
        let snap60 = make_snapshot(60, 64000.0, bear_ctx.clone());
        let snap180 = make_snapshot(180, 63900.0, bear_ctx.clone());
        let snap300 = make_snapshot(300, 63800.0, bear_ctx.clone());
        let snap900 = make_snapshot(900, 63700.0, bear_ctx.clone());
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");
        assert!(matches!(
            result.analysis.bias,
            analysis::MarketBias::Bearish | analysis::MarketBias::StrongBearish
        ));
        assert_eq!(opp.entry_zone.low, opp.short_entry_zone.low);
        assert_eq!(opp.entry_zone.high, opp.short_entry_zone.high);
        assert_eq!(opp.target_zone.low, opp.short_target_zone.low);
        assert_eq!(opp.target_zone.high, opp.short_target_zone.high);
        assert_eq!(opp.invalidation_level, opp.short_invalidation_level);
    }

    // ─── v6.10.1 (bug-fix): the four regression-locking tests for the
    // `opportunity_score = raw * ratio` bug — the user observed 5 of 7
    // profiles silently scored 0 whenever preconditions were unmet. These
    // tests lock in:
    //   (1) inactive setups still surface raw viability;
    //   (2) NoClearOpportunity stays the unconditional zero;
    //   (3) `scoring_factors.precondition_ratio` is preserved on the
    //       Rust struct (telemetry consumers can still read the ratio);
    //   (4) `primary_opportunity` selection is unaffected by the fix
    //       (it was already driven by raw preconditions, not by score).

    #[test]
    fn inactive_candidates_survive_precondition_discount() {
        // Mirrors the user's screenshot: BTC +0.78% with a moderate-vol
        // mid-range regime. The four big conditional setups (Trend,
        // Breakout, Reversal, MeanReversion) almost never have all
        // preconditions met on a quiet-volatility regime, but their raw
        // viability must still show through to the dashboard.
        let ctx = make_context("COMPRESSION", 0.55, 0.50, 0.40, 0.45, 25);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap), (180, &snap), (300, &snap), (900, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        assert!(!opp.profiles.is_empty());

        // Every non-NoClear profile must have a non-zero score now
        // (the previous v6.10 implementation forced every score with
        // 0/N preconditions to 0).
        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        for p in &opp.profiles {
            if p.opportunity_type != analysis::OpportunityType::NoClearOpportunity {
                assert!(
                    p.score > 0.0,
                    "inactive profile {:?} has score 0 (raw viability dropped): score={}, raw={:?}",
                    p.opportunity_type,
                    p.score,
                    p.scoring_factors.as_ref().map(|sf| sf.raw_score),
                );
            }
        }
    }

    #[test]
    fn no_clear_opportunity_score_is_unconditional_zero() {
        // NoClearOpportunity is the explicit "no setup detected"
        // placeholder and must stay at score 0 regardless of the fix.
        // It has a single precondition (`tradability_dim < 30.0`); when
        // met, the previous code still emitted score 0. The fix
        // preserves that semantic via the explicit branch above.
        let ctx = make_context("RANGE", 0.30, 0.30, 0.20, 0.20, -5);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap), (180, &snap), (300, &snap), (900, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        let no_clear = opp
            .profiles
            .iter()
            .find(|p| p.opportunity_type == analysis::OpportunityType::NoClearOpportunity)
            .expect("NoClearOpportunity profile must be present in every OpportunityMatrix");
        assert_eq!(
            no_clear.score, 0.0,
            "NoClearOpportunity must stay at score 0"
        );
    }

    #[test]
    fn precondition_ratio_is_preserved_in_scoring_factors() {
        // The fix dropped `raw * ratio` from `score`, but the ratio is
        // still published on the wire via the per-profile
        // `scoring_factors.precondition_ratio` field (serde-skipped per
        // the Rust struct definition, but kept for telemetry consumers
        // that read profiles directly).
        let ctx = make_context("TRENDING", 0.50, 0.50, 0.50, 0.50, 10);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap), (180, &snap), (300, &snap), (900, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        for p in &opp.profiles {
            let sf = p
                .scoring_factors
                .as_ref()
                .expect("scoring_factors must be present on every profile");
            let expected_ratio = if p.preconditions_total > 0 {
                p.preconditions_met as f64 / p.preconditions_total as f64
            } else {
                0.0
            };
            assert!(
                (sf.precondition_ratio - expected_ratio).abs() < 1e-9,
                "precondition_ratio drifted: {} (expected {})",
                sf.precondition_ratio,
                expected_ratio,
            );
            // raw_score must also still be in [0, 100]
            assert!(
                sf.raw_score >= 0.0 && sf.raw_score <= 100.0,
                "raw_score out of range: {}",
                sf.raw_score,
            );
            // After the fix, score == raw_score for non-NoClear profiles
            if p.opportunity_type != analysis::OpportunityType::NoClearOpportunity {
                assert!(
                    (p.score - sf.raw_score).abs() < 1e-9,
                    "non-NoClear score ({}) must equal raw_score ({}) after fix",
                    p.score,
                    sf.raw_score,
                );
            } else {
                // NoClearOpportunity stays at 0 regardless of raw_score
                assert_eq!(p.score, 0.0);
            }
            // v6.14: `display_score` must be the wire-emitted scaled score
            // — `round(score × min(1, ratio))`, additive on top of the raw
            // `score` (the raw value is preserved for data-science).
            let display = p
                .display_score
                .expect("display_score must be present on every profile");
            let expected_display = (p.score * expected_ratio.min(1.0)).round();
            assert!(
                (display - expected_display).abs() < 1e-9,
                "display_score drifted: {} (expected {}) for {:?} (score {}, ratio {})",
                display,
                expected_display,
                p.opportunity_type,
                p.score,
                expected_ratio,
            );
        }
    }

    #[test]
    fn display_score_is_zero_for_dead_setups_but_raw_score_survives() {
        // v6.14: the operator-facing `display_score` scales to 0 when no
        // precondition is met (0/N), while the raw `score` keeps showing
        // how close the setup is to firing — the v6.10.1 "hide the dead
        // setups' viability" regression must never come back via the new
        // additive field. NoClearOpportunity is 0 in both.
        let ctx = make_context("COMPRESSION", 0.55, 0.50, 0.40, 0.45, 25);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap), (180, &snap), (300, &snap), (900, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        assert!(!opp.profiles.is_empty());
        let mut saw_dead_with_raw_viability = false;
        for p in &opp.profiles {
            let display = p
                .display_score
                .expect("display_score must be present on every profile");
            if p.opportunity_type == analysis::OpportunityType::NoClearOpportunity {
                assert_eq!(p.score, 0.0);
                assert_eq!(display, 0.0);
                continue;
            }
            if p.preconditions_met == 0 {
                // 0/N → the operator-facing score is muted to 0 …
                assert_eq!(
                    display, 0.0,
                    "{:?} with 0/N preconditions must display 0",
                    p.opportunity_type
                );
                // … but the raw viability blend must still surface.
                assert!(
                    p.score > 0.0,
                    "{:?} raw score must survive the scale",
                    p.opportunity_type
                );
                saw_dead_with_raw_viability = true;
            }
        }
        assert!(
            saw_dead_with_raw_viability,
            "scenario must contain at least one setup with 0/N preconditions"
        );
    }

    #[test]
    fn primary_opportunity_unaffected_by_score_fix() {
        // The fix changed `score` to drop the precondition ratio, but
        // `primary_opportunity` is selected from a separate chain at
        // synthesis.rs:800-819 (raw preconditions, not the score). The
        // primary's reported `opportunity_score` should also be the raw
        // viability, not a discounted value.
        let ctx = make_context("TRENDING", 0.65, 0.60, 0.55, 0.55, 45);
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap), (180, &snap), (300, &snap), (900, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        // The headline `opportunity_score` equals the selected primary
        // profile's `score` (synthesis.rs:916-920). After the fix both
        // are equal to the primary profile's raw viability.
        let opp = result
            .opportunity
            .as_ref()
            .expect("opportunity must be emitted");
        let primary_type = opp.primary_opportunity;
        let primary_profile = opp
            .profiles
            .iter()
            .find(|p| p.opportunity_type == primary_type)
            .expect("primary_opportunity must be present in profiles");
        assert!(
            (opp.opportunity_score - primary_profile.score).abs() < 1e-9,
            "matrix-level score ({}) must equal primary profile score ({})",
            opp.opportunity_score,
            primary_profile.score,
        );
        // Setup quality derives from opportunity_score via the same
        // private `setup_quality_band_params` helper, so the matrix-level and
        // primary-profile scores must classify identically.
        assert_eq!(
            opp.setup_quality,
            setup_quality_band_params(primary_profile.score, [85.0, 70.0, 50.0, 30.0]),
            "matrix-level setup_quality must match primary profile score",
        );
    }

    /// Phase B regression: `representative_indicators` is now a per-key
    /// union across all 4 TFs rather than the first non-empty TF's
    /// snapshot. Build a scenario where TF1 (the first iteration slot)
    /// has no `fibonacci` indicator, but TF4 (last iteration slot) does.
    /// The confluent levels must still populate from TF4's Fibonacci.
    ///
    /// We can't easily null out an indicator on the existing
    /// `make_snapshot` helper, so we hand-build the TF1 snapshot and
    /// reuse `make_snapshot` for the others.
    #[test]
    fn representative_indicators_merges_across_tfs_when_first_tf_lacks_fib() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);

        // TF1 snapshot: drop the `fibonacci` indicator entirely.
        let snap1 = {
            let mut s = make_snapshot(60, 64000.0, ctx.clone());
            s.indicators.remove("fibonacci");
            s
        };
        let snap2 = make_snapshot(180, 64100.0, ctx.clone());
        let snap3 = make_snapshot(300, 64200.0, ctx.clone());
        // TF4 keeps Fibonacci (default in make_snapshot).
        let snap4 = make_snapshot(900, 64300.0, ctx);

        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap1), (180, &snap2), (300, &snap3), (900, &snap4)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");

        // Before the fix, the per-key merge took the first non-empty
        // TF's indicator map; since TF1 had no fibonacci at all,
        // confluent levels stayed empty. After the fix, the union pulls
        // fibonacci from TF4 (or whichever later TF has it) and the
        // entry/target pools populate.
        assert!(
            !opp.confluent_entry_levels.is_empty() || !opp.confluent_target_levels.is_empty(),
            "confluent levels must populate from a later TF even when TF1 \
             lacks fibonacci (got entry={:?}, target={:?})",
            opp.confluent_entry_levels.len(),
            opp.confluent_target_levels.len(),
        );
    }

    /// Phase C regression: when every structural source (Fibonacci /
    /// Volume Profile / Pivot Points / Liquidation Clusters) is empty,
    /// the ATR fallback fires and emits at least one entry / target
    /// level derived from `close ± k·ATR`. The fallback is hard-coded
    /// ON by default (matches `OpportunityMatrixConfig::default()`) and
    /// exists so the Opportunities panel never shows the empty state
    /// for a healthy market.
    #[test]
    fn atr_fallback_fires_when_candidate_pool_is_empty() {
        let ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);

        // Build a snapshot whose indicators have NO fibonacci / Volume
        // Profile / Pivot Points / Liquidation Cluster values that
        // match the entry/target proximity conditions. `make_snapshot`
        // already sets Fibonacci/VP to empty (no values are emitted by
        // `make_snapshot`), and we explicitly clear the support_resistance
        // indicator and pass `None` for the cluster.
        let snap = make_snapshot(60, 64000.0, ctx);
        let result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = result.opportunity.expect("opportunity must be emitted");

        // ATR fallback must populate at least one entry level.
        assert!(
            !opp.confluent_entry_levels.is_empty(),
            "ATR fallback must emit at least one entry level when candidate pool is empty (got {})",
            opp.confluent_entry_levels.len(),
        );
        // And at least one target level.
        assert!(
            !opp.confluent_target_levels.is_empty(),
            "ATR fallback must emit at least one target level when candidate pool is empty (got {})",
            opp.confluent_target_levels.len(),
        );
        // The fallback levels must be flagged with the AtrFallback
        // source so the dashboard can render them with a distinct
        // visual style (the panel's `sourceColor` already maps
        // `LevelSource::AtrFallback` to its own colour).
        assert!(
            opp.confluent_entry_levels
                .iter()
                .any(|l| l.sources.contains(&LevelSource::AtrFallback)),
            "entry fallback level must carry the AtrFallback source marker"
        );
    }

    /// Phase C pin: the ATR fallback's directionality is correct.
    /// For a bullish bias the fallback entry sits BELOW close and the
    /// fallback target sits ABOVE close. For a bearish bias it's the
    /// mirror. v7.3: the matrix-level confluent sets carry the union of
    /// BOTH sides' levels, so the directionality pin must select the
    /// level by its side tag instead of `.first()` (the merged vector
    /// holds long levels ahead of short on equal strength).
    #[test]
    fn atr_fallback_levels_respect_bias_directionality() {
        // Bullish context → bias Bullish → entry below close.
        let bull_ctx = make_context("TRENDING", 0.7, 0.6, 0.2, 0.1, 60);
        let bull_snap = make_snapshot(60, 64000.0, bull_ctx);
        let bull_result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &bull_snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let bull_opp = bull_result.opportunity.expect("bullish opp");
        let close = 64000.0_f64;
        let bull_entry = bull_opp
            .confluent_entry_levels
            .iter()
            .find(|l| l.side.as_deref() == Some("LONG"))
            .expect("bullish fallback entry must be present");
        let bull_target = bull_opp
            .confluent_target_levels
            .iter()
            .find(|l| l.side.as_deref() == Some("LONG"))
            .expect("bullish fallback target must be present");
        assert!(
            bull_entry.price < close,
            "bullish fallback entry {} must be < close {close}",
            bull_entry.price
        );
        assert!(
            bull_target.price > close,
            "bullish fallback target {} must be > close {close}",
            bull_target.price
        );

        // Bearish context → bias Bearish → entry above close.
        let bear_ctx = make_context("TRENDING", -0.7, -0.6, 0.2, 0.1, -60);
        let bear_snap = make_snapshot(60, 64000.0, bear_ctx);
        let bear_result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &bear_snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let bear_opp = bear_result.opportunity.expect("bearish opp");
        let bear_entry = bear_opp
            .confluent_entry_levels
            .iter()
            .find(|l| l.side.as_deref() == Some("SHORT"))
            .expect("bearish fallback entry must be present");
        let bear_target = bear_opp
            .confluent_target_levels
            .iter()
            .find(|l| l.side.as_deref() == Some("SHORT"))
            .expect("bearish fallback target must be present");
        assert!(
            bear_entry.price > close,
            "bearish fallback entry {} must be > close {close}",
            bear_entry.price
        );
        assert!(
            bear_target.price < close,
            "bearish fallback target {} must be < close {close}",
            bear_target.price
        );
    }

    /// v7.3 pin: the matrix-level confluent level sets carry the UNION of
    /// both sides' pools, not just the actionable side's. Before the fix a
    /// NoClear state whose actionable side fell back to SHORT published
    /// only SHORT-tagged levels while the panel showed a LONG reference
    /// bracket — the frontend Expected R:R section had no LONG row. Both
    /// sides' pools are always derived (`derive_side_zones` runs for both
    /// biases), so even a single-sided bias context must surface LONG AND
    /// SHORT-tagged levels once the union is published.
    #[test]
    fn confluent_levels_union_both_sides_even_when_single_side_actionable() {
        // Bearish context → actionable side SHORT (the pre-fix publisher
        // would emit short_conf_* only).
        let bear_ctx = make_context("TRENDING", -0.7, -0.6, 0.2, 0.1, -60);
        let bear_snap = make_snapshot(60, 64000.0, bear_ctx);
        let bear_result = synthesize_cross_tf(
            "BTC-USD",
            &[(60, &bear_snap)],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );
        let opp = bear_result.opportunity.expect("bearish opp");
        assert!(
            opp.confluent_entry_levels
                .iter()
                .any(|l| l.side.as_deref() == Some("SHORT")),
            "SHORT entry levels must be present (actionable side)"
        );
        assert!(
            opp.confluent_entry_levels
                .iter()
                .any(|l| l.side.as_deref() == Some("LONG")),
            "LONG entry levels must also be present (union — informational bracket side)"
        );
        assert!(
            opp.confluent_target_levels
                .iter()
                .any(|l| l.side.as_deref() == Some("SHORT")),
            "SHORT target levels must be present"
        );
        assert!(
            opp.confluent_target_levels
                .iter()
                .any(|l| l.side.as_deref() == Some("LONG")),
            "LONG target levels must also be present (union)"
        );
    }

    #[test]

    fn alignment_rows_echo_the_exact_contexts_supplied() {
        // D4 contract: `AlignmentMatrix.timeframe_alignments` must carry the
        // per-TF context values bit-for-bit (trend score, overall score,
        // regime, signal count). The ETH export inconsistency (FAST row
        // -7/21 signals vs the metrics tab +8/31 for the same candle) was a
        // pipeline-race feeding a stale snapshot into `synthesize_cross_tf`;
        // this test locks the derive contract so any future regression is
        // visible at the matrix level.
        let ctx_fast = make_context("COMPRESSION", 0.06898435026003727, 0.09, 0.1, 0.1, 8);
        let ctx_slow = make_context("RANGE", -0.20007980505874928, 0.06, 0.1, 0.1, -9);
        let snap60 = make_snapshot(
            60,
            64000.0,
            make_context("TRENDING", 0.4656, 0.05, 0.1, 0.1, 25),
        );
        let snap180 = make_snapshot(180, 64100.0, ctx_fast.clone());
        let snap300 = make_snapshot(300, 64200.0, ctx_slow.clone());
        let snap900 = make_snapshot(
            900,
            64300.0,
            make_context("COMPRESSION", -0.1648, 0.2, 0.1, 0.1, -1),
        );

        let result = synthesize_cross_tf(
            "BTC-USD",
            &[
                (60, &snap60),
                (180, &snap180),
                (300, &snap300),
                (900, &snap900),
            ],
            None,
            None,
            &[],
            None,
            None,
            None,
            None,
            &OpportunityParams::default(),
            &core_domain::decision_params::DecisionParams::default(),
            &core_domain::analysis::AnalysisParams::default(),
            &core_domain::alignment::AlignmentParams::default(),
            &core_domain::risk::RiskParams::default(),
        );

        let rows = &result.alignment.timeframe_alignments;
        assert_eq!(rows.len(), 4);
        let fast = rows
            .iter()
            .find(|r| r.timeframe_secs == 180)
            .expect("fast row");
        assert!(
            (fast.trend_score - ctx_fast.trend.score).abs() < 1e-12,
            "fast trend_score {} must equal the supplied context {}",
            fast.trend_score,
            ctx_fast.trend.score
        );
        assert_eq!(fast.overall_score, ctx_fast.overall_score);
        assert_eq!(fast.regime, ctx_fast.regime);
        let slow = rows
            .iter()
            .find(|r| r.timeframe_secs == 300)
            .expect("slow row");
        assert!(
            (slow.trend_score - ctx_slow.trend.score).abs() < 1e-12,
            "slow trend_score {} must equal the supplied context {}",
            slow.trend_score,
            ctx_slow.trend.score
        );
        assert_eq!(slow.overall_score, ctx_slow.overall_score);
    }

    // ── B2: primary selection must meet its own preconditions ───────────

    #[test]
    fn mean_reversion_primary_requires_range_regime() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        let indicators: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        // vol_dim = 25 (≤ 30) — the only MeanReversion precondition input.
        alignment.mtf_volatility_alignment = -0.5;

        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Neutral;

        // Range regime → MeanReversion qualifies (2/2 preconditions).
        analysis.market_regime = MarketRegime::Range;
        let opp_range = compute_opportunity(
            &analysis,
            &alignment,
            &indicators,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("range opportunity");
        assert_eq!(
            opp_range.primary_opportunity,
            OpportunityType::MeanReversion
        );
        let p = opp_range
            .profiles
            .iter()
            .find(|p| p.opportunity_type == OpportunityType::MeanReversion)
            .expect("MeanReversion profile");
        assert_eq!((p.preconditions_met, p.preconditions_total), (2, 2));

        // Expansion regime → the same vol reads must NOT headline
        // MeanReversion with 0/2 preconditions (B2). Falls through to
        // NoClearOpportunity.
        analysis.market_regime = MarketRegime::Expansion;
        let opp_expansion = compute_opportunity(
            &analysis,
            &alignment,
            &indicators,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("expansion opportunity");
        assert_ne!(
            opp_expansion.primary_opportunity,
            OpportunityType::MeanReversion,
            "MeanReversion must not be primary outside a range regime"
        );
        assert_eq!(
            opp_expansion.primary_opportunity,
            OpportunityType::NoClearOpportunity
        );
        let no_clear = opp_expansion
            .profiles
            .iter()
            .find(|p| p.opportunity_type == OpportunityType::NoClearOpportunity)
            .expect("NoClearOpportunity profile");
        assert_eq!(
            (no_clear.preconditions_met, no_clear.preconditions_total),
            (0, 1),
            "v6.10.19a (N1): the no-setup sentinel must never read met — \
             the strip would otherwise claim '1/1 preconditions met' next \
             to 'informational only'"
        );
    }

    // ── 4b: CounterTrend deviation-driven side resolution ────────────────

    #[test]
    fn resolve_countertrend_side_mean_reversion_follows_zscore_sign() {
        use std::collections::HashMap;

        let mut ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(1.2, 0.6, "EXTENDED"),
        );
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::MeanReversion, &ind),
            Some(false),
            "z ≥ +threshold → SHORT (sell the rip)"
        );

        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(-1.4, 0.6, "EXTENDED"),
        );
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::MeanReversion, &ind),
            Some(true),
            "z ≤ −threshold → LONG (buy the dip)"
        );

        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(0.1, 0.6, "NEUTRAL"),
        );
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::MeanReversion, &ind),
            None,
            "|z| < threshold → ambiguous, caller falls back to family × bias"
        );
    }

    #[test]
    fn resolve_countertrend_side_reversal_follows_divergence_direction() {
        use core_domain::indicator_dtos::{
            IndicatorSignal, SignalDirection, SignalKind, SignalStatus,
        };
        use std::collections::HashMap;

        let mut ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        let insert_divergence = |ind: &mut HashMap<String, NormalizedIndicatorValue>,
                                 label: &str| {
            ind.insert(
                "rsi".into(),
                NormalizedIndicatorValue {
                    raw_value: 50.0,
                    normalized: 0.0,
                    state_label: "NEUTRAL".into(),
                    values: None,
                    signals: vec![IndicatorSignal::new(
                        SignalKind::Divergence,
                        SignalDirection::Bullish,
                        SignalStatus::Confirmed,
                        label,
                    )
                    .with_strength(0.8)],
                    confidence: 0.8,
                },
            );
        };

        insert_divergence(&mut ind, "CONFIRMED_BULLISH_DIVERGENCE");
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::Reversal, &ind),
            Some(true)
        );

        insert_divergence(&mut ind, "CONFIRMED_BEARISH_DIVERGENCE");
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::Reversal, &ind),
            Some(false)
        );

        insert_divergence(&mut ind, "CONFIRMED_DIVERGENCE");
        assert_eq!(
            resolve_countertrend_side(analysis::OpportunityType::Reversal, &ind),
            None
        );
    }

    #[test]
    fn countertrend_mean_reversion_surfaces_deviation_side() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        // Bearish bias + Range regime + compressed volatility → the
        // primary is MeanReversion (CounterTrend family).
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Bearish;
        analysis.market_regime = MarketRegime::Range;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.mtf_volatility_alignment = -0.5;

        // z ≥ +threshold → SHORT (sell the rip): the profile surfaces
        // SHORT zones, and the 4a matrix-level surfaces follow it.
        let mut ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(1.2, 0.6, "EXTENDED"),
        );
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::MeanReversion);
        let p = find_profile(&opp, OpportunityType::MeanReversion);
        assert!(
            p.long_entry_zone.is_none(),
            "z>0 must not surface LONG zones"
        );
        assert!(
            p.short_entry_zone.is_some(),
            "z>0 must surface SHORT zones (sell the rip)"
        );
        assert!(
            opp.invalidation_note.starts_with("A close above "),
            "note must reference the SHORT thesis, was: {}",
            opp.invalidation_note
        );
        assert_eq!(
            opp.entry_zone, opp.short_entry_zone,
            "legacy scalars must follow the actionable SHORT side"
        );

        // z ≤ −threshold → LONG (buy the dip).
        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(-1.4, 0.6, "EXTENDED"),
        );
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        let p = find_profile(&opp, OpportunityType::MeanReversion);
        assert!(
            p.short_entry_zone.is_none(),
            "z<0 must not surface SHORT zones"
        );
        assert!(
            p.long_entry_zone.is_some(),
            "z<0 must surface LONG zones (buy the dip)"
        );
        assert!(
            opp.invalidation_note.starts_with("A close below "),
            "note must reference the LONG thesis, was: {}",
            opp.invalidation_note
        );
        assert_eq!(
            opp.entry_zone, opp.long_entry_zone,
            "legacy scalars must follow the actionable LONG side"
        );

        // Ambiguous z (≈ 0) → family × bias fallback: bearish bias →
        // LONG (counter-trend buy-the-dip).
        ind.insert(
            "zscore".into(),
            NormalizedIndicatorValue::scalar(0.0, 0.6, "NEUTRAL"),
        );
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        let p = find_profile(&opp, OpportunityType::MeanReversion);
        assert!(
            p.long_entry_zone.is_some(),
            "ambiguous z must fall back to family × bias (bearish → LONG)"
        );
        assert!(p.short_entry_zone.is_none());
    }

    #[test]
    fn countertrend_reversal_follows_divergence_direction() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use core_domain::indicator_dtos::{
            IndicatorSignal, SignalDirection, SignalKind, SignalStatus,
        };
        use std::collections::HashMap;

        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Bullish;
        analysis.market_regime = MarketRegime::Expansion;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        // structure_broken (< 40) + momentum_exhausted (< 25) so the
        // divergence triggers the Reversal branch.
        if let Some(d) = alignment.dimensions.get_mut(4) {
            d.score = 30.0;
        }
        alignment.mtf_momentum_alignment = -0.8; // momentum_dim = 10

        let divergence_label = |label: &str| {
            let mut ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
            ind.insert(
                "rsi".into(),
                NormalizedIndicatorValue {
                    raw_value: 50.0,
                    normalized: 0.0,
                    state_label: "NEUTRAL".into(),
                    values: None,
                    signals: vec![IndicatorSignal::new(
                        SignalKind::Divergence,
                        SignalDirection::Bullish,
                        SignalStatus::Confirmed,
                        label,
                    )
                    .with_strength(0.8)],
                    confidence: 0.8,
                },
            );
            ind
        };

        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &divergence_label("CONFIRMED_BULLISH_DIVERGENCE"),
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::Reversal);
        assert!(find_profile(&opp, OpportunityType::Reversal)
            .long_entry_zone
            .is_some());
        assert!(find_profile(&opp, OpportunityType::Reversal)
            .short_entry_zone
            .is_none());
        assert!(opp.invalidation_note.starts_with("A close below "));

        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &divergence_label("CONFIRMED_BEARISH_DIVERGENCE"),
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::Reversal);
        assert!(find_profile(&opp, OpportunityType::Reversal)
            .short_entry_zone
            .is_some());
        assert!(find_profile(&opp, OpportunityType::Reversal)
            .long_entry_zone
            .is_none());
        assert!(opp.invalidation_note.starts_with("A close above "));
    }

    // ── Invalidation note: direction awareness + level binding ──────────

    #[test]
    fn invalidation_note_suppressed_without_directional_thesis() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        // The regression this locks: a BREAKOUT primary under a NEUTRAL
        // bias. Breakout is TrendRiding, so with no directional bias the
        // profile carries NO zones — the frontend renders a NEUTRAL card
        // (aggregate fallback). The historical fallback chain then emitted
        // "Close below X invalidates the Breakout thesis" with a matrix
        // scalar level the card never displayed — a directional sentence
        // on a directionally-neutral setup. There is no thesis to
        // invalidate: the note must be empty.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Neutral;
        analysis.market_regime = MarketRegime::TrendingBull;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.mtf_volatility_alignment = 0.4; // vol_dim = 70
        if let Some(d) = alignment.dimensions.get_mut(4) {
            d.score = 65.0; // struct_dim = 65
        }

        let ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::Breakout);
        let p = find_profile(&opp, OpportunityType::Breakout);
        assert!(
            p.long_entry_zone.is_none() && p.short_entry_zone.is_none(),
            "TrendRiding under neutral bias must carry no directional zones"
        );
        assert_eq!(
            opp.invalidation_note,
            String::new(),
            "no directional thesis — the note must be suppressed, was: {}",
            opp.invalidation_note
        );
    }

    #[test]
    fn invalidation_note_level_binds_to_top_profile_side() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        // The level quoted in the note must be EXACTLY the level the top
        // qualifying profile's card displays (its per-side invalidation),
        // and the direction word must match that side.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Bearish;
        analysis.market_regime = MarketRegime::TrendingBear;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.mtf_volatility_alignment = 0.4; // vol_dim = 70
        if let Some(d) = alignment.dimensions.get_mut(4) {
            d.score = 65.0; // struct_dim = 65
        }

        let ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::Breakout);
        let p = find_profile(&opp, OpportunityType::Breakout);
        let short_inv = p
            .short_invalidation_level
            .expect("bearish TrendRiding must surface the SHORT invalidation");
        assert_eq!(
            opp.invalidation_note,
            format!(
                "A close above {:.1} on the completed candle invalidates the Breakout thesis.",
                short_inv
            ),
            "note must quote the top profile's own SHORT invalidation level"
        );
        assert_eq!(
            short_inv, opp.short_invalidation_level,
            "profile and matrix levels must be the same value (single binding)"
        );
        assert!(
            short_inv > 100.0,
            "SHORT invalidation must sit above close (stop semantics)"
        );
    }

    // ── v6.10.21 (NBR): neutral range reference bracket ──────────────────

    #[test]
    fn neutral_bracket_emitted_only_for_noclear_range() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        // NoClear + Range → the neutral reference bracket is emitted.
        // vol_dim = 50 (mtf_volatility_alignment 0.0 → 50) fails the
        // MeanReversion gate (≤ 30); nothing else qualifies at neutral
        // bias, so the primary falls through to NoClearOpportunity.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Neutral;
        analysis.market_regime = MarketRegime::Range;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.mtf_volatility_alignment = 0.0;

        let mut ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        let mut atr_values = HashMap::new();
        atr_values.insert("atr_14".into(), 1.0);
        ind.insert(
            "atr".into(),
            NormalizedIndicatorValue {
                raw_value: 1.0,
                normalized: 0.0,
                state_label: "NORMAL".into(),
                values: Some(atr_values),
                signals: vec![],
                confidence: 0.5,
            },
        );

        let close = 100.0;
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            close,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::NoClearOpportunity);
        let bracket = opp
            .neutral_reference_bracket
            .expect("neutral bracket must be emitted under NoClear + Range");
        assert!(bracket.geometry_consistent, "synthetic frame must be valid");
        assert!(
            bracket.invalidation_level < bracket.entry_zone.low,
            "SL must sit below the entry band (SL {} vs entry.low {})",
            bracket.invalidation_level,
            bracket.entry_zone.low
        );
        assert!(
            bracket.target_zone.low > bracket.entry_zone.high,
            "target must sit above the entry band (target.low {} vs entry.high {})",
            bracket.target_zone.low,
            bracket.entry_zone.high
        );
        assert!(
            bracket.expected_rr_internal > 0.0,
            "valid frame must carry R:R"
        );
        assert!(
            !bracket.rationale.is_empty(),
            "rationale must explain the frame origin"
        );
        assert_eq!(
            (bracket.entry_zone.low, bracket.entry_zone.high),
            (close - 0.2, close + 0.2),
            "entry band must center on close at ±0.2×ATR"
        );
    }

    #[test]
    fn neutral_bracket_absent_outside_noclear_range() {
        use core_domain::alignment::AlignmentMatrix;
        use core_domain::analysis::{AnalysisMatrix, MarketBias, MarketRegime, OpportunityType};
        use std::collections::HashMap;

        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 1;
        analysis.bias = MarketBias::Neutral;
        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.mtf_volatility_alignment = 0.0;
        let ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();

        // Range + vol_dim ≤ 30 → MeanReversion is the primary: no neutral
        // bracket (a real setup exists — the frame is only for NoClear).
        analysis.market_regime = MarketRegime::Range;
        alignment.mtf_volatility_alignment = -0.5;
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::MeanReversion);
        assert!(
            opp.neutral_reference_bracket.is_none(),
            "MeanReversion primary must not carry a neutral reference bracket"
        );

        // Trending + no candidates → NoClearOpportunity primary, but the
        // regime is NOT a range: no neutral bracket.
        analysis.market_regime = MarketRegime::TrendingBull;
        alignment.mtf_volatility_alignment = 0.0;
        let opp = compute_opportunity(
            &analysis,
            &alignment,
            &ind,
            None,
            None,
            100.0,
            &OpportunityParams::default(),
        )
        .expect("opportunity");
        assert_eq!(opp.primary_opportunity, OpportunityType::NoClearOpportunity);
        assert!(
            opp.neutral_reference_bracket.is_none(),
            "NoClear outside a range regime must not emit a neutral bracket"
        );
    }

    #[test]
    fn derive_neutral_bracket_guards_invalid_inputs() {
        use std::collections::HashMap;

        let ind: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        // Missing ATR falls back to close × 0.01 → still valid at close 100.
        assert!(
            derive_neutral_bracket(&ind, 100.0, &OpportunityParams::default()).is_some(),
            "ATR fallback must still produce a valid frame"
        );
        // Non-positive close → None.
        assert!(derive_neutral_bracket(&ind, 0.0, &OpportunityParams::default()).is_none());
        assert!(derive_neutral_bracket(&ind, -5.0, &OpportunityParams::default()).is_none());
        assert!(derive_neutral_bracket(&ind, f64::NAN, &OpportunityParams::default()).is_none());
    }

    /// AUDIT-AIU-126: value-level pin of the L4 viability blend
    /// (`0.35·Q + 0.30·S + 0.20·A + 0.15·F`, `02-08-opportunity-matrix.md`
    /// §6) through the REAL scoring path with synthetic-but-realistic
    /// inputs — the previous corpus only pinned the formula in prose.
    #[test]
    fn candidate_score_blend_matches_the_documented_weights() {
        use core_domain::analysis::{AnalysisMatrix, QualityLevel};
        use core_domain::indicator_dtos::{
            IndicatorSignal, SignalDirection, SignalKind, SignalStatus,
        };
        use std::collections::HashMap;

        // q = 100 (Excellent), s_sig = 80 (mean signal strength 0.8),
        // a_mtf = 75 (trend agreement %), f_fresh = 100 (min age 0).
        // Expected raw: 0.35×100 + 0.30×80 + 0.20×75 + 0.15×100 = 89.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.market_quality = QualityLevel::Excellent;

        let mut alignment = AlignmentMatrix::empty("BTC-USD");
        alignment.trend_agreement_pct = 75.0;

        let mut signals: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        signals.insert(
            "rsi".to_string(),
            NormalizedIndicatorValue {
                signals: vec![IndicatorSignal::new(
                    SignalKind::Threshold,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    "BULLISH_MOMENTUM",
                )],
                ..NormalizedIndicatorValue::scalar(60.0, 0.35, "BULLISH_MOMENTUM")
            },
        );
        signals.insert(
            "macd".to_string(),
            NormalizedIndicatorValue {
                signals: vec![IndicatorSignal::new(
                    SignalKind::Crossover,
                    SignalDirection::Bullish,
                    SignalStatus::Confirmed,
                    "BULLISH_CROSSOVER",
                )],
                ..NormalizedIndicatorValue::scalar(1.0, 0.8, "BULLISH_CROSSOVER")
            },
        );
        // Signal strengths 1.0 + 0.6 → mean 0.8 → s_sig = 80.
        signals.get_mut("rsi").unwrap().signals[0].strength = 1.0;
        signals.get_mut("macd").unwrap().signals[0].strength = 0.6;
        // age_bars: 0 (rsi) + 10 (macd) → min 0 → f_fresh = 100.
        signals.get_mut("rsi").unwrap().signals[0].age_bars = 0;
        signals.get_mut("macd").unwrap().signals[0].age_bars = 10;

        let (score, _, raw, _, _) = compute_candidate_score(
            OpportunityType::Scalp,
            &analysis,
            &alignment,
            &signals,
            3,
            3,
            &OpportunityParams::default(),
        );
        let expected = 0.35 * 100.0 + 0.30 * 80.0 + 0.20 * 75.0 + 0.15 * 100.0;
        assert!(
            (raw - expected).abs() < 1e-9,
            "raw viability blend must equal 0.35Q+0.30S+0.20A+0.15F = {expected}, got {raw}"
        );
        // Full precondition ratio → score == raw.
        assert!((score - raw).abs() < 1e-9);
    }

    /// AUDIT-AIU-126 (cont.): the QualityLevel → f64 mapping must match
    /// the canonical L6 fallback table (`02-04-decision-matrix.md` §2.3)
    /// and the NoClearOpportunity sentinel must stay unconditional zero.
    #[test]
    fn candidate_score_quality_mapping_and_sentinel() {
        use core_domain::analysis::{AnalysisMatrix, QualityLevel};
        use core_domain::indicator_dtos::{
            IndicatorSignal, SignalDirection, SignalKind, SignalStatus,
        };
        use std::collections::HashMap;

        let mapping = [
            (QualityLevel::Excellent, 100.0),
            (QualityLevel::Good, 70.0),
            (QualityLevel::Average, 55.0),
            (QualityLevel::Weak, 40.0),
            (QualityLevel::Poor, 20.0),
        ];
        for (ql, expected_q) in mapping {
            let mut analysis = AnalysisMatrix::empty("BTC-USD");
            analysis.market_quality = ql;
            let alignment = AlignmentMatrix::empty("BTC-USD");
            let signals: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
            let (score, _, raw, _, _) = compute_candidate_score(
                OpportunityType::TrendContinuation,
                &analysis,
                &alignment,
                &signals,
                0,
                3,
                &OpportunityParams::default(),
            );
            // No signals → s_sig default 40; no agreement → 0; no signals
            // → f_fresh uses the min_age fallback 10 → 100×(1−10/20) = 50.
            // raw = 0.35·Q + 0.30·40 + 0.20·0 + 0.15·50.
            let expected_raw = 0.35 * expected_q + 0.30 * 40.0 + 0.15 * 50.0;
            assert!(
                (raw - expected_raw).abs() < 1e-9,
                "QualityLevel {:?} must map to {} in the blend, got raw {}",
                ql,
                expected_q,
                raw
            );
            // Preconditions 0/3 → display_score muted while score stays
            // the raw viability (v6.10.1 contract).
            assert!((score - expected_raw).abs() < 1e-9);
        }

        // NoClearOpportunity → unconditional zero, regardless of inputs.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.market_quality = QualityLevel::Excellent;
        let alignment = AlignmentMatrix::empty("BTC-USD");
        let mut signals: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        signals.insert(
            "rsi".to_string(),
            NormalizedIndicatorValue {
                signals: vec![IndicatorSignal::new(
                    SignalKind::Threshold,
                    SignalDirection::Bullish,
                    SignalStatus::Confirmed,
                    "BULLISH_MOMENTUM",
                )],
                ..NormalizedIndicatorValue::scalar(60.0, 0.9, "BULLISH_MOMENTUM")
            },
        );
        let (score, _, raw, _, display) = compute_candidate_score(
            OpportunityType::NoClearOpportunity,
            &analysis,
            &alignment,
            &signals,
            3,
            3,
            &OpportunityParams::default(),
        );
        assert_eq!(score, 0.0, "NoClearOpportunity score must be 0");
        // The sentinel zeroes the PUBLISHED score/display; `raw` stays the
        // un-gated blend (the operator-facing zero is what matters).
        assert!(raw.is_finite() && raw >= 0.0);
        assert_eq!(display, 0.0, "NoClearOpportunity display must be 0");
    }
}
