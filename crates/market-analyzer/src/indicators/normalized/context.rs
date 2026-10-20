//! Context-dependent normalization mappers.
//!
//! These mappers require directional/positional context (trend bias, price
//! vs VWAP, DI crossover sign, active position, RVOL) supplied via
//! [`NormalizationContext`]. They remain pure/static and side-effect free.

use super::{
    clamp_unit, pick, NormalizationContext, NormalizationEngine, NormalizedIndicatorValue,
};
use super::{IndicatorSignal, SignalDirection, SignalKind, SignalStatus};
use std::collections::HashMap;

impl NormalizationEngine {
    /// ADX: non-directional strength signed by the +DI/-DI crossover bias.
    pub fn normalize_adx(
        adx: f64,
        plus_di: f64,
        minus_di: f64,
        adx_slope: f64,
        consecutive_deceleration: bool,
    ) -> NormalizedIndicatorValue {
        let sign = if plus_di >= minus_di { 1.0 } else { -1.0 };
        // SIG-14 (v2.1): the original bands were `< 20` → `<= 20` → `<= 25` → `<= 40` → else.
        // That classification collapsed the `[18, 20)` zone into "TRENDLESS_CONGESTION",
        // producing a discontinuous label flip at ADX = 20. The corrected form below
        // carries five bands (matching the doc's regime table):
        //   - `TRENDLESS_CONGESTION`        : adx < 18
        //   - `TRANSITION_BULL/BEAR_TREND`  : 18 ≤ adx < 20
        //   - `EMERGING_BULL/BEAR_TREND`    : 20 ≤ adx ≤ 25
        //   - `STRONG_BULL/BEAR_TREND`      : 25 < adx ≤ 40
        //   - `CLIMACTIC_BULL/BEAR_TREND`   : adx > 40
        let (norm, label): (f64, String) = if adx < 18.0 {
            (0.0, "TRENDLESS_CONGESTION".into())
        } else if adx < 20.0 {
            // Smooth transition ramp: 0.0 → 0.30 across [18, 20)
            let ramp = (adx - 18.0) / 2.0;
            (
                sign * (0.30 * ramp),
                pick(sign, "TRANSITION_BULL_TREND", "TRANSITION_BEAR_TREND"),
            )
        } else if adx > 40.0 && consecutive_deceleration {
            // Hard hook exit on 2-bar consecutive deceleration of extreme trend.
            (
                sign * 0.10,
                if sign > 0.0 {
                    "BULL_TREND_EXHAUSTION_HOOK".into()
                } else {
                    "BEAR_TREND_EXHAUSTION_HOOK".into()
                },
            )
        } else if adx <= 25.0 {
            (
                sign * 0.30,
                pick(sign, "EMERGING_BULL_TREND", "EMERGING_BEAR_TREND"),
            )
        } else if adx <= 40.0 {
            // AUDIT-AIU-021: continuous ramp 0.30 → 0.80 across [25, 40].
            // The previous `0.50 + (…)×0.30` started at 0.50, producing a
            // 0.20 jump at ADX = 25 that contradicted the doc's continuity
            // claim (04-02-05 §6).
            let mag = 0.30 + ((adx - 25.0) / 15.0) * 0.50;
            (
                sign * mag,
                pick(sign, "STRONG_BULL_TREND", "STRONG_BEAR_TREND"),
            )
        } else {
            // AUDIT-AIU-021: continuous ramp 0.80 → 1.00 across (40, 60].
            // The previous `0.90 + (…)×0.10` started at 0.90, producing a
            // 0.10 jump at ADX = 40.
            let mag = 0.80 + ((adx - 40.0) / 20.0).min(1.0) * 0.20;
            (
                sign * mag,
                pick(sign, "CLIMACTIC_BULL_TREND", "CLIMACTIC_BEAR_TREND"),
            )
        };

        let mut values = HashMap::new();
        values.insert("adx".to_string(), adx);
        values.insert("plus_di".to_string(), plus_di);
        values.insert("minus_di".to_string(), minus_di);
        values.insert("adx_slope".to_string(), adx_slope);
        NormalizedIndicatorValue::with_values(adx, clamp_unit(norm), label, values)
    }

