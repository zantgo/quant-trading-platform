//! # Domain Data Models
//!
//! This module defines the common data structures representing market telemetry.
//! It includes raw ticker prices, consolidated candle bars, and the unified
//! dual-representation normalized indicator map (v2.0).

use crate::indicator_dtos::{IndicatorLifecycleMap, NormalizedIndicatorValue};
use crate::liquidity::{LiquidationClusterMatrix, LiquidityFlow, LiquiditySignal};
use crate::normalized::Exchange;
use crate::volume_profile::VolumeProfileSnapshot;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_indicators: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_signals: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_signal_kinds: Vec<String>,
    #[serde(default)]
    pub liquidity: LiquidityActivation,
    pub config_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityActivation {
    pub enabled: bool,
    pub liquidation_feed: bool,
    pub cluster_estimation: bool,
    pub signals: bool,
}

impl Default for LiquidityActivation {
    fn default() -> Self {
        Self {
            enabled: true,
            liquidation_feed: true,
            cluster_estimation: true,
            signals: true,
        }
    }
}

/// v11.9: duration-keyed timeframe identity. A timeframe IS its duration
/// in seconds — there are no named slots. The supported pool is the closed
/// 14-duration set below; activation is any subset of 1..=14 durations.
pub const SUPPORTED_DURATIONS: [u64; 14] = [
    1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
];

/// Canonical display label for a supported duration ("1s".."1d").
/// Durations outside the pool resolve to "<secs>s".
pub fn duration_label(secs: u64) -> String {
    match secs {
        1 => "1s".into(),
        3 => "3s".into(),
        5 => "5s".into(),
        15 => "15s".into(),
        30 => "30s".into(),
        60 => "1m".into(),
        180 => "3m".into(),
        300 => "5m".into(),
        900 => "15m".into(),
        1800 => "30m".into(),
        3600 => "1h".into(),
        14400 => "4h".into(),
        43200 => "12h".into(),
        86400 => "1d".into(),
        other => format!("{}s", other),
    }
}

/// Uppercase display label ("1S", "5M", "1H", "1D" style) for table headers.
pub fn duration_label_upper(secs: u64) -> String {
    duration_label(secs).to_uppercase()
}

/// True when `secs` is a member of the supported pool.
pub fn is_supported_duration(secs: u64) -> bool {
    SUPPORTED_DURATIONS.contains(&secs)
}

/// Derive the canonical timeframe label (canonical alias of
/// [`duration_label`]).
pub fn slot_label_for(secs: u64) -> String {
    duration_label(secs)
}

/// Reverse of [`duration_label`]: parse a canonical label ("1s".."1d") or
/// a raw seconds string ("60s" / "60") back into seconds. `None` when the
/// label is not recognized.
pub fn duration_from_label(label: &str) -> Option<u64> {
    let trimmed = label.trim();
    if let Some(secs) = SUPPORTED_DURATIONS
        .iter()
        .copied()
        .find(|&secs| duration_label(secs).eq_ignore_ascii_case(trimmed))
    {
        return Some(secs);
    }
    trimmed
        .strip_suffix('s')
        .unwrap_or(trimmed)
        .parse::<u64>()
        .ok()
}

/// Per-timeframe pipeline lifecycle state. One value per `(instance, slot)`.
/// Published on every emitted `MarketSnapshot` as `pipeline_state`. The most-
/// severe aggregate of its 50 per-indicator states (severity ordering:
/// `Failed > Stale > Loading > Live`), gated by the parent `ConnectionStatus`.
/// See `docs/engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md`
/// DCP-01 … DCP-15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandlePipelineState {
    /// Transient state from `TimeframePipeline::new()` to the moment bootstrap
    /// returns. No `MarketSnapshot` is emitted in this state.
    #[default]
    Initializing,
    /// Bootstrap returned; the pipeline has 0..`size` candles and is
    /// accumulating live trades. Every indicator starts in `Loading`.
    Loading,
    /// Buffer is at `size` AND parent `ConnectionStatus = Connected` AND all
    /// 52 indicators are ≥ `Live`. The chart paints with full history.
    Live,
    /// Pipeline was `Live` and then no completed candle for
    /// `candle_buffer.stale_threshold_secs`. Recovers to `Live` on the next
    /// completed candle (live or reconstructed).
    Stale,
    /// Bootstrap elected cold-fail, OR parent `ConnectionStatus = Failed` for
    /// > `FailedThreshold`, OR a non-self-recoverable calculator panic
    /// > propagated to the pipeline. `reload_timeframe` is the only recovery
    /// > path (DCP-14).
    Failed,
}

