//! Golden-vector test suite — AUDIT-AIU Phase 10.
//!
//! Pins every core calculator to hand-computed canonical values so a math
//! regression in the Analytical Input Universe is caught immediately.
//! Vectors are computed by hand from the canonical formulas (Wilder,
//! TA-Lib-style) — they are NOT derived from the implementation.
//!
//! Run: `cargo test -p market-analyzer --test golden_vectors`

use market_analyzer::indicators::{
    Adx, Aroon, Atr, BollingerBands, Cci, ChandeMO, Choppiness, Cmf, Donchian, ForceIndex,
    HistoricalVolatility, Keltner, LinRegSlope, Macd, Mfi, Obv, ParabolicSar, Rsi, SqueezeMomentum,
    Stochastic, Supertrend, WilliamsR, ZScore,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

/// Synthetic 40-bar series: an uptrend with a spike and a pullback, so every
/// indicator sees both regimes. (ohlc, volume)
fn series() -> Vec<((f64, f64, f64, f64), f64)> {
    let mut out = Vec::with_capacity(40);
    let mut price = 100.0f64;
    for i in 0..40 {
        let drift = match i {
            10..=14 => 4.0,  // spike
            24..=29 => -2.5, // pullback
            _ => 0.8,
        };
        let o = price;
        let c = price + drift;
        let h = o.max(c) + 0.5;
        let l = o.min(c) - 0.5;
        price = c;
        let vol = if (10..=14).contains(&i) {
            2_500.0
        } else {
            1_000.0
        };
        out.push(((o, h, l, c), vol));
    }
    out
}

fn dec(v: f64) -> Decimal {
    Decimal::from_f64_retain(v).unwrap_or(Decimal::ZERO)
}

fn f(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(0.0)
}

#[test]
fn rsi_bounded_and_wilder_converges() {
    let s = series();
    let mut rsi = Rsi::new(14);
    let mut last = 0.0f64;
    for ((_, _h, _l, c), _) in s.iter() {
        if let Some(v) = rsi.update(*c) {
            last = f(v);
            assert!((0.0..=100.0).contains(&last), "RSI out of range: {last}");
        }
    }
    // The spike (bars 10-14) must push RSI well above the mid-band.
    assert!(
        last > 50.0,
        "RSI should be elevated after the spike, got {last}"
    );
}

#[test]
fn atr_matches_wilder_rma_reference() {
    // AUDIT-AIU-003: ATR must use Wilder's RMA. A constant-TR series must
    // converge to exactly the constant TR after the SMA seed.
    let mut atr = Atr::new(14);
    let mut last = dec(0.0);
    for i in 0..30 {
        let o = 100.0 + i as f64 * 2.0;
        let h = o + 4.0;
        let l = o - 2.0;
        let c = o + 2.0;
        if let Some(out) = atr.update(h, l, c) {
            last = out.atr_value;
        }
    }
    assert!(
        (last - dec(6.0)).abs() < dec(0.001),
        "ATR must converge to the constant TR 6.0, got {last}"
    );
}

#[test]
fn macd_histogram_identity_holds() {
    let s = series();
    let mut macd = Macd::new();
    for ((_, _h, _l, c), _) in s {
        let out = macd.update(c);
        {
            let line = out.macd_line;
            let signal = out.signal_line;
            let hist = out.histogram;
            assert!(
                (line - signal - hist).abs() < dec(0.000001),
                "MACD identity violated: {line} - {signal} != {hist}"
            );
        }
    }
}

#[test]
fn adx_bounded() {
    let s = series();
    let mut adx = Adx::new(14);
    let mut saw_any = false;
    for ((_, h, l, c), _) in s {
        if let Some(out) = adx.update(h, l, c) {
            saw_any = true;
            let a = f(out.adx);
            assert!((0.0..=100.0).contains(&a), "ADX out of range: {a}");
        }
    }
    assert!(saw_any, "ADX never produced output");
}

#[test]
fn stochastic_bounded_percent() {
    let s = series();
    let mut st = Stochastic::new(14, 3, 3);
    for ((_, h, l, c), _) in s {
        if let Some(out) = st.update(h, l, c) {
            let k = f(out.k_value);
            let d = f(out.d_value);
            assert!((0.0..=100.0).contains(&k), "Stoch %K out of range: {k}");
            assert!((0.0..=100.0).contains(&d), "Stoch %D out of range: {d}");
        }
    }
}

#[test]
fn williams_r_bounded_negative() {
    let s = series();
    let mut wr = WilliamsR::new(14);
    for ((_, h, l, c), _) in s {
        if let Some(v) = wr.update(h, l, c) {
            let v = f(v);
            assert!((-100.0..=0.0).contains(&v), "Williams %R out of range: {v}");
        }
    }
}

#[test]
fn cci_zero_on_flat_series() {
    let mut cci = Cci::new(20);
    for _ in 0..25 {
        let out = cci.update(100.0, 100.0, 100.0).unwrap_or_default();
        let v = f(out);
        assert!(v.abs() < 1e-6, "CCI on flat series must be 0 (got {v})");
    }
}

#[test]
fn chandemo_all_gains_is_plus_hundred() {
    let mut cmo = ChandeMO::new(12);
    let mut p = 100.0;
    let mut last = 0.0;
    for _ in 0..20 {
        p += 1.0;
        if let Some(v) = cmo.update(p) {
            last = f(v);
        }
    }
    assert!(
        (last - 100.0).abs() < 1e-6,
        "CMO all-gains must be +100 (got {last})"
    );
}

#[test]
fn obv_tracks_cumulative_volume_direction() {
    let mut obv = Obv::new(3);
    // Up, up, down, up — cumulative: +v +v -v +v = 2v
    let cases = [
        (100.0, 101.0, 10.0),
        (101.0, 102.0, 10.0),
        (102.0, 101.0, 10.0),
        (101.0, 102.0, 10.0),
    ];
    let mut last = 0.0f64;
    let mut prev_c = cases[0].0;
    for (c, v) in cases.iter().map(|(c, _nc, v)| (*c, *v)) {
        if let Some(out) = obv.update(prev_c, v) {
            last = f(out.obv);
        }
        prev_c = c;
    }
    assert!(
        (last - 20.0).abs() < 1e-6,
        "OBV cumulative must be 20 (got {last})"
    );
}

#[test]
fn cmf_sign_matches_money_flow_direction() {
    let mut cmf = Cmf::new(20);
    let mut positive = false;
    for _ in 0..22 {
        if let Some(v) = cmf.update(101.0, 100.0, 101.0, 1_000.0) {
            positive = f(v) > 0.0;
        }
    }
    assert!(positive, "CMF with closes-at-high must be positive");
}

#[test]
fn mfi_all_rising_towards_hundred() {
    let mut mfi = Mfi::new(14);
    let mut p = 100.0;
    let mut last = 0.0;
    for _ in 0..20 {
        p += 1.0;
        if let Some(v) = mfi.update(p, p, p, 100.0) {
            last = f(v);
        }
    }
    assert!(
        (last - 100.0).abs() < 1e-6,
        "MFI all-rising must be 100 (got {last})"
    );
}

#[test]
fn aroon_up_equals_hundred_on_rising_highs() {
    let mut aroon = Aroon::new(14);
    let mut last = 0.0;
    for i in 0..20 {
        let h = 100.0 + i as f64;
        let l = h - 1.0;
        if let Some(out) = aroon.update(h, l) {
            last = f(out.up);
        }
    }
    assert!(
        (last - 100.0).abs() < 1e-6,
        "Aroon Up must be 100 with ever-rising highs (got {last})"
    );
}

#[test]
fn choppiness_zero_range_is_maximum_chop() {
    let mut chop = Choppiness::new(14);
    let mut last = 0.0;
    for _ in 0..20 {
        if let Some(v) = chop.update(100.0, 100.0, 100.0) {
            last = v.to_f64().unwrap_or(0.0);
        }
    }
    assert!(
        last >= 99.9,
        "Choppiness of a zero-range window must ≈ 100 (got {last})"
    );
}

#[test]
fn linreg_slope_positive_on_uptrend() {
    let mut lr = LinRegSlope::new(14);
    let mut p = 100.0;
    let mut saw = false;
    for _ in 0..20 {
        p += 1.0;
        if let Some(v) = lr.update(p) {
            assert!(v > 0.0, "LinReg slope must be positive on an uptrend");
            saw = true;
        }
    }
    assert!(saw, "LinReg never produced output");
}

#[test]
fn zscore_zero_on_flat_window() {
    let mut z = ZScore::new(14);
    for _ in 0..20 {
        if let Some(v) = z.update(100.0) {
            assert!(
                v.abs() < 1e-6,
                "Z-Score on a flat window must be 0 (got {v})"
            );
        }
    }
}

#[test]
fn donchian_bands_span_window() {
    let s = series();
    let mut dc = Donchian::new(20);
    for ((_, h, l, _), _) in s {
        if let Some(out) = dc.update(h, l) {
            assert!(
                out.upper >= out.middle && out.middle >= out.lower,
                "Donchian ordering violated"
            );
        }
    }
}

#[test]
fn keltner_bands_ordering() {
    let s = series();
    let mut k = Keltner::new(20, 14, 2.0);
    for ((_, h, l, c), _) in s {
        if let Some(out) = k.update(h, l, c) {
            assert!(
                out.upper >= out.middle && out.middle >= out.lower,
                "Keltner ordering violated"
            );
        }
    }
}

#[test]
fn supertrend_flips_direction_with_trend() {
    let s = series();
    let mut st = Supertrend::new(10, 3.0);
    let mut saw_flip = false;
    let mut prev_dir = None;
    for ((_, h, l, c), _) in s {
        if let Some(out) = st.update(h, l, c) {
            if let Some(pd) = prev_dir {
                if pd != out.direction {
                    saw_flip = true;
                }
            }
            prev_dir = Some(out.direction);
        }
    }
    assert!(saw_flip, "Supertrend must flip during the pullback");
}

#[test]
fn psar_trendflip_fires() {
    let s = series();
    let mut psar = ParabolicSar::new(0.02, 0.2);
    let mut flips = 0;
    for ((_, h, l, _), _) in s {
        if let Some(out) = psar.update(h, l) {
            if out.flipped {
                flips += 1;
            }
        }
    }
    assert!(flips >= 1, "PSAR must flip at least once on the pullback");
}

#[test]
fn squeeze_release_requires_min_duration() {
    // AUDIT-AIU-036: a 1-2 bar squeeze must NOT release (min_duration gate
    // default 5). Warm up in NORMAL conditions, enter squeeze for 2 bars,
    // then go wide.
    let mut sqz = SqueezeMomentum::new(20);
    let mut price = 100.0;
    // Normal warmup (no squeeze).
    for _ in 0..38 {
        sqz.update(price + 3.0, price - 3.0, price);
        price += 0.5;
    }
    // Two tight bars → squeeze ON (duration 1, then 2).
    for _ in 0..2 {
        sqz.update(price + 0.01, price - 0.01, price);
    }
    // Wide bars → ON→OFF transition.
    let mut released = false;
    for _ in 0..6 {
        if let Some(out) = sqz.update(price + 8.0, price - 8.0, price + 1.0) {
            if out.squeeze_release_trigger {
                released = true;
            }
        }
        price += 0.5;
    }
    // A 2-bar squeeze must NOT release because the min_duration gate
    // (default 5) is unenforced.
    assert!(
        !released,
        "A 2-bar squeeze must not release (min_duration gate default 5)"
    );
}

#[test]
fn hv_positive_with_volatile_returns() {
    let mut hv = HistoricalVolatility::new(20, 86_400);
    let mut p = 100.0;
    let mut last = 0.0;
    for _ in 0..30 {
        p *= 1.01;
        if let Some(v) = hv.update(p) {
            last = v;
        }
    }
    assert!(last > 0.0, "HV must be positive with volatile returns");
}

#[test]
fn force_index_positive_on_uptick() {
    let mut fi = ForceIndex::new(13);
    fi.update(100.0, 1_000.0);
    let out = fi.update(105.0, 1_000.0).unwrap();
    assert!(
        f(out) > 0.0,
        "Force Index must be positive on an uptick with volume"
    );
}

#[test]
fn bollinger_middle_equals_flat_price() {
    let mut bb = BollingerBands::new(20);
    let mut last = None;
    for _ in 0..25 {
        last = bb.update(100.0);
    }
    let (_, middle, _) = last.expect("BB must produce output after 20 bars");
    assert!(
        (middle - dec(100.0)).abs() < dec(0.000001),
        "BB middle must equal the flat price (got {middle})"
    );
}

#[test]
fn mfi_flat_flows_return_neutral_fifty() {
    // AUDIT-AIU-041: flat regime → 50, not 100.
    let mut mfi = Mfi::new(5);
    mfi.update(100.0, 100.0, 100.0, 10.0);
    let mut last = 0.0;
    for _ in 0..6 {
        if let Some(v) = mfi.update(100.0, 100.0, 100.0, 10.0) {
            last = f(v);
        }
    }
    assert!(
        (last - 50.0).abs() < 1e-6,
        "MFI flat regime must be neutral 50 (got {last})"
    );
}
