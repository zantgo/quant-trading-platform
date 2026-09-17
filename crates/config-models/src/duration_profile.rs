//! duration_profile — per-duration indicator parameter baseline (v11.8,
//! v11.9 extended to the full 14-duration pool).
//!
//! The canonical per-duration tuning matrix for the supported duration
//! pool. Each duration carries explicit provenance from the trader-approved
//! tier architecture; no silent inheritance. Canonical thresholds (RSI
//! 30/50/70, RVOL 1.5/3.0, squeeze-min 5, candlestick window 5) stay
//! constant across durations — periods carry the responsiveness.
//!
//! Provenance rules:
//!   R1: 1s and 3s share the microstructure tier (T0) verbatim
//!   R2: 5s=T1, 15s=T2, 30s=T3, 1m=T4, 5m=T5, 15m=T6, 1h=T8 (verbatim)
//!   R3: 3m = T4·T5 blend (structure from T5, fast oscillators from T4)
//!   R4: v11.9 extends past the strategic ceiling: 30m=T7, 4h=T9,
//!       12h=T9·T10 lean-4h, 1d=T10
//!   R5: canonical thresholds constant
//!   R6: provenance recorded per column in the docs table

use crate::models::IndicatorsConfig;

/// All profile entries keyed by `timeframe_secs` (aligned with
/// `core_domain::SUPPORTED_DURATIONS`).
pub const PROFILE_DURATIONS: &[u64] = &[
    1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
];

/// Provenance label per duration (aligned with PROFILE_DURATIONS).
pub const PROFILE_PROVENANCE: &[&str] = &[
    "T0",
    "T0 (sub-5s family)",
    "T1",
    "T2",
    "T3",
    "T4",
    "T4·T5 blend",
    "T5",
    "T6",
    "T7",
    "T8",
    "T9",
    "T9·T10 lean-4h",
    "T10",
];

/// Resolve the tier provenance label for a duration.
pub fn provenance_for(secs: u64) -> &'static str {
    let idx = PROFILE_DURATIONS.iter().position(|&d| d == secs);
    match idx {
        Some(i) => PROFILE_PROVENANCE[i],
        None => "static default",
    }
}

