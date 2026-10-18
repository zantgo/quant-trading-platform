//! **SV-B — adversarial cases** (P11).
//!
//! 01-20 §8.2: "Adversarial cases attempt to *break* the model with degenerate,
//! contradictory, or hostile input." Every case here is input a real market
//! produces at least once — a flat bar, a zero-range window, a spike, an
//! inverted series — and every one of them must produce a **typed** answer rather
//! than a plausible wrong number.

use core_domain::axes::{AxisId, AxisValue, Domain, Stage};
use core_domain::event::{EventIdentity, EventJournal, EventKind, EventState};
use core_domain::leg::{
    progress_of, stage_from_progress, Classification, Direction, LegKey, LegRecord, StructureLevel,
};
use market_analyzer::model::{assess_terminality, reduce_axis, reduce_direction, Reading};

use market_analyzer::model::{CompletionCriterion, Terminality};

fn reading(axis: AxisId, value: AxisValue) -> Reading {
    Reading::produced("hostile", axis, value, true)
}

// ── degenerate price inputs ────────────────────────────────────────────────

#[test]
fn sv_b_a_flat_series_produces_no_progress_not_an_infinite_one() {
    // Every price identical: the denominator is zero for any target.
    assert!(progress_of(100.0, 100.0, Some(100.0), 0.0).is_undefined());
    // And a non-flat target on a flat series still yields a defined ratio.
    let p = progress_of(100.0, 100.0, Some(110.0), 0.0);
    assert_eq!(
        p.value(),
        Some(0.0),
        "price at the origin is genuinely zero progress"
    );
}

#[test]
fn sv_b_the_noise_floor_boundary_is_below_not_at() {
    // "below the leg's own noise floor" is a strict comparison: a span exactly AT
    // the floor is the smallest displacement the leg can speak to, so it is
    // defined; one hair below it is a refusal.
    let floor = 1.0;
    let below = progress_of(100.0, 100.5, Some(100.9), floor);
    assert!(
        below.is_undefined(),
        "a span of 0.9 under a floor of 1.0 is a refusal"
    );

    let exactly = progress_of(100.0, 100.5, Some(101.0), floor);
    assert_eq!(
        exactly.value(),
        Some(0.5),
        "a span exactly AT the floor is defined — the floor bounds what the leg \
         can speak to, it does not exclude it"
    );
}

// ── contradictory readings ────────────────────────────────────────────────

#[test]
fn sv_b_two_readings_on_different_domains_do_not_average() {
    let readings = vec![
        reading(
            AxisId::Direction,
            AxisValue::signed(0.9, Domain::DirectionalBias),
        ),
        reading(
            AxisId::Direction,
            AxisValue::signed(0.9, Domain::Acceleration),
        ),
    ];
    let refs: Vec<&Reading> = readings.iter().collect();
    let value = reduce_axis(AxisId::Direction, &refs, None);
    assert!(
        value.is_undefined(),
        "0.9 of one meaning + 0.9 of another is not 0.9"
    );
}

#[test]
fn sv_b_a_direction_row_of_all_zeroes_reports_zero_not_a_crash() {
    let readings = vec![
        reading(
            AxisId::Direction,
            AxisValue::signed(0.0, Domain::DirectionalBias),
        ),
        reading(
            AxisId::Direction,
            AxisValue::signed(0.0, Domain::DirectionalBias),
        ),
    ];
    let refs: Vec<&Reading> = readings.iter().collect();
    let value = reduce_axis(AxisId::Direction, &refs, None);
    assert_eq!(value.as_signed().map(|(v, _)| v), Some(0.0));
    // And an all-zero row has no majority: agreement is defined, not assumed.
    let (d, _) = reduce_direction(&[
        (1, AxisValue::signed(0.0, Domain::DirectionalBias)),
        (60, AxisValue::signed(0.0, Domain::DirectionalBias)),
    ]);
    assert_eq!(d.mean_bias, 0.0);
    assert!(
        d.contributing_durations.is_empty(),
        "zero is neither bullish nor bearish, so it joins neither camp"
    );
}

#[test]
fn sv_b_a_single_duration_has_no_gradient_and_no_fast_vs_slow() {
    let (momentum, _) = market_analyzer::model::reduce_momentum(&[(
        60,
        AxisValue::signed(0.5, Domain::Acceleration),
    )]);
    assert_eq!(momentum.gradient, 0.0, "one point has no slope");
    assert!(
        !momentum.fast_vs_slow.divergent,
        "a duration cannot diverge from itself"
    );
}

