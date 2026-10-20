//! Consolidation of the normalization mappers into a unified map.

use super::super::atr::VolatilityRegime;
use super::super::candlestick::{CandlestickResult, CandlestickStatus};
use super::super::squeeze::MomentumDirection;
use super::{
    DivergenceState, IndicatorSignal, NormalizationContext, NormalizationEngine,
    NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus,
};
use core_domain::axes::Readiness;
use std::collections::HashMap;

/// Raw indicator inputs bundle consumed by [`NormalizationEngine::normalize_all`].
///
/// Every field is optional so partially-warmed pipelines can still produce a
/// consistent map (missing indicators are simply omitted from the output).
#[derive(Debug, Clone, Default)]
pub struct IndicatorInputs {
    // RSI
    pub rsi: Option<f64>,
    pub rsi_divergence: DivergenceState,
    // Stochastic Oscillator (%K slowed, %D signal)
    pub stoch_k: Option<f64>,
    pub stoch_d: Option<f64>,
    // Chande Momentum Oscillator
    pub chandemo: Option<f64>,
    // Supertrend
    pub supertrend_line: Option<f64>,
    pub supertrend_dir: Option<i8>,
    /// True when Supertrend direction flipped this bar.
    pub supertrend_flipped: bool,
    // Keltner Channels
    pub keltner_upper: Option<f64>,
    pub keltner_middle: Option<f64>,
    pub keltner_lower: Option<f64>,
    // Donchian Channels
    pub donchian_upper: Option<f64>,
    pub donchian_middle: Option<f64>,
    pub donchian_lower: Option<f64>,
    // On-Balance Volume
    pub obv: Option<f64>,
    pub obv_sma: Option<f64>,
    // Chaikin Money Flow
    pub cmf: Option<f64>,
    // Money Flow Index
    pub mfi: Option<f64>,
    // Historical Volatility
    pub hv: Option<f64>,
    // Market Regime
    pub aroon_up: Option<f64>,
    pub aroon_down: Option<f64>,
    pub choppiness: Option<f64>,
    pub linreg_slope: Option<f64>,
    pub zscore: Option<f64>,
    // Generalized divergence states (Phase 2) for the extra oscillators.
    pub stochastic_divergence: DivergenceState,
    pub chandemo_divergence: DivergenceState,
    pub mfi_divergence: DivergenceState,
    pub cmf_divergence: DivergenceState,
    pub obv_divergence: DivergenceState,
    pub squeeze_divergence: DivergenceState,
    // MACD
    pub macd_line: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub macd_histogram_peak: Option<f64>,
    /// Crossover: Some(1) bullish, Some(-1) bearish, None none.
    pub macd_crossover: Option<i8>,
    pub macd_divergence: DivergenceState,
    // Squeeze
    pub squeeze_on: Option<bool>,
    pub squeeze_release_trigger: bool,
    pub squeeze_momentum: Option<f64>,
    pub squeeze_direction: Option<MomentumDirection>,
    // ADX
    pub adx: Option<f64>,
    pub adx_plus_di: Option<f64>,
    pub adx_minus_di: Option<f64>,
    pub adx_slope: Option<f64>,
    /// DI crossover this bar: Some(1) +DI crossed above -DI (bullish),
    /// Some(-1) -DI crossed above +DI (bearish), None no cross.
    pub adx_di_crossover: Option<i8>,
    // BBWP
    pub bbwp: Option<f64>,
    // RVOL
    pub rvol: Option<f64>,
    // VWAP
    pub vwap: Option<f64>,
    // Anchored VWAP (weekly / monthly / swing)
    pub avwap_weekly: Option<f64>,
    pub avwap_monthly: Option<f64>,
    pub avwap_swing: Option<f64>,
    // Fibonacci
    pub fib_gp_low: Option<f64>,
    pub fib_gp_high: Option<f64>,
    pub fib_ext_1618: Option<f64>,
    pub fib_ext_2618: Option<f64>,
    // Patterns
    pub pattern_bullish: bool,
    pub pattern_bearish: bool,
    pub pattern_confidence: Option<f64>,
    pub pattern_upper_slope: Option<f64>,
    pub pattern_upper_intercept: Option<f64>,
    pub pattern_lower_slope: Option<f64>,
    pub pattern_lower_intercept: Option<f64>,
    // Supplemental raw-only series preserved for frontend charts (normalized 0.0)
    pub atr_14: Option<f64>,
    pub atr_slope: Option<f64>,
    /// Volatility regime classification (current ATR vs 5-period mean ATR). Drives
    /// `ATR_EXPANDING` / `ATR_CONTRACTING` signal emission in a scale-invariant way.
    pub atr_regime: Option<VolatilityRegime>,
    pub bb_upper: Option<f64>,
    pub bb_middle: Option<f64>,
    pub bb_lower: Option<f64>,
    // EMA ribbon values for fast/medium crossover detection.
    pub ema_fast: Option<f64>,
    pub ema_medium: Option<f64>,
    /// Full ribbon (slow/long) for the StackChange transition detector
    /// (AUDIT-AIU-030).
    pub ema_slow: Option<f64>,
    pub ema_long: Option<f64>,
    // Session Pivot Points: seven levels + proximity threshold (fraction).
    pub pivot: Option<f64>,
    pub pivot_r1: Option<f64>,
    pub pivot_r2: Option<f64>,
    pub pivot_r3: Option<f64>,
    pub pivot_s1: Option<f64>,
    pub pivot_s2: Option<f64>,
    pub pivot_s3: Option<f64>,
    pub pivot_proximity_pct: f64,
    // Candlestick pattern reading (Stage 1 geometry + Stage 3 confirmation).
    pub candlestick: Option<CandlestickResult>,
    pub candlestick_min_confidence: f64,
    // Ichimoku Cloud: 5 lines + current-applicable cloud spans.
    pub ichimoku_tenkan: Option<f64>,
    pub ichimoku_kijun: Option<f64>,
    pub ichimoku_senkou_a: Option<f64>,
    pub ichimoku_senkou_b: Option<f64>,
    pub ichimoku_chikou: Option<f64>,
    pub ichimoku_senkou_a_current: Option<f64>,
    pub ichimoku_senkou_b_current: Option<f64>,
    // CCI (Commodity Channel Index).
    pub cci: Option<f64>,
    // Parabolic SAR.
    pub psar_sar: Option<f64>,
    pub psar_direction: Option<i8>,
    pub psar_flipped: bool,
    // Williams %R, AO, Force Index, Hull MA, StdDev Channel.
    pub williams_r: Option<f64>,
    pub awesome_oscillator: Option<f64>,
    pub ao_rising: bool,
    pub force_index: Option<f64>,
    /// Rolling mean of |FI| — scale-relative extreme baseline (AUDIT-AIU-043).
    pub force_index_mean_abs: Option<f64>,
    pub hull_ma: Option<f64>,
    pub stddev_upper: Option<f64>,
    pub stddev_center: Option<f64>,
    pub stddev_lower: Option<f64>,
    // Volume Profile
    pub volprofile_poc: Option<f64>,
    pub volprofile_vah: Option<f64>,
    pub volprofile_val: Option<f64>,
    pub volprofile_total_volume: f64,
    // Smart Money Concepts (SMC)
    pub smc_structure_bullish: bool,
    pub smc_structure_bearish: bool,
    pub smc_bos_bullish: bool,
    pub smc_bos_bearish: bool,
    pub smc_choch_bullish: bool,
    pub smc_choch_bearish: bool,
    pub smc_liq_sweep_buy: bool,
    pub smc_liq_sweep_sell: bool,
    pub smc_ob_bullish_high: Option<f64>,
    pub smc_ob_bullish_low: Option<f64>,
    pub smc_ob_bearish_high: Option<f64>,
    pub smc_ob_bearish_low: Option<f64>,
    pub smc_fvg_top: Option<f64>,
    pub smc_fvg_bottom: Option<f64>,
    pub smc_fvg_bullish: bool,
    pub smc_premium_discount: f64,
}

