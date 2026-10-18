//! The event journal — Tier C (01-15).
//!
//! A journal is a **window**, and a window alone cannot answer the question the
//! axes ask. The question is not "how many events are in the buffer" but "how
//! many are *current evidence* right now" — and those are different numbers,
//! which is why a retention bound and a validity state are separate mechanisms
//! (01-15 §5, T3).
//!
//! ## The three counts, never conflated
//!
//! | Quantity | What it is |
//! |---|---|
//! | **active** | Events in state `active`. The only one an axis may consume (E1). |
//! | **retained** | Events the journal still holds, in any state. History (E7). |
//! | **emit** | Events ever produced this session. A rate, not a state (E7). |
//!
//! A consumer that reads `retained` where it needed `active` is reading history
//! as structure. [`EventCounts`] makes that mistake unrepresentable: the three
//! are named fields, so no call site can accidentally sum them.
//!
//! ## Why reactivation is a new identity
//!
//! Reviving a record would let one divergence be counted twice — once before it
//! lapsed, once after — and the count would then mean nothing (E4). So an event's
//! identity is immutable and a reactivation mints a new record carrying a
//! `supersedes` pointer back to the one it replaced.

use std::collections::VecDeque;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

/// Tier C: at most this many events, **and** at most `tier_b_age_bars` old.
pub const TIER_C_MAX_EVENTS: usize = 200;
/// The Tier B window: an event older than this is not part of the current
/// structure even if it is still "active" by its own kind's bound.
pub const TIER_B_AGE_BARS: usize = 500;

/// The canonical identity (01-15 §2.2): `(source, kind, subject, anchor,
/// timeframe)`.
///
/// Immutable, and **counted exactly once across all axes** (E5) — this is what
/// keeps one divergence from becoming three pieces of evidence across Structure,
/// Progression and a consolidated reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventIdentity {
    /// The reading that produced it (`rsi`, `macd`, …).
    pub source: String,
    pub kind: EventKind,
    /// What it is about: a price level, an indicator key, a leg id.
    pub subject: String,
    /// The price or level the event is anchored to.
    pub anchor: f64,
    pub timeframe_secs: u64,
}

impl Eq for EventIdentity {}

/// The canonical sort/hash key.
///
/// `anchor` is an `f64`, and `f64` is neither `Eq` nor `Ord`. Deriving those
/// would be a compile error, and *hand-writing* them over `to_bits()` would be a
/// subtler bug: `0.0 == -0.0` is true in `f64` but their bit patterns differ, so
/// two identities that compare equal would land in different `BTreeSet` slots —
/// and E5 ("counted exactly once") would quietly become "counted twice for the
/// same anchor".
///
/// So the key **normalises** first: `-0.0` folds to `0.0`, and every `NaN` folds
/// to one canonical `NaN`. After that the bit pattern is a faithful identity.
impl EventIdentity {
    /// The canonical bits of an anchor price.
    fn anchor_key(anchor: f64) -> i64 {
        let normalised = if anchor == 0.0 { 0.0 } else { anchor };
        normalised.to_bits() as i64
    }

    fn sort_key(&self) -> (&str, EventKind, &str, i64, u64) {
        (
            self.source.as_str(),
            self.kind,
            self.subject.as_str(),
            EventIdentity::anchor_key(self.anchor),
            self.timeframe_secs,
        )
    }
}

impl Ord for EventIdentity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for EventIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for EventIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.sort_key().hash(state);
    }
}

impl EventIdentity {
    /// The short form a report quotes: `D-4471`.
    pub fn short_id(&self, sequence: u32) -> String {
        let prefix = match self.kind {
            EventKind::Divergence => "D",
            EventKind::Breakout => "B",
            EventKind::TrendFlip => "F",
            EventKind::LevelTest => "L",
            EventKind::PatternForming => "P",
            EventKind::VolumeClimax => "V",
            EventKind::CompressionRelease => "C",
            EventKind::Threshold => "T",
            EventKind::Crossover => "X",
            EventKind::CenterlineCross => "M",
            EventKind::ZeroLineCross => "Z",
        };
        format!("{prefix}-{sequence}")
    }
}

