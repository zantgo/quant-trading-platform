use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidConfig {
    #[serde(default = "default_hyperliquid_ws_url")]
    pub ws_url: String,
}

impl Default for HyperliquidConfig {
    fn default() -> Self {
        Self {
            ws_url: default_hyperliquid_ws_url(),
        }
    }
}

impl HyperliquidConfig {
    pub fn rest_url(&self) -> String {
        self.ws_url
            .replace("wss://", "https://")
            .replace("ws://", "http://")
            .replace("/ws", "/info")
    }
}

fn default_hyperliquid_ws_url() -> String {
    "wss://api.hyperliquid.xyz/ws".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitgetConfig {
    #[serde(default = "default_bitget_ws_url")]
    pub ws_url: String,
}

impl Default for BitgetConfig {
    fn default() -> Self {
        Self {
            ws_url: default_bitget_ws_url(),
        }
    }
}

impl BitgetConfig {
    /// Base path for Bitget V2 mix (perpetual futures) market endpoints.
    pub fn mix_base_url(&self) -> String {
        "https://api.bitget.com/api/v2/mix/market".to_string()
    }

    pub fn rest_url(&self) -> String {
        format!("{}/candles", self.mix_base_url())
    }

    /// Ticker endpoint used to verify a contract symbol exists.
    pub fn ticker_url(&self) -> String {
        format!("{}/ticker", self.mix_base_url())
    }
}

fn default_bitget_ws_url() -> String {
    "wss://ws.bitget.com/v2/ws/public".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandlesConfig {
    #[serde(default = "default_candle_duration")]
    pub duration_seconds: u64,
}

impl Default for CandlesConfig {
    fn default() -> Self {
        Self {
            duration_seconds: default_candle_duration(),
        }
    }
}

fn default_candle_duration() -> u64 {
    60
}

/// Single source of truth for the platform's candle buffer behavior.
/// Replaces the previous per-instance `analysis_limit` field. See
/// `docs/operations-and-compliance/08-08-candle-buffer-spec.md` (CB-01 … CB-12).
///
/// Lives as a top-level block in `config.toml`:
/// ```toml
/// [candle_buffer]
/// size = 500                                # CB-01
/// stale_threshold_secs = 300                # CB-04 / DCP-05 / ILS-07
/// fetch_timeout_ms = 30000                  # HFP-10
/// sub_minute_skip_historical = false        # PRI-03 (v6.10.7): sub-minute state-replay warmup
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandleBufferConfig {
    /// Canonical rolling buffer length — the **historical warmup depth**
    /// (CB-01/CB-08): every ≥ 1-minute pipeline starts with exactly this
    /// many candles fetched from the exchange REST endpoint, and every
    /// per-timeframe in-memory buffer (`NormalizedCandle` history,
    /// `MarketSnapshot` history) is rolled at exactly this many entries
    /// (CB-03). The three candle numbers are **independent tiers**:
    /// `INDICATORS_MAX_BARS_REQUIRED = 300` (indicator calculation floor),
    /// this `size` (default **500**, the internet warmup), and
    /// `HIST_BUFFER_MAX = 1000` (absolute in-memory cap — never more than
    /// 1000 candles, sub-minute and above-minute, same behavior).
    #[serde(default = "default_candle_buffer_size")]
    pub size: usize,

    /// Seconds without a completed candle before the pipeline transitions
    /// `LIVE → STALE` (DCP-05) or before an indicator in `Loading` escalates
    /// to `Failed` (ILS-06 / ILS-09).
    /// Default: **300** (5 minutes).
    #[serde(default = "default_stale_threshold_secs")]
    pub stale_threshold_secs: u64,

    /// Maximum total time (ms) for `HistoricalFetchPolicy::fetch` to spend
    /// paginating the exchange REST endpoint before returning a partial
    /// result (HFP-10). Default: **30 000 ms**.
    #[serde(default = "default_fetch_timeout_ms")]
    pub fetch_timeout_ms: u64,

    /// When `true`, sub-minute timeframes (`timeframe_secs < 60`) skip the
    /// historical state-replay warmup (PRI-03) and start at 0 candles
    /// (CB-05 / HFP-03). When `false` (default since v6.10.7), sub-minute
    /// slots warm their indicator state machines / `history` buffer by
    /// replaying real closes: local DB rows for the sub-minute TF first,
    /// topped up with REST candles at the nearest exchange-standard interval
    /// (60 s). The replayed bars never enter `snapshot_history` (PRI-08) —
    /// the chart stays live-only.
    #[serde(default = "default_sub_minute_skip_historical")]
    pub sub_minute_skip_historical: bool,
}

impl Default for CandleBufferConfig {
    fn default() -> Self {
        Self {
            size: default_candle_buffer_size(),
            stale_threshold_secs: default_stale_threshold_secs(),
            fetch_timeout_ms: default_fetch_timeout_ms(),
            sub_minute_skip_historical: default_sub_minute_skip_historical(),
        }
    }
}

fn default_candle_buffer_size() -> usize {
    500
}

fn default_stale_threshold_secs() -> u64 {
    300
}

fn default_fetch_timeout_ms() -> u64 {
    30_000
}

fn default_sub_minute_skip_historical() -> bool {
    // PRI-03 (v6.10.7): sub-minute state-replay warmup is ON by default so
    // sub-minute slots reach the same post-warmup analytical state as
    // above-minute slots. Set `true` to opt out.
    false
}

/// Migration helper: legacy `analysis_limit` keys (the previous per-instance
/// field) are silently ignored after v6.5. The canonical number is
/// `candle_buffer.size`. This function reports whether the operator has any
/// stale keys so the daemon can log a one-shot warning at startup.
pub fn detect_legacy_analysis_limit_keys(raw_toml: &str) -> bool {
    raw_toml.contains("analysis_limit")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorsConfig {
    #[serde(default = "default_ema_fast")]
    pub ema_fast: usize,
    #[serde(default = "default_ema_medium")]
    pub ema_medium: usize,
    #[serde(default = "default_ema_slow")]
    pub ema_slow: usize,
    #[serde(default = "default_ema_long")]
    pub ema_long: usize,
    #[serde(default = "default_rsi_period")]
    pub rsi_period: usize,
    #[serde(default = "default_macd_fast")]
    pub macd_fast: usize,
    #[serde(default = "default_macd_slow")]
    pub macd_slow: usize,
    #[serde(default = "default_macd_signal")]
    pub macd_signal: usize,
    #[serde(default = "default_adx_period")]
    pub adx_period: usize,
    #[serde(default = "default_atr_period")]
    pub atr_period: usize,
    #[serde(default = "default_squeeze_period")]
    pub squeeze_period: usize,
    #[serde(default = "default_stoch_k")]
    pub stoch_k_period: usize,
    #[serde(default = "default_stoch_d")]
    pub stoch_d_period: usize,
    #[serde(default = "default_stoch_s")]
    pub stoch_s_period: usize,
    #[serde(default = "default_chandemo")]
    pub chandemo_period: usize,
    #[serde(default = "default_supertrend_period")]
    pub supertrend_period: usize,
    #[serde(default = "default_supertrend_multiplier")]
    pub supertrend_multiplier: f64,
    #[serde(default = "default_keltner_ema")]
    pub keltner_ema_period: usize,
    #[serde(default = "default_keltner_atr")]
    pub keltner_atr_period: usize,
    #[serde(default = "default_keltner_multiplier")]
    pub keltner_multiplier: f64,
    #[serde(default = "default_donchian_period")]
    pub donchian_period: usize,
    #[serde(default = "default_obv_smoothing")]
    pub obv_smoothing: usize,
    #[serde(default = "default_cmf_period")]
    pub cmf_period: usize,
    #[serde(default = "default_mfi_period")]
    pub mfi_period: usize,
    #[serde(default = "default_hv_period")]
    pub hv_period: usize,
    #[serde(default = "default_aroon_period")]
    pub aroon_period: usize,
    #[serde(default = "default_chop_period")]
    pub chop_period: usize,
    #[serde(default = "default_linreg_period")]
    pub linreg_period: usize,
    #[serde(default = "default_zscore_period")]
    pub zscore_period: usize,
    #[serde(default = "default_bbwp_lookback")]
    pub bbwp_lookback: usize,
    #[serde(default = "default_bbwp_period")]
    pub bbwp_period: usize,
    #[serde(default = "default_macd_extreme_high")]
    pub macd_extreme_high_threshold: f64,
    #[serde(default = "default_macd_extreme_low")]
    pub macd_extreme_low_threshold: f64,
    #[serde(default = "default_macd_contraction_threshold")]
    pub macd_histogram_contraction_threshold: f64,
    #[serde(default = "default_adx_trend_threshold")]
    pub adx_trend_threshold: u32,
    #[serde(default = "default_adx_exhaustion_threshold")]
    pub adx_exhaustion_threshold: u32,
    #[serde(default = "default_adx_slope_lookback")]
    pub adx_slope_lookback: usize,
    #[serde(default = "default_squeeze_min_duration")]
    pub squeeze_min_duration: u32,
    #[serde(default = "default_squeeze_bb_period")]
    pub squeeze_bb_period: usize,
    #[serde(default = "default_squeeze_bb_std_dev")]
    pub squeeze_bb_std_dev: f64,
    #[serde(default = "default_squeeze_kc_period")]
    pub squeeze_kc_period: usize,
    #[serde(default = "default_squeeze_kc_atr_multiplier")]
    pub squeeze_kc_atr_multiplier: f64,
    #[serde(default = "default_atr_multiplier")]
    pub atr_multiplier_coefficient: f64,
    #[serde(default = "default_atr_target_rr")]
    pub atr_target_rr_ratio: f64,
    #[serde(default = "default_volume_average_period")]
    pub volume_average_period: usize,
    #[serde(default = "default_rvol_threshold_institutional")]
    pub rvol_threshold_institutional: f64,
    #[serde(default = "default_rvol_threshold_climax")]
    pub rvol_threshold_climax: f64,
    /// Operator-selected integer × leverage tiers for the liquidation
    /// heatmap tier highlight (each ∈ [1, 100], default `[10]`). The
    /// dashboard's heatmap tier picker persists selections here; the
    /// frontend consumes it via `/api/config` so tiers survive reloads.
    #[serde(default = "default_heatmap_leverage_tiers")]
    pub heatmap_leverage_tiers: Vec<u32>,
    /// AUDIT-AIU-072: the pivot-points method (classic/fibonacci/camarilla/
    /// woodie) was declared in the registry `config_params` but never wired —
    /// the analyzer hardcoded `Classic`.
    #[serde(default = "default_pivot_points_method")]
    pub pivot_points_method: String,
    /// AUDIT-AIU-073: the minimum candlestick-pattern confidence gate was
    /// hardcoded 0.3 in the analyzer; registry-declared config now exists.
    #[serde(default = "default_candlestick_min_confidence")]
    pub candlestick_min_confidence: f64,
    #[serde(default = "default_ichimoku_tenkan")]
    pub ichimoku_tenkan: usize,
    #[serde(default = "default_ichimoku_kijun")]
    pub ichimoku_kijun: usize,
    #[serde(default = "default_ichimoku_senkou_b")]
    pub ichimoku_senkou_b: usize,
    #[serde(default = "default_ichimoku_displacement")]
    pub ichimoku_displacement: usize,
    #[serde(default = "default_cci_period")]
    pub cci_period: usize,
    #[serde(default = "default_psar_af_step")]
    pub psar_af_step: f64,
    #[serde(default = "default_psar_af_max")]
    pub psar_af_max: f64,
    #[serde(default = "default_williams_r_period")]
    pub williams_r_period: usize,
    #[serde(default = "default_hull_ma_period")]
    pub hull_ma_period: usize,
    #[serde(default = "default_force_index_smoothing")]
    pub force_index_smoothing: usize,
    #[serde(default = "default_stddev_channel_period")]
    pub stddev_channel_period: usize,
    #[serde(default = "default_smc_lookback")]
    pub smc_lookback: usize,
    #[serde(default = "default_volume_profile_bins")]
    pub volume_profile_bins: usize,
    #[serde(default = "default_volume_profile_window")]
    pub volume_profile_window: usize,
    #[serde(default = "default_volume_profile_value_area")]
    pub volume_profile_value_area: f64,
}

impl Default for IndicatorsConfig {
    fn default() -> Self {
        Self {
            ema_fast: default_ema_fast(),
            ema_medium: default_ema_medium(),
            ema_slow: default_ema_slow(),
            ema_long: default_ema_long(),
            rsi_period: default_rsi_period(),
            macd_fast: default_macd_fast(),
            macd_slow: default_macd_slow(),
            macd_signal: default_macd_signal(),
            adx_period: default_adx_period(),
            atr_period: default_atr_period(),
            squeeze_period: default_squeeze_period(),
            stoch_k_period: default_stoch_k(),
            stoch_d_period: default_stoch_d(),
            stoch_s_period: default_stoch_s(),
            chandemo_period: default_chandemo(),
            supertrend_period: default_supertrend_period(),
            supertrend_multiplier: default_supertrend_multiplier(),
            keltner_ema_period: default_keltner_ema(),
            keltner_atr_period: default_keltner_atr(),
            keltner_multiplier: default_keltner_multiplier(),
            donchian_period: default_donchian_period(),
            obv_smoothing: default_obv_smoothing(),
            cmf_period: default_cmf_period(),
            mfi_period: default_mfi_period(),
            hv_period: default_hv_period(),
            aroon_period: default_aroon_period(),
            chop_period: default_chop_period(),
            linreg_period: default_linreg_period(),
            zscore_period: default_zscore_period(),
            bbwp_lookback: default_bbwp_lookback(),
            bbwp_period: default_bbwp_period(),
            macd_extreme_high_threshold: default_macd_extreme_high(),
            macd_extreme_low_threshold: default_macd_extreme_low(),
            macd_histogram_contraction_threshold: default_macd_contraction_threshold(),
            adx_trend_threshold: default_adx_trend_threshold(),
            adx_exhaustion_threshold: default_adx_exhaustion_threshold(),
            adx_slope_lookback: default_adx_slope_lookback(),
            squeeze_min_duration: default_squeeze_min_duration(),
            squeeze_bb_period: default_squeeze_bb_period(),
            squeeze_bb_std_dev: default_squeeze_bb_std_dev(),
            squeeze_kc_period: default_squeeze_kc_period(),
            squeeze_kc_atr_multiplier: default_squeeze_kc_atr_multiplier(),
            atr_multiplier_coefficient: default_atr_multiplier(),
            atr_target_rr_ratio: default_atr_target_rr(),
            volume_average_period: default_volume_average_period(),
            rvol_threshold_institutional: default_rvol_threshold_institutional(),
            rvol_threshold_climax: default_rvol_threshold_climax(),
            heatmap_leverage_tiers: default_heatmap_leverage_tiers(),
            pivot_points_method: default_pivot_points_method(),
            candlestick_min_confidence: default_candlestick_min_confidence(),
            ichimoku_tenkan: default_ichimoku_tenkan(),
            ichimoku_kijun: default_ichimoku_kijun(),
            ichimoku_senkou_b: default_ichimoku_senkou_b(),
            ichimoku_displacement: default_ichimoku_displacement(),
            cci_period: default_cci_period(),
            psar_af_step: default_psar_af_step(),
            psar_af_max: default_psar_af_max(),
            williams_r_period: default_williams_r_period(),
            hull_ma_period: default_hull_ma_period(),
            force_index_smoothing: default_force_index_smoothing(),
            stddev_channel_period: default_stddev_channel_period(),
            smc_lookback: default_smc_lookback(),
            volume_profile_bins: default_volume_profile_bins(),
            volume_profile_window: default_volume_profile_window(),
            volume_profile_value_area: default_volume_profile_value_area(),
        }
    }
}

fn default_ema_fast() -> usize {
    10
}
fn default_ema_medium() -> usize {
    50
}
fn default_ema_slow() -> usize {
    100
}
fn default_ema_long() -> usize {
    200
}
fn default_rsi_period() -> usize {
    14
}
fn default_macd_fast() -> usize {
    12
}
fn default_macd_slow() -> usize {
    26
}
fn default_macd_signal() -> usize {
    9
}
fn default_adx_period() -> usize {
    14
}
fn default_atr_period() -> usize {
    14
}
fn default_squeeze_period() -> usize {
    20
}
fn default_bbwp_lookback() -> usize {
    252
}
fn default_bbwp_period() -> usize {
    20
}
fn default_stoch_k() -> usize {
    18
}
fn default_stoch_d() -> usize {
    5
}
fn default_stoch_s() -> usize {
    9
}
fn default_chandemo() -> usize {
    12
}
fn default_supertrend_period() -> usize {
    10
}
fn default_supertrend_multiplier() -> f64 {
    3.0
}
fn default_keltner_ema() -> usize {
    20
}
fn default_keltner_atr() -> usize {
    10
}
fn default_keltner_multiplier() -> f64 {
    2.0
}
fn default_donchian_period() -> usize {
    20
}
fn default_obv_smoothing() -> usize {
    20
}
fn default_cmf_period() -> usize {
    20
}
fn default_mfi_period() -> usize {
    14
}
fn default_hv_period() -> usize {
    20
}
fn default_aroon_period() -> usize {
    25
}
fn default_chop_period() -> usize {
    14
}
fn default_linreg_period() -> usize {
    20
}
fn default_zscore_period() -> usize {
    20
}
fn default_macd_extreme_high() -> f64 {
    1000.0
}
fn default_macd_extreme_low() -> f64 {
    -1000.0
}
fn default_macd_contraction_threshold() -> f64 {
    0.30
}
fn default_adx_trend_threshold() -> u32 {
    20
}
fn default_adx_exhaustion_threshold() -> u32 {
    40
}
fn default_adx_slope_lookback() -> usize {
    3
}
fn default_squeeze_min_duration() -> u32 {
    5
}
fn default_squeeze_bb_period() -> usize {
    20
}
fn default_squeeze_bb_std_dev() -> f64 {
    2.0
}
fn default_squeeze_kc_period() -> usize {
    20
}
fn default_squeeze_kc_atr_multiplier() -> f64 {
    1.5
}
fn default_atr_multiplier() -> f64 {
    2.0
}
fn default_atr_target_rr() -> f64 {
    2.5
}
fn default_volume_average_period() -> usize {
    20
}
fn default_rvol_threshold_institutional() -> f64 {
    1.5
}
fn default_rvol_threshold_climax() -> f64 {
    3.0
}

fn default_heatmap_leverage_tiers() -> Vec<u32> {
    vec![10]
}
fn default_pivot_points_method() -> String {
    "classic".to_string()
}
fn default_candlestick_min_confidence() -> f64 {
    0.3
}
fn default_ichimoku_tenkan() -> usize {
    9
}
fn default_ichimoku_kijun() -> usize {
    26
}
fn default_ichimoku_senkou_b() -> usize {
    52
}
fn default_ichimoku_displacement() -> usize {
    26
}
fn default_cci_period() -> usize {
    20
}
fn default_psar_af_step() -> f64 {
    0.02
}
fn default_psar_af_max() -> f64 {
    0.2
}
fn default_williams_r_period() -> usize {
    14
}
fn default_hull_ma_period() -> usize {
    21
}
fn default_force_index_smoothing() -> usize {
    13
}
fn default_stddev_channel_period() -> usize {
    20
}
fn default_smc_lookback() -> usize {
    20
}
fn default_volume_profile_bins() -> usize {
    100
}
fn default_volume_profile_window() -> usize {
    500
}
fn default_volume_profile_value_area() -> f64 {
    0.7
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FibonacciConfig {
    #[serde(default = "default_swing_lookback")]
    pub swing_lookback: usize,
    #[serde(default = "default_swing_scan_range")]
    pub swing_scan_range: usize,
    #[serde(default = "default_retracement_coefficients")]
    pub retracement_coefficients: Vec<f64>,
    #[serde(default = "default_extension_coefficients")]
    pub extension_coefficients: Vec<f64>,
}

impl Default for FibonacciConfig {
    fn default() -> Self {
        Self {
            swing_lookback: default_swing_lookback(),
            swing_scan_range: default_swing_scan_range(),
            retracement_coefficients: default_retracement_coefficients(),
            extension_coefficients: default_extension_coefficients(),
        }
    }
}

fn default_swing_lookback() -> usize {
    10
}
fn default_swing_scan_range() -> usize {
    120
}
fn default_retracement_coefficients() -> Vec<f64> {
    vec![0.236, 0.382, 0.500, 0.618, 0.660, 0.786]
}
fn default_extension_coefficients() -> Vec<f64> {
    vec![1.272, 1.618, 2.000, 2.618]
}

/// Order book configuration for depth analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBookConfig {
    #[serde(default = "default_ob_depth_levels")]
    pub depth_levels: usize,
    #[serde(default = "default_ob_imbalance_threshold")]
    pub imbalance_threshold: f64,
    #[serde(default = "default_ob_wall_threshold")]
    pub wall_threshold: f64,
    #[serde(default = "default_ob_spread_warning")]
    pub spread_warning_pct: f64,
    #[serde(default = "default_ob_spread_wide")]
    pub spread_wide_threshold_pct: f64,
}

impl Default for OrderBookConfig {
    fn default() -> Self {
        Self {
            depth_levels: default_ob_depth_levels(),
            imbalance_threshold: default_ob_imbalance_threshold(),
            wall_threshold: default_ob_wall_threshold(),
            spread_warning_pct: default_ob_spread_warning(),
            spread_wide_threshold_pct: default_ob_spread_wide(),
        }
    }
}

fn default_ob_depth_levels() -> usize {
    20
}
fn default_ob_imbalance_threshold() -> f64 {
    0.3
}
fn default_ob_wall_threshold() -> f64 {
    // AUDIT-AIU-012: the previous default 5.0 made wall detection
    // unreachable in production — the detector compares a level's volume
    // against the total top-N volume (a ratio mathematically ≤ 1.0), so any
    // threshold > 1.0 can never fire. 0.5 = a wall holding ≥ 50% of the
    // top-of-book volume. Unit tests used 0.15–0.5; production now uses a
    // sane default. NOTE (2026-08-17 audit): despite the historical claim,
    // there is NO `[order_book]` section in the config surface — the
    // runtime hardcodes `OrderBookConfig::default()` in the pipeline
    // constructor; tuning these defaults requires a code change.
    0.5
}
fn default_ob_spread_warning() -> f64 {
    0.1
}
fn default_ob_spread_wide() -> f64 {
    0.05
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotsConfig {
    #[serde(default = "default_pivot_strength_n")]
    pub strength_n: usize,
    #[serde(default = "default_scan_range_candles")]
    pub scan_range_candles: usize,
    #[serde(default = "default_sr_proximity_threshold")]
    pub sr_proximity_threshold_pct: f64,
    #[serde(default = "default_sr_flip_tolerance")]
    pub sr_flip_tolerance_pct: f64,
    #[serde(default = "default_pattern_slope_tolerance")]
    pub pattern_slope_tolerance: f64,
}

impl Default for PivotsConfig {
    fn default() -> Self {
        Self {
            strength_n: default_pivot_strength_n(),
            scan_range_candles: default_scan_range_candles(),
            sr_proximity_threshold_pct: default_sr_proximity_threshold(),
            sr_flip_tolerance_pct: default_sr_flip_tolerance(),
            pattern_slope_tolerance: default_pattern_slope_tolerance(),
        }
    }
}

fn default_pivot_strength_n() -> usize {
    10
}
fn default_scan_range_candles() -> usize {
    120
}
fn default_sr_proximity_threshold() -> f64 {
    0.5
}
fn default_sr_flip_tolerance() -> f64 {
    0.3
}
fn default_pattern_slope_tolerance() -> f64 {
    0.2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowTimeframeConfig {
    #[serde(default = "default_enabled_true")]
    pub enabled: bool,
    pub duration_seconds: u64,
}

fn default_enabled_true() -> bool {
    true
}

impl Default for SlowTimeframeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeverageConfig {
    #[serde(default = "default_cross_leverage")]
    pub cross_leverage: u32,
}

impl Default for LeverageConfig {
    fn default() -> Self {
        Self {
            cross_leverage: default_cross_leverage(),
        }
    }
}

fn default_cross_leverage() -> u32 {
    20
}

// v9 (F-06): `ScoringConfig` (score-tiered allocation percentages) is
// ERASED with the scaled-entry/pyramiding machinery — sizing is the
// v8.2 allocation model (`allocation_pct` + the strategy's
// `tae.sizing.quality_curve`, the replacement for score-based sizing).

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeesConfig {
    #[serde(default = "default_maker_fee")]
    pub maker_fee_pct: f64,
    #[serde(default = "default_taker_fee")]
    pub taker_fee_pct: f64,
    #[serde(default = "default_funding_rate_8h")]
    pub funding_rate_8h: f64,
}

impl Default for FeesConfig {
    fn default() -> Self {
        Self {
            maker_fee_pct: default_maker_fee(),
            taker_fee_pct: default_taker_fee(),
            funding_rate_8h: default_funding_rate_8h(),
        }
    }
}

fn default_maker_fee() -> f64 {
    0.02
}
fn default_taker_fee() -> f64 {
    0.06
}
fn default_funding_rate_8h() -> f64 {
    0.01
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_automation_interval")]
    pub interval_seconds: u64,
    #[serde(default = "default_max_opposite_exit_signals")]
    pub max_opposite_exit_signals: usize,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: default_automation_interval(),
            max_opposite_exit_signals: default_max_opposite_exit_signals(),
        }
    }
}

fn default_max_opposite_exit_signals() -> usize {
    5
}
fn default_automation_interval() -> u64 {
    900
}

// ─── Operational Mode ──────────────────────────────────────────

/// Each instance runs in exactly one mode:
///
/// - **Advisory**: market monitor only — indicators, signals, snapshots are
///   computed and broadcast, but no trade orders are ever submitted. This is
///   the default and the safest mode for observation.
/// - **PaperTrading**: the paper trading engine executes simulated orders on
///   the internal matching engine. Portfolio, risk, and performance analytics
///   are updated as if real trades occurred.
/// - **LiveTrading**: the live exchange adapter (Hyperliquid or Bitget) submits
///   real orders. This mode is **not yet implemented** — enabling it currently
///   panics at the execution boundary.
///
/// PaperTrading and LiveTrading follow the **same code path** — the
/// execution layer is strategy-identical. Toggling between them changes only
/// the order-routing backend, ensuring a strategy that works in paper mode
/// works identically in live mode (when implemented).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalMode {
    #[default]
    Advisory,
    PaperTrading,
    LiveTrading,
}

impl OperationalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationalMode::Advisory => "advisory",
            OperationalMode::PaperTrading => "paper_trading",
            OperationalMode::LiveTrading => "live_trading",
        }
    }

    /// True when this mode permits the execution layer to submit orders
    /// (either simulated or real).
    pub fn is_trading(&self) -> bool {
        matches!(
            self,
            OperationalMode::PaperTrading | OperationalMode::LiveTrading
        )
    }
}

