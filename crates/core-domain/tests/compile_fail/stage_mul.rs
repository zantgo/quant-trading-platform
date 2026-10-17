//! V-37 — `Stage` must have no arithmetic. This file MUST NOT compile.
use core_domain::axes::Stage;

fn main() {
    let s = Stage::Late;
    // E0369: binary operation `*` cannot be applied to type `Stage`
    let _ = s * 0.5;
}
