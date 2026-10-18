//! **CG8 — the event lifecycle, and non-reviving reactivation** (P9).
//!
//! ## The claim
//!
//! 01-20 §6 CG8: *Activate → expire → re-trigger evidence; old stays expired,
//! new identity with `supersedes`.* 01-15 §2 makes the reason explicit:
//! "Reactivating a record would let one divergence be counted twice — once before
//! it lapsed, once after — and would make the count meaningless."
//!
//! ## The five transitions, and the one that must not exist
//!
//! Activation · Persistence · Expiration · Invalidation · Reactivation.
//! There is no **revival**: nothing moves a record from `expired` or
//! `invalidated` back to `active`. The only way back to `active` is a *new*
//! record.
//!
//! ## The mutation
//!
//! **M2 — event expiry suppressed, absence of contradiction treated as
//! persistence.** It is caught here and by CG9.

use core_domain::event::{
    EventCounts, EventIdentity, EventJournal, EventKind, EventRecord, EventState, TIER_B_AGE_BARS,
};

fn identity(kind: EventKind, anchor: f64) -> EventIdentity {
    EventIdentity {
        source: "rsi".into(),
        kind,
        subject: "support-1".into(),
        anchor,
        timeframe_secs: 3600,
    }
}

/// 01-15 §8's worked example, parameterised: a bearish divergence on a 1h candle.
struct Worked {
    journal: EventJournal,
}

impl Worked {
    fn new() -> Self {
        Self {
            journal: EventJournal::with_unit_bars(12),
        }
    }

    /// bar 40: emitted, unconfirmed.
    fn emit(&mut self, bar: usize) -> String {
        self.journal
            .emit(identity(EventKind::Divergence, 100.0), bar)
            .expect("stored")
    }

    fn bound(&self, id: &str) -> usize {
        self.journal.get(id).expect("record").expiry_bars as usize
    }
}

#[test]
fn cg8_the_worked_example_reproduces_bar_for_bar() {
    // 01-15 §8, verbatim in shape: emitted → active → expired → NEW event
    // supersedes → active.
    let mut w = Worked::new();

    let d1 = w.emit(40);
    assert_eq!(w.journal.counts().active, 0, "bar 40: active count 0");
    assert_eq!(
        w.journal.get(&d1).unwrap().state,
        EventState::Pending,
        "bar 40: pending"
    );

    // The divergence's own detection lookback is 12 bars per unit, ×2 units.
    let bound = w.bound(&d1);
    assert_eq!(bound, 24, "2 x its own detection lookback");
    assert!(w.journal.confirm(&d1, 43));
    assert_eq!(w.journal.counts().active, 1, "bar 43: active count 1");

    // bar 67: 24 bars after the last validation → expired.
    w.journal.expire_at(43 + bound);
    assert_eq!(w.journal.counts().active, 0, "bar 67: active count 0");
    assert_eq!(
        w.journal.get(&d1).unwrap().state,
        EventState::Expired,
        "the journal still holds it for history"
    );
    assert_eq!(w.journal.counts().retained, 1, "retained, not counted");

    // bar 90: the premise reappears → a NEW event supersedes the old one.
    let d2 = w
        .journal
        .reactivate(&d1, 90)
        .expect("a new canonical event");
    assert_ne!(d2, d1, "a reactivation is a new identity");
    assert_eq!(
        w.journal.get(&d2).unwrap().supersedes.as_deref(),
        Some(d1.as_str())
    );
    assert_eq!(
        w.journal.get(&d1).unwrap().state,
        EventState::Superseded,
        "the old record is superseded, never revived"
    );
    assert_eq!(
        w.journal.counts().active,
        0,
        "bar 90: the new event is pending"
    );

    assert!(w.journal.confirm(&d2, 93));
    assert_eq!(w.journal.counts().active, 1, "bar 93: active count 1");
    assert_eq!(w.journal.counts().emitted, 2);
}

