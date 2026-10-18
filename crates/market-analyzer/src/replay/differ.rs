//! Element-wise differ (P7 W7.3).

use super::{Frame, Run};

/// Where two runs part company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The earliest bar at which the runs differ.
    pub bar: usize,
    /// The frame the baseline published there.
    pub baseline: Option<String>,
    /// The frame the perturbed run published there.
    pub perturbed: Option<String>,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "divergence at bar {}: baseline {:?} vs perturbed {:?}",
            self.bar, self.baseline, self.perturbed
        )
    }
}

/// Compares two [`Run`]s bar by bar.
pub struct Differ;

impl Differ {
    pub fn new() -> Self {
        Differ
    }

    /// The earliest bar at which the two runs differ, or `None` if they are
    /// identical at every bar.
    ///
    /// Earliest-first is deliberate. A violation that first shows at bar 40 with
    /// a cascade of knock-on differences after it is one bug; reporting the last
    /// difference would report forty.
    pub fn first_divergence(&self, baseline: &Run, perturbed: &Run) -> Option<Divergence> {
        let mut bars: Vec<usize> = baseline.keys().chain(perturbed.keys()).copied().collect();
        bars.sort_unstable();
        bars.dedup();

        for bar in bars {
            let b = baseline.get(&bar).map(|v| render(v)).unwrap_or_default();
            let p = perturbed.get(&bar).map(|v| render(v)).unwrap_or_default();
            if b != p {
                return Some(Divergence {
                    bar,
                    baseline: baseline
                        .get(&bar)
                        .and_then(|v| v.first())
                        .map(Frame::render),
                    perturbed: perturbed
                        .get(&bar)
                        .and_then(|v| v.first())
                        .map(Frame::render),
                });
            }
        }
        None
    }

    /// Assert that nothing published at or before `t` changed.
    ///
    /// This is the CG2 assertion in its general form. `t` is a **bar**, not a
    /// run index: the caller perturbs at `t + k` and asks whether anything at or
    /// before `t` moved. Returns the divergence if one exists so the caller can
    /// quote it rather than re-deriving it.
    pub fn assert_unchanged_through(
        &self,
        baseline: &Run,
        perturbed: &Run,
        t: usize,
    ) -> Option<Divergence> {
        let mut through = baseline.clone();
        let mut pthrough = perturbed.clone();
        through.retain(|k, _| *k <= t);
        pthrough.retain(|k, _| *k <= t);
        self.first_divergence(&through, &pthrough)
    }
}

fn render(frames: &[Frame]) -> String {
    frames
        .iter()
        .map(Frame::render)
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::snapshot::Recorder;

    fn run_of(entries: &[(usize, &str, f64)]) -> Run {
        let r = Recorder::new();
        for (bar, key, v) in entries {
            r.record(Frame::new(*bar, *bar, *key, *v));
        }
        r.into_run()
    }

    #[test]
    fn identical_runs_do_not_diverge() {
        let a = run_of(&[(0, "x", 1.0), (1, "x", 2.0)]);
        let b = run_of(&[(0, "x", 1.0), (1, "x", 2.0)]);
        assert_eq!(Differ::new().first_divergence(&a, &b), None);
    }

    #[test]
    fn the_earliest_divergence_is_reported_not_the_latest() {
        // A violation at bar 1 cascades into 2 and 3. Reporting the last would
        // report three bugs for one.
        let base = run_of(&[(0, "x", 1.0), (1, "x", 1.0), (2, "x", 1.0), (3, "x", 1.0)]);
        let pert = run_of(&[(0, "x", 1.0), (1, "x", 9.0), (2, "x", 9.0), (3, "x", 9.0)]);
        let d = Differ::new().first_divergence(&base, &pert).unwrap();
        assert_eq!(d.bar, 1);
    }

    #[test]
    fn a_change_past_t_is_not_reported_as_a_change_at_or_before_t() {
        // The exact shape CG2 asserts: perturb the future, ask about the past.
        let base = run_of(&[(0, "x", 1.0), (5, "x", 1.0)]);
        let pert = run_of(&[(0, "x", 1.0), (5, "x", 2.0)]);
        let d = Differ::new();
        assert_eq!(d.first_divergence(&base, &pert).map(|x| x.bar), Some(5));
        assert_eq!(d.assert_unchanged_through(&base, &pert, 4), None);
    }

    #[test]
    fn a_look_ahead_is_caught_at_the_bar_it_leaks_to() {
        // A calculation that reads bar 6 while publishing at bar 5. The gate
        // must catch it at 5, which is the bar the reader would have believed.
        let base = run_of(&[(0, "x", 1.0), (5, "x", 1.0)]);
        let leaky = run_of(&[(0, "x", 1.0), (5, "x", 6.0)]);
        let d = Differ::new().assert_unchanged_through(&base, &leaky, 5);
        let rendered = d.as_ref().map(|x| x.to_string()).unwrap_or_default();
        assert_eq!(d.as_ref().map(|x| x.bar), Some(5));
        assert!(rendered.contains("bar 5"), "report said {rendered:?}");
    }
}