fn default_true_bool() -> bool {
    true
}

// v9 (F-06): `AllocationCurveModel` / `AllocationCurve` /
// `PositionScalingConfig` are ERASED with the scaled-entry/pyramiding
// machinery. One position per instance, one side, one SL, one TP —
// sizing is the v8.2 allocation model (`allocation_pct` + strategy
// `tae.sizing`).

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeframeConfig {
    pub candles: CandlesConfig,
    #[serde(default)]
    pub indicators: IndicatorsConfig,
    /// v6.10 (Phase 2 / B4): per-TF leverage configuration feeding the
    /// cluster estimator. `#[serde(default)]` preserves backwards compat
    /// with existing `config.toml` files that don't include the section.
    #[serde(default)]
    pub leverage: TfLeverageConfig,
}

impl TimeframeConfig {
    pub fn new(duration_seconds: u64, indicators: IndicatorsConfig) -> Self {
        Self {
            candles: CandlesConfig { duration_seconds },
            indicators,
            leverage: TfLeverageConfig::default(),
        }
    }
}

impl Default for TimeframeConfig {
    /// Placeholder bound to the fastest duration (1s). Used by serde when a
    /// per-duration block omits fields that serde must default."""

    fn default() -> Self {
        Self::new(1, IndicatorsConfig::default())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_default_pair")]
    pub default_pair: String,
}

fn default_default_pair() -> String {
    "BTC/USDT".to_string()
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            default_pair: default_default_pair(),
        }
    }
}

