//! Property + boundary tests for the Fractional Normalized Indicator Model.
//!
//! Verifies that raw telemetry maps into the continuous `[-1.0, 1.0]` scale
//! with the correct context-aware state labels at each mathematical boundary.

use market_analyzer::indicators::normalized::{DivergenceState, NormalizationEngine};
use proptest::prelude::*;

// ─────────────────────────── RSI ───────────────────────────

proptest! {
    #[test]
    fn rsi_always_within_unit_interval(rsi in 0.0f64..=100.0) {
        let v = NormalizationEngine::normalize_rsi(rsi, DivergenceState::None);
        prop_assert!(v.normalized >= -1.0 && v.normalized <= 1.0);
    }

    #[test]
    fn rsi_oversold_maps_positive_accumulation(rsi in 0.0f64..30.0) {
        let v = NormalizationEngine::normalize_rsi(rsi, DivergenceState::None);
        prop_assert!(v.normalized >= 0.70 && v.normalized <= 1.0,
            "rsi={} normalized={}", rsi, v.normalized);
        prop_assert_eq!(v.state_label, "OVERSOLD_ACCUMULATION");
    }

    #[test]
    fn rsi_overbought_maps_negative_distribution(rsi in 70.0001f64..=100.0) {
        let v = NormalizationEngine::normalize_rsi(rsi, DivergenceState::None);
        prop_assert!(v.normalized <= -0.70 && v.normalized >= -1.0,
            "rsi={} normalized={}", rsi, v.normalized);
        prop_assert_eq!(v.state_label, "OVERBOUGHT_DISTRIBUTION");
    }
}

#[test]
fn rsi_equilibrium_at_fifty() {
    let v = NormalizationEngine::normalize_rsi(50.0, DivergenceState::None);
    assert!((v.normalized - 0.0).abs() < 1e-9);
    assert_eq!(v.state_label, "EQUILIBRIUM");
}

#[test]
fn rsi_confirmed_divergence_hard_overrides() {
    let bull = NormalizationEngine::normalize_rsi(80.0, DivergenceState::ConfirmedBullish);
    assert_eq!(bull.normalized, 1.0);
    // AUDIT-AIU-033: the override label must not be the price-zone label
    // (which would trigger a spurious OVERSOLD Threshold at RSI=80).
    assert_eq!(bull.state_label, "DIVERGENCE_BULLISH_CONFIRMED");
    let bear = NormalizationEngine::normalize_rsi(20.0, DivergenceState::ConfirmedBearish);
    assert_eq!(bear.normalized, -1.0);
    assert_eq!(bear.state_label, "DIVERGENCE_BEARISH_CONFIRMED");
}

#[test]
fn rsi_potential_divergence_capped() {
    let v = NormalizationEngine::normalize_rsi(10.0, DivergenceState::PotentialBullish);
    assert!(v.normalized <= 0.90);
}

// ─────────────────────────── RVOL ───────────────────────────
//
// Per the v2.1 contract (see `04-02-19-rvol.md` §3 and `normalized/context.rs`),
// RVOL is a non-directional gate — `normalized` is always `0.0` and the band
// value lives in `values.rvol_band`. The previous version of these tests
// asserted the old (incorrect) signed-into-normalized behaviour.

proptest! {
    #[test]
    fn rvol_below_one_band_is_negative(rvol in 0.0f64..1.0) {
        let v = NormalizationEngine::normalize_rvol(rvol, 1.5, 3.0);
        prop_assert_eq!(v.normalized, 0.0, "rvol gate normalized is always 0.0");
        let band = v.values.as_ref().and_then(|m| m.get("rvol_band")).copied();
        prop_assert_eq!(band, Some(-0.5));
        prop_assert_eq!(v.state_label, "LOW_PARTICIPATION_VOLUME");
    }

    #[test]
    fn rvol_climax_band_is_negative_one(rvol in 3.0f64..100.0) {
        let v = NormalizationEngine::normalize_rvol(rvol, 1.5, 3.0);
        prop_assert_eq!(v.normalized, 0.0, "rvol gate normalized is always 0.0");
        let band = v.values.as_ref().and_then(|m| m.get("rvol_band")).copied();
        prop_assert_eq!(band, Some(-1.0));
        prop_assert_eq!(v.state_label, "EXHAUSTION_CLIMAX_VOLUME");
    }
}