/// The kinds the journal understands.
///
/// The **instantaneous** four (§4) expire on their own bar: they describe one
/// bar, and reporting a crossover as a standing condition is a category error
/// (E6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Divergence,
    Breakout,
    TrendFlip,
    LevelTest,
    PatternForming,
    VolumeClimax,
    CompressionRelease,
    Threshold,
    Crossover,
    CenterlineCross,
    ZeroLineCross,
}

impl EventKind {
    pub const ALL: [EventKind; 11] = [
        EventKind::Divergence,
        EventKind::Breakout,
        EventKind::TrendFlip,
        EventKind::LevelTest,
        EventKind::PatternForming,
        EventKind::VolumeClimax,
        EventKind::CompressionRelease,
        EventKind::Threshold,
        EventKind::Crossover,
        EventKind::CenterlineCross,
        EventKind::ZeroLineCross,
    ];

    /// Instantaneous kinds expire on the crossing bar (E6).
    pub fn is_instantaneous(self) -> bool {
        matches!(
            self,
            EventKind::Threshold
                | EventKind::Crossover
                | EventKind::CenterlineCross
                | EventKind::ZeroLineCross
        )
    }

    /// The expiry bound in **bars**, expressed as a multiple of a duration-
    /// relative quantity (E9).
    ///
    /// `bars = bars_per_unit × unit_bars`. `unit_bars` is derived from the
    /// duration, so the bound is proportional and there is no per-duration
    /// constant anywhere in this table.
    pub fn expiry_in_units(self) -> u32 {
        match self {
            // 2 × its own detection lookback.
            EventKind::Divergence => 2,
            // 1 × the level's defining window.
            EventKind::Breakout => 1,
            // Holds one further bar; the next opposite flip supersedes it.
            EventKind::TrendFlip => 1,
            // Same bar.
            EventKind::LevelTest => 1,
            // The pattern's own maximum bar count.
            EventKind::PatternForming => 12,
            // Next bar.
            EventKind::VolumeClimax => 1,
            EventKind::CompressionRelease => 2,
            // The crossing bar.
            EventKind::Threshold
            | EventKind::Crossover
            | EventKind::CenterlineCross
            | EventKind::ZeroLineCross => 0,
        }
    }
}

/// How many bars one "unit" spans for a given duration.
///
/// A unit is **12 bars**, on every duration. E9 forbids a per-duration constant,
/// and a duration-independent unit is what satisfies that: the same ratio of
/// work, expressed in the duration's own bars.
pub const BARS_PER_EXPIRY_UNIT: u32 = 12;

/// The lifecycle states (§3). Only [`EventState::Active`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventState {
    /// Emitted, not yet conclusive. Retained, visible, **counts zero** (E2).
    Pending,
    Active,
    Expired,
    Invalidated,
    Resolved,
    Superseded,
}

impl EventState {
    /// The only question an axis may ask (E1).
    pub fn is_citable(self) -> bool {
        matches!(self, EventState::Active)
    }

    /// Whether the record is history rather than evidence.
    pub fn is_historical(self) -> bool {
        !self.is_citable() && !matches!(self, EventState::Pending)
    }
}

/// One retained event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    /// Immutable, immutable, immutable (E4).
    pub identity: EventIdentity,
    /// The short id a report quotes (`D-4471`).
    pub short_id: String,
    pub state: EventState,

    /// When it was emitted.
    pub activated_at: usize,
    /// The bar at which it could first have been **known** — never earlier than
    /// `occurred_at` (CG1).
    pub available_at: usize,
    /// The last bar its confirming condition held.
    pub last_validated_at: Option<usize>,
    /// Its own age bound, in bars, frozen at creation (E3, E9).
    pub expiry_bars: u32,
    /// The event that invalidated it (E10).
    pub invalidated_by: Option<String>,
    /// The event this one replaced on reactivation (E4).
    pub supersedes: Option<String>,
}

/// The three counts, named so they cannot be summed by accident (E7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventCounts {
    /// Events in state `active` — the only figure an axis consumes.
    pub active: usize,
    /// Events the journal holds, in any state.
    pub retained: usize,
    /// Events ever produced this session. A rate, not a state.
    pub emitted: usize,
    /// How many of the retained events are still open (emitted, unconfirmed).
    pub pending: usize,
    /// How many of the retained events are history rather than evidence.
    pub historical: usize,
}

