// Minimal shim types (previously defined in db/queries/memory.rs)
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DecisionMemoryBufferRow {
    pub id: i64,
    pub symbol: String,
    pub timestamp: i64,
    pub regime_classification: String,
    pub orchestrator_decision: String,
    pub confidence_score: f64,
    pub eight_factor_score: i32,
    pub portfolio_risk_pct: f64,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CompletedTradesBufferRow {
    pub id: i64,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub realized_pnl: f64,
    pub roi_pct: f64,
    pub execution_score: f64,
    pub primary_mistake: String,
    pub closed_at: i64,
}

use market_analyzer::indicators::normalized::NormalizedIndicatorValue;
use market_analyzer::indicators::normalized::{SignalKind, SignalStatus};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;

/// Accept JSON number OR numeric string for a `Decimal` field.
/// Clients using TypeScript `number` will send `1000.0`, clients using
/// `Decimal.js` will send `"1000.00"`. Both must be supported.
fn deserialize_decimal_flexible<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct FlexibleDecimal;

    impl<'de> Visitor<'de> for FlexibleDecimal {
        type Value = Decimal;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a number or numeric string")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Decimal::from(v))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Decimal::from(v))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Decimal::from_f64(v).ok_or_else(|| de::Error::custom("invalid f64 for Decimal"))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Decimal::from_str(v).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(FlexibleDecimal)
}

/// Same as `deserialize_decimal_flexible` but for `Option<Decimal>` fields
/// that may be missing from the JSON payload entirely.
fn deserialize_optional_decimal_flexible<'de, D>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct OptionalFlexibleDecimal;

    impl<'de> Visitor<'de> for OptionalFlexibleDecimal {
        type Value = Option<Decimal>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "null, a number, or a numeric string")
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserialize_decimal_flexible(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionalFlexibleDecimal)
}

#[derive(Debug, Deserialize)]
pub struct SetKeyRequest {
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SetRulesRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct RulesResponse {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub api_key_configured: bool,
    pub symbols: Vec<String>,
    pub candles: config_models::CandlesConfig,
    pub indicators: config_models::IndicatorsConfig,
    pub instances: Vec<config_models::InstanceEntry>,
    pub indicator_registry: Vec<market_analyzer::indicators::IndicatorMeta>,
    pub api_failover: config_models::ApiFailoverConfig,
    /// v7.2 parity: the workspace's slow/macro timeframe defaults — the
    /// same values the registry falls back to when an instance is created
    /// without a config entry. The Launch Setup wizard derives its
    /// per-instance TF defaults from these, so GUI, CLI, and registry
    /// always agree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow_timeframe: Option<config_models::SlowTimeframeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_timeframe: Option<config_models::SlowTimeframeConfig>,
    /// v7.3: workspace liquidity config (retentions, feed toggles) —
    /// surfaced so DIE Settings can render the true retention values and
    /// the PME can derive data-retention facts from one source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity: Option<config_models::LiquidityConfig>,
    /// v7.3: v7 setup-executor config — PME "Risk per trade" and TAE
    /// surfaces render the real sizing knob instead of a hardcoded 1%.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimal_tae: Option<config_models::MinimalTaeConfig>,
    /// v7.3: PAE significance treatment (α, Monte Carlo runs, min trades).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analytics: Option<config_models::AnalyticsConfig>,
    /// v7.3: portfolio risk limits — concentration / exposure / correlation
    /// caps the PME Exposure tab renders and the backend enforces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_limits: Option<config_models::RiskLimitsConfig>,
    /// v7.3: safety ladder thresholds — the PME Safety ladder and the engine
    /// Settings tabs render the real values (previously the PME read
    /// `cfg.safety` which this response never carried, silently falling
    /// back to hardcoded defaults).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<config_models::SafetyConfig>,
    /// v7.3: fee schedule (maker/taker/funding) — TAE/PME Settings tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees: Option<config_models::FeesConfig>,
    /// v7.3: cross leverage — TAE/PME Settings tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leverage: Option<config_models::LeverageConfig>,
    /// v7.3: execution layer config (slippage ceiling) — TAE Settings tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<config_models::ExecutionConfig>,
    /// v7.4: workspace-wide indicator/signal activation defaults — the MME
    /// Workspace Settings "Indicator Activation" card falls back to these
    /// when an instance carries no per-instance `activation` override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<config_models::ActivationConfig>,
    /// v8: Backtesting Engine config (archive depth 1..=365, warmup bars,
    /// per-exchange paging limits) — BTE Settings tab + run form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backtest: Option<config_models::BacktestConfig>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub timeframe_secs: Option<u64>,
    /// AUDIT-AIU-121: optional slot hint (`micro|fast|slow|macro|custom-N`).
    /// When present the pipeline is resolved BY SLOT first, so two slots
    /// sharing one duration (which the UI permits) each get their OWN
    /// history instead of both falling back to the micro pipeline via the
    /// duration-only `pipeline_for_duration` shim.
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default = "default_history_limit")]
    pub limit: usize,
}
fn default_history_limit() -> usize {
    100
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub timeframe_secs: Option<u64>,
    /// Authoritative slot identifier (`micro`/`fast`/`slow`/`macro`).
    /// Optional for backward compatibility with older clients that
    /// identified the slot purely by `timeframe_secs`.
    #[serde(default)]
    pub slot: Option<String>,
}