#[test]
fn rvol_institutional_band_is_positive() {
    let v = NormalizationEngine::normalize_rvol(2.0, 1.5, 3.0);
    assert_eq!(v.normalized, 0.0);
    let band = v.values.as_ref().and_then(|m| m.get("rvol_band")).copied();
    assert_eq!(band, Some(0.8));
    assert_eq!(v.state_label, "INSTITUTIONAL_BREAKOUT_VOLUME");
}

// ─────────────────────────── ADX ───────────────────────────

proptest! {
    #[test]
    fn adx_always_within_unit_interval(
        adx in 0.0f64..=100.0,
        plus in 0.0f64..=100.0,
        minus in 0.0f64..=100.0,
        slope in -10.0f64..=10.0,
    ) {
        let v = NormalizationEngine::normalize_adx(adx, plus, minus, slope, false);
        prop_assert!(v.normalized >= -1.0 && v.normalized <= 1.0);
    }

    #[test]
    fn adx_congestion_is_neutral(adx in 0.0f64..18.0) {
        // SIG-14: the v2.1 congestion band is `adx < 18` (`TRENDLESS_CONGESTION`).
        // The `[18, 20)` zone is the smooth `TRANSITION_BULL/BEAR_TREND`
        // ramp — non-zero on purpose to avoid the discontinuity at the
        // 20 boundary.
        let v = NormalizationEngine::normalize_adx(adx, 30.0, 10.0, 1.0, false);
        prop_assert_eq!(v.normalized, 0.0);
    }

    #[test]
    fn adx_transition_ramp_is_continuous(adx in 18.0f64..20.0) {
        // The transition ramp maps [18, 20) → [0.0, 0.30) × sign, smoothly.
        // Verify continuity at both endpoints.
        let v = NormalizationEngine::normalize_adx(adx, 30.0, 10.0, 1.0, false);
        prop_assert!(v.normalized >= 0.0 && v.normalized <= 0.30);
        let label = v.state_label.as_str();
        prop_assert!(label == "TRANSITION_BULL_TREND" || label == "EMERGING_BULL_TREND");
    }
}

#[test]
fn adx_sign_follows_di_bias() {
    let bull = NormalizationEngine::normalize_adx(30.0, 30.0, 10.0, 1.0, false);
    let bear = NormalizationEngine::normalize_adx(30.0, 10.0, 30.0, 1.0, false);
    assert!(bull.normalized > 0.0);
    assert!(bear.normalized < 0.0);
}

// ─────────────────────────── BBWP ───────────────────────────

proptest! {
    #[test]
    fn bbwp_within_unit_interval(bbwp in 0.0f64..=100.0, bias in -1i8..=1) {
        let v = NormalizationEngine::normalize_bbwp(bbwp, bias);
        prop_assert!(v.normalized >= -1.0 && v.normalized <= 1.0);
    }
}

#[test]
fn bbwp_compression_is_neutral() {
    let v = NormalizationEngine::normalize_bbwp(5.0, 1);
    assert_eq!(v.normalized, 0.0);
    assert_eq!(v.state_label, "MAX_VOLATILITY_COMPRESSION");
}

