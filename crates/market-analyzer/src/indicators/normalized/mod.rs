//! # Fractional Normalized Indicator Model (v2.0)
//!
//! Centralized, stateless normalization layer. The low-level indicator
//! calculators under `crates/market-analyzer/src/indicators/` remain pure math
//! producers of raw metrics. This module maps those raw metrics — together
//! with a [`NormalizationContext`] carrying directional/positional state —
//! into a unified `[-1.0, 1.0]` scale paired with a context-aware label.
//!
//! `-1.0` = absolute bearish conviction, `0.0` = equilibrium/compression,
//! `+1.0` = absolute bullish conviction.
//!
//! Self-contained mappers (RSI/MACD/Squeeze) live here; context-dependent
//! mappers live in [`context`]; the [`NormalizationEngine::normalize_all`]
//! consolidation lives in [`all`].

mod all;
mod context;
mod extended;
mod signals;

pub use all::IndicatorInputs;
pub use signals::derive_signals;

// The normalized indicator DTOs (NormalizedIndicatorValue, IndicatorSignal,
// SignalKind, SignalDirection, SignalStatus, SignalPoint, DivergenceState,
// clamp_unit) are defined in `core-domain::indicator_dtos` and re-exported
// here so the rest of the workspace can refer to them via either path.
pub use core_domain::indicator_dtos::{
    clamp_unit, DivergenceState, IndicatorSignal, NormalizedIndicatorValue, SignalDirection,
    SignalKind, SignalPoint, SignalStatus,
};

use super::squeeze::MomentumDirection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Discrete signal kind an indicator can emit. Capabilities are declared in the
/// registry (`signal_types`); occurrences are recorded per snapshot in
/// [`NormalizedIndicatorValue::signals`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviousBarState {
    pub rsi: Option<f64>,
    pub stoch_k: Option<f64>,
    pub stoch_d: Option<f64>,
    pub cmf: Option<f64>,
    pub chandemo: Option<f64>,
    pub aroon_up: Option<f64>,
    pub aroon_down: Option<f64>,
    pub macd_line: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub linreg_slope: Option<f64>,
    pub zscore: Option<f64>,
    pub obv: Option<f64>,
    pub obv_sma: Option<f64>,
    pub mfi: Option<f64>,
    pub adx_plus_di: Option<f64>,
    pub adx_minus_di: Option<f64>,
    pub price: Option<f64>,
    pub ema_fast: Option<f64>,
    pub ema_medium: Option<f64>,
    /// Previous EMA slow / long — carried for the StackChange transition
    /// detector (AUDIT-AIU-030), which must know the full 4-line ribbon
    /// ordering of the previous bar.
    pub ema_slow: Option<f64>,
    pub ema_long: Option<f64>,
    pub supertrend_line: Option<f64>,
    // ── Deferred-indicator transition state (pivots / ichimoku) ──
    /// Signed position vs the nearest active pivot level: +1 above pivot,
    /// -1 below pivot, 0 unknown — used to detect pivot crossovers.
    pub pivot_active_level: Option<f64>,
    /// Previous Ichimoku Tenkan-sen (conversion line) for TK crossover.
    pub ichimoku_tenkan: Option<f64>,
    /// Previous Ichimoku Kijun-sen (base line) for TK crossover.
    pub ichimoku_kijun: Option<f64>,
    /// Previous price-vs-cloud position: +1 above cloud, -1 below, 0 inside —
    /// used to detect cloud breakouts and price entering/leaving the cloud.
    pub price_vs_cloud: Option<f64>,
    /// Previous future-cloud colour sign (Senkou A − Senkou B) for twist detection.
    pub ichimoku_future_bias: Option<f64>,
    pub hull_ma: Option<f64>,
    pub awesome_oscillator: Option<f64>,
    pub force_index: Option<f64>,
    pub williams_r: Option<f64>,
    pub cci: Option<f64>,
    pub psar_sar: Option<f64>,
    // CR-10: `funding_rate` and `cascade_state` are deleted. The funding rate is
    // a venue feed value and the cascade state came from the forced-close
    // accumulator; both existed only so the deleted liquidity signals could
    // detect a flip between bars. With those signals gone the carry has no
    // reader.
}

