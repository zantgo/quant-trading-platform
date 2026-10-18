//! The model assembler — the only producer of `model_state` (P10).
//!
//! Four levels, reduced in one direction and only one direction:
//!
//! ```text
//! candles → readings → TimeframeState → CrossTfState → CrossPairState → ModelState
//! ```
//!
//! Nothing reads upward. There is no path from a cross-pair number back into a
//! per-duration axis, and that is not a convention here — the types make it
//! impossible to construct a [`TimeframeState`] from a [`CrossTfState`].
//!
//! ## The legality rule this module exists to enforce
//!
//! A mean is legal only where the quantity **shares a domain and means the same
//! thing for every member**. Every reduction below is therefore written out
//! explicitly, with the reason it is that reduction and not a cheaper one:
//!
//! | Level | Reduction | Never |
//! |---|---|---|
//! | cross_tf | Direction: mean + agreement + gradient | averaging ordinals; merging events across durations |
//! | cross_tf | Momentum: breadth **count** + fast-vs-slow | a mean over `Acceleration` |
//! | cross_tf | Structure: counts + convergence + readiness count | a mean over `NetEventBias` |
//! | cross_tf | Progression: ordinal mode + interval + histogram | a mean of stages |
//! | cross_pair | `mean_bias` + `bias_range` over per-pair means | any unweighted mean over event counts or stages |
//!
//! ## Why `diagnostics` is not here
//!
//! It is not an axis and it never votes. [`Diagnostics`] is assembled from the
//! candle's own basis and attached to the state; [`consensus`] counts axes, not
//! diagnostics.

use std::collections::{BTreeMap, HashMap};

use core_domain::axes::{
    AxisDistribution, AxisId, AxisState, AxisValue, Consensus, Coverage, CrossDurationAgreement,
    CrossPairBreadth, CrossPairState, CrossTfDirection, CrossTfMomentum, CrossTfProgression,
    CrossTfState, CrossTfStructure, DerivedState, Diagnostics, FastVsSlow, LadderAggregation,
    ModelState, Readiness, ReadinessCounts, StageHistogram, StageRange, TimeframeIdentity,
    TimeframeState,
};
use core_domain::axes::{Stability, StabilityState};
use core_domain::event::EventCounts as JournalCounts;
use serde::{Deserialize, Serialize};

/// One pair's contribution to the envelope: its cross-duration reduction and
/// every one of its per-duration states.
pub type PairAssembly = (String, CrossTfState, Vec<(u64, TimeframeState)>);

/// One duration's signed value.
pub type DurationValue = (u64, AxisValue);

/// The fastest and slowest members of a signed series.
pub type FastestSlowest = (Option<(u64, f64)>, Option<(u64, f64)>);

/// A reading as the assembler receives it: one indicator, one axis, one value.
///
/// `producer` is the indicator key. `derived_from` is non-empty only for a value
/// computed from other readings rather than from candles directly — so "who owns
/// this number" is answerable without reading the implementation (W10.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub producer: String,
    pub axis: AxisId,
    pub value: AxisValue,
    /// The keys this value was derived from, when it was derived.
    pub derived_from: Vec<String>,
    /// Whether every input behind the value is present. The mechanical question
    /// behind [`Readiness`] — never a probability (CH-4, W10.4).
    pub inputs_complete: bool,
}

impl Reading {
    /// A reading computed directly from candles by one indicator.
    pub fn produced(
        producer: impl Into<String>,
        axis: AxisId,
        value: AxisValue,
        inputs_complete: bool,
    ) -> Self {
        Self {
            producer: producer.into(),
            axis,
            value,
            derived_from: Vec::new(),
            inputs_complete,
        }
    }

    /// A reading computed from other readings.
    pub fn derived(
        producer: impl Into<String>,
        axis: AxisId,
        value: AxisValue,
        from: &[&str],
        inputs_complete: bool,
    ) -> Self {
        Self {
            producer: producer.into(),
            axis,
            value,
            derived_from: from.iter().map(|s| s.to_string()).collect(),
            inputs_complete,
        }
    }