/// Nested dual-representation indicator DTO (v2.0). Carries the normalized
/// indicator map plus non-indicator market context. Legacy flat accessor
/// methods reconstruct scalar values from the map for existing consumers.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct IndicatorSnapshot {
    #[serde(default)]
    pub indicators: HashMap<String, NormalizedIndicatorValue>,
    #[serde(default)]
    pub current_price: Option<f64>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub average_volume: Option<f64>,
}

impl IndicatorSnapshot {
    pub fn new(
        indicators: HashMap<String, NormalizedIndicatorValue>,
        current_price: Option<f64>,
    ) -> Self {
        Self {
            indicators,
            current_price,
            volume: None,
            average_volume: None,
        }
    }

    fn raw(&self, k: &str) -> Option<f64> {
        self.indicators.get(k).map(|v| v.raw_value)
    }
    fn sub(&self, k: &str, s: &str) -> Option<f64> {
        self.indicators
            .get(k)
            .and_then(|v| v.values.as_ref())
            .and_then(|m| m.get(s))
            .copied()
    }
    fn lbl(&self, k: &str) -> Option<&str> {
        self.indicators.get(k).map(|v| v.state_label.as_str())
    }
    fn norm(&self, k: &str) -> Option<f64> {
        self.indicators.get(k).map(|v| v.normalized)
    }