/// Opportunity-matrix knobs (v6.10). **Advisory-only (2026-08-17 audit):
/// no runtime reader exists** — `market-analyzer::synthesis::derive_confluent_zones`
/// hardcodes `FALLBACK_ENABLED = true`, `K_ENTRY = 1.5`, `K_TARGET = 2.5`
/// (the workspace-config threading is a tracked follow-up). Tuning these
/// keys currently has NO effect on the emitted levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpportunityMatrixConfig {
    #[serde(default = "default_confluent_atr_fallback_enabled")]
    pub confluent_atr_fallback_enabled: bool,
    /// Multiplier on ATR for the entry-level fallback (entry sits
    /// `close − k_entry·ATR` for a bullish bias, or `close + k_entry·ATR`
    /// for bearish). Higher k = wider entry bracket.
    #[serde(default = "default_confluent_atr_k_entry")]
    pub confluent_atr_k_entry: f64,
    /// Multiplier on ATR for the target-level fallback (target sits
    /// `close + k_target·ATR` for bullish, `close − k_target·ATR`
    /// for bearish). Higher k = wider target bracket.
    #[serde(default = "default_confluent_atr_k_target")]
    pub confluent_atr_k_target: f64,
    /// v6.10.19 (P5): taker fee in basis points per side for the NET R:R
    /// cost model (default 6 = 0.06%). Plumbed through in a follow-up —
    /// the synthesis currently uses `NetCostModel::default()`.
    #[serde(default = "default_net_taker_fee_bps")]
    pub net_taker_fee_bps: f64,
    /// Assumed slippage in basis points per side (default 5 = 0.05%).
    #[serde(default = "default_net_slippage_bps")]
    pub net_slippage_bps: f64,
    /// Hold-time funding cost in basis points on the entry notional
    /// (default 0).
    #[serde(default = "default_net_funding_bps")]
    pub net_funding_bps: f64,
}