#[test]
fn bbwp_contract_zero_normalized_for_all_regimes() {
    // Per the canonical contract in
    // docs/engines/market-monitoring-engine/indicators/04-02-27-bbwp.md §6,
    // BBWP is a non-directional gate: `normalized` is contractually 0.0
    // for every regime. The legacy bias-signed output was retired because
    // it sign-flipped bearish breakouts into bullish signals when used
    // as a multiplier.
    let regimes = [
        ("MAX_VOLATILITY_COMPRESSION", 5.0),
        ("LOW_VOLATILITY_BULL_CYCLE", 25.0),
        ("NORMAL_VOLATILITY_BULL_CYCLE", 50.0),
        ("HIGH_VOLATILITY_BULL_EXPANSION", 80.0),
        ("VOLATILITY_EXHAUSTION_REVERSION_WARNING", 95.0),
    ];
    for (expected_label, bbwp) in regimes {
        for bias in -1i8..=1i8 {
            let v = NormalizationEngine::normalize_bbwp(bbwp, bias);
            assert_eq!(
                v.normalized, 0.0,
                "BBWP normalized must be 0.0 for regime '{expected_label}' (bias={bias}); got {}",
                v.normalized,
            );
            assert_eq!(v.state_label, expected_label);
            assert_eq!(
                v.readiness,
                core_domain::axes::Readiness::Solid,
                "BBWP inputs are complete for regime {expected_label} (bias {bias})"
            );
            let gate = v
                .values
                .as_ref()
                .and_then(|vals| vals.get("bbwp_gate"))
                .copied()
                .unwrap_or(0.0);
            assert!(
                gate > 0.0,
                "BBWP must retain its documented gate band in values.bbwp_gate (regime {expected_label}, bias {bias}); got {gate}"
            );
        }
    }
}

// ─────────────────────────── MACD / Squeeze / VWAP ───────────────────────────

#[test]
fn macd_bullish_crossover_below_zero_accelerates() {
    let v = NormalizationEngine::normalize_macd(-12.4, -17.6, 5.2, 8.0, Some(1));
    assert!(v.normalized >= 0.8);
    assert_eq!(v.state_label, "BULLISH_CROSSOVER_ACCELERATING");
}

#[test]
fn macd_fomo_crossover_above_zero_rejected() {
    // AUDIT-AIU-022: a zero-line-filtered (FOMO) crossover must contribute
    // ZERO to the directional accumulator per 04-02-17. This test previously
    // codified the buggy ±0.2 leak.
    let v = NormalizationEngine::normalize_macd(12.4, 8.0, 4.0, 8.0, Some(1));
    assert_eq!(v.normalized, 0.0);
    assert_eq!(v.state_label, "FOMO_BULLISH_CROSSOVER_REJECTED");
}

#[test]
fn macd_panic_crossover_below_zero_rejected() {
    // AUDIT-AIU-022: symmetric PANIC rejection must also contribute zero.
    let v = NormalizationEngine::normalize_macd(-12.4, -8.0, -4.0, -8.0, Some(-1));
    assert_eq!(v.normalized, 0.0);
    assert_eq!(v.state_label, "PANIC_BEARISH_CROSSOVER_REJECTED");
}

// ─────────────────── AUDIT-AIU-020 + P0-1 (v6.10.17): Williams %R ───────────────────

proptest! {
    #[test]
    fn williams_r_normalized_sign_follows_convention(wr in -100.0f64..=0.0f64) {
        // v6.10.17 (P0-1): the platform's uniform RSI convention — price at
        // the period high (wr >= −20, OVERBOUGHT) contributes NEGATIVE
        // (distribution warning), price at the period low (wr <= −80,
        // OVERSOLD) contributes POSITIVE (accumulation), and the middle
        // band signs by momentum bias (wr > −50 bullish, < −50 bearish).
        let v = NormalizationEngine::normalize_williams_r(wr);
        let expected = if wr >= -20.0 {
            -0.6 - ((wr + 20.0) / 20.0) * 0.4
        } else if wr <= -80.0 {
            0.6 + ((-80.0 - wr) / 20.0) * 0.4
        } else {
            (wr + 50.0) / 50.0
        };
        prop_assert!((v.normalized - expected).abs() < 1e-9);
        prop_assert!(v.normalized >= -1.0 && v.normalized <= 1.0);
        // Sign per band (the OVERBOUGHT/OVERSOLD boundaries are warning
        // state transitions — the sign flips exactly there).
        if wr >= -20.0 {
            prop_assert!(v.normalized < 0.0, "wr={wr} overbought must be negative");
        } else if wr <= -80.0 {
            prop_assert!(v.normalized > 0.0, "wr={wr} oversold must be positive");
        }
    }
}