    /// BBWP: volatility width percentile — non-directional context gate.
    ///
    /// Per the canonical contract in
    /// `docs/engines/market-monitoring-engine/indicators/04-02-27-bbwp.md` §6,
    /// BBWP is a non-directional gate: `normalized` is contractually **0.0
    /// always**; the bias-signed output that previously leaked through
    /// would sign-flip bearish breakouts into bullish signals when used
    /// as a multiplier (the same anti-pattern retired for RVOL). The
    /// volatility context reaches the rest of the system through the
    /// mechanical `readiness` flag, the `values.bbwp_gate` coefficient and the
    /// `risk.volatility_risk` dimension.
    pub fn normalize_bbwp(bbwp: f64, _bias: i8) -> NormalizedIndicatorValue {
        // AUDIT-AIU-023: confidence band edges aligned to the doc
        // (04-02-27 §6): <10 → 0.85, [10,25) → 0.60, [25,75] → 0.50,
        // (75,90] → 0.60, >90 → 0.85. The previous code used [10,30) and
        // [30,70] / (70,90], so BBWP ∈ [25,30) and (70,75] scored the wrong
        // confidence.
        let (label, base_confidence) = if bbwp < 10.0 {
            ("MAX_VOLATILITY_COMPRESSION", 0.85)
        } else if bbwp > 90.0 {
            ("VOLATILITY_EXHAUSTION_REVERSION_WARNING", 0.85)
        } else if bbwp <= 25.0 {
            ("LOW_VOLATILITY_BULL_CYCLE", 0.60)
        } else if bbwp <= 75.0 {
            ("NORMAL_VOLATILITY_BULL_CYCLE", 0.50)
        } else {
            ("HIGH_VOLATILITY_BULL_EXPANSION", 0.60)
        };
        let mut values = HashMap::new();
        values.insert("bbwp_gate".to_string(), base_confidence);
        NormalizedIndicatorValue::with_values(bbwp, 0.0, label, values)
    }

    /// RVOL: volume validation gate for structural breakouts.
    ///
    /// Per the v2.1 contract in `docs/engines/market-monitoring-engine/indicators/04-02-19-rvol.md` §3,
    /// RVOL is a **non-directional gate** — its `normalized` field is always `0.0` (consistent with the
    /// BBWP convention). The signed 4-band values (−0.5, 0.2, 0.8, −1.0) are exposed as a scalar
    /// gate coefficient via `IndicatorEvaluation.values.rvol_band` and consumed by the gate logic,
    /// never added to the directional confluence sum. A previous version of this function emitted
    /// the signed band values directly into `normalized`, which double-counted RVOL as both a gate
    /// and a directional voter.
    /// AUDIT-AIU-071: threshold parameters (`institutional`/`climax`) are
    /// now threaded from config — the registry declares them in
    /// `config_params` but the code hardcoded 1.5 / 3.0.
    pub fn normalize_rvol(
        rvol: f64,
        institutional_threshold: f64,
        climax_threshold: f64,
    ) -> NormalizedIndicatorValue {
        let band = if rvol < 1.0 {
            -0.5
        } else if rvol < institutional_threshold {
            0.2
        } else if rvol < climax_threshold {
            0.8
        } else {
            -1.0
        };
        let label = if rvol < 1.0 {
            // SIG-13 (v2.1 canonical): "LOW_PARTICIPATION_VOLUME" is the
            // string the downstream consumers (regex / policy string matching,
            // property tests, GUI panels) read. The previous
            // "CONSOLIDATION_VOLUME" label name was rolled forward to align
            // with the RVOL spec ([04-02-19-rvol.md §3]) and the volume
            // normalization ([04-02-18-volume.md §Normalization]).
            "LOW_PARTICIPATION_VOLUME"
        } else if rvol < institutional_threshold {
            "NORMAL_PARTICIPATION_VOLUME"
        } else if rvol < climax_threshold {
            "INSTITUTIONAL_BREAKOUT_VOLUME"
        } else {
            "EXHAUSTION_CLIMAX_VOLUME"
        };
        let mut values = HashMap::new();
        values.insert("rvol_band".to_string(), band);
        NormalizedIndicatorValue::with_values(rvol, 0.0, label, values)
    }

