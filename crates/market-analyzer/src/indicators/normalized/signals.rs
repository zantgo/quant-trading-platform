//! Signal derivation: converts the current normalized indicator map into
//! discrete `IndicatorSignal`s (badges/markers) plus divergence scored entries.
//!
//! Signals detectable from the current snapshot's `state_label` are emitted with
//! `Active` status (threshold/breakout/compression/pattern). Divergence signals
//! come from the generalized detectors passed via `IndicatorInputs`.

use super::{
    DivergenceState, IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind,
    SignalStatus,
};
use std::collections::HashMap;

type Map = HashMap<String, NormalizedIndicatorValue>;

/// Append a signal to the entry for `key`, deduplicating on the
/// `(label, kind)` pair so a parent indicator can never accumulate two
/// signals that would collide on the frontend's
/// `{#each ... (label + kind)}` keying (which would otherwise throw
/// `each_key_duplicate` in Svelte 5). Returns `true` when the signal
/// was actually appended, `false` when the parent was missing or the
/// pair was already present.
fn push_signal(map: &mut Map, key: &str, sig: IndicatorSignal) -> bool {
    if let Some(entry) = map.get_mut(key) {
        let already_present = entry
            .signals
            .iter()
            .any(|existing| existing.label == sig.label && existing.kind == sig.kind);
        if !already_present {
            entry.signals.push(sig);
            return true;
        }
    }
    false
}

fn threshold(dir: SignalDirection, label: &str) -> IndicatorSignal {
    IndicatorSignal::new(SignalKind::Threshold, dir, SignalStatus::Active, label)
}

/// Emit a `Divergence` signal on the parent oscillator and return the paired
/// scored-key `NormalizedIndicatorValue` (±1 confirmed / ±0.5 potential).
pub(super) fn divergence_entry(
    map: &mut Map,
    parent: &str,
    state: DivergenceState,
) -> Option<NormalizedIndicatorValue> {
    let (norm, dir, status, label) = match state {
        DivergenceState::ConfirmedBullish => (
            1.0_f64,
            SignalDirection::Bullish,
            SignalStatus::Confirmed,
            "CONFIRMED_BULLISH_DIVERGENCE",
        ),
        DivergenceState::PotentialBullish => (
            0.5_f64,
            SignalDirection::Bullish,
            SignalStatus::Potential,
            "POTENTIAL_BULLISH_DIVERGENCE",
        ),
        DivergenceState::ConfirmedBearish => (
            -1.0_f64,
            SignalDirection::Bearish,
            SignalStatus::Confirmed,
            "CONFIRMED_BEARISH_DIVERGENCE",
        ),
        DivergenceState::PotentialBearish => (
            -0.5_f64,
            SignalDirection::Bearish,
            SignalStatus::Potential,
            "POTENTIAL_BEARISH_DIVERGENCE",
        ),
        DivergenceState::None => return None,
    };
    push_signal(
        map,
        parent,
        IndicatorSignal::new(SignalKind::Divergence, dir, status, label).with_strength(norm.abs()),
    );
    Some(NormalizedIndicatorValue::scalar(norm, norm, label))
}