    // ── Scalar accessors (flat-equivalent) ──
    pub fn rsi(&self) -> Option<f64> {
        self.raw("rsi")
    }
    pub fn stoch_k(&self) -> Option<f64> {
        self.sub("stochastic", "k_line")
    }
    pub fn stoch_d(&self) -> Option<f64> {
        self.sub("stochastic", "d_line")
    }
    pub fn chandemo(&self) -> Option<f64> {
        self.raw("chandemo")
    }
    pub fn supertrend(&self) -> Option<f64> {
        self.sub("supertrend", "line")
    }
    pub fn keltner_middle(&self) -> Option<f64> {
        self.sub("keltner", "middle")
    }
    pub fn donchian_upper(&self) -> Option<f64> {
        self.sub("donchian", "upper")
    }
    pub fn obv(&self) -> Option<f64> {
        self.raw("obv")
    }
    pub fn cmf(&self) -> Option<f64> {
        self.raw("cmf")
    }
    pub fn mfi(&self) -> Option<f64> {
        self.raw("mfi")
    }
    pub fn hv(&self) -> Option<f64> {
        self.raw("hv")
    }
    pub fn macd_line(&self) -> Option<f64> {
        self.sub("macd", "line")
    }
    pub fn macd_signal(&self) -> Option<f64> {
        self.sub("macd", "signal")
    }
    pub fn macd_histogram(&self) -> Option<f64> {
        self.sub("macd", "histogram")
    }
    pub fn macd_histogram_peak(&self) -> Option<f64> {
        self.sub("macd", "histogram_peak")
    }
    pub fn adx(&self) -> Option<f64> {
        self.sub("adx", "adx")
    }
    pub fn adx_plus(&self) -> Option<f64> {
        self.sub("adx", "plus_di")
    }
    pub fn adx_minus(&self) -> Option<f64> {
        self.sub("adx", "minus_di")
    }
    pub fn adx_slope(&self) -> Option<f64> {
        self.sub("adx", "adx_slope")
    }
    pub fn atr(&self) -> Option<f64> {
        self.sub("atr", "atr_14")
    }
    pub fn bb_upper(&self) -> Option<f64> {
        self.sub("bollinger", "upper")
    }
    pub fn bb_middle(&self) -> Option<f64> {
        self.sub("bollinger", "middle")
    }
    pub fn bb_lower(&self) -> Option<f64> {
        self.sub("bollinger", "lower")
    }
    pub fn bbwp(&self) -> Option<f64> {
        self.raw("bbwp")
    }
    pub fn rvol(&self) -> Option<f64> {
        self.raw("rvol")
    }
    pub fn vwap(&self) -> Option<f64> {
        self.sub("vwap", "vwap")
    }
    pub fn squeeze_momentum(&self) -> Option<f64> {
        self.raw("squeeze")
    }
    pub fn ema_fast(&self) -> Option<f64> {
        self.sub("ema_stack", "fast")
    }
    pub fn ema_medium(&self) -> Option<f64> {
        self.sub("ema_stack", "medium")
    }
    pub fn ema_slow(&self) -> Option<f64> {
        self.sub("ema_stack", "slow")
    }
    pub fn ema_long(&self) -> Option<f64> {
        self.sub("ema_stack", "long")
    }
    pub fn chart_pattern_confidence(&self) -> Option<f64> {
        self.raw("patterns")
    }

    // ── Boolean accessors ──
    pub fn squeeze_on(&self) -> Option<bool> {
        self.lbl("squeeze").map(|l| l == "COMPRESSION_COILING")
    }
    pub fn squeeze_release_trigger(&self) -> Option<bool> {
        self.lbl("squeeze")
            .map(|l| l.ends_with("VOLATILITY_RELEASE"))
    }
    pub fn macd_crossover_detected(&self) -> Option<bool> {
        self.lbl("macd").map(|l| l.contains("CROSSOVER"))
    }

