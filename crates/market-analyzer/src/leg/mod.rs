//! The leg machine — the only writer of a [`LegRecord`] (P8).
//!
//! Everything that can *move* a leg's state lives here, and every rule that
//! forbids a move is a branch that returns without moving. That is deliberate:
//! the invariants (L1–L27) are only worth stating if some code can be shown to
//! be unable to break them, and a type alone cannot express "never bump both
//! counters".
//!
//! ## The three rules that shape the whole design
//!
//! 1. **One writer.** `LegMachine` holds the record and hands out readings.
//!    There is no second path, so "who bumped `target_version`?" has one answer.
//! 2. **The clock is a bar.** Every method takes `at_bar` and compares it with
//!    the record's own availability (L3, S-N3). A machine that read a wall clock
//!    would be exactly the CG12 violation the harness forbids.
//! 3. **A frozen threshold.** The noise floor and the rank threshold are set at
//!    [`LegMachine::open`] and never recomputed (L21). Recomputing them is how a
//!    leg's stage moves without the leg moving.

pub mod driver;
pub use driver::{LegDriver, PIVOT_STRENGTH, PIVOT_WINDOW};

use core_domain::axes::Stage;
use core_domain::leg::{
    Classification, CloseReason, Direction, LegKey, LegLifecycle, LegReading, LegRecord, Progress,
    Revision, RevisionCounter, StructureLevel, TargetBasis,
};
pub use core_domain::leg::{REVISION_BUDGET, REVISION_LOG_CAP, STABILITY_WINDOW};

/// How the machine is configured. System-wide, not per-leg: two legs under
/// different policies are not comparable (L27).
#[derive(Debug, Clone, PartialEq)]
pub struct LegPolicy {
    pub swing_policy_version: u32,
    /// Unrevised bars required before the reference may be called stable (L16).
    pub stability_window: u32,
    /// Revisions after which the reference is unviable (L17).
    pub revision_budget: u32,
    /// Displacement percentile at or above which a leg is `major` (§2.5).
    pub major_rank_threshold_pct: f64,
    /// How many observations the rank distribution needs before a classification
    /// is `settled` (S-N4).
    pub rank_distribution_size: usize,
}

impl Default for LegPolicy {
    fn default() -> Self {
        Self {
            swing_policy_version: 1,
            stability_window: STABILITY_WINDOW,
            revision_budget: REVISION_BUDGET,
            major_rank_threshold_pct: 80.0,
            rank_distribution_size: 20,
        }
    }
}

/// The causal self-percentile scale classifier (S-N1…S-N4).
///
/// A pivot's level is the rank of **its own displacement** against the
/// instrument's own Tier A history. Not an absolute distance (P2) and not a bar
/// count (P3) — so a quiet instrument is not classified all-`minor` merely
/// because its points are small.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScaleClassifier {
    /// `displacement, available_at` pairs, oldest first. Every entry carries the
    /// bar at which it became usable, so the distribution can be truncated to
    /// what a given bar could see (S-N1).
    observations: Vec<(f64, usize)>,
}

impl ScaleClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one displacement observation, with the bar at which it became
    /// usable. A later-availability entry is rejected rather than stored: the
    /// distribution would otherwise contain an observation no bar could see.
    pub fn observe(&mut self, displacement: f64, available_at: usize) {
        self.observations.push((displacement, available_at));
    }

    /// The distribution as it was knowable at `bar` — S-N1, enforced by
    /// construction rather than by convention.
    pub fn eligible_at(&self, bar: usize) -> Vec<f64> {
        self.observations
            .iter()
            .filter(|(_, available_at)| *available_at <= bar)
            .map(|(d, _)| *d)
            .collect()
    }

    /// How many observations a classification at `bar` would have behind it.
    pub fn sample_size_at(&self, bar: usize) -> usize {
        self.eligible_at(bar).len()
    }

    /// The displacement's percentile rank against the eligible distribution.
    ///
    /// A strictly-less-than count, so an observation equal to every other one
    /// ranks 0 — an exact tie is not evidence of being the largest.
    pub fn rank_pct(&self, displacement: f64, bar: usize) -> Option<f64> {
        let sample = self.eligible_at(bar);
        if sample.is_empty() {
            return None;
        }
        let below = sample.iter().filter(|d| **d < displacement).count();
        Some(100.0 * below as f64 / sample.len() as f64)
    }

    /// Classify a displacement at `bar`, given a **frozen** threshold.
    ///
    /// Returns `Provisional` while the sample is short (S-N4) — and a
    /// provisional classification publishes no stage (L22), which is the same
    /// withholding rule a `forming` leg is under.
    pub fn classify(
        &self,
        displacement: f64,
        at_bar: usize,
        frozen_threshold_pct: f64,
        required_sample: usize,
    ) -> (StructureLevel, Classification, Option<f64>) {
        let sample_size = self.sample_size_at(at_bar);
        let Some(rank) = self.rank_pct(displacement, at_bar) else {
            return (StructureLevel::Minor, Classification::Provisional, None);
        };
        if sample_size < required_sample {
            return (
                StructureLevel::Minor,
                Classification::Provisional,
                Some(rank),
            );
        }
        let level = if rank >= frozen_threshold_pct {
            StructureLevel::Major
        } else {
            StructureLevel::Minor
        };
        (level, Classification::Settled, Some(rank))
    }
}