    /// EMA stacking & price location across the ribbon.
    pub fn normalize_ema_stack(ctx: &NormalizationContext) -> NormalizedIndicatorValue {
        let state = ctx
            .ema_stack_state
            .as_deref()
            .unwrap_or("tangled")
            .to_lowercase();
        let retest = matches!((ctx.ema_medium, ctx.price), (Some(m), p)
            if p > 0.0 && (p - m).abs() / p < 0.005);
        let (norm, label) = match state.as_str() {
            "bullish" => {
                if retest {
                    (0.8, "DYNAMIC_BULLISH_SUPPORT_RETEST")
                } else {
                    (1.0, "ESTABLISHED_BULLISH_STACK")
                }
            }
            "bearish" => {
                if retest {
                    (-0.8, "DYNAMIC_BEARISH_RESISTANCE_RETEST")
                } else {
                    (-1.0, "ESTABLISHED_BEARISH_STACK")
                }
            }
            _ => (0.0, "CONSOLIDATED_TANGLED_STACK"),
        };
        // PRI-10 (v6.10.7): raw_value = fast EMA (registry `value_source:
        // "sub:fast"`). The previous price-anchored raw (close) made the
        // Metrics Raw cell show the close and fed the raw-fallback bug in
        // the chart history layer. Fall back to price only when the fast
        // EMA is not yet available (cold edge).
        let raw = ctx.ema_fast.unwrap_or(ctx.price);
        NormalizedIndicatorValue::scalar(raw, norm, label)
    }

    /// VWAP fair-value baseline (premium/discount reversion zones).
    ///
    /// Mean-reversion interpretation: a price stretched **above** VWAP
    /// (`EXTREME_PREMIUM_REVERSION_ZONE` / `BEARISH_PREMIUM_PULLBACK`) is
    /// overvalued and is expected to revert downward — sign **negative**.
    /// A price stretched **below** VWAP (`EXTREME_DISCOUNT_REVERSION_ZONE`
    /// / `BULLISH_DISCOUNT_PULLBACK`) is undervalued and is expected to
    /// revert upward — sign **positive**. This matches the canonical
    /// labels in `docs/engines/market-monitoring-engine/indicators/04-02-06-vwap.md`
    /// §Normalization. A previous version of this function assigned the
    /// labels to the *opposite* price zones, contradicting both the per-signal
    /// table and the §3.1 narrative in that file.
    pub fn normalize_vwap(price: f64, vwap: f64) -> NormalizedIndicatorValue {
        if vwap <= 0.0 {
            return NormalizedIndicatorValue::scalar(price, 0.0, "INTRA_DAY_VALUE_EQUILIBRIUM");
        }
        let ratio = price / vwap;
        let (norm, label) = if ratio > 1.01 {
            (-0.8, "EXTREME_PREMIUM_REVERSION_ZONE")
        } else if ratio > 1.001 {
            (-0.5, "BEARISH_PREMIUM_PULLBACK")
        } else if ratio < 0.99 {
            (0.8, "EXTREME_DISCOUNT_REVERSION_ZONE")
        } else if ratio < 0.999 {
            (0.5, "BULLISH_DISCOUNT_PULLBACK")
        } else {
            (0.0, "INTRA_DAY_VALUE_EQUILIBRIUM")
        };
        let mut values = HashMap::new();
        values.insert("vwap".to_string(), vwap);
        values.insert("price".to_string(), price);
        NormalizedIndicatorValue::with_values(vwap, norm, label, values)
    }

    /// Fibonacci golden pocket & extension targets relative to trend bias.
    pub fn normalize_fibonacci(
        price: f64,
        gp_low: Option<f64>,
        gp_high: Option<f64>,
        ext_1618: Option<f64>,
        ext_2618: Option<f64>,
        bias: i8,
    ) -> NormalizedIndicatorValue {
        if let (Some(lo), Some(hi)) = (gp_low, gp_high) {
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            if price >= lo && price <= hi {
                return match bias {
                    b if b > 0 => NormalizedIndicatorValue::scalar(
                        price,
                        1.0,
                        "BULLISH_GOLDEN_POCKET_REBOUND",
                    ),
                    b if b < 0 => NormalizedIndicatorValue::scalar(
                        price,
                        -1.0,
                        "BEARISH_GOLDEN_POCKET_REJECTION",
                    ),
                    _ => NormalizedIndicatorValue::scalar(price, 0.0, "GOLDEN_POCKET_NEUTRAL"),
                };
            }
        }
        if bias > 0 {
            if let Some(e) = ext_2618 {
                if price >= e {
                    return NormalizedIndicatorValue::scalar(
                        price,
                        0.1,
                        "CLIMACTIC_EXTENSION_TARGET_REACHED",
                    );
                }
            }
            if let Some(e) = ext_1618 {
                if price >= e {
                    return NormalizedIndicatorValue::scalar(
                        price,
                        0.2,
                        "PRIMARY_EXTENSION_TARGET_REACHED",
                    );
                }
            }
        } else if bias < 0 {
            if let Some(e) = ext_2618 {
                if price <= e {
                    return NormalizedIndicatorValue::scalar(
                        price,
                        -0.1,
                        "CLIMACTIC_EXTENSION_TARGET_REACHED",
                    );
                }
            }
            if let Some(e) = ext_1618 {
                if price <= e {
                    return NormalizedIndicatorValue::scalar(
                        price,
                        -0.2,
                        "PRIMARY_EXTENSION_TARGET_REACHED",
                    );
                }
            }
        }
        NormalizedIndicatorValue::scalar(price, 0.0, "FIBONACCI_NEUTRAL")
    }