    // ── State-string accessors (legacy vocabulary) ──
    pub fn ema_stack_state(&self) -> Option<String> {
        self.lbl("ema_stack").map(|l| {
            if l.contains("BULLISH") {
                "bullish".into()
            } else if l.contains("BEARISH") {
                "bearish".into()
            } else {
                "tangled".into()
            }
        })
    }
    pub fn vwap_bias(&self) -> Option<String> {
        self.lbl("vwap").map(|l| {
            if l.contains("PREMIUM") {
                "premium".into()
            } else if l.contains("DISCOUNT") {
                "discount".into()
            } else {
                "equilibrium".into()
            }
        })
    }
    pub fn adx_regime(&self) -> Option<String> {
        self.lbl("adx").map(|l| {
            if l.contains("CONGESTION") {
                "congestion".into()
            } else if l.contains("EMERGING") {
                "emerging".into()
            } else if l.contains("CLIMACTIC") {
                "extreme".into()
            } else if l.contains("STRONG") {
                "strong".into()
            } else {
                "congestion".into()
            }
        })
    }
    pub fn macd_crossover_direction(&self) -> Option<String> {
        let v = self.indicators.get("macd")?;
        if !v.state_label.contains("CROSSOVER") {
            return None;
        }
        Some(
            if v.normalized >= 0.0 {
                "BULLISH"
            } else {
                "BEARISH"
            }
            .to_string(),
        )
    }
    pub fn macd_trend_state(&self) -> Option<String> {
        let hist = self.sub("macd", "histogram")?.abs();
        let peak = self.sub("macd", "histogram_peak")?.abs();
        Some(if peak > 0.0 && hist < peak {
            "decelerating".into()
        } else {
            "accelerating".into()
        })
    }
    pub fn squeeze_momentum_direction(&self) -> Option<String> {
        self.indicators.get("squeeze").map(|v| {
            let l = v.state_label.as_str();
            if l.contains("BULLISH") && v.normalized >= 0.5 {
                "BullishAcceleration".into()
            } else if l.contains("BULLISH") {
                "BullishDeceleration".into()
            } else if l.contains("BEARISH") && v.normalized <= -0.5 {
                "BearishAcceleration".into()
            } else if l.contains("BEARISH") {
                "BearishDeceleration".into()
            } else {
                "Flat".into()
            }
        })
    }
    pub fn chart_pattern(&self) -> Option<String> {
        self.indicators.get("patterns").and_then(|v| {
            if v.normalized > 0.0 {
                Some("BullishPattern".to_string())
            } else if v.normalized < 0.0 {
                Some("BearishPattern".to_string())
            } else {
                None
            }
        })
    }
    pub fn rsi_divergence_status(&self) -> Option<String> {
        divergence_from_signals(self.indicators.get("rsi"))
    }
    pub fn macd_divergence_status(&self) -> Option<String> {
        divergence_from_signals(self.indicators.get("macd"))
    }
    pub fn norm_of(&self, key: &str) -> f64 {
        self.norm(key).unwrap_or(0.0)
    }

    // ── Fields not preserved in the normalized map (return None) ──
    pub fn squeeze_duration(&self) -> Option<u32> {
        None
    }
    pub fn atr_trend(&self) -> Option<String> {
        None
    }
    pub fn atr_volatility_regime(&self) -> Option<String> {
        None
    }
    pub fn macd_histogram_trend(&self) -> Option<String> {
        None
    }
    pub fn adx_di_crossover_detected(&self) -> Option<bool> {
        None
    }
    pub fn adx_di_crossover_direction(&self) -> Option<String> {
        None
    }
}

