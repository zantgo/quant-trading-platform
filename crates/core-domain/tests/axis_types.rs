//! V-37 / V-38 — the ordinal prohibition is a compile error, not a convention.
//!
//! `trybuild` compiles each fixture under `tests/compile_fail/` and asserts it
//! FAILS. A test cannot prove a negative; only the compiler can. If someone adds
//! `impl Add for Stage`, `impl From<f64> for Stage`, or any other numeric
//! conversion, these fixtures start passing and this suite goes red.

#[test]
fn stage_has_no_arithmetic() {
    let t = trybuild::TestCases::new();
    // E0369: binary operation `+` cannot be applied to `Stage`
    t.compile_fail("tests/compile_fail/stage_add.rs");
    // E0369: binary operation `*` cannot be applied to `Stage`
    t.compile_fail("tests/compile_fail/stage_mul.rs");
    // E0277: `f64: From<Stage>` is not satisfied — the mean(stage) guard
    t.compile_fail("tests/compile_fail/stage_from_f64.rs");
}