#[test]
fn sv_b_an_empty_axis_is_typed_absence_not_a_default_zero() {
    let value = reduce_axis(AxisId::Direction, &[], None);
    assert!(value.is_undefined(), "no readings is not 'zero readings'");
    assert!(
        matches!(value, AxisValue::Undefined { .. }),
        "an empty axis renders as an absent value with a reason, never as 0"
    );
}

// ── hostile event input ───────────────────────────────────────────────────

#[test]
fn sv_b_an_event_at_a_negative_anchor_does_not_panic() {
    let mut j = EventJournal::new();
    let id = j.emit(
        EventIdentity {
            source: "rsi".into(),
            kind: EventKind::Divergence,
            subject: "support-1".into(),
            anchor: -5.0,
            timeframe_secs: 60,
        },
        10,
    );
    assert!(id.is_some());
    assert!(j.confirm(&id.unwrap(), 11));
    assert_eq!(j.counts().active, 1);
}

#[test]
fn sv_b_a_journal_flooded_past_its_bound_reports_the_bound() {
    let mut j = EventJournal::new();
    let mut stored = 0;
    for i in 0..1000 {
        let id = j.emit(
            EventIdentity {
                source: "flood".into(),
                kind: EventKind::LevelTest,
                subject: format!("l{i}"),
                anchor: i as f64,
                timeframe_secs: 60,
            },
            i,
        );
        if id.is_some() {
            stored += 1;
        }
    }
    assert_eq!(
        stored,
        core_domain::event::TIER_C_MAX_EVENTS,
        "the bound held"
    );
    assert_eq!(j.counts().retained, core_domain::event::TIER_C_MAX_EVENTS);
}

#[test]
fn sv_b_operations_on_an_unknown_id_are_refusals() {
    let mut j = EventJournal::new();
    assert!(!j.confirm("D-9999", 1));
    assert!(!j.persist("D-9999", 1));
    assert!(!j.invalidate("D-9999", "X-1"));
    assert!(!j.resolve("D-9999"));
    assert!(j.reactivate("D-9999", 1).is_none());
    assert_eq!(j.counts().active, 0);
}

#[test]
fn sv_b_a_bars_supplied_out_of_order_never_regresses_a_state() {
    let mut j = EventJournal::with_unit_bars(12);
    let id = j
        .emit(
            EventIdentity {
                source: "rsi".into(),
                kind: EventKind::Breakout,
                subject: "level-1".into(),
                anchor: 110.0,
                timeframe_secs: 60,
            },
            100,
        )
        .unwrap();
    j.confirm(&id, 110);
    let bound = j.get(&id).unwrap().expiry_bars as usize;
    // Walk time backwards. Nothing may become MORE alive than it was.
    for bar in (0..110).rev() {
        j.expire_at(bar);
    }
    assert!(
        matches!(
            j.get(&id).unwrap().state,
            EventState::Active | EventState::Expired | EventState::Invalidated
        ),
        "the state is one of the six, never something new"
    );
    // And the current bar is the arbiter: asking about bar 200 finds it expired.
    j.expire_at(200);
    assert_eq!(j.get(&id).unwrap().state, EventState::Expired);
    let _ = bound;
}

// ── contradictory terminality input ───────────────────────────────────────

#[test]
fn sv_b_criteria_counted_twice_do_not_manufacture_evidence() {
    let duplicated = assess_terminality(&[
        CompletionCriterion::CharacterChange,
        CompletionCriterion::CharacterChange,
    ]);
    assert_eq!(
        duplicated.terminality,
        Terminality::Indeterminate,
        "the same criterion twice is one criterion"
    );
    assert_eq!(duplicated.criteria.len(), 1, "and the log says so");
}

#[test]
fn sv_b_a_leg_with_no_reference_publishes_nothing_rather_than_a_default() {
    let leg = LegRecord::open(
        "L-0002",
        LegKey {
            instrument: "BTC-USDT".into(),
            timeframe_secs: 60,
            structure_level: StructureLevel::Minor,
        },
        Direction::Short,
        100.0,
        0,
        3,
        Classification::Provisional,
        0.5,
        80.0,
        1,
    );
    assert!(
        leg.progress(95.0).is_undefined(),
        "no reference, no progress"
    );
    assert_eq!(leg.stage(95.0), None, "and no stage");
    assert!(
        leg.invalidation_price.is_none(),
        "L9: set before active, not before forming"
    );
}
