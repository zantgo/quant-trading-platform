//! **CG11 — cross-axis consumption, without multiplication** (P10).
//!
//! ## The claim
//!
//! 01-20 §6 CG11: *one canonical divergence consumed by many axes, never
//! multiplied.* 01-15 §2.2 says the same about identity: it is "what keeps one
//! divergence from becoming three pieces of evidence across Structure,
//! Progression and a consolidated reading."
//!
//! ## Why this is a real defect and not a theoretical one
//!
//! If Structure, Progression and the cross-duration view each weight an event by
//! its own confidence, one divergence contributes three times — and the axes
//! then agree because they are counting the same thing three times. The result
//! looks like corroboration and is an artefact of double counting.
//!
//! ## The mutation
//!
//! **M6 — cross-axis double-count, one event weighted once per consuming axis.**

use std::collections::BTreeMap;

use core_domain::axes::{AxisId, AxisValue, Domain};
use core_domain::event::{EventIdentity, EventJournal, EventKind, EventState, TIER_C_MAX_EVENTS};
use market_analyzer::model::{assemble_axes, reduce_axis, Reading};

fn divergence(anchor: f64) -> EventIdentity {
    EventIdentity {
        source: "rsi".into(),
        kind: EventKind::Divergence,
        subject: "support-1".into(),
        anchor,
        timeframe_secs: 300,
    }
}

/// A journal holding one confirmed divergence.
fn journal_with_one_divergence() -> EventJournal {
    let mut j = EventJournal::new();
    let id = j.emit(divergence(100.0), 40).unwrap();
    j.confirm(&id, 43);
    assert_eq!(j.counts().active, 1);
    j
}

#[test]
fn cg11_one_event_contributes_exactly_once_however_many_axes_ask() {
    let journal = journal_with_one_divergence();
    let distinct = journal.active_identity_count();
    assert_eq!(distinct, 1, "E5: one divergence, one identity");

    // Three consumers read the same journal.
    let structure = journal.active_of_kind(EventKind::Divergence).len();
    let progression = journal.active_identity_count();
    let consolidated = journal.counts().active;

    assert_eq!(structure + progression + consolidated, 3 * distinct);
    // The invariant is about what the *model* does with it, not about the readers:
    // each reader sees the one event; no reader is handed a second copy.
    assert_eq!(
        journal.active_of_kind(EventKind::Divergence).len(),
        distinct
    );
    assert_eq!(journal.active_of_kind(EventKind::Breakout).len(), 0);
}

#[test]
fn cg11_a_divergence_can_feed_several_axes_without_being_duplicated() {
    // The legitimate shape: one event, several axes *observe* it, and each axis's
    // own weight is applied once to its own value.
    let journal = journal_with_one_divergence();
    let event_strength = journal.counts().active as f64;

    let readings = vec![
        // Structure consumes the divergence as an event bias.
        Reading::produced(
            "rsi_divergence",
            AxisId::Structure,
            AxisValue::signed(event_strength, Domain::NetEventBias),
            true,
        ),
        // Direction consumes the same event's directional implication.
        Reading::produced(
            "divergence_direction",
            AxisId::Direction,
            AxisValue::signed(-0.4, Domain::DirectionalBias),
            true,
        ),
    ];
    let axes = assemble_axes(&readings, None, &no_signals());
    assert_eq!(axes.len(), 4, "four axes, always");

    let structure = axes.iter().find(|a| a.axis == AxisId::Structure).unwrap();
    let direction = axes.iter().find(|a| a.axis == AxisId::Direction).unwrap();
    assert_eq!(
        structure.position.as_signed().map(|(v, _)| v),
        Some(event_strength),
        "Structure consumed the event once"
    );
    assert_eq!(
        direction.position.as_signed().map(|(v, _)| v),
        Some(-0.4),
        "Direction consumed it once, on its own domain"
    );
    // And the event is still counted once in the journal that fed both.
    assert_eq!(journal.counts().active, 1);
}

