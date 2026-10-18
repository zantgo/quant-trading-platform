//! **CG6 — a degenerate reference is `UNDEFINED`, never a number** (P8).
//!
//! ## The claim
//!
//! 01-20 §6 G6: *`UNDEFINED` — never `0`, never `inf`, never `NaN`; floor fixed
//! at creation and causal.* 01-14 §4.0 is blunter: "The denominator is never
//! guarded by substituting a default. Returning `0`, returning `progress = 0`,
//! or falling back to a previous target all produce a number that *looks* like a
//! measurement and is not one."
//!
//! ## Why `0` is the dangerous default
//!
//! `progress = 0` means "at the origin". A substituted `0` would assert that the
//! movement has not started — a claim, not an absence. The same applies to `NaN`:
//! it propagates silently through an aggregate and turns one undefined reading
//! into a whole-column absence nobody can locate (L14).
//!
//! The mutation this catches is **M4 — progress clamped to `[0, 1]`** — which
//! shares this gate with CG3 because clamping is a degenerate-reference failure
//! wearing a different hat: it substitutes a number for a refusal.

use core_domain::axes::UndefinedReason;
use core_domain::leg::{
    progress_of, Classification, Direction, LegKey, Progress, StructureLevel, TargetBasis,
};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy};

fn key() -> LegKey {
    LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    }
}

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
fn cg6_an_exactly_degenerate_target_is_undefined() {
    let p = progress_of(100.0, 105.0, Some(100.0), 0.5);
    assert!(
        p.is_undefined(),
        "T == O is a division by zero, not a progress of zero"
    );
    assert_eq!(p.value(), None, "and it yields no number to propagate");
    match p {
        Progress::Undefined { reason } => {
            assert!(
                matches!(reason, UndefinedReason::DegenerateReference { .. }),
                "the reason is typed and nameable, got {reason:?}"
            );
        }
        other => panic!("expected an undefined reading, got {other:?}"),
    }
}

#[test]
fn cg6_a_target_below_the_legs_own_noise_floor_is_undefined() {
    // The floor is the leg's OWN realised range — not an absolute price distance
    // (P2) and not a per-duration constant (P3).
    let floor = noise_floor_from_range(50.0);
    assert!(floor > 0.0);
    // |T - O| = 0.1 with a floor of 0.5 → undefined.
    assert!(progress_of(100.0, 105.0, Some(100.1), floor).is_undefined());
    // The same span with a floor of 0.05 → a number. The floor is the only thing
    // that changed.
    assert!(progress_of(100.0, 105.0, Some(100.1), 0.05)
        .value()
        .is_some());
}

#[test]
fn cg6_no_reading_is_infinite_or_nan() {
    let cases = [
        progress_of(100.0, 105.0, Some(100.0), 0.5),
        progress_of(100.0, 105.0, Some(100.0001), 0.5),
        progress_of(100.0, 105.0, None, 0.5),
        progress_of(100.0, 105.0, Some(f64::NAN), 0.5),
        progress_of(100.0, 105.0, Some(f64::INFINITY), 0.5),
    ];
    for p in cases {
        if let Some(v) = p.value() {
            assert!(
                v.is_finite(),
                "a progress reading is finite or absent, never NaN/inf: {v}"
            );
        }
    }
}

#[test]
fn cg6_undefined_does_not_render_as_zero() {
    // L14: it is a distinct value. `value()` returning None IS the type-level
    // statement; asserting `.unwrap_or(0.0)` never happens means a consumer
    // cannot accidentally turn absence into "at the origin".
    let p = progress_of(100.0, 105.0, Some(100.0), 0.5);
    assert_ne!(p.value(), Some(0.0));
    assert_eq!(
        p.value().unwrap_or(0.0),
        0.0,
        "only the consumer's default does that"
    );
    // Which is precisely why the default must not be written down at a call
    // site: this gate asserts the type refuses before any consumer can.
    assert!(p.is_undefined());
}

#[test]
fn cg6_an_overrun_is_not_substituted_onto_the_reference() {
    // The same failure as a substituted default, at the other end of the scale:
    // clamping replaces "well past the target" with the target's own value. CG6
    // owns this because §4.0's rule — *never guard the denominator by
    // substituting a value* — is the same rule applied to the result: a number
    // that stands in for a refusal or for a fact is not a measurement.
    let past = progress_of(100.0, 160.0, Some(110.0), 0.5);
    let at = progress_of(100.0, 110.0, Some(110.0), 0.5);
    assert_ne!(
        past.value(),
        at.value(),
        "an overrun must not read as 'at the target' — that is what clamping does"
    );
    let backwards = progress_of(100.0, 98.0, Some(110.0), 0.5);
    let origin = progress_of(100.0, 100.0, Some(110.0), 0.5);
    assert_ne!(
        backwards.value(),
        origin.value(),
        "a move against the leg must not read as 'at the origin'"
    );
}

#[test]
fn cg6_the_floor_is_frozen_at_creation_and_never_evolves() {
    // L21: a floor that evolved would make progress move with no price moving.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let floor = m.leg(&k).unwrap().noise_floor;

    // Anything at all happens: observations, revisions, a reclassification.
    for bar in 16..40 {
        m.observe(&k, 105.0 + (bar as f64) * 3.0, bar);
    }
    m.revise_target(&k, 130.0, 41, TargetBasis::Extension, "wide");
    m.reclassify(&k, StructureLevel::Minor);
    assert_eq!(
        m.leg(&LegKey {
            structure_level: StructureLevel::Minor,
            ..k.clone()
        })
        .unwrap()
        .noise_floor,
        floor,
        "L21: the noise floor is frozen for the life of the leg"
    );
}

#[test]
fn cg6_a_degenerate_reference_publishes_no_stage() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    for bar in 16..19 {
        m.observe(&k, 105.0, bar);
    }
    assert!(
        m.stage(&k, 105.0).is_some(),
        "a sound reference gives a stage"
    );

    // Collapse the reference onto the origin.
    assert!(m.revise_target(&k, 100.0, 20, TargetBasis::Measured, "collapsed"));
    for bar in 21..24 {
        m.observe(&k, 105.0, bar);
    }
    assert!(
        m.leg(&k).unwrap().target_stable,
        "stability is about revisions, not soundness"
    );
    assert!(
        m.stage(&k, 105.0).is_none(),
        "L13: an undefined progress can never yield a stage, however stable the reference is"
    );
}

#[test]
fn cg6_an_absent_target_is_undefined_rather_than_a_previous_value() {
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
    assert!(
        m.progress(&k, 105.0).is_undefined(),
        "before a reference exists there is no progress — and no fallback to one"
    );
    match m.progress(&k, 105.0) {
        Progress::Undefined { reason } => assert!(matches!(
            reason,
            UndefinedReason::DegenerateReference { .. } | UndefinedReason::NotApplicable { .. }
        )),
        other => panic!("expected undefined, got {other:?}"),
    }
}

#[test]
fn cg6_a_missing_leg_is_typed_absence_not_a_panic() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let absent = LegKey {
        instrument: "ETH-USDC".into(),
        ..key()
    };
    assert!(m.leg(&absent).is_none());
    assert!(m.progress(&absent, 100.0).is_undefined());
    assert!(m.stage(&absent, 100.0).is_none());
    assert_eq!(m.observe(&absent, 100.0, 1), None);
    assert!(!m.revise_target(&absent, 110.0, 2, TargetBasis::Swing, "x"));
    assert!(!m.revise_origin(&absent, 100.0, 2));
    assert!(!m.reclassify(&absent, StructureLevel::Minor));
}