fn default_confluent_atr_fallback_enabled() -> bool {
    true
}
fn default_confluent_atr_k_entry() -> f64 {
    1.5
}
fn default_confluent_atr_k_target() -> f64 {
    2.5
}

fn default_net_taker_fee_bps() -> f64 {
    6.0
}

fn default_net_slippage_bps() -> f64 {
    5.0
}

fn default_net_funding_bps() -> f64 {
    0.0
}

impl Default for OpportunityMatrixConfig {
    fn default() -> Self {
        Self {
            confluent_atr_fallback_enabled: default_confluent_atr_fallback_enabled(),
            confluent_atr_k_entry: default_confluent_atr_k_entry(),
            confluent_atr_k_target: default_confluent_atr_k_target(),
            net_taker_fee_bps: default_net_taker_fee_bps(),
            net_slippage_bps: default_net_slippage_bps(),
            net_funding_bps: default_net_funding_bps(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_consecutive_loss_caution")]
    pub consecutive_loss_caution: u32,
    #[serde(default = "default_consecutive_loss_dropout")]
    pub consecutive_loss_dropout: u32,
    #[serde(default = "default_dropout_duration_hours")]
    pub dropout_duration_hours: u64,
    #[serde(default = "default_drawdown_limit_pct")]
    pub drawdown_limit_pct: f64,
    #[serde(default = "default_max_daily_drawdown_pct")]
    pub max_daily_drawdown_pct: f64,
    #[serde(default = "default_systemic_risk_threshold")]
    pub systemic_risk_threshold: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_reset_cron: Option<String>,
}

fn default_consecutive_loss_caution() -> u32 {
    3
}
fn default_consecutive_loss_dropout() -> u32 {
    5
}
fn default_dropout_duration_hours() -> u64 {
    8
}
fn default_drawdown_limit_pct() -> f64 {
    30.0
}
fn default_max_daily_drawdown_pct() -> f64 {
    5.0
}
fn default_systemic_risk_threshold() -> f64 {
    80.0
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            consecutive_loss_caution: default_consecutive_loss_caution(),
            consecutive_loss_dropout: default_consecutive_loss_dropout(),
            dropout_duration_hours: default_dropout_duration_hours(),
            drawdown_limit_pct: default_drawdown_limit_pct(),
            max_daily_drawdown_pct: default_max_daily_drawdown_pct(),
            systemic_risk_threshold: default_systemic_risk_threshold(),
            session_reset_cron: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntervalsConfig {
    #[serde(default = "default_slow_seconds")]
    pub slow_seconds: u64,
    #[serde(default = "default_normal_seconds")]
    pub normal_seconds: u64,
    #[serde(default = "default_fast_seconds")]
    pub fast_seconds: u64,
}

fn default_slow_seconds() -> u64 {
    3600
}
fn default_normal_seconds() -> u64 {
    900
}
fn default_fast_seconds() -> u64 {
    300
}

/// Configurable activation: per-indicator, per-signal, and per-SignalKind
/// v6.10 (Phase 5 / E3): the three liquidity sub-toggles are now
/// `Option<bool>` so an instance config can:
/// - omit the field entirely → inherit the global default
/// - set `liquidation_feed = false` → override the global to false
///
/// Previously the field was `bool` with serde `default = true`, so an
/// instance could not opt out of the global. With `Option<bool>`,
/// `None` means "fall through to global" and `Some(false)` means
/// "force-disable this sub-feature on this instance".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivationConfig {
    #[serde(default)]
    pub disabled_indicators: Vec<String>,
    #[serde(default)]
    pub disabled_signals: Vec<String>,
    #[serde(default)]
    pub disabled_signal_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidation_feed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_estimation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_signals_enabled: Option<bool>,
}

impl Default for ActivationConfig {
    fn default() -> Self {
        Self {
            disabled_indicators: Vec::new(),
            disabled_signals: Vec::new(),
            disabled_signal_kinds: Vec::new(),
            liquidation_feed: Some(true),
            cluster_estimation: Some(true),
            liquidity_signals_enabled: Some(true),
        }
    }
}

/// Liquidity Intelligence configuration.
///
/// Controls derivatives telemetry activation, mark-price polling cadence,
/// liquidation event ingestion, and the assumptions used by the cluster
/// estimator. All fields have defaults; the platform remains fully
/// functional when this section is absent from legacy configs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquidityConfig {
    #[serde(default = "default_liquidity_enabled")]
    pub enabled: bool,
    #[serde(default = "default_true_bool")]
    pub liquidation_feed: bool,
    #[serde(default = "default_true_bool")]
    pub cluster_estimation: bool,
    #[serde(default = "default_true_bool")]
    pub signals: bool,
    #[serde(default = "default_mark_poll_ms")]
    pub mark_price_poll_ms: u64,
    #[serde(default = "default_liquidation_retention_days")]
    pub event_retention_days: u32,
    #[serde(default = "default_liquidation_bucket_retention_days")]
    pub bucket_retention_days: u32,
    #[serde(default = "default_cluster_refresh_secs")]
    pub cluster_refresh_secs: u64,
    #[serde(default = "default_maintenance_margin_rate")]
    pub maintenance_margin_rate: f64,
    #[serde(default = "default_cascade_detected_zscore")]
    pub cascade_detected_zscore: f64,
    #[serde(default = "default_cascade_sustained_events")]
    pub cascade_sustained_events: u32,
    #[serde(default = "default_funding_extreme_pct")]
    pub funding_extreme_pct: f64,
    #[serde(default = "default_magnet_activation_distance_pct")]
    pub magnet_activation_distance_pct: f64,
    #[serde(default = "default_liquidity_vacuum_threshold")]
    pub liquidity_vacuum_threshold: f64,
    #[serde(default = "default_oi_funding_divergence_pct")]
    pub oi_funding_divergence_pct: f64,
    /// Minimum cluster notional (USD) below which a bin is treated as
    /// noise and dropped from the cluster list. Default 50_000 — matches
    /// the legacy hardcoded production value so the visible behaviour
    /// does not regress for existing operators.
    #[serde(default = "default_min_cluster_notional_usd")]
    pub min_cluster_notional_usd: f64,
    /// AUDIT-AIU-057: per-signal confidence defaults. These were hardcoded
    /// in `derive_liquidity_signals`; they are now operator-tunable until
    /// an empirical calibration study replaces them (CHANGELOG note).
    #[serde(default = "default_signal_confidences")]
    pub signal_confidences: LiquiditySignalConfidences,
    /// Hyperliquid user address (0x-prefixed 40-hex-char string).
    /// Retained for backward compatibility with existing `config.toml`
    /// entries, but no longer required: as of the trades-channel
    /// liquidation extraction, every forced-close fill on Hyperliquid
    /// is marked on the public `trades` stream and ingested without
    /// per-account setup. Bitget has always worked this way (its public
    /// `liquidation` channel). The field is kept so older configs keep
    /// parsing; its value is otherwise ignored.
    #[serde(default)]
    pub hyperliquid_user_address: String,
}

impl Default for LiquidityConfig {
    fn default() -> Self {
        Self {
            enabled: default_liquidity_enabled(),
            liquidation_feed: true,
            cluster_estimation: true,
            signals: true,
            mark_price_poll_ms: default_mark_poll_ms(),
            event_retention_days: default_liquidation_retention_days(),
            bucket_retention_days: default_liquidation_bucket_retention_days(),
            cluster_refresh_secs: default_cluster_refresh_secs(),
            maintenance_margin_rate: default_maintenance_margin_rate(),
            cascade_detected_zscore: default_cascade_detected_zscore(),
            cascade_sustained_events: default_cascade_sustained_events(),
            funding_extreme_pct: default_funding_extreme_pct(),
            magnet_activation_distance_pct: default_magnet_activation_distance_pct(),
            liquidity_vacuum_threshold: default_liquidity_vacuum_threshold(),
            oi_funding_divergence_pct: default_oi_funding_divergence_pct(),
            min_cluster_notional_usd: default_min_cluster_notional_usd(),
            signal_confidences: default_signal_confidences(),
            hyperliquid_user_address: String::new(),
        }
    }
}

/// API-failover tolerance knobs for the derivatives-data pollers.
/// `max_consecutive_failures` is consumed by the Hyperliquid derivatives
/// poller (it permanently disables the poller after this many consecutive
/// REST failures); `max_retries_per_call` / `retry_delay_seconds` are
/// reserved for per-call retry behavior and carried for operator
/// visibility and future wiring.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ApiFailoverConfig {
    #[serde(default = "default_failover_max_retries")]
    pub max_retries_per_call: u32,
    #[serde(default = "default_failover_retry_delay_seconds")]
    pub retry_delay_seconds: u32,
    #[serde(default = "default_failover_max_consecutive_failures")]
    pub max_consecutive_failures: u32,
}

impl Default for ApiFailoverConfig {
    fn default() -> Self {
        Self {
            max_retries_per_call: default_failover_max_retries(),
            retry_delay_seconds: default_failover_retry_delay_seconds(),
            max_consecutive_failures: default_failover_max_consecutive_failures(),
        }
    }
}

fn default_failover_max_retries() -> u32 {
    5
}

fn default_failover_retry_delay_seconds() -> u32 {
    30
}

fn default_failover_max_consecutive_failures() -> u32 {
    30
}

/// AUDIT-AIU-057: per-signal confidence defaults for the liquidity layer.
/// Values match the legacy hardcoded constants; operators may tune them
/// via `[liquidity.signal_confidences]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquiditySignalConfidences {
    #[serde(default = "default_conf_cascade_detected")]
    pub cascade_detected: f64,
    #[serde(default = "default_conf_cascade_sustained")]
    pub cascade_sustained: f64,
    #[serde(default = "default_conf_cascade_exhausted")]
    pub cascade_exhausted: f64,
    #[serde(default = "default_conf_funding_extreme")]
    pub funding_extreme: f64,
    #[serde(default = "default_conf_oi_funding_divergence")]
    pub oi_funding_divergence: f64,
    #[serde(default = "default_conf_liquidity_vacuum")]
    pub liquidity_vacuum: f64,
    #[serde(default = "default_conf_funding_flip")]
    pub funding_flip: f64,
    #[serde(default = "default_conf_oi_price_divergence")]
    pub oi_price_divergence: f64,
}

