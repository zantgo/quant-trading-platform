//! **CG9 — retention is not validity** (P9).
//!
//! ## The claim
//!
//! 01-20 §6 CG9: *expired events inside storage bounds are stored but **never**
//! counted as active.* 01-15 §5 says the same thing from the other side: "The
//! lifecycle policy does not extend the Tier C bound. A journal can be within
//! its count and age bounds and still contain only expired events, because
//! expiry is a *semantic* decay and the bounds are a *storage* bound. Both must
//! be reported separately."
//!
//! ## The concrete shape
//!
//! A journal holding 200 records — all of them expired — is **within every
//! storage bound** and contributes **zero** to every axis. That is the assertion
//! this file exists to make, because the natural (wrong) implementation reports
//! `records.len()` and the bug is invisible until an axis starts counting
//! history.
//!
//! ## The mutations
//!
//! **M2** (expiry suppressed) and the **CG9 half of M7** (byte-budget eviction
//! reaching a Tier D record) are both caught here.

use core_domain::event::{
    EventIdentity, EventJournal, EventKind, EventState, TIER_B_AGE_BARS, TIER_C_MAX_EVENTS,
};

fn identity(kind: EventKind, anchor: f64, bar_hint: usize) -> EventIdentity {
    EventIdentity {
        source: "rsi".into(),
        kind,
        subject: format!("support-{}", bar_hint),
        anchor,
        timeframe_secs: 3600,
    }
}

#[test]
fn cg9_a_journal_full_of_expired_events_contributes_zero() {
    // The headline: within every storage bound, and empty of evidence.
    let mut j = EventJournal::with_unit_bars(12);
    for i in 0..TIER_C_MAX_EVENTS {
        j.emit(
            identity(EventKind::Divergence, 100.0 + i as f64, i),
            100 + i,
        );
    }
    assert_eq!(
        j.counts().retained,
        TIER_C_MAX_EVENTS,
        "at the storage bound"
    );
    assert_eq!(j.counts().active, 0, "none was ever confirmed");
    assert_eq!(j.active_identity_count(), 0, "and the axis sees nothing");

    // Confirm them all, then let them all lapse.
    let ids: Vec<String> = j.records().iter().map(|r| r.short_id.clone()).collect();
    for (i, id) in ids.iter().enumerate() {
        assert!(j.confirm(id, 101 + i));
    }
    assert_eq!(j.counts().active, TIER_C_MAX_EVENTS);
    j.expire_at(100 + TIER_C_MAX_EVENTS + 500);
    assert_eq!(
        j.counts().active,
        0,
        "a journal holding {TIER_C_MAX_EVENTS} expired records contributes ZERO"
    );
    assert_eq!(
        j.counts().retained,
        TIER_C_MAX_EVENTS,
        "and still holds them — retention and validity are different mechanisms"
    );
}

#[test]
fn cg9_expiry_is_semantic_decay_and_eviction_is_storage() {
    // T3: eviction never changes a lifecycle state, and expiry never causes
    // eviction. The two must be separable in both directions, so the fixture
    // needs one record that lapses while still young, and one that is still
    // alive but old.
    let mut j = EventJournal::with_unit_bars(12);

    // Young: will lapse by semantics, and is inside the age window.
    let young = j
        .emit(identity(EventKind::Divergence, 100.0, 0), 1000)
        .unwrap();
    j.confirm(&young, 1001);
    // Old: still active, and will be dropped by the age sweep.
    let old = j
        .emit(identity(EventKind::Divergence, 105.0, 1), 1400)
        .unwrap();
    j.confirm(&old, 1401);

    // Semantic decay: the young record lapses. Persistence is the only thing
    // that could have saved it, and nothing asserted it.
    let bound = j.get(&young).unwrap().expiry_bars as usize;
    j.expire_at(1001 + bound);
    assert_eq!(j.get(&young).unwrap().state, EventState::Expired);
    assert_eq!(
        j.get(&old).unwrap().state,
        EventState::Active,
        "decay is per-record; it did not touch the other one"
    );
    assert_eq!(
        j.get(&young).unwrap().activated_at,
        1000,
        "and it did not evict anything either"
    );

    // Storage: the age sweep drops the lapsed record — it is the one outside the
    // window — and leaves the ACTIVE record exactly as it was. That is T3's
    // claim in one assertion: a storage decision moved no lifecycle state.
    let evicted = j.evict_beyond_age(1200 + TIER_B_AGE_BARS);
    assert_eq!(evicted, 1, "exactly the record older than the window");
    assert!(
        j.get(&young).is_none(),
        "the lapsed record was dropped by storage"
    );
    assert_eq!(
        j.get(&old).unwrap().state,
        EventState::Active,
        "T3: eviction left the survivor's lifecycle state untouched"
    );
    assert_eq!(j.counts().active, 1, "and the axis still has its evidence");
}

