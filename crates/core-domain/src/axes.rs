//! Axis values, signal taxonomy and the three reduction levels.
//!
//! Implements CR-18 (see `docs/conceptual-foundations/01-17-change-requests.md`)
//! and the produced spec text
//! `docs/conceptual-foundations/01-22-charter-close-and-axis-filing.md`.
//!
//! The model is **three reductions deep plus one envelope**. Each level collapses
//! over exactly one axis:
//!
//! ```text
//! candles -> indicators -> axes -> TimeframeState -> CrossTfState -> CrossPairState
//!                                                                   \-> ModelState
//! ```
//!
//! Every name is `<collapsed-dimension>_state`. `portfolio` is PME's domain and
//! appears nowhere in this file (V-107).
//!
//! # The load-bearing rule
//!
//! **A mean is legal only where the quantity shares a domain *and* means the same
//! thing for every member.** Everything else is a count, an ordinal mode, an
//! interval, or a comparison. This is why `Stage` has no arithmetic impls at
//! all, and why `Structure`'s event bias is counted rather than averaged.

use serde::{Deserialize, Serialize};

/// The four axes. A reading has exactly one owner axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisId {
    Direction,
    Momentum,
    Structure,
    Progression,
}

impl AxisId {
    pub const ALL: [AxisId; 4] = [
        AxisId::Direction,
        AxisId::Momentum,
        AxisId::Structure,
        AxisId::Progression,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AxisId::Direction => "direction",
            AxisId::Momentum => "momentum",
            AxisId::Structure => "structure",
            AxisId::Progression => "progression",
        }
    }

    /// Direction is the only axis whose value may be averaged across durations,
    /// because every duration measures the same thing on a shared domain.
    pub fn permits_mean(self) -> bool {
        matches!(self, AxisId::Direction)
    }

    /// Progression is ordinal. It can be summarised by a mode or an interval,
    /// never by arithmetic.
    pub fn is_ordinal(self) -> bool {
        matches!(self, AxisId::Progression)
    }
}

impl std::fmt::Display for AxisId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The meaning attached to a [`AxisValue::Signed`].
///
/// A `Signed` value without its domain is a bare float, and a bare float cannot be
/// aggregated safely — which is exactly why `NetEventBias` and `Acceleration` may
/// never be averaged together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    /// Direction axis position.
    DirectionalBias,
    /// Momentum axis acceleration.
    Acceleration,
    /// Structure axis bias accumulated from events.
    NetEventBias,
}

impl Domain {
    pub fn axis(self) -> AxisId {
        match self {
            Domain::DirectionalBias => AxisId::Direction,
            Domain::Acceleration => AxisId::Momentum,
            Domain::NetEventBias => AxisId::Structure,
        }
    }
}

