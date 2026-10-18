//! **SV-A — reference cases** (P11).
//!
//! 01-20 §8.2: "Reference cases establish *specified* behaviour — one known input,
//! one known output, derived from the document rather than from the code."
//!
//! The gate suite proves properties hold. It cannot prove they hold for the
//! *right reason*. SV-A is the counterweight: hand-computed cases where the
//! expected value was derived from the spec text, so a regression that changes the
//! answer has to argue with the specification rather than with the previous
//! output.

use core_domain::axes::{AxisId, AxisValue, Domain, Stage};
use core_domain::event::{EventIdentity, EventJournal, EventKind, EventState};
use core_domain::leg::{
    progress_of, stage_from_progress, Classification, Direction, LegKey, StructureLevel,
};
use market_analyzer::model::{assess_terminality, reduce_axis, reduce_direction, Reading};

/// `(P - O) / (T - O)` on hand-chosen numbers.
///
/// | Case | O | P | T | Result |
/// |---|---|---|---|---|
/// | at the origin | 100 | 100 | 110 | 0.0 |
/// | at the target | 100 | 110 | 110 | 1.0 |
/// | half way | 100 | 105 | 110 | 0.5 |
/// | short, half way down | 100 | 95 | 90 | 0.5 |
/// | overrun | 100 | 132 | 110 | 3.2 |
/// | against the leg | 100 | 96 | 110 | −0.4 |
#[test]
fn sv_a_progress_matches_the_hand_computed_values() {
    let cases = [
        (100.0, 100.0, 110.0, Some(0.0)),
        (100.0, 110.0, 110.0, Some(1.0)),
        (100.0, 105.0, 110.0, Some(0.5)),
        (100.0, 95.0, 90.0, Some(0.5)),
        (100.0, 132.0, 110.0, Some(3.2)),
        (100.0, 96.0, 110.0, Some(-0.4)),
    ];
    for (o, p, t, expected) in cases {
        let got = progress_of(o, p, Some(t), 0.0).value();
        let expected = expected.expect("the case is defined");
        assert!(
            (got.expect("a value") - expected).abs() < 1e-12,
            "O={o} P={p} T={t}: expected {expected}, got {got:?}"
        );
    }
}

#[test]
fn sv_a_the_stage_bands_match_the_specification() {
    // §4.4: early / early_mid / mid / late / terminal, in that order.
    assert_eq!(stage_from_progress(0.0), Stage::Early);
    assert_eq!(stage_from_progress(0.24), Stage::Early);
    assert_eq!(stage_from_progress(0.25), Stage::EarlyMid);
    assert_eq!(stage_from_progress(0.74), Stage::EarlyMid);
    assert_eq!(stage_from_progress(0.75), Stage::Mid);
    assert_eq!(stage_from_progress(1.0), Stage::Mid);
    assert_eq!(stage_from_progress(1.01), Stage::Late);
    assert_eq!(stage_from_progress(1.5), Stage::Late);
    assert_eq!(stage_from_progress(1.51), Stage::Terminal);
}

#[test]
fn sv_a_the_agreement_share_matches_the_specification() {
    // agreement_pct = the share of durations whose sign matches the mean.
    let (d, _) = reduce_direction(&[
        (1, AxisValue::signed(-0.5, Domain::DirectionalBias)),
        (60, AxisValue::signed(-0.3, Domain::DirectionalBias)),
        (3600, AxisValue::signed(-0.1, Domain::DirectionalBias)),
    ]);
    assert!(
        (d.mean_bias - (-0.3)).abs() < 1e-12,
        "the mean of three negatives"
    );
    assert_eq!(
        d.agreement_pct, 100.0,
        "all three agree with a negative mean"
    );
    assert_eq!(d.dissenting_durations.len(), 0);

    let (mixed, _) = reduce_direction(&[
        (1, AxisValue::signed(0.8, Domain::DirectionalBias)),
        (60, AxisValue::signed(-0.4, Domain::DirectionalBias)),
        (3600, AxisValue::signed(0.2, Domain::DirectionalBias)),
    ]);
    // mean = 0.2; the 0.8 and the 0.2 agree, the −0.4 dissents.
    assert!((mixed.mean_bias - 0.2).abs() < 1e-12);
    assert_eq!(mixed.contributing_durations, vec![1, 3600]);
    assert_eq!(mixed.dissenting_durations, vec![60]);
    assert!((mixed.agreement_pct - (200.0 / 3.0)).abs() < 1e-9);
}

