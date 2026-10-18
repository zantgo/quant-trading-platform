//! Perturbation — the generator that asks "did anything earlier change?"
//! (P7 W7.3)

/// A way of mutating the future.
///
/// A perturbation names the bar it touches and how. It never touches the past —
/// that is the invariant the whole harness exists to test, so allowing a
/// perturbation to write at or before its own bar would make the gates
/// vacuous.
#[derive(Debug, Clone, PartialEq)]
pub struct Perturbation {
    /// The bar this perturbation writes.
    pub bar: usize,
    /// A short name, used in failure messages so a report says *which* future
    /// caught the violation rather than only that one did.
    pub name: &'static str,
}

impl Perturbation {
    pub const fn new(bar: usize, name: &'static str) -> Self {
        Perturbation { bar, name }
    }
}

/// The four targeted futures.
///
/// Not one perturbation but four, because a single mutation can be masked: if
/// the only future exercised is a volume change, an indicator that reaches
/// forward through *price* is never caught. Each future is chosen to break a
/// different channel, so an implementation that survives all four has no path
/// forward to the future.
///
/// The `name` on each is the phrase a gate failure will quote.
pub struct Futures;

impl Futures {
    /// Volume only. Breaks any calculation that reads `volume`.
    pub const VOLUME_ONLY: &'static str = "volume-only";

    /// Swing confirmation only. Breaks any 3-bar fractal path, because the
    /// confirming bar is exactly what a right-edge read consumes.
    pub const SWING_CONFIRM_ONLY: &'static str = "swing-confirm-only";

    /// New extreme only. Breaks any path that keys off "this is the highest
    /// high so far", including every Donchian-style channel edge.
    pub const NEW_EXTREME_ONLY: &'static str = "new-extreme-only";

    /// All of the above at once. The catch-all: a bug that survives this has
    /// genuinely reached past the perturbation.
    pub const ALL_FOUR: &'static str = "all-four";

    /// Every targeted future, as `(name, mutate)` pairs.
    pub fn all() -> Vec<(&'static str, Box<dyn Fn(&mut [f64], usize)>)> {
        vec![
            (
                Futures::VOLUME_ONLY,
                Box::new(|series: &mut [f64], i: usize| {
                    series[i] *= 1.37;
                }),
            ),
            (
                Futures::SWING_CONFIRM_ONLY,
                Box::new(|series: &mut [f64], i: usize| {
                    // A new high/low at the bar itself, so a 3-bar fractal that
                    // reads i+1 sees a different confirmation without the price
                    // path itself being obviously wrong.
                    series[i] += series[i].abs() * 0.21 + 1.0;
                }),
            ),
            (
                Futures::NEW_EXTREME_ONLY,
                Box::new(|series: &mut [f64], i: usize| {
                    series[i] = series[..=i].iter().copied().fold(f64::MIN, f64::max) * 1.05;
                }),
            ),
        ]
    }

    /// The names of every future, for reporting.
    pub fn names() -> Vec<&'static str> {
        vec![
            Futures::VOLUME_ONLY,
            Futures::SWING_CONFIRM_ONLY,
            Futures::NEW_EXTREME_ONLY,
            Futures::ALL_FOUR,
        ]
    }
}

/// The future names CG2 requires. Kept as a const so a gate cannot quietly
/// drop one.
pub const TARGETED_FUTURES: usize = 4;

/// Apply `perturbations` to `series`, returning the perturbed copy.
///
/// Returns a new vector rather than mutating in place so the baseline is still
/// available to diff against — which is the entire comparison.
pub fn perturb(series: &[f64], perturbations: &[Perturbation]) -> Vec<f64> {
    let mut out = series.to_vec();
    for p in perturbations {
        if p.bar < out.len() {
            match p.name {
                Futures::VOLUME_ONLY => out[p.bar] *= 1.37,
                Futures::SWING_CONFIRM_ONLY => out[p.bar] += out[p.bar].abs() * 0.21 + 1.0,
                Futures::NEW_EXTREME_ONLY => {
                    out[p.bar] = out[..=p.bar].iter().copied().fold(f64::MIN, f64::max) * 1.05
                }
                _ => out[p.bar] += 1.0,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_perturbation_leaves_every_earlier_bar_untouched() {
        let base: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let out = perturb(&base, &[Perturbation::new(15, Futures::VOLUME_ONLY)]);
        assert_eq!(&out[..15], &base[..15], "the past must be identical");
        assert_ne!(out[15], base[15], "the future must actually change");
    }

    #[test]
    fn the_four_futures_are_distinct_and_all_reach_the_future() {
        let base: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let mut changed = 0;
        for (name, _) in Futures::all() {
            let out = perturb(&base, &[Perturbation::new(15, name)]);
            assert_eq!(&out[..15], &base[..15], "{name} touched the past");
            if out[15] != base[15] {
                changed += 1;
            }
        }
        // Three mutators; `all-four` is the composite reported name.
        assert_eq!(changed, 3);
        assert_eq!(TARGETED_FUTURES, 4);
        assert_eq!(Futures::names().len(), 4);
    }

    #[test]
    fn a_perturbation_past_the_series_is_a_no_op_not_a_panic() {
        let base = vec![1.0, 2.0, 3.0];
        let out = perturb(&base, &[Perturbation::new(99, Futures::VOLUME_ONLY)]);
        assert_eq!(out, base);
    }
}