/// The five progression anchors, in order.
///
/// **No `impl` of `Add`, `Sub`, `Mul` or `Div` exists for this type, and none
/// ever will.** `stage + 1` is a compile error (E0369) and `mean(stage)` is a
/// compile error (E0277). Both are proven by the `axis_types` trybuild
/// compile-fail test (V-37, V-38) — a test cannot prove a negative, only the
/// compiler can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Stage {
    Early,
    EarlyMid,
    Mid,
    Late,
    Terminal,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::Early,
        Stage::EarlyMid,
        Stage::Mid,
        Stage::Late,
        Stage::Terminal,
    ];

    /// The ordinal index of this anchor within [`Stage::ALL`].
    ///
    /// This exists so that "which of the five anchors" can be asked. It is **not**
    /// a value: nothing in the model subtracts one `Stage` from another, and this
    /// index must never be published on the wire as if it were a magnitude.
    pub fn anchor_index(self) -> usize {
        match self {
            Stage::Early => 0,
            Stage::EarlyMid => 1,
            Stage::Mid => 2,
            Stage::Late => 3,
            Stage::Terminal => 4,
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Why a value is absent.
///
/// CH-7: `UNDEFINED` is a distinct value. Never 0, never NaN, never a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code", content = "detail")]
pub enum UndefinedReason {
    /// Not enough bars have closed yet.
    InsufficientBars { available: usize, required: usize },
    /// A structural reference is still moving, so the stage is withheld.
    ReferenceMoving { reference: String },
    /// The input window was degenerate (zero range, all-equal values, …).
    DegenerateReference { detail: String },
    /// The reading was not computed because its producer is absent on this duration.
    NotApplicable { detail: String },
}

/// A typed axis value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AxisValue {
    /// `[-1, 1]` **with its meaning attached**.
    Signed { value: f64, domain: Domain },
    /// `[0, 100]` percentile against its own Tier A reference set (CH-4 permits
    /// percentile ranks; they are arithmetic, not inference).
    Rank { pct: f64 },
    /// One of five anchors. No arithmetic exists for this.
    Ordinal { stage: Stage },
    /// Absent, for a stated reason. Never 0, never NaN.
    Undefined { reason: UndefinedReason },
}

impl AxisValue {
    pub fn signed(value: f64, domain: Domain) -> Self {
        AxisValue::Signed { value, domain }
    }

    pub fn ordinal(stage: Stage) -> Self {
        AxisValue::Ordinal { stage }
    }

    pub fn rank(pct: f64) -> Self {
        AxisValue::Rank { pct }
    }

    pub fn undefined(reason: UndefinedReason) -> Self {
        AxisValue::Undefined { reason }
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, AxisValue::Undefined { .. })
    }

    /// The signed value, if this is one. Returns `None` for every other variant —
    /// which is what makes a bare cross-axis average a compile-time mistake at the
    /// call site rather than a silent zero at runtime.
    pub fn as_signed(&self) -> Option<(f64, Domain)> {
        match self {
            AxisValue::Signed { value, domain } => Some((*value, *domain)),
            _ => None,
        }
    }

    pub fn as_stage(&self) -> Option<Stage> {
        match self {
            AxisValue::Ordinal { stage } => Some(*stage),
            _ => None,
        }
    }
}

/// One axis's current value plus its typed signal history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisState {
    pub axis: AxisId,
    pub position: AxisValue,
    pub size: AxisValue,
    pub readiness: Readiness,
    /// 50 bars of typed signal history, oldest first. Hard-capped, no override
    /// (V-95q).
    pub readings: Vec<Signal>,
}

/// How completely the inputs behind a value are present.
///
/// This replaces the old `confidence` field. Readiness asks a mechanical question —
/// *are all the inputs there?* — and never a probabilistic one (CH-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Readiness {
    /// Every input present.
    Solid,
    /// At least one input absent or still moving. Rendered as an outline, never a
    /// fill and never a radius.
    Dashed,
}

impl AxisState {
    /// Push one signal onto the history, trimming to [`READINGS_CAP`].
    pub fn push_reading(&mut self, signal: Signal) {
        const READINGS_CAP: usize = 50;
        self.readings.push(signal);
        if self.readings.len() > READINGS_CAP {
            let excess = self.readings.len() - READINGS_CAP;
            self.readings.drain(0..excess);
        }
    }
}

/// Why a signal was emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    Threshold,
    Divergence,
    ZeroLineCross,
    CenterlineCross,
    LevelTouch,
    StateChange,
}

/// A typed reading. **A bare number is never a reading** (V-94q) — if it cannot be
/// named as one of these three variants, it does not belong in the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Signal {
    /// Priced and persistent — a zone, not an occurrence.
    Level {
        price_low: f64,
        price_high: f64,
        basis: String,
    },
    /// Dated occurrence. **Divergence is an Event subtype, not a fourth variant.**
    Event {
        kind: SignalKind,
        at_bar: usize,
        anchor: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtype: Option<String>,
    },
    /// A state change without a price.
    Transition {
        kind: String,
        from: String,
        to: String,
    },
}

impl Signal {
    pub fn is_event(&self) -> bool {
        matches!(self, Signal::Event { .. })
    }

