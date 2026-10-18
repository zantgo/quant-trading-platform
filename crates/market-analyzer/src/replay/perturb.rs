//! Perturbation — the generator that asks "did anything earlier change?"
//! (P7 W7.3, revised to the OHLCV fixture the spec names)
//!
//! 01-20 §4.2 requires four futures built by altering **highs, lows, closes,
//! volumes** — and specifically swing-confirming bars and new extremes. The
//! first cut of this module held a single `Vec<f64>` and therefore could not:
//! every future altered the *close*, so a `volume-only` future was not a volume
//! future, a `new-extreme` future was invisible to a high-reading calculator, and
//! the composite was indistinguishable from the other three.
//!
//! [`Fixture`] is OHLCV. [`Futures::apply`] is the **only** implementation of a
//! future — there is no second closure copy that could drift from it — and
//! [`Futures::channels`] names, mechanically, which channel each future breaks
//! so a gate can assert the futures are independent rather than assert it in a
//! comment.
//!
//! ## The invariant every future owes
//!
//! A future writes at exactly one bar and never at or before it. A perturbation
//! that could reach backwards would make every gate vacuous — it would be able to
//! explain any observed difference without there being a bug.

/// One candle. The unit a future writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// A contiguous, deterministic candle series.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub bars: Vec<Bar>,
}

/// The OHLCV channels a future can touch.
pub mod channel {
    pub const CLOSE: u8 = 1 << 0;
    pub const HIGH: u8 = 1 << 1;
    pub const LOW: u8 = 1 << 2;
    pub const VOLUME: u8 = 1 << 3;
}

/// The smallest-fraction positive gap used to keep perturbed fixtures from
/// colliding with the baseline's own values bit-for-bit.
const MARGIN: f64 = 1.04;

impl Fixture {
    /// A deterministic, non-degenerate sawtooth candle series.
    ///
    /// No RNG anywhere in this module — CH-3 forbids sampling, and a gate whose
    /// fixture depends on a seed is a gate whose failure nobody can reproduce.
    /// The shape alternates so a 3-bar fractal has real pivots to confirm.
    pub fn sawtooth(n: usize) -> Self {
        let bars = (0..n)
            .map(|i| {
                let phase = i % 6;
                let swing = if phase < 3 {
                    phase as f64 * 0.9
                } else {
                    (5 - phase) as f64 * 0.9
                };
                let close = 100.0 + (i as f64) * 0.35 + (i % 7) as f64 * 1.1 + swing;
                Bar {
                    open: close - 0.2 + (i % 2) as f64 * 0.1,
                    high: close + 0.6 + (i % 3) as f64 * 0.15,
                    low: close - 0.5 - (i % 4) as f64 * 0.11,
                    close,
                    volume: 1_000.0 + (i as f64) * 3.0 + (i % 5) as f64 * 120.0,
                }
            })
            .collect();
        Self { bars }
    }

    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    pub fn closes(&self) -> Vec<f64> {
        self.bars.iter().map(|b| b.close).collect()
    }

    pub fn highs(&self) -> Vec<f64> {
        self.bars.iter().map(|b| b.high).collect()
    }

    pub fn lows(&self) -> Vec<f64> {
        self.bars.iter().map(|b| b.low).collect()
    }

    pub fn volumes(&self) -> Vec<f64> {
        self.bars.iter().map(|b| b.volume).collect()
    }

    /// FNV-1a (64) over every channel's bytes.
    ///
    /// The report banner carries one of these per fixture (01-20 §9) so a green
    /// verdict names the data it was produced from. A digest over the whole
    /// fixture — not over the perturbed bar alone — also makes "two futures are
    /// the same fixture" a mechanical claim rather than an opinion.
    pub fn digest(&self) -> u64 {
        self.digest_of(&self.bars)
    }

    /// The digest of bars `0..=bar`. A future must not change this.
    pub fn prefix_digest(&self, bar: usize) -> u64 {
        let end = (bar + 1).min(self.bars.len());
        self.digest_of(&self.bars[..end])
    }

    fn digest_of(&self, bars: &[Bar]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        };
        feed(&(bars.len() as u64).to_le_bytes());
        for b in bars {
            for v in [b.open, b.high, b.low, b.close, b.volume] {
                feed(&v.to_le_bytes());
            }
        }
        h
    }
}