/// The definitive per-duration `IndicatorsConfig` baseline. Returns a full
/// config with the matrix row for `secs` overlaid on the static defaults.
/// Durations outside the pool fall back to the static defaults.
pub fn for_duration(secs: u64) -> IndicatorsConfig {
    let mut cfg = IndicatorsConfig::default();
    match secs {
        // ── T0: microstructure (1s and 3s share verbatim) ──
        1 | 3 => {
            cfg.ema_fast = 3;
            cfg.ema_medium = 8;
            cfg.ema_slow = 21;
            cfg.ema_long = 55;
            cfg.rsi_period = 7;
            cfg.macd_fast = 5;
            cfg.macd_slow = 13;
            cfg.macd_signal = 4;
            cfg.adx_period = 7;
            cfg.adx_trend_threshold = 18;
            cfg.adx_exhaustion_threshold = 35;
            cfg.atr_period = 7;
            cfg.supertrend_period = 7;
            cfg.supertrend_multiplier = 1.8;
            cfg.donchian_period = 20;
            cfg.keltner_ema_period = 10;
            cfg.keltner_atr_period = 10;
            cfg.keltner_multiplier = 1.4;
            cfg.ichimoku_tenkan = 7;
            cfg.ichimoku_kijun = 22;
            cfg.ichimoku_senkou_b = 44;
            cfg.psar_af_step = 0.03;
            cfg.psar_af_max = 0.25;
            cfg.hull_ma_period = 12;
            cfg.stoch_k_period = 7;
            cfg.chandemo_period = 9;
            cfg.williams_r_period = 7;
            cfg.cci_period = 10;
            cfg.bbwp_period = 14;
            cfg.bbwp_lookback = 100;
            cfg.squeeze_period = 14;
            cfg.squeeze_bb_period = 14;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 14;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 30;
            cfg.obv_smoothing = 20;
            cfg.cmf_period = 10;
            cfg.mfi_period = 7;
            cfg.force_index_smoothing = 5;
            cfg.aroon_period = 14;
            cfg.chop_period = 10;
            cfg.linreg_period = 20;
            cfg.zscore_period = 20;
            cfg.volume_average_period = 20;
        }
        // ── T1: very-short momentum (5s) ──
        5 => {
            cfg.ema_fast = 3;
            cfg.ema_medium = 9;
            cfg.ema_slow = 21;
            cfg.ema_long = 55;
            cfg.rsi_period = 7;
            cfg.macd_fast = 5;
            cfg.macd_slow = 13;
            cfg.macd_signal = 4;
            cfg.adx_period = 7;
            cfg.adx_trend_threshold = 18;
            cfg.adx_exhaustion_threshold = 35;
            cfg.atr_period = 7;
            cfg.supertrend_period = 7;
            cfg.supertrend_multiplier = 1.8;
            cfg.keltner_ema_period = 10;
            cfg.keltner_atr_period = 10;
            cfg.keltner_multiplier = 1.5;
            cfg.ichimoku_tenkan = 7;
            cfg.ichimoku_kijun = 22;
            cfg.ichimoku_senkou_b = 44;
            cfg.psar_af_step = 0.03;
            cfg.psar_af_max = 0.25;
            cfg.hull_ma_period = 14;
            cfg.stoch_k_period = 7;
            cfg.chandemo_period = 9;
            cfg.williams_r_period = 7;
            cfg.cci_period = 10;
            cfg.bbwp_period = 14;
            cfg.bbwp_lookback = 120;
            cfg.squeeze_period = 14;
            cfg.squeeze_bb_period = 14;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 14;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 30;
            cfg.cmf_period = 10;
            cfg.mfi_period = 7;
            cfg.force_index_smoothing = 5;
            cfg.aroon_period = 14;
            cfg.chop_period = 10;
            cfg.linreg_period = 20;
            cfg.zscore_period = 20;
        }
        // ── T2: micro trend (15s) ──
        15 => {
            cfg.ema_fast = 5;
            cfg.ema_medium = 10;
            cfg.ema_slow = 21;
            cfg.ema_long = 55;
            cfg.rsi_period = 9;
            cfg.macd_fast = 6;
            cfg.macd_slow = 15;
            cfg.macd_signal = 5;
            cfg.adx_period = 8;
            cfg.adx_trend_threshold = 18;
            cfg.adx_exhaustion_threshold = 35;
            cfg.atr_period = 9;
            cfg.supertrend_period = 8;
            cfg.supertrend_multiplier = 2.0;
            cfg.donchian_period = 24;
            cfg.keltner_ema_period = 12;
            cfg.keltner_atr_period = 12;
            cfg.keltner_multiplier = 1.5;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.hull_ma_period = 16;
            cfg.stoch_k_period = 9;
            cfg.chandemo_period = 10;
            cfg.williams_r_period = 9;
            cfg.cci_period = 12;
            cfg.bbwp_period = 16;
            cfg.bbwp_lookback = 150;
            cfg.squeeze_period = 16;
            cfg.squeeze_bb_period = 16;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 16;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 40;
            cfg.cmf_period = 14;
            cfg.mfi_period = 9;
            cfg.force_index_smoothing = 7;
            cfg.aroon_period = 20;
            cfg.chop_period = 12;
            cfg.linreg_period = 30;
            cfg.zscore_period = 30;
        }
        // ── T3: super-fast confirmation (30s) ──
        30 => {
            cfg.ema_fast = 5;
            cfg.ema_medium = 13;
            cfg.ema_slow = 34;
            cfg.ema_long = 89;
            cfg.rsi_period = 10;
            cfg.macd_fast = 8;
            cfg.macd_slow = 17;
            cfg.macd_signal = 5;
            cfg.adx_period = 10;
            cfg.adx_trend_threshold = 18;
            cfg.adx_exhaustion_threshold = 38;
            cfg.atr_period = 10;
            cfg.supertrend_period = 10;
            cfg.supertrend_multiplier = 2.0;
            cfg.donchian_period = 30;
            cfg.keltner_ema_period = 14;
            cfg.keltner_atr_period = 14;
            cfg.keltner_multiplier = 1.6;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.psar_af_step = 0.025;
            cfg.psar_af_max = 0.22;
            cfg.hull_ma_period = 18;
            cfg.stoch_k_period = 10;
            cfg.chandemo_period = 12;
            cfg.williams_r_period = 10;
            cfg.cci_period = 14;
            cfg.bbwp_period = 18;
            cfg.bbwp_lookback = 150;
            cfg.squeeze_period = 18;
            cfg.squeeze_bb_period = 18;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 18;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 40;
            cfg.cmf_period = 14;
            cfg.mfi_period = 10;
            cfg.force_index_smoothing = 7;
            cfg.aroon_period = 20;
            cfg.chop_period = 14;
            cfg.linreg_period = 30;
            cfg.zscore_period = 30;
        }
        // ── T4: fast trading (1m) ──
        60 => {
            cfg.ema_fast = 5;
            cfg.ema_medium = 13;
            cfg.ema_slow = 34;
            cfg.ema_long = 89;
            cfg.rsi_period = 11;
            cfg.macd_fast = 8;
            cfg.macd_slow = 17;
            cfg.macd_signal = 5;
            cfg.adx_period = 10;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 10;
            cfg.supertrend_period = 10;
            cfg.supertrend_multiplier = 2.0;
            cfg.donchian_period = 30;
            cfg.keltner_ema_period = 14;
            cfg.keltner_atr_period = 14;
            cfg.keltner_multiplier = 1.6;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.psar_af_step = 0.025;
            cfg.psar_af_max = 0.22;
            cfg.hull_ma_period = 20;
            cfg.stoch_k_period = 12;
            cfg.chandemo_period = 12;
            cfg.williams_r_period = 14;
            cfg.cci_period = 14;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 200;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 50;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 40;
            cfg.zscore_period = 40;
        }
        // ── T4·T5 blend: execution confirmation (3m) ──
        // Structure/smoothing from T5, fast oscillators from T4.
        180 => {
            cfg.ema_fast = 8;
            cfg.ema_medium = 21;
            cfg.ema_slow = 55;
            cfg.ema_long = 144;
            cfg.rsi_period = 12;
            cfg.macd_fast = 12;
            cfg.macd_slow = 26;
            cfg.macd_signal = 9;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 14;
            cfg.supertrend_period = 10;
            cfg.supertrend_multiplier = 2.2;
            cfg.donchian_period = 40;
            cfg.keltner_ema_period = 20;
            cfg.keltner_atr_period = 20;
            cfg.keltner_multiplier = 1.8;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.psar_af_step = 0.02;
            cfg.psar_af_max = 0.20;
            cfg.hull_ma_period = 24;
            cfg.stoch_k_period = 12;
            cfg.chandemo_period = 14;
            cfg.williams_r_period = 14;
            cfg.cci_period = 20;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 252;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 75;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 50;
            cfg.zscore_period = 50;
        }
        // ── T5: intraday execution (5m) ──
        300 => {
            cfg.ema_fast = 8;
            cfg.ema_medium = 21;
            cfg.ema_slow = 55;
            cfg.ema_long = 144;
            cfg.rsi_period = 14;
            cfg.macd_fast = 12;
            cfg.macd_slow = 26;
            cfg.macd_signal = 9;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 14;
            cfg.supertrend_period = 10;
            cfg.supertrend_multiplier = 2.2;
            cfg.donchian_period = 40;
            cfg.keltner_ema_period = 20;
            cfg.keltner_atr_period = 20;
            cfg.keltner_multiplier = 1.8;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.psar_af_step = 0.02;
            cfg.psar_af_max = 0.20;
            cfg.hull_ma_period = 24;
            cfg.stoch_k_period = 14;
            cfg.chandemo_period = 14;
            cfg.williams_r_period = 14;
            cfg.cci_period = 20;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 252;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 75;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 50;
            cfg.zscore_period = 50;
        }
        // ── T6: intraday structure (15m) ──
        900 => {
            cfg.ema_fast = 8;
            cfg.ema_medium = 21;
            cfg.ema_slow = 55;
            cfg.ema_long = 144;
            cfg.rsi_period = 14;
            cfg.macd_fast = 12;
            cfg.macd_slow = 26;
            cfg.macd_signal = 9;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 14;
            cfg.supertrend_period = 10;
            cfg.supertrend_multiplier = 2.5;
            cfg.donchian_period = 48;
            cfg.keltner_ema_period = 20;
            cfg.keltner_atr_period = 20;
            cfg.keltner_multiplier = 2.0;
            cfg.ichimoku_tenkan = 9;
            cfg.ichimoku_kijun = 26;
            cfg.ichimoku_senkou_b = 52;
            cfg.psar_af_step = 0.02;
            cfg.psar_af_max = 0.20;
            cfg.hull_ma_period = 32;
            cfg.stoch_k_period = 14;
            cfg.chandemo_period = 14;
            cfg.williams_r_period = 14;
            cfg.cci_period = 20;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 252;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 100;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 50;
            cfg.zscore_period = 50;
        }
        // ── T7: swing structure (30m) ──
        1800 => {
            cfg.ema_fast = 10;
            cfg.ema_medium = 21;
            cfg.ema_slow = 55;
            cfg.ema_long = 144;
            cfg.rsi_period = 14;
            cfg.macd_fast = 12;
            cfg.macd_slow = 26;
            cfg.macd_signal = 9;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 14;
            cfg.supertrend_period = 12;
            cfg.supertrend_multiplier = 2.5;
            cfg.donchian_period = 55;
            cfg.keltner_ema_period = 20;
            cfg.keltner_atr_period = 20;
            cfg.keltner_multiplier = 2.0;
            cfg.ichimoku_tenkan = 12;
            cfg.ichimoku_kijun = 30;
            cfg.ichimoku_senkou_b = 60;
            cfg.psar_af_step = 0.02;
            cfg.psar_af_max = 0.20;
            cfg.hull_ma_period = 55;
            cfg.stoch_k_period = 14;
            cfg.stoch_d_period = 3;
            cfg.stoch_s_period = 3;
            cfg.chandemo_period = 14;
            cfg.williams_r_period = 14;
            cfg.cci_period = 20;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 252;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 100;
            cfg.obv_smoothing = 30;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 50;
            cfg.zscore_period = 50;
        }
        // ── T8: strategic intraday ceiling (1h) ──
        3600 => {
            cfg.ema_fast = 10;
            cfg.ema_medium = 21;
            cfg.ema_slow = 55;
            cfg.ema_long = 200;
            cfg.rsi_period = 14;
            cfg.macd_fast = 12;
            cfg.macd_slow = 26;
            cfg.macd_signal = 9;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 20;
            cfg.adx_exhaustion_threshold = 40;
            cfg.atr_period = 14;
            cfg.supertrend_period = 12;
            cfg.supertrend_multiplier = 2.5;
            cfg.donchian_period = 55;
            cfg.keltner_ema_period = 20;
            cfg.keltner_atr_period = 20;
            cfg.keltner_multiplier = 2.0;
            cfg.ichimoku_tenkan = 12;
            cfg.ichimoku_kijun = 30;
            cfg.ichimoku_senkou_b = 60;
            cfg.psar_af_step = 0.02;
            cfg.psar_af_max = 0.20;
            cfg.hull_ma_period = 55;
            cfg.stoch_k_period = 14;
            cfg.chandemo_period = 14;
            cfg.williams_r_period = 14;
            cfg.cci_period = 20;
            cfg.bbwp_period = 20;
            cfg.bbwp_lookback = 252;
            cfg.squeeze_period = 20;
            cfg.squeeze_bb_period = 20;
            cfg.squeeze_bb_std_dev = 2.0;
            cfg.squeeze_kc_period = 20;
            cfg.squeeze_kc_atr_multiplier = 1.5;
            cfg.stddev_channel_period = 100;
            cfg.obv_smoothing = 30;
            cfg.cmf_period = 20;
            cfg.mfi_period = 14;
            cfg.force_index_smoothing = 13;
            cfg.aroon_period = 25;
            cfg.chop_period = 14;
            cfg.linreg_period = 50;
            cfg.zscore_period = 50;
        }
        // ── T9: multi-session swing (4h) ──
        14400 => {
            cfg.ema_fast = 10;
            cfg.ema_medium = 20;
            cfg.ema_slow = 50;
            cfg.ema_long = 200;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 22;
            cfg.adx_exhaustion_threshold = 40;
            cfg.ichimoku_tenkan = 12;
            cfg.ichimoku_kijun = 36;
            cfg.ichimoku_senkou_b = 72;
            cfg.chandemo_period = 20;
            cfg.supertrend_period = 14;
            cfg.supertrend_multiplier = 3.0;
        }
        // ── T9·T10 lean-4h: multi-day structure (12h) ──
        43200 => {
            cfg.ema_fast = 20;
            cfg.ema_medium = 50;
            cfg.ema_slow = 100;
            cfg.ema_long = 200;
            cfg.adx_period = 14;
            cfg.adx_trend_threshold = 25;
            cfg.adx_exhaustion_threshold = 45;
        }
        // ── T10: positional (1d) ──
        86400 => {
            cfg.ema_fast = 20;
            cfg.ema_medium = 50;
            cfg.ema_slow = 100;
            cfg.ema_long = 200;
            cfg.donchian_period = 20;
            cfg.ichimoku_tenkan = 20;
            cfg.ichimoku_kijun = 60;
            cfg.ichimoku_senkou_b = 120;
        }
        _ => {}
    }
    cfg
}