impl CandlePipelineState {
    /// Severity ranking per DCP-10. Higher = more severe.
    pub fn severity(self) -> u8 {
        match self {
            CandlePipelineState::Live => 0,
            CandlePipelineState::Loading => 1,
            CandlePipelineState::Stale => 2,
            CandlePipelineState::Failed => 3,
            // `Initializing` is transient and never compared.
            CandlePipelineState::Initializing => 1,
        }
    }

    /// Most-severe aggregation across an iterator (DCP-10).
    pub fn most_severe<'a>(
        iter: impl IntoIterator<Item = &'a CandlePipelineState>,
    ) -> CandlePipelineState {
        let mut best = CandlePipelineState::Live;
        for s in iter {
            if s.severity() > best.severity() {
                best = *s;
            }
        }
        best
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange: Option<Exchange>,
    /// v11.9: stable duration label ("1s".."1d") of the pipeline that
    /// produced this snapshot, derived from `timeframe_secs`. Carried on
    /// every wire payload so consumers never re-derive the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe_label: Option<String>,
    pub timeframe_secs: u64,
    pub timestamp: u64,
    pub symbol: String,
    pub is_completed: Option<bool>,
    pub mid_price: Decimal,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub bid_size: Option<Decimal>,
    pub ask_size: Option<Decimal>,
    pub funding_rate: Option<Decimal>,

    // Consolidated Candle OHLC Bars (core, non-indicator telemetry)
    pub open: Option<Decimal>,
    pub high: Option<Decimal>,
    pub low: Option<Decimal>,
    pub close: Option<Decimal>,
    pub volume: Option<Decimal>,
    pub average_volume: Option<Decimal>,

    /// Per-timeframe pipeline lifecycle state. Always populated on every
    /// emitted snapshot. Severity-aggregated from the 50 per-indicator
    /// states below. See
    /// `docs/engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md`
    /// for the full state machine (DCP-01 … DCP-15).
    #[serde(default)]
    pub pipeline_state: CandlePipelineState,

    /// Per-indicator operational lifecycle map. Keys match `indicators` keys.
    /// Always populated for active-set indicators; disabled indicators are
    /// absent from both maps (ILS-12). See
    /// `docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md`.
    #[serde(default)]
    pub indicator_lifecycle: IndicatorLifecycleMap,

    /// Unified dual-representation indicator map.
    ///
    /// Each entry pairs a raw value, a `[-1.0, 1.0]` normalized score, and a
    /// context-aware state label. Keys: `rsi`, `macd`, `squeeze`, `adx`,
    /// `bbwp`, `rvol`, `ema_stack`, `vwap`, `fibonacci`, `patterns`,
    /// `support_resistance` (plus auxiliary chart series carried in `values`).
    #[serde(default)]
    pub indicators: HashMap<String, NormalizedIndicatorValue>,

    /// Synthesized higher-level market context (trend/momentum/volatility/
    /// volume/liquidity/regime + overall). Populated for completed snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<crate::market_context::MarketContext>,

    /// Cross-timeframe Alignment Matrix per symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<crate::alignment::AlignmentMatrix>,

    /// Analysis Matrix per symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<crate::analysis::AnalysisMatrix>,

    /// Market risk assessment per symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<crate::risk::RiskMatrix>,

    /// Advisory guidance per symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisory: Option<crate::advisory::AdvisoryMatrix>,

    /// Open Interest value at snapshot time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_interest: Option<Decimal>,

    /// 1-hour Open Interest delta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oi_delta_1h: Option<Decimal>,

    /// Mark price (perpetual mark for margin + liquidation price computation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_price: Option<Decimal>,

    /// Index price (underlying spot composite).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_price: Option<Decimal>,

    /// Mark-vs-index spread in percent (positive = perp premium).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_index_spread_pct: Option<f64>,

    /// Previous day price (from asset context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_day_px: Option<Decimal>,

    /// Statistical intelligence context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statistical_context: Option<StatisticalContext>,

    /// Decision context from the decision_context module.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_context: Option<crate::decision_context::DecisionContext>,

    /// Opportunity matrix (L4). Populated for completed snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity: Option<crate::opportunity::OpportunityMatrix>,

    /// Liquidity signals (Phase 3). Per-snapshot derived from L1.5 + L2.5.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub liquidity_signals: Vec<LiquiditySignal>,

    /// Metrics config block — present only when the active indicator/signal
    /// set differs from the registry default (all enabled). Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics_config: Option<MetricsConfig>,

    /// Risk profile ID (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_profile: Option<i32>,

    /// Liquidity flow matrix (Phase 1). Per-candle aggregate of real
    /// liquidation events observed on the exchange WS. `None` for live
    /// (flickering) snapshots; populated only for completed bars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity: Option<LiquidityFlow>,

    /// Estimated liquidation cluster matrix (Phase 2). Refreshed at each
    /// TF's own candle cadence (per-TF pipeline task); `valid_until_ms`
    /// grants a fixed 5-minute TTL. `None` when the data is insufficient
    /// or before the first refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<LiquidationClusterMatrix>,

    /// Per-timeframe volume profile snapshot. Recomputed on each
    /// completed candle. `None` before the analyzer has accumulated
    /// enough history, or when the candle set has zero volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_profile: Option<VolumeProfileSnapshot>,

    /// Per-candle data-quality envelope (from DIE L3). Attached to completed
    /// snapshots after validity, outlier, and gap-fill checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_envelope: Option<CandleQualityEnvelope>,
}