/// Extract divergence status from a parent oscillator's signals array.
/// Divergence lives as a secondary output on the parent (e.g., "rsi"), not
/// as a separate mirror entry in the indicator map.
fn divergence_from_signals(parent: Option<&NormalizedIndicatorValue>) -> Option<String> {
    let signal = parent?
        .signals
        .iter()
        .find(|s| s.kind == SignalKind::Divergence)?;
    let direction = match signal.direction {
        market_analyzer::indicators::normalized::SignalDirection::Bullish => "bullish",
        market_analyzer::indicators::normalized::SignalDirection::Bearish => "bearish",
        _ => return None,
    };
    let status = match signal.status {
        SignalStatus::Confirmed => "confirmed",
        SignalStatus::Potential => "potential",
        _ => return None,
    };
    Some(format!("{}_{}", status, direction))
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryCandle {
    pub time: u64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    /// Reconstruction provenance. `None` means a real, persisted OHLCV
    /// candle. `Some(_)` means the candle was synthesised by a
    /// reconstruction path (e.g. an EMA gap-fill). The frontend uses this
    /// flag to filter reconstructed candles out of the persistent candle
    /// cache so the chart never paints a flat-line "ghost" from
    /// minute-close interpolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstructed: Option<core_domain::normalized::ReconstructionMethod>,
}

/// Parallel time-series arrays for a single indicator (aligned to `times`).
#[derive(Debug, Default, Serialize)]
pub struct HistoricalIndicatorArrays {
    pub raw: Vec<Option<f64>>,
    pub normalized: Vec<Option<f64>>,
    pub state_label: Vec<Option<String>>,
    pub values: HashMap<String, Vec<Option<f64>>>,
}

impl HistoricalIndicatorArrays {
    pub fn with_value_keys(value_keys: &BTreeSet<String>) -> Self {
        let mut values = HashMap::new();
        for k in value_keys {
            values.insert(k.clone(), Vec::new());
        }
        Self {
            raw: Vec::new(),
            normalized: Vec::new(),
            state_label: Vec::new(),
            values,
        }
    }

    pub fn push_value(&mut self, v: &NormalizedIndicatorValue) {
        self.raw.push(Some(v.raw_value));
        self.normalized.push(Some(v.normalized));
        self.state_label.push(Some(v.state_label.clone()));
        for (k, series) in self.values.iter_mut() {
            let sv = v.values.as_ref().and_then(|m| m.get(k)).copied();
            series.push(sv);
        }
    }

    pub fn push_none(&mut self) {
        self.raw.push(None);
        self.normalized.push(None);
        self.state_label.push(None);
        for series in self.values.values_mut() {
            series.push(None);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IndicatorHistoryArrays {
    pub symbol: String,
    pub timeframe_secs: u64,
    pub times: Vec<u64>,
    pub indicators: HashMap<String, HistoricalIndicatorArrays>,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    // AUDIT-F1: the API contract (06-01 §2.2) documents a top-level
    // `symbol` member that was never serialized.
    pub symbol: String,
    pub prices: Vec<String>,
    pub candles: Vec<HistoryCandle>,
    pub indicator_history: IndicatorHistoryArrays,
    /// v6.5: per-timeframe cluster matrices. One entry per TF slot the
    /// history was loaded for. Empty map if the analyzer hasn't computed
    /// the cluster yet (freshly started). Cost: ~2 KB per TF.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub clusters: HashMap<String, core_domain::liquidity::LiquidationClusterMatrix>,
    /// v6.5: per-timeframe volume profile snapshot (right-edge histogram).
    /// One entry per TF slot. Cost: ~2 KB per TF.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub volume_profiles: HashMap<String, core_domain::volume_profile::VolumeProfileSnapshot>,
    /// Phase 0-4: per-timeframe latest `LiquidityFlow` (per-bar real
    /// liquidation aggregates). One entry per TF slot. Cost: ~200 B
    /// per TF. Lets the dashboard render the Metrics-tab Flow / Cluster
    /// / Context cards immediately after a daemon restart, before the
    /// WS delivers the next completed bar.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub liquidity_flows: HashMap<String, core_domain::liquidity::LiquidityFlow>,
}

// ─── Terminal Monitor (cross-timeframe meta-intelligence) ──────

#[derive(Debug, Serialize)]
pub struct MonitorTimeframe {
    pub label: String,
    pub timeframe_secs: u64,
    pub regime: String,
    pub overall_score: i32,
    pub overall_label: String,
    pub confluence_score: i32,
}

#[derive(Debug, Serialize)]
pub struct MtfIndicatorRow {
    pub key: String,
    pub display_name: String,
    pub per_tf: Vec<i8>,
    pub agreement: f64,
}

#[derive(Debug, Serialize)]
pub struct MtfConfirmation {
    pub trend_agreement_pct: f64,
    pub structural_trend: String,
    pub rows: Vec<MtfIndicatorRow>,
}

#[derive(Debug, Serialize)]
pub struct MonitorResponse {
    pub symbol: String,
    pub timeframes: Vec<MonitorTimeframe>,
    pub mtf: MtfConfirmation,
    pub market_context: Option<core_domain::market_context::MarketContext>,
}

#[derive(Debug, Deserialize)]
pub struct AddTradeRequest {
    pub symbol: String,
    pub direction: String,
    pub outcome: String,
    pub risk_multiplier: f64,
    pub reward_multiplier: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatusResponse {
    pub connected: bool,
    pub latency_ms: u64,
    pub ingest_skew_ms: u64,
    pub observation_loop_latency_ms: u64,
    pub system_heartbeat_latency_ms: u64,
    pub journal_mode: String,
    pub total_allocated_margin: f64,
    pub active_pairs_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservabilityBuffersResponse {
    pub symbol: String,
    pub recent_decisions: Vec<DecisionMemoryBufferRow>,
    pub completed_trades: Vec<CompletedTradesBufferRow>,
}

// ─── Decision Profiles ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DecisionProfileCreate {
    pub profile_name: String,
    #[serde(default = "default_long_threshold")]
    pub long_threshold: i32,
    #[serde(default = "default_short_threshold")]
    pub short_threshold: i32,
}
fn default_long_threshold() -> i32 {
    15
}
fn default_short_threshold() -> i32 {
    -15
}

#[derive(Debug, Deserialize)]
pub struct DecisionProfileUpdate {
    pub profile_name: String,
    pub long_threshold: i32,
    pub short_threshold: i32,
}

#[derive(Debug, Deserialize)]
pub struct ProfileIndicatorAdd {
    pub indicator_name: String,
    #[serde(default = "default_weight")]
    pub weight: i32,
    pub override_status: String,
}
fn default_weight() -> i32 {
    10
}

#[derive(Debug, Deserialize)]
pub struct ProfileIndicatorUpdate {
    pub weight: i32,
    pub override_status: String,
}

#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub symbol: String,
    pub latest_snapshot: Option<serde_json::Value>,
    #[serde(default)]
    pub historical_prices: Option<Vec<f64>>,
    #[serde(default)]
    pub rsi: Option<f64>,
    #[serde(default)]
    pub squeeze_on: Option<bool>,
    #[serde(default)]
    pub squeeze_momentum: Option<f64>,
    #[serde(default)]
    pub macd_line: Option<f64>,
    #[serde(default)]
    pub macd_signal: Option<f64>,
    #[serde(default)]
    pub macd_hist: Option<f64>,
    #[serde(default)]
    pub adx: Option<f64>,
    #[serde(default)]
    pub adx_plus: Option<f64>,
    #[serde(default)]
    pub adx_minus: Option<f64>,
    #[serde(default)]
    pub bb_upper: Option<f64>,
    #[serde(default)]
    pub bb_middle: Option<f64>,
    #[serde(default)]
    pub bb_lower: Option<f64>,
    #[serde(default)]
    pub atr: Option<f64>,
    #[serde(default)]
    pub ema_fast: Option<f64>,
    #[serde(default)]
    pub ema_medium: Option<f64>,
    #[serde(default)]
    pub ema_slow: Option<f64>,
    #[serde(default)]
    pub ema_long: Option<f64>,
    #[serde(default)]
    pub ema_stack_state: Option<String>,
    #[serde(default)]
    pub vwap: Option<f64>,
    #[serde(default)]
    pub close: Option<f64>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub average_volume: Option<f64>,
    #[serde(default)]
    pub rvol: Option<f64>,
    #[serde(default)]
    pub vwap_bias: Option<String>,
    #[serde(default)]
    pub rsi_divergence_status: Option<String>,
    #[serde(default)]
    pub macd_divergence_status: Option<String>,
    #[serde(default)]
    pub macd_trend_state: Option<String>,
    #[serde(default)]
    pub macd_crossover_detected: Option<bool>,
    #[serde(default)]
    pub macd_crossover_direction: Option<String>,
    #[serde(default)]
    pub macd_histogram_peak: Option<f64>,
    #[serde(default)]
    pub squeeze_duration: Option<u32>,
    #[serde(default)]
    pub squeeze_release_trigger: Option<bool>,
    #[serde(default)]
    pub squeeze_momentum_direction: Option<String>,
    #[serde(default)]
    pub chart_pattern: Option<String>,
    #[serde(default)]
    pub chart_pattern_confidence: Option<f64>,
    #[serde(default)]
    pub bbwp: Option<f64>,
    #[serde(default)]
    pub atr_volatility_regime: Option<String>,
    #[serde(default)]
    pub current_price: Option<f64>,
    #[serde(default)]
    pub adx_slope: Option<f64>,
    #[serde(default)]
    pub adx_regime: Option<String>,
    #[serde(default)]
    pub adx_di_crossover_detected: Option<bool>,
    #[serde(default)]
    pub adx_di_crossover_direction: Option<String>,
}

// ─── Risk ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RiskProfileCreate {
    pub profile_name: String,
    #[serde(
        default = "default_capital_decimal",
        deserialize_with = "deserialize_decimal_flexible"
    )]
    pub capital: Decimal,
    #[serde(
        default = "default_max_risk_decimal",
        deserialize_with = "deserialize_decimal_flexible"
    )]
    pub max_risk_pct: Decimal,
    pub leverage: i32,
    #[serde(default, deserialize_with = "deserialize_decimal_flexible")]
    pub commission_pct: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal_flexible")]
    pub funding_rate_8h: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal_flexible")]
    pub spread: Decimal,
}
fn default_capital_decimal() -> Decimal {
    dec!(10000)
}
fn default_max_risk_decimal() -> Decimal {
    dec!(2)
}