impl Default for LiquiditySignalConfidences {
    fn default() -> Self {
        default_signal_confidences()
    }
}

fn default_signal_confidences() -> LiquiditySignalConfidences {
    LiquiditySignalConfidences {
        cascade_detected: default_conf_cascade_detected(),
        cascade_sustained: default_conf_cascade_sustained(),
        cascade_exhausted: default_conf_cascade_exhausted(),
        funding_extreme: default_conf_funding_extreme(),
        oi_funding_divergence: default_conf_oi_funding_divergence(),
        liquidity_vacuum: default_conf_liquidity_vacuum(),
        funding_flip: default_conf_funding_flip(),
        oi_price_divergence: default_conf_oi_price_divergence(),
    }
}

fn default_conf_cascade_detected() -> f64 {
    0.8
}
fn default_conf_cascade_sustained() -> f64 {
    0.9
}
fn default_conf_cascade_exhausted() -> f64 {
    0.7
}
fn default_conf_funding_extreme() -> f64 {
    0.95
}
fn default_conf_oi_funding_divergence() -> f64 {
    0.7
}
fn default_conf_liquidity_vacuum() -> f64 {
    0.6
}
fn default_conf_funding_flip() -> f64 {
    0.75
}
fn default_conf_oi_price_divergence() -> f64 {
    0.7
}

fn default_liquidity_enabled() -> bool {
    true
}
fn default_mark_poll_ms() -> u64 {
    60_000
}
fn default_liquidation_retention_days() -> u32 {
    90
}
fn default_liquidation_bucket_retention_days() -> u32 {
    7
}
fn default_cluster_refresh_secs() -> u64 {
    // v6.5: each TF's cluster refresh now runs at its own candle cadence
    // (matching every other MME indicator/signal). The serialized default
    // is 0, which the cluster refresh task interprets as
    // "synchronize with the TF's `timeframe_secs`". Operators may
    // override with any value ≥ 1 (clamped to 1) — values between 60 s
    // and the TF cadence are also useful for high-TF operators who
    // want a much higher refresh rate than the candle cadence itself.
    0
}
fn default_maintenance_margin_rate() -> f64 {
    0.005
}
fn default_cascade_detected_zscore() -> f64 {
    2.5
}
fn default_cascade_sustained_events() -> u32 {
    3
}
fn default_funding_extreme_pct() -> f64 {
    0.0005
}
fn default_magnet_activation_distance_pct() -> f64 {
    0.5
}
fn default_liquidity_vacuum_threshold() -> f64 {
    0.3
}
fn default_oi_funding_divergence_pct() -> f64 {
    2.0
}
fn default_min_cluster_notional_usd() -> f64 {
    50_000.0
}