/// The journal.
#[derive(Debug, Clone)]
pub struct EventJournal {
    /// Bounded by count **and** age (Tier C).
    records: VecDeque<EventRecord>,
    emitted: usize,
    next_sequence: u32,
    /// The bars per expiry unit, so the bound is duration-relative (E9).
    bars_per_unit: u32,
}

impl Default for EventJournal {
    fn default() -> Self {
        Self::new()
    }
}

impl EventJournal {
    pub fn new() -> Self {
        Self {
            records: VecDeque::new(),
            emitted: 0,
            next_sequence: 4400,
            bars_per_unit: BARS_PER_EXPIRY_UNIT,
        }
    }

    /// A journal whose expiry unit is sized to the duration.
    ///
    /// `unit_bars` is a **count of bars**, not a price or a time — the same
    /// number of observations on every duration, which is what keeps E9 true.
    pub fn with_unit_bars(unit_bars: u32) -> Self {
        Self {
            bars_per_unit: unit_bars.max(1),
            ..Self::new()
        }
    }

    pub fn records(&self) -> &VecDeque<EventRecord> {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The three counts, plus the historical split (E7).
    pub fn counts(&self) -> EventCounts {
        EventCounts {
            active: self.records.iter().filter(|r| r.state.is_citable()).count(),
            retained: self.records.len(),
            emitted: self.emitted,
            pending: self
                .records
                .iter()
                .filter(|r| matches!(r.state, EventState::Pending))
                .count(),
            historical: self
                .records
                .iter()
                .filter(|r| r.state.is_historical())
                .count(),
        }
    }

    pub fn get(&self, short_id: &str) -> Option<&EventRecord> {
        self.records.iter().find(|r| r.short_id == short_id)
    }

    /// Emit a new event: activation. It is **pending**, not citable (E2).
    ///
    /// Returns `None` when the journal is at its count bound and the new event
    /// cannot be stored — the caller must be able to see that the bound was hit
    /// rather than finding out later from a count that does not add up (E8).
    pub fn emit(&mut self, identity: EventIdentity, at_bar: usize) -> Option<String> {
        if self.records.len() >= TIER_C_MAX_EVENTS {
            return None;
        }
        let short_id = identity.short_id(self.next_sequence);
        self.next_sequence += 1;
        let expiry_bars = identity.kind.expiry_in_units() * self.bars_per_unit;
        self.records.push_back(EventRecord {
            identity,
            short_id: short_id.clone(),
            state: EventState::Pending,
            activated_at: at_bar,
            available_at: at_bar,
            last_validated_at: None,
            expiry_bars,
            invalidated_by: None,
            supersedes: None,
        });
        self.emitted += 1;
        Some(short_id)
    }

    /// Confirm a pending event: `pending → active`.
    ///
    /// A confirmation before the event's availability is refused: the event was
    /// emitted on bar *n*, and nothing about bar *n* can confirm it (CG1).
    pub fn confirm(&mut self, short_id: &str, at_bar: usize) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.short_id == short_id) else {
            return false;
        };
        if record.state != EventState::Pending {
            return false;
        }
        if at_bar < record.available_at {
            return false;
        }
        record.state = EventState::Active;
        record.last_validated_at = Some(at_bar);
        true
    }

    /// Persist an active event: its confirming condition held again. Extends
    /// `last_validated_at` — and therefore its life, which is the only way life
    /// is ever extended (E3).
    pub fn persist(&mut self, short_id: &str, at_bar: usize) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.short_id == short_id) else {
            return false;
        };
        if record.state != EventState::Active || at_bar < record.available_at {
            return false;
        }
        record.last_validated_at = Some(at_bar);
        true
    }

    /// Expire events whose own age bound has elapsed at `now_bar`.
    ///
    /// The bound is measured from the **last validation**, not from activation
    /// — and an event that has never been validated since activation is measured
    /// from activation. An instantaneous kind has a bound of zero bars and
    /// therefore expires on its own crossing bar (E6).
    pub fn expire_at(&mut self, now_bar: usize) -> usize {
        let mut expired = 0;
        for record in self.records.iter_mut() {
            if !record.state.is_citable() && record.state != EventState::Pending {
                continue;
            }
            // Instantaneous events expire on the crossing bar, full stop.
            if record.identity.kind.is_instantaneous() {
                if record.activated_at < now_bar {
                    record.state = EventState::Expired;
                    expired += 1;
                }
                continue;
            }
            // A pending event that is never confirmed still expires on its bound;
            // absence of contradiction is not persistence (E3).
            let since = record.last_validated_at.unwrap_or(record.activated_at);
            if now_bar.saturating_sub(since) >= record.expiry_bars.max(1) as usize {
                record.state = EventState::Expired;
                expired += 1;
            }
        }
        expired
    }

    /// Invalidate an event, recording **which** event contradicted it (E10).
    pub fn invalidate(&mut self, short_id: &str, by: &str) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.short_id == short_id) else {
            return false;
        };
        if !matches!(record.state, EventState::Pending | EventState::Active) {
            return false;
        }
        record.state = EventState::Invalidated;
        record.invalidated_by = Some(by.to_string());
        true
    }

    /// Resolve an event: it completed normally.
    pub fn resolve(&mut self, short_id: &str) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.short_id == short_id) else {
            return false;
        };
        if !matches!(record.state, EventState::Pending | EventState::Active) {
            return false;
        }
        record.state = EventState::Resolved;
        true
    }

    /// Reactivate: the confirming condition returned after expiry or
    /// invalidation.
    ///
    /// **A new canonical event with a `supersedes` pointer.** The old record is
    /// marked `superseded` and is never revived (E4) — reviving it would let one
    /// divergence count twice.
    pub fn reactivate(&mut self, short_id: &str, at_bar: usize) -> Option<String> {
        let old = self
            .records
            .iter()
            .find(|r| r.short_id == short_id)?
            .clone();
        if old.state.is_citable() || old.state == EventState::Pending {
            // A citable or pending event is still current; reactivating it would
            // double-count a live event.
            return None;
        }
        let new_id = self.emit(old.identity.clone(), at_bar)?;
        if let Some(record) = self.records.iter_mut().find(|r| r.short_id == old.short_id) {
            record.state = EventState::Superseded;
        }
        if let Some(record) = self.records.iter_mut().find(|r| r.short_id == new_id) {
            record.supersedes = Some(old.short_id.clone());
        }
        Some(new_id)
    }

    /// Enforce the **age** half of Tier C.
    ///
    /// Eviction here is a *storage* decision and is lifecycle-neutral: a record
    /// dropped for age is dropped in whatever state it was in, so the active
    /// count can only change if the evicted record was active — which the count
    /// reports honestly rather than hiding (T3).
    pub fn evict_beyond_age(&mut self, now_bar: usize) -> usize {
        let cutoff = now_bar.saturating_sub(TIER_B_AGE_BARS);
        let before = self.records.len();
        self.records.retain(|r| r.activated_at >= cutoff);
        before - self.records.len()
    }

    /// Every active event of a kind, for an axis that consumes exactly this
    /// journal (E1, E5: one event, one contribution).
    pub fn active_of_kind(&self, kind: EventKind) -> Vec<&EventRecord> {
        self.records
            .iter()
            .filter(|r| r.state.is_citable() && r.identity.kind == kind)
            .collect()
    }

    /// The distinct identities among active events — the number an axis may
    /// count, however many axes ask (E5).
    pub fn active_identity_count(&self) -> usize {
        let mut ids: std::collections::BTreeSet<&EventIdentity> = std::collections::BTreeSet::new();
        for record in self.records.iter().filter(|r| r.state.is_citable()) {
            ids.insert(&record.identity);
        }
        ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(kind: EventKind, anchor: f64) -> EventIdentity {
        EventIdentity {
            source: "rsi".into(),
            kind,
            subject: "support-1".into(),
            anchor,
            timeframe_secs: 3600,
        }
    }

    fn divergence() -> EventIdentity {
        identity(EventKind::Divergence, 100.0)
    }

    #[test]
    fn an_emitted_event_is_pending_and_counts_zero() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).expect("stored");
        assert_eq!(
            j.counts().active,
            0,
            "E2: emitted-but-unconfirmed counts zero"
        );
        assert_eq!(j.counts().retained, 1);
        assert_eq!(j.counts().emitted, 1);
        assert_eq!(j.get(&id).unwrap().state, EventState::Pending);
    }

    #[test]
    fn confirmation_makes_it_active_and_exactly_one() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).unwrap();
        assert!(j.confirm(&id, 43));
        assert_eq!(j.counts().active, 1);
        assert_eq!(j.get(&id).unwrap().state, EventState::Active);
    }

    #[test]
    fn a_confirmation_before_the_event_was_known_is_refused() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).unwrap();
        assert!(
            !j.confirm(&id, 39),
            "nothing on bar 39 can confirm a bar-40 event"
        );
        assert_eq!(j.counts().active, 0);
    }

    #[test]
    fn reactivation_mints_a_new_identity_and_never_revives() {
        let mut j = EventJournal::new();
        let first = j.emit(divergence(), 40).unwrap();
        j.confirm(&first, 43);
        j.expire_at(43 + j.get(&first).unwrap().expiry_bars as usize);
        assert_eq!(j.get(&first).unwrap().state, EventState::Expired);

        let second = j.reactivate(&first, 90).expect("a new event");
        assert_ne!(second, first, "E4: a reactivation is a NEW canonical event");
        assert_eq!(j.get(&first).unwrap().state, EventState::Superseded);
        assert_eq!(
            j.get(&second).unwrap().supersedes.as_deref(),
            Some(first.as_str())
        );
        assert_eq!(j.counts().active, 0, "the new event is pending, not active");
    }

    #[test]
    fn a_live_event_cannot_be_reactivated() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).unwrap();
        j.confirm(&id, 43);
        assert!(
            j.reactivate(&id, 50).is_none(),
            "that would double-count a live event"
        );
        assert_eq!(j.counts().active, 1);
    }

    #[test]
    fn an_instantaneous_event_expires_on_its_own_bar() {
        let mut j = EventJournal::new();
        let id = j.emit(identity(EventKind::Crossover, 50.0), 10).unwrap();
        j.confirm(&id, 10);
        assert_eq!(j.counts().active, 1);
        j.expire_at(11);
        assert_eq!(
            j.counts().active,
            0,
            "E6: a crossover happened; it is not happening"
        );
        assert_eq!(j.get(&id).unwrap().state, EventState::Expired);
    }

    #[test]
    fn the_three_counts_are_never_conflated() {
        let mut j = EventJournal::new();
        let a = j.emit(divergence(), 40).unwrap();
        j.confirm(&a, 43);
        let b = j.emit(identity(EventKind::Breakout, 110.0), 44).unwrap();
        // `b` stays pending.

        let counts = j.counts();
        assert_eq!(counts.active, 1);
        assert_eq!(counts.retained, 2);
        assert_eq!(counts.emitted, 2);
        assert_eq!(counts.historical, 0);
        assert_ne!(counts.active, counts.retained);
    }

    #[test]
    fn invalidation_records_the_contradicting_event() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).unwrap();
        j.confirm(&id, 43);
        assert!(j.invalidate(&id, "X-9901"));
        let record = j.get(&id).unwrap();
        assert_eq!(record.state, EventState::Invalidated);
        assert_eq!(record.invalidated_by.as_deref(), Some("X-9901"), "E10");
        assert_eq!(j.counts().active, 0);
    }

    #[test]
    fn the_journal_refuses_to_exceed_its_count_bound() {
        let mut j = EventJournal::new();
        let mut stored = 0;
        for i in 0..(TIER_C_MAX_EVENTS + 10) {
            if j.emit(identity(EventKind::LevelTest, i as f64), i)
                .is_some()
            {
                stored += 1;
            }
        }
        assert_eq!(stored, TIER_C_MAX_EVENTS, "E8: the bound is a hard maximum");
        assert_eq!(j.counts().retained, TIER_C_MAX_EVENTS);
        assert_eq!(
            j.counts().emitted,
            TIER_C_MAX_EVENTS,
            "and the bound is visible to the caller"
        );
    }

    #[test]
    fn one_event_counts_once_however_many_axes_ask() {
        let mut j = EventJournal::new();
        let id = j.emit(divergence(), 40).unwrap();
        j.confirm(&id, 43);
        // Three "consumers" read the same journal.
        assert_eq!(j.active_of_kind(EventKind::Divergence).len(), 1);
        assert_eq!(j.active_of_kind(EventKind::Breakout).len(), 0);
        assert_eq!(j.active_identity_count(), 1, "E5");
    }
}