#[derive(Debug, Deserialize)]
pub struct RiskCalculateRequest {
    pub profile_id: i64,
    pub direction: String,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub entry_price: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub stop_loss: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub take_profit: Decimal,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub max_risk_pct: Option<Decimal>,
    #[serde(default)]
    pub leverage: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub commission_pct: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub funding_rate_8h: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub spread: Option<Decimal>,
    #[serde(default)]
    pub use_dynamic_atr: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub atr_value: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub capital: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub stop_loss_price: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub take_profit_price: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub atr_multiplier: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub atr_target_rr: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub struct CommissionProjectionPayload {
    pub profile_id: i64,
    pub direction: String,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub entry_1: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub entry_2: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub stop_loss_1: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub stop_loss_2: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub take_profit_1: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub take_profit_2: Decimal,
    #[serde(deserialize_with = "deserialize_decimal_flexible")]
    pub capital_entry_1_pct: Decimal,
    pub order_type: String,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub max_risk_pct: Option<Decimal>,
    #[serde(default)]
    pub leverage: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub commission_pct: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub funding_rate_8h: Option<Decimal>,
    #[serde(default, deserialize_with = "deserialize_optional_decimal_flexible")]
    pub capital: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub struct FeeTableQuery {
    pub order_type: String,
    #[serde(default)]
    pub capitals: Option<Vec<f64>>,
    #[serde(default)]
    pub leverages: Option<Vec<i32>>,
}

// ─── Dashboard / Journal ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    #[serde(default)]
    #[serde(alias = "initial_capital")]
    pub portfolio_capital_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct TradeJournalQuery {
    #[serde(default = "default_journal_limit")]
    pub limit: u32,
}
fn default_journal_limit() -> u32 {
    50
}

/// AUDIT-F4: shared cap for journal/ledger/analytics `limit` params —
/// the documented `/api/history` ceiling is 1000; the unbounded journal/
/// ledger/optimization endpoints previously allowed `?limit=2_000_000_000`
/// which dumped entire tables in one response.
pub const API_MAX_LIMIT: u32 = 1000;

#[derive(Debug, Deserialize)]
pub struct UpdateJournalNotesRequest {
    pub human_notes: String,
    pub execution_score: f64,
}

#[derive(Debug, Deserialize)]
pub struct TradeLedgerQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
}
fn default_limit() -> u32 {
    200
}
#[derive(Debug, Deserialize)]
pub struct TradeTelemetryRequest {
    pub exchange: String,
    pub symbol: String,
    pub direction: String,
    pub entry_timestamp: i64,
    pub exit_timestamp: i64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub commission_fees: f64,
    pub funding_fees: f64,
    pub realized_pnl: f64,
    pub roi_pct: f64,
    #[serde(default = "default_trigger")]
    pub trigger_source: String,
}
fn default_trigger() -> String {
    "MANUAL".to_string()
}

// ─── Session ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SessionInitRequest {
    pub exchange: String,
    pub currency: String,
    /// "observe" | "paper" | "live" — default execution mode for created
    /// instances. Observe is market-monitoring only (no orders dispatched).
    #[serde(default)]
    pub mode: Option<String>,
    /// Paper-session capital (USD) — default `portfolio_capital_usd`
    /// (v9 F-07: ONE capital dial; the session default overrides the
    /// workspace value for new sessions).
    #[serde(default)]
    pub portfolio_capital_usd: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    pub active: bool,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub instance_count: usize,
    /// Default execution mode for created instances
    /// ("observe" | "paper" | "live").
    pub mode: Option<String>,
    /// Paper-session capital (USD).
    pub capital: Option<f64>,
    /// v10: the persisted session number (monotonic, never reused).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<i64>,
    /// v11.6 crash recovery: the previous session was not shut down
    /// gracefully — the Welcome screen offers Recover / Discard.
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted_session: Option<crate::types::InterruptedSessionInfoWire>,
}

