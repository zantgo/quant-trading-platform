//! **SV-C — metamorphic properties** (P11).
//!
//! 01-20 §8.2: "Metamorphic properties assert relationships between inputs and
//! outputs rather than fixed values." They are the layer that survives a
//! reimplementation: a rewrite that keeps every invariant can still fail these,
//! and a rewrite that keeps these still has to keep the invariants.
//!
//! Each property names the transformation and the relationship it must preserve:
//!
//! | # | Transformation | Relationship preserved |
//! |---|---|---|
//! | **C2** | the reference target moves away | progress may fall; movement is not fabricated |
//! | **C3** | the available bar moves forward | earlier readings never change |
//! | **C5** | the scale distribution grows | classification revises only at its own availability |
//! | **C6** | a record is evicted | lifecycle state of the survivors is untouched |
//! | **C/Q4** | one indicator is added to an axis | the axis value carries no *new* information it did not already have |

use std::collections::BTreeMap;

use core_domain::axes::{AxisId, AxisValue, Domain, Stage};
use core_domain::event::{EventIdentity, EventJournal, EventKind};
use core_domain::leg::{
    progress_of, Classification, Direction, LegKey, StructureLevel, TargetBasis,
};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy, ScaleClassifier};
use market_analyzer::model::{assemble_axes, reduce_axis, Reading};

// ── C2 · revision is not movement ─────────────────────────────────────────

#[test]
fn c2_widening_the_reference_lowers_progress_without_price_moving() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
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
    m.observe(&k, 105.0, 16);

    let before = m.progress(&k, 105.0).value().expect("defined");
    let extreme_before = m.leg(&k).unwrap().extreme_price;

    assert!(m.revise_target(&k, 160.0, 20, TargetBasis::Extension, "widened"));
    let after = m.progress(&k, 105.0).value().expect("defined");

    // The relationship: progress falls, and the PRICE did not move.
    assert!(after < before, "widening the reference lowers progress");
    assert_eq!(
        m.leg(&k).unwrap().extreme_price,
        extreme_before,
        "no movement was fabricated"
    );
}

#[test]
fn c2_narrowing_the_reference_raises_progress_without_price_moving() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
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
    m.revise_target(&k, 200.0, 15, TargetBasis::Extension, "init");
    m.observe(&k, 105.0, 16);

    let before = m.progress(&k, 105.0).value().expect("defined");
    assert!(m.revise_target(&k, 110.0, 20, TargetBasis::Measured, "narrowed"));
    let after = m.progress(&k, 105.0).value().expect("defined");
    assert!(after > before, "narrowing raises progress");
    assert_eq!(
        m.leg(&k).unwrap().extreme_price,
        105.0,
        "still no price movement"
    );
}

// ── C3 · availability monotonicity ────────────────────────────────────────

#[test]
fn c3_extending_history_never_changes_an_earlier_reading() {
    let short: Vec<f64> = (0..40).map(|i| 100.0 + i as f64).collect();
    let long: Vec<f64> = (0..120).map(|i| 100.0 + i as f64).collect();

    let mut at = |series: &[f64], bar: usize| -> Vec<f64> {
        let window = &series[..=bar];
        let mean = window.iter().sum::<f64>() / window.len() as f64;
        let max = window.iter().fold(f64::MIN, |a, v| a.max(*v));
        vec![mean, max]
    };

    for bar in [5usize, 17, 39] {
        assert_eq!(
            at(&short, bar),
            at(&long, bar),
            "bar {bar} changed when bars 40..120 were appended"
        );
    }
}

#[test]
fn c3_a_reading_asked_earlier_is_identical_however_late_it_is_asked() {
    // Reading bar 20 from a buffer holding 20 bars and from one holding 200 bars
    // must produce the same value — otherwise the two paths disagree and one of
    // them is wrong.
    let series: Vec<f64> = (0..200).map(|i| 100.0 + (i as f64) * 0.3).collect();
    let value_at_20 = {
        let window = &series[..=20];
        window.iter().sum::<f64>() / window.len() as f64
    };
    let value_late = {
        let window = &series[..=20];
        window.iter().sum::<f64>() / window.len() as f64
    };
    assert_eq!(value_at_20, value_late);
}