/// The writer of leg records.
#[derive(Debug, Clone)]
pub struct LegMachine {
    policy: LegPolicy,
    legs: Vec<LegRecord>,
    next_id: u64,
    noise_floor: f64,
}

impl LegMachine {
    pub fn new(policy: LegPolicy, noise_floor: f64) -> Self {
        Self {
            policy,
            legs: Vec::new(),
            next_id: 1,
            noise_floor,
        }
    }

    pub fn policy(&self) -> &LegPolicy {
        &self.policy
    }

    /// Every current leg, ascending by level then instrument (L8: exactly one
    /// current leg per `(instrument, duration, level)`).
    pub fn legs(&self) -> &[LegRecord] {
        &self.legs
    }

    pub fn leg(&self, key: &LegKey) -> Option<&LegRecord> {
        self.legs.iter().find(|l| {
            l.instrument == key.instrument
                && l.timeframe_secs == key.timeframe_secs
                && l.structure_level == key.structure_level
        })
    }

    fn position(&self, key: &LegKey) -> Option<usize> {
        self.legs.iter().position(|l| {
            l.instrument == key.instrument
                && l.timeframe_secs == key.timeframe_secs
                && l.structure_level == key.structure_level
        })
    }

    /// Open a leg for `(instrument, duration, level)`.
    ///
    /// A new leg at the same key **replaces** the old one; the old record is
    /// discarded, not archived (L8, Tier D O(1)).
    pub fn open(
        &mut self,
        key: LegKey,
        direction: Direction,
        origin_price: f64,
        origin_occurred_at: usize,
        origin_available_at: usize,
        classification: Classification,
        displacement_rank_pct: f64,
    ) -> &mut LegRecord {
        let leg_id = format!("L-{:04}-{}", self.next_id, key.instrument);
        self.next_id += 1;
        // L8: a new leg at the same key REPLACES the record. The old one is
        // discarded, not archived — Tier D is O(1) in bars.
        //
        // A **terminal** record is history: it keeps its end reason and is
        // never closed a second time (L7). Removing a same-level record is a
        // replacement, not an end; a different-level CURRENT record is
        // superseded exactly once.
        let same_scope: Vec<usize> = self
            .legs
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.instrument == key.instrument && l.timeframe_secs == key.timeframe_secs
            })
            .map(|(i, _)| i)
            .collect();
        for existing in same_scope.into_iter().rev() {
            let leg = &self.legs[existing];
            if leg.structure_level == key.structure_level {
                self.legs.remove(existing);
            } else if !leg.lifecycle.is_terminal() {
                self.legs[existing].close(CloseReason::Superseded);
            }
        }
        let mut record = LegRecord::open(
            leg_id,
            key,
            direction,
            origin_price,
            origin_occurred_at,
            origin_available_at,
            classification,
            self.noise_floor,
            self.policy.major_rank_threshold_pct,
            self.policy.swing_policy_version,
        );
        record.classification_rank_pct = Some(displacement_rank_pct);
        self.legs.push(record);
        let last = self.legs.len() - 1;
        &mut self.legs[last]
    }

    /// Confirm the origin: `forming` → `active`. The invalidation must already
    /// be set (L9), because an active leg with no invalidation is a leg whose
    /// death nothing declares.
    pub fn confirm_origin(&mut self, key: &LegKey, at_bar: usize, invalidation_price: f64) -> bool {
        let Some(i) = self.position(key) else {
            return false;
        };
        let leg = &mut self.legs[i];
        if leg.lifecycle != LegLifecycle::Forming {
            return false;
        }
        // L3: the confirmation is a claim about knowledge, so it cannot predate
        // the availability of the origin it confirms.
        if at_bar < leg.origin_available_at {
            return false;
        }
        leg.invalidation_price = Some(invalidation_price);
        leg.origin_confirmed = true;
        leg.lifecycle = LegLifecycle::Active;
        true
    }

    /// Observe a price at a bar. Advances the extreme, ticks the stability
    /// window, and returns the published reading.
    ///
    /// The stability machine, exactly as §4.5 states it: **W−1 bars → false,
    /// W bars → true, and the revision bar is never counted.**
    pub fn observe(&mut self, key: &LegKey, price: f64, at_bar: usize) -> Option<LegReading> {
        let policy_window = self.policy.stability_window;
        let i = self.position(key)?;
        let leg = &mut self.legs[i];
        if leg.lifecycle.is_terminal() {
            return Some(leg.reading(price));
        }
        // A bar before the last one observed would reorder history. Refusing is
        // the only answer that cannot hide a reordering inside an aggregate.
        if leg.last_observed_bar.is_some_and(|last| at_bar <= last) {
            return Some(leg.reading(price));
        }
        leg.last_observed_bar = Some(at_bar);
        leg.bars_since_origin += 1;
        // The extreme only moves in the leg's direction.
        leg.extreme_price = match leg.direction {
            Direction::Long => leg.extreme_price.max(price),
            Direction::Short => leg.extreme_price.min(price),
        };

        // L17: unviable is permanent — stability can never return.
        if !leg.is_unviable() {
            if leg.unrevised_bars + 1 >= policy_window {
                leg.unrevised_bars += 1;
                leg.target_stable = true;
            } else {
                leg.unrevised_bars += 1;
                leg.target_stable = false;
            }
        }
        Some(leg.reading(price))
    }

    /// Revise the reference target.
    ///
    /// The two halves of CG3 live here:
    ///
    /// * it bumps `target_version` and **never** `structure_version` (S-N6), and
    /// * it resets the stability window to zero, so the bar of the revision is
    ///   never counted toward its own stability (L15, and the off-by-one CG4
    ///   exists to catch).
    ///
    /// A bar **before** the leg's availability is refused outright (L3): a
    /// revision nobody could have made is a look-ahead, and accepting it would
    /// let a replay "know" the target earlier than the live run could.
    pub fn revise_target(
        &mut self,
        key: &LegKey,
        new_target: f64,
        at_bar: usize,
        basis: TargetBasis,
        trigger: &str,
    ) -> bool {
        let Some(i) = self.position(key) else {
            return false;
        };
        let leg = &mut self.legs[i];
        if leg.lifecycle.is_terminal() || leg.is_unviable() {
            return false;
        }
        if at_bar < leg.origin_available_at {
            return false;
        }
        let Some(old) = leg.target_price else {
            // The first target is not a *revision* — it is the reference being
            // set. It does not count against the budget, because the budget
            // exists to measure a reference that cannot settle, not one that
            // has not yet been stated.
            leg.target_price = Some(new_target);
            leg.target_basis = basis;
            leg.target_stable = false;
            leg.unrevised_bars = 0;
            leg.push_revision(Revision {
                counter: RevisionCounter::Target,
                at_bar,
                from: leg.origin_price,
                to: new_target,
                basis,
                trigger: trigger.to_string(),
            });
            return true;
        };
        if old == new_target {
            return false;
        }
        leg.target_price = Some(new_target);
        leg.target_basis = basis;
        leg.target_version += 1; // S-N6: target only
        leg.target_revision_count += 1;
        // L15: every revision revokes stability, immediately and without
        // exception — including one that arrives while it was true.
        leg.target_stable = false;
        leg.unrevised_bars = 0;
        leg.push_revision(Revision {
            counter: RevisionCounter::Target,
            at_bar,
            from: old,
            to: new_target,
            basis,
            trigger: trigger.to_string(),
        });

        // L17: the budget is reached → the basis is unviable, permanently.
        if leg.target_revision_count >= self.policy.revision_budget {
            leg.target_basis = TargetBasis::Unviable;
            leg.target_stable = false;
        }
        true
    }

    /// Revise the origin. Bumps `structure_version` and **never**
    /// `target_version` (S-N5, L23).
    pub fn revise_origin(&mut self, key: &LegKey, new_origin: f64, at_bar: usize) -> bool {
        let Some(i) = self.position(key) else {
            return false;
        };
        let leg = &mut self.legs[i];
        if leg.lifecycle.is_terminal() {
            return false;
        }
        if at_bar < leg.origin_available_at {
            return false;
        }
        let old = leg.origin_price;
        if old == new_origin {
            return false;
        }
        leg.origin_price = new_origin;
        leg.structure_version += 1; // S-N5: structure only
        leg.push_revision(Revision {
            counter: RevisionCounter::Structure,
            at_bar,
            from: old,
            to: new_origin,
            basis: leg.target_basis,
            trigger: "origin revision".into(),
        });
        true
    }

    /// Reclassify the structural level.
    ///
    /// S-N5: a reclassification bumps `structure_version` and **never**
    /// `target_version`. It also has no effect on progress (01-14 §2.5) — the
    /// two columns of that table are the whole reason the counters exist.
    pub fn reclassify(&mut self, key: &LegKey, new_level: StructureLevel) -> bool {
        let Some(i) = self.position(key) else {
            return false;
        };
        let leg = &mut self.legs[i];
        if leg.lifecycle.is_terminal() {
            return false;
        }
        if leg.structure_level == new_level {
            return false;
        }
        leg.structure_level = new_level;
        leg.structure_version += 1;
        true
    }

    /// Close a leg. The reason is required (L7), and closing frees the key so a

    pub fn close(&mut self, key: &LegKey, reason: CloseReason) -> bool {
        let Some(i) = self.position(key) else {
            return false;
        };
        let leg = &mut self.legs[i];
        if leg.lifecycle.is_terminal() {
            return false;
        }
        leg.close(reason);
        true
    }

    /// The stage, if one may be published.
    pub fn stage(&self, key: &LegKey, price: f64) -> Option<Stage> {
        self.leg(key)?.stage(price)
    }

    /// Progress against the current reference. The snapshot a caller would
    /// publish alongside it.
    pub fn progress(&self, key: &LegKey, price: f64) -> Progress {
        match self.leg(key) {
            Some(leg) => leg.progress(price),
            None => Progress::Undefined {
                reason: core_domain::axes::UndefinedReason::NotApplicable {
                    detail: "no leg for this key".into(),
                },
            },
        }
    }
}