#[test]
fn cg9_the_storage_bound_is_a_hard_maximum_not_a_target() {
    // T6 / E8. A quiet instrument and a wild one both stop at the same bound.
    let mut quiet = EventJournal::new();
    for i in 0..(TIER_C_MAX_EVENTS * 2) {
        if quiet
            .emit(identity(EventKind::LevelTest, i as f64, i), i)
            .is_some()
        {
            assert!(i < TIER_C_MAX_EVENTS, "emitted past the bound at {i}");
        }
    }
    assert_eq!(quiet.counts().retained, TIER_C_MAX_EVENTS);
}

#[test]
fn cg9_the_age_bound_is_separate_from_the_count_bound() {
    // Both ends of Tier C, each doing its own job.
    let mut j = EventJournal::new();
    // Well under the count bound, well over the age bound.
    for i in 0..10 {
        j.emit(identity(EventKind::LevelTest, i as f64, i), i);
    }
    assert_eq!(j.counts().retained, 10, "under the count bound");
    // The cutoff is `now - TIER_B_AGE_BARS`, so events older than the Tier B
    // window are dropped and the rest are not. The arithmetic is the point: the
    // bound is an age, and it removes exactly what exceeds it.
    let evicted = j.evict_beyond_age(TIER_B_AGE_BARS + 5);
    assert_eq!(evicted, 5, "records 0..4 are older than the window");
    assert_eq!(j.counts().retained, 5);
    assert_eq!(
        j.evict_beyond_age(TIER_B_AGE_BARS + 5),
        0,
        "and the sweep is idempotent"
    );
    assert_eq!(
        j.counts().emitted,
        10,
        "the emit count is a session rate and survives eviction"
    );
}

#[test]
fn cg9_an_evicted_active_record_is_reported_honestly_not_hidden() {
    // The uncomfortable case: eviction drops an ACTIVE record because it is old.
    // The active count falls — and that is correct, because the axis genuinely
    // no longer has it. What must never happen is the opposite: an expired
    // record being counted as active to keep a number looking healthy.
    let mut j = EventJournal::new();
    let id = j
        .emit(identity(EventKind::Divergence, 100.0, 0), 0)
        .unwrap();
    j.confirm(&id, 1);
    assert_eq!(j.counts().active, 1);
    j.evict_beyond_age(TIER_B_AGE_BARS + 10);
    assert_eq!(
        j.counts().active,
        0,
        "the record is gone, and so is its contribution"
    );
    assert_eq!(j.counts().retained, 0);
}

#[test]
fn cg9_expiry_never_extends_or_revives_through_the_storage_path() {
    // The round trip: expired → evicted → a new premise arrives → a NEW event.
    let mut j = EventJournal::with_unit_bars(12);
    let first = j
        .emit(identity(EventKind::Divergence, 100.0, 0), 100)
        .unwrap();
    j.confirm(&first, 101);
    let bound = j.get(&first).unwrap().expiry_bars as usize;
    j.expire_at(101 + bound);
    assert_eq!(j.get(&first).unwrap().state, EventState::Expired);

    let second = j.reactivate(&first, 300).expect("a new canonical event");
    assert_ne!(second, first, "E4 survives the storage path");
    assert_eq!(j.get(&first).unwrap().state, EventState::Superseded);
    assert_eq!(
        j.counts().active,
        0,
        "the replacement is pending, not evidence"
    );
}

#[test]
fn cg9_a_pending_event_expires_on_its_own_bound() {
    // E3 at the storage/lifecycle boundary: "an activated but unconfirmed event"
    // is retained and visible — but it does not live forever, because absence of
    // a contradicting event is not persistence.
    let mut j = EventJournal::with_unit_bars(12);
    let id = j
        .emit(identity(EventKind::Divergence, 100.0, 0), 100)
        .unwrap();
    let bound = j.get(&id).unwrap().expiry_bars as usize;
    assert_eq!(j.get(&id).unwrap().state, EventState::Pending);
    j.expire_at(100 + bound);
    assert_eq!(j.get(&id).unwrap().state, EventState::Expired);
    assert_eq!(j.counts().active, 0);
    assert_eq!(j.counts().retained, 1, "retained as history");
}

#[test]
fn cg9_a_tier_d_record_is_outside_the_byte_budget_reach() {
    // The CG9 half of **M7**. The leg machine is Tier D; the event journal is
    // Tier C. Only Tier C is bounded by count and age, and only Tier C is subject
    // to a storage sweep. Asserted against the leg module's own source for the
    // same reason CG7 asserts it: the defect being prevented is the *existence*
    // of a path, not a bad return value.
    let leg_source = include_str!("../src/leg/mod.rs");
    let code: String = leg_source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["evict_to", "truncate(", "remove(0)", "shrink_to"] {
        assert!(
            !code.contains(forbidden),
            "the leg machine contains `{forbidden}`: Tier D is never charged or \
             evicted by the byte budget (T2, L24)"
        );
    }
    // And the journal — which IS bounded — says so with its own bound, so the two
    // tiers cannot be mistaken for one.
    assert!(TIER_C_MAX_EVENTS > 0 && TIER_B_AGE_BARS > 0);
    assert_ne!(
        TIER_C_MAX_EVENTS, TIER_B_AGE_BARS,
        "count and age are separate axes"
    );
}