/// Stateful context bridging the pure calculators to signed normalization.
#[derive(Debug, Clone, Default)]
pub struct NormalizationContext {
    /// Prevailing trend bias: +1 bull, -1 bear, 0 neutral.
    pub trend_bias: i8,
    /// Current reference price (typically the completed candle close).
    pub price: f64,
    /// Session VWAP, if available.
    pub vwap: Option<f64>,
    /// Active position direction: Some(+1) long, Some(-1) short, None flat.
    pub active_position: Option<i8>,
    /// EMA stack classification string ("bullish"/"bearish"/"tangled").
    pub ema_stack_state: Option<String>,
    /// Medium EMA value for dynamic support/resistance retest detection.
    pub ema_medium: Option<f64>,
    /// Fast EMA value (PRI-10, v6.10.7): the `ema_stack` entry's `raw_value`
    /// must match the registry `value_source: "sub:fast"`. The previous
    /// price-anchored raw made the Metrics Raw cell show the close and
    /// poisoned any consumer that fell back to raw (the price-following
    /// lines bug). Falls back to `price` when unavailable (cold edge).
    pub ema_fast: Option<f64>,
    /// Sorted support price levels.
    pub support_levels: Vec<f64>,
    /// Sorted resistance price levels.
    pub resistance_levels: Vec<f64>,
    /// Relative volume for breakout confirmation gates.
    pub rvol: Option<f64>,
    /// True when ADX has decelerated (negative slope) for >= 2 consecutive
    /// bars while in the extreme regime — triggers the hard hook exit. Tracked
    /// statefully by the analyzer from the historical ADX buffer.
    pub adx_consecutive_deceleration: bool,
    /// Previous completed-bar indicator values for crossover/zero-line detection.
    pub prev: PreviousBarState,
    /// RVOL institutional/climax thresholds from config (AUDIT-AIU-071).
    /// Defaults 1.5 / 3.0 match the legacy hardcoded values.
    pub rvol_institutional_threshold: f64,
    pub rvol_climax_threshold: f64,
}

// `clamp_unit` is defined in `core_domain::indicator_dtos` and re-exported above.

/// Soft saturation helper for unbounded raw magnitudes into `[0, 1)`.
#[inline]
pub(crate) fn saturate(x: f64) -> f64 {
    x.abs().tanh()
}

/// Signed-label selector helper.
#[inline]
pub(crate) fn pick(sign: f64, bull: &str, bear: &str) -> String {
    if sign > 0.0 {
        bull.to_string()
    } else {
        bear.to_string()
    }
}

/// Pure, static normalization mappers. Context-dependent mappers are
/// implemented in the [`context`] submodule; consolidation in [`all`].
pub struct NormalizationEngine;

impl NormalizationEngine {
    /// RSI (14): piecewise sigmoid-style compression with divergence override.
    pub fn normalize_rsi(rsi: f64, divergence: DivergenceState) -> NormalizedIndicatorValue {
        // Confirmed divergence hard-overrides the entire score.
        // AUDIT-AIU-033: the override previously forced the label to
        // `OVERSOLD_ACCUMULATION` / `OVERBOUGHT_DISTRIBUTION` regardless of
        // the actual RSI zone, so a confirmed bullish divergence at RSI=80
        // labeled the bar "OVERSOLD" and the deriver then emitted a spurious
        // OVERSOLD Threshold. The label now reflects the divergence itself,
        // which the deriver does not misread as a price zone.
        match divergence {
            DivergenceState::ConfirmedBullish => {
                return NormalizedIndicatorValue::scalar(rsi, 1.0, "DIVERGENCE_BULLISH_CONFIRMED")
            }
            DivergenceState::ConfirmedBearish => {
                return NormalizedIndicatorValue::scalar(rsi, -1.0, "DIVERGENCE_BEARISH_CONFIRMED")
            }
            _ => {}
        }

        // AUDIT-AIU-108 (v6.10.21): the middle-band sign was inverted —
        // `rsi 40` reported +0.35 (BULLISH_DISCOUNT) while MFI/Stochastic/
        // WilliamsR reported negative at the same reading, feeding opposite
        // directional votes into L3/L4 scoring on every snapshot. The RSI
        // middle band now follows the uniform momentum convention of the
        // other oscillators (04-02-23 "uniform RSI convention"): below the
        // 50 line is bearish (negative), above is bullish (positive). The
        // extreme zones stay contrarian (oversold → positive accumulation).
        let base = if rsi <= 30.0 {
            0.7 + ((30.0 - rsi) / 30.0) * 0.3
        } else if rsi >= 70.0 {
            -0.7 - ((rsi - 70.0) / 30.0) * 0.3
        } else if rsi <= 50.0 {
            -((50.0 - rsi) / 20.0) * 0.7
        } else {
            ((rsi - 50.0) / 20.0) * 0.7
        };

        // Potential (unconfirmed) divergence additive boost, capped at ±0.90.
        let boosted = match divergence {
            DivergenceState::PotentialBullish => (base + 0.15).min(0.90),
            DivergenceState::PotentialBearish => (base - 0.15).max(-0.90),
            _ => base,
        };

        let norm = clamp_unit(boosted);
        NormalizedIndicatorValue::scalar(rsi, norm, rsi_label(norm))
    }