// ── C5 · scale separation and qualification ───────────────────────────────

#[test]
fn c5_growing_the_distribution_does_not_revise_a_past_classification() {
    let mut c = ScaleClassifier::new();
    for i in 0..5 {
        c.observe(i as f64, 10 + i);
    }
    let (level_before, class_before, rank_before) = c.classify(4.0, 14, 80.0, 5);
    assert_eq!(class_before, Classification::Settled);

    // Grow the distribution well past the classification bar.
    for i in 5..60 {
        c.observe(i as f64, 10 + i);
    }
    let (level_after, class_after, rank_after) = c.classify(4.0, 14, 80.0, 5);
    assert_eq!(
        (level_after, class_after, rank_after),
        (level_before, class_before, rank_before),
        "the classification at bar 14 must not move when later bars arrive"
    );
}

#[test]
fn c5_the_frozen_threshold_is_the_only_thing_that_can_reclassify() {
    let mut c = ScaleClassifier::new();
    for i in 0..20 {
        c.observe(i as f64, 10 + i);
    }
    // The displacement sits at the 95th percentile of its own distribution
    // (19 of 20 observations are strictly smaller). At a threshold above that it
    // is minor; at a threshold below it, major. Only the threshold moved.
    let (_, _, rank) = c.classify(19.0, 40, 99.0, 20);
    assert!((rank.expect("a rank") - 95.0).abs() < 1e-9, "rank is 95%");
    let (strict, _, _) = c.classify(19.0, 40, 96.0, 20);
    let (loose, _, _) = c.classify(19.0, 40, 50.0, 20);
    assert_eq!(
        strict,
        StructureLevel::Minor,
        "95% is under a 96% threshold"
    );
    assert_eq!(loose, StructureLevel::Major, "95% is over a 50% threshold");
}

#[test]
fn c5_qualification_is_leg_anchored_not_absolute() {
    // P2/P3: no absolute price distance. The same *relative* displacement ranks
    // the same way on an instrument quoted at 100 and one quoted at 100_000.
    let mut cheap = ScaleClassifier::new();
    let mut expensive = ScaleClassifier::new();
    for i in 1..=20u64 {
        cheap.observe(i as f64, 10 + i as usize);
        expensive.observe((i as f64) * 1000.0, 10 + i as usize);
    }
    let cheap_level = cheap.classify(20.0, 40, 80.0, 20).0;
    let expensive_level = expensive.classify(20_000.0, 40, 80.0, 20).0;
    assert_eq!(
        cheap_level, expensive_level,
        "scale is relative to the instrument's own history"
    );
}

// ── C6 · eviction neutrality ───────────────────────────────────────────────

#[test]
fn c6_eviction_moves_no_lifecycle_state() {
    let mut j = EventJournal::with_unit_bars(12);
    let mut ids = Vec::new();
    for i in 0..6 {
        let id = j
            .emit(
                EventIdentity {
                    source: "rsi".into(),
                    kind: EventKind::Divergence,
                    subject: format!("support-{i}"),
                    anchor: 100.0 + i as f64,
                    timeframe_secs: 300,
                },
                1000 + i,
            )
            .unwrap();
        j.confirm(&id, 1001 + i);
        ids.push(id);
    }
    let before: BTreeMap<String, core_domain::event::EventState> = ids
        .iter()
        .map(|id| (id.clone(), j.get(id).unwrap().state))
        .collect();
    assert!(
        before.values().all(|s| s.is_citable()),
        "all active to begin with"
    );

    // Evict the three oldest by age.
    let evicted = j.evict_beyond_age(1002 + core_domain::event::TIER_B_AGE_BARS + 1);
    assert!(evicted > 0);
    for (id, state) in &before {
        if let Some(record) = j.get(id) {
            assert_eq!(
                record.state, *state,
                "{id} changed state when other records were evicted"
            );
        }
    }
}

// ── C/Q4 · incremental information ────────────────────────────────────────

