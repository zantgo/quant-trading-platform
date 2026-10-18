//! **CG3 — target revision ≠ movement** (P8).
//!
//! ## The question
//!
//! 01-20 §6: *price rises, target moves away; `progress` may fall, no movement
//! fabricated, unstable stage not promoted.*
//!
//! ## Why this is the gate that catches the most expensive bug
//!
//! `progress` changes for two independent reasons — the target moved and the
//! price moved — and a consumer watching `progress` alone reads target expansion
//! as a fresh advance into new territory (01-14 §4.3). The fix is not
//! documentation; it is that the two causes are **separately observable**:
//! `target_version` counts one, the price series counts the other, and no gate
//! lets them blur.
//!
//! The mutations this gate must catch are M4 (progress clamped to `[0,1]`) and
//! M5 (a reclassification bumping `target_version`) — see `01-20` §8.1.

use core_domain::leg::{Classification, Direction, LegKey, StructureLevel, TargetBasis};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy};

fn key(level: StructureLevel) -> LegKey {
    LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: level,
    }
}

fn machine() -> LegMachine {
    LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0))
}

/// A confirmed, referenced long leg: origin 100, target 110.
fn opened(m: &mut LegMachine) -> LegKey {
    let k = key(StructureLevel::Major);
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
fn cg3_a_widening_target_lowers_progress_with_no_price_movement() {
    let mut m = machine();
    let k = opened(&mut m);

    // Observe once, so the leg's extreme is a real observation rather than its
    // origin. Every price from here on is 105: nothing moves but the reference.
    m.observe(&k, 105.0, 16);
    let before = m.progress(&k, 105.0).value().expect("defined at 105");
    let price_before = 105.0;

    assert!(
        m.revise_target(&k, 140.0, 20, TargetBasis::Extension, "extended"),
        "the target really did move"
    );

    let after = m.progress(&k, price_before).value().expect("still defined");
    assert!(
        after < before,
        "progress must fall when the reference moves away: {after} !< {before}"
    );

    // And no movement was fabricated: the leg's extreme is the price, and the
    // price never moved.
    let leg = m.leg(&k).unwrap();
    assert_eq!(leg.extreme_price, price_before, "the price did not move");
    assert_eq!(leg.movement_since(price_before, price_before), 0.0);
}

#[test]
fn cg3_progress_is_never_clamped_to_the_unit_interval() {
    let mut m = machine();
    let k = opened(&mut m);

    // Well past the target.
    let far = m.progress(&k, 160.0).value().expect("defined");
    assert!(far > 1.0, "progress must overrun: {far}");

    // Back against the leg: negative, not clamped to zero.
    let back = m.progress(&k, 97.0).value().expect("defined");
    assert!(back < 0.0, "progress must go negative: {back}");

    // And the two are distinguishable — which is the entire point of §4.1.
    assert_ne!(
        far, 1.0,
        "clamped progress cannot tell 'at the target' from 'well past it'"
    );
}

#[test]
fn cg3_an_unstable_reference_promotes_no_stage() {
    let mut m = machine();
    let k = opened(&mut m);

    // Three unrevised bars → stable → a stage is publishable.
    for bar in 16..=18 {
        m.observe(&k, 106.0, bar);
    }
    assert!(m.leg(&k).unwrap().target_stable);
    assert!(
        m.stage(&k, 106.0).is_some(),
        "a stable, confirmed leg publishes a stage"
    );

    // A revision revokes it immediately, and the stage is withheld again — even
    // though the progress value is still perfectly defined.
    assert!(m.revise_target(&k, 120.0, 19, TargetBasis::Measured, "widened"));
    assert!(
        m.progress(&k, 106.0).value().is_some(),
        "progress is still a number — only the STAGE is withheld"
    );
    assert!(
        m.stage(&k, 106.0).is_none(),
        "L10: stage promotion is blocked while the reference is moving"
    );
}

#[test]
fn cg3_a_reclassification_does_not_bump_the_target_counter() {
    // M5 lives here: if a reclassification bumped `target_version`, `progress`
    // would move with no price movement AND no target movement — a third
    // revision source, precisely the ambiguity 01-14 §4.3 exists to eliminate.
    let mut m = machine();
    let k = opened(&mut m);
    let tv_before = m.leg(&k).unwrap().target_version;
    let sv_before = m.leg(&k).unwrap().structure_version;

    assert!(m.reclassify(&k, StructureLevel::Minor));

    let moved = LegKey {
        structure_level: StructureLevel::Minor,
        ..k.clone()
    };
    let leg = m.leg(&moved).unwrap();
    assert_eq!(
        leg.target_version, tv_before,
        "S-N5: reclassification is not retargeting"
    );
    assert_eq!(
        leg.structure_version,
        sv_before + 1,
        "S-N5: it is a structure revision"
    );
}

#[test]
fn cg3_the_two_revision_sources_move_the_two_counters_apart() {
    let mut m = machine();
    let k = opened(&mut m);

    let (t0, s0) = {
        let l = m.leg(&k).unwrap();
        (l.target_version, l.structure_version)
    };
    m.revise_target(&k, 111.0, 20, TargetBasis::Volatility, "t");
    let (t1, s1) = {
        let l = m.leg(&k).unwrap();
        (l.target_version, l.structure_version)
    };
    m.revise_origin(&k, 99.0, 21);
    let (t2, s2) = {
        let l = m.leg(&k).unwrap();
        (l.target_version, l.structure_version)
    };

    assert_eq!((t0, s0), (0, 0));
    assert_eq!((t1, s1), (1, 0), "a retarget moves the target counter only");
    assert_eq!(
        (t2, s2),
        (1, 1),
        "an origin revision moves the structure counter only"
    );
}

#[test]
fn cg3_the_revision_log_records_which_counter_moved() {
    let mut m = machine();
    let k = opened(&mut m);
    m.revise_target(&k, 112.0, 20, TargetBasis::Volatility, "widened");
    m.revise_origin(&k, 99.5, 21);

    let counters: Vec<&str> = m
        .leg(&k)
        .unwrap()
        .revisions
        .iter()
        .map(|r| match r.counter {
            core_domain::leg::RevisionCounter::Structure => "structure",
            core_domain::leg::RevisionCounter::Target => "target",
        })
        .collect();
    // Three entries: the reference being STATED (the first target), then the
    // retarget, then the origin revision. The first is logged as a target entry
    // because it moved `target_price`; it is not counted against the revision
    // budget, because the budget measures a reference that cannot settle.
    assert_eq!(
        counters,
        vec!["target", "target", "structure"],
        "every log entry names which counter it bumped, so P1 can be checked per revision"
    );
}

#[test]
fn cg3_progress_is_undefined_rather_than_zero_when_the_reference_is_degenerate() {
    // L13, and the reason CG3 needs CG6: a degenerate denominator must withhold,
    // not substitute.
    let mut m = machine();
    let k = opened(&mut m);
    // A target at the origin is degenerate for every floor.
    assert!(m.revise_target(&k, 100.0, 20, TargetBasis::Measured, "collapsed"));
    let reading = m.progress(&k, 105.0);
    assert!(
        reading.is_undefined(),
        "a degenerate reference yields a typed absence, not 0.0"
    );
    assert_eq!(reading.value(), None);
    assert!(
        m.stage(&k, 105.0).is_none(),
        "and no stage comes from an undefined progress"
    );
}