#[test]
fn cg8_nothing_revives_a_terminal_record() {
    // The "no revival" property stated directly, across every terminal state.
    for terminal_state in [
        EventState::Expired,
        EventState::Invalidated,
        EventState::Resolved,
        EventState::Superseded,
    ] {
        let mut j = EventJournal::new();
        let id = j.emit(identity(EventKind::Breakout, 110.0), 10).unwrap();
        j.confirm(&id, 11);
        match terminal_state {
            EventState::Expired => {
                j.expire_at(11 + j.get(&id).unwrap().expiry_bars as usize + 1);
            }
            EventState::Invalidated => {
                j.invalidate(&id, "X-1");
            }
            EventState::Resolved => {
                assert!(j.resolve(&id));
            }
            EventState::Superseded => {
                // Reactivation requires a lapsed event: it is what a *returning*
                // premise looks like, not a way to refresh a live one.
                j.expire_at(11 + j.get(&id).unwrap().expiry_bars as usize);
                let replacement = j.reactivate(&id, 20).expect("a new canonical event");
                assert_ne!(replacement, id);
            }
            _ => unreachable!(),
        }
        assert_eq!(j.get(&id).unwrap().state, terminal_state);
        assert!(
            !j.confirm(&id, 30),
            "{terminal_state:?} must never be re-confirmed into active"
        );
        assert!(
            !j.persist(&id, 31),
            "{terminal_state:?} must never be persisted"
        );
        assert!(
            !j.invalidate(&id, "X-2"),
            "{terminal_state:?} is already terminal"
        );
        assert_eq!(
            j.counts().active,
            0,
            "{terminal_state:?} contributes nothing to any axis"
        );
    }
}

#[test]
fn cg8_only_active_is_citable() {
    // E1, asserted against the type rather than the reader.
    assert!(EventState::Active.is_citable());
    for state in [
        EventState::Pending,
        EventState::Expired,
        EventState::Invalidated,
        EventState::Resolved,
        EventState::Superseded,
    ] {
        assert!(!state.is_citable(), "{state:?} must not be citable");
    }
}

#[test]
fn cg8_a_pending_event_is_retained_and_visible_but_counts_zero() {
    let mut j = EventJournal::new();
    let id = j.emit(identity(EventKind::TrendFlip, 90.0), 10).unwrap();
    let counts = j.counts();
    assert_eq!(counts.retained, 1, "retained");
    assert_eq!(counts.active, 0, "E2: pending is not evidence");
    let record: &EventRecord = j.get(&id).unwrap();
    assert_eq!(record.state, EventState::Pending);
    assert!(
        !record.state.is_historical(),
        "pending is not history — it is open"
    );
}

#[test]
fn cg8_absence_of_contradiction_does_not_extend_an_event() {
    // E3. The mutation M2 is precisely the removal of this: treating "nothing
    // happened" as "it is still happening".
    let mut j = EventJournal::with_unit_bars(12);
    let id = j.emit(identity(EventKind::Divergence, 100.0), 40).unwrap();
    j.confirm(&id, 43);
    let bound = j.get(&id).unwrap().expiry_bars as usize;

    // A hundred quiet bars, with no contradicting event anywhere.
    for bar in 44..140 {
        j.expire_at(bar);
    }
    assert_eq!(
        j.get(&id).unwrap().state,
        EventState::Expired,
        "silence is not persistence"
    );
    assert_eq!(j.counts().active, 0);
    assert!(bound > 0);
}

#[test]
fn cg8_persistence_is_the_only_thing_that_extends_life() {
    let mut j = EventJournal::with_unit_bars(12);
    let id = j.emit(identity(EventKind::Divergence, 100.0), 40).unwrap();
    j.confirm(&id, 43);
    let bound = j.get(&id).unwrap().expiry_bars as usize;

    // Re-validate at intervals shorter than the bound: it lives.
    for bar in (50..200).step_by(bound / 2 + 1) {
        assert!(j.persist(&id, bar), "bar {bar} should persist");
        j.expire_at(bar);
        assert_eq!(j.get(&id).unwrap().state, EventState::Active);
    }
    // Then stop persisting: the bound is measured from the LAST validation.
    let last = j.get(&id).unwrap().last_validated_at.unwrap();
    j.expire_at(last + bound);
    assert_eq!(j.get(&id).unwrap().state, EventState::Expired);
}