    /// Chart pattern breakout / accumulation gated by RVOL confirmation.
    pub fn normalize_patterns(
        is_bullish: bool,
        is_bearish: bool,
        confidence: f64,
        rvol: f64,
        upper_slope: Option<f64>,
        upper_intercept: Option<f64>,
        lower_slope: Option<f64>,
        lower_intercept: Option<f64>,
    ) -> NormalizedIndicatorValue {
        let (norm, label) = if is_bullish {
            if rvol >= 1.5 {
                (1.0, "BULLISH_PATTERN_BREAKOUT")
            } else {
                (0.5, "BULLISH_PATTERN_ACCUMULATION")
            }
        } else if is_bearish {
            if rvol >= 1.5 {
                (-1.0, "BEARISH_PATTERN_BREAKOUT")
            } else {
                (-0.5, "BEARISH_PATTERN_DISTRIBUTION")
            }
        } else {
            (0.0, "NO_PATTERN")
        };
        let mut values = HashMap::new();
        if let Some(s) = upper_slope {
            values.insert("upper_slope".to_string(), s);
        }
        if let Some(i) = upper_intercept {
            values.insert("upper_intercept".to_string(), i);
        }
        if let Some(s) = lower_slope {
            values.insert("lower_slope".to_string(), s);
        }
        if let Some(i) = lower_intercept {
            values.insert("lower_intercept".to_string(), i);
        }
        NormalizedIndicatorValue::with_values(confidence, norm, label, values)
    }

    /// Support & Resistance with proximity + role-reversal breakout detection.
    pub fn normalize_sr(
        price: f64,
        support_levels: &[f64],
        resistance_levels: &[f64],
        rvol: f64,
    ) -> NormalizedIndicatorValue {
        let near = |lvl: f64| lvl > 0.0 && (price - lvl).abs() / lvl <= 0.005;
        if support_levels.iter().copied().any(near) {
            return NormalizedIndicatorValue::scalar(price, 1.0, "SUPPORT_DEMAND_ZONE");
        }
        if resistance_levels.iter().copied().any(near) {
            return NormalizedIndicatorValue::scalar(price, -1.0, "RESISTANCE_SUPPLY_ZONE");
        }
        if rvol >= 1.5 {
            let highest_res = resistance_levels
                .iter()
                .copied()
                .filter(|r| *r > 0.0)
                .fold(None, |acc: Option<f64>, r| {
                    Some(acc.map_or(r, |a| a.max(r)))
                });
            if let Some(r) = highest_res {
                if price > r {
                    return NormalizedIndicatorValue::scalar(
                        price,
                        0.8,
                        "RESISTANCE_FLIP_CONFIRMED",
                    );
                }
            }
            let lowest_sup = support_levels
                .iter()
                .copied()
                .filter(|s| *s > 0.0)
                .fold(None, |acc: Option<f64>, s| {
                    Some(acc.map_or(s, |a| a.min(s)))
                });
            if let Some(s) = lowest_sup {
                if price < s {
                    return NormalizedIndicatorValue::scalar(price, -0.8, "SUPPORT_FLIP_CONFIRMED");
                }
            }
        }
        NormalizedIndicatorValue::scalar(price, 0.0, "STRUCTURE_NEUTRAL")
    }