    /// The `timeframe` component of an event's canonical identity
    /// `(source, kind, subject, anchor, timeframe)` lives on the event itself —
    /// events are never merged across durations.
    pub fn identity(&self, source: &str, subject: &str, timeframe_secs: u64) -> String {
        match self {
            Signal::Event {
                kind,
                at_bar,
                anchor,
                ..
            } => format!("{source}:{kind:?}:{subject}:{anchor}:{timeframe_secs}:{at_bar}"),
            other => format!("{source}:{other:?}:{subject}:{timeframe_secs}"),
        }
    }
}

/// How one axis was reduced across its members within a duration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisAggregation {
    /// Family members share one weight (they cannot disagree). Clusters have none.
    Family { weight: f64 },
    /// An owned reading contributes its own weight.
    Owned { weight: f64 },
    /// `diagnostics` never votes on an axis.
    NonVoting,
}

// ---------------------------------------------------------------------------
// Level 2 — CrossTfState
// ---------------------------------------------------------------------------

/// The legal reduction for one axis across durations.
///
/// `MeanAgreement` is `Direction` only. `StageDistribution` covers Momentum,
/// Structure and Progression — whose legal reductions are counts, comparisons,
/// ordinal mode and ordinal interval, never a mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum LadderAggregation {
    /// Direction only. Permitted because every duration measures the same thing on
    /// the shared `DirectionalBias` domain.
    MeanAgreement {
        mean: f64,
        agreement_pct: f64,
        gradient: f64,
    },
    /// Counts, comparisons and ordinal summaries. `rows` is **absent** by design —
    /// every per-duration value already ships in the twelve `TimeframeState`
    /// objects and is fetchable with `mme state --tf`. The reduction headlines
    /// only what no single duration can say.
    StageDistribution {
        histogram: StageHistogram,
        gradient: f64,
    },
}

/// Count per stage across durations. An ordinal histogram — never a mean.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StageHistogram {
    pub early: u32,
    pub early_mid: u32,
    pub mid: u32,
    pub late: u32,
    pub terminal: u32,
}

impl StageHistogram {
    pub fn increment(&mut self, stage: Stage) {
        match stage {
            Stage::Early => self.early += 1,
            Stage::EarlyMid => self.early_mid += 1,
            Stage::Mid => self.mid += 1,
            Stage::Late => self.late += 1,
            Stage::Terminal => self.terminal += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.early + self.early_mid + self.mid + self.late + self.terminal
    }

    /// The ordinal **mode**. This is a legal ordinal aggregate; `mean(stage)` is
    /// not, and does not compile.
    pub fn mode(&self) -> Option<Stage> {
        let counts = [
            (Stage::Early, self.early),
            (Stage::EarlyMid, self.early_mid),
            (Stage::Mid, self.mid),
            (Stage::Late, self.late),
            (Stage::Terminal, self.terminal),
        ];
        counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .filter(|(_, c)| *c > 0)
            .map(|(s, _)| *s)
    }

    /// The ordinal **interval** — min and max anchor, not a centre.
    pub fn range(&self) -> Option<(Stage, Stage)> {
        let present: Vec<Stage> = [
            (Stage::Early, self.early),
            (Stage::EarlyMid, self.early_mid),
            (Stage::Mid, self.mid),
            (Stage::Late, self.late),
            (Stage::Terminal, self.terminal),
        ]
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .map(|(s, _)| s)
        .collect();
        let lo = present.iter().min()?;
        let hi = present.iter().max()?;
        Some((*lo, *hi))
    }
}

/// Direction reduced across durations. The only mean in the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTfDirection {
    pub mean_bias: f64,
    pub agreement_pct: f64,
    pub gradient: f64,
    pub contributing_durations: Vec<u64>,
    pub dissenting_durations: Vec<u64>,
}