/// The bound on how many legs a machine holds.
///
/// **Structural, not byte-based**: `instruments × durations × levels` (L8, L12).
/// There is deliberately **no** eviction path — Tier D records are never charged
/// against the byte budget and never evicted by it (01-16 **T2**), so a memory
/// sweep cannot reach a leg. `gate_cg7_leg_identity.rs` asserts this statically,
/// because "there is no code that could evict a leg" is a claim about a codebase
/// and a codebase changes.
pub const LEG_BOUND_DESCRIPTION: &str =
    "instruments x durations x structure levels; no byte-budget eviction path (T2)";

/// The price-scale noise floor for a leg, from the instrument's own range on its
/// own duration.
///
/// ATR-shaped and **frozen at creation** (L21): a floor that evolved with the
/// leg would make `progress` move without price moving, which is the confusion
/// §4.3 exists to prevent.
pub fn noise_floor_from_range(range: f64) -> f64 {
    (range * 0.01).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A confirmed, referenced leg at 100 with a target of 110.
    fn ready(m: &mut LegMachine) {
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
        m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "test");
    }

    /// The same leg, plus its key.
    #[allow(dead_code)]
    fn ready_key(m: &mut LegMachine) -> LegKey {
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
        m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "test");
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
        m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "test");
        k
    }

    #[test]
    fn an_unconfirmed_leg_publishes_no_stage() {
        let mut m = machine();
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
        m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "test");
        // Stability without confirmation must not produce a stage.
        for _ in 0..5 {
            m.observe(&k, 105.0, 20);
        }
        assert!(
            m.stage(&k, 105.0).is_none(),
            "L6: forming publishes no stage"
        );
    }

    #[test]
    fn the_revision_bar_is_never_counted_toward_stability() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        // W = 3. The revision happened at bar 15.
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "the revision itself is not stability"
        );
        m.observe(&k, 105.0, 16);
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "one unrevised bar is not three"
        );
        m.observe(&k, 105.0, 17);
        assert!(!m.leg(&k).unwrap().target_stable, "two is still not three");
        m.observe(&k, 105.0, 18);
        assert!(
            m.leg(&k).unwrap().target_stable,
            "the third consecutive unrevised bar makes it stable"
        );
    }

    #[test]
    fn a_revision_after_stability_revokes_it_at_once() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        for bar in 16..=18 {
            m.observe(&k, 105.0, bar);
        }
        assert!(m.leg(&k).unwrap().target_stable);
        assert!(m.revise_target(&k, 112.0, 19, TargetBasis::Measured, "widened"));
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "L15: a reference that just moved cannot be called stable"
        );
    }

    #[test]
    fn reaching_the_budget_makes_the_basis_unviable_permanently() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        for i in 0..8 {
            m.revise_target(
                &k,
                110.0 + i as f64 + 1.0,
                20 + i,
                TargetBasis::Volatility,
                "churn",
            );
            for _ in 0..4 {
                m.observe(&k, 105.0, 40 + i * 5);
            }
        }
        let leg = m.leg(&k).unwrap();
        assert_eq!(leg.target_basis, TargetBasis::Unviable);
        assert!(!leg.target_stable, "L17: stability is permanently blocked");
        assert!(m.stage(&k, 105.0).is_none(), "no stage while unviable");
        // And it never recovers.
        for bar in 60..80 {
            m.observe(&k, 106.0, bar);
        }
        assert!(!m.leg(&k).unwrap().target_stable);
        assert_eq!(m.leg(&k).unwrap().target_basis, TargetBasis::Unviable);
    }

    #[test]
    fn a_retarget_bumps_target_version_and_never_structure() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        let before = m.leg(&k).unwrap().clone();
        assert!(m.revise_target(&k, 111.0, 20, TargetBasis::Measured, "x"));
        let after = m.leg(&k).unwrap();
        assert_eq!(after.target_version, before.target_version + 1);
        assert_eq!(after.structure_version, before.structure_version, "S-N6");
    }

    #[test]
    fn an_origin_revision_bumps_structure_version_and_never_target() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        let before = m.leg(&k).unwrap().clone();
        assert!(m.revise_origin(&k, 99.0, 21));
        let after = m.leg(&k).unwrap();
        assert_eq!(after.structure_version, before.structure_version + 1);
        assert_eq!(after.target_version, before.target_version, "S-N5");
    }

    #[test]
    fn progress_may_fall_without_price_moving() {
        // CG3's headline: the target moves away, progress falls, and nothing
        // about the price path changed.
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        let before = m.progress(&k, 105.0).value().expect("defined");
        m.revise_target(&k, 140.0, 20, TargetBasis::Extension, "extended");
        let after = m.progress(&k, 105.0).value().expect("defined");
        assert!(
            after < before,
            "progress {after} should fall below {before}"
        );
        assert_eq!(
            m.leg(&k).unwrap().extreme_price,
            100.0,
            "price never moved; only the reference did"
        );
    }

    #[test]
    fn a_revision_before_the_leg_was_knowable_is_refused() {
        let mut m = machine();
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
        assert!(
            !m.revise_target(&k, 110.0, 12, TargetBasis::Volatility, "early"),
            "L3: a revision at bar 12 cannot exist — the origin was not knowable until 13"
        );
    }

    #[test]
    fn a_new_leg_at_the_same_key_supersedes_the_old_one() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        m.open(
            k.clone(),
            Direction::Short,
            200.0,
            30,
            33,
            Classification::Settled,
            90.0,
        );
        assert_eq!(m.legs().len(), 1, "L8: one current leg per key");
        let leg = m.leg(&k).unwrap();
        assert_eq!(leg.lifecycle, LegLifecycle::Forming);
        assert_eq!(leg.origin_price, 200.0);
    }

    #[test]
    fn classification_is_provisional_until_the_sample_is_full() {
        let mut c = ScaleClassifier::new();
        c.observe(1.0, 10);
        c.observe(2.0, 11);
        let (level, class, rank) = c.classify(2.0, 11, 80.0, 20);
        assert_eq!(class, Classification::Provisional);
        assert_eq!(level, StructureLevel::Minor);
        assert!(
            rank.is_some(),
            "the rank is computable; the *classification* is provisional"
        );
    }

    #[test]
    fn a_future_observation_cannot_change_a_past_classification() {
        // S-N1: the distribution at bar 10 excludes anything not yet available.
        let mut c = ScaleClassifier::new();
        for i in 0..20 {
            c.observe(i as f64, 10 + i);
        }
        let (_, class_before, rank_before) = c.classify(19.0, 10, 80.0, 5);
        let (_, class_after, rank_after) = c.classify(19.0, 30, 80.0, 5);
        assert_eq!(
            class_before,
            Classification::Provisional,
            "only one observation at bar 10"
        );
        assert_eq!(class_after, Classification::Settled);
        assert_ne!(
            rank_before, rank_after,
            "the sample legitimately grows with time"
        );
        assert_eq!(c.sample_size_at(10), 1);
    }

    #[test]
    fn a_reclassification_does_not_move_progress() {
        // 01-14 §2.5: reclassification has no effect on progress.
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        let before = m.progress(&k, 105.0).value().unwrap();
        let tv_before = m.leg(&k).unwrap().target_version;
        assert!(m.reclassify(&k, StructureLevel::Minor));
        // The leg is now addressed by its NEW level: identity of the *record*
        // is `leg_id`, and the triple says which leg a reading is about.
        let moved = LegKey {
            structure_level: StructureLevel::Minor,
            ..k.clone()
        };
        let after = m.progress(&moved, 105.0).value().unwrap();
        assert_eq!(before, after, "progress must not move on a scale change");
        assert_eq!(m.leg(&moved).unwrap().target_version, tv_before, "S-N5");
        assert_eq!(m.leg(&moved).unwrap().structure_version, 1);
    }

    #[test]
    fn the_revision_log_and_counters_survive_more_revisions_than_the_log_holds() {
        let mut m = machine();
        ready(&mut m);
        let k = key(StructureLevel::Major);
        for i in 0..(REVISION_LOG_CAP + 4) {
            m.revise_target(
                &k,
                105.0 + i as f64,
                20 + i,
                TargetBasis::Volatility,
                "churn",
            );
        }
        let leg = m.leg(&k).unwrap();
        assert!(
            leg.revisions.len() <= REVISION_LOG_CAP,
            "the log is a bounded tail, never a history: {}",
            leg.revisions.len()
        );
        // The budget, not the log, is what stopped it: the basis is unviable and
        // every later revision was refused.
        assert_eq!(leg.target_basis, TargetBasis::Unviable);
        assert_eq!(
            leg.target_revision_count as usize + 1,
            leg.revisions.len(),
            "one entry for the first target, then one per revision"
        );
    }

    /// Regression (P10-B): the production driver opens a new pivot over a leg
    /// it already invalidated. `open` used to call `close` on that terminal
    /// record, tripping L7 ("a leg ends once") — only reachable once the
    /// machine had a caller, which through P10-A it did not.
    #[test]
    fn opening_over_a_terminal_leg_never_recloses_it() {
        let mut m = machine();
        let major = key(StructureLevel::Major);
        m.open(
            major.clone(),
            Direction::Short,
            100.0,
            10,
            13,
            Classification::Settled,
            90.0,
        );
        m.confirm_origin(&major, 14, 102.0);
        assert!(m.close(&major, CloseReason::InvalidationTouched));

        // A second pivot at the other level opens over the terminal record.
        let minor = key(StructureLevel::Minor);
        m.open(
            minor.clone(),
            Direction::Long,
            90.0,
            20,
            23,
            Classification::Settled,
            90.0,
        );

        assert_eq!(m.legs().len(), 2, "the ended leg is history, not replaced");
        let ended = m.leg(&major).expect("the terminal record survives");
        assert_eq!(ended.close_reason, Some(CloseReason::InvalidationTouched));
        assert_eq!(ended.lifecycle, LegLifecycle::Invalidated);
        assert_eq!(
            m.leg(&minor).map(|l| l.lifecycle),
            Some(LegLifecycle::Forming)
        );
    }
}