    /// Session Pivot Points: directional bias from proximity to the nearest
    /// active level. Near a support (P/S1/S2/S3 below or at price) → bullish;
    /// near a resistance (R1/R2/R3 above) → bearish; between levels → neutral,
    /// signed by which side of the central pivot price sits on.
    ///
    /// All seven levels are stored in the `values` sub-map so the frontend can
    /// render them as horizontal price lines and the AI can reference the active
    /// level. `proximity_pct` is the fraction (e.g. 0.0015 for 0.15%) within
    /// which price is considered "at" a level.
    #[allow(clippy::too_many_arguments)]
    pub fn normalize_pivot_points(
        price: f64,
        pivot: f64,
        r1: f64,
        r2: f64,
        r3: f64,
        s1: f64,
        s2: f64,
        s3: f64,
        proximity_pct: f64,
    ) -> NormalizedIndicatorValue {
        use std::collections::HashMap;
        let mut values = HashMap::new();
        values.insert("pivot".to_string(), pivot);
        values.insert("r1".to_string(), r1);
        values.insert("r2".to_string(), r2);
        values.insert("r3".to_string(), r3);
        values.insert("s1".to_string(), s1);
        values.insert("s2".to_string(), s2);
        values.insert("s3".to_string(), s3);

        if price <= 0.0 || pivot <= 0.0 {
            return NormalizedIndicatorValue::with_values(price, 0.0, "PIVOT_UNAVAILABLE", values);
        }

        let near = |lvl: f64| lvl > 0.0 && (price - lvl).abs() / price <= proximity_pct;

        // Proximity tests, strongest structural pull first.
        // Support touches → bullish (demand); resistance touches → bearish.
        let (norm, label): (f64, &str) = if near(s3) {
            (1.0, "PIVOT_S3_SUPPORT_TEST")
        } else if near(s2) {
            (0.9, "PIVOT_S2_SUPPORT_TEST")
        } else if near(s1) {
            (0.7, "PIVOT_S1_SUPPORT_TEST")
        } else if near(r3) {
            (-1.0, "PIVOT_R3_RESISTANCE_TEST")
        } else if near(r2) {
            (-0.9, "PIVOT_R2_RESISTANCE_TEST")
        } else if near(r1) {
            (-0.7, "PIVOT_R1_RESISTANCE_TEST")
        } else if near(pivot) {
            (0.0, "PIVOT_CENTRAL_TEST")
        } else {
            // Between levels: mild directional bias from position vs central pivot,
            // scaled by distance across the R1..S1 band.
            let band = (r1 - s1).abs().max(f64::EPSILON);
            let bias = ((price - pivot) / band).clamp(-1.0, 1.0);
            // Above pivot leans bearish-into-resistance (mean reversion framing);
            // below pivot leans bullish-into-support. Keep it mild (×0.4).
            let n = (-bias * 0.4).clamp(-1.0, 1.0);
            let lbl = if price > pivot {
                "PIVOT_ABOVE_CENTRAL"
            } else if price < pivot {
                "PIVOT_BELOW_CENTRAL"
            } else {
                "PIVOT_AT_CENTRAL"
            };
            return NormalizedIndicatorValue::with_values(price, n, lbl, values);
        };

        NormalizedIndicatorValue::with_values(price, norm, label, values)
    }

    /// Candlestick pattern normalization (Stage 2 output).
    ///
    /// Stage 1 (geometry) produced `direction`/`quality`/`status` upstream. This
    /// applies the pre-computed context multiplier (trend/S-R/volume/volatility/
    /// regime alignment, in `[0, ~1.5]`) to derive a final confidence, gates it
    /// by `min_confidence`, and emits a `PatternForming` signal whose status is
    /// `Potential` (formed) or `Confirmed`. Invalidated / below-threshold
    /// readings collapse to a neutral entry.
    ///
    /// `status_code`: 1 = Formed, 2 = Confirmed, 3 = Invalidated.
    #[allow(clippy::too_many_arguments)]
    pub fn normalize_candlestick(
        pattern_name: &str,
        category: &str,
        direction: i8,
        quality: f64,
        status_code: u8,
        context_mult: f64,
        min_confidence: f64,
    ) -> NormalizedIndicatorValue {
        let confidence = (quality * context_mult).clamp(0.0, 1.0);

        let mut values = HashMap::new();
        values.insert("direction".to_string(), direction as f64);
        values.insert("quality".to_string(), quality);
        values.insert("confidence".to_string(), confidence);
        values.insert("status".to_string(), status_code as f64);

        // Invalidated or sub-threshold → neutral (no directional contribution).
        if status_code == 3 || direction == 0 || confidence < min_confidence {
            let label = if status_code == 3 {
                format!("{}_INVALIDATED", pattern_name)
            } else {
                format!("{}_UNCONFIRMED", pattern_name)
            };
            return NormalizedIndicatorValue::with_values(0.0, 0.0, label, values);
        }

        // Confirmed readings score at full confidence; merely-formed at 60%.
        let scale = if status_code == 2 { 1.0 } else { 0.6 };
        let norm = clamp_unit(direction as f64 * confidence * scale);

        let (sig_status, status_word) = if status_code == 2 {
            (SignalStatus::Confirmed, "CONFIRMED")
        } else {
            (SignalStatus::Potential, "FORMED")
        };
        let label = format!("{}_{}", pattern_name, status_word);
        let dir = if direction > 0 {
            SignalDirection::Bullish
        } else {
            SignalDirection::Bearish
        };

        let mut entry =
            NormalizedIndicatorValue::with_values(confidence, norm, label.clone(), values);
        let _ = category;
        entry.signals.push(
            IndicatorSignal::new(SignalKind::PatternForming, dir, sig_status, &label)
                .with_strength(confidence),
        );
        entry
    }