/// Momentum reduced across durations: breadth **count** plus a comparison.
///
/// Not a mean. A mean would not weight by acceleration magnitude, so +0.40 and
/// +0.01 would count equally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTfMomentum {
    pub breadth_positive_pct: f64,
    pub fast_vs_slow: FastVsSlow,
    pub gradient: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FastVsSlow {
    pub fast_secs: u64,
    pub fast_accel: f64,
    pub slow_secs: u64,
    pub slow_accel: f64,
    pub divergent: bool,
}

/// Structure reduced across durations: counts, convergence, readiness count.
///
/// Not a mean. A mean over `NetEventBias` would not weight by event count, so a
/// duration with 0 events and one with 8 would count equally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTfStructure {
    /// Within 1x ATR(n) of the **slower** duration (V-99q).
    pub levels_converging: bool,
    pub divergence_active_on: Vec<u64>,
    pub divergence_duration_count: u32,
    pub divergence_duration_total: u32,
    pub ready_duration_count: u32,
    pub total_durations: u32,
    pub gradient: f64,
}

/// Progression reduced across durations: ordinal mode, ordinal interval, leg
/// count. Never a mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTfProgression {
    pub modal_stage: Option<Stage>,
    pub stage_range: Option<StageRange>,
    pub histogram: StageHistogram,
    /// Legs are duration-local and are **never merged**, so the count of distinct
    /// legs is itself the finding: 12 legs over 12 durations means no
    /// correspondence.
    pub distinct_leg_count: u32,
    pub legs_correspond: bool,
    pub gradient: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRange {
    pub min: Stage,
    pub max: Stage,
}

/// Cross-duration agreement. **Counts and compares; never merges.** Two durations'
/// events are two events — the timeframe is part of the canonical identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossDurationAgreement {
    pub levels_converging: bool,
    pub divergence_duration_count: u32,
    pub divergence_duration_total: u32,
    pub concordant_duration_count: u32,
}

/// What a reduction is only as fresh as.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberFreshness {
    pub oldest_candle_age_secs: u64,
    pub stale_members: Vec<u64>,
}

/// **Level 2 — the reduction over one pair's durations.**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossTfState {
    pub durations: Vec<u64>,
    pub active_count: u32,
    pub direction: CrossTfDirection,
    pub momentum: CrossTfMomentum,
    pub structure: CrossTfStructure,
    pub progression: CrossTfProgression,
    pub cross_duration_agreement: CrossDurationAgreement,
    pub derived: DerivedState,
    pub member_freshness: MemberFreshness,
}

// ---------------------------------------------------------------------------
// Level 3 — CrossPairState
// ---------------------------------------------------------------------------

/// How dispersed the pairs are around the mean tilt.
///
/// Load-bearing: two markets can carry an identical breadth **count** and be
/// completely different. `mean_bias` and `bias_range` separate them (V-110).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossPairBreadth {
    pub bullish_count: u32,
    pub bearish_count: u32,
    pub unclear_count: u32,
    pub breadth_pct: f64,
    /// Mean of per-pair `CrossTfDirection::mean_bias`. Legal: shared domain, same
    /// meaning per pair.
    pub mean_bias: f64,
    /// max - min of the same per-pair values.
    pub bias_range: f64,
    pub dispersion: Dispersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dispersion {
    Narrow,
    Moderate,
    Wide,
}

/// Counts. Always legal at any level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub complete_pair_count: u32,
    pub stale_pair_count: u32,
    pub converging_level_pair_count: u32,
}

