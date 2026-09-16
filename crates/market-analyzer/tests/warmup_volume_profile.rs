//! Regression test: warm-up snapshots must carry a populated `volume_profile`
//! from the same bar the live analyzer does (i.e. the 250th bar in for the
//! default `volume_profile_window=500`). Before this fix the warm-up snapshot
//! always serialised `volume_profile: None`, so any dashboard that fetched
//! `/api/history?…` before the first live candle close saw nothing for fast /
//! slow / macro TFs (micro recovered fastest because its 250-bar gate clears
//! within minutes, the others sat empty until their first live close).

use config_models::{FibonacciConfig, TimeframeConfig};
use core_domain::normalized::{Exchange, NormalizedCandle};
use rust_decimal::Decimal;

fn make_test_config(window: usize, bins: usize) -> TimeframeConfig {
    use config_models::IndicatorsConfig;
    TimeframeConfig {
        candles: config_models::CandlesConfig {
            duration_seconds: 60,
        },
        indicators: IndicatorsConfig {
            ema_fast: 10,
            ema_medium: 50,
            ema_slow: 100,
            ema_long: 200,
            rsi_period: 14,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            adx_period: 14,
            atr_period: 14,
            squeeze_period: 20,
            squeeze_bb_period: 20,
            squeeze_bb_std_dev: 2.0,
            squeeze_kc_period: 20,
            stoch_k_period: 18,
            stoch_d_period: 5,
            stoch_s_period: 9,
            chandemo_period: 12,
            supertrend_period: 10,
            supertrend_multiplier: 3.0,
            keltner_ema_period: 20,
            keltner_atr_period: 10,
            keltner_multiplier: 2.0,
            donchian_period: 20,
            obv_smoothing: 20,
            cmf_period: 20,
            mfi_period: 20,
            hv_period: 20,
            aroon_period: 25,
            chop_period: 14,
            linreg_period: 20,
            zscore_period: 20,
            bbwp_lookback: 252,
            bbwp_period: 20,
            macd_extreme_high_threshold: 1000.0,
            macd_extreme_low_threshold: -1000.0,
            macd_histogram_contraction_threshold: 0.3,
            adx_trend_threshold: 20,
            adx_exhaustion_threshold: 40,
            adx_slope_lookback: 3,
            squeeze_min_duration: 5,
            squeeze_kc_atr_multiplier: 1.5,
            atr_multiplier_coefficient: 2.0,
            atr_target_rr_ratio: 2.5,
            volume_average_period: 20,
            rvol_threshold_institutional: 1.5,
            rvol_threshold_climax: 3.0,
            ichimoku_tenkan: 9,
            ichimoku_kijun: 26,
            ichimoku_senkou_b: 52,
            ichimoku_displacement: 26,
            cci_period: 20,
            psar_af_step: 0.02,
            psar_af_max: 0.2,
            williams_r_period: 14,
            hull_ma_period: 21,
            force_index_smoothing: 13,
            stddev_channel_period: 20,
            smc_lookback: 20,
            volume_profile_bins: bins,
            volume_profile_window: window,
            volume_profile_value_area: 0.7,
            ..Default::default()
        },
        leverage: Default::default(),
    }
}

fn fib_config() -> FibonacciConfig {
    FibonacciConfig {
        swing_lookback: 5,
        swing_scan_range: 50,
        retracement_coefficients: vec![0.236, 0.382, 0.5, 0.618, 0.786],
        extension_coefficients: vec![1.0, 1.272, 1.618],
    }
}