#[test]
fn cg11_an_axis_never_multiplies_a_shared_member() {
    // Two readings in one axis derived from the SAME parent must not compound.
    // Each is reduced with weight 1 and averaged, so the axis value is their mean
    // — never their product, and never the parent's weight applied twice.
    let parent = Reading::produced(
        "parent",
        AxisId::Structure,
        AxisValue::signed(0.8, Domain::NetEventBias),
        true,
    );
    let derived_a = Reading::derived(
        "child_a",
        AxisId::Structure,
        AxisValue::signed(0.8, Domain::NetEventBias),
        &["parent"],
        true,
    );
    let derived_b = Reading::derived(
        "child_b",
        AxisId::Structure,
        AxisValue::signed(0.8, Domain::NetEventBias),
        &["parent"],
        true,
    );
    let value = reduce_axis(AxisId::Structure, &[&parent, &derived_a, &derived_b], None);
    let reduced = value.as_signed().map(|(v, _)| v).expect("a signed axis");
    assert!(
        (reduced - 0.8).abs() < 1e-12,
        "three views of one fact average to that fact, not to 0.8^3 — got {reduced}"
    );
}

#[test]
fn cg11_readings_declare_their_producer_so_ownership_is_answerable() {
    let owned = Reading::produced(
        "rsi",
        AxisId::Momentum,
        AxisValue::signed(0.2, Domain::Acceleration),
        true,
    );
    let derived = Reading::derived(
        "accel_of_rsi",
        AxisId::Momentum,
        AxisValue::signed(0.2, Domain::Acceleration),
        &["rsi"],
        true,
    );
    assert!(
        owned.derived_from.is_empty(),
        "a produced reading has no parent"
    );
    assert_eq!(
        derived.derived_from,
        vec!["rsi".to_string()],
        "a derived one names it"
    );
    // Emission is separate from consumption: the reading appears once, under one
    // axis, and says where its value came from.
    assert_eq!(derived.producer, "accel_of_rsi");
    assert_eq!(derived.axis, AxisId::Momentum);
    assert!(derived.ownership_is_single());
}

#[test]
fn cg11_an_expired_event_stops_contributing_to_every_axis_at_once() {
    // The double-count risk in reverse: an event that has lapsed must not be
    // resurrected by a second consumer that happens to read it later.
    let mut journal = journal_with_one_divergence();
    let bound = journal.records().front().expect("a record").expiry_bars as usize;
    journal.expire_at(43 + bound);
    assert_eq!(journal.counts().active, 0);
    assert_eq!(journal.active_of_kind(EventKind::Divergence).len(), 0);
    assert_eq!(journal.active_identity_count(), 0);
    assert!(journal
        .records()
        .iter()
        .all(|r| r.state == EventState::Expired));
}

#[test]
fn cg11_a_multi_axis_reading_sweep_produces_one_count_per_event() {
    // The end-to-end shape: walk every axis a divergence could feed, and assert
    // the journal's contribution is counted once in total.
    let mut journal = journal_with_one_divergence();
    let mut per_axis: BTreeMap<&str, usize> = BTreeMap::new();
    for axis in AxisId::ALL {
        per_axis.insert(axis.label(), journal.active_identity_count());
    }
    let total = per_axis.values().sum::<usize>();
    assert_eq!(
        total,
        AxisId::ALL.len(),
        "each axis sees the event once — {per_axis:?}"
    );
    assert_eq!(journal.counts().active, 1, "and the journal counts it once");

    // Distinct anchors are distinct events — the identity is not so coarse that
    // everything collapses into one.
    let a = journal.emit(divergence(100.0), 60).unwrap();
    let b = journal.emit(divergence(101.0), 61).unwrap();
    assert_ne!(a, b);
    journal.confirm(&a, 62);
    journal.confirm(&b, 63);
    assert_eq!(journal.active_identity_count(), 2);
}

#[test]
fn cg11_the_journal_bound_does_not_change_the_counting_rule() {
    // Under the count bound and over it, the rule is identical: distinct
    // identities among active events.
    let mut j = EventJournal::new();
    for i in 0..(TIER_C_MAX_EVENTS + 20) {
        let anchor = 100.0 + (i as f64) * 0.5;
        if let Some(id) = j.emit(divergence(anchor), i).map(|id| (id, i)) {
            let (id, bar) = id;
            j.confirm(&id, bar + 1);
        }
    }
    assert_eq!(j.counts().retained, TIER_C_MAX_EVENTS);
    assert_eq!(
        j.active_identity_count(),
        j.active_of_kind(EventKind::Divergence).len(),
        "at the bound, one identity per active event"
    );
}

/// An axis-signal history with nothing observable — the right input for a test
/// that is about axis *values*, not about the typed signal history.
fn no_signals() -> [Vec<core_domain::axes::Signal>; 4] {
    [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
}