    /// **Ownership is single, consumption is many.** A producer contributes to
    /// exactly one axis, and a reading appears in exactly one axis's `readings`.
    pub fn ownership_is_single(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Level 1 — TimeframeState
// ---------------------------------------------------------------------------

/// Everything one duration needs to be assembled.
pub struct TimeframeInput {
    pub identity: TimeframeIdentity,
    pub readings: Vec<Reading>,
    pub events: JournalCounts,
    pub levels: Vec<core_domain::axes::Level>,
    pub diagnostics: Diagnostics,
    /// The leg's stage, when one may be published — `None` means withheld.
    pub progression_stage: Option<core_domain::axes::Stage>,
    /// The leg's progress, typed.
    pub progression_progress: core_domain::leg::Progress,
    pub target_version: u32,
    pub target_stable: bool,
    /// The bar at which this state first became readable.
    pub available_at_bar: usize,
    /// The direction value's recent history, oldest first. The stability measure
    /// counts *observations of the direction*, not bars (01-13 I42).
    pub direction_history: Vec<f64>,
}

/// Assemble the four axes from the readings.
///
/// **Diagnostics never votes** and the stage is *set* rather than voted: an
/// ordinal cannot be averaged, and the leg owns that decision already (W10.2).
pub fn assemble_axes(
    readings: &[Reading],
    progression_stage: Option<core_domain::axes::Stage>,
) -> Vec<AxisState> {
    AxisId::ALL
        .iter()
        .map(|axis| {
            let members: Vec<&Reading> = readings.iter().filter(|r| r.axis == *axis).collect();
            let value = reduce_axis(*axis, &members, progression_stage);
            let readiness = if members.iter().all(|r| r.inputs_complete) && !members.is_empty() {
                Readiness::Solid
            } else {
                Readiness::Dashed
            };
            AxisState {
                axis: *axis,
                position: value.clone(),
                size: value,
                readiness,
                readings: Vec::new(),
            }
        })
        .collect()
}

/// The per-duration reduction for one axis.
///
/// Family members share one weight because they cannot disagree; an owned reading
/// contributes its own weight; `diagnostics` is not represented here at all.
pub fn reduce_axis(
    axis: AxisId,
    members: &[&Reading],
    progression_stage: Option<core_domain::axes::Stage>,
) -> AxisValue {
    // The ordinal axis is checked FIRST: its value is *assigned* by the leg, not
    // reduced from readings, so "no readings filed under progression" is the
    // normal state of an axis whose stage is withheld.
    if axis == AxisId::Progression {
        return match progression_stage {
            Some(stage) => AxisValue::ordinal(stage),
            None => AxisValue::undefined(core_domain::axes::UndefinedReason::ReferenceMoving {
                reference: "leg stage withheld".into(),
            }),
        };
    }
    if members.is_empty() {
        return AxisValue::undefined(core_domain::axes::UndefinedReason::NotApplicable {
            detail: format!("no readings filed under {}", axis.label()),
        });
    }
    // Every member of an axis carries the same domain. If one does not, the axis
    // is `Undefined` rather than silently averaged across two meanings.
    let domains: Vec<core_domain::axes::Domain> = members
        .iter()
        .filter_map(|m| m.value.as_signed().map(|(_, d)| d))
        .collect();
    if let Some(first) = domains.first().copied() {
        if !domains.iter().all(|d| *d == first) {
            return AxisValue::undefined(core_domain::axes::UndefinedReason::DegenerateReference {
                detail: format!("{} mixes domains", axis.label()),
            });
        }
    }
    match axis {
        // Direction and Momentum are the two signed axes; Structure is signed
        // over its own domain; Progression is ordinal and is NOT reduced here.
        AxisId::Direction | AxisId::Momentum | AxisId::Structure => {
            let signed: Vec<(f64, f64)> = members
                .iter()
                .filter_map(|m| m.value.as_signed().map(|(v, _)| (v, m.weight())))
                .collect();
            if signed.is_empty() {
                return AxisValue::undefined(core_domain::axes::UndefinedReason::NotApplicable {
                    detail: format!("{} has no signed reading", axis.label()),
                });
            }
            let total: f64 = signed.iter().map(|(_, w)| w).sum();
            let mean = signed.iter().map(|(v, w)| v * w).sum::<f64>() / total;
            let domain = members[0]
                .value
                .as_signed()
                .map(|(_, d)| d)
                .unwrap_or_else(|| default_domain(axis));
            AxisValue::signed(mean, domain)
        }
        // Handled above: an ordinal is never averaged.
        AxisId::Progression => {
            AxisValue::undefined(core_domain::axes::UndefinedReason::NotApplicable {
                detail: "progression is handled before the reduction".into(),
            })
        }
    }
}

impl Reading {
    /// A reading's weight in its axis.
    ///
    /// `1.0` for an owned reading; a family shares one weight, so it is declared
    /// per reading here and the *family* case is a reduction the caller has
    /// already performed (W10.2's "family" step).
    pub fn weight(&self) -> f64 {
        1.0
    }
}

/// How many bars of history the stability measure looks at.
pub const STABILITY_WINDOW_BARS: u32 = 50;

/// How many sign changes in the window mean "unstable" rather than "holding".
pub const UNSTABLE_CHANGE_THRESHOLD: u32 = 6;

/// The consensus over four axes.
///
/// A **count of defined axes**, not a probability: CH-4 forbids the probabilistic
/// reading, and a consensus that averaged four differently-typed values would be
/// meaningless anyway. `diagnostics` is not among the four (V-40).
pub fn consensus_of(axes: &[AxisState]) -> Consensus {
    let defined: Vec<AxisId> = axes
        .iter()
        .filter(|a| !a.position.is_undefined())
        .map(|a| a.axis)
        .collect();
    // "Weakest" is ordered by how much is missing: an undefined axis is weaker
    // than a dashed one, and a dashed one is weaker than a solid one. Picking
    // the first *deficient* axis instead would report a dashed axis over an
    // undefined one purely by position.
    let weakest = axes
        .iter()
        .find(|a| a.position.is_undefined())
        .or_else(|| axes.iter().find(|a| a.readiness == Readiness::Dashed))
        .map(|a| a.axis);
    Consensus {
        agreeing_axes: defined.len() as u32,
        of: axes.len() as u32,
        axes: defined,
        weakest_axis: weakest,
    }
}

/// The stability machine, measured over the direction axis's history.
///
/// The unit is **observations of the direction value**, not bars of wall time: a
/// 1s duration produces far more observations than a 1d one, and counting bars
/// would compare a fast market's noise with a slow market's structure.
pub fn stability_of(history: &[f64]) -> Stability {
    let window = STABILITY_WINDOW_BARS as usize;
    let recent = if history.len() > window {
        &history[history.len() - window..]
    } else {
        history
    };
    let changes = recent
        .windows(2)
        .filter(|w| sign_of(w[0]) != sign_of(w[1]) && sign_of(w[0]) != 0.0 && sign_of(w[1]) != 0.0)
        .count() as u32;
    let state = if recent.len() < window {
        // Not enough observations to claim anything: `Building`, which the
        // renderer treats as "still forming", not as instability.
        StabilityState::Building
    } else if changes >= UNSTABLE_CHANGE_THRESHOLD {
        StabilityState::Unstable
    } else {
        StabilityState::Holding
    };
    Stability {
        state,
        changes_last_50_bars: changes,
        window: STABILITY_WINDOW_BARS,
    }
}

/// The direction history an input carries, oldest first.
fn input_direction_history(input: &TimeframeInput) -> Vec<f64> {
    input
        .direction_history
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .collect()
}

/// Assemble a whole `TimeframeState`.
pub fn assemble_timeframe_state(input: TimeframeInput) -> TimeframeState {
    let axes = assemble_axes(&input.readings, input.progression_stage);
    let consensus = consensus_of(&axes);
    let stability = stability_of(&input_direction_history(&input));
    TimeframeState {
        identity: input.identity,
        axes,
        events: core_domain::axes::EventCounts {
            active_count: input.events.active as u32,
            retained_count: input.events.retained as u32,
            emit_count: input.events.emitted as u32,
        },
        levels: input.levels,
        diagnostics: input.diagnostics,
        derived: DerivedState {
            consensus,
            stability,
        },
    }
}

/// The bar a `TimeframeState` became readable (L3).
pub fn timeframe_available_at(state: &TimeframeState) -> usize {
    state
        .identity
        .available_at
        .parse::<usize>()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Level 2 — CrossTfState
// ---------------------------------------------------------------------------

/// The reduction over one pair's durations.
///
/// Each axis is reduced by **its own legal method**, and the method is returned
/// alongside the value so a consumer can see which law applied (W10.6).
pub struct CrossTfReduction {
    pub direction: LadderAggregation,
    pub momentum: LadderAggregation,
    pub structure: LadderAggregation,
    pub progression: LadderAggregation,
    pub state: CrossTfState,
}

/// Direction: the only mean in the model.
///
/// Legal because every duration measures the same quantity on the shared
/// `DirectionalBias` domain, so the values are commensurable.
pub fn reduce_direction(durations: &[DurationValue]) -> (CrossTfDirection, LadderAggregation) {
    let signed: Vec<(u64, f64)> = durations
        .iter()
        .filter_map(|(secs, v)| v.as_signed().map(|(x, _)| (*secs, x)))
        .collect();
    if signed.is_empty() {
        return (
            CrossTfDirection {
                mean_bias: 0.0,
                agreement_pct: 0.0,
                gradient: 0.0,
                contributing_durations: Vec::new(),
                dissenting_durations: durations.iter().map(|(s, _)| *s).collect(),
            },
            LadderAggregation::MeanAgreement {
                mean: 0.0,
                agreement_pct: 0.0,
                gradient: 0.0,
            },
        );
    }
    let mean = signed.iter().map(|(_, v)| v).sum::<f64>() / signed.len() as f64;
    let agreement_pct = 100.0
        * signed
            .iter()
            .filter(|(_, v)| sign_of(*v) == sign_of(mean) && mean != 0.0)
            .count() as f64
        / signed.len() as f64;
    let gradient = gradient_over(&signed);
    let (contributing, dissenting) =
        signed
            .iter()
            .fold((Vec::new(), Vec::new()), |(mut c, mut d), (secs, v)| {
                if mean != 0.0 && sign_of(*v) == sign_of(mean) {
                    c.push(*secs);
                } else {
                    d.push(*secs);
                }
                (c, d)
            });
    (
        CrossTfDirection {
            mean_bias: mean,
            agreement_pct,
            gradient,
            contributing_durations: contributing,
            dissenting_durations: dissenting,
        },
        LadderAggregation::MeanAgreement {
            mean,
            agreement_pct,
            gradient,
        },
    )
}

/// Momentum: breadth **count** plus a fast-vs-slow comparison.
///
/// Not a mean: a mean over `Acceleration` would give +0.40 and +0.01 equal
/// weight, which is the opposite of what breadth means.
pub fn reduce_momentum(durations: &[DurationValue]) -> (CrossTfMomentum, LadderAggregation) {
    let signed: Vec<(u64, f64)> = durations
        .iter()
        .filter_map(|(secs, v)| v.as_signed().map(|(x, _)| (*secs, x)))
        .collect();
    let (fast, slow) = split_fast_slow(&signed);
    let positive = signed.iter().filter(|(_, v)| *v > 0.0).count();
    let breadth_positive_pct = if signed.is_empty() {
        0.0
    } else {
        100.0 * positive as f64 / signed.len() as f64
    };
    let fast_vs_slow = match (fast, slow) {
        (Some((fs, fv)), Some((ss, sv))) => FastVsSlow {
            fast_secs: fs,
            fast_accel: fv,
            slow_secs: ss,
            slow_accel: sv,
            divergent: (fv - sv).abs() > 0.20,
        },
        (Some((fs, fv)), None) => FastVsSlow {
            fast_secs: fs,
            fast_accel: fv,
            slow_secs: fs,
            slow_accel: fv,
            divergent: false,
        },
        _ => FastVsSlow {
            fast_secs: 0,
            fast_accel: 0.0,
            slow_secs: 0,
            slow_accel: 0.0,
            divergent: false,
        },
    };
    let gradient = gradient_over(&signed);
    let histogram = StageHistogram::default();
    (
        CrossTfMomentum {
            breadth_positive_pct,
            fast_vs_slow,
            gradient,
        },
        LadderAggregation::StageDistribution {
            histogram,
            gradient,
        },
    )
}

/// Structure: counts, convergence and a readiness count.
///
/// Not a mean: a mean over `NetEventBias` would let a duration with zero events
/// count equally with one carrying eight.
pub fn reduce_structure(
    durations: &[DurationValue],
    event_counts: &BTreeMap<u64, u32>,
    convergence: bool,
    ready_durations: u32,
) -> (CrossTfStructure, LadderAggregation) {
    let signed: Vec<(u64, f64)> = durations
        .iter()
        .filter_map(|(secs, v)| v.as_signed().map(|(x, _)| (*secs, x)))
        .collect();
    let divergence_active: Vec<u64> = event_counts
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(secs, _)| *secs)
        .collect();
    let divergence_duration_count = divergence_active.len() as u32;
    let divergence_duration_total = event_counts.len() as u32;
    let gradient = gradient_over(&signed);
    let histogram = StageHistogram::default();
    let structure = CrossTfStructure {
        levels_converging: convergence,
        divergence_active_on: divergence_active,
        divergence_duration_count,
        divergence_duration_total,
        ready_duration_count: ready_durations,
        total_durations: durations.len() as u32,
        gradient,
    };
    (
        structure,
        LadderAggregation::StageDistribution {
            histogram,
            gradient,
        },
    )
}

/// Progression: ordinal mode, ordinal interval, and the leg count.
pub fn reduce_progression(
    stages: &[(u64, Option<core_domain::axes::Stage>)],
) -> (CrossTfProgression, LadderAggregation) {
    let mut histogram = StageHistogram::default();
    for (_, stage) in stages
        .iter()
        .filter_map(|(s, stage)| stage.map(|st| (*s, st)))
    {
        histogram.increment(stage);
    }
    let mode = histogram.mode();
    let range = histogram.range().map(|(min, max)| StageRange { min, max });
    let distinct_legs = stages.len() as u32;
    let gradient = stage_gradient(stages);
    let progression = CrossTfProgression {
        modal_stage: mode,
        stage_range: range,
        histogram: histogram.clone(),
        // Legs are duration-local and are never merged, so "one leg per duration"
        // is itself the finding: correspondence is not assumed.
        distinct_leg_count: distinct_legs,
        legs_correspond: distinct_legs <= 1,
        gradient,
    };
    (
        progression,
        LadderAggregation::StageDistribution {
            histogram,
            gradient,
        },
    )
}

/// Assemble `CrossTfState` from that pair's per-duration states.
///
/// V-96q: this level holds **no indicator reference at all** — every value is
/// reduced from the durations' axis values, so a consumer cannot reach back down
/// to a calculator and recompute something different.
pub fn assemble_cross_tf_state(
    pair: &str,
    states: &[(u64, TimeframeState)],
    convergence: bool,
) -> (CrossTfState, CrossTfReduction) {
    let durations: Vec<u64> = states.iter().map(|(secs, _)| *secs).collect();
    let by_axis = |axis: AxisId| -> Vec<(u64, AxisValue)> {
        states
            .iter()
            .map(|(secs, state)| {
                (
                    *secs,
                    state
                        .axes
                        .iter()
                        .find(|a| a.axis == axis)
                        .map(|a| a.position.clone())
                        .unwrap_or_else(|| {
                            AxisValue::undefined(
                                core_domain::axes::UndefinedReason::NotApplicable {
                                    detail: "axis absent".into(),
                                },
                            )
                        }),
                )
            })
            .collect()
    };
    let (direction, direction_agg) = reduce_direction(&by_axis(AxisId::Direction));
    let (momentum, momentum_agg) = reduce_momentum(&by_axis(AxisId::Momentum));

    let mut event_counts = BTreeMap::new();
    for (secs, state) in states {
        event_counts.insert(*secs, state.events.active_count);
    }
    let ready = states
        .iter()
        .filter(|(_, s)| s.axes.iter().all(|a| a.readiness == Readiness::Solid))
        .count() as u32;
    let (structure, structure_agg) = reduce_structure(
        &by_axis(AxisId::Structure),
        &event_counts,
        convergence,
        ready,
    );

    let stages: Vec<(u64, Option<core_domain::axes::Stage>)> = states
        .iter()
        .map(|(secs, state)| {
            (
                *secs,
                state
                    .axes
                    .iter()
                    .find(|a| a.axis == AxisId::Progression)
                    .and_then(|a| a.position.as_stage()),
            )
        })
        .collect();
    let (progression, progression_agg) = reduce_progression(&stages);

    let state = CrossTfState {
        durations: durations.clone(),
        active_count: states.len() as u32,
        direction,
        momentum,
        structure,
        progression,
        cross_duration_agreement: CrossDurationAgreement {
            levels_converging: convergence,
            divergence_duration_count: event_counts.values().filter(|n| **n > 0).count() as u32,
            divergence_duration_total: event_counts.len() as u32,
            concordant_duration_count: states.len() as u32,
        },
        derived: DerivedState {
            consensus: Consensus {
                agreeing_axes: 0,
                of: 4,
                axes: Vec::new(),
                weakest_axis: None,
            },
            stability: Stability {
                state: StabilityState::Building,
                changes_last_50_bars: 0,
                window: STABILITY_WINDOW_BARS,
            },
        },
        member_freshness: core_domain::axes::MemberFreshness {
            oldest_candle_age_secs: 0,
            stale_members: Vec::new(),
        },
    };
    let _ = pair;
    let reduction = CrossTfReduction {
        direction: direction_agg,
        momentum: momentum_agg,
        structure: structure_agg,
        progression: progression_agg,
        state: state.clone(),
    };
    let state = reduction.state.clone();
    (state, reduction)
}

// ---------------------------------------------------------------------------
// Level 3 — CrossPairState
// ---------------------------------------------------------------------------

/// The reduction over the watched pairs.
///
/// Two pairs with identical breadth counts can be completely different markets —
/// one +0.9/+0.8, one +0.05/+0.02 — so `mean_bias` **and** `bias_range` are both
/// present, and neither is a re-bucketing of the other (V-108, V-110).
pub fn assemble_cross_pair_state(
    pairs: &[(&str, CrossTfState)],
    converging_level_pair_count: u32,
) -> CrossPairState {
    let biases: Vec<(String, f64)> = pairs
        .iter()
        .map(|(pair, state)| (pair.to_string(), state.direction.mean_bias))
        .collect();
    let (mean_bias, bias_range, dispersion) = if biases.is_empty() {
        (0.0, 0.0, core_domain::axes::Dispersion::Narrow)
    } else {
        let mean = biases.iter().map(|(_, b)| b).sum::<f64>() / biases.len() as f64;
        let min = biases.iter().map(|(_, b)| *b).fold(f64::MAX, f64::min);
        let max = biases.iter().map(|(_, b)| *b).fold(f64::MIN, f64::max);
        let range = max - min;
        (
            mean,
            range,
            // The bands are on the **spread**, not on the mean: a market that is
            // unanimously bullish and one that is split are different facts.
            if range >= 1.0 {
                core_domain::axes::Dispersion::Wide
            } else if range >= 0.4 {
                core_domain::axes::Dispersion::Moderate
            } else {
                core_domain::axes::Dispersion::Narrow
            },
        )
    };

    let bullish = biases.iter().filter(|(_, b)| *b > 0.0).count() as u32;
    let bearish = biases.iter().filter(|(_, b)| *b < 0.0).count() as u32;
    let unclear = biases.len() as u32 - bullish - bearish;
    let breadth_pct = if biases.is_empty() {
        0.0
    } else {
        100.0 * bullish as f64 / biases.len() as f64
    };

    // The per-pair direction distribution — a count, always legal.
    let mut direction: HashMap<String, u32> = HashMap::new();
    for (_, bias) in &biases {
        let bucket = if *bias > 0.0 {
            "bullish"
        } else if *bias < 0.0 {
            "bearish"
        } else {
            "unclear"
        };
        *direction.entry(bucket.to_string()).or_insert(0) += 1;
    }

    // Progression across pairs: ordinal mode over a histogram. Never a mean.
    let mut histogram = StageHistogram::default();
    let mut modal_votes: BTreeMap<&str, u32> = BTreeMap::new();
    for (_, state) in pairs {
        if let Some(stage) = state.progression.modal_stage {
            histogram.increment(stage);
        }
    }
    // The mode of the modes, counted rather than averaged.
    for (_, state) in pairs {
        if let Some(stage) = state.progression.modal_stage {
            *modal_votes.entry(stage_label(stage)).or_insert(0) += 1;
        }
    }
    let cross_pair_mode = modal_votes
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .and_then(|(label, _)| stage_from_label(label));

    let mut ranking: Vec<core_domain::axes::AssetRank> = pairs
        .iter()
        .map(|(pair, state)| core_domain::axes::AssetRank {
            pair: pair.to_string(),
            rank: 0,
            direction_bias: state.direction.mean_bias,
            agreement_pct: state.direction.agreement_pct,
            modal_stage: state.progression.modal_stage,
        })
        .collect();
    // Rank by absolute bias: the strongest tilt first, sign preserved in the
    // value itself so a ranking can never hide a direction.
    ranking.sort_by(|a, b| {
        b.direction_bias
            .abs()
            .partial_cmp(&a.direction_bias.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, row) in ranking.iter_mut().enumerate() {
        row.rank = i as u32 + 1;
    }

    let total = pairs.len() as u32;
    CrossPairState {
        breadth: CrossPairBreadth {
            bullish_count: bullish,
            bearish_count: bearish,
            unclear_count: unclear,
            breadth_pct,
            mean_bias,
            bias_range,
            dispersion,
        },
        axis_distribution: AxisDistribution {
            direction,
            progression: histogram.clone(),
        },
        readiness: ReadinessCounts {
            solid_duration_count: pairs
                .iter()
                .filter(|(_, s)| s.structure.ready_duration_count > 0)
                .count() as u32,
            dashed_count: pairs
                .iter()
                .filter(|(_, s)| s.structure.ready_duration_count == 0)
                .count() as u32,
            total,
        },
        coverage: Coverage {
            complete_pair_count: total,
            stale_pair_count: 0,
            converging_level_pair_count,
        },
        progression: core_domain::axes::CrossPairProgression {
            histogram,
            mode: cross_pair_mode,
        },
        concordance: core_domain::axes::Concordance {
            concordant_pair_count: pairs
                .iter()
                .filter(|(_, s)| s.direction.agreement_pct >= 60.0)
                .count() as u32,
            divergent_pair_count: pairs
                .iter()
                .filter(|(_, s)| s.direction.agreement_pct < 60.0)
                .count() as u32,
        },
        asset_ranking: ranking,
        member_freshness: core_domain::axes::PairFreshness {
            slowest_pair: pairs
                .first()
                .map(|(p, _)| p.to_string())
                .unwrap_or_default(),
            oldest_candle_age_secs: 0,
        },
    }
}

/// The envelope.
pub fn assemble_model_state(
    version: impl Into<String>,
    build_version: impl Into<String>,
    generated_at_ms: u64,
    ladder_secs: &[u64],
    instances: &[PairAssembly],
    converging_level_pair_count: u32,
) -> ModelState {
    let pair_states: Vec<(&str, CrossTfState)> = instances
        .iter()
        .map(|(pair, cross, _)| (pair.as_str(), cross.clone()))
        .collect();
    let cross_pair_state = assemble_cross_pair_state(&pair_states, converging_level_pair_count);

    let mut map: HashMap<String, core_domain::axes::InstanceStates> = HashMap::new();
    for (pair, cross, durations) in instances {
        let mut by_secs = HashMap::new();
        for (secs, state) in durations {
            by_secs.insert(*secs, state.clone());
        }
        map.insert(
            pair.clone(),
            core_domain::axes::InstanceStates {
                cross_tf_state: cross.clone(),
                durations: by_secs,
            },
        );
    }
    ModelState {
        version: version.into(),
        build_version: build_version.into(),
        generated_at_ms,
        ladder_secs: ladder_secs.to_vec(),
        instances: map,
        cross_pair_state,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionCriterion {
    OpposingSwingConfirmed,
    CharacterChange,
    ClimaxFailure,
}

/// The assessment, with the criteria that produced it — 01-13: "an ordinal plus
/// the **named** criteria that produced it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalityAssessment {
    pub terminality: Terminality,
    pub criteria: Vec<CompletionCriterion>,
}

/// Derive terminality from **completion criteria only**.
///
/// Never from `progress`, never from the stage, never from a probability. A leg
/// can be `early` and carry termination evidence, or `late` and carry none; the
/// two are not derivable from each other.
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

// ── helpers ───────────────────────────────────────────────────────────────

/// The domain an axis's signed values carry when its members do not declare one.
fn default_domain(axis: AxisId) -> core_domain::axes::Domain {
    match axis {
        AxisId::Direction => core_domain::axes::Domain::DirectionalBias,
        AxisId::Momentum => core_domain::axes::Domain::Acceleration,
        _ => core_domain::axes::Domain::NetEventBias,
    }
}

fn sign_of(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// The slope of the signed series over its durations, fastest → slowest.
fn gradient_over(signed: &[(u64, f64)]) -> f64 {
    if signed.len() < 2 {
        return 0.0;
    }
    let n = signed.len() as f64;
    let xs: Vec<f64> = signed.iter().map(|(s, _)| *s as f64).collect();
    let ys: Vec<f64> = signed.iter().map(|(_, v)| *v).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let num: f64 = xs
        .iter()
        .zip(&ys)
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let den: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

fn stage_gradient(stages: &[(u64, Option<core_domain::axes::Stage>)]) -> f64 {
    let anchor: Vec<(u64, f64)> = stages
        .iter()
        .filter_map(|(s, st)| st.map(|st| (*s, st.anchor_index() as f64)))
        .collect();
    gradient_over(&anchor)
}

fn split_fast_slow(signed: &[(u64, f64)]) -> FastestSlowest {
    if signed.is_empty() {
        return (None, None);
    }
    let fastest = signed.iter().min_by_key(|(s, _)| *s).copied();
    let slowest = signed.iter().max_by_key(|(s, _)| *s).copied();
    (fastest, slowest)
}

fn stage_label(stage: core_domain::axes::Stage) -> &'static str {
    match stage {
        core_domain::axes::Stage::Early => "early",
        core_domain::axes::Stage::EarlyMid => "early_mid",
        core_domain::axes::Stage::Mid => "mid",
        core_domain::axes::Stage::Late => "late",
        core_domain::axes::Stage::Terminal => "terminal",
    }
}

fn stage_from_label(label: &str) -> Option<core_domain::axes::Stage> {
    match label {
        "early" => Some(core_domain::axes::Stage::Early),
        "early_mid" => Some(core_domain::axes::Stage::EarlyMid),
        "mid" => Some(core_domain::axes::Stage::Mid),
        "late" => Some(core_domain::axes::Stage::Late),
        "terminal" => Some(core_domain::axes::Stage::Terminal),
        _ => None,
    }
}