impl NormalizationEngine {
    /// Consolidate all available indicators into the unified normalized map.
    ///
    /// Keys: `rsi`, `stochastic`, `chandemo`, `macd`, `squeeze`, `adx`, `bbwp`,
    /// `rvol`, `ema_stack`, `vwap`, `fibonacci`, `patterns`,
    /// `support_resistance`.
    ///
    /// `shadow` controls the close-only indicator behavior:
    ///   - `false` (completed candle): every registered key receives either a
    ///     real value or a `WARMING` placeholder so the frontend can
    ///     distinguish warming from not-configured.
    ///   - `true` (live tick): close-only indicators (those with
    ///     `updates_on_shadow = false`) are skipped entirely — the
    ///     frontend per-key merge preserves the last completed-candle
    ///     value across shadow ticks, and the WARMING fill would otherwise
    ///     erase that history with a zero-valued placeholder.
    pub fn normalize_all(
        inputs: &IndicatorInputs,
        ctx: &NormalizationContext,
        shadow: bool,
    ) -> HashMap<String, NormalizedIndicatorValue> {
        let mut out: HashMap<String, NormalizedIndicatorValue> = HashMap::new();

        if let Some(rsi) = inputs.rsi {
            out.insert(
                "rsi".into(),
                Self::normalize_rsi(rsi, inputs.rsi_divergence),
            );
        }

        if let (Some(k), Some(d)) = (inputs.stoch_k, inputs.stoch_d) {
            out.insert("stochastic".into(), Self::normalize_stochastic(k, d));
        }

        if let Some(cmo) = inputs.chandemo {
            out.insert("chandemo".into(), Self::normalize_chandemo(cmo));
        }

        if let Some(wr) = inputs.williams_r {
            out.insert("williams_r".into(), Self::normalize_williams_r(wr));
        }

        if let Some(ao) = inputs.awesome_oscillator {
            // AUDIT-AIU-044: ATR-relative scaling for the normalization.
            let atr = inputs.atr_14.filter(|a| *a > 0.0);
            out.insert(
                "awesome_oscillator".into(),
                Self::normalize_awesome_oscillator(ao, inputs.ao_rising, atr),
            );
        }

        if let Some(fi) = inputs.force_index {
            // AUDIT-AIU-043: carry the scale-relative mean so the extreme
            // threshold detector can compare |fi| against its own baseline.
            let mut niv = Self::normalize_force_index(fi);
            if let Some(ma) = inputs.force_index_mean_abs {
                let mut vals = niv.values.clone().unwrap_or_default();
                vals.insert("mean_abs".to_string(), ma);
                niv.values = Some(vals);
            }
            out.insert("force_index".into(), niv);
        }

        if let Some(cci) = inputs.cci {
            out.insert("cci".into(), Self::normalize_cci(cci));
        }

        if let (Some(line), Some(dir)) = (inputs.supertrend_line, inputs.supertrend_dir) {
            out.insert(
                "supertrend".into(),
                Self::normalize_supertrend(ctx.price, line, dir),
            );
        }

        if let (Some(sar), Some(dir)) = (inputs.psar_sar, inputs.psar_direction) {
            out.insert("psar".into(), Self::normalize_psar(ctx.price, sar, dir));
        }

        if let (Some(u), Some(m), Some(l)) = (
            inputs.keltner_upper,
            inputs.keltner_middle,
            inputs.keltner_lower,
        ) {
            out.insert(
                "keltner".into(),
                Self::normalize_keltner(ctx.price, u, m, l),
            );
        }

        if let (Some(u), Some(m), Some(l)) = (
            inputs.donchian_upper,
            inputs.donchian_middle,
            inputs.donchian_lower,
        ) {
            out.insert(
                "donchian".into(),
                Self::normalize_donchian(ctx.price, u, m, l),
            );
        }

        if let (Some(obv), Some(sma)) = (inputs.obv, inputs.obv_sma) {
            out.insert("obv".into(), Self::normalize_obv(obv, sma));
        }

        if let Some(cmf) = inputs.cmf {
            out.insert("cmf".into(), Self::normalize_cmf(cmf));
        }

        if let Some(mfi) = inputs.mfi {
            out.insert("mfi".into(), Self::normalize_mfi(mfi));
        }

        if let Some(hv) = inputs.hv {
            out.insert("hv".into(), Self::normalize_hv(hv));
        }

        if let (Some(up), Some(down)) = (inputs.aroon_up, inputs.aroon_down) {
            out.insert("aroon".into(), Self::normalize_aroon(up, down));
        }

        if let Some(chop) = inputs.choppiness {
            out.insert("choppiness".into(), Self::normalize_choppiness(chop));
        }

        if let Some(slope) = inputs.linreg_slope {
            out.insert(
                "linreg_slope".into(),
                Self::normalize_linreg_slope(slope, ctx.price),
            );
        }

        if let Some(z) = inputs.zscore {
            out.insert("zscore".into(), Self::normalize_zscore(z));
        }

        if let (Some(line), Some(signal), Some(hist)) =
            (inputs.macd_line, inputs.macd_signal, inputs.macd_histogram)
        {
            out.insert(
                "macd".into(),
                Self::normalize_macd(
                    line,
                    signal,
                    hist,
                    inputs.macd_histogram_peak.unwrap_or(0.0),
                    inputs.macd_crossover,
                ),
            );
            // Structured Crossover signal: the MACD normalizer computes the state
            // label, but the crossover event itself is a structured boolean that
            // `derive_signals()` cannot detect from the label string alone.
            // AUDIT-AIU-022: rejected crossovers (FOMO bullish above the zero
            // line / PANIC bearish below it) must NOT emit a Crossover signal —
            // the zero-line filter suppresses the event entirely (04-02-17),
            // otherwise a filtered-out crossover leaks into TAE policy streams.
            if let Some(cross_dir) = inputs.macd_crossover {
                let rejected = (cross_dir > 0 && line >= 0.0) || (cross_dir < 0 && line <= 0.0);
                if !rejected {
                    if let Some(entry) = out.get_mut("macd") {
                        let (d, label) = if cross_dir > 0 {
                            (SignalDirection::Bullish, "BULLISH_CROSSOVER")
                        } else {
                            (SignalDirection::Bearish, "BEARISH_CROSSOVER")
                        };
                        entry.signals.push(IndicatorSignal::new(
                            SignalKind::Crossover,
                            d,
                            SignalStatus::Active,
                            label,
                        ));
                    }
                }
            }
        }

        if let (Some(on), Some(mom)) = (inputs.squeeze_on, inputs.squeeze_momentum) {
            out.insert(
                "squeeze".into(),
                Self::normalize_squeeze(
                    on,
                    inputs.squeeze_release_trigger,
                    mom,
                    inputs.squeeze_direction.unwrap_or(MomentumDirection::Flat),
                ),
            );
        }

        if let (Some(adx), Some(plus), Some(minus)) =
            (inputs.adx, inputs.adx_plus_di, inputs.adx_minus_di)
        {
            out.insert(
                "adx".into(),
                Self::normalize_adx(
                    adx,
                    plus,
                    minus,
                    inputs.adx_slope.unwrap_or(0.0),
                    ctx.adx_consecutive_deceleration,
                ),
            );
        }

        if let Some(bbwp) = inputs.bbwp {
            out.insert("bbwp".into(), Self::normalize_bbwp(bbwp, ctx.trend_bias));
        }

        if let Some(rvol) = inputs.rvol {
            out.insert(
                "rvol".into(),
                Self::normalize_rvol(
                    rvol,
                    ctx.rvol_institutional_threshold,
                    ctx.rvol_climax_threshold,
                ),
            );
        }

        if ctx.ema_stack_state.is_some() {
            out.insert("ema_stack".into(), Self::normalize_ema_stack(ctx));
        }

        if let Some(hma) = inputs.hull_ma {
            out.insert("hull_ma".into(), Self::normalize_hull_ma(hma));
        }

        if let Some(vwap) = inputs.vwap {
            out.insert("vwap".into(), Self::normalize_vwap(ctx.price, vwap));
        }

        if let Some(wk) = inputs.avwap_weekly {
            let mut values = HashMap::new();
            if let Some(w) = inputs.avwap_weekly {
                values.insert("weekly".into(), w);
            }
            if let Some(m) = inputs.avwap_monthly {
                values.insert("monthly".into(), m);
            }
            if let Some(s) = inputs.avwap_swing {
                values.insert("swing".into(), s);
            }
            let price = ctx.price;
            let active = [
                wk,
                inputs.avwap_monthly.unwrap_or(wk),
                inputs.avwap_swing.unwrap_or(wk),
            ]
            .iter()
            .map(|&a| (a, (price - a).abs()))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|a| a.0)
            .unwrap_or(wk);
            let ratio = price / active;
            let (norm, label) = if ratio > 1.01 {
                (-0.7, "AVWAP_PREMIUM_ZONE")
            } else if ratio < 0.99 {
                (0.7, "AVWAP_DISCOUNT_ZONE")
            } else if ratio > 1.001 {
                (-0.3, "AVWAP_ABOVE_ACTIVE")
            } else if ratio < 0.999 {
                (0.3, "AVWAP_BELOW_ACTIVE")
            } else {
                (0.0, "AVWAP_AT_ACTIVE")
            };
            out.insert(
                "anchored_vwap".into(),
                NormalizedIndicatorValue::with_values(active, norm, label, values),
            );
        }