// v6.10 (Phase 2 / B4): per-TF leverage configuration for the cluster
// estimator. Operators can tune the leverage-bucket distribution that
// feeds `ClusterEstimateInput::leverage_buckets` / `leverage_weights`
// per timeframe, and disable the cluster estimator for specific TFs.
// Defaults preserve the legacy hardcoded values
// `[1, 3, 5, 10, 20, 50, 100]` / `[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05]`
// so existing operators see no behavior change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfLeverageConfig {
    /// Per-TF kill switch. When `false`, the per-TF cluster refresh task
    /// is suppressed entirely (no spawn, no `cluster` field on snapshot).
    /// Default `true` matches the prior always-on behavior.
    #[serde(default = "default_true_bool")]
    pub enabled: bool,
    /// Leverage buckets (in ascending order) used to distribute OI
    /// notional when estimating liquidation clusters.
    /// Default: `[1, 3, 5, 10, 20, 50, 100]`.
    #[serde(default = "default_leverage_buckets")]
    pub buckets: Vec<u32>,
    /// Per-bucket weights summing to ~1.0. The default is the documented
    /// platform-wide distribution:
    /// `[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05]` (peaks at 10×).
    #[serde(default = "default_leverage_weights")]
    pub weights: Vec<f64>,
    /// Minimum cluster notional in USD below which a bin is treated as
    /// noise and dropped from the cluster list. Default `50_000` matches
    /// the existing `LiquidityConfig.min_cluster_notional_usd` default.
    #[serde(default = "default_min_cluster_notional_usd")]
    pub min_cluster_notional_usd: f64,
}

impl Default for TfLeverageConfig {
    fn default() -> Self {
        Self {
            enabled: default_true_bool(),
            buckets: default_leverage_buckets(),
            weights: default_leverage_weights(),
            min_cluster_notional_usd: default_min_cluster_notional_usd(),
        }
    }
}

fn default_leverage_buckets() -> Vec<u32> {
    vec![1, 3, 5, 10, 20, 50, 100]
}

fn default_leverage_weights() -> Vec<f64> {
    vec![0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05]
}

/// Liquidation Heatmap configuration.
///
/// Independent feature gate for the price-bucketed real-event heatmap
/// overlay (Blocks B/C/D). Operators can keep `[liquidity]` enabled
/// while disabling the heatmap bucket aggregation (e.g. when memory is
/// constrained or when an external backtest pipeline is the primary
/// consumer of `liquidation_events`).
///
/// Defaults match the platform-wide spec: 0.1% bucket size relative to
/// mid-price, 24-hour rolling retention. The bucket layer is
/// **display-only** — it does not feed `cascade_risk`,
/// `LiquiditySqueeze`, or any policy decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapConfig {
    /// Master switch for the price-bucketed aggregation. When
    /// `false`, the accumulator skips `bucket_event` and the
    /// `recent_real_buckets` field stays empty. The estimated cluster
    /// matrix (separately gated by `[liquidity].cluster_estimation`)
    /// continues to render in that case.
    #[serde(default = "default_heatmap_enabled")]
    pub enabled: bool,
    /// Bucket width as a fraction of mid-price. `0.001` = 0.1%
    /// bins, so a 50_000 BTC mid produces 0.05% wide absolute bins
    /// of ~$50 each. Smaller bins are finer but produce more
    /// entries; the memory cap (4 × retention periods, ≈ 6000
    /// pairs at default) bounds the worst case.
    #[serde(default = "default_heatmap_bucket_size_pct")]
    pub bucket_size_pct: f64,
    /// Sliding-window retention for the bucketed aggregation
    /// (seconds). Default 86_400 (24h) so per-bar bucket churn does
    /// not flood the payload. At cascade volume this can be
    /// reduced to 6–12h to cap memory.
    #[serde(default = "default_heatmap_retention_secs")]
    pub retention_secs: u64,
    /// Whether the frontend should render the layered real-bucket
    /// and estimated-cluster view. Independent of `enabled`: with
    /// `render_real = false`, the buckets are still aggregated but
    /// the chart shows only the estimated clusters. Useful for
    /// isolating the two signal sources for diagnosis.
    #[serde(default = "default_true_bool")]
    pub render_real: bool,
    /// Whether the frontend should render the estimated cluster
    /// bands underneath the real buckets. When both are true,
    /// real events are saturated and the estimated bands fade in
    /// only where no real events have ever landed.
    #[serde(default = "default_true_bool")]
    pub render_estimated: bool,
    /// Hyperliquid has no public market-wide liquidation feed. The
    /// UI can still render the estimated cluster bands for HL by
    /// default; toggling this flag hides them entirely (so the
    /// panel reads "Model only — no public liquidation feed"). Does
    /// not affect Bitget.
    #[serde(default = "default_true_bool")]
    pub render_hl_caveat: bool,
}

impl Default for HeatmapConfig {
    fn default() -> Self {
        Self {
            enabled: default_heatmap_enabled(),
            bucket_size_pct: default_heatmap_bucket_size_pct(),
            retention_secs: default_heatmap_retention_secs(),
            render_real: true,
            render_estimated: true,
            render_hl_caveat: true,
        }
    }
}

fn default_heatmap_enabled() -> bool {
    true
}
fn default_heatmap_bucket_size_pct() -> f64 {
    0.001
}
fn default_heatmap_retention_secs() -> u64 {
    86_400
}

impl Default for IntervalsConfig {
    fn default() -> Self {
        Self {
            slow_seconds: default_slow_seconds(),
            normal_seconds: default_normal_seconds(),
            fast_seconds: default_fast_seconds(),
        }
    }
}

// ─── Data Quality Configuration (DIE L3 median filter + outlier rejection) ────

/// Configuration block for the DIE L3 Data Quality Layer's median price filter
/// and outlier rejection. Maps to the TOML `[quality]` section.
///
/// All fields are optional and fall back to the spec-defined defaults.
/// When this section is absent, the median filter is disabled.
///
/// See `docs/engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md` §4.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityConfig {
    /// Size of the rolling window for the median price filter. Ticks are
    /// accepted unfiltered (warm-up) until this many ticks have been observed.
    #[serde(default = "default_median_window_size")]
    pub median_window_size: usize,

    /// Maximum allowed deviation from the rolling median (as a decimal fraction).
    /// A tick is rejected when `|p − median| / median > outlier_tolerance`.
    /// Default: 0.05 (5% deviation).
    #[serde(default = "default_outlier_tolerance")]
    pub outlier_tolerance: f64,

    /// When true, bypass the filter for a tick whose rolling median is exactly
    /// zero (rare venue reset edge case). Logged at debug level.
    #[serde(default = "default_bypass_on_zero_median")]
    pub bypass_on_zero_median: bool,

    /// Maximum age of the last trade (in seconds) before a completed candle is
    /// considered stale. A candle whose last observed trade occurred more than
    /// this many seconds before the candle close is flagged with `is_stale = true`.
    /// Default: 600 (10 minutes).
    #[serde(default = "default_staleness_threshold_secs")]
    pub staleness_threshold_secs: u64,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            median_window_size: default_median_window_size(),
            outlier_tolerance: default_outlier_tolerance(),
            bypass_on_zero_median: default_bypass_on_zero_median(),
            staleness_threshold_secs: default_staleness_threshold_secs(),
        }
    }
}

fn default_median_window_size() -> usize {
    20
}

fn default_outlier_tolerance() -> f64 {
    0.05
}

fn default_bypass_on_zero_median() -> bool {
    true
}

fn default_staleness_threshold_secs() -> u64 {
    600
}

// ─── Clock Drift Monitor (NTP-based UTC alignment enforcement) ────

/// Configuration block for the runtime `ClockMonitor`. Maps to the TOML
/// `[clock_monitor]` section (or `null`/missing to disable the monitor).
///
/// All fields are optional and fall back to conservative defaults, so legacy
/// `config.toml` files keep working without modification. When this section is
/// absent the engine simply skips spawning the monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockMonitorTomlConfig {
    #[serde(default = "default_clock_monitor_enabled")]
    pub enabled: bool,
    #[serde(default = "default_clock_monitor_servers")]
    pub ntp_servers: Vec<String>,
    #[serde(default = "default_clock_monitor_poll_secs")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_clock_monitor_threshold_micros")]
    pub threshold_micros: i64,
    #[serde(default = "default_clock_monitor_query_timeout_secs")]
    pub query_timeout_secs: u64,
    #[serde(default = "default_clock_monitor_jitter_window")]
    pub jitter_window_size: usize,
    #[serde(default = "default_clock_monitor_breach_action")]
    pub breach_action: ClockMonitorBreachAction,
    #[serde(default = "default_clock_monitor_warn_on_breach")]
    pub warn_on_breach: bool,
}