    /// Ichimoku Cloud: complete trend system normalized on price-vs-cloud
    /// position, with conviction from Tenkan/Kijun alignment, current cloud
    /// colour, and future cloud colour. All five lines plus cloud metadata are
    /// stored in `values` for chart rendering and AI context.
    #[allow(clippy::too_many_arguments)]
    pub fn normalize_ichimoku(
        price: f64,
        tenkan: f64,
        kijun: f64,
        senkou_a_future: f64,
        senkou_b_future: f64,
        senkou_a_current: f64,
        senkou_b_current: f64,
        chikou: f64,
    ) -> NormalizedIndicatorValue {
        let cloud_top = senkou_a_current.max(senkou_b_current);
        let cloud_bottom = senkou_a_current.min(senkou_b_current);
        let cloud_thickness = cloud_top - cloud_bottom;

        // Directional position vs the applicable cloud.
        let pos = if price > cloud_top {
            1.0
        } else if price < cloud_bottom {
            -1.0
        } else {
            0.0
        };

        // Conviction factors (each -1/0/+1).
        let tk = (tenkan - kijun).signum()
            * if (tenkan - kijun).abs() > f64::EPSILON {
                1.0
            } else {
                0.0
            };
        let cur_cloud = (senkou_a_current - senkou_b_current).signum()
            * if (senkou_a_current - senkou_b_current).abs() > f64::EPSILON {
                1.0
            } else {
                0.0
            };
        let fut_cloud = (senkou_a_future - senkou_b_future).signum()
            * if (senkou_a_future - senkou_b_future).abs() > f64::EPSILON {
                1.0
            } else {
                0.0
            };
        let future_bias = fut_cloud;

        let mut values = HashMap::new();
        values.insert("tenkan".to_string(), tenkan);
        values.insert("kijun".to_string(), kijun);
        values.insert("senkou_a".to_string(), senkou_a_future);
        values.insert("senkou_b".to_string(), senkou_b_future);
        values.insert("chikou".to_string(), chikou);
        values.insert("senkou_a_current".to_string(), senkou_a_current);
        values.insert("senkou_b_current".to_string(), senkou_b_current);
        values.insert("cloud_thickness".to_string(), cloud_thickness);
        values.insert("future_bias".to_string(), future_bias);

        let norm;
        let label: &str;
        if pos > 0.0 {
            // Bullish: agreement of TK + current + future cloud strengthens.
            let agree = ((tk.max(0.0)) + (cur_cloud.max(0.0)) + (fut_cloud.max(0.0))) / 3.0;
            norm = clamp_unit(0.6 + 0.4 * agree);
            label = if agree >= 0.99 {
                "STRONG_BULLISH_ABOVE_CLOUD"
            } else {
                "BULLISH_ABOVE_CLOUD"
            };
        } else if pos < 0.0 {
            let agree =
                (((-tk).max(0.0)) + ((-cur_cloud).max(0.0)) + ((-fut_cloud).max(0.0))) / 3.0;
            norm = clamp_unit(-(0.6 + 0.4 * agree));
            label = if agree >= 0.99 {
                "STRONG_BEARISH_BELOW_CLOUD"
            } else {
                "BEARISH_BELOW_CLOUD"
            };
        } else {
            // Inside the cloud: no trend conviction; slight lean from TK cross.
            norm = clamp_unit(tk * 0.2);
            label = "PRICE_INSIDE_CLOUD";
        }

        NormalizedIndicatorValue::with_values(price, norm, label, values)
    }

