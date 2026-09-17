//! # Risk Matrix — Risk Assessment Layer
//!
//! The Risk Matrix evaluates the level of uncertainty surrounding the current
//! market interpretation. Risk is a property of an interpretation, not of raw
//! observations. It consumes the Analysis Matrix — you cannot evaluate how
//! risky a bullish trend is without first determining that there IS a bullish
//! trend.
//!
//! Risk is independent from market direction. A bullish market can be high risk.
//! A bearish market can be low risk.
//!
//! Layer: L4.25 in the architecture (Risk Assessment).

use crate::analysis::AnalysisMatrix;
use crate::indicator_dtos::NormalizedIndicatorValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Risk level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiskLevel {
    VeryLow,
    Low,
    #[default]
    Moderate,
    High,
    Extreme,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::VeryLow => write!(f, "VERY_LOW"),
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Moderate => write!(f, "MODERATE"),
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Extreme => write!(f, "EXTREME"),
        }
    }
}

/// Risk dimension state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiskState {
    #[default]
    Stable,
    Increasing,
    Elevated,
    Critical,
    Improving,
}

impl std::fmt::Display for RiskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskState::Stable => write!(f, "STABLE"),
            RiskState::Increasing => write!(f, "INCREASING"),
            RiskState::Elevated => write!(f, "ELEVATED"),
            RiskState::Critical => write!(f, "CRITICAL"),
            RiskState::Improving => write!(f, "IMPROVING"),
        }
    }
}

/// One risk dimension with score, level, state, confidence, and evidence.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RiskDimension {
    /// Risk score 0-100 (higher = riskier).
    pub score: f64,
    /// Risk level.
    pub level: RiskLevel,
    /// Risk state (trend).
    pub state: RiskState,
    /// Confidence in this measurement 0-100%.
    pub confidence: f64,
    /// Supporting evidence strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// ATR(14) ÷ top-of-book bid-ask spread (raw price units) — execution
    /// friction gauge. Populated only on `execution_risk`; other dimensions
    /// leave it `None` (absent from the wire).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volatility_to_spread_ratio: Option<f64>,
}

impl RiskDimension {
    /// Build a `RiskDimension` from a raw 0-100 score using the
    /// maximally-uncertain default confidence (50%). Public API used by
    /// downstream crates (e.g. `DecisionContext::compute`) that need to
    /// produce a `RiskDimension` shape from a single scalar without access
    /// to the upstream L3 state_confidence.
    pub fn from_score(score: f64) -> Self {
        Self::from_score_with_confidence(score, 0.5)
    }

    /// Build a `RiskDimension` from a raw 0-100 score and a propagation
    /// confidence in `[0.0, 1.0]`. The resulting `confidence` field is
    /// `state_confidence * 100` (matches L3→L6 propagation per docs/matrices
    /// `02-00b-confidence-hierarchy.md`). Pass `state_confidence = 0.0`
    /// to override to "0" (e.g. `cascade_risk` when liquidity feed is OFF).
    /// Public so callers (e.g. `DecisionContext::compute`) can wire the L3
    /// `state_confidence` through to the RiskDimension's `confidence` field.
    pub fn from_score_with_confidence(score: f64, state_confidence: f64) -> Self {
        let level = if score >= 80.0 {
            RiskLevel::Extreme
        } else if score >= 60.0 {
            RiskLevel::High
        } else if score >= 40.0 {
            RiskLevel::Moderate
        } else if score >= 20.0 {
            RiskLevel::Low
        } else {
            RiskLevel::VeryLow
        };
        let confidence = (state_confidence * 100.0).clamp(0.0, 100.0);
        Self {
            score: score.clamp(0.0, 100.0),
            level,
            state: RiskState::Stable,
            confidence,
            evidence: Vec::new(),
            volatility_to_spread_ratio: None,
        }
    }

    fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    fn with_volatility_to_spread_ratio(mut self, ratio: Option<f64>) -> Self {
        self.volatility_to_spread_ratio = ratio;
        self
    }

    /// Override the risk state (trend descriptor). `from_score_with_confidence`
    /// defaults to `Stable`; `compute_risk` applies the derived state via this
    /// builder (v6.10.9 — the state is now functional).
    pub fn with_state(mut self, state: RiskState) -> Self {
        self.state = state;
        self
    }
}

/// Derive the risk state (trend descriptor) from the current score and the
/// previous synthesis reference (v6.10.9).
///
/// `RiskState` was previously dead code — `from_score_with_confidence`
/// hardcoded `Stable` and no producer ever emitted another variant, so the
/// panel's state pills and the L5 header sublabel permanently read "STABLE".
/// The state is now functional:
///
///   1. **Level escalation** — a score already in the danger zone is its own
///      state: `≥ 80` → `Critical`, `≥ 60` → `Elevated`.
///   2. **Trend from the previous synthesis** — otherwise the delta against
///      `previous_overall` (the pipeline's previous L2 mtf overall score,
///      normalized to the 0-100 risk scale): `> +10` → `Increasing`,
///      `< −10` → `Improving`, else `Stable`.
///
/// The state is **descriptive only** — it never feeds back into the weighted
/// sum (`overall_risk` remains a plain weighted aggregate per 02-11 §3).
pub fn derive_risk_state(score: f64, previous_overall: Option<f64>) -> RiskState {
    if score >= 80.0 {
        RiskState::Critical
    } else if score >= 60.0 {
        RiskState::Elevated
    } else {
        match previous_overall {
            Some(prev) if score > prev + 10.0 => RiskState::Increasing,
            Some(prev) if score < prev - 10.0 => RiskState::Improving,
            _ => RiskState::Stable,
        }
    }
}

/// Market risk assessment for a single symbol — 9 dimensions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RiskMatrix {
    pub symbol: String,
    pub market_risk: RiskDimension,
    pub volatility_risk: RiskDimension,
    /// Phase 3 rename: was `liquidity_risk` (execution-liquidity / slippage).
    /// Now: execution liquidity / market depth.
    #[serde(rename = "execution_liquidity_risk")]
    pub execution_liquidity_risk: RiskDimension,
    pub structure_risk: RiskDimension,
    pub momentum_risk: RiskDimension,
    pub signal_risk: RiskDimension,
    pub execution_risk: RiskDimension,
    /// Phase 3: cascade risk — danger from forced liquidation cascades.
    #[serde(default)]
    pub cascade_risk: RiskDimension,
    pub overall_risk: RiskDimension,
}