/// Concordance **measured**, not re-bucketed from breadth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Concordance {
    pub concordant_pair_count: u32,
    pub divergent_pair_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisDistribution {
    pub direction: std::collections::HashMap<String, u32>,
    pub progression: StageHistogram,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRank {
    pub pair: String,
    pub rank: u32,
    pub direction_bias: f64,
    pub agreement_pct: f64,
    pub modal_stage: Option<Stage>,
}

/// **Level 3 — the reduction over the watched pairs.**
///
/// V-108: no field here is a monotone re-bucketing of another field here. The
/// previous layer expressed one number four times as three enums plus a
/// distribution, so three of its four headline fields carried no information the
/// first did not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossPairState {
    pub breadth: CrossPairBreadth,
    pub axis_distribution: AxisDistribution,
    pub readiness: ReadinessCounts,
    pub coverage: Coverage,
    pub progression: CrossPairProgression,
    pub concordance: Concordance,
    pub asset_ranking: Vec<AssetRank>,
    pub member_freshness: PairFreshness,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessCounts {
    pub solid_duration_count: u32,
    pub dashed_count: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossPairProgression {
    pub histogram: StageHistogram,
    /// Ordinal mode across pairs. Never a mean.
    pub mode: Option<Stage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairFreshness {
    pub slowest_pair: String,
    pub oldest_candle_age_secs: u64,
}

// ---------------------------------------------------------------------------
// Derived — consensus and stability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consensus {
    pub agreeing_axes: u32,
    pub of: u32,
    pub axes: Vec<AxisId>,
    pub weakest_axis: Option<AxisId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stability {
    pub state: StabilityState,
    pub changes_last_50_bars: u32,
    pub window: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum StabilityState {
    Building,
    Holding,
    Unstable,
}

/// `diagnostics` is **not** an axis and never votes on one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostics {
    pub state: DiagnosticState,
    pub volatility_change_pct: f64,
    pub participation_change_pct: f64,
    pub basis: DiagnosticBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DiagnosticState {
    Chopping,
    Expanding,
    Settling,
    Coiling,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticBasis {
    pub bbwp_pct: f64,
    pub rvol_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedState {
    pub consensus: Consensus,
    pub stability: Stability,
}

// ---------------------------------------------------------------------------
// Level 1 — TimeframeState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeframeIdentity {
    pub instance_id: String,
    pub pair: String,
    pub exchange: String,
    pub timeframe_secs: u64,
    pub timeframe_label: String,
    pub candle_count: usize,
    pub last_closed_at: String,
    pub available_at: String,
    pub spec_version: String,
    pub build_version: String,
}

/// **Level 1 — the per-duration model output.**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeframeState {
    pub identity: TimeframeIdentity,
    /// Exactly four entries, one per [`AxisId`].
    pub axes: Vec<AxisState>,
    pub events: EventCounts,
    pub levels: Vec<Level>,
    pub diagnostics: Diagnostics,
    pub derived: DerivedState,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EventCounts {
    pub active_count: u32,
    pub retained_count: u32,
    pub emit_count: u32,
}

/// A priced, persistent zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub kind: String,
    pub price_low: f64,
    pub price_high: f64,
    pub basis: String,
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// **The final deliverable — `model_state`.**
///
/// Contains all three states (V-111). A point-in-time document must be
/// self-consistent, so it is one payload; a consumer reads nothing else to render
/// the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelState {
    pub version: String,
    pub build_version: String,
    pub generated_at_ms: u64,
    pub ladder_secs: Vec<u64>,
    /// Keyed by pair. Each entry holds that pair's cross-duration reduction and
    /// every one of its per-duration states.
    pub instances: std::collections::HashMap<String, InstanceStates>,
    pub cross_pair_state: CrossPairState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceStates {
    pub cross_tf_state: CrossTfState,
    /// Keyed by `timeframe_secs`. **All twelve are present** — a duration with no
    /// closed candle still appears, holding a typed `Undefined` (CH-7).
    pub durations: std::collections::HashMap<u64, TimeframeState>,
}

/// The socket splits [`ModelState`] into these three frame types, because they
/// invalidate at different rates: a 1s tick changes `timeframe_state["1"]` and
/// `cross_tf_state` but not `cross_pair_state` nor the eleven slow durations
/// (V-112).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum StateFrame {
    TimeframeState { state: TimeframeState },
    CrossTfState { state: CrossTfState },
    CrossPairState { state: CrossPairState },
}