#[test]
fn sv_a_the_worked_divergence_example_holds_at_every_bar() {
    // 01-15 §8, transcribed as an assertion rather than as prose.
    let mut j = EventJournal::with_unit_bars(12);
    let identity = EventIdentity {
        source: "rsi".into(),
        kind: EventKind::Divergence,
        subject: "support-1".into(),
        anchor: 100.0,
        timeframe_secs: 3600,
    };

    let d1 = j.emit(identity.clone(), 40).expect("stored");
    assert_eq!(j.get(&d1).unwrap().state, EventState::Pending);
    assert_eq!(j.counts().active, 0, "bar 40: active count 0");

    assert!(j.confirm(&d1, 43));
    assert_eq!(j.counts().active, 1, "bar 43: active count 1");

    let bound = j.get(&d1).unwrap().expiry_bars as usize;
    assert_eq!(bound, 24, "2 x its own detection lookback");
    j.expire_at(43 + bound);
    assert_eq!(j.counts().active, 0, "bar 67: active count 0");
    assert_eq!(j.counts().retained, 1, "still held for history");

    let d2 = j.reactivate(&d1, 90).expect("a new canonical event");
    assert_ne!(d2, d1);
    assert_eq!(j.get(&d2).unwrap().supersedes.as_deref(), Some(d1.as_str()));
    assert_eq!(j.get(&d1).unwrap().state, EventState::Superseded);
    assert_eq!(j.counts().active, 0, "bar 90: the new event is pending");

    assert!(j.confirm(&d2, 93));
    assert_eq!(j.counts().active, 1, "bar 93: active count 1");
    assert_eq!(j.counts().emitted, 2);
}

#[test]
fn sv_a_terminality_states_match_the_specified_thresholds() {
    // 01-13: zero criteria and one criterion both leave the evidence pointing
    // nowhere; two named criteria pointing the same way are evidence.
    use market_analyzer::model::{CompletionCriterion, Terminality};

    assert_eq!(
        assess_terminality(&[]).terminality,
        Terminality::Indeterminate
    );
    assert_eq!(
        assess_terminality(&[CompletionCriterion::CharacterChange]).terminality,
        Terminality::Indeterminate
    );
    assert_eq!(
        assess_terminality(&[
            CompletionCriterion::CharacterChange,
            CompletionCriterion::ClimaxFailure
        ])
        .terminality,
        Terminality::TerminationEvidence
    );
}

#[test]
fn sv_a_the_axis_reduction_of_identical_members_is_that_member() {
    // Three readings at 0.4 reduce to 0.4, not to 0.4 + 0.4 + 0.4.
    let readings = vec![
        Reading::produced(
            "a",
            AxisId::Momentum,
            AxisValue::signed(0.4, Domain::Acceleration),
            true,
        ),
        Reading::produced(
            "b",
            AxisId::Momentum,
            AxisValue::signed(0.4, Domain::Acceleration),
            true,
        ),
        Reading::produced(
            "c",
            AxisId::Momentum,
            AxisValue::signed(0.4, Domain::Acceleration),
            true,
        ),
    ];
    let refs: Vec<&Reading> = readings.iter().collect();
    let value = reduce_axis(AxisId::Momentum, &refs, None);
    let v = value.as_signed().map(|(v, _)| v).expect("signed");
    assert!((v - 0.4).abs() < 1e-12, "an average is not a sum: {v}");
}

#[test]
fn sv_a_an_unreadable_leg_reports_absence_not_zero() {
    // The specification's own example of the wrong answer: `progress = 0` means
    // "at the origin", which is a claim.
    let leg = core_domain::leg::LegRecord::open(
        "L-0001",
        LegKey {
            instrument: "BTC-USDT".into(),
            timeframe_secs: 300,
            structure_level: StructureLevel::Major,
        },
        Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        0.5,
        80.0,
        1,
    );
    assert!(leg.progress(105.0).is_undefined());
    assert_eq!(leg.progress(105.0).value(), None);
    assert_eq!(leg.stage(105.0), None);
}
