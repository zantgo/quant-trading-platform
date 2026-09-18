//! # Strategy Configuration (v9)
//!
//! The **single source of truth** for all model behavior — MME L1/L1.5/
//! L2/L2.5/L3/L4/L5/L6/L7 plus TAE, PME, and PAE execution/risk/verdict
//! policy. One Strategy JSON per model; the built-in `default` strategy
//! reproduces v8.2 behavior byte-for-byte.
//!
//! Semantics:
//! - **Patch inheritance:** a strategy may declare `base`; every field
//!   explicitly present in its JSON overrides the base, everything else
//!   inherits (`StrategyConfig::resolve` performs the deep merge).
//! - **Disable-friendly:** `null` / `0` / empty = disabled = today's
//!   behavior (see each field's doc comment).
//! - Every `#[serde(default = "…")]` mirrors the pre-v9 hardcoded constant.

use serde::{Deserialize, Serialize};

// ─── L1: Metrics ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1ContextParams {
    /// `overall_score = trend·w0 + momentum·w1` (directional blend).
    pub trend_momentum_blend: [f64; 2],
    /// Local-regime conviction dampening on `overall_score`.
    pub regime_gate_damp: L1RegimeDamp,
    /// The local 4-state regime rule thresholds.
    pub regime_rule: L1RegimeRule,
    /// Volatility-dimension source blend (bbwp / hv / atr_pct). Defaults
    /// bbwp-only = today.
    pub volatility_sources: L1VolSources,
}