/// Build `count` synthetic candles spanning a realistic walk. Each candle has
/// open/high/low/close around a slowly-trending midpoint and a synthetic
/// volume that's higher for bullish candles (so the buy/sell split is
/// meaningful for later parity checks).
fn synth_candles(count: usize, secs_per_candle: u64) -> Vec<NormalizedCandle> {
    let base_ts: u64 = 1_700_000_000_000;
    let mut candles = Vec::with_capacity(count);
    for i in 0..count {
        let ts = base_ts + (i as u64) * secs_per_candle * 1000;
        let drift = (i as f64) * 0.05;
        let mid = 50_000.0 + drift;
        let span = 12.0 + (i as f64 % 7.0);
        let high = mid + span;
        let low = mid - span;
        // Bullish every 3rd bar, bearish otherwise — exercises buy/sell split.
        let (open, close) = if i % 3 == 0 { (low, high) } else { (high, low) };
        let vol = 10.0 + (i as f64 % 13.0);
        candles.push(NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDC".to_string(),
            start_time_ms: ts,
            duration_ms: secs_per_candle * 1000,
            trades_count: 10,
            reconstructed: None,
            open: Decimal::from_f64_retain(open).unwrap_or_default(),
            high: Decimal::from_f64_retain(high).unwrap_or_default(),
            low: Decimal::from_f64_retain(low).unwrap_or_default(),
            close: Decimal::from_f64_retain(close).unwrap_or_default(),
            volume: Decimal::from_f64_retain(vol).unwrap_or_default(),
        });
    }
    candles
}

#[test]
fn warmup_populates_volume_profile_from_gate_bar_onward() {
    // 500 candles is the default `volume_profile_window`; gate clears at 250.
    let candles = synth_candles(500, 60);
    let cfg = make_test_config(500, 50);

    let warmed = market_analyzer::analyzer::warm::warm_indicators_for_timeframe(
        candles.clone(),
        &cfg,
        &fib_config(),
        "BTC-USDC",
        60,
        core_domain::models::TimeframeSlot::Micro1,
        500,
        &market_analyzer::active_set::ActiveSet::all_enabled(),
        Some(core_domain::normalized::Exchange::Hyperliquid),
    );

    assert!(
        !warmed.snapshot_history.is_empty(),
        "warmup should produce at least one snapshot",
    );

    // The seeded path uses a soft floor of 25 bars (so sub-minute TFs paint
    // a profile on first mount). Pre-floor snapshots should still be None.
    let pre_floor_nones = warmed
        .snapshot_history
        .iter()
        .take(24)
        .filter(|s| s.volume_profile.is_none())
        .count();
    assert!(
        pre_floor_nones >= 23,
        "expected the first ~24 warm-up snapshots to have volume_profile:None under soft floor (got {}/24)",
        pre_floor_nones,
    );

    // From the 25th snapshot onward (under the soft seeded floor) the bin-level
    // snapshot must be present. The LIVE per-candle path keeps the strict
    // `window_size / 2 = 250` gate; only the warm-up path softens it.
    let post_floor_snapshots: Vec<_> = warmed
        .snapshot_history
        .iter()
        .skip(24)
        .filter(|s| s.volume_profile.is_some())
        .collect();
    assert!(
        !post_floor_snapshots.is_empty(),
        "no warm-up snapshots past the soft-floor carried volume_profile",
    );

    // Last snapshot must have the bin-level profile (this is what /api/history reads).
    let last_vp = warmed
        .snapshot_history
        .last()
        .and_then(|s| s.volume_profile.as_ref())
        .expect("last warm-up snapshot must carry volume_profile");

    assert!(
        !last_vp.bins.is_empty(),
        "last warm-up volume_profile should have populated bins",
    );
    assert!(
        last_vp.bins.len() <= 100,
        "bin count must not exceed the configured volume_profile_bins (got {})",
        last_vp.bins.len(),
    );
    assert!(
        last_vp.poc_price >= last_vp.range_low && last_vp.poc_price <= last_vp.range_high,
        "POC must be within range (got poc={}, range=({}..{}))",
        last_vp.poc_price,
        last_vp.range_low,
        last_vp.range_high,
    );
    // Bins sorted ascending by price_low per doc contract.
    let mut prev_lo = f64::NEG_INFINITY;
    for bin in &last_vp.bins {
        assert!(
            bin.price_low >= prev_lo,
            "bins must be sorted ascending by price_low (got {} after {})",
            bin.price_low,
            prev_lo,
        );
        prev_lo = bin.price_low;
    }
    // POC bin must be the bin with highest volume.
    let poc_bin = last_vp
        .bins
        .iter()
        .find(|b| b.is_poc)
        .expect("exactly one bin must be POC");
    let max_vol = last_vp
        .bins
        .iter()
        .map(|b| b.volume)
        .fold(0.0_f64, f64::max);
    assert!(
        (poc_bin.volume - max_vol).abs() < 1e-9,
        "POC bin must be the highest-volume bin",
    );
}