/// The four targeted futures.
///
/// Not one perturbation but four, because a single mutation can be masked: if
/// the only future exercised is a volume change, a calculator that reaches
/// forward through **price** is never caught, and CG2 passes having proved
/// nothing about price. Each future breaks a different channel, so an
/// implementation that survives all four has no path forward to the future.
///
/// The `name` on each is the phrase a gate failure will quote.
pub struct Futures;

impl Futures {
    /// **FX1** — volume only. Breaks any calculation that reads `volume`.
    pub const VOLUME_ONLY: &'static str = "volume-only";

    /// **FX2** — the swing-confirming bar only. Breaks any 3-bar fractal path,
    /// because the far-side bar is exactly what a right-edge read consumes. It
    /// moves highs and lows and nothing else: the close path is untouched, so a
    /// close-reading calculator cannot notice this future at all.
    pub const SWING_CONFIRM_ONLY: &'static str = "swing-confirm-only";

    /// **FX3** — a new extreme only. Breaks any path keyed off "this is the
    /// highest high so far", including every Donchian-style channel edge and
    /// every self-percentile rank.
    pub const NEW_EXTREME_ONLY: &'static str = "new-extreme-only";

    /// **FX4** — the composite. The catch-all: a bug that survives a future
    /// touching every channel simultaneously has genuinely reached past the
    /// perturbation.
    pub const ALL_FOUR: &'static str = "all-four";

    /// Every targeted future, as `(name, channels, mechanism)`.
    ///
    /// The mechanism string is the *reason* the channel matters, and is what a
    /// gate failure quotes.
    pub fn all() -> Vec<(&'static str, u8, &'static str)> {
        vec![
            (
                Futures::VOLUME_ONLY,
                channel::VOLUME,
                "participation and volume-derived readings",
            ),
            (
                Futures::SWING_CONFIRM_ONLY,
                channel::HIGH | channel::LOW,
                "pivot `available_at`, leg origin timing",
            ),
            (
                Futures::NEW_EXTREME_ONLY,
                channel::HIGH | channel::LOW,
                "rank distributions, ATR noise floor, scale classification",
            ),
            (
                Futures::ALL_FOUR,
                channel::CLOSE | channel::HIGH | channel::LOW | channel::VOLUME,
                "composite interaction",
            ),
        ]
    }

    /// The names of every future, for reporting.
    pub fn names() -> Vec<&'static str> {
        Futures::all().into_iter().map(|(n, _, _)| n).collect()
    }

    /// The channels `future` breaks. An unknown future breaks **nothing** and is
    /// reported by [`Futures::is_known`] rather than silently treated as a
    /// composite.
    pub fn channels(future: &str) -> u8 {
        Futures::all()
            .into_iter()
            .find(|(n, _, _)| *n == future)
            .map(|(_, c, _)| c)
            .unwrap_or(0)
    }

    pub fn is_known(future: &str) -> bool {
        Futures::channels(future) != 0
    }

    /// Apply `future` at `bar`, returning a new fixture.
    ///
    /// Returns `None` when `bar` is past the end of the fixture: an
    /// unreachable perturbation is a no-op, never a panic and never a silent
    /// no-change that a gate would read as "this future is inert".
    pub fn apply(fixture: &Fixture, bar: usize, future: &str) -> Option<Fixture> {
        let mut out = fixture.clone();
        match future {
            Futures::VOLUME_ONLY => {
                if bar >= out.bars.len() {
                    return None;
                }
                out.bars[bar].volume *= 1.37;
            }
            Futures::SWING_CONFIRM_ONLY => {
                if bar >= out.bars.len() {
                    return None;
                }
                swing_confirm(&mut out.bars, bar);
            }
            Futures::NEW_EXTREME_ONLY => {
                if bar >= out.bars.len() {
                    return None;
                }
                new_extreme(&mut out.bars, bar);
            }
            Futures::ALL_FOUR => {
                if bar >= out.bars.len() {
                    return None;
                }
                swing_confirm(&mut out.bars, bar);
                new_extreme(&mut out.bars, bar);
                let c = out.bars[bar].close;
                out.bars[bar].close = c * 1.11;
                out.bars[bar].open *= 1.11;
                out.bars[bar].volume *= 1.37;
            }
            _ => return None,
        }
        Some(out)
    }
}

/// Break the confirmation of the pivot one bar back, touching only highs/lows.
///
/// The pivot candidate sits at `bar - 1`; this bar is on its far side, and it is
/// the far side that decides whether the candidate is a pivot at all. Moving the
/// far-side extremes past the candidate turns a high into a lower high (and a low
/// into a higher low), which is exactly the reclassification a right-edge reader
/// consumes — while leaving every close untouched.
fn swing_confirm(bars: &mut [Bar], bar: usize) {
    let candidate = bar.saturating_sub(1);
    let (cand_high, cand_low) = (bars[candidate].high, bars[candidate].low);
    bars[bar].high = bars[bar].high.max(cand_high * MARGIN);
    bars[bar].low = bars[bar].low.min(cand_low / MARGIN);
}