impl RiskMatrix {
    pub fn empty(symbol: &str) -> Self {
        let def = RiskDimension::from_score(50.0);
        Self {
            symbol: symbol.to_string(),
            market_risk: def.clone(),
            volatility_risk: def.clone(),
            execution_liquidity_risk: def.clone(),
            structure_risk: def.clone(),
            momentum_risk: def.clone(),
            signal_risk: def.clone(),
            execution_risk: def.clone(),
            cascade_risk: def.clone(),
            overall_risk: def.clone(),
        }
    }
}

/// Level from score 0-100.
#[allow(dead_code)]
fn level_from(score: f64) -> RiskLevel {
    if score >= 80.0 {
        RiskLevel::Extreme
    } else if score >= 60.0 {
        RiskLevel::High
    } else if score >= 40.0 {
        RiskLevel::Moderate
    } else if score >= 20.0 {
        RiskLevel::Low
    } else {
        RiskLevel::VeryLow
    }
}

/// Score from magnitude: maps raw values to 0-100 risk score.
fn score_mag(value: f64, max: f64) -> f64 {
    (value / max * 100.0).clamp(0.0, 100.0)
}

/// v9: the L5 runtime parameters — the strategy's `l5` section. Every
/// default reproduces the pre-v9 baselines, additive tiers, band borders,
/// and the overall-weight vector.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskParams {
    pub weights_market: f64,
    pub weights_volatility: f64,
    pub weights_execution_liquidity: f64,
    pub weights_structure: f64,
    pub weights_momentum: f64,
    pub weights_signal: f64,
    pub weights_execution: f64,
    pub weights_cascade: f64,
    pub bands: [f64; 4],
    pub state_delta: f64,
    pub market: RiskMarketParams,
    pub volatility: RiskVolatilityParams,
    pub execution_liquidity: RiskExecLiquidityParams,
    pub structure: RiskStructureParams,
    pub momentum: RiskMomentumParams,
    pub signal: RiskSignalParams,
    pub execution: RiskExecutionParams,
    pub cascade: RiskCascadeParams,
    /// v9 (strategy `l1_5.signal_weights`): the 11-kind trust axis.
    /// Multiplies each discrete-signal bonus (OI-price divergence,
    /// funding flip). Empty = all 1.0 (v8.2 behavior).
    pub signal_weights: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskMarketParams {
    pub baseline: f64,
    pub weak_trend: f64,
    pub broken_structure: f64,
    pub poor_quality: f64,
    pub low_conf_max: f64,
    pub low_conf: f64,
    pub contradicting: f64,
    pub strong_trend: f64,
    pub high_conf_min: f64,
    pub high_conf: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskVolatilityParams {
    pub baseline: f64,
    pub bbwp_extreme: f64,
    pub bbwp_extreme_add: f64,
    pub bbwp_elevated: f64,
    pub bbwp_elevated_add: f64,
    pub squeeze_add: f64,
    pub micro_fast_blend: [f64; 2],
    pub atr_pct_floor: f64,
    pub atr_pct_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskExecLiquidityParams {
    pub baseline: f64,
    pub rvol_very_low: f64,
    pub rvol_very_low_add: f64,
    pub rvol_low: f64,
    pub rvol_low_add: f64,
    pub rvol_high: f64,
    pub rvol_high_add: f64,
    pub spread_wide: f64,
    pub spread_wide_add: f64,
    pub spread_tight: f64,
    pub spread_tight_add: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskStructureParams {
    pub baseline: f64,
    pub broken: f64,
    pub weak: f64,
    pub healthy: f64,
    pub flip: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskMomentumParams {
    pub baseline: f64,
    pub exhausted: f64,
    pub reversing: f64,
    pub weakening: f64,
    pub increasing: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskSignalParams {
    pub baseline: f64,
    pub per_contradicting: f64,
    pub contradicting_cap: f64,
    pub none_active: f64,
    pub low_conf_max: f64,
    pub low_conf: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskExecutionParams {
    pub baseline: f64,
    pub spread_wide: f64,
    pub spread_wide_add: f64,
    pub spread_moderate: f64,
    pub spread_moderate_add: f64,
    pub rvol_low: f64,
    pub rvol_add: f64,
    /// (max, add) tiers ascending + the favorable (min, add) tier.
    pub ratio_tiers: Vec<(Option<f64>, Option<f64>, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskCascadeParams {
    pub baseline: f64,
    pub sustained: f64,
    pub detected: f64,
    pub asymmetry_min: f64,
    pub asymmetry_scale: f64,
    pub oi_divergence_max: f64,
    pub funding_flip_max: f64,
}

impl Default for RiskParams {
    fn default() -> Self {
        Self {
            weights_market: 0.14,
            weights_volatility: 0.14,
            weights_execution_liquidity: 0.14,
            weights_structure: 0.10,
            weights_momentum: 0.14,
            weights_signal: 0.10,
            weights_execution: 0.10,
            weights_cascade: 0.14,
            bands: [80.0, 60.0, 40.0, 20.0],
            state_delta: 10.0,
            market: RiskMarketParams {
                baseline: 50.0,
                weak_trend: 15.0,
                broken_structure: 15.0,
                poor_quality: 10.0,
                low_conf_max: 0.4,
                low_conf: 10.0,
                contradicting: 10.0,
                strong_trend: -10.0,
                high_conf_min: 0.7,
                high_conf: -10.0,
            },
            volatility: RiskVolatilityParams {
                baseline: 30.0,
                bbwp_extreme: 90.0,
                bbwp_extreme_add: 30.0,
                bbwp_elevated: 70.0,
                bbwp_elevated_add: 15.0,
                squeeze_add: 10.0,
                micro_fast_blend: [0.7, 0.3],
                atr_pct_floor: 1.0,
                atr_pct_max: 5.0,
            },
            execution_liquidity: RiskExecLiquidityParams {
                baseline: 20.0,
                rvol_very_low: 0.4,
                rvol_very_low_add: 30.0,
                rvol_low: 1.0,
                rvol_low_add: 15.0,
                rvol_high: 5.0,
                rvol_high_add: -15.0,
                spread_wide: 0.2,
                spread_wide_add: 20.0,
                spread_tight: 0.05,
                spread_tight_add: -10.0,
            },
            structure: RiskStructureParams {
                baseline: 40.0,
                broken: 30.0,
                weak: 15.0,
                healthy: -15.0,
                flip: 15.0,
            },
            momentum: RiskMomentumParams {
                baseline: 30.0,
                exhausted: 40.0,
                reversing: 30.0,
                weakening: 15.0,
                increasing: -10.0,
            },
            signal: RiskSignalParams {
                baseline: 30.0,
                per_contradicting: 10.0,
                contradicting_cap: 40.0,
                none_active: 10.0,
                low_conf_max: 0.5,
                low_conf: 15.0,
            },
            execution: RiskExecutionParams {
                baseline: 25.0,
                spread_wide: 0.15,
                spread_wide_add: 25.0,
                spread_moderate: 0.08,
                spread_moderate_add: 10.0,
                rvol_low: 0.7,
                rvol_add: 15.0,
                ratio_tiers: vec![
                    (Some(1.5), None, 15.0),
                    (Some(3.0), None, 5.0),
                    (None, Some(10.0), -5.0),
                ],
            },
            cascade: RiskCascadeParams {
                baseline: 30.0,
                sustained: 30.0,
                detected: 15.0,
                asymmetry_min: 0.3,
                asymmetry_scale: 30.0,
                oi_divergence_max: 15.0,
                funding_flip_max: 10.0,
            },
            signal_weights: std::collections::HashMap::new(),
        }
    }
}

impl RiskParams {
    pub fn level_label(&self, score: f64) -> &'static str {
        let [extreme, high, moderate, low] = self.bands;
        if score >= extreme {
            "Extreme"
        } else if score >= high {
            "High"
        } else if score >= moderate {
            "Moderate"
        } else if score >= low {
            "Low"
        } else {
            "VeryLow"
        }
    }
}

/// Assess market risk: general uncertainty from conflicting signals, weak structure.
fn assess_market_risk(
    analysis: &AnalysisMatrix,
    _indicators: &HashMap<String, NormalizedIndicatorValue>,
    p: &RiskMarketParams,
) -> RiskDimension {
    let mut score: f64 = p.baseline;
    let mut evidence = Vec::new();
    if analysis.trend_assessment == crate::analysis::TrendAssessment::Weak {
        score += p.weak_trend;
        evidence.push("Weak trend".into());
    }
    if analysis.structure_assessment == crate::analysis::StructureAssessment::Broken {
        score += p.broken_structure;
        evidence.push("Broken structure".into());
    }
    if analysis.market_quality == crate::analysis::QualityLevel::Poor {
        score += p.poor_quality;
        evidence.push("Poor market quality".into());
    }
    if analysis.state_confidence < p.low_conf_max {
        score += p.low_conf;
        evidence.push("Low confidence".into());
    }
    if !analysis.contradicting_signals.is_empty() {
        score += p.contradicting;
        evidence.push("Conflicting signals".into());
    }
    // Reduce risk if conditions are good
    if analysis.trend_assessment == crate::analysis::TrendAssessment::Strong {
        score += p.strong_trend;
    }
    if analysis.state_confidence > p.high_conf_min {
        score += p.high_conf;
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess volatility risk: danger from abnormal price movement.
///
/// v6.10.18 (I-8): the dimension now integrates the ACTIONABLE timeframe
/// volatility STATE — `tf_volatility` carries the per-window L2 volatility
/// dimension `(label, state_label, score)` pairs (scores on the 0–100
/// unipolar scale). The legacy BBWP + relative-ATR formula said LOW (23)
/// while the micro window was EXPANDING and the fast window
/// EXPANSION_CLIMAX — evidence and score contradicted each other and the
/// operator. The fast-weighted state (micro 0.7 / fast 0.3 — the horizons
/// a scalp/intraday operator actually trades) is blended with the BBWP
/// component; the relative-ATR term only modulates when it is meaningful
/// (≥1% of price) and never drags a BTC-scale sub-0.1% print to zero.
fn assess_volatility_risk(
    analysis: &AnalysisMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    close: f64,
    tf_volatility: &[(String, String, f64)],
    p: &RiskVolatilityParams,
) -> RiskDimension {
    let bbwp = indicators.get("bbwp").map(|v| v.raw_value).unwrap_or(50.0);
    let atr = indicators.get("atr").map(|v| v.raw_value).unwrap_or(0.0);
    let squeeze_on = indicators
        .get("squeeze")
        .map(|v| v.state_label.contains("COMPRESSION"))
        .unwrap_or(false);
    let mut evidence = Vec::new();
    let mut score: f64 = p.baseline;
    if bbwp >= p.bbwp_extreme {
        score += p.bbwp_extreme_add;
        evidence.push("BBWP extreme expansion".into());
    } else if bbwp >= p.bbwp_elevated {
        score += p.bbwp_elevated_add;
        evidence.push("BBWP elevated".into());
    } else if squeeze_on {
        score += p.squeeze_add;
        evidence.push("Squeeze compression active".into());
    }
    // v6.10.18 (I-8): the fast-weighted TF volatility state.
    // AUDIT-AIU-109: pair the micro/fast states BY DURATION, not by list
    // position. `tf_volatility` arrives in `tf_data` order, which is
    // current-pipeline-first — on the fast/slow/macro pipelines `.first()`
    // was the pipeline itself, so the documented `0.7 × micro + 0.3 × fast`
    // blend was silently computed as `0.7 × self + 0.3 × micro` on 3 of 4
    // pipelines (corrupting `volatility_risk` and its downstream guidance
    // on those snapshots). v11.9: labels are duration labels ("1s"…"1d"),
    // so the pairing sorts by parsed duration and reads the two FASTEST
    // windows (fastest = micro leg, second-fastest = fast leg) — stable
    // for any ACTIVE subset.
    let mut vol_component: Option<f64> = None;
    if !tf_volatility.is_empty() {
        for (label, state, _) in tf_volatility {
            evidence.push(format!("{} volatility {}", label, state));
        }
        let mut by_duration: Vec<(u64, f64)> = tf_volatility
            .iter()
            .map(|(label, _, s)| (crate::duration_from_label(label).unwrap_or(u64::MAX), *s))
            .collect();
        by_duration.sort_by_key(|(secs, _)| *secs);
        let micro = by_duration.first().map(|(_, s)| *s);
        let fast = by_duration.get(1).map(|(_, s)| *s);
        let (micro, fast) = match (micro, fast) {
            // Defensive fallback: no parseable labels (degenerate snapshot).
            (None, None) => (
                tf_volatility.first().map(|(_, _, s)| *s),
                tf_volatility.get(1).map(|(_, _, s)| *s),
            ),
            other => other,
        };
        vol_component = match (micro, fast) {
            (Some(m), Some(f)) => Some(p.micro_fast_blend[0] * m + p.micro_fast_blend[1] * f),
            (Some(m), None) => Some(m),
            (None, Some(f)) => Some(f),
            (None, None) => None,
        };
    }
    if let Some(vc) = vol_component {
        score = (score + vc) / 2.0;
    }
    if atr > 0.0 && close > 0.0 {
        // Relative ATR (ATR as % of close). The legacy absolute-ATR
        // formula `score_mag(atr, 500.0)` saturated for any high-TF BTC
        // candle (1d ATR ≈ $1000, 1w ATR ≈ $5000–$15000) — `volatility_risk`
        // hit 100 even on quiet 1w prints, which broke the L5→L6
        // discount and the dashboard's risk gauge for HTF symbols.
        // Relative ATR is price-normalized: 0% → 0 risk, 5% → 100 risk.
        // v6.10.18: only modulates when meaningful (≥1%), so it can no
        // longer drag a sub-0.1% BTC print to LOW beside a climax state.
        let atr_pct = (atr / close) * 100.0;
        if atr_pct >= p.atr_pct_floor {
            let rel_atr = score_mag(atr_pct, p.atr_pct_max);
            score = (score + rel_atr) / 2.0;
        }
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess liquidity risk: quality of market participation.
fn assess_execution_liquidity_risk(
    analysis: &AnalysisMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    p: &RiskExecLiquidityParams,
) -> RiskDimension {
    let rvol = indicators.get("rvol").map(|v| v.raw_value).unwrap_or(1.0);
    let spread = indicators.get("spread").map(|v| v.raw_value).unwrap_or(0.0);
    let mut evidence = Vec::new();
    let mut score: f64 = p.baseline;
    if rvol < p.rvol_very_low {
        score += p.rvol_very_low_add;
        evidence.push("Very low relative volume".into());
    } else if rvol < p.rvol_low {
        score += p.rvol_low_add;
        evidence.push("Low relative volume".into());
    } else if rvol > p.rvol_high {
        score += p.rvol_high_add;
        evidence.push("Strong participation".into());
    }
    if spread > p.spread_wide {
        score += p.spread_wide_add;
        evidence.push("Wide spread".into());
    } else if spread < p.spread_tight {
        score += p.spread_tight_add;
        evidence.push("Tight spread".into());
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess structure risk: uncertainty from weak/damaged price structure.
fn assess_structure_risk(
    analysis: &AnalysisMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    p: &RiskStructureParams,
) -> RiskDimension {
    let mut score: f64 = p.baseline;
    let mut evidence = Vec::new();
    match analysis.structure_assessment {
        crate::analysis::StructureAssessment::Broken => {
            score += p.broken;
            evidence.push("Broken structure".into());
        }
        crate::analysis::StructureAssessment::Weak => {
            score += p.weak;
            evidence.push("Weak structure".into());
        }
        crate::analysis::StructureAssessment::Strong
        | crate::analysis::StructureAssessment::Healthy => {
            score += p.healthy;
        }
        _ => {}
    }
    let sr_label = indicators
        .get("support_resistance")
        .map(|v| v.state_label.as_str())
        .unwrap_or("");
    if sr_label.contains("FLIP") {
        score += p.flip;
        evidence.push("S/R level flip".into());
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess momentum risk: vulnerability from exhausted/diverging momentum.
fn assess_momentum_risk(analysis: &AnalysisMatrix, p: &RiskMomentumParams) -> RiskDimension {
    let mut score: f64 = p.baseline;
    let mut evidence = Vec::new();
    match analysis.momentum_assessment {
        crate::analysis::MomentumAssessment::Exhausted => {
            score += p.exhausted;
            evidence.push("Momentum exhausted".into());
        }
        crate::analysis::MomentumAssessment::Reversing => {
            score += p.reversing;
            evidence.push("Momentum reversing".into());
        }
        crate::analysis::MomentumAssessment::Weakening => {
            score += p.weakening;
            evidence.push("Momentum weakening".into());
        }
        crate::analysis::MomentumAssessment::Increasing => {
            score += p.increasing;
            evidence.push("Momentum increasing".into());
        }
        _ => {}
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess signal risk: uncertainty from conflicting/unreliable signals.
fn assess_signal_risk(analysis: &AnalysisMatrix, p: &RiskSignalParams) -> RiskDimension {
    let mut score: f64 = p.baseline;
    let mut evidence = Vec::new();
    if !analysis.contradicting_signals.is_empty() {
        let n = analysis.contradicting_signals.len() as f64;
        score += (n * p.per_contradicting).min(p.contradicting_cap);
        evidence.push(format!(
            "{} contradicting signals",
            analysis.contradicting_signals.len()
        ));
    }
    if analysis.supporting_signals.is_empty() && analysis.contradicting_signals.is_empty() {
        score += p.none_active;
        evidence.push("No signals active".into());
    }
    if analysis.state_confidence < p.low_conf_max {
        score += p.low_conf;
        evidence.push("Low analysis confidence".into());
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
}

/// Assess execution risk: practical difficulties from spread/movement.
///
/// Also computes the L5 `volatility_to_spread_ratio` (ATR-14 ÷ top-of-book
/// spread, raw price units): a high ratio means the average candle range
/// dwarfs transaction cost (favorable for scalping); a low ratio means
/// spread friction and slippage consume potential profits. Scoring rules
/// (additive, baseline 25):
///   +15 ratio < 1.5  — spread friction dominates the environment
///   +5  ratio < 3.0  — moderate friction
///   −5  ratio > 10.0 — range-to-cost favorable (execution-friendly)
/// The ratio is `None` (absent) when ATR or spread is unavailable/zero.
///
/// v6.10.21 unit fix: the `spread` indicator's `raw_value` is a
/// PERCENTAGE of mid price (`(best_ask − best_bid) / mid × 100`). The
/// ratio formula requires the spread in raw price units, so the percentage
/// is converted via the close price (`spread / 100 × close`) before
/// dividing ATR. The legacy behavior divided ATR by the percentage scalar
/// directly — e.g. a real BTC-USDC spread of 0.000568 % produced a
/// meaningless ratio of ~2659 instead of ~4.2.
fn assess_execution_risk(
    analysis: &AnalysisMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    close: f64,
    p: &RiskExecutionParams,
) -> RiskDimension {
    let spread = indicators.get("spread").map(|v| v.raw_value).unwrap_or(0.0);
    let rvol = indicators.get("rvol").map(|v| v.raw_value).unwrap_or(1.0);
    let atr_14 = indicators
        .get("atr")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("atr_14").copied())
        .unwrap_or(0.0);
    // Spread in raw price units: percentage → `spread / 100 × close`.
    let spread_price = if close > 0.0 {
        spread / 100.0 * close
    } else {
        0.0
    };
    let volatility_to_spread_ratio = if spread_price > 1e-9 && atr_14 > 0.0 {
        Some(atr_14 / spread_price)
    } else {
        None
    };
    let mut evidence = Vec::new();
    let mut score: f64 = p.baseline;
    if spread > p.spread_wide {
        score += p.spread_wide_add;
        evidence.push("Wide spread".into());
    } else if spread > p.spread_moderate {
        score += p.spread_moderate_add;
        evidence.push("Moderate spread".into());
    }
    if rvol < p.rvol_low {
        score += p.rvol_add;
        evidence.push("Low participation".into());
    }
    if let Some(ratio) = volatility_to_spread_ratio {
        for (idx, (max, min, add)) in p.ratio_tiers.iter().enumerate() {
            let hit = match (max, min) {
                (Some(m), None) => ratio < *m,
                (None, Some(m)) => ratio > *m,
                _ => false,
            };
            if hit {
                score += *add;
                let label = match idx {
                    0 => "Low volatility-to-spread",
                    1 => "Moderate volatility-to-spread",
                    _ => "Favorable volatility-to-spread",
                };
                evidence.push(format!("{label} ({:.1})", ratio));
                break;
            }
        }
    }
    RiskDimension::from_score_with_confidence(score.clamp(0.0, 100.0), analysis.state_confidence)
        .with_evidence(evidence)
        .with_volatility_to_spread_ratio(volatility_to_spread_ratio)
}

/// Assess cascade risk (Phase 3): danger from forced liquidation
/// cascades. Reads `cascade_intensity` from the per-candle
/// `LiquidityFlow` if present, plus `cascade_asymmetry` from the
/// `LiquidationClusterMatrix` for forward-looking pressure. Higher
/// intensity or larger absolute asymmetry → higher risk.
///
/// Per docs/engines `03-02-12-mme-configurable-activation.md` §CA-15,
/// `cascade_risk` emits `confidence: 0` when the liquidity data feed
/// is off (no `flow` and no `cluster`), since the dimension has no
/// underlying measurement to be confident about.
fn assess_cascade_risk(
    analysis: &AnalysisMatrix,
    flow: Option<&crate::liquidity::LiquidityFlow>,
    cluster: Option<&crate::liquidity::LiquidationClusterMatrix>,
    // AUDIT-AIU-062: the discrete liquidity signals are now consumed here
    // (previously computed and broadcast but unused by any downstream
    // layer). OI-Price divergence and funding flips are positioning-stress
    // tell-tales that belong in the cascade dimension.
    liquidity_signals: &[crate::liquidity::LiquiditySignal],
    p: &RiskCascadeParams,
    signal_weights: &std::collections::HashMap<String, f64>,
) -> RiskDimension {
    let mut score: f64 = p.baseline; // baseline
    let mut evidence = Vec::new();
    // Confidence override: if the liquidity data feed is off, there is no
    // measurement for this dimension → confidence 0 (docs §CA-15).
    let state_confidence = if flow.is_none() && cluster.is_none() {
        0.0
    } else {
        analysis.state_confidence
    };
    if let Some(f) = flow {
        // The intensity is already 0..100; pull it in proportionally.
        score = score.max(f.cascade_intensity);
        match f.cascade_state {
            crate::liquidity::CascadeState::Sustained => {
                score = (score + p.sustained).min(100.0);
                evidence.push("Cascade sustained in rolling window".into());
            }
            crate::liquidity::CascadeState::Detected => {
                score = (score + p.detected).min(100.0);
                evidence.push("Cascade detected this bar".into());
            }
            crate::liquidity::CascadeState::Exhausted => {
                evidence.push("Cascade exhausted (decaying)".into());
            }
            _ => {}
        }
    }
    if let Some(c) = cluster {
        // Forward-looking pressure: |asymmetry| contributes up to 20.
        let asym = c.cascade_asymmetry.abs();
        if asym > p.asymmetry_min {
            score = (score + asym * p.asymmetry_scale).min(100.0);
            evidence.push(format!(
                "Cluster asymmetry {:.2} (significant one-sided pressure)",
                c.cascade_asymmetry
            ));
        }
    }
    // AUDIT-AIU-062: discrete-signal bonuses — OI-price divergence and
    // funding flips are positioning-stress tell-tales. Strength is 0..100;
    // scale the contribution to ≤ 15 points.
    for sig in liquidity_signals {
        // v9: the strategy's `l1_5.signal_weights` trust axis multiplies
        // each kind's bonus (default 1.0 = v8.2 behavior).
        let kind_weight = signal_weights
            .get(&format!("{:?}", sig.kind))
            .copied()
            .unwrap_or(1.0);
        match sig.kind {
            crate::liquidity::LiquiditySignalKind::OiPriceDivergence => {
                let bonus = (sig.strength / 100.0 * p.oi_divergence_max).min(p.oi_divergence_max)
                    * kind_weight;
                score = (score + bonus).min(100.0);
                evidence.push("OI-price divergence (positioning stress)".into());
            }
            crate::liquidity::LiquiditySignalKind::FundingFlip => {
                let bonus = (sig.strength / 100.0 * p.funding_flip_max).min(p.funding_flip_max)
                    * kind_weight;
                score = (score + bonus).min(100.0);
                evidence.push("Funding rate flipped (crowd positioning stress)".into());
            }
            _ => {}
        }
    }
    RiskDimension::from_score_with_confidence(score, state_confidence).with_evidence(evidence)
}

/// Compute the Risk Matrix from the Analysis Matrix and per-timeframe indicators.
///
/// `previous_overall` is the pipeline's previous L2 mtf overall score
/// (signed −100..+100), used as the risk-trend reference for the derived
/// `RiskState` (v6.10.9). It is normalized to the 0-100 unipolar risk
/// scale before the delta comparison; `None` (first synthesis) yields
/// `Stable` for the trend arm.
#[allow(clippy::too_many_arguments)]
pub fn compute_risk(
    symbol: &str,
    analysis: &AnalysisMatrix,
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    flow: Option<&crate::liquidity::LiquidityFlow>,
    cluster: Option<&crate::liquidity::LiquidationClusterMatrix>,
    close: f64,
    // AUDIT-AIU-062: discrete liquidity signals feed the cascade dimension.
    liquidity_signals: &[crate::liquidity::LiquiditySignal],
    previous_overall: Option<f64>,
    // v6.10.18 (I-8): per-TF L2 volatility states `(label, state_label,
    // score_0_100)` — the actionable-horizon volatility signal for the
    // volatility-risk dimension. Empty for legacy callers (warmup).
    tf_volatility: &[(String, String, f64)],
    // v9: the strategy's `l5` section.
    params: &RiskParams,
) -> RiskMatrix {
    if analysis.timeframes_considered == 0 {
        return RiskMatrix::empty(symbol);
    }

    let market = assess_market_risk(analysis, indicators, &params.market);
    let volatility = assess_volatility_risk(
        analysis,
        indicators,
        close,
        tf_volatility,
        &params.volatility,
    );
    let liquidity =
        assess_execution_liquidity_risk(analysis, indicators, &params.execution_liquidity);
    let structure = assess_structure_risk(analysis, indicators, &params.structure);
    let momentum = assess_momentum_risk(analysis, &params.momentum);
    let signal = assess_signal_risk(analysis, &params.signal);
    let execution = assess_execution_risk(analysis, indicators, close, &params.execution);
    let cascade = assess_cascade_risk(
        analysis,
        flow,
        cluster,
        liquidity_signals,
        &params.cascade,
        &params.signal_weights,
    );

    // v6.10 (Phase 1 / A6): Risk weights restored to the canonical spec table
    // at `docs/matrices/02-11-risk-matrix.md §3`. The previous v6.9 weights
    // put cascade at 0.11 (vs spec 0.14), under-weighting the cascade
    // dimension by ~21% relative to spec. We restore the spec weighting:
    //
    //   market × 0.14
    //   volatility × 0.14
    //   liquidity × 0.14
    //   structure × 0.10
    //   momentum × 0.14
    //   signal × 0.10
    //   execution × 0.10
    //   cascade × 0.14
    //
    // Sum = 5×0.14 + 3×0.10 = 0.70 + 0.30 = 1.00.
    let overall_score = (market.score * params.weights_market
        + volatility.score * params.weights_volatility
        + liquidity.score * params.weights_execution_liquidity
        + structure.score * params.weights_structure
        + momentum.score * params.weights_momentum
        + signal.score * params.weights_signal
        + execution.score * params.weights_execution
        + cascade.score * params.weights_cascade)
        .clamp(0.0, 100.0);
    // v6.10.9: functional risk state. The overall state derives from the
    // level + the previous-synthesis delta; each dimension escalates by its
    // own level (≥60 Elevated, ≥80 Critical) and otherwise inherits the
    // market risk trend (the overall state).
    let previous_risk_ref = previous_overall.map(|p| ((p + 100.0) / 2.0).clamp(0.0, 100.0));
    let overall_state = derive_risk_state(overall_score, previous_risk_ref);
    let dim_state = |score: f64| {
        if score >= 80.0 {
            RiskState::Critical
        } else if score >= 60.0 {
            RiskState::Elevated
        } else {
            overall_state
        }
    };
    let market_score = market.score;
    let volatility_score = volatility.score;
    let liquidity_score = liquidity.score;
    let structure_score = structure.score;
    let momentum_score = momentum.score;
    let signal_score = signal.score;
    let execution_score = execution.score;
    let cascade_score = cascade.score;
    let overall =
        RiskDimension::from_score_with_confidence(overall_score, analysis.state_confidence)
            .with_state(overall_state);
    let market = market.with_state(dim_state(market_score));
    let volatility = volatility.with_state(dim_state(volatility_score));
    let liquidity = liquidity.with_state(dim_state(liquidity_score));
    let structure = structure.with_state(dim_state(structure_score));
    let momentum = momentum.with_state(dim_state(momentum_score));
    let signal = signal.with_state(dim_state(signal_score));
    let execution = execution.with_state(dim_state(execution_score));
    let cascade = cascade.with_state(dim_state(cascade_score));

    RiskMatrix {
        symbol: symbol.to_string(),
        market_risk: market,
        volatility_risk: volatility,
        execution_liquidity_risk: liquidity,
        structure_risk: structure,
        momentum_risk: momentum,
        signal_risk: signal,
        execution_risk: execution,
        cascade_risk: cascade,
        overall_risk: overall,
    }
}

#[allow(dead_code)]
fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        AnalysisMatrix, MarketBias, MarketPhase, MarketRegime, MomentumAssessment, QualityLevel,
        StructureAssessment, TrendAssessment, VolatilityAssessment, VolumeAssessment,
    };

    fn make_analysis_with_timeframes() -> AnalysisMatrix {
        AnalysisMatrix {
            symbol: "BTC-USD".to_string(),
            bias: MarketBias::Neutral,
            market_bias_score: 0.0,
            state_confidence: 0.5,
            confidence: 0.5,
            market_quality_score: 50.0,
            trend_score: None,
            momentum_score: None,
            structure_score: None,
            volatility_score: None,
            volume_score: None,
            representative_bbwp: None,
            representative_adx: None,
            market_regime: MarketRegime::Range,
            trend_assessment: TrendAssessment::Healthy,
            momentum_assessment: MomentumAssessment::Stable,
            structure_assessment: StructureAssessment::Healthy,
            volatility_assessment: VolatilityAssessment::Normal,
            volume_assessment: VolumeAssessment::Normal,
            market_quality: QualityLevel::Average,
            market_phase: MarketPhase::Unknown,
            market_interpretation: "Test".into(),
            rationale: String::new(),
            supporting_signals: Vec::new(),
            contradicting_signals: Vec::new(),
            timeframes_considered: 4,
        }
    }

    #[test]
    fn empty_returns_default() {
        let r = RiskMatrix::empty("BTC-USD");
        assert!(matches!(r.overall_risk.level, RiskLevel::Moderate));
    }

    #[test]
    fn compute_with_analysis_produces_valid_dimensions() {
        let analysis = make_analysis_with_timeframes();
        let indicators = HashMap::new();
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        // Even with empty analysis, should produce valid scores
        assert!(r.volatility_risk.score >= 0.0 && r.volatility_risk.score <= 100.0);
        assert!(
            r.execution_liquidity_risk.score >= 0.0 && r.execution_liquidity_risk.score <= 100.0
        );
        assert!(r.cascade_risk.score >= 0.0 && r.cascade_risk.score <= 100.0);
    }

    #[test]
    fn volatility_risk_integrates_actionable_tf_state() {
        // v6.10.18 (I-8): the 13:25 capture — micro vol EXPANDING 0.665
        // (→ 83), fast EXPANSION_CLIMAX 0.944 (→ 97), BBWP 78.5 elevated.
        // The legacy BBWP+relative-ATR formula printed 23 (LOW) next to a
        // climax window; the dimension must now read HIGH with the TF
        // states in the evidence.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 4;
        analysis.state_confidence = 0.4677;
        let mut indicators = HashMap::new();
        indicators.insert(
            "bbwp".to_string(),
            NormalizedIndicatorValue::scalar(78.5, 0.0, "BBWP_ELEVATED".to_string()),
        );
        indicators.insert(
            "atr".to_string(),
            NormalizedIndicatorValue::scalar(9.26, 0.0, "ATR".to_string()),
        );
        let tf_volatility = vec![
            ("MICRO".to_string(), "EXPANDING".to_string(), 83.25),
            ("FAST".to_string(), "EXPANSION_CLIMAX".to_string(), 97.2),
            ("SLOW".to_string(), "NORMAL".to_string(), 58.0),
            ("MACRO".to_string(), "MAX_COMPRESSION".to_string(), 1.2),
        ];
        let risk = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            63017.0,
            &[],
            None,
            &tf_volatility,
            &RiskParams::default(),
        );
        let vol = &risk.volatility_risk;
        // (30 + 15) blended with 0.7×83.25 + 0.3×97.2 = 87.4 → 66.2 → HIGH.
        assert!(
            vol.score >= 60.0,
            "volatility_risk {} must be HIGH with a climax window",
            vol.score
        );
        let evidence = vol.evidence.join(" ");
        assert!(
            evidence.contains("EXPANSION_CLIMAX"),
            "evidence must list the TF states: {}",
            evidence
        );
        assert!(evidence.contains("BBWP elevated"));
    }

    #[test]
    fn volatility_risk_pairs_micro_fast_by_label_not_position() {
        // AUDIT-AIU-109: `tf_volatility` arrives current-pipeline-first —
        // on the fast/slow/macro pipelines the legacy positional read
        // computed `0.7 × self + 0.3 × micro` instead of the documented
        // `0.7 × micro + 0.3 × fast`. The blend must follow the SLOT
        // LABELS regardless of list order.
        let mut analysis = AnalysisMatrix::empty("BTC-USD");
        analysis.timeframes_considered = 4;
        analysis.state_confidence = 0.5;
        let indicators = HashMap::new();

        // 5s pipeline order: [5S, 1S, 30S, 1H] — 1s is NOT first. A
        // positional read would blend 0.7×5s + 0.3×1s.
        let tf_volatility = vec![
            ("5S".to_string(), "EXPANSION_CLIMAX".to_string(), 97.2),
            ("1S".to_string(), "EXPANDING".to_string(), 83.25),
            ("30S".to_string(), "NORMAL".to_string(), 58.0),
            ("1H".to_string(), "MAX_COMPRESSION".to_string(), 1.2),
        ];
        let risk = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            63017.0,
            &[],
            None,
            &tf_volatility,
            &RiskParams::default(),
        );
        let vol = &risk.volatility_risk;
        // (30 + 0) blended with 0.7×83.25 + 0.3×97.2 = 87.435 → 58.72.
        let expected = (30.0 + 0.7 * 83.25 + 0.3 * 97.2) / 2.0;
        assert!(
            (vol.score - expected).abs() < 0.01,
            "volatility_risk {} must blend micro/fast by label (expected ~{}), not position",
            vol.score,
            expected
        );
    }

    #[test]
    fn cascade_risk_does_not_crash_with_zero_inputs() {
        let analysis = make_analysis_with_timeframes();
        let indicators = HashMap::new();
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        // Baseline (no flow, no cluster) → score = 30.0
        assert!(
            (r.cascade_risk.score - 30.0).abs() < 1e-9,
            "expected 30.0, got {}",
            r.cascade_risk.score
        );
    }

    #[test]
    fn cascade_risk_with_sustained_state_is_high() {
        use crate::liquidity::{CascadeState, LiquidityFlow};
        let flow = LiquidityFlow {
            cascade_state: CascadeState::Sustained,
            cascade_intensity: 90.0,
            ..Default::default()
        };
        let analysis = make_analysis_with_timeframes();
        let indicators = HashMap::new();
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            Some(&flow),
            None,
            0.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        // Sustained + high intensity → cascade_risk >= 90 (capped at 100).
        assert!(
            r.cascade_risk.score >= 90.0,
            "sustained cascade should drive risk >= 90, got {}",
            r.cascade_risk.score
        );
    }

    #[test]
    fn volatility_risk_does_not_saturate_for_high_tf_btc() {
        // Bug-fix #6: the legacy absolute-ATR formula saturated
        // volatility_risk at 100 for any high-TF BTC candle (1d ATR
        // ≈ $1500, 1w ATR ≈ $5000-$15000). Verify that a high absolute
        // ATR with a typical close price surfaces a sub-100 score via
        // the relative-ATR formula (atr/close*100, 5% cap).
        use crate::indicator_dtos::NormalizedIndicatorValue;
        let mut indicators = HashMap::new();
        let mut atr_values = HashMap::new();
        atr_values.insert("atr_14".into(), 1500.0);
        indicators.insert(
            "atr".into(),
            NormalizedIndicatorValue {
                raw_value: 1500.0,
                normalized: 0.0,
                state_label: "ATR_RAW".into(),
                values: Some(atr_values),
                signals: vec![],
                confidence: 0.5,
            },
        );
        let analysis = make_analysis_with_timeframes();
        // close = $60_000 → atr_pct = 2.5% → score_mag(2.5, 5.0) = 50
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            60_000.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        assert!(
            r.volatility_risk.score < 100.0,
            "volatility_risk should not saturate; got {}",
            r.volatility_risk.score
        );
    }

    #[test]
    fn overall_risk_weights_sum_to_one() {
        // Bug-fix #7: the legacy weights summed to 0.90, which
        // systematically understated overall risk by ~10%. Verify the
        // ratio of overall_risk to the unweighted mean of the 8
        // dimension scores is ≈ 1.0 (within ±5%) rather than ≈ 0.90.
        let analysis = make_analysis_with_timeframes();
        let indicators = HashMap::new();
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        let dims = [
            r.market_risk.score,
            r.volatility_risk.score,
            r.execution_liquidity_risk.score,
            r.structure_risk.score,
            r.momentum_risk.score,
            r.signal_risk.score,
            r.execution_risk.score,
            r.cascade_risk.score,
        ];
        let unweighted = dims.iter().sum::<f64>() / 8.0;
        let ratio = r.overall_risk.score / unweighted.max(1e-9);
        assert!(
            (0.85..=1.05).contains(&ratio),
            "overall_risk/unweighted_avg = {}, expected ~1.0 (weights sum to 1.0)",
            ratio
        );
    }

    // ── v6.10.9: functional RiskState ─────────────────────────────────

    #[test]
    fn derive_risk_state_level_escalation() {
        // Scores already in the danger zone are their own state.
        assert_eq!(derive_risk_state(80.0, None), RiskState::Critical);
        assert_eq!(derive_risk_state(95.0, Some(30.0)), RiskState::Critical);
        assert_eq!(derive_risk_state(60.0, None), RiskState::Elevated);
        assert_eq!(derive_risk_state(75.0, Some(90.0)), RiskState::Elevated);
    }

    #[test]
    fn derive_risk_state_trend_from_previous_reference() {
        // Sub-60 scores follow the previous-synthesis delta.
        assert_eq!(derive_risk_state(45.0, Some(30.0)), RiskState::Increasing);
        assert_eq!(derive_risk_state(45.0, Some(60.0)), RiskState::Improving);
        assert_eq!(derive_risk_state(45.0, Some(40.0)), RiskState::Stable);
        assert_eq!(derive_risk_state(30.0, None), RiskState::Stable);
        // Delta exactly at the +10 band edge is Stable (strict >).
        assert_eq!(derive_risk_state(50.0, Some(40.0)), RiskState::Stable);
        assert_eq!(derive_risk_state(50.5, Some(40.01)), RiskState::Increasing);
    }

    #[test]
    fn compute_risk_applies_functional_states() {
        let analysis = make_analysis_with_timeframes();
        let indicators = HashMap::new();
        // First synthesis (no previous reference) → trend arm is Stable;
        // every sub-60 dimension inherits the overall state.
        let r = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            None,
            &[],
            &RiskParams::default(),
        );
        for dim in [
            &r.market_risk,
            &r.volatility_risk,
            &r.execution_liquidity_risk,
            &r.structure_risk,
            &r.momentum_risk,
            &r.signal_risk,
            &r.execution_risk,
            &r.cascade_risk,
        ] {
            if dim.score < 60.0 {
                assert_eq!(
                    dim.state, r.overall_risk.state,
                    "dim state must inherit the overall trend"
                );
            } else if dim.score >= 80.0 {
                assert_eq!(dim.state, RiskState::Critical);
            } else {
                assert_eq!(dim.state, RiskState::Elevated);
            }
        }
        // A strongly bearish previous synthesis (normalized reference far
        // above the current overall) → Improving trend.
        let r2 = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            Some(90.0),
            &[],
            &RiskParams::default(),
        );
        assert_eq!(r2.overall_risk.state, RiskState::Improving);
        // A strongly negative previous synthesis (reference ≈ 5) → Increasing.
        let r3 = compute_risk(
            "BTC-USD",
            &analysis,
            &indicators,
            None,
            None,
            0.0,
            &[],
            Some(-90.0),
            &[],
            &RiskParams::default(),
        );
        assert_eq!(r3.overall_risk.state, RiskState::Increasing);
    }

    fn execution_indicators(atr_14: f64, spread: f64) -> HashMap<String, NormalizedIndicatorValue> {
        let mut map = HashMap::new();
        map.insert(
            "atr".to_string(),
            NormalizedIndicatorValue {
                raw_value: 0.0,
                normalized: 0.0,
                state_label: "ATR_RAW".into(),
                values: Some(HashMap::from([("atr_14".to_string(), atr_14)])),
                signals: vec![],
                confidence: 1.0,
            },
        );
        map.insert(
            "spread".to_string(),
            NormalizedIndicatorValue {
                raw_value: spread,
                normalized: 0.0,
                state_label: "SPREAD".into(),
                values: None,
                signals: vec![],
                confidence: 1.0,
            },
        );
        map
    }

    #[test]
    fn execution_risk_low_vol_to_spread_adds_friction_penalty() {
        // ATR 0.05 / spread 0.04 % at close 100 → price spread 0.04 →
        // ratio 1.25 < 1.5 → +15 on the baseline 25. (Spread 0.04 stays
        // under the 0.08 moderate-spread threshold, so only the ratio rule
        // fires.)
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(0.05, 0.04),
            100.0,
            &RiskParams::default().execution,
        );
        assert_eq!(dim.score, 40.0);
        assert_eq!(dim.volatility_to_spread_ratio, Some(1.25));
        assert!(
            dim.evidence
                .iter()
                .any(|e| e.contains("volatility-to-spread")),
            "evidence must carry the ratio: {:?}",
            dim.evidence
        );
    }

    #[test]
    fn execution_risk_moderate_vol_to_spread_adds_small_penalty() {
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(0.06, 0.03),
            100.0,
            &RiskParams::default().execution,
        );
        assert_eq!(dim.score, 30.0);
        assert_eq!(dim.volatility_to_spread_ratio, Some(2.0));
    }

    #[test]
    fn execution_risk_high_vol_to_spread_reduces_score() {
        // Ratio 12.5 > 10.0 → −5 on the baseline 25.
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(0.5, 0.04),
            100.0,
            &RiskParams::default().execution,
        );
        assert_eq!(dim.score, 20.0);
        assert_eq!(dim.volatility_to_spread_ratio, Some(12.5));
    }

    #[test]
    fn execution_risk_missing_spread_yields_none_ratio() {
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(12.0, 0.0),
            100.0,
            &RiskParams::default().execution,
        );
        assert_eq!(dim.volatility_to_spread_ratio, None);
        assert_eq!(dim.score, 25.0);
    }

    #[test]
    fn execution_risk_zero_atr_yields_none_ratio() {
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(0.0, 1.0),
            100.0,
            &RiskParams::default().execution,
        );
        assert_eq!(dim.volatility_to_spread_ratio, None);
    }

    #[test]
    fn execution_risk_converts_spread_percent_to_price_units() {
        // v6.10.21 unit-fix regression: the live BTC-USDC capture had
        // ATR-14 = 1.5107 and a real spread of 0.000568 % at close 63040.
        // The spread in price units is 0.000568/100 × 63040 ≈ 0.358 → the
        // ratio must be ≈ 4.22 — NOT 1.5107/0.000568 ≈ 2659 (the legacy
        // percent-vs-price unit bug).
        let analysis = make_analysis_with_timeframes();
        let dim = assess_execution_risk(
            &analysis,
            &execution_indicators(1.5107, 0.000568),
            63_040.0,
            &RiskParams::default().execution,
        );
        let ratio = dim.volatility_to_spread_ratio.expect("ratio must compute");
        assert!(
            (ratio - 4.22).abs() < 0.05,
            "spread must be converted to price units before dividing ATR, got {ratio}"
        );
    }

    #[test]
    fn risk_dimension_serde_roundtrip_skips_none_ratio() {
        let dim = RiskDimension {
            score: 25.0,
            level: RiskLevel::Low,
            state: RiskState::Stable,
            confidence: 50.0,
            evidence: vec![],
            volatility_to_spread_ratio: None,
        };
        let json = serde_json::to_string(&dim).unwrap();
        assert!(!json.contains("volatility_to_spread_ratio"));
        let dim2 = RiskDimension {
            volatility_to_spread_ratio: Some(7.5),
            ..dim
        };
        let json2 = serde_json::to_string(&dim2).unwrap();
        assert!(json2.contains("volatility_to_spread_ratio"));
        let back: RiskDimension = serde_json::from_str(&json).unwrap();
        assert_eq!(back.volatility_to_spread_ratio, None);
    }
}