#[test]
fn cg8_an_invalidation_records_the_contradicting_identity() {
    // E10 — without it, "why did this event die?" has no answer.
    let mut j = EventJournal::new();
    let id = j.emit(identity(EventKind::Divergence, 100.0), 40).unwrap();
    j.confirm(&id, 43);
    assert!(j.invalidate(&id, "B-7781"));
    let record = j.get(&id).unwrap();
    assert_eq!(record.state, EventState::Invalidated);
    assert_eq!(record.invalidated_by.as_deref(), Some("B-7781"));
    assert_eq!(j.counts().active, 0);
}

#[test]
fn cg8_the_three_counts_are_separately_exported() {
    // E7: a journal that reports only a total is under-specified.
    let mut j = EventJournal::with_unit_bars(12);
    let active = j.emit(identity(EventKind::Divergence, 100.0), 40).unwrap();
    j.confirm(&active, 43);
    let pending = j.emit(identity(EventKind::Breakout, 110.0), 44).unwrap();
    let bound = j.get(&active).unwrap().expiry_bars as usize;
    j.expire_at(43 + bound);

    let counts: EventCounts = j.counts();
    assert_eq!(counts.active, 0, "the divergence has lapsed");
    assert_eq!(counts.retained, 2, "both are still in the journal");
    // Both are history now: the pending breakout was never confirmed and E3
    // says a pending event still expires on its own bound — absence of
    // contradiction is not persistence.
    assert_eq!(
        counts.historical, 2,
        "neither lapsed event is evidence any more"
    );
    assert_eq!(counts.emitted, 2);
    let _ = pending;

    // The three are different numbers and a reader must choose between them.
    // `retained` and `emitted` coincide here only because nothing has been
    // evicted yet — which is exactly why conflating them is a bug waiting to
    // happen rather than a mistake that announces itself.
    assert_ne!(counts.active, counts.retained);
    assert_eq!(counts.retained, counts.emitted, "nothing evicted yet");
    // The four figures partition the journal exactly: no state is unaccounted
    // for, and none is counted twice.
    assert_eq!(
        counts.active + counts.pending + counts.historical,
        counts.retained,
        "the exported counts must partition the journal"
    );
}

#[test]
fn cg8_expiry_bounds_are_duration_relative_not_per_duration_constants() {
    // E9: the bound is a multiple of a duration-derived unit, so the same rule
    // applies at every duration with no per-duration table.
    let mut j1 = EventJournal::with_unit_bars(12);
    let id = j1.emit(identity(EventKind::Divergence, 100.0), 0).unwrap();
    assert_eq!(
        j1.get(&id).unwrap().expiry_bars,
        EventKind::Divergence.expiry_in_units() * 12
    );
    // A different unit size — one duration's unit in that duration's bars.
    let mut j2 = EventJournal::with_unit_bars(3);
    let id2 = j2.emit(identity(EventKind::Divergence, 100.0), 0).unwrap();
    assert_eq!(
        j2.get(&id2).unwrap().expiry_bars,
        EventKind::Divergence.expiry_in_units() * 3,
        "the unit scales, the multiplier does not"
    );
}

#[test]
fn cg8_a_candidate_event_is_not_counted_while_it_is_unconfirmed() {
    // The block above is a unit test of the identity rule; this one is the
    // integration shape: a leg's event journal feeding an axis count.
    let mut j = EventJournal::with_unit_bars(12);
    let id = j.emit(identity(EventKind::Divergence, 100.0), 40).unwrap();
    assert_eq!(j.active_identity_count(), 0);
    j.confirm(&id, 43);
    assert_eq!(j.active_identity_count(), 1, "E5: exactly one contribution");
    let bound = j.get(&id).unwrap().expiry_bars as usize;
    j.expire_at(43 + bound);
    assert_eq!(j.active_identity_count(), 0);
    assert_eq!(
        j.counts().retained,
        1,
        "still retained: retention and validity are different mechanisms"
    );
    assert!(TIER_B_AGE_BARS >= bound as usize);
}