/// Per-candle data-quality envelope.
///
/// Attached to every `MarketSnapshot` by the DIE L3 Data Quality Layer.
/// Evaluates one candle's validity. Complementary to `PipelineReliabilityMetrics`
/// which measures the sanitization pipeline's health across a session.
///
/// See `docs/engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md` §5
/// and `docs/matrices/02-03-data-quality-matrix.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleQualityEnvelope {
    /// 0.0–100.0 composite quality score.
    /// 100.0 = fully valid (no gap, no spike, no staleness, valid integrity).
    pub quality_score: f64,
    /// Whether the candle passed all structural validity checks.
    pub is_valid: bool,
    /// Whether this candle was gap-filled (reconstructed or REST backfill).
    pub is_gap_filled: bool,
    /// Whether any outlier tick was rejected during this candle's construction.
    pub had_outliers_rejected: bool,
    /// Whether a price spike was filtered from this candle.
    #[serde(default)]
    pub spike_detected: bool,
    /// Whether the candle's last trade timestamp exceeds the staleness threshold.
    #[serde(default)]
    pub is_stale: bool,
    /// Per-candle sequence integrity classification.
    #[serde(default)]
    pub sequence_integrity: SequenceIntegrity,
    /// Seconds since the last valid candle (≤ timeframe_secs = continuous).
    #[serde(default)]
    pub gap_since_last: u64,
    /// Unix epoch of quality validation, in milliseconds.
    #[serde(default)]
    pub validated_at: u64,
}

/// Per-candle sequence ordering classification produced by the DIE Layer 3
/// sequence audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SequenceIntegrity {
    /// Candle arrived in the expected chronological order and is not a duplicate.
    #[default]
    Valid,
    /// Candle arrived out of chronological order relative to the preceding candle.
    OutOfOrder,
    /// Candle with identical `start_time_ms` has already been processed.
    Duplicate,
}

/// Placeholder for statistical intelligence context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalContext {
    pub close_z: Option<f64>,
    pub rsi_z: Option<f64>,
    pub macd_z: Option<f64>,
    pub monte_carlo_expected: Option<f64>,
    pub monte_carlo_stdev: Option<f64>,
}

/// Legacy-compatible read accessors that reconstruct flat indicator values
/// from the nested [`MarketSnapshot::indicators`] map. These bridge existing
/// consumers (CLI, server pipeline, DB persistence) during the transition to
/// the fully nested dual-representation model.
impl MarketSnapshot {
    /// Fetch a normalized indicator entry by key.
    pub fn ind(&self, key: &str) -> Option<&NormalizedIndicatorValue> {
        self.indicators.get(key)
    }

    /// Fetch an indicator's primary raw scalar.
    pub fn ind_raw(&self, key: &str) -> Option<f64> {
        self.indicators.get(key).map(|v| v.raw_value)
    }

    /// Fetch an indicator's normalized `[-1.0, 1.0]` score.
    pub fn ind_norm(&self, key: &str) -> Option<f64> {
        self.indicators.get(key).map(|v| v.normalized)
    }