#[test]
fn warmup_sub_minute_timeframes_also_populate() {
    // Sub-minute timeframes (e.g. 5s bars) are supported by `duration_seconds`
    // and must also produce the bin-level snapshot in warm-up, otherwise the
    // dashboard would still show no volume profile until the first live
    // sub-minute close — exactly the bug the user reported.
    let candles = synth_candles(500, 5);
    let cfg = make_test_config(500, 50);

    let warmed = market_analyzer::analyzer::warm::warm_indicators_for_timeframe(
        candles,
        &cfg,
        &fib_config(),
        "BTC-USDC",
        5,
        core_domain::models::TimeframeSlot::Micro1,
        500,
        &market_analyzer::active_set::ActiveSet::all_enabled(),
        Some(core_domain::normalized::Exchange::Hyperliquid),
    );

    let last_vp = warmed
        .snapshot_history
        .last()
        .and_then(|s| s.volume_profile.as_ref())
        .expect("5s-TF warm-up must carry volume_profile on last snapshot");
    assert!(!last_vp.bins.is_empty());
    assert_eq!(last_vp.timeframe_secs, 5);
    assert_eq!(last_vp.timeframe_slot, "micro1");
}

/// Seeded path soft floor (`min_bars = 25`): volume-profile must render as
/// soon as the warm-up reaches 25 candles, regardless of the strict
/// `window_size / 2 = 250` gate enforced by the live path. This is what lets
/// sub-minute TFs (where the venue caps history at 26–51 bars) paint a bin
/// distribution on first mount for parity with every other indicator.
#[test]
fn seeded_volume_profile_clears_at_25_bars() {
    let candles = synth_candles(40, 15); // 40 × 15 s = 10 min warm-up
    let cfg = make_test_config(500, 50);

    let warmed = market_analyzer::analyzer::warm::warm_indicators_for_timeframe(
        candles,
        &cfg,
        &fib_config(),
        "BTC-USDC",
        15,
        core_domain::models::TimeframeSlot::Micro1,
        500,
        &market_analyzer::active_set::ActiveSet::all_enabled(),
        Some(core_domain::normalized::Exchange::Hyperliquid),
    );

    let populated: Vec<_> = warmed
        .snapshot_history
        .iter()
        .filter(|s| s.volume_profile.is_some())
        .collect();
    assert!(
        !populated.is_empty(),
        "seeded path should populate volume_profile from bar 25 onward",
    );
    // The last snapshot must be the most recent and must have bins.
    let last_vp = warmed
        .snapshot_history
        .last()
        .and_then(|s| s.volume_profile.as_ref())
        .expect("last warm-up snapshot must carry volume_profile after 40-bar seed");
    assert!(
        !last_vp.bins.is_empty(),
        "bins array must be non-empty for the chart primitive to render anything",
    );
    assert!(
        last_vp.range_high > last_vp.range_low,
        "range must span the seeded bar window",
    );
}

/// Live gate remains strict at `window_size / 2 = 250` — even though the
/// seeded path softens this for warm-up, the live per-candle path must keep
/// the full half-window gate so we never represent an under-filled live
/// window as a real profile.
#[test]
fn volume_profile_indicator_keeps_strict_live_gate() {
    use market_analyzer::indicators::VolumeProfile;
    let mut vp = VolumeProfile::new(500, 50, 0.7);
    let mut last_reading = None;
    for i in 0..249 {
        let price = 50_000.0 + i as f64 * 0.01;
        last_reading = vp.update_with_open(price, price, price, price, 1.0);
    }
    assert!(
        last_reading.is_none(),
        "live `update_with_open` must reject bars below window_size/2 (got Some at 249 bars)",
    );
    assert!(
        vp.compute().is_none(),
        "live `compute` must reject bars below window_size/2 (got Some at 249 bars)",
    );
    assert!(
        vp.compute_bins().is_none(),
        "live `compute_bins` must reject bars below window_size/2 (got Some at 249 bars)",
    );

    // Soft floor (used by the seeded path) lets the indicator report with
    // as few as 25 bars.
    let reading_25 = vp.compute_with_min_bars(25);
    assert!(
        reading_25.is_some() || last_reading.is_some(),
        "compute_with_min_bars(25) must work with however many bars are present",
    );
}
