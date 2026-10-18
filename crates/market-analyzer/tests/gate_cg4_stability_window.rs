//! **CG4 — the stability window boundary** (P8).
//!
//! ## The claim
//!
//! 01-20 §6 G4: *`W−1` bars → false; `W` bars → true; the revision bar is never
//! counted.* 01-14 §4.5 states the same thing and explains why a minimum
//! observation count is required at all: without it, a target set on one bar and
//! untouched on the next would report `stable` immediately, which asserts a
//! confidence no one-bar observation supports.
//!
//! ## Why the off-by-one is the whole gate
//!
//! Counting the revision bar would make `W` unrevised bars elapse one bar early,
//! and the error is invisible in every other test: the leg still stabilises, still
//! publishes a stage, still behaves. M3 injects exactly this and CG4 and CG5 both
//! catch it.

use core_domain::leg::{Classification, Direction, LegKey, StructureLevel, TargetBasis};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy, STABILITY_WINDOW};

fn key() -> LegKey {
    LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    }
}

/// A leg whose reference was stated at bar 15 and never revised since.
fn referenced(m: &mut LegMachine) -> LegKey {
    let k = key();
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "init");
    k
}

#[test]
fn cg4_the_window_is_exactly_w_bars_and_not_one_fewer() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let w = m.policy().stability_window as usize;
    assert_eq!(
        w as u32, STABILITY_WINDOW,
        "the policy states the window it enforces"
    );

    // Bar 15 was the revision. It is not an unrevised bar.
    assert!(
        !m.leg(&k).unwrap().target_stable,
        "the revision bar itself is never stability"
    );

    // W-1 unrevised bars.
    for bar in 16..(16 + w - 1) {
        m.observe(&k, 105.0, bar);
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "{bar}: {w}-1 unrevised bars is still not stable"
        );
    }

    // The W-th.
    m.observe(&k, 105.0, 16 + w - 1);
    assert!(
        m.leg(&k).unwrap().target_stable,
        "exactly {w} consecutive unrevised bars is the boundary"
    );
}

#[test]
fn cg4_a_window_of_one_still_requires_one_observation() {
    // The degenerate policy makes the off-by-one visible: if the revision bar
    // were counted, `W = 1` would report stable with zero observations.
    let mut policy = LegPolicy::default();
    policy.stability_window = 1;
    let mut m = LegMachine::new(policy, noise_floor_from_range(50.0));
    let k = referenced(&mut m);

    assert!(
        !m.leg(&k).unwrap().target_stable,
        "with W=1 the revision bar is still not an observation"
    );
    m.observe(&k, 105.0, 16);
    assert!(m.leg(&k).unwrap().target_stable);
}

#[test]
fn cg4_a_revision_resets_the_window_completely() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let w = m.policy().stability_window as usize;

    // Build stability, then lose it.
    for bar in 16..(16 + w) {
        m.observe(&k, 105.0, bar);
    }
    assert!(m.leg(&k).unwrap().target_stable);
    assert!(m.revise_target(&k, 112.0, 30, TargetBasis::Measured, "moved"));

    // The counter restarts: the next W-1 bars must NOT stabilise it.
    for bar in 31..(31 + w - 1) {
        m.observe(&k, 105.0, bar);
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "{bar}: the window restarted at the revision"
        );
    }
    m.observe(&k, 105.0, 31 + w - 1);
    assert!(m.leg(&k).unwrap().target_stable);
}

#[test]
fn cg4_stability_is_revoked_at_a_single_bars_notice() {
    // §4.5: "no grace period and no hysteresis".
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    for bar in 16..19 {
        m.observe(&k, 105.0, bar);
    }
    assert!(m.leg(&k).unwrap().target_stable);

    assert!(m.revise_target(&k, 113.0, 25, TargetBasis::Swing, "revoked"));
    assert!(
        !m.leg(&k).unwrap().target_stable,
        "a reference that just moved cannot be called stable, whatever its history"
    );
}

#[test]
fn cg4_an_out_of_order_bar_does_not_advance_the_window() {
    // The window counts OBSERVATIONS. A bar at or before the last observed one is
    // not a new observation — it would be a reordering, and counting it would let
    // a caller stabilise a leg by replaying the same bars.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let w = m.policy().stability_window as usize;

    for bar in 16..(16 + w) {
        m.observe(&k, 105.0, bar);
    }
    assert!(m.leg(&k).unwrap().target_stable);
    let stable_at = m.leg(&k).unwrap().bars_since_origin;

    // Replay from the end of history downwards: bar 19 is genuinely newer, and
    // every bar from 18 back to 10 is a re-read of the past.
    for bar in (10..30usize).rev() {
        m.observe(&k, 105.0, bar);
    }
    assert_eq!(
        m.leg(&k).unwrap().bars_since_origin,
        stable_at + 1,
        "exactly one bar was genuinely newer; the other ten re-reads advanced nothing"
    );
}

#[test]
fn cg4_a_target_that_is_stated_equals_a_revision_for_stability() {
    // Stating the first reference is a move of `target_price`, so it starts the
    // window at zero — otherwise a leg would be stable before it was referenced.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = key();
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    assert!(m.leg(&k).unwrap().target_price.is_none());
    assert!(!m.leg(&k).unwrap().target_stable);
    assert!(m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "first"));
    assert!(!m.leg(&k).unwrap().target_stable);
    assert_eq!(m.leg(&k).unwrap().unrevised_bars, 0);
}