    /// Fetch an indicator's state label.
    pub fn ind_label(&self, key: &str) -> Option<&str> {
        self.indicators.get(key).map(|v| v.state_label.as_str())
    }

    /// Fetch an auxiliary raw sub-component (macd line/signal, bollinger bands).
    pub fn ind_sub(&self, key: &str, sub: &str) -> Option<f64> {
        self.indicators
            .get(key)
            .and_then(|v| v.values.as_ref())
            .and_then(|m| m.get(sub))
            .copied()
    }

    fn dec(x: Option<f64>) -> Option<Decimal> {
        x.and_then(Decimal::from_f64_retain)
    }

    // ── Raw scalar accessors (Option<Decimal>) ──
    pub fn rsi_14(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("rsi"))
    }
    pub fn stoch_k(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("stochastic", "k_line"))
    }
    pub fn stoch_d(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("stochastic", "d_line"))
    }
    pub fn chandemo(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("chandemo"))
    }
    pub fn supertrend_line(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("supertrend", "line"))
    }
    pub fn keltner_middle(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("keltner", "middle"))
    }
    pub fn donchian_upper(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("donchian", "upper"))
    }
    pub fn obv(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("obv"))
    }
    pub fn cmf(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("cmf"))
    }
    pub fn mfi(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("mfi"))
    }
    pub fn hv(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("hv"))
    }
    pub fn aroon_oscillator(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("aroon"))
    }
    pub fn choppiness(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("choppiness"))
    }
    pub fn linreg_slope(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("linreg_slope"))
    }
    pub fn zscore(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("zscore"))
    }
    pub fn macd_line(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("macd", "line"))
    }
    pub fn macd_signal(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("macd", "signal"))
    }
    pub fn macd_hist(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("macd", "histogram"))
    }
    pub fn macd_histogram_peak(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("macd", "histogram_peak"))
    }
    pub fn adx_14(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("adx", "adx"))
    }
    pub fn adx_plus(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("adx", "plus_di"))
    }
    pub fn adx_minus(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("adx", "minus_di"))
    }
    pub fn adx_slope(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("adx", "adx_slope"))
    }
    pub fn atr_14(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("atr", "atr_14"))
    }
    pub fn atr_slope(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("atr", "atr_slope"))
    }
    pub fn bb_upper(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("bollinger", "upper"))
    }
    pub fn bb_middle(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("bollinger", "middle"))
    }
    pub fn bb_lower(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("bollinger", "lower"))
    }
    pub fn bbwp(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("bbwp"))
    }
    pub fn rvol(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("rvol"))
    }
    pub fn vwap(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("vwap", "vwap"))
    }
    pub fn squeeze_momentum(&self) -> Option<Decimal> {
        Self::dec(self.ind_raw("squeeze"))
    }
    pub fn ema_fast(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("ema_stack", "fast"))
    }
    pub fn ema_medium(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("ema_stack", "medium"))
    }
    pub fn ema_slow(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("ema_stack", "slow"))
    }
    pub fn ema_long(&self) -> Option<Decimal> {
        Self::dec(self.ind_sub("ema_stack", "long"))
    }

    // ── Boolean accessors ──
    pub fn squeeze_on(&self) -> Option<bool> {
        self.ind_label("squeeze")
            .map(|l| l == "COMPRESSION_COILING")
    }
    pub fn squeeze_release_trigger(&self) -> Option<bool> {
        self.ind_label("squeeze")
            .map(|l| l.ends_with("VOLATILITY_RELEASE"))
    }
    pub fn macd_crossover_detected(&self) -> Option<bool> {
        self.ind_label("macd").map(|l| l.contains("CROSSOVER"))
    }

    // ── Fibonacci resting-level accessors (raw prices) ──
    pub fn fib_gp_top(&self) -> Option<f64> {
        self.ind_sub("fibonacci", "gp_top")
    }
    pub fn fib_gp_bottom(&self) -> Option<f64> {
        self.ind_sub("fibonacci", "gp_bottom")
    }
    pub fn fib_ext_1618(&self) -> Option<f64> {
        self.ind_sub("fibonacci", "ext_1618")
    }
    pub fn fib_ext_2618(&self) -> Option<f64> {
        self.ind_sub("fibonacci", "ext_2618")
    }
}
