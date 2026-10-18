//! # Replay harness — the causality substrate (P7 W7.3)
//!
//! Every gate in `CG1`–`CG13` asks the same shape of question: *did anything
//! published at or before bar `t` change when I perturbed bar `t+k`?* This module
//! is the machinery that makes that question answerable.
//!
//! It is deliberately small and deliberately **not** a simulator. It does not
//! know what an indicator is. It records what was published, with the
//! `available_at` at which it became knowable, and it diffs two such records.
//!
//! ## Why `available_at` and not the bar index
//!
//! A value computed from bars `0..=i` is knowable at bar `i` only if the
//! calculation does not read `i+1`. A 3-bar fractal reads one bar ahead of its
//! pivot, so a pivot at `i` is confirmable only at `i+1` — see
//! `PivotPoint::confirming_bar`.
//!
//! Indexing availability by bar number would let a reader assume `published[i]`
//! was knowable at `i`, which is exactly the assumption that produces a
//! backtest that wins on hindsight. [`Recorded`] stores the bar at which each
//! frame became available, and [`Differ`] compares **at a bar**, not at a list
//! position.
//!
//! ## The contract
//!
//! A correct calculation satisfies, for every `t` and every `k > 0`:
//!
//! ```text
//! perturb(bar = t + k)  ⇒  frame.available_at <= t is byte-identical
//! ```
//!
//! [`Differ::first_divergence`] returns the earliest bar at which the two
//! recordings part company, so a violation is reported as a location rather
//! than as a diff dump.

pub mod differ;
pub mod perturb;
pub mod snapshot;

pub use differ::{Differ, Divergence};
pub use perturb::{Futures, Perturbation, TARGETED_FUTURES};
pub use snapshot::{Frame, Recorder};

use std::collections::BTreeMap;

/// The simulated clock.
///
/// An `available_at` is a bar index, not a wall-clock timestamp: causality here
/// is about *ordering of information*, and a bar index makes the ordering
/// total and its violations legible. Wall-clock rounding is a separate concern
/// and would only obscure this one.
pub trait Clock {
    /// The bar at which values are now being computed.
    fn bar(&self) -> usize;
}

/// The clock every harness test runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarClock(pub usize);

impl Clock for BarClock {
    fn bar(&self) -> usize {
        self.0
    }
}

/// A canonical, comparable rendering of a published value.
///
/// Serialising to a canonical string is what makes "byte-identical" meaningful
/// across two independent runs. `f64` would compare `0.0 != -0.0` and would let
/// `NaN` compare unequal to itself, so a gate built on it would report
/// differences that are not differences and miss ones that are.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Canonical(String);

impl Canonical {
    pub fn new(s: impl Into<String>) -> Self {
        Canonical(s.into())
    }

    /// Canonicalise an `f64` so that two numerically equal values always render
    /// identically.
    ///
    /// `-0.0` and `0.0` are equal numerically but render differently, and a
    /// gate that flags that would cry wolf on the first negative zero it meets.
    /// `NaN` is folded to a sentinel rather than propagated, because a NaN that
    /// never equals itself would make every diff unreportable — and CH-7 means
    /// NaN should not be here in the first place, so a NaN is already a finding.
    pub fn from_f64(v: f64) -> Self {
        let normalised = if v == 0.0 { 0.0 } else { v };
        if normalised.is_nan() {
            return Canonical::new("NaN");
        }
        // 17 significant digits round-trips every finite `f64` exactly.
        Canonical::new(format!("{normalised:.17e}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Canonical {
    fn from(s: &str) -> Self {
        Canonical::new(s)
    }
}

impl From<String> for Canonical {
    fn from(s: String) -> Self {
        Canonical::new(s)
    }
}

impl From<f64> for Canonical {
    fn from(v: f64) -> Self {
        Canonical::from_f64(v)
    }
}

/// A complete recording of one run, keyed by the bar each value became
/// available.
pub type Run = BTreeMap<usize, Vec<Frame>>;

/// Compare two runs and report the earliest bar at which they part company.
///
/// `None` means the two runs are identical at every bar — which is the property
/// CG2 asserts.
pub fn first_divergence(baseline: &Run, perturbed: &Run) -> Option<Divergence> {
    Differ::new().first_divergence(baseline, perturbed)
}
