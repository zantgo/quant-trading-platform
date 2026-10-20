//! The leg record — Tier D (01-14).
//!
//! Thirteen scalars plus a bounded revision log, and **two disjoint counters**.
//! That separation is the whole point of the type: a reclassification of the
//! structural level and a retargeting of the reference target are different
//! events, and a model where both bumped one counter could not tell them apart
//! (L23, S-N5, S-N6).
//!
//! ## What this type makes impossible
//!
//! * **`progress` clamped.** [`Progress`] is a value or a typed absence. There
//!   is no path that produces a clamped number, because the arithmetic lives in
//!   [`progress_of`] and it returns `Progress`.
//! * **A stage from an unconfirmed origin.** [`LegRecord::stage`] returns
//!   `Option<Stage>` and is `None` while the leg is `forming`, while the target
//!   is moving, and while the classification is provisional (L6, L10, L22).
//! * **A terminal leg with no reason.** [`LegRecord::close`] requires the reason
//!   as an argument (L7).
//!
//! Everything else the record *stores* is data; everything it *publishes* is
//! derived, and derived values carry no version counter (L26).

use serde::{Deserialize, Serialize};

use crate::axes::{Stage, UndefinedReason};

/// Schema version of this record layout.
pub const LEG_SCHEMA_VERSION: u32 = 1;

/// `instrument × timeframe × level` — one current leg per triple (L8).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LegKey {
    pub instrument: String,
    pub timeframe_secs: u64,
    pub structure_level: StructureLevel,
}

/// Which structural scale a leg belongs to.
///
/// **Fixed and small by design** (L12). An unbounded level set would make Tier D
/// unbounded and break the O(1) retention guarantee, so adding a third level is
/// a schema change with a stated cost — not an open-ended hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureLevel {
    /// The dominant leg on the duration.
    Major,
    /// A counter-movement inside the major leg. Reported separately, never mixed.
    Minor,
}

impl StructureLevel {
    pub const ALL: [StructureLevel; 2] = [StructureLevel::Major, StructureLevel::Minor];

    pub fn label(self) -> &'static str {
        match self {
            StructureLevel::Major => "major",
            StructureLevel::Minor => "minor",
        }
    }
}

/// Whether the scale classification is settled or still provisional (S-N4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// The rank distribution is incomplete. Publishes **no stage** (L22).
    Provisional,
    /// The distribution was full at classification time.
    Settled,
}

/// Which way the leg runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Long,
    Short,
}

/// How the reference target was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetBasis {
    Extension,
    Measured,
    Volatility,
    Swing,
    /// The reference failed: the revision budget was reached. **Permanent** (L17).
    Unviable,
}

/// What ended the leg. A leg that reached its target is still `active` — the
/// target is an estimate, not an exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    OpposingSwingConfirmed,
    ExtensionReached,
    CharacterChange,
    VolumeClimaxFailure,
    InvalidationTouched,
    Superseded,
}

impl CloseReason {
    pub const ALL: [CloseReason; 6] = [
        CloseReason::OpposingSwingConfirmed,
        CloseReason::ExtensionReached,
        CloseReason::CharacterChange,
        CloseReason::VolumeClimaxFailure,
        CloseReason::InvalidationTouched,
        CloseReason::Superseded,
    ];

