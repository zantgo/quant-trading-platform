//! V-38 — `Stage` must not be convertible to a magnitude. MUST NOT compile.
//!
//! This is the type-level guard against `mean(stage)`. Progression is ordinal:
//! its cross-duration summary is a mode (`StageHistogram::mode`) or an interval
//! (`StageHistogram::range`), never a centre. If `Stage: From<f64>` existed, a
//! caller could average stages by accident and the compiler would not object.
use core_domain::axes::Stage;

fn centre(stages: &[Stage]) -> f64 {
    // E0277: `f64: From<Stage>` is not satisfied — `Stage` holds an anchor, not a
    // magnitude. There is deliberately no `as f64`, no `From<f64>`, and no `Deref`.
    stages.iter().map(|s| f64::from(*s)).sum::<f64>() / stages.len() as f64
}

fn main() {
    let _ = centre(&[Stage::Early, Stage::Mid, Stage::Late]);
}