    /// MACD: crossover / zero-line filtering + histogram exhaustion warnings.
    pub fn normalize_macd(
        macd_line: f64,
        signal_line: f64,
        histogram: f64,
        histogram_peak: f64,
        crossover: Option<i8>,
    ) -> NormalizedIndicatorValue {
        let (norm, label) = match crossover {
            Some(c) if c > 0 => {
                if macd_line < 0.0 {
                    (
                        0.8 + 0.2 * saturate(macd_line),
                        "BULLISH_CROSSOVER_ACCELERATING",
                    )
                } else {
                    // AUDIT-AIU-022: a zero-line-filtered crossover (FOMO)
                    // must contribute ZERO to the directional accumulator
                    // (doc 04-02-17: "A 'rejected' crossover must contribute
                    // zero… any non-zero value would let a filter-out signal
                    // leak into the directional accumulator"). The previous
                    // ±0.2 leaked a directional vote for rejected entries.
                    (0.0, "FOMO_BULLISH_CROSSOVER_REJECTED")
                }
            }
            Some(c) if c < 0 => {
                if macd_line > 0.0 {
                    (
                        -(0.8 + 0.2 * saturate(macd_line)),
                        "BEARISH_CROSSOVER_ACCELERATING",
                    )
                } else {
                    // AUDIT-AIU-022: see FOMO above — zero contribution.
                    (0.0, "PANIC_BEARISH_CROSSOVER_REJECTED")
                }
            }
            _ => {
                // No crossover — momentum regime from line/signal ordering.
                // SIG-09 (continuous piecewise): the previous flat-`±0.3`
                // "exhaustion" branches produced discontinuous step jumps in
                // the per-tick confluence sum when the histogram drifted
                // across the contraction threshold. The continuous form
                // below ramps smoothly from a near-zero baseline at full
                // contraction to the full `0.4 + 0.3 × tanh(|hist|)`
                // expansion magnitude, preserving the ADX-style continuity
                // invariant. The labels remain exhaustively populated.
                let contracting = histogram_peak.abs() > f64::EPSILON
                    && (histogram_peak.abs() - histogram.abs()) / histogram_peak.abs() >= 0.30;
                let peak_ratio = if histogram_peak.abs() > f64::EPSILON {
                    (histogram.abs() / histogram_peak.abs()).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let expand_mag = 0.4 + 0.3 * saturate(histogram);
                let base = 0.1 * (1.0 - peak_ratio); // smoothly decay to 0.1 as r → 0
                let magnitude = expand_mag * peak_ratio + base;
                if histogram.abs() < 1e-9 {
                    (0.0, "MOMENTUM_FLATLINE")
                } else if macd_line > signal_line {
                    if contracting {
                        (magnitude, "BULLISH_MOMENTUM_EXHAUSTION_WARNING")
                    } else {
                        (magnitude, "BULLISH_MOMENTUM_EXPANDING")
                    }
                } else if contracting {
                    (-magnitude, "BEARISH_MOMENTUM_EXHAUSTION_WARNING")
                } else {
                    (-magnitude, "BEARISH_MOMENTUM_EXPANDING")
                }
            }
        };

        let mut values = HashMap::new();
        values.insert("line".to_string(), macd_line);
        values.insert("signal".to_string(), signal_line);
        values.insert("histogram".to_string(), histogram);
        values.insert("histogram_peak".to_string(), histogram_peak);
        NormalizedIndicatorValue::with_values(histogram, clamp_unit(norm), label, values)
    }

    /// Squeeze Momentum: coiling compression + 4-phase release direction.
    pub fn normalize_squeeze(
        squeeze_on: bool,
        release_trigger: bool,
        momentum_value: f64,
        direction: MomentumDirection,
    ) -> NormalizedIndicatorValue {
        if squeeze_on {
            return NormalizedIndicatorValue::scalar(momentum_value, 0.0, "COMPRESSION_COILING");
        }
        if release_trigger {
            return if momentum_value >= 0.0 {
                NormalizedIndicatorValue::scalar(momentum_value, 1.0, "BULLISH_VOLATILITY_RELEASE")
            } else {
                NormalizedIndicatorValue::scalar(momentum_value, -1.0, "BEARISH_VOLATILITY_RELEASE")
            };
        }
        let (norm, label) = match direction {
            MomentumDirection::BullishAcceleration => (
                0.5 + 0.4 * saturate(momentum_value),
                "BULLISH_EXPANSION_ACCELERATING",
            ),
            MomentumDirection::BullishDeceleration => (0.2, "BULLISH_MOMENTUM_EXHAUSTING"),
            MomentumDirection::BearishAcceleration => (
                -(0.5 + 0.4 * saturate(momentum_value)),
                "BEARISH_EXPANSION_ACCELERATING",
            ),
            MomentumDirection::BearishDeceleration => (-0.2, "BEARISH_MOMENTUM_EXHAUSTING"),
            MomentumDirection::Flat => (0.0, "COMPRESSION_COILING"),
        };
        NormalizedIndicatorValue::scalar(momentum_value, clamp_unit(norm), label)
    }
}

/// RSI level label derived from its normalized score band.
fn rsi_label(norm: f64) -> &'static str {
    if norm >= 0.70 {
        "OVERSOLD_ACCUMULATION"
    } else if norm >= 0.10 {
        "BULLISH_MOMENTUM"
    } else if norm > -0.10 {
        "EQUILIBRIUM"
    } else if norm > -0.70 {
        "BEARISH_MOMENTUM"
    } else {
        "OVERBOUGHT_DISTRIBUTION"
    }
}