    /// The wire form. Named rather than derived so a consumer never depends on
    /// a `Debug` rendering.
    pub fn label(self) -> &'static str {
        match self {
            CloseReason::OpposingSwingConfirmed => "opposing_swing_confirmed",
            CloseReason::ExtensionReached => "extension_reached",
            CloseReason::CharacterChange => "character_change",
            CloseReason::VolumeClimaxFailure => "volume_climax_failure",
            CloseReason::InvalidationTouched => "invalidation_touched",
            CloseReason::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegLifecycle {
    /// Origin exists but is not confirmed. Publishes no stage (L6).
    Forming,
    Active,
    Completing,
    Completed,
    Invalidated,
    Superseded,
}

impl LegLifecycle {
    /// Whether a stage may be published in this state.
    ///
    /// `forming` never publishes one; the other states may, subject to the
    /// stability and classification gates that sit in front of them.
    pub fn permits_stage(self) -> bool {
        !matches!(self, LegLifecycle::Forming)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            LegLifecycle::Completed | LegLifecycle::Invalidated | LegLifecycle::Superseded
        )
    }
}

// ---------------------------------------------------------------------------
// Terminality — an ordinal assessment, never a probability, never completion
// ---------------------------------------------------------------------------

/// The three terminality states, named for **the direction of the claim**
/// (01-13 §"Terminality — definable now, ordinal only").
///
/// An earlier draft used `resolving`, which read equally as "the leg is
/// terminating" and as "uncertainty clearing in favour of continuation". The
/// names here cannot be read both ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminality {
    /// The structural evidence available supports further development.
    ContinuationSupported,
    /// The evidence does not distinguish the two.
    Indeterminate,
    /// The evidence supports a potential end to the current leg.
    TerminationEvidence,
}

/// The named criteria that may produce a terminality assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionCriterion {
    OpposingSwingConfirmed,
    CharacterChange,
    ClimaxFailure,
}

impl CompletionCriterion {
    pub const ALL: [CompletionCriterion; 3] = [
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::CharacterChange,
        CompletionCriterion::ClimaxFailure,
    ];
}

/// What one bar's evidence says about one completion criterion.
///
/// The third outcome is the load-bearing one: **not evaluated is not the same
/// as not fired**. Collapsing them is how "we did not look" becomes
/// `continuation_supported` — a claim the candles do not support (CH-5, CH-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionOutcome {
    /// The criterion fired on this bar.
    Fired,
    /// The criterion was evaluated against this bar and did not fire.
    NotFired,
    /// Nothing in this bar's evidence can decide the criterion.
    Unknown,
}

/// The assessment, with the criteria that produced it — 01-13: "an ordinal plus
/// the **named** criteria that produced it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalityAssessment {
    pub terminality: Terminality,
    pub criteria: Vec<CompletionCriterion>,
}

impl TerminalityAssessment {
    /// No criterion has fired and none has been evaluated.
    pub fn indeterminate() -> Self {
        Self {
            terminality: Terminality::Indeterminate,
            criteria: Vec::new(),
        }
    }
}

/// Derive terminality from **fired criteria only**.
///
/// Never from `progress`, never from the stage, never from a probability. A leg
/// can be `early` and carry termination evidence, or `late` and carry none; the
/// two are not derivable from each other.
///
/// This is the **lower bound**: with nothing known about whether the criteria
/// were even evaluated, absence of a fired criterion is absence of evidence, not
/// evidence of continuation. Use [`assess_terminality_from_outcomes`] when the
/// evaluated set is known.
///
/// And this **never closes a leg**: the leg completes when a named criterion
/// actually fires with a `close_reason`, which is a separate, timestamped event
/// (CG10).
pub fn assess_terminality(fired: &[CompletionCriterion]) -> TerminalityAssessment {
    // **Distinct** criteria only. Two reports of the same criterion are one
    // criterion — and without this, a caller that fires the same evidence twice
    // would manufacture the two-criteria threshold and claim termination
    // evidence it does not have. Found by the adversarial suite (SV-B).
    let mut criteria: Vec<CompletionCriterion> = Vec::new();
    for c in fired {
        if !criteria.contains(c) {
            criteria.push(*c);
        }
    }
    criteria.sort_by_key(|c| format!("{c:?}"));
    let terminality = match criteria.len() {
        0 => Terminality::Indeterminate,
        1 => Terminality::Indeterminate,
        // Two independent criteria pointing the same way is evidence; one is a
        // coincidence until it is confirmed.
        _ => Terminality::TerminationEvidence,
    };
    TerminalityAssessment {
        terminality,
        criteria,
    }
}