        if inputs.fib_gp_low.is_some()
            || inputs.fib_gp_high.is_some()
            || inputs.fib_ext_1618.is_some()
            || inputs.fib_ext_2618.is_some()
        {
            out.insert(
                "fibonacci".into(),
                Self::normalize_fibonacci(
                    ctx.price,
                    inputs.fib_gp_low,
                    inputs.fib_gp_high,
                    inputs.fib_ext_1618,
                    inputs.fib_ext_2618,
                    ctx.trend_bias,
                ),
            );
        }

        // Insert the patterns entry whenever the calculator ran on this bar
        // (i.e. `pattern_confidence.is_some()`). Previously the entry was
        // gated on `pattern_bullish || pattern_bearish`, which only fired
        // on actual pattern detection — every other bar the entry was
        // missing, and the lifecycle builder fell into the
        // `Conditional → WaitingFeed` branch, surfacing the misleading
        // "WAITING FEED ⏳" label in the dashboard even though the
        // calculator had run and reported `ChartPattern::None`. Inserting
        // unconditionally on calculator-run lets the UI distinguish
        // "no pattern in current market structure" (state_label =
        // "NO_PATTERN", feed connected) from "feed not connected yet"
        // (entry absent, WaitingFeed).
        if inputs.pattern_confidence.is_some() {
            out.insert(
                "patterns".into(),
                Self::normalize_patterns(
                    inputs.pattern_bullish,
                    inputs.pattern_bearish,
                    inputs.pattern_confidence.unwrap_or(0.0),
                    ctx.rvol.unwrap_or(0.0),
                    inputs.pattern_upper_slope,
                    inputs.pattern_upper_intercept,
                    inputs.pattern_lower_slope,
                    inputs.pattern_lower_intercept,
                ),
            );
        }

        // v6.10 (Phase 5 / E5): `support_resistance` is always inserted into the
        // map on the completed path with a meaningful state_label, not just
        // when the SrRoleTracker has produced levels. This fixes BUG-4
        // ("support_resistance never inserted in live map"): previously the
        // entry was WARMING-placeholdered until the tracker accumulated
        // enough levels, leaving the dashboard unable to distinguish
        // "tracker warming up" from "no S/R levels detected in this
        // regime". With this fix the entry is always present and
        // normalized state_label reports whether levels are absent / scarce
        // / populated.
        if !shadow {
            out.insert(
                "support_resistance".into(),
                Self::normalize_sr(
                    ctx.price,
                    &ctx.support_levels,
                    &ctx.resistance_levels,
                    ctx.rvol.unwrap_or(0.0),
                ),
            );
        }

        // Session Pivot Points (levels published from the prior UTC session).
        //
        // Insert unconditionally on completed candles. On the very first
        // UTC day of operation `inputs.pivot = None` (no prior session has
        // finalized yet), so the unwrap defaults to `0.0` and
        // `normalize_pivot_points` returns `state_label="PIVOT_UNAVAILABLE"`
        // instead of letting the WARMING-fill overwrite with `state_label=
        // "WARMING"` — which previously made the dashboard show an
        // ambiguous Loading/WaitingFeed for the first ~24 h of every
        // daemon start. Shadow ticks are excluded so the frontend's
        // per-key merge keeps the last completed-candle values intact
        // between candle closes.
        if !shadow {
            out.insert(
                "pivot_points".into(),
                Self::normalize_pivot_points(
                    ctx.price,
                    inputs.pivot.unwrap_or(0.0),
                    inputs.pivot_r1.unwrap_or(0.0),
                    inputs.pivot_r2.unwrap_or(0.0),
                    inputs.pivot_r3.unwrap_or(0.0),
                    inputs.pivot_s1.unwrap_or(0.0),
                    inputs.pivot_s2.unwrap_or(0.0),
                    inputs.pivot_s3.unwrap_or(0.0),
                    inputs.pivot_proximity_pct,
                ),
            );
        }

        // ── Candlestick patterns: Stage 2 (Context Validation) ──
        // Stage 1 geometry + Stage 3 confirmation happened upstream in the
        // `Candlestick` calculator. Here we cross-check the live indicator map
        // (trend / S-R / volume / volatility / regime) to adjust confidence and
        // reject low-context readings before scoring.
        //
        // Insert unconditionally on completed candles. The Candlestick
        // calculator returns `Some(CandlestickResult { pattern: None,
        // status: None, ... })` on every bar where no recognizable pattern
        // forms (the vast majority). Previously the inner `if status_code
        // != 0` check skipped insertion in that case, letting the WARMING
        // fill overwrite with `state_label="WARMING"` and producing a
        // misleading Loading state on the dashboard. Inserting the entry
        // unconditionally lets `normalize_candlestick` produce a clear
        // `state_label="NONE_UNCONFIRMED"` (or the real pattern name when
        // one fires) so the trader reads "no pattern forming right now"
        // vs. "pattern just fired" — accurate.
        if let Some(cs) = inputs.candlestick {
            let status_code = match cs.status {
                CandlestickStatus::Formed => 1u8,
                CandlestickStatus::Confirmed => 2u8,
                CandlestickStatus::Invalidated => 3u8,
                CandlestickStatus::None => 0u8,
            };
            let dir = cs.direction as f64;
            let read_norm = |k: &str| out.get(k).map(|v| v.normalized).unwrap_or(0.0);
            let read_raw = |k: &str| out.get(k).map(|v| v.raw_value).unwrap_or(0.0);

            // Trend alignment: reversal patterns gain when the prevailing
            // ema-stack trend opposes them (exhaustion); continuation
            // patterns gain when aligned.
            let trend = read_norm("ema_stack");
            let is_continuation = cs.pattern.category() == "continuation";
            let trend_factor = if is_continuation {
                if trend * dir > 0.0 {
                    1.25
                } else {
                    0.8
                }
            } else if trend * dir < 0.0 {
                1.25 // reversal against trend = higher value
            } else {
                0.9
            };

            // S/R proximity: a pattern at a structural level is stronger.
            let sr_mag = read_norm("support_resistance").abs();
            let pivot_mag = read_norm("pivot_points").abs();
            let struct_factor = 1.0 + 0.3 * sr_mag.max(pivot_mag);

            // Volume: institutional participation reinforces the pattern.
            let rvol = read_raw("rvol");
            let vol_factor = if rvol >= 1.5 {
                1.2
            } else if rvol > 0.0 && rvol < 0.8 {
                0.85
            } else {
                1.0
            };

            // Regime: choppy/range conditions reduce reliability.
            let chop = read_raw("choppiness");
            let regime_factor = if chop >= 61.8 {
                0.75
            } else if chop > 0.0 && chop <= 38.2 {
                1.1
            } else {
                1.0
            };

            // Volatility: extreme BBWP expansion can produce noise wicks.
            let bbwp = read_raw("bbwp");
            let vola_factor = if bbwp >= 95.0 { 0.85 } else { 1.0 };

            let context_mult =
                (trend_factor * struct_factor * vol_factor * regime_factor * vola_factor)
                    .clamp(0.0, 2.0);

            out.insert(
                "candlestick".into(),
                Self::normalize_candlestick(
                    cs.pattern.name(),
                    cs.pattern.category(),
                    cs.direction,
                    cs.quality,
                    status_code,
                    context_mult,
                    inputs.candlestick_min_confidence,
                ),
            );
        }