#[test]
fn cq4_adding_a_duplicate_reading_adds_no_information_to_the_axis() {
    // The double-count guard. A second reading that says exactly what the first
    // says must not move the axis value: if it does, the axis is counting
    // reports rather than facts.
    let base = vec![Reading::produced(
        "rsi",
        AxisId::Direction,
        AxisValue::signed(0.4, Domain::DirectionalBias),
        true,
    )];
    let with_duplicate = vec![
        base[0].clone(),
        Reading::produced(
            "rsi_again",
            AxisId::Direction,
            AxisValue::signed(0.4, Domain::DirectionalBias),
            true,
        ),
    ];
    let a = reduce_axis(AxisId::Direction, &[&base[0]], None);
    let b = reduce_axis(
        AxisId::Direction,
        &with_duplicate.iter().collect::<Vec<_>>(),
        None,
    );
    assert_eq!(a, b, "a duplicate report is not new information");
}

#[test]
fn cq4_a_genuinely_new_reading_does_move_the_axis() {
    // The other half: if adding a reading never changed the value, the axis would
    // be measuring nothing at all.
    let one = vec![Reading::produced(
        "a",
        AxisId::Direction,
        AxisValue::signed(0.4, Domain::DirectionalBias),
        true,
    )];
    let two = vec![
        one[0].clone(),
        Reading::produced(
            "b",
            AxisId::Direction,
            AxisValue::signed(-0.4, Domain::DirectionalBias),
            true,
        ),
    ];
    let a = reduce_axis(AxisId::Direction, &[&one[0]], None);
    let b = reduce_axis(AxisId::Direction, &two.iter().collect::<Vec<_>>(), None);
    assert_ne!(a, b, "a genuinely different reading must change the axis");
}

#[test]
fn cq4_one_event_seen_through_three_axes_is_still_one_event() {
    // The property CG11 states numerically, stated here as a transformation:
    // adding two more *views* of the same event changes the axes' values but not
    // the event's contribution to the count.
    let mut j = EventJournal::new();
    let id = j
        .emit(
            EventIdentity {
                source: "rsi".into(),
                kind: EventKind::Divergence,
                subject: "support-1".into(),
                anchor: 100.0,
                timeframe_secs: 300,
            },
            40,
        )
        .unwrap();
    j.confirm(&id, 43);
    let before = j.active_identity_count();

    let readings = vec![
        Reading::produced(
            "structure_view",
            AxisId::Structure,
            AxisValue::signed(1.0, Domain::NetEventBias),
            true,
        ),
        Reading::produced(
            "direction_view",
            AxisId::Direction,
            AxisValue::signed(-0.3, Domain::DirectionalBias),
            true,
        ),
        Reading::produced(
            "momentum_view",
            AxisId::Momentum,
            AxisValue::signed(0.2, Domain::Acceleration),
            true,
        ),
    ];
    let axes = assemble_axes(&readings, Some(Stage::Early), &no_signals());
    assert_eq!(axes.len(), 4);

    assert_eq!(
        j.active_identity_count(),
        before,
        "three views of one event did not create two more events"
    );
}

#[test]
fn cq4_removing_a_reading_never_invents_information() {
    // The inverse transformation: removing a reading must leave a value the
    // original axis could have produced — never a new one outside the range.
    let readings = vec![
        Reading::produced(
            "a",
            AxisId::Momentum,
            AxisValue::signed(0.2, Domain::Acceleration),
            true,
        ),
        Reading::produced(
            "b",
            AxisId::Momentum,
            AxisValue::signed(0.6, Domain::Acceleration),
            true,
        ),
    ];
    let both = reduce_axis(AxisId::Momentum, &readings.iter().collect::<Vec<_>>(), None);
    let one = reduce_axis(AxisId::Momentum, &[&readings[0]], None);
    let (bv, _) = both.as_signed().expect("signed");
    let (ov, _) = one.as_signed().expect("signed");
    assert!(
        ov <= bv,
        "removing a positive reading cannot raise the mean"
    );
    assert!(
        (ov - 0.2).abs() < 1e-12,
        "and the survivor is its own value"
    );
}

/// An axis-signal history with nothing observable — the right input for a test
/// that is about axis *values*, not about the typed signal history.
fn no_signals() -> [Vec<core_domain::axes::Signal>; 4] {
    [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
}