/// Derive terminality from **per-criterion outcomes**.
///
/// This is the full assessment and the only rule that can say
/// `continuation_supported`: all three named criteria must have been
/// **evaluated** on this bar and none may have fired. A criterion that is
/// absent from `outcomes` is `Unknown` — not evaluated — and any `Unknown`
/// makes the result `Indeterminate`, because "we did not look" is not evidence
/// (CH-5, CH-7).
///
/// The fired criteria are returned by name, exactly as
/// [`assess_terminality`] returns them. Two or more fired criteria are
/// termination evidence; one is still a coincidence.
pub fn assess_terminality_from_outcomes(
    outcomes: &[(CompletionCriterion, CriterionOutcome)],
) -> TerminalityAssessment {
    let mut fired: Vec<CompletionCriterion> = Vec::new();
    let mut all_evaluated = true;

    for criterion in CompletionCriterion::ALL {
        let mut saw_fired = false;
        let mut saw_not_fired = false;
        let mut saw_unknown = false;
        for (c, outcome) in outcomes {
            if *c != criterion {
                continue;
            }
            match outcome {
                CriterionOutcome::Fired => saw_fired = true,
                CriterionOutcome::NotFired => saw_not_fired = true,
                CriterionOutcome::Unknown => saw_unknown = true,
            }
        }
        if saw_fired {
            fired.push(criterion);
        } else if !saw_not_fired || saw_unknown {
            // Absent, or explicitly undecidable. Both mean "not evaluated" for
            // the purpose of claiming continuation.
            all_evaluated = false;
        }
    }

    let terminality = if fired.len() >= 2 {
        Terminality::TerminationEvidence
    } else if fired.is_empty() && all_evaluated {
        Terminality::ContinuationSupported
    } else {
        // Zero or one fired criterion with incomplete coverage, or exactly one
        // fired criterion: the evidence points nowhere yet.
        Terminality::Indeterminate
    };

    fired.sort_by_key(|c| format!("{c:?}"));
    TerminalityAssessment {
        terminality,
        criteria: fired,
    }
}

/// Terminality with no fired criteria is **indeterminate**, not
/// `continuation_supported`: absence of termination evidence is not evidence of
/// continuation, and calling it continuation would be a claim the candles do not
/// support (CH-5, CH-7).
pub fn terminality_of_evidence(criteria: &[CompletionCriterion]) -> TerminalityAssessment {
    assess_terminality(criteria)
}

/// Assess terminality, the name the rest of the model uses.
pub fn terminality(criteria: &[CompletionCriterion]) -> TerminalityAssessment {
    assess_terminality(criteria)
}

/// Progress: a value, or a **typed absence**.
///
/// CH-7: absence is a value, never `0`, never `NaN`. L13/L14.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// `(P - O) / (T - O)`, unclamped (L4).
    Value {
        value: f64,
    },
    Undefined {
        reason: UndefinedReason,
    },
}

impl Progress {
    pub fn is_undefined(&self) -> bool {
        matches!(self, Progress::Undefined { .. })
    }

    /// The number, when there is one. `None` is the answer, not a `0.0`.
    pub fn value(&self) -> Option<f64> {
        match self {
            Progress::Value { value } => Some(*value),
            Progress::Undefined { .. } => None,
        }
    }
}