/// Overlay the duration-profile values on a base config. Profile fields
/// win; base fields not covered by the matrix are preserved.
pub fn overlay(base: &IndicatorsConfig, secs: u64) -> IndicatorsConfig {
    let mut cfg = for_duration(secs);
    // Preserve operator-set values for fields the matrix does not tune:
    cfg.heatmap_leverage_tiers = base.heatmap_leverage_tiers.clone();
    cfg.pivot_points_method = base.pivot_points_method.clone();
    cfg.candlestick_min_confidence = base.candlestick_min_confidence;
    cfg.atr_multiplier_coefficient = base.atr_multiplier_coefficient;
    cfg.atr_target_rr_ratio = base.atr_target_rr_ratio;
    cfg.smc_lookback = base.smc_lookback;
    cfg.volume_profile_bins = base.volume_profile_bins;
    cfg.volume_profile_window = base.volume_profile_window;
    cfg.volume_profile_value_area = base.volume_profile_value_area;
    cfg.macd_extreme_high_threshold = base.macd_extreme_high_threshold;
    cfg.macd_extreme_low_threshold = base.macd_extreme_low_threshold;
    cfg.macd_histogram_contraction_threshold = base.macd_histogram_contraction_threshold;
    cfg.adx_slope_lookback = base.adx_slope_lookback;
    cfg.squeeze_min_duration = base.squeeze_min_duration;
    cfg
}
