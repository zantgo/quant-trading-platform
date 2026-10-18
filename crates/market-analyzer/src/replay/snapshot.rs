//! The immutable recorder (P7 W7.3).

use super::{Canonical, Clock, Run};
use std::cell::RefCell;

/// One published value, stamped with the bar at which it became knowable.
///
/// `available_at` is a first-class field rather than inferred from position in
/// a list. Position is not availability: a calculation that reads one bar ahead
/// of its pivot publishes that pivot one bar later than its index suggests, and
/// a harness that indexed by position would silently bless the look-ahead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The bar at which this value could first have been known.
    pub available_at: usize,
    /// The bar the value is *about* — the candle it was computed from.
    ///
    /// Distinct from `available_at` for every confirmation-delayed calculation.
    pub subject_bar: usize,
    /// What was published — the indicator key.
    pub key: String,
    /// The value, canonicalised for comparison.
    pub value: Canonical,
}

impl Frame {
    pub fn new(
        available_at: usize,
        subject_bar: usize,
        key: impl Into<String>,
        value: impl Into<Canonical>,
    ) -> Self {
        Frame {
            available_at,
            subject_bar,
            key: key.into(),
            value: value.into(),
        }
    }

    /// A frame about bar `i` that is knowable immediately (no look-ahead).
    pub fn contemporaneous(key: impl Into<String>, value: impl Into<Canonical>) -> Self {
        Frame {
            available_at: 0,
            subject_bar: 0,
            key: key.into(),
            value: value.into(),
        }
        .at(0)
    }

    fn at(mut self, bar: usize) -> Self {
        self.available_at = bar;
        self.subject_bar = bar;
        self
    }

    /// Whether this frame was knowable at or before `bar`.
    pub fn is_knowable_at(&self, bar: usize) -> bool {
        self.available_at <= bar
    }

    /// The rendering used for byte-comparison across two runs.
    pub fn render(&self) -> String {
        format!("{}@{}={}", self.key, self.subject_bar, self.value.as_str())
    }
}

/// An append-only recording of one run.
///
/// Append-only rather than mutable because the whole value of the harness is
/// that a run cannot be revised after the fact. A recorder that could rewrite
/// history would let a bug hide itself.
#[derive(Debug, Default)]
pub struct Recorder {
    frames: RefCell<Vec<Frame>>,
}

impl Recorder {
    pub fn new() -> Self {
        Recorder {
            frames: RefCell::new(Vec::new()),
        }
    }

    /// Publish a frame. Returns it for chaining.
    pub fn record(&self, frame: Frame) -> Frame {
        self.frames.borrow_mut().push(frame.clone());
        frame
    }

    /// Publish a frame that is knowable exactly now.
    pub fn publish(&self, clock: &dyn Clock, key: &str, value: impl Into<Canonical>) -> Frame {
        let bar = clock.bar();
        self.record(Frame::new(bar, bar, key, value))
    }

    pub fn len(&self) -> usize {
        self.frames.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.borrow().is_empty()
    }

    /// The recorded frames, ordered by `available_at` then `subject_bar`.
    ///
    /// Sorted rather than returned in push order so that two runs that publish
    /// the same values in a different interleaving still compare equal — the
    /// gate is about *content*, not about scheduling.
    pub fn into_run(self) -> Run {
        let mut by_bar: Run = std::collections::BTreeMap::new();
        for f in self.frames.into_inner() {
            by_bar.entry(f.available_at).or_default().push(f);
        }
        for frames in by_bar.values_mut() {
            frames.sort_by(|a, b| {
                a.subject_bar
                    .cmp(&b.subject_bar)
                    .then_with(|| a.key.cmp(&b.key))
            });
        }
        by_bar
    }

    pub fn run(&self) -> Run {
        let copy = Recorder::new();
        for f in self.frames.borrow().iter() {
            copy.record(f.clone());
        }
        copy.into_run()
    }
}

impl Recorder {
    /// Convenience: a fresh run from a per-bar publication function.
    ///
    /// Lives on the type so there is exactly one way to turn a recorder into a
    /// [`Run`] — a second, subtly different one would let two gates compare
    /// differently-shaped runs and both pass.
    pub fn record_all<F>(bars: usize, mut publish: F) -> Run
    where
        F: FnMut(usize, &dyn Clock) -> Vec<Frame>,
    {
        let recorder = Recorder::new();
        for bar in 0..bars {
            for f in publish(bar, &BarClockOnly(bar)) {
                recorder.record(f);
            }
        }
        recorder.into_run()
    }
}

struct BarClockOnly(usize);

impl Clock for BarClockOnly {
    fn bar(&self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zero_and_zero_render_identically() {
        // Otherwise the first negative zero a publisher emits reads as a
        // divergence that is not one.
        assert_eq!(Canonical::from_f64(0.0), Canonical::from_f64(-0.0));
        assert_eq!(
            Canonical::from_f64(0.0).as_str(),
            Canonical::from_f64(-0.0).as_str()
        );
    }

    #[test]
    fn nan_is_folded_rather_than_propagated() {
        // A NaN never equals itself, so a diff built on it is unreportable. CH-7
        // says NaN should not be here; folding it keeps the report legible
        // instead of turning a finding into noise.
        assert_eq!(Canonical::from_f64(f64::NAN), Canonical::from_f64(f64::NAN));
        assert_eq!(Canonical::from_f64(f64::NAN).as_str(), "NaN");
        assert_ne!(Canonical::from_f64(f64::NAN), Canonical::from_f64(1.0));
    }

    #[test]
    fn a_frame_knowable_later_is_not_knowable_now() {
        let f = Frame::new(
            /* available_at */ 10, /* subject_bar */ 8, "pivot", 1.0,
        );
        assert!(!f.is_knowable_at(9));
        assert!(f.is_knowable_at(10));
    }

    #[test]
    fn a_run_sorts_by_bar_then_subject_then_key() {
        // Two runs that publish the same values in a different interleaving must
        // compare equal — the gate is about content, not scheduling.
        let a = Recorder::new();
        a.record(Frame::new(1, 1, "x", 1.0));
        a.record(Frame::new(0, 0, "y", 2.0));

        let b = Recorder::new();
        b.record(Frame::new(0, 0, "y", 2.0));
        b.record(Frame::new(1, 1, "x", 1.0));

        assert_eq!(a.run(), b.run());
    }
}