/// v11.6: wire shape of the interrupted-session summary (Welcome card).
#[derive(Debug, Clone, serde::Serialize)]
pub struct InterruptedSessionInfoWire {
    pub id: i64,
    pub mode: Option<String>,
    pub exchange: Option<String>,
    pub currency: Option<String>,
    pub started_at_ms: i64,
    pub instance_count: usize,
}

/// v10: one persisted session row (list + history).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionListRow {
    pub id: i64,
    pub mode: String,
    pub exchange: Option<String>,
    pub currency: Option<String>,
    pub portfolio_capital_usd: Option<f64>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub status: String,
}

// ─── Instance ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddInstanceRequest {
    pub base: String,
    pub quote: String,
}

#[derive(Debug, Serialize)]
pub struct InstanceListResponse {
    pub instances: Vec<portfolio_supervisor::registry::InstanceSummary>,
    pub total_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct InstanceDetailQuery {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub pair_key: Option<String>,
    /// Legacy alias accepted by the reload endpoint (`?slot=`); prefer
    /// `?tf=<secs|label>`. Not used by other routes.
    #[serde(default)]
    pub slot: Option<String>,
    /// v11.9: duration in seconds (`60`) or label (`1m`) for the reload
    /// endpoint. `all` (default) recharges every ACTIVE duration.
    #[serde(default)]
    pub tf: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceConfigPayload {
    /// v11.9: per-duration overrides keyed by duration seconds
    /// (`{"60": { candles, indicators, leverage }, ...}`). Each entry is
    /// the COMPLETE `TimeframeConfig` for that duration; absent durations
    /// keep the per-duration profile. When present the whole map replaces
    /// the stored overrides.
    #[serde(default)]
    pub timeframes: Option<std::collections::BTreeMap<u64, config_models::TimeframeConfig>>,
    #[serde(default)]
    pub automation: Option<config_models::AutomationConfig>,
    #[serde(default)]
    pub operational_mode: Option<String>,
    #[serde(default)]
    pub weight_overrides: Option<std::collections::HashMap<String, i32>>,
    #[serde(default)]
    pub activation: Option<config_models::ActivationConfig>,
    /// v9: bind the instance to a strategy (by name). Recharges fully at
    /// the next candle boundary.
    #[serde(default)]
    pub strategy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InstanceManualRequest {
    pub action: String,
    pub direction: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct InstanceIntervalsRequest {
    pub slow_seconds: i64,
    pub normal_seconds: i64,
    pub fast_seconds: i64,
}