impl Default for ClockMonitorTomlConfig {
    fn default() -> Self {
        Self {
            enabled: default_clock_monitor_enabled(),
            ntp_servers: default_clock_monitor_servers(),
            poll_interval_secs: default_clock_monitor_poll_secs(),
            threshold_micros: default_clock_monitor_threshold_micros(),
            query_timeout_secs: default_clock_monitor_query_timeout_secs(),
            jitter_window_size: default_clock_monitor_jitter_window(),
            breach_action: default_clock_monitor_breach_action(),
            warn_on_breach: default_clock_monitor_warn_on_breach(),
        }
    }
}

impl ClockMonitorTomlConfig {
    pub fn is_active(&self) -> bool {
        self.enabled && !self.ntp_servers.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockMonitorBreachAction {
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "panic")]
    Panic,
}

fn default_clock_monitor_enabled() -> bool {
    true
}
fn default_clock_monitor_servers() -> Vec<String> {
    vec!["pool.ntp.org".to_string(), "time.aws.com".to_string()]
}
fn default_clock_monitor_poll_secs() -> u64 {
    30
}
fn default_clock_monitor_threshold_micros() -> i64 {
    10000
}
fn default_clock_monitor_query_timeout_secs() -> u64 {
    5
}
fn default_clock_monitor_jitter_window() -> usize {
    20
}
fn default_clock_monitor_breach_action() -> ClockMonitorBreachAction {
    ClockMonitorBreachAction::Warn
}
fn default_clock_monitor_warn_on_breach() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReconnectConfig {
    #[serde(default = "default_reconnect_initial_ms")]
    pub initial_backoff_ms: u64,
    #[serde(default = "default_reconnect_max_ms")]
    pub max_backoff_ms: u64,
    #[serde(default = "default_reconnect_jitter")]
    pub jitter_pct: f64,
    /// Time (ms) after a successful WS handshake at which the supervisor
    /// marks the exchange "Connected". Default: 2000. If the handshake
    /// completes faster, the mark fires later (so transient error
    /// windows do not flip the panel red). Increase if your network
    /// path is consistently slow to complete the WS handshake.
    #[serde(default = "default_reconnect_connect_grace_ms")]
    pub connect_grace_ms: u64,
    /// Time (ms) after a WS termination during which the supervisor
    /// waits before marking the exchange "Disconnected". If the next
    /// iteration's "Connected" event fires within this window the
    /// pending disconnect is cancelled. Default: 5000. Set to 0 to
    /// disable (legacy behaviour: instant flip to Disconnected).
    #[serde(default = "default_reconnect_disconnect_grace_ms")]
    pub disconnect_grace_ms: u64,
}

fn default_reconnect_initial_ms() -> u64 {
    1000
}
fn default_reconnect_max_ms() -> u64 {
    30000
}
fn default_reconnect_jitter() -> f64 {
    0.2
}
fn default_reconnect_connect_grace_ms() -> u64 {
    2000
}
fn default_reconnect_disconnect_grace_ms() -> u64 {
    5000
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            jitter_pct: 0.2,
            connect_grace_ms: 2000,
            disconnect_grace_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fibonacci_config() {
        let cfg = FibonacciConfig::default();
        assert_eq!(cfg.retracement_coefficients.len(), 6);
        assert_eq!(cfg.extension_coefficients.len(), 4);
    }

    #[test]
    fn test_default_pivots_config() {
        let cfg = PivotsConfig::default();
        assert_eq!(cfg.strength_n, 10);
        assert_eq!(cfg.scan_range_candles, 120);
    }

    #[test]
    fn test_default_leverage() {
        let cfg = LeverageConfig::default();
        assert_eq!(cfg.cross_leverage, 20);
    }
}

/// Type alias for the FAST timeframe configuration block. Structurally
/// identical to `SlowTimeframeConfig` (enabled flag + duration + analysis
/// limit) — the two are differentiated only by convention.
pub type FastTimeframeConfig = SlowTimeframeConfig;

/// v7 setup-executor configuration (minimal TAE) is defined above; below is
/// the PAE significance-treatment configuration.
///
/// The statistical significance treatment (t-test, Monte Carlo sign
/// randomization, verdict classification) previously ran on hardcoded
/// constants. Operators of an institutional platform must be able to audit
/// and tune the bar: `alpha` is the significance level, `monte_carlo_runs`
/// the randomization count, and `min_trades_for_verdict` the minimum sample
/// below which no edge verdict is issued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    /// Significance level α — an edge is significant when BOTH the t-test
    /// p-value and the Monte Carlo p-value fall below this threshold.
    /// Default: 0.05.
    #[serde(default = "default_analytics_alpha")]
    pub alpha: f64,
    /// Monte Carlo sign-randomization runs for the empirical p-value.
    /// Default: 10_000.
    #[serde(default = "default_analytics_monte_carlo_runs")]
    pub monte_carlo_runs: u32,
    /// Minimum trade count before an edge verdict is issued; below this the
    /// classification is `InsufficientData`. Default: 30.
    #[serde(default = "default_analytics_min_trades")]
    pub min_trades_for_verdict: u32,
}

fn default_analytics_alpha() -> f64 {
    0.05
}
fn default_analytics_monte_carlo_runs() -> u32 {
    10_000
}
fn default_analytics_min_trades() -> u32 {
    30
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            alpha: default_analytics_alpha(),
            monte_carlo_runs: default_analytics_monte_carlo_runs(),
            min_trades_for_verdict: default_analytics_min_trades(),
        }
    }
}

/// v7.3 portfolio risk limits — the concentration / exposure / correlation
/// caps the PME Exposure layer enforces. Previously hardcoded constants in
/// `exposure_layer.rs`; now operator-tunable and rendered by the PME
/// Exposure tab so the displayed limit is always the enforced one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskLimitsConfig {
    /// Max single-pair concentration as a fraction of equity (0.2 = 20%).
    #[serde(default = "default_risk_limit_single_pair")]
    pub max_single_pair_exposure_pct: f64,
    /// Max portfolio exposure as a fraction of equity (0.5 = 50%).
    #[serde(default = "default_risk_limit_portfolio")]
    pub max_portfolio_exposure_pct: f64,
    /// Max allowed pairwise correlation between holdings (0.8).
    #[serde(default = "default_risk_limit_correlation")]
    pub max_correlation: f64,
}

fn default_risk_limit_single_pair() -> f64 {
    20.0
}
fn default_risk_limit_portfolio() -> f64 {
    50.0
}
fn default_risk_limit_correlation() -> f64 {
    0.8
}

impl Default for RiskLimitsConfig {
    fn default() -> Self {
        Self {
            max_single_pair_exposure_pct: default_risk_limit_single_pair(),
            max_portfolio_exposure_pct: default_risk_limit_portfolio(),
            max_correlation: default_risk_limit_correlation(),
        }
    }
}

// ─── TAE: Lifecycle State ─────────────────────────────────────────

/// Per-instance lifecycle state. Four live values per
/// `03-03-06-tae-instance-lifecycle-spec.md §IL-01`.
///
/// Scoped-enum rule: `instance PAUSED` (lifecycle), not to be confused with
/// `AUTO_PAUSED` (policy) or `SUSPENDED` (safety). The serde names carry the
/// `lifecycle_` prefix to make the axis explicit in persisted TOML/JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LifecycleState {
    Running,
    #[serde(rename = "lifecycle_paused")]
    LifecyclePaused,
    Stopping,
    #[default]
    Stopped,
}

impl LifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleState::Running => "RUNNING",
            LifecycleState::LifecyclePaused => "PAUSED",
            LifecycleState::Stopping => "STOPPING",
            LifecycleState::Stopped => "STOPPED",
        }
    }
}

// ─── TAE: Per-Symbol Execution Stance ────────────────────────────
//
// Controls per-symbol execution authorization: whether a symbol may accept
// new entries, only close existing positions, or no orders at all.
// Managed by the PME Veto and the operator via REST API.
//
// This is the PME/TAE execution-authorization enum — NOT the L6 Decision
// Matrix `MarketStance` (environmental aggressiveness assessment).
// The only shared variant is `Avoid` (both AGGRESSIVE/CAUTIOUS/NON_AVOID
// are exclusive to MarketStance; CLOSE_ONLY is exclusive to this enum).

// ─── TAE: Trade Direction ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Long,
    Short,
}

impl Direction {
    pub fn sign(&self) -> rust_decimal::Decimal {
        use rust_decimal::Decimal;
        match self {
            Direction::Long => Decimal::ONE,
            Direction::Short => Decimal::NEGATIVE_ONE,
        }
    }
}