/// The progress arithmetic (§4).
///
/// `T == O`, or `|T - O|` below the leg's **own** frozen noise floor, is
/// `UNDEFINED` — never a substituted denominator, never `0`, never `NaN`
/// (§4.0). The floor is ATR-scaled rather than an absolute price distance, so
/// it satisfies P2 (no absolute) and P3 (the same rule at every duration).
pub fn progress_of(origin: f64, price: f64, target: Option<f64>, noise_floor: f64) -> Progress {
    let Some(t) = target else {
        return Progress::Undefined {
            reason: UndefinedReason::DegenerateReference {
                detail: "no reference target".into(),
            },
        };
    };
    // A non-finite input is a degenerate reference, not a measurement. Without
    // this, `T = NaN` yields `NaN` (every comparison with NaN is false, so the
    // guards below fall straight through) and `T = inf` yields `0.0` — which
    // reads as "at the origin", a claim rather than an absence. L14: never NaN,
    // never a silent zero.
    if !t.is_finite() || !origin.is_finite() || !price.is_finite() {
        return Progress::Undefined {
            reason: UndefinedReason::DegenerateReference {
                detail: format!("non-finite reference or price (O={origin}, P={price}, T={t})"),
            },
        };
    }
    let span = t - origin;
    if span == 0.0 {
        return Progress::Undefined {
            reason: UndefinedReason::DegenerateReference {
                detail: "T == O".into(),
            },
        };
    }
    if span.abs() < noise_floor {
        return Progress::Undefined {
            reason: UndefinedReason::DegenerateReference {
                detail: format!("|T - O| = {:.8} below the leg's noise floor", span.abs()),
            },
        };
    }
    Progress::Value {
        value: (price - origin) / span,
    }
}

/// One entry of the bounded revision log (01-14 §2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    /// `structure_version` **or** `target_version`, whichever moved — never both
    /// (L23).
    pub counter: RevisionCounter,
    pub at_bar: usize,
    pub from: f64,
    pub to: f64,
    pub basis: TargetBasis,
    /// The reading that triggered it, for the audit trail.
    pub trigger: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionCounter {
    Structure,
    Target,
}

/// The Tier D record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegRecord {
    /// Stable identity — **survives target revisions** (L1).
    pub leg_id: String,
    /// What this leg belongs to.
    pub instrument: String,
    /// The duration. A leg never spans durations.
    pub timeframe_secs: u64,
    /// Which structural scale this leg belongs to.
    ///
    /// A **field, not part of the record's identity**: `leg_id` identifies the
    /// record, and 01-14 §2.1 says the `(instrument, duration, level)` triple
    /// identifies *which leg a reading is about*. A reclassification therefore
    /// moves the leg between lookups rather than minting a new record.
    pub structure_level: StructureLevel,

    /// The swing-selection policy in force at admission. Shared by every leg
    /// under the same policy; legs with different values are not comparable (L27).
    pub swing_policy_version: u32,
    /// Bumped **only** by an origin revision or a level reclassification (L25).
    pub structure_version: u32,
    /// Bumped **only** when `target_price` changes (S-N6).
    pub target_version: u32,
    pub schema_version: u32,

    pub origin_price: f64,
    /// When the pivot printed.
    pub origin_occurred_at: usize,
    /// When the system could first know it was a pivot. `>= origin_occurred_at`
    /// **always** (L2) — collapsing the two is how a replay learns to win.
    pub origin_available_at: usize,
    pub origin_confirmed: bool,
    pub direction: Direction,

    pub classification: Classification,
    /// The displacement's percentile rank at classification time — the audit
    /// trail for the level. Frozen with the threshold (L21).
    pub classification_rank_pct: Option<f64>,
    /// The bar of the most recent observation. Makes an out-of-order bar a
    /// refusal rather than a silent reordering.
    pub last_observed_bar: Option<usize>,

    pub target_price: Option<f64>,
    pub target_basis: TargetBasis,
    pub target_revision_count: u32,
    pub target_stable: bool,

    pub invalidation_price: Option<f64>,
    pub extreme_price: f64,

    pub lifecycle: LegLifecycle,
    pub close_reason: Option<CloseReason>,

    /// The current ordinal assessment of continuation versus termination.
    ///
    /// It is **consumed** by the machine (updated from the criteria the machine
    /// evaluated on a bar) and published beside the stage. It never closes the
    /// leg (CG10, I23): a `termination_evidence` label is not a `close_reason`.
    pub terminality: TerminalityAssessment,
    /// Criteria that have fired so far, distinct and bounded by
    /// [`CompletionCriterion::ALL`]. Named evidence, not a count.
    pub fired_criteria: Vec<CompletionCriterion>,
    /// Criteria this machine has actually evaluated. A criterion that is not in
    /// this list is `Unknown` and cannot support a continuation claim.
    pub evaluated_criteria: Vec<CompletionCriterion>,

    /// The leg's own realised-range noise floor, **frozen at creation** (L21).
    pub noise_floor: f64,
    /// The scale rank threshold, **frozen at creation** (L21).
    pub rank_threshold_pct: f64,
    /// Consecutive unrevised bars so far — the stability machine's counter (L16).
    pub unrevised_bars: u32,
    /// How many times the reference has been recomputed since it was declared
    /// unviable. Kept so the record explains itself after the fact.
    pub bars_since_origin: u32,

    /// Bounded: the newest N revisions, oldest evicted (Tier D, O(1)).
    pub revisions: Vec<Revision>,
}