#[cfg(test)]
mod meta_tests {
    use super::*;

    #[test]
    fn readiness_defaults_to_solid_for_produced_values() {
        let v = NormalizedIndicatorValue::scalar(0.0, -0.8, "X");
        assert_eq!(v.readiness, core_domain::axes::Readiness::Solid);
        let n = NormalizedIndicatorValue::neutral("N");
        assert_eq!(n.readiness, core_domain::axes::Readiness::Solid);
    }

    #[test]
    fn readiness_is_declared_mechanically_not_computed() {
        let v = NormalizedIndicatorValue::scalar(0.0, 0.2, "X")
            .with_readiness(core_domain::axes::Readiness::Dashed);
        assert_eq!(v.readiness, core_domain::axes::Readiness::Dashed);
        // Readiness has no magnitude to clamp or scale.
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["readiness"], "dashed");
        assert!(json.get("confidence").is_none());
    }

    #[test]
    fn signal_carries_age_and_builders() {
        let s = IndicatorSignal::new(
            SignalKind::Divergence,
            SignalDirection::Bullish,
            SignalStatus::Confirmed,
            "CONFIRMED_BULLISH_DIVERGENCE",
        )
        .with_strength(0.9);
        assert_eq!(s.age_bars, 0);
        assert!((s.strength - 0.9).abs() < 1e-9);
        assert_eq!(s.direction, SignalDirection::Bullish);
    }

    #[test]
    fn derive_signals_do_not_invent_conviction() {
        let inputs = IndicatorInputs {
            rsi: Some(15.0), // deep oversold → strong normalized + OB/OS threshold signal
            ..IndicatorInputs::default()
        };
        let ctx = NormalizationContext::default();
        let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        let rsi = map.get("rsi").expect("rsi present");
        assert!(
            !rsi.signals.is_empty(),
            "oversold RSI should emit a threshold signal"
        );
        assert_eq!(
            rsi.readiness,
            core_domain::axes::Readiness::Solid,
            "a fired signal says the inputs are complete; it does not manufacture a probability"
        );
    }

    #[test]
    fn warming_fill_populates_all_registered_indicators() {
        let inputs = IndicatorInputs::default();
        let ctx = NormalizationContext::default();
        let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        // v6.10 (Phase 5 / E5): `support_resistance` is now always
        // inserted with a meaningful state_label (STRUCTURE_NEUTRAL when
        // no levels are present), so the operator can distinguish
        // "tracker warming up" from "no S/R detected in this regime".
        // Only fibonacci and patterns remain EventDriven with WARMING.
        let event_driven = ["fibonacci", "patterns"];
        let divergent_keys = ["fibonacci", "patterns", "support_resistance"];
        let divergence_keys = [
            "rsi_divergence",
            "macd_divergence",
            "stochastic_divergence",
            "chandemo_divergence",
            "mfi_divergence",
            "cmf_divergence",
            "obv_divergence",
            "squeeze_divergence",
        ];
        for &key in &event_driven {
            let v = map
                .get(key)
                .unwrap_or_else(|| panic!("{key} must be present"));
            assert_eq!(v.state_label, "WARMING", "{key} should be WARMING");
            assert_eq!(
                v.readiness,
                core_domain::axes::Readiness::Dashed,
                "{key} has no inputs yet and must say so"
            );
        }
        for &key in &divergent_keys {
            let v = map
                .get(key)
                .unwrap_or_else(|| panic!("{key} must be present"));
            // support_resistance always emits a real state_label
            // (STRUCTURE_NEUTRAL when the SrRoleTracker has no levels
            // yet), so it is no longer treated as EventDriven-WARMING.
            if key == "support_resistance" {
                assert_eq!(
                    v.state_label, "STRUCTURE_NEUTRAL",
                    "support_resistance must report STRUCTURE_NEUTRAL when levels empty"
                );
            } else {
                assert_eq!(v.state_label, "WARMING", "{key} should be WARMING");
            }
        }
        for &key in &divergence_keys {
            assert!(!map.contains_key(key), "{key} mirror should NOT be present");
        }
    }

    #[test]
    fn warming_fill_covers_all_registry_keys() {
        // The WARMING fill is now suppressed for non-CandleBased indicators
        // (`OrderBook`, `DerivativesWs`, `EventDriven`). For SMC and
        // derivatives this is the fix for the regression that surfaced as
        // `Raw 0.00 / Norm 0.00 / State UNKNOWN` rows in the Metrics
        // Indicators table: the WARMING placeholder was rendering as if a
        // real reading existed when no event had been detected (SMC) or no
        // WS message had arrived (derivatives / order book).
        let inputs = IndicatorInputs::default();
        let ctx = NormalizationContext::default();
        let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        let non_directional = ["atr", "bbwp", "hv", "rvol", "choppiness"];
        for key in non_directional {
            let v = map
                .get(key)
                .unwrap_or_else(|| panic!("{key} gate must be present"));
            assert_eq!(v.state_label, "WARMING", "{key} should be WARMING");
        }
        for meta in crate::indicators::registry::INDICATORS {
            use crate::indicators::registry::IndicatorDataSource;
            let skip_for_data_source = !matches!(
                meta.data_source.unwrap_or_default(),
                IndicatorDataSource::CandleBased
            );
            if skip_for_data_source {
                assert!(
                    !map.contains_key(meta.key),
                    "{} (non-CandleBased) must NOT receive a WARMING placeholder",
                    meta.key
                );
            } else {
                assert!(
                    map.contains_key(meta.key),
                    "{} (CandleBased registry key) must be present",
                    meta.key
                );
            }
        }
    }

    #[test]
    fn shadow_path_skips_close_only_indicators() {
        // Regression: the previous shadow-tick path emitted a zero-valued
        // `WARMING` placeholder for every close-only indicator. The frontend
        // per-key merge then promoted the placeholder to a "real" reading
        // and wiped the last completed-candle value for Hull MA, Ichimoku,
        // Anchored VWAP, CCI, PSAR, Williams %R, AO, Force Index, StdDev
        // Channel, etc. — surfacing as `Raw 0.0 / Norm 0.0 / State UNKNOWN`
        // rows in the Metrics Indicators table.
        let inputs = IndicatorInputs::default();
        let ctx = NormalizationContext::default();
        let shadow_map = NormalizationEngine::normalize_all(&inputs, &ctx, true);
        let completed_map = NormalizationEngine::normalize_all(&inputs, &ctx, false);

        // Tick-safe indicators (RSI/MACD/EMA) keep their WARMING fill on
        // shadow ticks so the UI can still render the "Loading" badge.
        for tick_safe in ["rsi", "macd", "ema_stack", "vwap", "atr"] {
            assert!(
                shadow_map.contains_key(tick_safe),
                "{tick_safe} must remain in the shadow map (tick-safe indicator)"
            );
            assert!(
                completed_map.contains_key(tick_safe),
                "{tick_safe} must be in the completed map"
            );
        }

        // Close-only indicators MUST be absent from the shadow map so the
        // frontend per-key merge preserves the last completed-candle value.
        for close_only in [
            "hull_ma",
            "ichimoku",
            "anchored_vwap",
            "cci",
            "psar",
            "williams_r",
            "awesome_oscillator",
            "force_index",
            "stddev_channel",
            "fibonacci",
            "support_resistance",
            "pivot_points",
            "patterns",
            "candlestick",
            "volume_profile",
        ] {
            assert!(
                !shadow_map.contains_key(close_only),
                "{close_only} must NOT be in the shadow map (close-only indicator — frontend per-key merge must preserve last completed value)"
            );
            assert!(
                completed_map.contains_key(close_only),
                "{close_only} must still appear in the completed map (WARMING placeholder)"
            );
        }

        // Event-driven SMC indicators MUST be absent from BOTH the shadow
        // and completed maps when no event has fired yet. The WARMING fill
        // is suppressed for them — emitting a `raw_value = 0.0` placeholder
        // would surface as a misleading `Raw 0.00 / Norm 0.00` row in the
        // Metrics Indicators table until the first BOS / CHoCH / sweep /
        // FVG / OB is detected. The lifecycle builder (Loading state) plus
        // the UI (--/--/Warming) cover the "no event yet" case correctly.
        for event_driven in [
            "smc_structure",
            "smc_liquidity",
            "smc_fvg",
            "smc_order_blocks",
        ] {
            assert!(
                !shadow_map.contains_key(event_driven),
                "{event_driven} must NOT be in the shadow map (event-driven — no WARMING placeholder)"
            );
            assert!(
                !completed_map.contains_key(event_driven),
                "{event_driven} must NOT be in the completed map when no event has fired (event-driven — no WARMING placeholder)"
            );
        }
    }

    /// Regression: derivatives (DerivativesWs / OrderBook) indicators must
    /// also skip the WARMING fill on the completed path. They are
    /// WS-driven: an entry only exists when the upstream WS / REST poller
    /// has produced data. Emitting a `raw_value = 0.0` placeholder for them
    /// was the root cause of the `Raw 0.0 / Norm 0.0 / State UNKNOWN`
    /// rows the user reported for all derivatives indicators in the
    /// Metrics Indicators table.
    #[test]
    fn derivatives_skip_warming_fill_on_completed_path() {
        let inputs = IndicatorInputs::default();
        let ctx = NormalizationContext::default();
        let completed_map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        for derivative in [
            "open_interest",
            "oi_delta",
            "funding_rate",
            "oi_price_divergence",
            "order_flow_imbalance",
            "spread",
            "depth_bias",
        ] {
            assert!(
                !completed_map.contains_key(derivative),
                "{derivative} must NOT receive a WARMING placeholder when no WS data has arrived (WS-driven indicator)"
            );
        }
    }

    /// Sanity check that the new `EventDriven` variant of `IndicatorDataSource`
    /// is wired correctly into the registry: every SMC indicator must
    /// declare `data_source = Some(EventDriven)` so the WARMING fill gate
    /// recognises it.
    #[test]
    fn smc_indicators_are_tagged_event_driven() {
        use crate::indicators::registry::IndicatorDataSource;
        for key in [
            "smc_structure",
            "smc_liquidity",
            "smc_fvg",
            "smc_order_blocks",
        ] {
            let meta = crate::indicators::registry::get(key)
                .unwrap_or_else(|| panic!("{key} must be in the registry"));
            assert_eq!(
                meta.data_source,
                Some(IndicatorDataSource::EventDriven),
                "{key} must declare data_source = Some(EventDriven)"
            );
        }
    }
}