/// Make this bar the running extreme in both directions.
fn new_extreme(bars: &mut [Bar], bar: usize) {
    let max = bars[..=bar].iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let min = bars[..=bar].iter().map(|b| b.low).fold(f64::MAX, f64::min);
    bars[bar].high = max * MARGIN;
    bars[bar].low = min / MARGIN;
}

/// The future names CG2 requires. Kept as a const so a gate cannot quietly
/// drop one.
pub const TARGETED_FUTURES: usize = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_future_leaves_every_earlier_bar_byte_identical() {
        let base = Fixture::sawtooth(40);
        for name in Futures::names() {
            let out = Futures::apply(&base, 25, name).expect("bar is in range");
            assert_eq!(
                out.prefix_digest(24),
                base.prefix_digest(24),
                "`{name}` touched the past"
            );
            assert_ne!(out.digest(), base.digest(), "`{name}` changed nothing");
        }
    }

    #[test]
    fn the_four_futures_are_four_distinct_fixtures() {
        // One shared future across all assertions would be a silent weakening:
        // if that future happened not to move the mechanism under test, CG2
        // would pass while proving nothing about it.
        let base = Fixture::sawtooth(40);
        let mut digests = Vec::new();
        for name in Futures::names() {
            let out = Futures::apply(&base, 25, name).unwrap();
            digests.push((name, out.digest()));
        }
        for (i, (na, da)) in digests.iter().enumerate() {
            for (nb, db) in digests.iter().skip(i + 1) {
                assert_ne!(da, db, "`{na}` and `{nb}` are the same future");
            }
        }
        assert_eq!(digests.len(), TARGETED_FUTURES);
    }

    #[test]
    fn only_the_composite_touches_the_close_path() {
        // A close-reading calculator must not be exercised by a volume or a
        // swing future, or the four futures collapse into one mechanism.
        for name in [Futures::VOLUME_ONLY, Futures::SWING_CONFIRM_ONLY] {
            let before = Fixture::sawtooth(40).closes();
            let after = Futures::apply(&Fixture::sawtooth(40), 25, name)
                .unwrap()
                .closes();
            assert_eq!(before, after, "`{name}` altered the close path");
        }
        let composite = Futures::apply(&Fixture::sawtooth(40), 25, Futures::ALL_FOUR)
            .unwrap()
            .closes();
        assert_ne!(composite, Fixture::sawtooth(40).closes());
    }

    #[test]
    fn volume_only_touches_nothing_but_volume() {
        let base = Fixture::sawtooth(40);
        let out = Futures::apply(&base, 25, Futures::VOLUME_ONLY).unwrap();
        assert_eq!(out.closes(), base.closes());
        assert_eq!(out.highs(), base.highs());
        assert_eq!(out.lows(), base.lows());
        assert_ne!(out.volumes(), base.volumes());
    }

    #[test]
    fn the_swing_future_touches_nothing_but_the_far_side_extremes() {
        let base = Fixture::sawtooth(40);
        let out = Futures::apply(&base, 25, Futures::SWING_CONFIRM_ONLY).unwrap();
        assert_eq!(out.closes(), base.closes());
        assert_eq!(out.volumes(), base.volumes());
        assert_eq!(
            &out.bars[..25],
            &base.bars[..25],
            "the far-side bar moved something before it"
        );
    }

    #[test]
    fn a_future_past_the_end_of_the_fixture_is_none_not_a_panic() {
        let base = Fixture::sawtooth(3);
        for name in Futures::names() {
            assert!(Futures::apply(&base, 99, name).is_none());
        }
    }

    #[test]
    fn an_unknown_future_breaks_nothing_and_says_so() {
        assert_eq!(Futures::channels("no-such-future"), 0);
        assert!(!Futures::is_known("no-such-future"));
        assert!(Futures::apply(&Fixture::sawtooth(5), 1, "no-such-future").is_none());
    }

    #[test]
    fn the_digest_is_stable_for_identical_fixtures() {
        // A digest that moved between two identical fixtures would make the
        // report banner meaningless.
        assert_eq!(
            Fixture::sawtooth(40).digest(),
            Fixture::sawtooth(40).digest()
        );
    }
}