impl Default for L1ContextParams {
    fn default() -> Self {
        Self {
            trend_momentum_blend: [0.6, 0.4],
            regime_gate_damp: L1RegimeDamp::default(),
            regime_rule: L1RegimeRule::default(),
            volatility_sources: L1VolSources::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1RegimeDamp {
    pub trending: f64,
    pub expansion: f64,
    pub range: f64,
    pub other: f64,
}

impl Default for L1RegimeDamp {
    fn default() -> Self {
        Self {
            trending: 1.0,
            expansion: 1.0,
            range: 0.6,
            other: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1RegimeRule {
    pub bbwp_compression: f64,
    pub bbwp_expansion: f64,
    pub adx_trending: f64,
    pub chop_compression: f64,
    pub chop_trending: f64,
}

impl Default for L1RegimeRule {
    fn default() -> Self {
        Self {
            bbwp_compression: 15.0,
            bbwp_expansion: 85.0,
            adx_trending: 25.0,
            chop_compression: 61.8,
            chop_trending: 38.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1VolSources {
    pub bbwp: f64,
    pub hv: f64,
    pub atr_pct: f64,
}

impl Default for L1VolSources {
    fn default() -> Self {
        Self {
            bbwp: 1.0,
            hv: 0.0,
            atr_pct: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1SignalParams {
    /// Per-SignalKind confidence boost (empty = detector defaults).
    pub confidence_boost: std::collections::HashMap<String, f64>,
    /// Drop stale signals older than N bars (null = no limit).
    pub max_age_bars: Option<u32>,
    /// Weak/Moderate/Strong/Extreme borders for `|normalized|`.
    pub strength_buckets: [f64; 3],
}

impl Default for L1SignalParams {
    fn default() -> Self {
        Self {
            confidence_boost: std::collections::HashMap::new(),
            max_age_bars: None,
            strength_buckets: [0.15, 0.6, 0.85],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L1Params {
    /// Per-indicator trust weights (0–5; default 1.0 = today). Zero =
    /// compute + display but muted from synthesis.
    pub indicator_weights: std::collections::HashMap<String, f64>,
    /// Third state: compute + display + signals, but contribute 0 to
    /// MarketContext/Alignment.
    pub monitor_only: Vec<String>,
    pub context: L1ContextParams,
    pub signals: L1SignalParams,
    /// Don't feed DIE-synthesized gap candles into indicator state machines.
    pub ignore_reconstructed_candles: bool,
    pub order_book: L1OrderBookParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1OrderBookParams {
    pub depth_levels: usize,
    pub imbalance_threshold: f64,
    pub wall_threshold: f64,
    pub spread_warning_pct: f64,
    pub spread_wide_threshold_pct: f64,
}

impl L1OrderBookParams {
    /// v9: the strategy's `l1.order_book` is the single source of truth for
    /// the depth/imbalance/wall/spread surface. `to_order_book_config` maps
    /// it onto the runtime `OrderBookConfig` the pipelines consume.
    pub fn to_order_book_config(&self) -> crate::OrderBookConfig {
        crate::OrderBookConfig {
            depth_levels: self.depth_levels,
            imbalance_threshold: self.imbalance_threshold,
            wall_threshold: self.wall_threshold,
            spread_warning_pct: self.spread_warning_pct,
            spread_wide_threshold_pct: self.spread_wide_threshold_pct,
        }
    }
}

impl Default for L1OrderBookParams {
    fn default() -> Self {
        Self {
            depth_levels: 20,
            imbalance_threshold: 0.3,
            wall_threshold: 0.5,
            spread_warning_pct: 0.1,
            spread_wide_threshold_pct: 0.05,
        }
    }
}

// ─── L1.5: Derivatives Telemetry ────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1_5AccumulatorParams {
    pub cascade_window_candles: usize,
    pub intensity_log_scale: f64,
    pub baseline_no_history_usd: f64,
    pub sig_window_events: usize,
    pub fallback_baseline_usd: f64,
    pub exhausted_intensity: f64,
    pub max_buffered_events: usize,
}

impl Default for L1_5AccumulatorParams {
    fn default() -> Self {
        Self {
            cascade_window_candles: 5,
            intensity_log_scale: 20.0,
            baseline_no_history_usd: 1000.0,
            sig_window_events: 50,
            fallback_baseline_usd: 500.0,
            exhausted_intensity: 30.0,
            max_buffered_events: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1_5Params {
    pub enabled: bool,
    pub liquidation_feed: bool,
    pub cluster_estimation: bool,
    pub signals: bool,
    pub mark_price_poll_ms: u64,
    pub event_retention_days: u32,
    pub bucket_retention_days: u32,
    pub cluster_refresh_secs: u64,
    pub maintenance_margin_rate: f64,
    pub cascade_detected_zscore: f64,
    pub cascade_sustained_events: u32,
    pub funding_extreme_pct: f64,
    pub magnet_activation_distance_pct: f64,
    pub liquidity_vacuum_threshold: f64,
    pub oi_funding_divergence_pct: f64,
    pub min_cluster_notional_usd: f64,
    pub signal_confidences: std::collections::HashMap<String, f64>,
    /// The 11-kind trust axis (0–5; default 1.0). Multiplies each signal's
    /// contribution to L5 cascade bonuses and L4 squeeze strength.
    pub signal_weights: std::collections::HashMap<String, f64>,
    pub accumulator: L1_5AccumulatorParams,
    pub api_failover: L1_5FailoverParams,
    pub per_tf_leverage: L1_5TfLeverageParams,
}

impl Default for L1_5Params {
    fn default() -> Self {
        Self {
            enabled: true,
            liquidation_feed: true,
            cluster_estimation: true,
            signals: true,
            mark_price_poll_ms: 60_000,
            event_retention_days: 90,
            bucket_retention_days: 7,
            cluster_refresh_secs: 0,
            maintenance_margin_rate: 0.005,
            cascade_detected_zscore: 2.5,
            cascade_sustained_events: 3,
            funding_extreme_pct: 0.0005,
            magnet_activation_distance_pct: 0.5,
            liquidity_vacuum_threshold: 0.3,
            oi_funding_divergence_pct: 2.0,
            min_cluster_notional_usd: 50_000.0,
            signal_confidences: default_signal_confidences(),
            signal_weights: std::collections::HashMap::new(),
            accumulator: L1_5AccumulatorParams::default(),
            api_failover: L1_5FailoverParams::default(),
            per_tf_leverage: L1_5TfLeverageParams::default(),
        }
    }
}

fn default_signal_confidences() -> std::collections::HashMap<String, f64> {
    let mut m = std::collections::HashMap::new();
    m.insert("CASCADE_DETECTED".into(), 0.8);
    m.insert("CASCADE_SUSTAINED".into(), 0.9);
    m.insert("CASCADE_EXHAUSTED".into(), 0.7);
    m.insert("FUNDING_EXTREME".into(), 0.95);
    m.insert("OI_FUNDING_DIVERGENCE".into(), 0.7);
    m.insert("LIQUIDITY_VACUUM".into(), 0.6);
    m.insert("FUNDING_FLIP".into(), 0.75);
    m.insert("OI_PRICE_DIVERGENCE".into(), 0.7);
    m
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1_5FailoverParams {
    pub max_retries_per_call: u32,
    pub retry_delay_seconds: u32,
    pub max_consecutive_failures: u32,
}

impl Default for L1_5FailoverParams {
    fn default() -> Self {
        Self {
            max_retries_per_call: 5,
            retry_delay_seconds: 30,
            max_consecutive_failures: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L1_5TfLeverageParams {
    pub enabled: bool,
    pub buckets: Vec<u32>,
    pub weights: Vec<f64>,
    pub min_cluster_notional_usd: f64,
}

impl Default for L1_5TfLeverageParams {
    fn default() -> Self {
        Self {
            enabled: true,
            buckets: vec![1, 3, 5, 10, 20, 50, 100],
            weights: vec![0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 50_000.0,
        }
    }
}

// ─── L2: Alignment ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2Params {
    pub tf_weighting: L2TfWeighting,
    pub overall_blend: L2Blend,
    pub thin_volume: L2ThinVolume,
    pub confluence: L2Confluence,
    /// Weight `trend_agreement_pct` by the TF weights (false = count-based,
    /// today).
    pub trend_agreement_weighted: bool,
    /// Per-dimension mute (stays on wire, contributes 0).
    pub dimension_mask: std::collections::HashMap<String, bool>,
    pub states: L2States,
}

impl Default for L2Params {
    fn default() -> Self {
        Self {
            tf_weighting: L2TfWeighting::default(),
            overall_blend: L2Blend::default(),
            thin_volume: L2ThinVolume::default(),
            confluence: L2Confluence::default(),
            trend_agreement_weighted: false,
            dimension_mask: L2Params::default_dimension_mask(),
            states: L2States::default(),
        }
    }
}

impl L2Params {
    fn default_dimension_mask() -> std::collections::HashMap<String, bool> {
        [
            ("trend", true),
            ("momentum", true),
            ("volume", true),
            ("volatility", true),
            ("structure", true),
            ("signal", true),
            ("regime", true),
            ("confidence", true),
            ("liquidity", true),
            ("tradability", true),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2TfWeighting {
    /// `proportional` (today) | `equal` | `custom`.
    pub mode: String,
    pub weights: std::collections::HashMap<String, f64>,
    pub floor: f64,
    pub ceil: f64,
}

impl Default for L2TfWeighting {
    fn default() -> Self {
        let mut w = std::collections::HashMap::new();
        // v11.9 duration-keyed pool (family-split defaults; the newer
        // long-horizon durations extend the top weight).
        w.insert("1s".into(), 0.1);
        w.insert("3s".into(), 0.1);
        w.insert("5s".into(), 0.1);
        w.insert("15s".into(), 0.1);
        w.insert("30s".into(), 0.166);
        w.insert("1m".into(), 0.166);
        w.insert("3m".into(), 0.5);
        w.insert("5m".into(), 0.5);
        w.insert("15m".into(), 1.0);
        w.insert("1h".into(), 1.0);
        w.insert("30m".into(), 1.0);
        w.insert("4h".into(), 1.0);
        w.insert("12h".into(), 1.0);
        w.insert("1d".into(), 1.0);
        Self {
            mode: "proportional".into(),
            weights: w,
            floor: 0.2,
            ceil: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2Blend {
    pub trend: f64,
    pub momentum: f64,
    pub volatility: f64,
    pub volume: f64,
}

impl Default for L2Blend {
    fn default() -> Self {
        Self {
            trend: 0.5,
            momentum: 0.3,
            volatility: 0.1,
            volume: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2ThinVolume {
    pub enabled: bool,
    /// Volume dim score below which the reweight fires.
    pub threshold: f64,
    pub blend: L2Blend,
}

impl Default for L2ThinVolume {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 25.0,
            blend: L2Blend {
                trend: 0.55,
                momentum: 0.35,
                volatility: 0.05,
                volume: 0.05,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2Confluence {
    /// Signals must appear in ≥ N TFs to count as cross-TF confluence.
    pub min_tfs: u8,
}

impl Default for L2Confluence {
    fn default() -> Self {
        Self { min_tfs: 2 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2States {
    pub signed: [f64; 2],
    pub unsigned: [f64; 4],
    pub overall_label_bands: [f64; 2],
    pub single_tf_confidence_score: f64,
}

impl Default for L2States {
    fn default() -> Self {
        Self {
            signed: [0.3, 0.6],
            unsigned: [20.0, 40.0, 60.0, 80.0],
            overall_label_bands: [20.0, 40.0],
            single_tf_confidence_score: 50.0,
        }
    }
}

// ─── L2.5: Liquidity Synthesis ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L2_5Params {
    pub estimation: L2_5Estimation,
    pub oi_split: L2_5OiSplit,
    pub confidence: L2_5Confidence,
    pub funding_modulation: L2_5FundingModulation,
    pub signals: L2_5Signals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2_5Estimation {
    pub swing_window_bars: usize,
    pub swing_lookback: usize,
    pub bin_size_pct: f64,
    pub peak_halfwidth_divisor: usize,
    pub bound_decay: f64,
    pub ttl_secs: u64,
}

impl Default for L2_5Estimation {
    fn default() -> Self {
        Self {
            swing_window_bars: 200,
            swing_lookback: 5,
            bin_size_pct: 0.001,
            peak_halfwidth_divisor: 20,
            bound_decay: 0.5,
            ttl_secs: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2_5OiSplit {
    /// `null` = follow `l1_5.funding_extreme_pct` (v9 F-01).
    pub funding_anchor: Option<f64>,
    pub funding_bias_scale: f64,
    pub price_anchor_pct: f64,
    pub price_bias_scale: f64,
    pub clamp: [f64; 2],
}

impl Default for L2_5OiSplit {
    fn default() -> Self {
        Self {
            funding_anchor: None,
            funding_bias_scale: 0.3,
            price_anchor_pct: 1.0,
            price_bias_scale: 0.2,
            clamp: [0.10, 0.90],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2_5Confidence {
    pub oi_adequacy_anchor_usd: f64,
    pub funding_penalty: f64,
}

impl Default for L2_5Confidence {
    fn default() -> Self {
        Self {
            oi_adequacy_anchor_usd: 1_000_000.0,
            funding_penalty: 0.3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2_5FundingModulation {
    pub shift: f64,
}

impl Default for L2_5FundingModulation {
    fn default() -> Self {
        Self { shift: 0.05 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L2_5Signals {
    pub sustained_events_this_bar: u32,
    pub vacuum_dense_events: u32,
    pub vacuum_dense_usd: f64,
    pub funding_extreme_strength_slope: f64,
}

impl Default for L2_5Signals {
    fn default() -> Self {
        Self {
            sustained_events_this_bar: 3,
            vacuum_dense_events: 3,
            vacuum_dense_usd: 50_000.0,
            funding_extreme_strength_slope: 50.0,
        }
    }
}

// ─── L3: Analysis ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Params {
    pub bias: L3Bias,
    pub confidence: L3Confidence,
    pub regime: L3Regime,
    pub assessments: L3Assessments,
    pub quality_bands: [f64; 4],
    /// Wyckoff phase knobs (advanced, interpretive only).
    pub phase: L3Phase,
}

impl Default for L3Params {
    fn default() -> Self {
        Self {
            bias: L3Bias::default(),
            confidence: L3Confidence::default(),
            regime: L3Regime::default(),
            assessments: L3Assessments::default(),
            quality_bands: [30.0, 50.0, 70.0, 85.0],
            phase: L3Phase::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L3Bias {
    pub bands: L3BiasBands,
    pub grace: L3Grace,
    pub lean: L3Lean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3BiasBands {
    pub strong: f64,
    pub plain: f64,
}

impl Default for L3BiasBands {
    fn default() -> Self {
        Self {
            strong: 25.0,
            plain: 12.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Grace {
    pub band: [f64; 2],
    pub vote_min: usize,
    pub flat_tf: f64,
    pub agreement_min: f64,
    pub signals_min: u32,
    pub haircut: f64,
    pub hold: L3GraceHold,
    pub skip_regime: String,
}

impl Default for L3Grace {
    fn default() -> Self {
        Self {
            band: [10.0, 15.0],
            vote_min: 2,
            flat_tf: 8.0,
            agreement_min: 60.0,
            signals_min: 2,
            haircut: 0.85,
            hold: L3GraceHold::default(),
            skip_regime: "COMPRESSION".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3GraceHold {
    pub band_min: f64,
    pub vote_min: usize,
}

impl Default for L3GraceHold {
    fn default() -> Self {
        Self {
            band_min: 12.0,
            vote_min: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Lean {
    pub tolerance: f64,
    pub haircut: f64,
}

impl Default for L3Lean {
    fn default() -> Self {
        Self {
            tolerance: 10.0,
            haircut: 0.8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Confidence {
    pub agreement: L3ConfPair,
    pub conflict: L3ConfPair,
    pub signals: L3ConfPair,
    pub single_tf_cap: f64,
}

impl Default for L3Confidence {
    fn default() -> Self {
        Self {
            agreement: L3ConfPair {
                bonus: 0.15,
                min: 75.0,
            },
            conflict: L3ConfPair {
                bonus: 0.5,
                min: 50.0,
            },
            signals: L3ConfPair {
                bonus: 0.10,
                min: 3.0,
            },
            single_tf_cap: 0.5,
        }
    }
}

/// Shared shape for the confidence bonus/min pairs (bonus + threshold;
/// the conflict pair's `bonus` is the cap value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3ConfPair {
    pub bonus: f64,
    pub min: f64,
}

impl Default for L3ConfPair {
    fn default() -> Self {
        Self {
            bonus: 0.0,
            min: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Regime {
    pub bbwp: L3RegimeBbwp,
    pub adx: f64,
    pub trend_score: f64,
    pub missing: L3RegimeMissing,
}

impl Default for L3Regime {
    fn default() -> Self {
        Self {
            bbwp: L3RegimeBbwp {
                expansion: 85.0,
                contraction: 10.0,
            },
            adx: 25.0,
            trend_score: 20.0,
            missing: L3RegimeMissing::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3RegimeBbwp {
    pub expansion: f64,
    pub contraction: f64,
}

impl Default for L3RegimeBbwp {
    fn default() -> Self {
        Self {
            expansion: 85.0,
            contraction: 10.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3RegimeMissing {
    pub bbwp: f64,
    pub adx: f64,
}

impl Default for L3RegimeMissing {
    fn default() -> Self {
        Self {
            bbwp: 50.0,
            adx: 25.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Assessments {
    pub trend: [f64; 4],
    pub momentum: [f64; 3],
    pub structure: [f64; 4],
    pub volatility: [f64; 4],
    pub volume: [f64; 3],
}

impl Default for L3Assessments {
    fn default() -> Self {
        Self {
            trend: [90.0, 75.0, 50.0, 25.0],
            momentum: [80.0, 60.0, 40.0],
            structure: [80.0, 60.0, 40.0, 20.0],
            volatility: [90.0, 70.0, 40.0, 20.0],
            volume: [90.0, 70.0, 40.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L3Phase {
    pub low_vol_max: f64,
    pub trend_score: f64,
    pub volume_strong: f64,
    pub structure_healthy: f64,
    pub volume_delta: f64,
}

impl Default for L3Phase {
    fn default() -> Self {
        Self {
            low_vol_max: 40.0,
            trend_score: 20.0,
            volume_strong: 70.0,
            structure_healthy: 60.0,
            volume_delta: 5.0,
        }
    }
}

// ─── L4: Opportunity ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Params {
    pub setups: L4Setups,
    pub preconditions: L4Preconditions,
    pub scoring: L4Scoring,
    pub zones: L4Zones,
    pub confluence_weights: std::collections::HashMap<String, f64>,
    pub costs: L4Costs,
}

impl Default for L4Params {
    fn default() -> Self {
        Self {
            setups: L4Setups::default(),
            preconditions: L4Preconditions::default(),
            scoring: L4Scoring::default(),
            zones: L4Zones::default(),
            confluence_weights: L4Params::default_confluence_weights(),
            costs: L4Costs::default(),
        }
    }
}

impl L4Params {
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Setups {
    pub enabled: Vec<String>,
    /// First-match priority order of the L4 selection tree.
    pub priority: Vec<String>,
}

impl Default for L4Setups {
    fn default() -> Self {
        let all = vec![
            "LiquiditySqueeze",
            "Scalp",
            "TrendContinuation",
            "Breakout",
            "Reversal",
            "Pullback",
            "MeanReversion",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        Self {
            enabled: all.clone(),
            priority: all,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Preconditions {
    pub trend_continuation: L4PcTrendContinuation,
    pub breakout: L4PcBreakout,
    pub reversal: L4PcReversal,
    pub pullback: L4PcPullback,
    pub mean_reversion: L4PcMeanReversion,
    pub scalp: L4PcScalp,
    pub liquidity_squeeze: L4PcSqueeze,
}

impl Default for L4Preconditions {
    fn default() -> Self {
        Self {
            trend_continuation: L4PcTrendContinuation { trend_min: 55.0 },
            breakout: L4PcBreakout {
                vol_min: 55.0,
                struct_min: 45.0,
            },
            reversal: L4PcReversal {
                momentum_exhausted_max: 30.0,
                structure_broken_max: 45.0,
            },
            pullback: L4PcPullback { trend_min: 45.0 },
            mean_reversion: L4PcMeanReversion {
                vol_max: 40.0,
                regimes: vec!["Range".into(), "Contraction".into()],
            },
            scalp: L4PcScalp {
                bbwp_range: [70.0, 95.0],
                struct_min: 55.0,
                regimes: vec!["TrendingBull".into(), "TrendingBear".into()],
            },
            liquidity_squeeze: L4PcSqueeze {
                asymmetry_min: 0.25,
                regimes: vec!["Expansion".into(), "Transition".into()],
            },
        }
    }
}

macro_rules! pc_struct {
    ($name:ident { $( $field:ident : $ty:ty ),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $name {
            $( pub $field : $ty, )*
        }
        impl Default for $name {
            fn default() -> Self {
                Self { $( $field : Default::default(), )* }
            }
        }
    };
}

pc_struct!(L4PcTrendContinuation { trend_min: f64 });
pc_struct!(L4PcBreakout {
    vol_min: f64,
    struct_min: f64
});
pc_struct!(L4PcReversal {
    momentum_exhausted_max: f64,
    structure_broken_max: f64
});
pc_struct!(L4PcPullback { trend_min: f64 });
pc_struct!(L4PcMeanReversion { vol_max: f64, regimes: Vec<String> });
pc_struct!(L4PcScalp { bbwp_range: [f64; 2], struct_min: f64, regimes: Vec<String> });
pc_struct!(L4PcSqueeze { asymmetry_min: f64, regimes: Vec<String> });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Scoring {
    pub blend: [f64; 4],
    pub quality_bands: [f64; 4],
}

impl Default for L4Scoring {
    fn default() -> Self {
        Self {
            blend: [0.35, 0.30, 0.20, 0.15],
            quality_bands: [85.0, 70.0, 50.0, 30.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Zones {
    pub atr_fallback: L4AtrFallback,
    pub tolerance_atr_mult: f64,
    pub tolerance_close_pct: f64,
    pub width_k: L4WidthK,
    pub fallback_strength: f64,
    pub invalidation_weights: std::collections::HashMap<String, f64>,
    pub range_frame: L4RangeFrame,
    pub horizon_stop_budgets: std::collections::HashMap<String, f64>,
}

impl Default for L4Zones {
    fn default() -> Self {
        let mut inv = std::collections::HashMap::new();
        inv.insert("fib_0786".into(), 0.5);
        inv.insert("vp_val".into(), 0.4);
        let mut horizon = std::collections::HashMap::new();
        horizon.insert("scalp".into(), 1.5);
        horizon.insert("swing".into(), 3.0);
        Self {
            atr_fallback: L4AtrFallback::default(),
            tolerance_atr_mult: 0.2,
            tolerance_close_pct: 0.1,
            width_k: L4WidthK::default(),
            fallback_strength: 35.0,
            invalidation_weights: inv,
            range_frame: L4RangeFrame::default(),
            horizon_stop_budgets: horizon,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4AtrFallback {
    pub enabled: bool,
    pub k_entry: f64,
    pub k_target: f64,
}

impl Default for L4AtrFallback {
    fn default() -> Self {
        Self {
            enabled: true,
            k_entry: 1.5,
            k_target: 2.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4WidthK {
    pub high: f64,
    pub threshold: f64,
    pub low: f64,
}

impl Default for L4WidthK {
    fn default() -> Self {
        Self {
            high: 2.0,
            threshold: 70.0,
            low: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4RangeFrame {
    pub entry_half_atr: f64,
    pub target_k_atr: f64,
    pub target_spread_atr: f64,
    pub inv_k_atr: f64,
}

impl Default for L4RangeFrame {
    fn default() -> Self {
        Self {
            entry_half_atr: 0.2,
            target_k_atr: 1.5,
            target_spread_atr: 0.2,
            inv_k_atr: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L4Costs {
    pub taker_fee_bps: f64,
    pub slippage_bps: f64,
    pub funding_bps: f64,
}

impl Default for L4Costs {
    fn default() -> Self {
        Self {
            taker_fee_bps: 6.0,
            slippage_bps: 5.0,
            funding_bps: 0.0,
        }
    }
}

// ─── L5: Risk ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Params {
    pub overall_weights: std::collections::HashMap<String, f64>,
    pub bands: [f64; 4],
    pub state_delta: f64,
    pub dimensions: L5Dimensions,
}

impl Default for L5Params {
    fn default() -> Self {
        let mut w = std::collections::HashMap::new();
        w.insert("market".into(), 0.14);
        w.insert("volatility".into(), 0.14);
        w.insert("execution_liquidity".into(), 0.14);
        w.insert("structure".into(), 0.10);
        w.insert("momentum".into(), 0.14);
        w.insert("signal".into(), 0.10);
        w.insert("execution".into(), 0.10);
        w.insert("cascade".into(), 0.14);
        Self {
            overall_weights: w,
            bands: [80.0, 60.0, 40.0, 20.0],
            state_delta: 10.0,
            dimensions: L5Dimensions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L5Dimensions {
    pub market: L5Market,
    pub volatility: L5Volatility,
    pub execution_liquidity: L5ExecLiquidity,
    pub structure: L5Structure,
    pub momentum: L5Momentum,
    pub signal: L5Signal,
    pub execution: L5Execution,
    pub cascade: L5Cascade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Market {
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

impl Default for L5Market {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Volatility {
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

impl Default for L5Volatility {
    fn default() -> Self {
        Self {
            baseline: 30.0,
            bbwp_extreme: 90.0,
            bbwp_extreme_add: 30.0,
            bbwp_elevated: 70.0,
            bbwp_elevated_add: 15.0,
            squeeze_add: 10.0,
            micro_fast_blend: [0.7, 0.3],
            atr_pct_floor: 1.0,
            atr_pct_max: 5.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5ExecLiquidity {
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

impl Default for L5ExecLiquidity {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Structure {
    pub baseline: f64,
    pub broken: f64,
    pub weak: f64,
    pub healthy: f64,
    pub flip: f64,
}

impl Default for L5Structure {
    fn default() -> Self {
        Self {
            baseline: 40.0,
            broken: 30.0,
            weak: 15.0,
            healthy: -15.0,
            flip: 15.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Momentum {
    pub baseline: f64,
    pub exhausted: f64,
    pub reversing: f64,
    pub weakening: f64,
    pub increasing: f64,
}

impl Default for L5Momentum {
    fn default() -> Self {
        Self {
            baseline: 30.0,
            exhausted: 40.0,
            reversing: 30.0,
            weakening: 15.0,
            increasing: -10.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Signal {
    pub baseline: f64,
    pub per_contradicting: f64,
    pub contradicting_cap: f64,
    pub none_active: f64,
    pub low_conf_max: f64,
    pub low_conf: f64,
}

impl Default for L5Signal {
    fn default() -> Self {
        Self {
            baseline: 30.0,
            per_contradicting: 10.0,
            contradicting_cap: 40.0,
            none_active: 10.0,
            low_conf_max: 0.5,
            low_conf: 15.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Execution {
    pub baseline: f64,
    pub spread_wide: f64,
    pub spread_wide_add: f64,
    pub spread_moderate: f64,
    pub spread_moderate_add: f64,
    pub rvol_low: f64,
    pub rvol_add: f64,
    pub ratio_tiers: Vec<L5RatioTier>,
}

impl Default for L5Execution {
    fn default() -> Self {
        Self {
            baseline: 25.0,
            spread_wide: 0.15,
            spread_wide_add: 25.0,
            spread_moderate: 0.08,
            spread_moderate_add: 10.0,
            rvol_low: 0.7,
            rvol_add: 15.0,
            ratio_tiers: vec![
                L5RatioTier {
                    max: Some(1.5),
                    min: None,
                    add: 15.0,
                },
                L5RatioTier {
                    max: Some(3.0),
                    min: None,
                    add: 5.0,
                },
                L5RatioTier {
                    max: None,
                    min: Some(10.0),
                    add: -5.0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5RatioTier {
    pub max: Option<f64>,
    pub min: Option<f64>,
    pub add: f64,
}

impl Default for L5RatioTier {
    fn default() -> Self {
        Self {
            max: None,
            min: None,
            add: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L5Cascade {
    pub baseline: f64,
    pub sustained: f64,
    pub detected: f64,
    pub asymmetry_min: f64,
    pub asymmetry_scale: f64,
    pub oi_divergence_max: f64,
    pub funding_flip_max: f64,
}

impl Default for L5Cascade {
    fn default() -> Self {
        Self {
            baseline: 30.0,
            sustained: 30.0,
            detected: 15.0,
            asymmetry_min: 0.3,
            asymmetry_scale: 30.0,
            oi_divergence_max: 15.0,
            funding_flip_max: 10.0,
        }
    }
}

// ─── L6: Decision ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L6Params {
    pub synthesis: L6Synthesis,
    pub stance: L6Stance,
    pub direction: L6Direction,
    pub entry: L6Entry,
    pub exit: L6Exit,
    pub protection: L6Protection,
    pub target: L6Target,
    pub stop: L6Stop,
    pub entry_danger: L6EntryDanger,
    pub readiness: L6Readiness,
    pub probability: L6Probability,
    pub risk_ceiling: L6RiskCeiling,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Synthesis {
    pub confluence_weights: [f64; 3],
    pub risk_discount_k: f64,
    pub opportunity_fallback: f64,
}

impl Default for L6Synthesis {
    fn default() -> Self {
        Self {
            confluence_weights: [0.50, 0.30, 0.20],
            risk_discount_k: 1.0,
            opportunity_fallback: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L6Stance {
    pub risk: L6StanceRisk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6StanceRisk {
    pub avoid: f64,
    pub cautious: f64,
    pub neutral: f64,
    pub constructive: f64,
    pub aggressive: f64,
}

impl Default for L6StanceRisk {
    fn default() -> Self {
        Self {
            avoid: 90.0,
            cautious: 75.0,
            neutral: 50.0,
            constructive: 55.0,
            aggressive: 45.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Direction {
    pub risk_strong: f64,
    pub risk_plain: f64,
}

impl Default for L6Direction {
    fn default() -> Self {
        Self {
            risk_strong: 50.0,
            risk_plain: 40.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Entry {
    pub vol_risk_no_entry: f64,
    pub vol_risk_immediate: f64,
    pub vol_risk_breakout: f64,
}

impl Default for L6Entry {
    fn default() -> Self {
        Self {
            vol_risk_no_entry: 75.0,
            vol_risk_immediate: 40.0,
            vol_risk_breakout: 30.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Exit {
    pub risk_increasing: f64,
    pub trend_weakening: f64,
}

impl Default for L6Exit {
    fn default() -> Self {
        Self {
            risk_increasing: 80.0,
            trend_weakening: 60.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Protection {
    pub vol_risk: f64,
    pub sr_proximity_atr_mult: f64,
}

impl Default for L6Protection {
    fn default() -> Self {
        Self {
            vol_risk: 60.0,
            sr_proximity_atr_mult: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Target {
    pub rr_based: f64,
    pub trailing: f64,
}

impl Default for L6Target {
    fn default() -> Self {
        Self {
            rr_based: 40.0,
            trailing: 60.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Stop {
    pub base_multiplier: L6StopBaseMultiplier,
    pub base_pct: f64,
    pub base_clamp: [f64; 2],
    pub vol_bump_scale: f64,
    pub final_clamp: [f64; 2],
}

impl Default for L6Stop {
    fn default() -> Self {
        Self {
            base_multiplier: L6StopBaseMultiplier::default(),
            base_pct: 2.0,
            base_clamp: [0.5, 5.0],
            vol_bump_scale: 10.0,
            final_clamp: [0.5, 15.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6StopBaseMultiplier {
    pub strong: f64,
    pub weak: f64,
}

impl Default for L6StopBaseMultiplier {
    fn default() -> Self {
        Self {
            strong: 1.0,
            weak: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6EntryDanger {
    pub quality_penalties: std::collections::HashMap<String, f64>,
    pub blend: [f64; 2],
}

impl Default for L6EntryDanger {
    fn default() -> Self {
        let mut q = std::collections::HashMap::new();
        q.insert("Excellent".into(), 10.0);
        q.insert("Good".into(), 25.0);
        q.insert("Average".into(), 50.0);
        q.insert("Weak".into(), 70.0);
        q.insert("Poor".into(), 80.0);
        Self {
            quality_penalties: q,
            blend: [0.5, 0.5],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Readiness {
    pub aside_max: f64,
    pub ready_min: f64,
}

impl Default for L6Readiness {
    fn default() -> Self {
        Self {
            aside_max: 20.0,
            ready_min: 20.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L6Probability {
    pub guidance_amp: f64,
    pub guidance_atten: f64,
    pub stance_amp: f64,
    pub avoid_atten: f64,
    pub avoid_hold_amp: f64,
    pub rr_penalty: f64,
    pub min_pct: f64,
    pub hold_cap: f64,
    pub arm_floor: f64,
    pub geometric_offset: f64,
    pub eff_conf_floor: f64,
    pub hold_scale: f64,
    pub contributing_conf_min: f64,
}

impl Default for L6Probability {
    fn default() -> Self {
        Self {
            guidance_amp: 1.2,
            guidance_atten: 0.5,
            stance_amp: 1.15,
            avoid_atten: 0.5,
            avoid_hold_amp: 1.5,
            rr_penalty: 0.6,
            min_pct: 2.0,
            hold_cap: 60.0,
            arm_floor: 15.0,
            geometric_offset: 0.15,
            eff_conf_floor: 0.5,
            hold_scale: 50.0,
            contributing_conf_min: 0.6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L6RiskCeiling {
    /// `null` = no ceiling (today). Soft-block when exceeded.
    pub max_overall_risk: Option<f64>,
}

// ─── L7: Overview ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7Params {
    pub breadth_bands: L7BreadthBands,
    pub global_bias: L7GlobalBias,
    pub sync_bands: [f64; 4],
    pub risk: L7Risk,
    pub systemic: L7Systemic,
    pub asset_rank: L7AssetRank,
    pub low_coverage_min_symbols: u32,
    pub alignment_buckets: [f64; 2],
    /// `null` = no market filter. TAE intake gate when set.
    pub breadth_entry_floor: Option<f64>,
}

impl Default for L7Params {
    fn default() -> Self {
        Self {
            breadth_bands: L7BreadthBands::default(),
            global_bias: L7GlobalBias::default(),
            sync_bands: [75.0, 50.0, 25.0, 10.0],
            risk: L7Risk::default(),
            systemic: L7Systemic::default(),
            asset_rank: L7AssetRank::default(),
            low_coverage_min_symbols: 3,
            alignment_buckets: [75.0, 50.0],
            breadth_entry_floor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7BreadthBands {
    pub strong: f64,
    pub positive: f64,
    pub balanced: f64,
}

impl Default for L7BreadthBands {
    fn default() -> Self {
        Self {
            strong: 60.0,
            positive: 20.0,
            balanced: 10.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7GlobalBias {
    pub strong_share: f64,
    pub plain_share: f64,
}

impl Default for L7GlobalBias {
    fn default() -> Self {
        Self {
            strong_share: 0.8,
            plain_share: 0.6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct L7Risk {
    pub dist_bins: L7DistBins,
    pub env_mean: L7EnvMean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7DistBins {
    pub low_max: f64,
    pub high_min: f64,
}

impl Default for L7DistBins {
    fn default() -> Self {
        Self {
            low_max: 30.0,
            high_min: 70.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7EnvMean {
    pub high: f64,
    pub moderate: f64,
}

impl Default for L7EnvMean {
    fn default() -> Self {
        Self {
            high: 50.0,
            moderate: 25.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7Systemic {
    pub weights: [f64; 2],
    pub sync_penalty: std::collections::HashMap<String, f64>,
    pub tf_decay: std::collections::HashMap<String, f64>,
    pub cascade_index_fallback: f64,
    /// The strategy's systemic appetite — the PME Gate-7 style veto is
    /// enforced as a TAE intake gate via `pme.enforce_systemic_veto`.
    pub entry_veto_threshold: f64,
}

impl Default for L7Systemic {
    fn default() -> Self {
        let mut sync = std::collections::HashMap::new();
        sync.insert("highly_synchronized".into(), 100.0);
        sync.insert("synchronized".into(), 60.0);
        sync.insert("mixed".into(), 30.0);
        sync.insert("fragmented".into(), 10.0);
        sync.insert("highly_fragmented".into(), 0.0);
        let mut decay = std::collections::HashMap::new();
        // v11.9 duration-keyed pool (family-split defaults, Σ = 1.0).
        decay.insert("1s".into(), 0.04);
        decay.insert("3s".into(), 0.04);
        decay.insert("5s".into(), 0.04);
        decay.insert("15s".into(), 0.07);
        decay.insert("30s".into(), 0.07);
        decay.insert("1m".into(), 0.10);
        decay.insert("3m".into(), 0.10);
        decay.insert("5m".into(), 0.10);
        decay.insert("15m".into(), 0.08);
        decay.insert("1h".into(), 0.08);
        decay.insert("30m".into(), 0.07);
        decay.insert("4h".into(), 0.07);
        decay.insert("12h".into(), 0.07);
        decay.insert("1d".into(), 0.07);
        Self {
            weights: [0.6, 0.4],
            sync_penalty: sync,
            tf_decay: decay,
            cascade_index_fallback: 50.0,
            entry_veto_threshold: 80.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct L7AssetRank {
    pub slope: f64,
    pub offset: f64,
}

impl Default for L7AssetRank {
    fn default() -> Self {
        Self {
            slope: 0.5,
            offset: 50.0,
        }
    }
}

// ─── TAE ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeParams {
    pub intake: TaeIntake,
    pub lifecycle: TaeLifecycle,
    pub sizing: TaeSizing,
    pub execution: TaeExecution,
    pub risk: TaeRisk,
    pub recovery: TaeRecovery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeIntake {
    pub min_net_rr: f64,
    pub min_score: Option<f64>,
    pub min_confidence: Option<f64>,
    pub max_setup_age_bars: Option<u32>,
    pub confirmation_bars: u32,
    pub execution_veto: Vec<String>,
    pub direction_policy: String,
    pub trading_hours_utc: Option<String>,
    pub volatility_gate: Option<TaeVolGate>,
    pub funding_gate: Option<TaeFundingGate>,
}

impl Default for TaeIntake {
    fn default() -> Self {
        Self {
            min_net_rr: 0.5,
            min_score: None,
            min_confidence: None,
            max_setup_age_bars: None,
            confirmation_bars: 0,
            execution_veto: Vec::new(),
            direction_policy: "both".into(),
            trading_hours_utc: None,
            volatility_gate: None,
            funding_gate: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeVolGate {
    pub min_hv_pct: Option<f64>,
    pub max_hv_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeFundingGate {
    /// Skip longs when funding below this (crowded long); skip shorts
    /// above `-this` (crowded short). null = no gate.
    pub extreme_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeLifecycle {
    pub max_open_positions: u32,
    pub max_per_setup_type: std::collections::HashMap<String, u32>,
    pub max_per_direction: std::collections::HashMap<String, u32>,
    pub pending_entry_expiry_bars: Option<u32>,
    pub reentry_cooldown_bars: u32,
    /// v10: behavior when a different setup type tops the ranking while an
    /// entry is pending: `cancel_and_adopt` (cancel old, accept the
    /// replacement same tick) | `cancel` (v9 behavior).
    pub replace_policy: String,
    /// v10: minimum price move (in source-TF ATR multiples) before a
    /// pending entry is re-priced to a fresh same-direction setup
    /// (0 = re-price every completed candle).
    pub min_reprice_delta_atr: f64,
    pub daily: TaeDaily,
}

impl Default for TaeLifecycle {
    fn default() -> Self {
        Self {
            max_open_positions: 10,
            max_per_setup_type: std::collections::HashMap::new(),
            max_per_direction: std::collections::HashMap::new(),
            pending_entry_expiry_bars: None,
            reentry_cooldown_bars: 1,
            replace_policy: "cancel_and_adopt".into(),
            min_reprice_delta_atr: 0.25,
            daily: TaeDaily::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeDaily {
    pub max_trades: Option<u32>,
    pub max_loss_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeSizing {
    pub allocation_pct: f64,
    pub per_setup_type_multipliers: std::collections::HashMap<String, f64>,
    /// Per-SetupQuality allocation map (null = flat allocation).
    pub quality_curve: Option<std::collections::HashMap<String, f64>>,
    pub after_loss_step_down: Option<TaeStepDown>,
    pub max_position_size_pct_of_equity: Option<f64>,
    pub max_total_exposure_pct: Option<f64>,
    pub vol_scale: TaeVolScale,
}

impl Default for TaeSizing {
    fn default() -> Self {
        Self {
            allocation_pct: 10.0,
            per_setup_type_multipliers: std::collections::HashMap::new(),
            quality_curve: None,
            after_loss_step_down: None,
            max_position_size_pct_of_equity: None,
            max_total_exposure_pct: None,
            vol_scale: TaeVolScale::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeStepDown {
    pub after_losses: u32,
    pub reduce_pct: f64,
}

impl Default for TaeStepDown {
    fn default() -> Self {
        Self {
            after_losses: 3,
            reduce_pct: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeVolScale {
    /// `auto` (ATR-history computed per instance) | `fixed`.
    pub mode: String,
    pub override_factor: Option<f64>,
}

impl Default for TaeVolScale {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            override_factor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeExecution {
    /// `zone_midpoint` | `zone_edge` | `zone_any` | `market_on_ready` |
    /// `chase` (v10 — wired). `chase` submits a marketable limit when the
    /// mid is within `chase_max_atr × ATR` of the entry zone AND the setup
    /// scores ≥ `chase_score_floor` (conditional tolerance).
    pub entry_mode: String,
    pub spread_gate_bps: Option<f64>,
    pub slippage_bps: f64,
    /// `take_better` | `cancel` (v10 — wired). `cancel` refuses the entry
    /// when the limit would be marketable beyond the zone at dispatch.
    pub instant_fill_policy: String,
    /// v10: max distance (in source-TF ATR multiples) at which the
    /// `chase` entry mode may still chase the zone.
    pub chase_max_atr: f64,
    /// v10: minimum setup score for the `chase` entry mode (0..=100).
    pub chase_score_floor: f64,
    /// v10: where inside the target zone the 100 % TP limit sits:
    /// `zone_near_edge` (conservative) | `zone_midpoint` | `zone_far_edge`.
    pub tp_placement: String,
    /// v11: TP reachability cap — `TP = entry ± min(net_rr, max_tp_rr) × SL_distance`.
    /// Reachable targets for short TFs; default 1.5 reproduces the v10.3 T2 fix.
    pub max_tp_rr: f64,
}

impl Default for TaeExecution {
    fn default() -> Self {
        Self {
            entry_mode: "zone_midpoint".into(),
            spread_gate_bps: None,
            slippage_bps: 5.0,
            instant_fill_policy: "take_better".into(),
            chase_max_atr: 0.5,
            chase_score_floor: 75.0,
            tp_placement: "zone_midpoint".into(),
            max_tp_rr: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaeRisk {
    pub invalidate_on: Vec<String>,
    /// v10 — wired: close the position when the source-snapshot confidence
    /// fell by ≥ pct (percentage points) vs the entry confidence.
    pub confidence_drop_pct: Option<f64>,
    pub breakeven_at_rr: Option<f64>,
    pub trailing: Option<TaeTrailing>,
    pub time_stop_bars: Option<u32>,
    /// `market` (today) | `pullback`.
    pub signal_exit: String,
    /// v10 posture for setup-gone management (config + UI tri-state):
    /// `balanced` (pending: keep until bar-expiry; open: hold behind
    /// SL/TP/time-stop) | `strict` (cancel pending / close open at market,
    /// `setup_gone`) | `risky` (pending: immortal; open: exit only on
    /// opposite-direction flip). Default = `balanced`.
    pub setup_gone_policy: String,
    /// v10: TP refresh on an open position only when the fresh setup
    /// improves net RR by ≥ this delta (0 = never refresh).
    pub tp_refresh_min_rr_delta: f64,
    /// v10 SL placement mode: `invalidation` (exact level) |
    /// `invalidation_padded` (level ± `sl_padding_atr × ATR`) |
    /// `atr_anchored` (entry ∓ `atr_anchor_mult × ATR`).
    pub sl_mode: String,
    /// v10: ATR padding applied to the invalidation level under
    /// `sl_mode = invalidation_padded` (≥ 0; 0 = exact level).
    pub sl_padding_atr: f64,
    /// v10: ATR multiplier under `sl_mode = atr_anchored` (≥ 0.1).
    pub atr_anchor_mult: f64,
    /// v10 strict guard: skip the trade when the invalidation level sits
    /// closer than `min_sl_atr × ATR` to the entry (null = never skip).
    pub min_sl_atr: Option<f64>,
    /// v11 stop floor — `l6_formula` (SL = max(zone, L6 stop distance)) |
    /// `atr_mult` (SL = max(zone, k×ATR(stop_tf))) | `zone_only` (legacy).
    pub stop_floor_source: String,
    /// v11 ATR multiplier when `stop_floor_source = atr_mult`.
    pub stop_floor_atr_mult: f64,
}

impl Default for TaeRisk {
    fn default() -> Self {
        Self {
            invalidate_on: vec!["direction_flip".into()],
            confidence_drop_pct: None,
            breakeven_at_rr: None,
            trailing: None,
            time_stop_bars: None,
            signal_exit: "market".into(),
            setup_gone_policy: "balanced".into(),
            tp_refresh_min_rr_delta: 0.3,
            sl_mode: "invalidation".into(),
            sl_padding_atr: 0.0,
            atr_anchor_mult: 1.5,
            min_sl_atr: None,
            stop_floor_source: "l6_formula".into(),
            stop_floor_atr_mult: 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeTrailing {
    pub activate_at_rr: Option<f64>,
    pub atr_mult: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct TaeRecovery {
    pub stale_state_window_secs: Option<u64>,
}

// ─── PME ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct PmeParams {
    pub safety: PmeSafety,
    pub exposure: PmeExposure,
    pub capital: PmeCapital,
    pub enforce_systemic_veto: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PmeSafety {
    pub max_daily_drawdown_pct: f64,
    pub drawdown_limit_pct: f64,
    pub consecutive_loss_caution: u32,
    pub consecutive_loss_dropout: u32,
    pub dropout_duration_hours: u64,
    /// `per_symbol` (today) | `portfolio`.
    pub loss_streak_scope: String,
    pub warn_extra_trigger_pct: Option<f64>,
    pub drawdown_stop_release: PmeRelease,
}

impl PmeSafety {
    /// v9: PME safety envelope → the runtime `SafetyConfig` the instance's
    /// SafetyManager is built from (single source of truth = strategy).
    /// The systemic-risk threshold stays a workspace/platform concern.
    pub fn to_safety_config(&self, systemic_risk_threshold: f64) -> crate::SafetyConfig {
        crate::SafetyConfig {
            consecutive_loss_caution: self.consecutive_loss_caution,
            consecutive_loss_dropout: self.consecutive_loss_dropout,
            dropout_duration_hours: self.dropout_duration_hours,
            drawdown_limit_pct: self.drawdown_limit_pct,
            max_daily_drawdown_pct: self.max_daily_drawdown_pct,
            systemic_risk_threshold,
            session_reset_cron: None,
        }
    }
}

impl Default for PmeSafety {
    fn default() -> Self {
        Self {
            max_daily_drawdown_pct: 5.0,
            drawdown_limit_pct: 30.0,
            consecutive_loss_caution: 3,
            consecutive_loss_dropout: 5,
            dropout_duration_hours: 8,
            loss_streak_scope: "per_symbol".into(),
            warn_extra_trigger_pct: None,
            drawdown_stop_release: PmeRelease::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PmeRelease {
    /// `manual` (today) | `auto_after_hours` | `auto_on_recovery`.
    pub mode: String,
    pub after_hours: Option<u64>,
    pub recovery_pct: Option<f64>,
    pub rebaseline_peak_on_release: bool,
}

impl Default for PmeRelease {
    fn default() -> Self {
        Self {
            mode: "manual".into(),
            after_hours: None,
            recovery_pct: None,
            rebaseline_peak_on_release: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PmeExposure {
    pub max_single_pair_exposure_pct: f64,
    pub max_portfolio_exposure_pct: f64,
    pub max_correlation: f64,
    /// Veto off by default, configurable on (TAE intake gates).
    pub enforce: PmeEnforce,
}

impl Default for PmeExposure {
    fn default() -> Self {
        Self {
            max_single_pair_exposure_pct: 20.0,
            max_portfolio_exposure_pct: 50.0,
            max_correlation: 0.8,
            enforce: PmeEnforce::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct PmeEnforce {
    pub single_pair: bool,
    pub portfolio: bool,
    pub correlation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct PmeCapital {
    pub margin_alert_bands: PmeMarginBands,
    pub enforce_margin_close_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PmeMarginBands {
    pub warning: f64,
    pub close_only: f64,
    pub emergency: f64,
}

impl Default for PmeMarginBands {
    fn default() -> Self {
        Self {
            warning: 0.80,
            close_only: 0.95,
            emergency: 1.00,
        }
    }
}

// ─── PAE ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct PaeParams {
    pub verdict: PaeVerdict,
    pub risk_math: PaeRiskMath,
    pub regimes: PaeRegimes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LadderRoles {
    pub enabled: bool,
    pub decision_tf: String,
    pub entry_tf: String,
    pub stop_tf: String,
    pub target_tf: String,
}

impl Default for LadderRoles {
    fn default() -> Self {
        Self {
            enabled: false,
            // v11.9 duration labels: decision/stop anchor on the slowest
            // supported duration (1d), entry/target on the fastest (1s).
            // When the configured duration is not ACTIVE the synthesis
            // falls back to the slowest ACTIVE duration.
            decision_tf: "1d".into(),
            entry_tf: "1s".into(),
            stop_tf: "1d".into(),
            target_tf: "1s".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaeVerdict {
    pub alpha: f64,
    pub monte_carlo_runs: u32,
    pub min_trades_for_verdict: u32,
    pub min_profit_factor: Option<f64>,
    pub min_expectancy: Option<f64>,
    pub edge_classification: PaeEdgeClassification,
}

impl Default for PaeVerdict {
    fn default() -> Self {
        Self {
            alpha: 0.05,
            monte_carlo_runs: 10_000,
            min_trades_for_verdict: 30,
            min_profit_factor: None,
            min_expectancy: None,
            edge_classification: PaeEdgeClassification::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaeEdgeClassification {
    pub strong: PaeEdgeTier,
    pub moderate: PaeEdgeTier,
    pub weak: PaeEdgeTier,
}

impl Default for PaeEdgeClassification {
    fn default() -> Self {
        Self {
            strong: PaeEdgeTier {
                profit_factor_min: Some(1.2),
                win_rate_min: Some(50.0),
                p_max: 0.01,
            },
            moderate: PaeEdgeTier {
                profit_factor_min: Some(1.5),
                win_rate_min: Some(45.0),
                p_max: 0.05,
            },
            weak: PaeEdgeTier {
                profit_factor_min: Some(1.0),
                win_rate_min: None,
                p_max: 0.10,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaeEdgeTier {
    pub profit_factor_min: Option<f64>,
    pub win_rate_min: Option<f64>,
    pub p_max: f64,
}

impl Default for PaeEdgeTier {
    fn default() -> Self {
        Self {
            profit_factor_min: None,
            win_rate_min: None,
            p_max: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaeRiskMath {
    pub risk_free_rate_pct: f64,
}

impl Default for PaeRiskMath {
    fn default() -> Self {
        Self {
            risk_free_rate_pct: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaeRegimes {
    pub min_regime_sample_trades: u32,
}

impl Default for PaeRegimes {
    fn default() -> Self {
        Self {
            min_regime_sample_trades: 5,
        }
    }
}

// ─── The strategy container ─────────────────────────────────────

/// One named model. `base` enables patch inheritance; every field
/// explicitly present in the strategy's JSON overrides the base, all
/// others inherit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrategyConfig {
    pub schema_version: u32,
    pub name: String,
    pub base: Option<String>,
    pub description: String,
    pub l1: L1Params,
    pub l1_5: L1_5Params,
    pub l2: L2Params,
    pub l2_5: L2_5Params,
    pub l3: L3Params,
    pub l4: L4Params,
    pub l5: L5Params,
    pub l6: L6Params,
    pub l7: L7Params,
    pub tae: TaeParams,
    pub pme: PmeParams,
    pub pae: PaeParams,
    #[serde(default)]
    pub ladder_roles: LadderRoles,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            name: "default".into(),
            base: None,
            description: "The platform baseline model — reproduces v8.2 behavior exactly.".into(),
            l1: L1Params::default(),
            l1_5: L1_5Params::default(),
            l2: L2Params::default(),
            l2_5: L2_5Params::default(),
            l3: L3Params::default(),
            l4: L4Params::default(),
            l5: L5Params::default(),
            l6: L6Params::default(),
            l7: L7Params::default(),
            tae: TaeParams::default(),
            pme: PmeParams::default(),
            pae: PaeParams::default(),
            ladder_roles: LadderRoles::default(),
        }
    }
}

impl StrategyConfig {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Deep-merge a base strategy's JSON under a child's raw JSON
    /// (explicit keys win; missing keys inherit), then parse. This is the
    /// patch-inheritance contract.
    pub fn resolve(
        base_json: Option<&serde_json::Value>,
        child_json: &serde_json::Value,
    ) -> Result<Self, String> {
        let merged = match base_json {
            Some(base) => deep_merge(base, child_json),
            None => child_json.clone(),
        };
        serde_json::from_value(merged).map_err(|e| format!("invalid strategy JSON: {e}"))
    }

    /// Structural + coherence validation. Returns human-readable problems
    /// (the API surfaces these as warnings on save).
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.schema_version != Self::SCHEMA_VERSION {
            problems.push(format!(
                "schema_version {} unsupported (expected {})",
                self.schema_version,
                Self::SCHEMA_VERSION
            ));
        }
        if self.name.trim().is_empty() {
            problems.push("strategy name must not be empty".into());
        }
        // Coherence: a setup enabled whose precondition inputs are
        // obviously absent is a warning-level check the API layer extends
        // with activation-set knowledge; here we check internal sanity.
        if self.tae.intake.min_net_rr < 0.0 {
            problems.push("tae.intake.min_net_rr must be ≥ 0".into());
        }
        if !(1.0..=100.0).contains(&self.tae.sizing.allocation_pct) {
            problems.push("tae.sizing.allocation_pct must be 1..=100".into());
        }
        // ── v10 TAE dial validation ──
        const SETUP_GONE: &[&str] = &["balanced", "strict", "risky"];
        const ENTRY_MODES: &[&str] = &[
            "zone_midpoint",
            "zone_edge",
            "zone_any",
            "market_on_ready",
            "chase",
        ];
        const FILL_POLICIES: &[&str] = &["take_better", "cancel"];
        const REPLACE_POLICIES: &[&str] = &["cancel_and_adopt", "cancel"];
        const SL_MODES: &[&str] = &["invalidation", "invalidation_padded", "atr_anchored"];
        const TP_PLACEMENTS: &[&str] = &["zone_near_edge", "zone_midpoint", "zone_far_edge"];
        if !SETUP_GONE.contains(&self.tae.risk.setup_gone_policy.as_str()) {
            problems.push(format!(
                "tae.risk.setup_gone_policy must be one of {SETUP_GONE:?}"
            ));
        }
        if !ENTRY_MODES.contains(&self.tae.execution.entry_mode.as_str()) {
            problems.push(format!(
                "tae.execution.entry_mode must be one of {ENTRY_MODES:?}"
            ));
        }
        if !FILL_POLICIES.contains(&self.tae.execution.instant_fill_policy.as_str()) {
            problems.push(format!(
                "tae.execution.instant_fill_policy must be one of {FILL_POLICIES:?}"
            ));
        }
        if !REPLACE_POLICIES.contains(&self.tae.lifecycle.replace_policy.as_str()) {
            problems.push(format!(
                "tae.lifecycle.replace_policy must be one of {REPLACE_POLICIES:?}"
            ));
        }
        if !SL_MODES.contains(&self.tae.risk.sl_mode.as_str()) {
            problems.push(format!("tae.risk.sl_mode must be one of {SL_MODES:?}"));
        }
        if !TP_PLACEMENTS.contains(&self.tae.execution.tp_placement.as_str()) {
            problems.push(format!(
                "tae.execution.tp_placement must be one of {TP_PLACEMENTS:?}"
            ));
        }
        if self.tae.risk.tp_refresh_min_rr_delta < 0.0 {
            problems.push("tae.risk.tp_refresh_min_rr_delta must be ≥ 0".into());
        }
        if self.tae.risk.sl_padding_atr < 0.0 {
            problems.push("tae.risk.sl_padding_atr must be ≥ 0".into());
        }
        if self.tae.risk.atr_anchor_mult < 0.1 {
            problems.push("tae.risk.atr_anchor_mult must be ≥ 0.1".into());
        }
        if self.tae.risk.min_sl_atr.is_some_and(|k| k < 0.0) {
            problems.push("tae.risk.min_sl_atr must be ≥ 0".into());
        }
        if self
            .tae
            .risk
            .confidence_drop_pct
            .is_some_and(|p| !(0.0..=100.0).contains(&p))
        {
            problems.push("tae.risk.confidence_drop_pct must be 0..=100".into());
        }
        if self.tae.execution.chase_max_atr <= 0.0 {
            problems.push("tae.execution.chase_max_atr must be > 0".into());
        }
        if !(0.0..=100.0).contains(&self.tae.execution.chase_score_floor) {
            problems.push("tae.execution.chase_score_floor must be 0..=100".into());
        }
        if self.tae.lifecycle.min_reprice_delta_atr < 0.0 {
            problems.push("tae.lifecycle.min_reprice_delta_atr must be ≥ 0".into());
        }
        if self.tae.execution.spread_gate_bps.is_some_and(|b| b < 0.0) {
            problems.push("tae.execution.spread_gate_bps must be ≥ 0".into());
        }
        let alpha_pct = self.pae.verdict.alpha * 100.0;
        if !(0.0..=100.0).contains(&alpha_pct) || self.pae.verdict.alpha <= 0.0 {
            problems.push("pae.verdict.alpha must be in (0, 1]".into());
        }
        problems
    }
}

/// Recursive JSON merge: `child` keys override `base` keys; objects merge
/// recursively; everything else (arrays, scalars, null) replaces.
fn deep_merge(base: &serde_json::Value, child: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (base, child) {
        (Value::Object(b), Value::Object(c)) => {
            let mut out = b.clone();
            for (k, v) in c {
                match out.get_mut(k) {
                    Some(existing) => {
                        let merged = deep_merge(existing, v);
                        *existing = merged;
                    }
                    None => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            Value::Object(out)
        }
        (_, c) => c.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_strategy_round_trips() {
        let s = StrategyConfig::default();
        let json = serde_json::to_value(&s).unwrap();
        let back: StrategyConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.schema_version, 1);
        assert_eq!(back.name, "default");
        assert!(back.validate().is_empty());
    }

    #[test]
    fn minimal_json_parses_to_defaults() {
        let raw: serde_json::Value = serde_json::json!({
            "name": "custom",
            "tae": { "sizing": { "allocation_pct": 25.0 } }
        });
        let resolved = StrategyConfig::resolve(None, &raw).unwrap();
        assert_eq!(resolved.tae.sizing.allocation_pct, 25.0);
        // Untouched sections inherit defaults.
        assert_eq!(resolved.l6.stance.risk.avoid, 90.0);
        assert_eq!(resolved.l1.context.trend_momentum_blend, [0.6, 0.4]);
        assert_eq!(resolved.pae.verdict.alpha, 0.05);
    }

    #[test]
    fn base_inheritance_patches() {
        let base: serde_json::Value = serde_json::json!({
            "name": "base",
            "l6": { "stop": { "base_pct": 3.0 } },
            "l4": { "setups": { "enabled": ["TrendContinuation"] } }
        });
        let child: serde_json::Value = serde_json::json!({
            "name": "child",
            "l6": { "stop": { "final_clamp": [1.0, 10.0] } }
        });
        let resolved = StrategyConfig::resolve(Some(&base), &child).unwrap();
        // Explicit child key overrides; sibling key from base survives.
        assert_eq!(resolved.l6.stop.base_pct, 3.0);
        assert_eq!(resolved.l6.stop.final_clamp, [1.0, 10.0]);
        assert_eq!(
            resolved.l4.setups.enabled,
            vec!["TrendContinuation".to_string()]
        );
        // Untouched sections inherit the default.
        assert_eq!(resolved.l5.bands, [80.0, 60.0, 40.0, 20.0]);
    }

    #[test]
    fn disable_friendly_defaults() {
        let s = StrategyConfig::default();
        assert!(s.tae.intake.min_score.is_none());
        assert!(s.tae.lifecycle.pending_entry_expiry_bars.is_none());
        assert!(s.tae.risk.trailing.is_none());
        assert!(s.tae.sizing.quality_curve.is_none());
        assert!(s.l6.risk_ceiling.max_overall_risk.is_none());
        assert!(s.l7.breadth_entry_floor.is_none());
        assert!(!s.pme.exposure.enforce.single_pair);
        assert!(!s.pme.enforce_systemic_veto);
        // v10: the posture is the one deliberate semantic bump (balanced);
        // every individual dial remains disable-friendly.
        assert_eq!(s.tae.risk.setup_gone_policy, "balanced");
        assert!(s.tae.risk.min_sl_atr.is_none());
        assert!(s.tae.risk.confidence_drop_pct.is_none());
    }

    #[test]
    fn v10_dial_defaults() {
        let s = StrategyConfig::default();
        assert_eq!(s.tae.execution.entry_mode, "zone_midpoint");
        assert_eq!(s.tae.execution.instant_fill_policy, "take_better");
        assert_eq!(s.tae.execution.chase_max_atr, 0.5);
        assert_eq!(s.tae.execution.chase_score_floor, 75.0);
        assert_eq!(s.tae.execution.tp_placement, "zone_midpoint");
        assert_eq!(s.tae.lifecycle.replace_policy, "cancel_and_adopt");
        assert_eq!(s.tae.lifecycle.min_reprice_delta_atr, 0.25);
        assert_eq!(s.tae.risk.tp_refresh_min_rr_delta, 0.3);
        assert_eq!(s.tae.risk.sl_mode, "invalidation");
        assert_eq!(s.tae.risk.sl_padding_atr, 0.0);
        assert_eq!(s.tae.risk.atr_anchor_mult, 1.5);
    }

    #[test]
    fn v10_validation_rejects_bad_enums() {
        let mut s = StrategyConfig::default();
        s.tae.risk.setup_gone_policy = "wild".into();
        s.tae.execution.entry_mode = "limit_at_moon".into();
        s.tae.execution.instant_fill_policy = "maybe".into();
        s.tae.lifecycle.replace_policy = "keep_both".into();
        s.tae.risk.sl_mode = "vibes".into();
        s.tae.execution.tp_placement = "anywhere".into();
        let problems = s.validate();
        assert!(problems.len() >= 6);
        assert!(problems.iter().any(|p| p.contains("setup_gone_policy")));
        assert!(problems.iter().any(|p| p.contains("entry_mode")));
        assert!(problems.iter().any(|p| p.contains("instant_fill_policy")));
        assert!(problems.iter().any(|p| p.contains("replace_policy")));
        assert!(problems.iter().any(|p| p.contains("sl_mode")));
        assert!(problems.iter().any(|p| p.contains("tp_placement")));
    }

    #[test]
    fn v10_validation_rejects_bad_ranges() {
        let mut s = StrategyConfig::default();
        s.tae.risk.tp_refresh_min_rr_delta = -0.1;
        s.tae.risk.sl_padding_atr = -1.0;
        s.tae.risk.atr_anchor_mult = 0.0;
        s.tae.execution.chase_max_atr = 0.0;
        s.tae.execution.chase_score_floor = 250.0;
        s.tae.lifecycle.min_reprice_delta_atr = -0.5;
        let problems = s.validate();
        assert!(problems
            .iter()
            .any(|p| p.contains("tp_refresh_min_rr_delta")));
        assert!(problems.iter().any(|p| p.contains("sl_padding_atr")));
        assert!(problems.iter().any(|p| p.contains("atr_anchor_mult")));
        assert!(problems.iter().any(|p| p.contains("chase_max_atr")));
        assert!(problems.iter().any(|p| p.contains("chase_score_floor")));
        assert!(problems.iter().any(|p| p.contains("min_reprice_delta_atr")));
    }

    #[test]
    fn v10_knobs_round_trip_through_json() {
        let s = StrategyConfig::default();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(
            json["tae"]["risk"]["setup_gone_policy"],
            serde_json::json!("balanced")
        );
        let back: StrategyConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, s);
    }
}