#[test]
fn williams_r_extreme_magnitudes_match_label_convention() {
    // v6.10.17 (P0-1): deeply overbought (wr near 0) → −1.0 (strongest
    // distribution warning); deeply oversold (wr near −100) → +1.0
    // (strongest accumulation opportunity). The legacy mapping inverted
    // both (wr=0 → +1.0 next to the OVERBOUGHT label).
    let top = NormalizationEngine::normalize_williams_r(0.0);
    assert_eq!(top.normalized, -1.0);
    assert_eq!(top.state_label, "WILLIAMS_R_OVERBOUGHT");
    let bottom = NormalizationEngine::normalize_williams_r(-100.0);
    assert_eq!(bottom.normalized, 1.0);
    assert_eq!(bottom.state_label, "WILLIAMS_R_OVERSOLD");
}

#[test]
fn williams_r_midline_is_neutral() {
    // AUDIT-AIU-020: wr = -50 must map to 0.0 (was +0.6, a spurious
    // strong-bullish vote at the neutral point).
    let v = NormalizationEngine::normalize_williams_r(-50.0);
    assert!((v.normalized - 0.0).abs() < 1e-9);
}

#[test]
fn williams_r_sign_flips_at_the_warning_boundaries() {
    // The OVERBOUGHT (≥ −20) and OVERSOLD (≤ −80) bands flip the sign
    // against the middle bias band — this is the label semantics (a
    // warning state is a bearish contribution), and the magnitude stays
    // continuous (±0.6 at the boundaries).
    let just_overbought = NormalizationEngine::normalize_williams_r(-19.9999);
    let mid_bull = NormalizationEngine::normalize_williams_r(-20.0001);
    assert!(just_overbought.normalized < 0.0);
    assert!(mid_bull.normalized > 0.0);
    assert!((just_overbought.normalized.abs() - 0.6).abs() < 0.01);
    assert!((mid_bull.normalized.abs() - 0.6).abs() < 0.01);
}

// ─────────────────── AUDIT-AIU-021: ADX continuity ───────────────────

#[test]
fn adx_normalization_continuous_at_25_and_40() {
    // AUDIT-AIU-021: the piecewise must not jump at ADX = 25 or 40.
    let v_before = NormalizationEngine::normalize_adx(24.999, 20.0, 10.0, 0.0, false);
    let v_at = NormalizationEngine::normalize_adx(25.0, 20.0, 10.0, 0.0, false);
    let v_after = NormalizationEngine::normalize_adx(25.001, 20.0, 10.0, 0.0, false);
    assert!((v_before.normalized - v_at.normalized).abs() < 0.001);
    assert!((v_after.normalized - v_at.normalized).abs() < 0.001);

    let v_before = NormalizationEngine::normalize_adx(39.999, 20.0, 10.0, 0.0, false);
    let v_at = NormalizationEngine::normalize_adx(40.0, 20.0, 10.0, 0.0, false);
    let v_after = NormalizationEngine::normalize_adx(40.001, 20.0, 10.0, 0.0, false);
    assert!((v_before.normalized - v_at.normalized).abs() < 0.001);
    assert!((v_after.normalized - v_at.normalized).abs() < 0.001);
}

// ─────────────────── AUDIT-AIU-024: mark-index spread gate ───────────────────

#[test]
fn squeeze_on_is_coiling() {
    use market_analyzer::indicators::squeeze::MomentumDirection;
    let v = NormalizationEngine::normalize_squeeze(true, false, 0.0, MomentumDirection::Flat);
    assert_eq!(v.normalized, 0.0);
    assert_eq!(v.state_label, "COMPRESSION_COILING");
}

#[test]
fn vwap_extreme_premium_is_reversion() {
    let v = NormalizationEngine::normalize_vwap(101.5, 100.0);
    assert_eq!(v.normalized, -0.8);
    assert_eq!(v.state_label, "EXTREME_PREMIUM_REVERSION_ZONE");
}
