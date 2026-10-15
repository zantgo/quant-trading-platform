//! AUDIT-M18: synthetic frames must report the structure that actually held.
//!
//! Doji-fill, idle-heartbeat and reconnect-gap snapshots are built from a flat
//! candle that never traded. They hardcoded
//! `DivergenceState::None` and `fib/pattern/support/resistance: None`, so each
//! one reported "no structure".
//!
//! The consequences were quiet and misleading: alignment dimension 5
//! (Structure) read `NO_DATA` for the whole stall window, and the
//! Analysis/Opportunity layers lost their fib and pattern inputs — while
//! EMA/RSI/ADX kept advancing, so the chart still looked healthy. The only
//! symptom was a structural layer going dark exactly when the feed was already
//! unreliable, which is the opposite of when you want it dark.
//!
//! Carrying the last REAL values forward is strictly more honest: the
//! structure genuinely did not change during a no-trade window.
//!
//! `CarriedStructure` is `pub(super)`, so the contract is pinned at the level
//! it is actually expressed in — the values the synthetic normalizers are
//! handed. Re-introducing the hardcoded empty block fails these tests.

/// The synthetic normalizers must read the CARRIED structure.
///
/// There are two of them: the completed-snapshot builder (doji/idle/gap) and
/// the shadow broadcaster. Before the fix both hardcoded
/// `fib: None, pattern: None, support_levels: &[], resistance_levels: &[]`.
#[test]
fn the_synthetic_normalizers_read_the_carried_structure() {
    let src = include_str!("../src/analyzer/mod.rs");

    let fib = src.matches("fib: carried.fib.as_ref()").count();
    assert!(
        fib >= 2,
        "AUDIT-M18 regression: only {fib} synthetic normalizer(s) read the \
         carried Fibonacci range; the rest would report \"no structure\" again"
    );

    let pattern = src.matches("pattern: carried.pattern.as_ref()").count();
    assert!(
        pattern >= 2,
        "AUDIT-M18 regression: only {pattern} synthetic normalizer(s) read the \
         carried chart pattern"
    );

    let support = src
        .matches("support_levels: &carried.support_levels")
        .count();
    assert!(
        support >= 2,
        "AUDIT-M18 regression: carried support levels are no longer threaded"
    );

    let resistance = src
        .matches("resistance_levels: &carried.resistance_levels")
        .count();
    assert!(
        resistance >= 2,
        "AUDIT-M18 regression: carried resistance levels are no longer threaded"
    );
}

/// The real-close path must RETURN the structure it computed, so the pipeline
/// can refresh what it carries. Without this the carried value would freeze at
/// whatever the cold-start snapshot happened to hold.
#[test]
fn the_real_close_path_returns_the_structure_it_computed() {
    let src = include_str!("../src/analyzer/mod.rs");
    assert!(
        src.contains("CarriedStructure::from_real("),
        "the real-close path must build a CarriedStructure from the readings it \
         just computed"
    );
    assert!(
        src.contains("carried_structure = real_structure;"),
        "run_single must refresh its carried structure after a real close, or \
         the carry-forward never advances past start-up"
    );
    assert!(
        src.contains("pub fib: Option<crate::indicators::FibonacciRange>")
            && src.contains("pub pattern: Option<crate::indicators::PatternResult>")
            && src.contains("pub support_levels: Vec<f64>")
            && src.contains("pub resistance_levels: Vec<f64>"),
        "CarriedStructure must own the fib / pattern / S-R readings; a \
         divergence-only struct is what left them hardcoded empty"
    );
}

/// A cold pipeline has nothing to carry, which is the correct state — the
/// guard against "always report something" being fixed by inventing values.
#[test]
fn a_cold_pipeline_still_reports_no_structure() {
    let src = include_str!("../src/analyzer/mod.rs");
    assert!(
        src.contains("..Self::default()"),
        "`from_snapshot` must fall back to the default (empty) structure rather \
         than fabricating fib/pattern levels it does not have"
    );
}