        // ── Ichimoku Cloud: complete trend system ──
        if let (Some(tenkan), Some(kijun), Some(sa), Some(sb)) = (
            inputs.ichimoku_tenkan,
            inputs.ichimoku_kijun,
            inputs.ichimoku_senkou_a,
            inputs.ichimoku_senkou_b,
        ) {
            let sa_cur = inputs.ichimoku_senkou_a_current.unwrap_or(sa);
            let sb_cur = inputs.ichimoku_senkou_b_current.unwrap_or(sb);
            let chikou = inputs.ichimoku_chikou.unwrap_or(ctx.price);
            let mut entry =
                Self::normalize_ichimoku(ctx.price, tenkan, kijun, sa, sb, sa_cur, sb_cur, chikou);

            // Transition signals via previous-bar state.
            let cloud_top = sa_cur.max(sb_cur);
            let cloud_bottom = sa_cur.min(sb_cur);
            let cur_side = if ctx.price > cloud_top {
                1.0
            } else if ctx.price < cloud_bottom {
                -1.0
            } else {
                0.0
            };

            // Tenkan/Kijun crossover (Crossover).
            if let (Some(pt), Some(pk)) = (ctx.prev.ichimoku_tenkan, ctx.prev.ichimoku_kijun) {
                if pt <= pk && tenkan > kijun {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "ICHIMOKU_TK_CROSS_BULLISH",
                    ));
                } else if pt >= pk && tenkan < kijun {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "ICHIMOKU_TK_CROSS_BEARISH",
                    ));
                }
            }

            // Cloud breakout / entering / leaving (Breakout + LevelTest).
            if let Some(prev_side) = ctx.prev.price_vs_cloud {
                if prev_side <= 0.0 && cur_side > 0.0 {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::Breakout,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "ICHIMOKU_CLOUD_BREAKOUT_UP",
                    ));
                } else if prev_side >= 0.0 && cur_side < 0.0 {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::Breakout,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "ICHIMOKU_CLOUD_BREAKOUT_DOWN",
                    ));
                } else if prev_side != 0.0 && cur_side == 0.0 {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::LevelTest,
                        SignalDirection::Neutral,
                        SignalStatus::Active,
                        "ICHIMOKU_PRICE_ENTERING_CLOUD",
                    ));
                }
            }

            // Future cloud twist (TrendFlip): the forward Senkou A crosses
            // Senkou B, flipping the future cloud colour.
            let fut_color = (sa - sb).signum();
            if let Some(prev_fut) = ctx.prev.ichimoku_future_bias {
                if prev_fut <= 0.0 && fut_color > 0.0 {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::TrendFlip,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "ICHIMOKU_FUTURE_CLOUD_TWIST_BULLISH",
                    ));
                } else if prev_fut >= 0.0 && fut_color < 0.0 {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::TrendFlip,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "ICHIMOKU_FUTURE_CLOUD_TWIST_BEARISH",
                    ));
                }
            }

            // Chikou confirmation: the lagging span (current close) clearing the
            // cloud in the trend direction reinforces conviction.
            if chikou > cloud_top && cur_side > 0.0 {
                entry.signals.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    "ICHIMOKU_CHIKOU_CONFIRMS_BULL",
                ));
            } else if chikou < cloud_bottom && cur_side < 0.0 {
                entry.signals.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    "ICHIMOKU_CHIKOU_CONFIRMS_BEAR",
                ));
            }

            out.insert("ichimoku".into(), entry);
        }

        // Supplemental raw-only chart series (neutral normalized score so they
        // never influence the confluence engine).
        if inputs.atr_14.is_some() || inputs.atr_slope.is_some() {
            let mut values = HashMap::new();
            if let Some(a) = inputs.atr_14 {
                values.insert("atr_14".to_string(), a);
            }
            if let Some(s) = inputs.atr_slope {
                values.insert("atr_slope".to_string(), s);
            }
            out.insert(
                "atr".into(),
                NormalizedIndicatorValue::with_values(
                    inputs.atr_14.unwrap_or(0.0),
                    0.0,
                    "ATR_RAW",
                    values,
                ),
            );
        }

        if inputs.bb_upper.is_some() || inputs.bb_middle.is_some() || inputs.bb_lower.is_some() {
            let mut values = HashMap::new();
            if let Some(u) = inputs.bb_upper {
                values.insert("upper".to_string(), u);
            }
            if let Some(m) = inputs.bb_middle {
                values.insert("middle".to_string(), m);
            }
            if let Some(l) = inputs.bb_lower {
                values.insert("lower".to_string(), l);
            }
            out.insert(
                "bollinger".into(),
                NormalizedIndicatorValue::with_values(
                    inputs.bb_middle.unwrap_or(0.0),
                    0.0,
                    "BOLLINGER_RAW",
                    values,
                ),
            );
        }

        if let (Some(u), Some(c), Some(l)) = (
            inputs.stddev_upper,
            inputs.stddev_center,
            inputs.stddev_lower,
        ) {
            out.insert(
                "stddev_channel".into(),
                Self::normalize_stddev_channel(ctx.price, u, c, l),
            );
        }

        if let (Some(poc), Some(vah), Some(val)) = (
            inputs.volprofile_poc,
            inputs.volprofile_vah,
            inputs.volprofile_val,
        ) {
            out.insert(
                "volume_profile".into(),
                Self::normalize_volume_profile(
                    ctx.price,
                    poc,
                    vah,
                    val,
                    inputs.volprofile_total_volume,
                ),
            );
        }

        // SMC Structure: BOS/CHoCH detection (Breakout/TrendFlip signals).
        if inputs.smc_structure_bullish
            || inputs.smc_structure_bearish
            || inputs.smc_bos_bullish
            || inputs.smc_bos_bearish
            || inputs.smc_choch_bullish
            || inputs.smc_choch_bearish
        {
            out.insert(
                "smc_structure".into(),
                Self::normalize_smc_structure(
                    inputs.smc_structure_bullish,
                    inputs.smc_structure_bearish,
                    inputs.smc_bos_bullish,
                    inputs.smc_bos_bearish,
                    inputs.smc_choch_bullish,
                    inputs.smc_choch_bearish,
                ),
            );
        }

        // SMC Liquidity: buy/sell-side sweep detection.
        if inputs.smc_liq_sweep_buy || inputs.smc_liq_sweep_sell {
            out.insert(
                "smc_liquidity".into(),
                Self::normalize_smc_liquidity(inputs.smc_liq_sweep_buy, inputs.smc_liq_sweep_sell),
            );
        }

        // SMC Fair Value Gap: 3-candle imbalance detection.
        if inputs.smc_fvg_top.is_some() || inputs.smc_fvg_bottom.is_some() {
            out.insert(
                "smc_fvg".into(),
                Self::normalize_smc_fvg(
                    inputs.smc_fvg_top,
                    inputs.smc_fvg_bottom,
                    inputs.smc_fvg_bullish,
                ),
            );
        }

        // SMC Order Blocks: institutional zone detection.
        if inputs.smc_ob_bullish_high.is_some() || inputs.smc_ob_bearish_high.is_some() {
            out.insert(
                "smc_order_blocks".into(),
                Self::normalize_smc_order_blocks(
                    ctx.price,
                    inputs.smc_ob_bullish_high,
                    inputs.smc_ob_bullish_low,
                    inputs.smc_ob_bearish_high,
                    inputs.smc_ob_bearish_low,
                ),
            );
        }

        // Generalized divergence scored entries (Phase 2). Each also pushes a
        // Divergence signal onto its parent oscillator.
        for (parent, _key, state) in [
            (
                "stochastic",
                "stochastic_divergence",
                inputs.stochastic_divergence,
            ),
            (
                "chandemo",
                "chandemo_divergence",
                inputs.chandemo_divergence,
            ),
            ("mfi", "mfi_divergence", inputs.mfi_divergence),
            ("cmf", "cmf_divergence", inputs.cmf_divergence),
            ("obv", "obv_divergence", inputs.obv_divergence),
            ("squeeze", "squeeze_divergence", inputs.squeeze_divergence),
        ] {
            let _ = super::signals::divergence_entry(&mut out, parent, state);
        }

        // ── Structured cross-over / zero-cross detection ──
        // Every detector below compares current indicator values against the
        // previous completed bar (ctx.prev) and emits discrete signals when a
        // state transition is detected.

        // Supertrend flip (TrendFlip).
        if inputs.supertrend_flipped {
            if let Some(entry) = out.get_mut("supertrend") {
                let d = if inputs.supertrend_dir == Some(1) {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                };
                entry.signals.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    d,
                    SignalStatus::Active,
                    if d == SignalDirection::Bullish {
                        "SUPERTREND_BULLISH_FLIP"
                    } else {
                        "SUPERTREND_BEARISH_FLIP"
                    },
                ));
            }
        }

        // PSAR flip (TrendFlip).
        if inputs.psar_flipped {
            if let Some(entry) = out.get_mut("psar") {
                let d = if inputs.psar_direction == Some(1) {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                };
                entry.signals.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    d,
                    SignalStatus::Active,
                    if d == SignalDirection::Bullish {
                        "PSAR_BULLISH_FLIP"
                    } else {
                        "PSAR_BEARISH_FLIP"
                    },
                ));
            }
        }

        // ADX DI crossover (TrendFlip).
        if let Some(cross) = inputs.adx_di_crossover {
            if let Some(entry) = out.get_mut("adx") {
                let (d, label) = if cross > 0 {
                    (SignalDirection::Bullish, "ADX_DI_CROSSOVER_BULLISH")
                } else {
                    (SignalDirection::Bearish, "ADX_DI_CROSSOVER_BEARISH")
                };
                entry.signals.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    d,
                    SignalStatus::Active,
                    label,
                ));
            }
        }

        // RSI midline cross (ZeroLineCross = RSI crosses 50).
        if let (Some(rsi), Some(prev_rsi)) = (inputs.rsi, ctx.prev.rsi) {
            if (prev_rsi <= 50.0 && rsi > 50.0) || (prev_rsi >= 50.0 && rsi < 50.0) {
                if let Some(entry) = out.get_mut("rsi") {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        if rsi > 50.0 {
                            SignalDirection::Bullish
                        } else {
                            SignalDirection::Bearish
                        },
                        SignalStatus::Active,
                        if rsi > 50.0 {
                            "RSI_ZERO_CROSS_BULLISH"
                        } else {
                            "RSI_ZERO_CROSS_BEARISH"
                        },
                    ));
                }
            }
        }

        // Stochastic K/D crossover.
        if let (Some(k), Some(d), Some(pk), Some(pd)) = (
            inputs.stoch_k,
            inputs.stoch_d,
            ctx.prev.stoch_k,
            ctx.prev.stoch_d,
        ) {
            if (pk <= pd && k > d) || (pk >= pd && k < d) {
                if let Some(entry) = out.get_mut("stochastic") {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        if k > d {
                            SignalDirection::Bullish
                        } else {
                            SignalDirection::Bearish
                        },
                        SignalStatus::Active,
                        if k > d {
                            "STOCH_BULLISH_CROSSOVER"
                        } else {
                            "STOCH_BEARISH_CROSSOVER"
                        },
                    ));
                }
            }
        }

        // ChandeMO / CMF / LinReg / Z-Score zero cross.
        for (key, current, prev_opt) in &[
            ("chandemo", inputs.chandemo, ctx.prev.chandemo),
            ("cmf", inputs.cmf, ctx.prev.cmf),
            ("linreg_slope", inputs.linreg_slope, ctx.prev.linreg_slope),
            ("zscore", inputs.zscore, ctx.prev.zscore),
        ] {
            if let (Some(cur), Some(prev)) = (*current, *prev_opt) {
                if (prev <= 0.0 && cur > 0.0) || (prev >= 0.0 && cur < 0.0) {
                    if let Some(entry) = out.get_mut(*key) {
                        entry.signals.push(IndicatorSignal::new(
                            SignalKind::ZeroLineCross,
                            if cur > 0.0 {
                                SignalDirection::Bullish
                            } else {
                                SignalDirection::Bearish
                            },
                            SignalStatus::Active,
                            format!(
                                "{}_ZERO_CROSS_{}",
                                key.to_uppercase(),
                                if cur > 0.0 { "BULLISH" } else { "BEARISH" }
                            ),
                        ));
                    }
                }
            }
        }

        // OBV trend-flip: accumulation ↔ distribution transition.
        if let (Some(obv), Some(prev_obv), Some(sma), Some(prev_sma)) =
            (inputs.obv, ctx.prev.obv, inputs.obv_sma, ctx.prev.obv_sma)
        {
            let cur_above = obv > sma;
            let prev_above = prev_obv > prev_sma;
            if cur_above != prev_above {
                if let Some(entry) = out.get_mut("obv") {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::TrendFlip,
                        if cur_above {
                            SignalDirection::Bullish
                        } else {
                            SignalDirection::Bearish
                        },
                        SignalStatus::Active,
                        if cur_above {
                            "OBV_TREND_FLIP_BULLISH"
                        } else {
                            "OBV_TREND_FLIP_BEARISH"
                        },
                    ));
                }
            }
        }

        // Aroon crossover: Up crosses Down.
        if let (Some(up), Some(down), Some(pu), Some(pd)) = (
            inputs.aroon_up,
            inputs.aroon_down,
            ctx.prev.aroon_up,
            ctx.prev.aroon_down,
        ) {
            if (pu <= pd && up > down) || (pu >= pd && down > up) {
                if let Some(entry) = out.get_mut("aroon") {
                    entry.signals.push(IndicatorSignal::new(
                        // SIG-02 (v2.1 reclassification): Aroon's Up/Down crossing
                        // is emitted as TrendFlip, not Crossover, since it
                        // represents a directional regime change rather than
                        // a generic two-series cross. Prevents double-counting
                        // in the confluence engine.
                        SignalKind::TrendFlip,
                        if up > down {
                            SignalDirection::Bullish
                        } else {
                            SignalDirection::Bearish
                        },
                        SignalStatus::Active,
                        if up > down {
                            "AROON_BULLISH_FLIP"
                        } else {
                            "AROON_BEARISH_FLIP"
                        },
                    ));
                }
            }
        }

        // MFI midline cross (50).
        if let (Some(mfi), Some(prev_mfi)) = (inputs.mfi, ctx.prev.mfi) {
            if (prev_mfi <= 50.0 && mfi > 50.0) || (prev_mfi >= 50.0 && mfi < 50.0) {
                if let Some(entry) = out.get_mut("mfi") {
                    entry.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        if mfi > 50.0 {
                            SignalDirection::Bullish
                        } else {
                            SignalDirection::Bearish
                        },
                        SignalStatus::Active,
                        if mfi > 50.0 {
                            "MFI_CROSSOVER_BULLISH"
                        } else {
                            "MFI_CROSSOVER_BEARISH"
                        },
                    ));
                }
            }
        }

        // Bollinger Band Touch (price at band edge).
        if let (Some(upper), Some(middle), Some(lower)) =
            (inputs.bb_upper, inputs.bb_middle, inputs.bb_lower)
        {
            let price = ctx.price;
            let inside = price >= lower && price <= upper;
            if !inside {
                if let Some(entry) = out.get_mut("bollinger") {
                    if price > upper {
                        entry.signals.push(IndicatorSignal::new(
                            SignalKind::Breakout,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "BOLLINGER_UPPER_BREAKOUT",
                        ));
                    } else {
                        entry.signals.push(IndicatorSignal::new(
                            SignalKind::Breakout,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "BOLLINGER_LOWER_BREAKOUT",
                        ));
                    }
                }
            } else {
                // Band touch (near band edge but inside)
                let band_w = upper - middle;
                if band_w > 0.0 {
                    let pct = (price - lower) / (upper - lower);
                    if pct > 0.90 {
                        if let Some(entry) = out.get_mut("bollinger") {
                            entry.signals.push(IndicatorSignal::new(
                                SignalKind::BandTouch,
                                SignalDirection::Bearish,
                                SignalStatus::Active,
                                "BOLLINGER_UPPER_BAND_TOUCH",
                            ));
                        }
                    } else if pct < 0.10 {
                        if let Some(entry) = out.get_mut("bollinger") {
                            entry.signals.push(IndicatorSignal::new(
                                SignalKind::BandTouch,
                                SignalDirection::Bullish,
                                SignalStatus::Active,
                                "BOLLINGER_LOWER_BAND_TOUCH",
                            ));
                        }
                    }
                }
            }
            // Normalize Bollinger: how far price is within bands (-1 bottom to +1 top)
            let norm = if upper > lower {
                ((price - middle) / (upper - middle)).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            if let Some(entry) = out.get_mut("bollinger") {
                entry.normalized = norm;
                entry.state_label = if price > upper {
                    "BOLLINGER_UPPER_BREAKOUT".into()
                } else if price < lower {
                    "BOLLINGER_LOWER_BREAKOUT".into()
                } else if (price - lower) / (upper - lower).max(f64::EPSILON) > 0.90 {
                    "BOLLINGER_UPPER_BAND_TOUCH".into()
                } else if (price - lower) / (upper - lower).max(f64::EPSILON) < 0.10 {
                    "BOLLINGER_LOWER_BAND_TOUCH".into()
                } else {
                    "BOLLINGER_INSIDE_BANDS".into()
                };
            }
        }

        // ATR expansion / contraction — driven by the scale-invariant regime
        // classifier (current ATR vs 5-period mean ATR; 1.02/0.98 bands), not
        // by a raw slope threshold of ±0.01 which was non-portable across
        // assets of different price scales.
        if inputs.atr_14.is_some() {
            if let Some(regime) = inputs.atr_regime {
                let label = match regime {
                    VolatilityRegime::Expanding => "ATR_EXPANDING",
                    VolatilityRegime::Contracting => "ATR_CONTRACTING",
                    VolatilityRegime::Stable => "ATR_STABLE",
                };
                if let Some(entry) = out.get_mut("atr") {
                    entry.state_label = label.into();
                    match regime {
                        VolatilityRegime::Expanding => entry.signals.push(IndicatorSignal::new(
                            SignalKind::Threshold,
                            SignalDirection::Neutral,
                            SignalStatus::Active,
                            "ATR_EXPANDING",
                        )),
                        VolatilityRegime::Contracting => entry.signals.push(IndicatorSignal::new(
                            SignalKind::CompressionRelease,
                            SignalDirection::Neutral,
                            SignalStatus::Active,
                            "ATR_CONTRACTING",
                        )),
                        VolatilityRegime::Stable => {}
                    }
                }
            }
        }

        // ── Donchian BandTouch (distinct from Breakout): price near a band
        // edge but still inside the channel (mean-reversion proximity). ──
        if let (Some(u), Some(l)) = (inputs.donchian_upper, inputs.donchian_lower) {
            let price = ctx.price;
            if price < u && price > l && u > l {
                let pos = (price - l) / (u - l);
                if pos > 0.85 {
                    if let Some(e) = out.get_mut("donchian") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "DONCHIAN_UPPER_BAND_TOUCH",
                        ));
                    }
                } else if pos < 0.15 {
                    if let Some(e) = out.get_mut("donchian") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "DONCHIAN_LOWER_BAND_TOUCH",
                        ));
                    }
                }
            }
        }

        // ── Keltner BandTouch (distinct from Breakout). ──
        if let (Some(u), Some(l)) = (inputs.keltner_upper, inputs.keltner_lower) {
            let price = ctx.price;
            if price < u && price > l && u > l {
                let pos = (price - l) / (u - l);
                if pos > 0.85 {
                    if let Some(e) = out.get_mut("keltner") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "KELTNER_UPPER_BAND_TOUCH",
                        ));
                    }
                } else if pos < 0.15 {
                    if let Some(e) = out.get_mut("keltner") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "KELTNER_LOWER_BAND_TOUCH",
                        ));
                    }
                }
            }
        }

        // ── StdDev Channel BandTouch (distinct from Breakout). ──
        if let (Some(u), Some(l)) = (inputs.stddev_upper, inputs.stddev_lower) {
            let price = ctx.price;
            if price < u && price > l && u > l {
                let pos = (price - l) / (u - l);
                if pos > 0.85 {
                    if let Some(e) = out.get_mut("stddev_channel") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "STDDEV_UPPER_BAND_TOUCH",
                        ));
                    }
                } else if pos < 0.15 {
                    if let Some(e) = out.get_mut("stddev_channel") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::BandTouch,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "STDDEV_LOWER_BAND_TOUCH",
                        ));
                    }
                }
            }
        }

        // ── EMA stack-order change (StackChange, transition-only) ──
        // AUDIT-AIU-030: StackChange must fire ONLY on the bar where the
        // ribbon order (or price-vs-fast placement) crosses — a momentary
        // event per 05-02-11 §4. The previous stateless deriver emitted a
        // StackChange on EVERY bar whose label contained "STACK", spamming
        // persistent Active signals. We compute the stack state for the
        // current and previous bars from the four EMAs + close and emit only
        // on a state transition.
        if let (Some(f), Some(m), Some(s), Some(lng), Some(pf), Some(pm), Some(ps), Some(pl)) = (
            inputs.ema_fast,
            inputs.ema_medium,
            inputs.ema_slow,
            inputs.ema_long,
            ctx.prev.ema_fast,
            ctx.prev.ema_medium,
            ctx.prev.ema_slow,
            ctx.prev.ema_long,
        ) {
            let stack_state = |fast: f64, med: f64, slow: f64, long: f64, price: f64| -> i8 {
                if fast > med && med > slow && slow > long && price > fast {
                    1
                } else if fast < med && med < slow && slow < long && price < fast {
                    -1
                } else {
                    0
                }
            };
            let cur = stack_state(f, m, s, lng, ctx.price);
            let prev_state = stack_state(pf, pm, ps, pl, ctx.prev.price.unwrap_or(ctx.price));
            if cur != prev_state {
                if let Some(e) = out.get_mut("ema_stack") {
                    let (d, label) = match cur {
                        1 => (SignalDirection::Bullish, "EMA_STACK_BULLISH_REORDER"),
                        -1 => (SignalDirection::Bearish, "EMA_STACK_BEARISH_REORDER"),
                        _ => (SignalDirection::Neutral, "EMA_STACK_TANGLED"),
                    };
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::StackChange,
                        d,
                        SignalStatus::Confirmed,
                        label,
                    ));
                }
            }
        }

        // ── EMA fast/medium Crossover (distinct from StackChange). ──
        if let (Some(f), Some(m), Some(pf), Some(pm)) = (
            inputs.ema_fast,
            inputs.ema_medium,
            ctx.prev.ema_fast,
            ctx.prev.ema_medium,
        ) {
            if pf <= pm && f > m {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "EMA_FAST_MEDIUM_BULLISH_CROSS",
                    ));
                }
            } else if pf >= pm && f < m {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "EMA_FAST_MEDIUM_BEARISH_CROSS",
                    ));
                }
            }
        }

        // ── Supertrend price/line Crossover (distinct from TrendFlip). ──
        if let (Some(line), Some(pline), Some(pprice)) = (
            inputs.supertrend_line,
            ctx.prev.supertrend_line,
            ctx.prev.price,
        ) {
            let price = ctx.price;
            if pprice <= pline && price > line {
                if let Some(e) = out.get_mut("supertrend") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "SUPERTREND_PRICE_CROSS_BULLISH",
                    ));
                }
            } else if pprice >= pline && price < line {
                if let Some(e) = out.get_mut("supertrend") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "SUPERTREND_PRICE_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── PSAR price/SAR line Crossover (distinct from TrendFlip). ──
        if let (Some(sar), Some(psar_prev), Some(pprice)) =
            (inputs.psar_sar, ctx.prev.psar_sar, ctx.prev.price)
        {
            let price = ctx.price;
            if pprice <= psar_prev && price > sar {
                if let Some(e) = out.get_mut("psar") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "PSAR_PRICE_CROSS_BULLISH",
                    ));
                }
            } else if pprice >= psar_prev && price < sar {
                if let Some(e) = out.get_mut("psar") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "PSAR_PRICE_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── Pivot central crossover (distinct from level-test proximity):
        // fires on the bar where price crosses the central pivot, using the
        // prior bar's side-of-pivot. `pivot_active_level` carries the signed
        // side (+1 above, -1 below) from the previous bar. ──
        if let (Some(p), Some(prev_side)) = (inputs.pivot, ctx.prev.pivot_active_level) {
            if p > 0.0 && ctx.price > 0.0 {
                let cur_side = if ctx.price >= p { 1.0 } else { -1.0 };
                if prev_side < 0.0 && cur_side > 0.0 {
                    if let Some(e) = out.get_mut("pivot_points") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Crossover,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "PIVOT_CENTRAL_CROSS_BULLISH",
                        ));
                    }
                } else if prev_side > 0.0 && cur_side < 0.0 {
                    if let Some(e) = out.get_mut("pivot_points") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Crossover,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "PIVOT_CENTRAL_CROSS_BEARISH",
                        ));
                    }
                }
            }
        }

        // ── Pivot level Breakouts (AUDIT-AIU-038) ──
        // The registry declares Breakout for pivot_points but the label-based
        // deriver could never fire it (no label containing "BREAKOUT"/"FLIP"
        // was ever emitted). Price crossing a pivot resistance from below is
        // an upward breakout; crossing a support from above is a downward
        // break. Detected structurally using the previous bar's price vs the
        // session levels (levels are stable within a session; rollover bars
        // simply won't produce a transition).
        if let (Some(ppx), Some(r1), Some(s1)) = (ctx.prev.price, inputs.pivot_r1, inputs.pivot_s1)
        {
            if ppx <= r1 && ctx.price > r1 {
                if let Some(e) = out.get_mut("pivot_points") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Breakout,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "PIVOT_R1_BREAKOUT",
                    ));
                }
            } else if ppx >= s1 && ctx.price < s1 {
                if let Some(e) = out.get_mut("pivot_points") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Breakout,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "PIVOT_S1_BREAKOUT",
                    ));
                }
            }
        }

        // ── EMA price/vs fast-EMA crossover (Crossover).
        // The EMA Ribbon registry entry claims Crossover; this is the price
        // crossing the fast EMA line, distinct from the EMA fast/medium
        // Crossover which fires on the ribbon-internal fast-vs-med cross. ──
        if let (Some(ppx), Some(pema), Some(ema)) =
            (ctx.prev.price, ctx.prev.ema_fast, inputs.ema_fast)
        {
            if ppx <= pema && ctx.price > ema {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "EMA_PRICE_CROSS_FAST_BULLISH",
                    ));
                }
            } else if ppx >= pema && ctx.price < ema {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "EMA_PRICE_CROSS_FAST_BEARISH",
                    ));
                }
            }
        }

        // ── EMA price/vs medium-EMA crossover (Crossover).
        // AUDIT-AIU-031: the doc (04-02-01 §Signals) declares the medium-EMA
        // price-cross variant but no code emitted it. Mirrors the fast-EMA
        // block with the medium line.
        if let (Some(ppx), Some(pema), Some(ema)) =
            (ctx.prev.price, ctx.prev.ema_medium, inputs.ema_medium)
        {
            if ppx <= pema && ctx.price > ema {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "EMA_PRICE_CROSS_MEDIUM_BULLISH",
                    ));
                }
            } else if ppx >= pema && ctx.price < ema {
                if let Some(e) = out.get_mut("ema_stack") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "EMA_PRICE_CROSS_MEDIUM_BEARISH",
                    ));
                }
            }
        }

        // ── Supertrend line proximity (LevelTest — price near dynamic S/R line). ──
        if let (Some(line), Some(dir)) = (inputs.supertrend_line, inputs.supertrend_dir) {
            let dist = if line.abs() > f64::EPSILON {
                (ctx.price - line).abs() / line
            } else {
                0.0
            };
            if dist < 0.005 {
                if let Some(e) = out.get_mut("supertrend") {
                    let d = if dir == 1 {
                        SignalDirection::Bearish
                    } else {
                        SignalDirection::Bullish
                    };
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::LevelTest,
                        d,
                        SignalStatus::Active,
                        if d == SignalDirection::Bearish {
                            "SUPERTREND_RESISTANCE_TEST"
                        } else {
                            "SUPERTREND_SUPPORT_TEST"
                        },
                    ));
                }
            }
        }

        // ── Stochastic 50-midline cross (ZeroLineCross). ──
        if let (Some(k), Some(pk)) = (inputs.stoch_k, ctx.prev.stoch_k) {
            if pk <= 50.0 && k > 50.0 {
                if let Some(e) = out.get_mut("stochastic") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "STOCH_50_CROSS_BULLISH",
                    ));
                }
            } else if pk >= 50.0 && k < 50.0 {
                if let Some(e) = out.get_mut("stochastic") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "STOCH_50_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── MACD histogram sign flip (TrendFlip). ──
        if let (Some(hist), Some(prev_hist)) = (inputs.macd_histogram, ctx.prev.macd_histogram) {
            if prev_hist <= 0.0 && hist > 0.0 {
                if let Some(e) = out.get_mut("macd") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::TrendFlip,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "MACD_HISTOGRAM_FLIP_BULLISH",
                    ));
                }
            } else if prev_hist >= 0.0 && hist < 0.0 {
                if let Some(e) = out.get_mut("macd") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::TrendFlip,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "MACD_HISTOGRAM_FLIP_BEARISH",
                    ));
                }
            }
        }

        // ── LevelTest on band proximity (bollinger/donchian/keltner/stddev). ──
        for (key, upper, lower) in &[
            ("bollinger", inputs.bb_upper, inputs.bb_lower),
            ("keltner", inputs.keltner_upper, inputs.keltner_lower),
            ("donchian", inputs.donchian_upper, inputs.donchian_lower),
            ("stddev_channel", inputs.stddev_upper, inputs.stddev_lower),
        ] {
            if let (Some(u), Some(l)) = (*upper, *lower) {
                if ctx.price < u && ctx.price > l && u > l {
                    let pos = (ctx.price - l) / (u - l);
                    if pos > 0.6 && pos <= 0.85 {
                        if let Some(e) = out.get_mut(*key) {
                            e.signals.push(IndicatorSignal::new(
                                SignalKind::LevelTest,
                                SignalDirection::Neutral,
                                SignalStatus::Active,
                                format!("{}_UPPER_LEVEL_TEST", key.to_uppercase()),
                            ));
                        }
                    } else if (0.15..0.4).contains(&pos) {
                        if let Some(e) = out.get_mut(*key) {
                            e.signals.push(IndicatorSignal::new(
                                SignalKind::LevelTest,
                                SignalDirection::Neutral,
                                SignalStatus::Active,
                                format!("{}_LOWER_LEVEL_TEST", key.to_uppercase()),
                            ));
                        }
                    }
                }
            }
        }

        // ── HMA price cross (Crossover). ──
        if let (Some(pprice), Some(prev_hma), Some(hma)) =
            (ctx.prev.price, ctx.prev.hull_ma, inputs.hull_ma)
        {
            if pprice <= prev_hma && ctx.price > hma {
                if let Some(e) = out.get_mut("hull_ma") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "HMA_PRICE_CROSS_BULLISH",
                    ));
                }
            } else if pprice >= prev_hma && ctx.price < hma {
                if let Some(e) = out.get_mut("hull_ma") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::Crossover,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "HMA_PRICE_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── Awesome Oscillator zero-line cross (ZeroLineCross). ──
        if let (Some(ao), Some(pao)) = (inputs.awesome_oscillator, ctx.prev.awesome_oscillator) {
            if pao <= 0.0 && ao > 0.0 {
                if let Some(e) = out.get_mut("awesome_oscillator") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "AO_ZERO_CROSS_BULLISH",
                    ));
                }
            } else if pao >= 0.0 && ao < 0.0 {
                if let Some(e) = out.get_mut("awesome_oscillator") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "AO_ZERO_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── Awesome Oscillator threshold (extreme values). ──
        // AUDIT-AIU-044: the extreme threshold was an absolute ±50 on a
        // price-scaled oscillator — scale-dependent across assets. Now
        // ATR-relative: extreme when |ao| > 5× ATR. Falls back to the
        // absolute ±50 only when ATR is unavailable.
        if let Some(ao) = inputs.awesome_oscillator {
            let atr = inputs.atr_14.filter(|a| *a > 0.0).unwrap_or(10.0);
            let extreme = 5.0 * atr;
            if ao > extreme {
                if let Some(e) = out.get_mut("awesome_oscillator") {
                    if !e.signals.iter().any(|s| s.label == "AO_EXTREME_BULLISH") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Threshold,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "AO_EXTREME_BULLISH",
                        ));
                    }
                }
            } else if ao < -extreme {
                if let Some(e) = out.get_mut("awesome_oscillator") {
                    if !e.signals.iter().any(|s| s.label == "AO_EXTREME_BEARISH") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Threshold,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "AO_EXTREME_BEARISH",
                        ));
                    }
                }
            }
        }

        // ── Force Index zero-line cross (ZeroLineCross). ──
        if let (Some(fi), Some(pfi)) = (inputs.force_index, ctx.prev.force_index) {
            if pfi <= 0.0 && fi > 0.0 {
                if let Some(e) = out.get_mut("force_index") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "FI_ZERO_CROSS_BULLISH",
                    ));
                }
            } else if pfi >= 0.0 && fi < 0.0 {
                if let Some(e) = out.get_mut("force_index") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "FI_ZERO_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── Force Index threshold (extreme values). ──
        // AUDIT-AIU-043: the extreme threshold was an absolute ±1000 on a
        // raw price×volume-scaled quantity — meaningless across assets ($100k
        // BTC contracts vs $0.01 alts). Now scale-relative: FI is extreme
        // when |fi| > 30× the rolling mean of |fi| (an outlier gate). The
        // rolling mean is carried in `values.mean_abs` by the normalizer.
        if let Some(fi) = inputs.force_index {
            let mean_abs = out
                .get("force_index")
                .and_then(|e| e.values.as_ref())
                .and_then(|v| v.get("mean_abs"))
                .copied()
                .unwrap_or(0.0);
            if fi > 0.0 && mean_abs > 0.0 && fi > mean_abs * 30.0 {
                if let Some(e) = out.get_mut("force_index") {
                    if !e.signals.iter().any(|s| s.label == "FI_EXTREME_BULLISH") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Threshold,
                            SignalDirection::Bullish,
                            SignalStatus::Active,
                            "FI_EXTREME_BULLISH",
                        ));
                    }
                }
            } else if fi < 0.0 && mean_abs > 0.0 && fi.abs() > mean_abs * 30.0 {
                if let Some(e) = out.get_mut("force_index") {
                    if !e.signals.iter().any(|s| s.label == "FI_EXTREME_BEARISH") {
                        e.signals.push(IndicatorSignal::new(
                            SignalKind::Threshold,
                            SignalDirection::Bearish,
                            SignalStatus::Active,
                            "FI_EXTREME_BEARISH",
                        ));
                    }
                }
            }
        }

        // ── Williams %R zero-line cross (ZeroLineCross — midpoint at -50). ──
        if let (Some(wr), Some(pwr)) = (inputs.williams_r, ctx.prev.williams_r) {
            if pwr <= -50.0 && wr > -50.0 {
                if let Some(e) = out.get_mut("williams_r") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "WR_50_CROSS_BULLISH",
                    ));
                }
            } else if pwr >= -50.0 && wr < -50.0 {
                if let Some(e) = out.get_mut("williams_r") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "WR_50_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── CCI zero-line cross (ZeroLineCross). ──
        if let (Some(cci), Some(pcci)) = (inputs.cci, ctx.prev.cci) {
            if pcci <= 0.0 && cci > 0.0 {
                if let Some(e) = out.get_mut("cci") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "CCI_ZERO_CROSS_BULLISH",
                    ));
                }
            } else if pcci >= 0.0 && cci < 0.0 {
                if let Some(e) = out.get_mut("cci") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "CCI_ZERO_CROSS_BEARISH",
                    ));
                }
            }
        }

        // ── MACD zero-line cross (ZeroLineCross). ──
        if let (Some(ml), Some(pml)) = (inputs.macd_line, ctx.prev.macd_line) {
            if pml <= 0.0 && ml > 0.0 {
                if let Some(e) = out.get_mut("macd") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bullish,
                        SignalStatus::Active,
                        "MACD_ZERO_CROSS_BULLISH",
                    ));
                }
            } else if pml >= 0.0 && ml < 0.0 {
                if let Some(e) = out.get_mut("macd") {
                    e.signals.push(IndicatorSignal::new(
                        SignalKind::ZeroLineCross,
                        SignalDirection::Bearish,
                        SignalStatus::Active,
                        "MACD_ZERO_CROSS_BEARISH",
                    ));
                }
            }
        }

        // Derive state-based discrete signals (threshold/breakout/etc.) from
        // each indicator's current label. Also surface the primary RSI/MACD
        // divergence as a signal on their parent oscillators.
        if let Some(v) = super::signals::divergence_entry(&mut out, "rsi", inputs.rsi_divergence) {
            let _ = v; // rsi_divergence scored entry already added above.
        }
        if let Some(v) = super::signals::divergence_entry(&mut out, "macd", inputs.macd_divergence)
        {
            let _ = v;
        }
        super::signals::derive_signals(&mut out);

        // ── WARMING fill for every registered indicator key ──
        // Every configured indicator MUST appear in the output map so the
        // frontend can distinguish "warming up (not yet available)" from
        // "indicator not configured."  Indicators that haven't produced a
        // real value yet (warm-up period, pivot detection, session boundary)
        // receive a WARMING placeholder with confidence 0.0.  As soon as
        // real data arrives the placeholder is overwritten via HashMap::insert.
        // The confluence engine and signal derivers skip entries whose
        // state_label matches "WARMING" — these placeholders never influence
        // scoring, MTF alignment, or trade decisions.
        //
        // On shadow ticks we skip the WARMING fill entirely for close-only
        // indicators (registry `updates_on_shadow = false`). The previous
        // behavior inserted a zero-valued `WARMING` placeholder every
        // shadow tick, which the frontend's per-key merge then promoted to
        // "real" — erasing the last completed-candle reading for
        // Hull MA, Ichimoku, Anchored VWAP, CCI, PSAR, Williams %R, AO,
        // Force Index, StdDev Channel, and every other close-only entry
        // (the regression that produced `Raw 0.0 / Norm 0.0 / State
        // UNKNOWN` rows in the Metrics Indicators table).
        //
        // For non-`CandleBased` indicators (OrderBook / DerivativesWs /
        // EventDriven), we also skip the WARMING fill on the completed
        // path. These indicators have a different contract: an entry only
        // exists when an event was detected (EventDriven) or a WS message
        // arrived (OrderBook / DerivativesWs). Emitting a `raw_value = 0.0`
        // placeholder for them was a regression that surfaced as
        // `Raw 0.00 / Norm 0.00 / State UNKNOWN` rows for SMC and
        // derivatives in the Metrics Indicators table — the WARMING
        // placeholder was rendering as if those indicators had real readings
        // when in fact they had no events / no WS data yet.
        for meta in crate::indicators::registry::INDICATORS {
            if shadow && !meta.updates_on_shadow {
                continue;
            }
            let skip_for_data_source = !matches!(
                meta.data_source.unwrap_or_default(),
                crate::indicators::registry::IndicatorDataSource::CandleBased
            );
            if skip_for_data_source {
                continue;
            }
            if !out.contains_key(meta.key) {
                out.insert(
                    meta.key.into(),
                    NormalizedIndicatorValue::scalar(0.0, 0.0, "WARMING")
                        .with_readiness(Readiness::Dashed),
                );
            }
        }

        out
    }
}

/// Map a divergence classification to a dedicated normalized confluence value.
/// Confirmed divergences map to ±1.0, potential to ±0.5, none is omitted.
#[allow(dead_code)]
fn divergence_value(state: DivergenceState) -> Option<NormalizedIndicatorValue> {
    let (norm, label) = match state {
        DivergenceState::ConfirmedBullish => (1.0, "CONFIRMED_BULLISH_DIVERGENCE"),
        DivergenceState::PotentialBullish => (0.5, "POTENTIAL_BULLISH_DIVERGENCE"),
        DivergenceState::ConfirmedBearish => (-1.0, "CONFIRMED_BEARISH_DIVERGENCE"),
        DivergenceState::PotentialBearish => (-0.5, "POTENTIAL_BEARISH_DIVERGENCE"),
        DivergenceState::None => return None,
    };
    Some(NormalizedIndicatorValue::scalar(norm, norm, label))
}