/// How many revisions the log retains.
///
/// Bounded because the record is Tier D: its cost must be O(1) in the number of
/// bars, not O(revisions).
pub const REVISION_LOG_CAP: usize = 16;

/// The stability window, in **observations** (§4.5).
///
/// An observation count, not a price or time threshold: a bar count is the same
/// count on every duration by construction, so it does not violate P2.
pub const STABILITY_WINDOW: u32 = 3;

/// After this many revisions the reference is declared unviable (L17).
pub const REVISION_BUDGET: u32 = 6;

impl LegRecord {
    /// Begin a leg. The only constructor: every field a leg must have is a
    /// parameter, so a new leg cannot start half-populated.
    /// The lookup key: which leg a reading is about.
    pub fn key(&self) -> LegKey {
        LegKey {
            instrument: self.instrument.clone(),
            timeframe_secs: self.timeframe_secs,
            structure_level: self.structure_level,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open(
        leg_id: impl Into<String>,
        key: LegKey,
        direction: Direction,
        origin_price: f64,
        origin_occurred_at: usize,
        origin_available_at: usize,
        classification: Classification,
        noise_floor: f64,
        rank_threshold_pct: f64,
        swing_policy_version: u32,
    ) -> Self {
        // L2 is enforced here rather than documented: a caller that passes an
        // availability before the occurrence has made a claim that cannot be
        // true, and the record would carry it silently.
        assert!(
            origin_available_at >= origin_occurred_at,
            "L2 violated: origin_available_at {origin_available_at} < \
             origin_occurred_at {origin_occurred_at}"
        );
        LegRecord {
            leg_id: leg_id.into(),
            instrument: key.instrument,
            timeframe_secs: key.timeframe_secs,
            structure_level: key.structure_level,
            swing_policy_version,
            structure_version: 0,
            target_version: 0,
            schema_version: LEG_SCHEMA_VERSION,
            origin_price,
            origin_occurred_at,
            origin_available_at,
            origin_confirmed: false,
            direction,
            classification,
            classification_rank_pct: None,
            last_observed_bar: None,
            target_price: None,
            target_basis: TargetBasis::Volatility,
            target_revision_count: 0,
            target_stable: false,
            invalidation_price: None,
            extreme_price: origin_price,
            lifecycle: LegLifecycle::Forming,
            close_reason: None,
            terminality: TerminalityAssessment::indeterminate(),
            fired_criteria: Vec::new(),
            evaluated_criteria: Vec::new(),
            noise_floor,
            rank_threshold_pct,
            unrevised_bars: 0,
            bars_since_origin: 0,
            revisions: Vec::new(),
        }
    }

    /// The bar at which this record first became readable (L3).
    pub fn available_at(&self) -> usize {
        self.origin_available_at
    }

    /// The target is unviable, so no stage can ever be published again (L17).
    pub fn is_unviable(&self) -> bool {
        self.target_basis == TargetBasis::Unviable
    }

    /// Progress against the **current** reference (L4: never clamped).
    pub fn progress(&self, price: f64) -> Progress {
        progress_of(
            self.origin_price,
            price,
            self.target_price,
            self.noise_floor,
        )
    }

    /// Movement, read from price and **never** inferred from `progress` (P2).
    pub fn movement_since(&self, previous_price: f64, price: f64) -> f64 {
        price - previous_price
    }

    /// The stage, or `None` when one may not be published.
    ///
    /// The gates, in order: the leg must not be `forming` (L6); the reference
    /// must be stable (L10); the basis must not be unviable (L17); the
    /// classification must be settled (L22); and progress must be defined
    /// (L13). Each is a *withholding*, never a substitution.
    pub fn stage(&self, price: f64) -> Option<Stage> {
        if !self.lifecycle.permits_stage() {
            return None;
        }
        if !self.target_stable {
            return None;
        }
        if self.is_unviable() {
            return None;
        }
        if matches!(self.classification, Classification::Provisional) {
            return None;
        }
        let progress = self.progress(price).value()?;
        Some(stage_from_progress(progress))
    }

    /// The published reading: the three fields that make a progress number
    /// interpretable (L5, L11).
    pub fn reading(&self, price: f64) -> LegReading {
        LegReading {
            leg_id: self.leg_id.clone(),
            structure_level: self.structure_level,
            progress: self.progress(price),
            stage: self.stage(price),
            target_version: self.target_version,
            target_stable: self.target_stable,
            structure_version: self.structure_version,
            lifecycle: self.lifecycle,
            terminality: self.terminality.clone(),
        }
    }

    /// Close the leg. The reason is a **parameter**, so a terminal leg cannot
    /// exist without one (L7).
    pub fn close(&mut self, reason: CloseReason) {
        assert!(
            !self.lifecycle.is_terminal(),
            "L7: `{}` is already terminal; a leg ends once",
            self.leg_id
        );
        self.lifecycle = match reason {
            CloseReason::InvalidationTouched => LegLifecycle::Invalidated,
            CloseReason::Superseded => LegLifecycle::Superseded,
            _ => LegLifecycle::Completed,
        };
        self.close_reason = Some(reason);
    }

    /// Append to the bounded revision log.
    ///
    /// Public because the *writer* lives in `market-analyzer::leg`: the record
    /// stores, the machine moves. Everything that can change a counter is still
    /// only reachable through `LegMachine`, so exposing the log append does not
    /// expose a second writer.
    pub fn push_revision(&mut self, revision: Revision) {
        self.revisions.push(revision);
        // Bounded (Tier D): the log is a tail, not a history. The counters are
        // the durable totals.
        if self.revisions.len() > REVISION_LOG_CAP {
            let excess = self.revisions.len() - REVISION_LOG_CAP;
            self.revisions.drain(0..excess);
        }
    }
}

/// The stage bands (§4.4).
///
/// Five anchors over an unclamped progress value. The bands are relative to the
/// leg's own reference — two instruments of different volatility reach `late` at
/// different progress values, which is correct: the band describes *this* leg's
/// extent, not an absolute distance (P2).
pub fn stage_from_progress(progress: f64) -> Stage {
    if progress < 0.25 {
        Stage::Early
    } else if progress < 0.75 {
        Stage::EarlyMid
    } else if progress <= 1.0 {
        Stage::Mid
    } else if progress <= 1.5 {
        Stage::Late
    } else {
        Stage::Terminal
    }
}

/// Everything a consumer is allowed to read about one leg at one bar.
///
/// L5/L11: a reading is only interpretable with `structure_level`,
/// `target_version` and `target_stable` beside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegReading {
    pub leg_id: String,
    pub structure_level: StructureLevel,
    pub progress: Progress,
    pub stage: Option<Stage>,
    pub target_version: u32,
    pub target_stable: bool,
    pub structure_version: u32,
    pub lifecycle: LegLifecycle,
    /// The ordinal continuation-versus-termination assessment. Never a
    /// probability, never a completion (CG10, I23).
    pub terminality: TerminalityAssessment,
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

    #[test]
    fn a_leg_starts_forming_with_no_stage_and_no_target() {
        let leg = LegRecord::open(
            "leg-1",
            key(StructureLevel::Major),
            Direction::Long,
            100.0,
            10,
            13,
            Classification::Settled,
            0.5,
            80.0,
            1,
        );
        assert_eq!(leg.lifecycle, LegLifecycle::Forming);
        assert_eq!(
            leg.stage(110.0),
            None,
            "L6: a forming leg publishes no stage"
        );
        assert!(leg.progress(110.0).is_undefined(), "no target yet");
    }

    #[test]
    fn l2_is_checked_at_construction_not_documented() {
        let result = std::panic::catch_unwind(|| {
            LegRecord::open(
                "leg-1",
                key(StructureLevel::Major),
                Direction::Long,
                100.0,
                13,
                10, // available BEFORE it occurred
                Classification::Settled,
                0.5,
                80.0,
                1,
            )
        });
        assert!(
            result.is_err(),
            "L2 must refuse a leg that claims to know its origin early"
        );
    }

    #[test]
    fn a_leg_cannot_close_twice() {
        let mut leg = LegRecord::open(
            "leg-1",
            key(StructureLevel::Major),
            Direction::Long,
            100.0,
            10,
            13,
            Classification::Settled,
            0.5,
            80.0,
            1,
        );
        leg.close(CloseReason::InvalidationTouched);
        assert_eq!(leg.close_reason, Some(CloseReason::InvalidationTouched));
        let second = std::panic::catch_unwind(move || {
            let mut l = leg;
            l.close(CloseReason::Superseded);
        });
        assert!(second.is_err(), "L7: a terminal leg records one reason");
    }

    #[test]
    fn progress_is_undefined_at_a_degenerate_reference_and_a_floor() {
        // T == O exactly.
        assert!(progress_of(100.0, 101.0, Some(100.0), 0.5).is_undefined());
        // |T - O| below the leg's own floor.
        assert!(progress_of(100.0, 101.0, Some(100.1), 0.5).is_undefined());
        // Above the floor: a number.
        assert_eq!(
            progress_of(100.0, 105.0, Some(110.0), 0.5).value(),
            Some(0.5)
        );
    }

    #[test]
    fn progress_is_never_clamped() {
        // Beyond the target and back against the leg, both survive (L4).
        assert_eq!(
            progress_of(100.0, 130.0, Some(110.0), 0.5).value(),
            Some(3.0)
        );
        assert_eq!(
            progress_of(100.0, 98.0, Some(110.0), 0.5).value(),
            Some(-0.2)
        );
    }

    #[test]
    fn a_short_leg_needs_no_branch() {
        // T < O, denominator negative, falling P gives positive progress.
        assert_eq!(progress_of(100.0, 95.0, Some(90.0), 0.5).value(), Some(0.5));
    }

    #[test]
    fn the_revision_log_is_bounded() {
        let mut leg = LegRecord::open(
            "leg-1",
            key(StructureLevel::Major),
            Direction::Long,
            100.0,
            10,
            13,
            Classification::Settled,
            0.5,
            80.0,
            1,
        );
        for i in 0..(REVISION_LOG_CAP + 5) {
            leg.push_revision(Revision {
                counter: RevisionCounter::Target,
                at_bar: i,
                from: 100.0,
                to: 101.0,
                basis: TargetBasis::Volatility,
                trigger: "test".into(),
            });
        }
        assert_eq!(leg.revisions.len(), REVISION_LOG_CAP);
        assert_eq!(leg.revisions[0].at_bar, 5, "the oldest entries are evicted");
    }
}