    /// Volume Profile: auction-state analysis from OHLCV volume distribution.
    /// Price above VAH = bullish breakout; below VAL = bearish; inside = equilibrium.
    pub fn normalize_volume_profile(
        price: f64,
        poc: f64,
        vah: f64,
        val: f64,
        total_volume: f64,
    ) -> NormalizedIndicatorValue {
        let mut values = HashMap::new();
        values.insert("poc".to_string(), poc);
        values.insert("vah".to_string(), vah);
        values.insert("val".to_string(), val);
        values.insert("total_volume".to_string(), total_volume);
        let (norm, label) = if price > vah {
            (
                clamp_unit(0.7 + 0.3 * ((price - vah) / (vah - val).max(f64::EPSILON)).min(1.0)),
                "VP_BREAKOUT_ABOVE_VAH",
            )
        } else if price < val {
            (
                clamp_unit(-0.7 - 0.3 * ((val - price) / (vah - val).max(f64::EPSILON)).min(1.0)),
                "VP_BREAKOUT_BELOW_VAL",
            )
        } else if (price - poc).abs() / poc.max(f64::EPSILON) <= 0.003 {
            let d = if price > poc { -0.3 } else { 0.3 };
            (
                d,
                if price > poc {
                    "VP_POC_RESISTANCE_TEST"
                } else {
                    "VP_POC_SUPPORT_TEST"
                },
            )
        } else {
            let pos = (price - val) / (vah - val).max(f64::EPSILON);
            let n = (pos - 0.5) * 0.4;
            (clamp_unit(n), "VP_VALUE_ACCEPTANCE")
        };
        NormalizedIndicatorValue::with_values(price, norm, label, values)
    }

    /// Parabolic SAR: trend-following trailing-stop overlay. Price above SAR
    /// → bullish; below → bearish. Distance-scaled conviction (same pattern as
    /// Supertrend).
    pub fn normalize_psar(price: f64, sar: f64, direction: i8) -> NormalizedIndicatorValue {
        let dist = if sar.abs() > f64::EPSILON {
            ((price - sar) / sar).abs()
        } else {
            0.0
        };
        let mag = 0.5 + 0.5 * (dist * 15.0).tanh();
        let norm = clamp_unit(direction as f64 * mag);
        let label = if direction > 0 {
            "PSAR_UPTREND"
        } else {
            "PSAR_DOWNTREND"
        };
        let mut values = HashMap::new();
        values.insert("sar".to_string(), sar);
        values.insert("direction".to_string(), direction as f64);
        NormalizedIndicatorValue::with_values(sar, norm, label.to_string(), values)
    }

    // ── SMC Structure: BOS (Break of Structure) + CHoCH (Change of Character) ──
    pub fn normalize_smc_structure(
        structure_bullish: bool,
        structure_bearish: bool,
        bos_bullish: bool,
        bos_bearish: bool,
        choch_bullish: bool,
        choch_bearish: bool,
    ) -> NormalizedIndicatorValue {
        let mut norm = 0.0f64;
        // AUDIT-AIU-113 (defensive): a contradictory double-BOS (both flags)
        // should never arrive — the calculator resolves it by dominant
        // displacement. If one ever does, drop BOTH bos contributions so the
        // entry can never carry a bullish label with a bearish score.
        let (bos_bullish, bos_bearish) = if bos_bullish && bos_bearish {
            (false, false)
        } else {
            (bos_bullish, bos_bearish)
        };
        if structure_bullish {
            norm += 0.7;
        } else if structure_bearish {
            norm -= 0.7;
        }
        if bos_bullish {
            norm += 0.3;
        }
        if bos_bearish {
            norm -= 0.3;
        }
        if choch_bullish {
            norm += 0.4;
        }
        if choch_bearish {
            norm -= 0.4;
        }
        let norm = clamp_unit(norm);
        let label = if choch_bullish {
            "SMC_STRUCTURE_BULLISH_CHOCH"
        } else if choch_bearish {
            "SMC_STRUCTURE_BEARISH_CHOCH"
        } else if bos_bullish {
            "SMC_STRUCTURE_BULLISH_BOS"
        } else if bos_bearish {
            "SMC_STRUCTURE_BEARISH_BOS"
        } else if structure_bullish {
            "SMC_STRUCTURE_BULLISH"
        } else if structure_bearish {
            "SMC_STRUCTURE_BEARISH"
        } else {
            "SMC_STRUCTURE_NEUTRAL"
        };
        let mut values = HashMap::new();
        values.insert(
            "structure".to_string(),
            if structure_bullish {
                1.0
            } else if structure_bearish {
                -1.0
            } else {
                0.0
            },
        );
        values.insert(
            "bos_bullish".to_string(),
            if bos_bullish { 1.0 } else { 0.0 },
        );
        values.insert(
            "bos_bearish".to_string(),
            if bos_bearish { 1.0 } else { 0.0 },
        );
        values.insert(
            "choch_bullish".to_string(),
            if choch_bullish { 1.0 } else { 0.0 },
        );
        values.insert(
            "choch_bearish".to_string(),
            if choch_bearish { 1.0 } else { 0.0 },
        );
        NormalizedIndicatorValue::with_values(0.0, norm, label, values)
    }