/// Derive state-based signals (threshold/breakout/compression/pattern/level)
/// from each indicator's current `state_label`.
pub fn derive_signals(map: &mut Map) {
    // Collect labels first to avoid borrow conflicts.
    let labels: Vec<(String, String)> = map
        .iter()
        .map(|(k, v)| (k.clone(), v.state_label.clone()))
        .collect();

    for (key, label) in labels {
        let l = label.as_str();
        let mut sigs: Vec<IndicatorSignal> = Vec::new();

        // Overbought / oversold thresholds (oscillators).
        if l.contains("OVERBOUGHT") {
            sigs.push(threshold(SignalDirection::Bearish, "OVERBOUGHT"));
        } else if l.contains("OVERSOLD") {
            sigs.push(threshold(SignalDirection::Bullish, "OVERSOLD"));
        }
        // Exhaustion extremes (CMO / Z-Score).
        if l.contains("CLIMACTIC_BULL") || l.contains("OVEREXTENDED_HIGH") {
            sigs.push(threshold(SignalDirection::Bearish, l));
        } else if l.contains("CLIMACTIC_BEAR") || l.contains("OVEREXTENDED_LOW") {
            sigs.push(threshold(SignalDirection::Bullish, l));
        }
        // AUDIT-AIU-021: the ADX 2-bar deceleration hook (`*_TREND_EXHAUSTION_HOOK`)
        // previously matched no deriver branch, so the documented climactic
        // Threshold never fired on a decelerating extreme trend. It is the
        // exhaustion counterpart of CLIMACTIC: a decelerating BULL trend is a
        // bearish warning, and vice versa.
        if l.contains("EXHAUSTION_HOOK") {
            if l.contains("BULL") {
                sigs.push(threshold(SignalDirection::Bearish, l));
            } else {
                sigs.push(threshold(SignalDirection::Bullish, l));
            }
        }
        // Breakouts (Donchian / Keltner / Bollinger).
        if l.contains("UPPER_BREAKOUT") {
            sigs.push(IndicatorSignal::new(
                SignalKind::Breakout,
                SignalDirection::Bullish,
                SignalStatus::Active,
                l,
            ));
        } else if l.contains("LOWER_BREAKOUT") {
            sigs.push(IndicatorSignal::new(
                SignalKind::Breakout,
                SignalDirection::Bearish,
                SignalStatus::Active,
                l,
            ));
        }
        // Volatility compression / release (Squeeze / BBWP).
        if l.contains("VOLATILITY_RELEASE") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::CompressionRelease,
                d,
                SignalStatus::Active,
                l,
            ));
        } else if l == "COMPRESSION_COILING" || l.contains("MAX_VOLATILITY_COMPRESSION") {
            sigs.push(IndicatorSignal::new(
                SignalKind::CompressionRelease,
                SignalDirection::Neutral,
                SignalStatus::Active,
                l,
            ));
        } else if l.contains("VOLATILITY_EXHAUSTION_REVERSION_WARNING") {
            // AUDIT-AIU-023: BBWP > 90 (volatility exhaustion) is documented
            // as a Threshold (04-02-27 §5) but previously no deriver branch
            // matched this label, so the >90 regime emitted no signal at all.
            // Non-directional gate: the reversion warning is Neutral.
            sigs.push(IndicatorSignal::new(
                SignalKind::Threshold,
                SignalDirection::Neutral,
                SignalStatus::Active,
                l,
            ));
        }
        // Chart pattern forming.
        // Gated on `key == "patterns"` so a future label on another indicator
        // that happens to contain the substring "PATTERN" cannot accidentally
        // emit a `PatternForming` signal here (every other derive branch uses
        // the same `key == "X"` gate — see ema_stack, aroon, choppiness, hv,
        // supertrend, psar, adx, stochastic, obv, volume_profile, pivot_points,
        // smc_*, anchored_vwap). The `l != "NO_PATTERN"` guard avoids emitting
        // a "PatternForming" signal when the calculator ran but found nothing
        // (this is the common steady-state for the patterns indicator — emitting
        // a signal there would inflate GroupConfluenceGrid's signal count and
        // surface a misleading "PatternForming" entry in the Signals view).
        if key == "patterns" && l.contains("PATTERN") && l != "NO_PATTERN" {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else if l.contains("BEARISH") {
                SignalDirection::Bearish
            } else {
                SignalDirection::Neutral
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::PatternForming,
                d,
                SignalStatus::Active,
                l,
            ));
        }
        // Structural level tests (Fibonacci GP / S-R zones / VWAP reversion).
        if l.contains("GOLDEN_POCKET")
            || l.contains("DEMAND_ZONE")
            || l.contains("SUPPLY_ZONE")
            || l.contains("REVERSION_ZONE")
        {
            let d = if l.contains("BULLISH") || l.contains("DEMAND") || l.contains("DISCOUNT") {
                SignalDirection::Bullish
            } else if l.contains("BEARISH") || l.contains("SUPPLY") || l.contains("PREMIUM") {
                SignalDirection::Bearish
            } else {
                SignalDirection::Neutral
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::LevelTest,
                d,
                SignalStatus::Active,
                l,
            ));
        }
        // S/R flip confirmed → Breakout.
        if l.contains("FLIP_CONFIRMED") {
            let d = if l.contains("RESISTANCE_FLIP") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::Breakout,
                d,
                SignalStatus::Active,
                l,
            ));
        }
        // Volume climax.
        if l.contains("CLIMAX") && (key == "rvol" || key == "volume") {
            sigs.push(IndicatorSignal::new(
                SignalKind::VolumeClimax,
                SignalDirection::Neutral,
                SignalStatus::Active,
                l,
            ));
        }

        // ── Extended patterns (indicators whose labels never matched before) ──

        // MACD crossover: REMOVED here (AUDIT-AIU-112). The structured
        // Crossover signal is emitted by the dedicated block in
        // `normalize_all` (all.rs), which dedupes to exactly one event per
        // bar AND suppresses zero-line-rejected crossovers (FOMO bullish
        // above / PANIC bearish below the zero line). The legacy
        // label-string branch below matched ANY label containing
        // "CROSSOVER" — including `FOMO_BULLISH_CROSSOVER_REJECTED` /
        // `PANIC_BEARISH_CROSSOVER_REJECTED` (re-emitting the very event
        // the zero-line filter was built to suppress) and
        // `BULLISH_CROSSOVER_ACCELERATING` (double-emitting a second,
        // differently-labelled Crossover on legitimate bars).

        // EMA ribbon stack alignment / retest: REMOVED here (AUDIT-AIU-030).
        // StackChange is a transition-only momentary signal per 05-02-11 §4;
        // it is now emitted by the structured block in
        // `normalize_all` (all.rs), gated on a ribbon-order change between
        // the previous and current bar. The previous stateless branch below
        // fired on every bar whose label contained "STACK" (i.e. every bar
        // while stacked or tangled), spamming persistent Active signals.

        // Aroon trend strength (primary directional signal).
        if key == "aroon" {
            if l.contains("UPTREND") {
                sigs.push(threshold(SignalDirection::Bullish, l));
            } else if l.contains("DOWNTREND") {
                sigs.push(threshold(SignalDirection::Bearish, l));
            }
        }

        // Choppiness regime classification (trending vs range).
        if key == "choppiness"
            && (l.contains("CONSOLIDATION")
                || l.contains("STRONG_TREND")
                || l.contains("TRANSITIONAL"))
        {
            sigs.push(threshold(SignalDirection::Neutral, l));
        }

        // Squeeze expansion-phase thresholds (AUDIT-AIU-035): the doc
        // (04-02-28 §4) declares `BULLISH/BEARISH_EXPANSION_ACCELERATING`
        // and `BULLISH/BEARISH_MOMENTUM_EXHAUSTING` Thresholds, but no
        // branch matched these labels so they never fired.
        if key == "squeeze"
            && (l.contains("EXPANSION_ACCELERATING") || l.contains("MOMENTUM_EXHAUSTING"))
        {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(threshold(d, l));
        }

        // HV extreme volatility (regime outlier warning).
        if key == "hv" && l.contains("EXTREME") {
            sigs.push(threshold(SignalDirection::Neutral, l));
        }

        // RSI / Stochastic / MFI / ChandeMO directional momentum bias (non-extreme).
        // AUDIT-AIU-032: the chandemo labels are `EMERGING_BULL_MOMENTUM` /
        // `EMERGING_BEAR_MOMENTUM` (no "BULLISH_MOMENTUM" substring) and
        // stochastic uses `BULLISH_MOMENTUM_ALIGNMENT` — the patterns below
        // cover every label form. (AUDIT-AIU-108: RSI's bias labels are now
        // `BULLISH_MOMENTUM` / `BEARISH_MOMENTUM` — matched by the first two
        // patterns; the legacy `BULLISH_DISCOUNT` / `BEARISH_PREMIUM` label
        // forms were retired in the same change.)
        const MOM_KEYS: &[&str] = &["rsi", "stochastic", "chandemo", "mfi"];
        if MOM_KEYS.contains(&key.as_str()) {
            if l.contains("BULLISH_MOMENTUM")
                || l.contains("BULL_MOMENTUM")
                || (l.contains("_BULLISH_") && l.contains("BIAS"))
            {
                sigs.push(threshold(SignalDirection::Bullish, l));
            } else if l.contains("BEARISH_MOMENTUM")
                || l.contains("BEAR_MOMENTUM")
                || (l.contains("_BEARISH_") && l.contains("BIAS"))
            {
                sigs.push(threshold(SignalDirection::Bearish, l));
            }
        }

        // CCI: REMOVED here (AUDIT-AIU-034). The generic overbought/oversold
        // branch (above) and the climactic-exhaustion branch already fire for
        // CCI labels (`OVERBOUGHT`/`OVERSOLD`/`CLIMACTIC_*`), so this
        // keyed branch double-emitted a second Threshold on the same bar
        // (e.g. `OVERBOUGHT` + `CCI_OVERBOUGHT`).

        // ── ZeroLineCross across oscillators ──
        const ZERO_CROSS_KEYS: &[&str] = &[
            "cmf",
            "chandemo",
            "rsi",
            "macd",
            "linreg_slope",
            "zscore",
            "cci",
            "williams_r",
            "awesome_oscillator",
            "force_index",
        ];
        if ZERO_CROSS_KEYS.contains(&key.as_str()) && l.contains("ZERO_CROSS") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else if l.contains("BEARISH") {
                SignalDirection::Bearish
            } else {
                SignalDirection::Neutral
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::ZeroLineCross,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── BandTouch for channel indicators ──
        const BAND_KEYS: &[&str] = &["donchian", "keltner", "bollinger", "stddev_channel"];
        if BAND_KEYS.contains(&key.as_str()) && l.contains("BAND_TOUCH") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else if l.contains("BEARISH") {
                SignalDirection::Bearish
            } else {
                SignalDirection::Neutral
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::BandTouch,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── Supertrend TrendFlip (from structured flipped field, surfaced via label) ──
        if key == "supertrend" && l.contains("FLIP") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── PSAR TrendFlip ──
        if key == "psar" && l.contains("FLIP") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── ADX DI Crossover TrendFlip ──
        if key == "adx" && l.contains("DI_CROSSOVER") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── Aroon Crossover (SIG-02 — reclassified to TrendFlip) ──
        // The structured push from `normalize_all` already emits Aroon's
        // Up/Down crossing as a TrendFlip signal (see all.rs). This block
        // was the legacy Crossover emission path; it is now a defensive
        // no-op alias that mirrors the structured push semantics in case
        // the label convention ever drifts. Crossover emissions for the
        // Aroon key are forbidden — see [04-02-36-aroon.md §4].
        if key == "aroon" && l.contains("CROSSOVER") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── Stochastic K/D Crossover ──
        if key == "stochastic" && l.contains("CROSSOVER") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::Crossover,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── OBV TrendFlip ──
        if key == "obv" && l.contains("TREND_FLIP") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else {
                SignalDirection::Bearish
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                l,
            ));
        }

        // ── Choppiness CompressionRelease (distinct from Threshold): a tight
        // consolidation range is coiled energy that precedes a volatility
        // release, so it emits CompressionRelease in addition to Threshold. ──
        if key == "choppiness" && (l.contains("COIL") || l.contains("CONSOLIDATION_RANGE")) {
            sigs.push(IndicatorSignal::new(
                SignalKind::CompressionRelease,
                SignalDirection::Neutral,
                SignalStatus::Active,
                l,
            ));
        }

        // Volume Profile breakout / POC retest / value rejection + TrendFlip.
        if key == "volume_profile" {
            if l.contains("BREAKOUT_ABOVE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::Breakout,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    l,
                ));
                sigs.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    "VP_TRENDFLIP_BULLISH",
                ));
            } else if l.contains("BREAKOUT_BELOW") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::Breakout,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    l,
                ));
                sigs.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    "VP_TRENDFLIP_BEARISH",
                ));
            } else if l.contains("POC_SUPPORT") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("POC_RESISTANCE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    l,
                ));
            }
        }

        // ── Pivot Points level test (support/resistance proximity) + Breakout. ──
        if key == "pivot_points" {
            if l.contains("SUPPORT_TEST") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("RESISTANCE_TEST") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("CENTRAL_TEST") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Neutral,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("BREAKOUT") || l.contains("FLIP") {
                let d = if l.contains("BULLISH") || l.contains("ABOVE") {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                };
                sigs.push(IndicatorSignal::new(
                    SignalKind::Breakout,
                    d,
                    SignalStatus::Active,
                    l,
                ));
            }
        }

        // ── SMC Structure: BOS→Breakout, CHoCH→TrendFlip. ──
        if key == "smc_structure" {
            if l.contains("BOS") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::Breakout,
                    if l.contains("BULLISH") {
                        SignalDirection::Bullish
                    } else {
                        SignalDirection::Bearish
                    },
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("CHOCH") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::TrendFlip,
                    if l.contains("BULLISH") {
                        SignalDirection::Bullish
                    } else {
                        SignalDirection::Bearish
                    },
                    SignalStatus::Active,
                    l,
                ));
            }
        }

        // ── SMC Liquidity: sweep→PatternForming + Threshold. ──
        if key == "smc_liquidity" {
            if l.contains("BUY_SWEEP") || l.contains("SELL_SWEEP") {
                let d = if l.contains("BUY") {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                };
                sigs.push(IndicatorSignal::new(
                    SignalKind::PatternForming,
                    d,
                    SignalStatus::Active,
                    l,
                ));
                sigs.push(IndicatorSignal::new(
                    SignalKind::Threshold,
                    d,
                    SignalStatus::Active,
                    "SMC_LIQUIDITY_SWEEP",
                ));
            }
            // AUDIT-AIU-049: `SMC_LIQUIDITY_BOTH_SWEEPS` previously emitted
            // zero signals (no deriver branch matched the label) and scored
            // 0.0 — the strongest event (both liquidity pools taken out on
            // one bar) was silent. Without sweep sizes the net direction is
            // genuinely balanced, so both signals are Neutral.
            if l == "SMC_LIQUIDITY_BOTH_SWEEPS" {
                sigs.push(IndicatorSignal::new(
                    SignalKind::PatternForming,
                    SignalDirection::Neutral,
                    SignalStatus::Active,
                    l,
                ));
                sigs.push(IndicatorSignal::new(
                    SignalKind::Threshold,
                    SignalDirection::Neutral,
                    SignalStatus::Active,
                    "SMC_LIQUIDITY_SWEEP",
                ));
            }
        }

        // ── SMC FVG: open gap→LevelTest. ──
        if key == "smc_fvg" && (l.contains("BULLISH_OPEN") || l.contains("BEARISH_OPEN")) {
            sigs.push(IndicatorSignal::new(
                SignalKind::LevelTest,
                if l.contains("BULLISH") {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                },
                SignalStatus::Active,
                l,
            ));
        }

        // ── SMC Order Blocks: tested→LevelTest + TrendFlip. ──
        // AUDIT-AIU-114: only genuine TEST events (`SMC_OB_*_TEST` — price
        // trading within 0.5% of the block's mid) emit signals. The legacy
        // branch also fired on `SMC_OB_*_ACTIVE` — the standing steady state
        // (zone persists until price closes through it) — pushing a LevelTest
        // AND a `SMC_OB_TRENDFLIP` on EVERY completed bar for the zone's
        // lifetime, spamming TAE/confluence with a "flip" that never flips.
        if key == "smc_order_blocks" && l.contains("TEST") {
            let d = if l.contains("BULLISH") {
                SignalDirection::Bullish
            } else if l.contains("BEARISH") {
                SignalDirection::Bearish
            } else {
                SignalDirection::Neutral
            };
            sigs.push(IndicatorSignal::new(
                SignalKind::LevelTest,
                d,
                SignalStatus::Active,
                l,
            ));
            sigs.push(IndicatorSignal::new(
                SignalKind::TrendFlip,
                d,
                SignalStatus::Active,
                "SMC_OB_TRENDFLIP",
            ));
        }

        // ── Anchored VWAP level tests / crossovers ──
        if key == "anchored_vwap" {
            if l.contains("PREMIUM_ZONE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("DISCOUNT_ZONE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::LevelTest,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("ABOVE_ACTIVE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::Crossover,
                    SignalDirection::Bearish,
                    SignalStatus::Active,
                    l,
                ));
            } else if l.contains("BELOW_ACTIVE") {
                sigs.push(IndicatorSignal::new(
                    SignalKind::Crossover,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    l,
                ));
            }
        }

        if !sigs.is_empty() {
            if let Some(entry) = map.get_mut(&key) {
                for sig in sigs {
                    let already_present = entry
                        .signals
                        .iter()
                        .any(|existing| existing.label == sig.label && existing.kind == sig.kind);
                    if !already_present {
                        entry.signals.push(sig);
                    }
                }
            }
        }
    }
}