// ─── TAE: Order Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    PreDispatch,
    Pending,
    Submitted,
    Open,
    PartiallyFilled,
    Closed,
    Cancelled,
    Rejected,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::PreDispatch => "PRE_DISPATCH",
            OrderStatus::Pending => "PENDING",
            OrderStatus::Submitted => "SUBMITTED",
            OrderStatus::Open => "OPEN",
            OrderStatus::PartiallyFilled => "PARTIALLY_FILLED",
            OrderStatus::Closed => "CLOSED",
            OrderStatus::Cancelled => "CANCELLED",
            OrderStatus::Rejected => "REJECTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPacket {
    pub client_order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<rust_decimal::Decimal>,
    pub size: rust_decimal::Decimal,
    pub reduce_only: bool,
    pub is_emergency_liquidation: bool,
    pub associated_position_id: Option<i64>,
    /// v7 TAE: free-form per-order metadata (e.g. `exit_reason`,
    /// `trigger_source` = setup type). Optional; serialized only when non-empty.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMatrixRow {
    pub order_id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub order_type: String,
    pub direction: String,
    pub price: Option<rust_decimal::Decimal>,
    pub trigger_price: Option<rust_decimal::Decimal>,
    pub size: rust_decimal::Decimal,
    pub filled_size: rust_decimal::Decimal,
    pub status: String,
    pub is_reduce_only: bool,
    pub is_emergency_liquidation: bool,
    pub associated_position_id: Option<i64>,
    pub created_at: u64,
    pub updated_at: u64,
    pub slippage_bps: Option<f64>,
}

// ─── TAE: Execution Config ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_slippage_ceiling_pct")]
    pub slippage_ceiling_pct: f64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            slippage_ceiling_pct: default_slippage_ceiling_pct(),
        }
    }
}

fn default_slippage_ceiling_pct() -> f64 {
    0.5
}

// ─── Snapshot Export (periodic JSON dump for offline data science) ─

/// Configuration for the snapshot-export scheduler. Lives at the
/// top level of `config.toml` under `[snapshot_export]` so the
/// operator can configure the export folder / cadence without
/// touching the workspace file.
///
/// Every snapshot writes one JSON file per (instance, tab) pair to
/// `<output_path>/<YYYY-MM-DD>/<HHhMMmSS>/<pairKey>.<tab>.json`.
/// See `crates/execution-daemon/src/snapshot_export.rs` for the
/// implementation and `docs/operations-and-compliance/08-09-snapshot-export.md`
/// for the operator manual.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotExportConfig {
    /// Master toggle — when `false` the scheduler task idles and
    /// writes nothing.
    #[serde(default = "default_snapshot_export_enabled")]
    pub enabled: bool,

    /// Destination directory. Created at startup if missing. Relative
    /// paths are resolved against the daemon's CWD (the project root
    /// in the standard deployment).
    #[serde(default = "default_snapshot_export_output_path")]
    pub output_path: String,

    /// Seconds between snapshots. Floor 5s, ceiling 3600s — values
    /// outside this range are clamped at task startup.
    #[serde(default = "default_snapshot_export_interval_secs")]
    pub interval_secs: u64,

    /// Hard upper bound on the number of timestamped snapshot
    /// directories retained. When exceeded, the oldest (lexicographic)
    /// directory is removed at the end of each tick.
    #[serde(default = "default_snapshot_export_max_snapshots_retained")]
    pub max_snapshots_retained: u32,

    /// Per-tab opt-in list. When `None` (the canonical default) all
    /// nine tabs are written: `metrics`, `mtf`, `alignment`,
    /// `opportunity`, `risk`, `analysis`, `advisory`, `decision`,
    /// `recommendation`. When `Some`, only the listed tab IDs are
    /// emitted (unknown IDs are silently dropped at task startup).
    #[serde(default)]
    pub tabs: Option<Vec<String>>,
}

impl Default for SnapshotExportConfig {
    fn default() -> Self {
        Self {
            enabled: default_snapshot_export_enabled(),
            output_path: default_snapshot_export_output_path(),
            interval_secs: default_snapshot_export_interval_secs(),
            max_snapshots_retained: default_snapshot_export_max_snapshots_retained(),
            tabs: None,
        }
    }
}

fn default_snapshot_export_enabled() -> bool {
    false
}

fn default_snapshot_export_output_path() -> String {
    "./snapshots".to_string()
}

fn default_snapshot_export_interval_secs() -> u64 {
    60
}

fn default_snapshot_export_max_snapshots_retained() -> u32 {
    1000
}

/// Backtesting Engine (BTE) configuration.
///
/// The deep-history backtest replays the full MME pipeline over archived
/// OHLCV candles. `archive_depth_days` bounds how far back the candle
/// archive reaches (and how deep an on-demand backfill may page); the
/// value is operator-tunable between 1 and 365 days and is enforced by the
/// M8 numeric guards. The per-exchange page caps mirror the documented /
/// empirical candle-endpoint limits (Hyperliquid `candleSnapshot` is
/// window-bounded with no `limit` parameter — we page conservatively at
/// 1000; Bitget accepts `limit` 1–1000 — we page at 200).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Candle-archive retention / maximum backfill depth in days.
    /// Default: 180. Enforced range: 1..=365 (M8).
    #[serde(default = "default_backtest_archive_depth_days")]
    pub archive_depth_days: u32,
    /// Warmup bars before the first valid MTF-aligned decision — the
    /// burn-in span is `warmup_bars × longest_timeframe_secs`.
    /// Default: 300.
    #[serde(default = "default_backtest_warmup_bars")]
    pub warmup_bars: u32,
    /// Persist the exact input candles per run for reproducibility.
    /// Default: true.
    #[serde(default = "default_backtest_store_input_bars")]
    pub store_input_bars: bool,
    /// Maximum equity-curve points persisted per run (downsampled).
    /// Default: 2000.
    #[serde(default = "default_backtest_max_equity_points")]
    pub max_equity_points: u32,
    /// Maximum recorded snapshots replayed per run (keeps the synchronous
    /// endpoint bounded). Default: 50_000.
    #[serde(default = "default_backtest_max_snapshots")]
    pub max_snapshots: u32,
    #[serde(default)]
    pub hyperliquid: ExchangeBacktestLimits,
    #[serde(default)]
    pub bitget: ExchangeBacktestLimits,
}

/// Per-exchange paging limits for the BTE archive backfill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeBacktestLimits {
    /// Candles per REST page.
    #[serde(default = "default_backtest_page_cap")]
    pub page_cap: u32,
    /// Delay between pages (rate-limit courtesy).
    #[serde(default = "default_backtest_rate_limit_delay_ms")]
    pub rate_limit_delay_ms: u64,
    /// Hard ceiling on pages per backfill run (bounded fetch time).
    #[serde(default = "default_backtest_max_pages_per_run")]
    pub max_pages_per_run: u32,
    /// v8.2: the endpoint's historical candle window per TF (0 = no cap).
    /// Hyperliquid's `candleSnapshot` exposes the most recent 5,000 candles;
    /// the per-TF max depth is `max_candles_per_tf × tf_secs`. Bitget pages
    /// deep history — cap 0 (the archive depth governs).
    #[serde(default = "default_backtest_max_candles_per_tf")]
    pub max_candles_per_tf: u32,
}

fn default_backtest_archive_depth_days() -> u32 {
    180
}
fn default_backtest_warmup_bars() -> u32 {
    300
}
fn default_backtest_store_input_bars() -> bool {
    true
}
fn default_backtest_max_equity_points() -> u32 {
    2000
}
fn default_backtest_max_snapshots() -> u32 {
    50_000
}
fn default_backtest_page_cap() -> u32 {
    1000
}
fn default_backtest_rate_limit_delay_ms() -> u64 {
    1000
}
fn default_backtest_max_pages_per_run() -> u32 {
    2000
}
fn default_backtest_max_candles_per_tf() -> u32 {
    0
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            archive_depth_days: default_backtest_archive_depth_days(),
            warmup_bars: default_backtest_warmup_bars(),
            store_input_bars: default_backtest_store_input_bars(),
            max_equity_points: default_backtest_max_equity_points(),
            max_snapshots: default_backtest_max_snapshots(),
            hyperliquid: ExchangeBacktestLimits {
                page_cap: 1000,
                rate_limit_delay_ms: 1000,
                max_pages_per_run: 2000,
                max_candles_per_tf: 5000,
            },
            bitget: ExchangeBacktestLimits {
                page_cap: 200,
                rate_limit_delay_ms: 100,
                max_pages_per_run: 6000,
                max_candles_per_tf: 0,
            },
        }
    }
}

impl Default for ExchangeBacktestLimits {
    fn default() -> Self {
        Self {
            page_cap: default_backtest_page_cap(),
            rate_limit_delay_ms: default_backtest_rate_limit_delay_ms(),
            max_pages_per_run: default_backtest_max_pages_per_run(),
            max_candles_per_tf: default_backtest_max_candles_per_tf(),
        }
    }
}