    // ── SMC Liquidity: buy-side and sell-side sweeps ──
    pub fn normalize_smc_liquidity(
        liq_sweep_buy: bool,
        liq_sweep_sell: bool,
    ) -> NormalizedIndicatorValue {
        let mut norm = 0.0f64;
        let label: &str;
        if liq_sweep_buy && liq_sweep_sell {
            label = "SMC_LIQUIDITY_BOTH_SWEEPS";
        } else if liq_sweep_buy {
            norm = 0.5;
            label = "SMC_LIQUIDITY_BUY_SWEEP";
        } else if liq_sweep_sell {
            norm = -0.5;
            label = "SMC_LIQUIDITY_SELL_SWEEP";
        } else {
            label = "SMC_LIQUIDITY_NONE";
        }
        let mut values = HashMap::new();
        values.insert(
            "sweep_buy".to_string(),
            if liq_sweep_buy { 1.0 } else { 0.0 },
        );
        values.insert(
            "sweep_sell".to_string(),
            if liq_sweep_sell { 1.0 } else { 0.0 },
        );
        NormalizedIndicatorValue::with_values(0.0, norm, label, values)
    }

    // ── SMC Fair Value Gap ──
    pub fn normalize_smc_fvg(
        fvg_top: Option<f64>,
        fvg_bottom: Option<f64>,
        fvg_bullish: bool,
    ) -> NormalizedIndicatorValue {
        let (norm, label): (f64, &str) = if fvg_top.is_some() && fvg_bottom.is_some() {
            if fvg_bullish {
                (0.45, "SMC_FVG_BULLISH_OPEN")
            } else {
                (-0.45, "SMC_FVG_BEARISH_OPEN")
            }
        } else {
            (0.0, "SMC_FVG_NONE")
        };
        let mut values = HashMap::new();
        if let Some(v) = fvg_top {
            values.insert("fvg_top".to_string(), v);
        }
        if let Some(v) = fvg_bottom {
            values.insert("fvg_bottom".to_string(), v);
        }
        values.insert(
            "fvg_bullish".to_string(),
            if fvg_bullish { 1.0 } else { 0.0 },
        );
        NormalizedIndicatorValue::with_values(0.0, norm, label, values)
    }

    // ── SMC Order Blocks ──
    pub fn normalize_smc_order_blocks(
        price: f64,
        ob_bullish_high: Option<f64>,
        ob_bullish_low: Option<f64>,
        ob_bearish_high: Option<f64>,
        ob_bearish_low: Option<f64>,
    ) -> NormalizedIndicatorValue {
        let mut norm = 0.0f64;
        let mut label = "SMC_OB_NONE";
        if let (Some(h), Some(l)) = (ob_bullish_high, ob_bullish_low) {
            if h > 0.0 && l > 0.0 {
                let mid = (h + l) / 2.0;
                if ((price - mid) / mid.max(f64::EPSILON)).abs() < 0.005 {
                    norm = 0.5;
                    label = "SMC_OB_BULLISH_TEST";
                } else {
                    label = "SMC_OB_BULLISH_ACTIVE";
                }
            }
        }
        if let (Some(h), Some(l)) = (ob_bearish_high, ob_bearish_low) {
            if h > 0.0 && l > 0.0 {
                let mid = (h + l) / 2.0;
                if ((price - mid) / mid.max(f64::EPSILON)).abs() < 0.005 {
                    norm = -0.5;
                    label = "SMC_OB_BEARISH_TEST";
                } else {
                    label = "SMC_OB_BEARISH_ACTIVE";
                }
            }
        }
        let mut values = HashMap::new();
        if let Some(v) = ob_bullish_high {
            values.insert("ob_bullish_high".to_string(), v);
        }
        if let Some(v) = ob_bullish_low {
            values.insert("ob_bullish_low".to_string(), v);
        }
        if let Some(v) = ob_bearish_high {
            values.insert("ob_bearish_high".to_string(), v);
        }
        if let Some(v) = ob_bearish_low {
            values.insert("ob_bearish_low".to_string(), v);
        }
        NormalizedIndicatorValue::with_values(0.0, norm, label, values)
    }
}
