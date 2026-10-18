//! Live assembly — from a published [`MarketSnapshot`] to a `TimeframeState`.
//!
//! The assembler in [`super`] is pure and knows nothing about the wire. This
//! module is the one place that reads a `MarketSnapshot`, because that is where
//! the pipeline's output and the model's vocabulary meet, and **one** meeting
//! point is what keeps the two from drifting.
//!
//! ## The filing rule
//!
//! Every published indicator is filed under the axis its **registry reading**
//! declares (`Reading::owner_axis`). Nothing is filed by convention, by name
//! pattern, or by the UI's grouping — so a new indicator cannot appear on an
//! axis nobody declared, and a retired one leaves the axis it used to feed.
//!
//! ## Availability
//!
//! A value's `available_at_bar` is carried onto the [`Reading`] and decides its
//! readiness: a value that has not declared where it became knowable is **not**
//! ready, because an undeclared availability is indistinguishable from an
//! unbounded one (CG1).

use std::collections::BTreeSet;

use crate::indicators::registry;
use core_domain::axes::{AxisId, AxisValue, Domain, Readiness, Stage, TimeframeState};
use core_domain::indicator_dtos::{NormalizedIndicatorValue, SignalKind};
use core_domain::models::MarketSnapshot;

use crate::model::{assemble_timeframe_state, Reading, SignalsByAxis, TimeframeInput};

/// Which domain a signed value on `axis` carries when the registry does not
/// declare one.
///
/// Direction is `DirectionalBias`, Momentum is `Acceleration`, Structure is
/// `NetEventBias` (01-13's domain table). Progression is ordinal and never uses
/// a domain.
pub fn domain_for(axis: AxisId) -> Domain {
    match axis {
        AxisId::Direction => Domain::DirectionalBias,
        AxisId::Momentum => Domain::Acceleration,
        AxisId::Structure => Domain::NetEventBias,
        // An ordinal axis has no domain — it is not signed.
        AxisId::Progression => Domain::NetEventBias,
    }
}

/// The reading id for a published indicator key.
///
/// The registry keys are the indicator keys (I28: stable, never renumbered), so
/// the mapping is exact rather than a lookup by display name.
fn reading_for(key: &str) -> Option<&'static registry::Reading> {
    registry::reading(key)
}

/// The axis a published indicator belongs to.
///
/// `None` when the registry has no reading for it — which is itself a finding
/// worth surfacing rather than silently filing it somewhere plausible.
pub fn axis_for(key: &str) -> Option<AxisId> {
    reading_for(key).map(|r| r.owner_axis)
}

/// Build the model's [`Reading`]s from one snapshot.
///
/// Ownership is single (`owner_axis`) and emission is separate from consumption:
/// the returned readings are what this duration publishes, one per registered
/// indicator, in a stable order.
pub fn readings_from_snapshot(snapshot: &MarketSnapshot) -> Vec<Reading> {
    let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
    keys.sort();

    let mut out = Vec::new();
    for key in keys {
        let value = &snapshot.indicators[key];
        let Some(declared) = reading_for(key) else {
            // An indicator the registry does not know about is not filed. It is
            // not filed *anywhere*, because filing it under a guessed axis would
            // be a claim the registry has not made.
            continue;
        };
        let axis = declared.owner_axis;
        // Whether every input behind this value is present. A missing
        // availability declaration is exactly "we cannot prove this was knowable
        // when it was published", so it reads as incomplete.
        let inputs_complete = value.available_at_bar.is_some();
        let axis_value = axis_value_for(axis, value);
        if declared.derived_from.is_empty() && declared.producer.is_some() {
            out.push(Reading::produced(key, axis, axis_value, inputs_complete));
        } else {
            let from: Vec<&str> = declared.derived_from.to_vec();
            out.push(Reading::derived(
                key,
                axis,
                axis_value,
                &from,
                inputs_complete,
            ));
        }
    }
    out
}

/// The axis value for one published indicator.
///
/// A `normalized` score in `[-1, 1]` becomes a signed value on the axis's
/// domain. The raw value is **not** filed: it is in different units per
/// indicator and averaging it across the axis would be meaningless.
fn axis_value_for(axis: AxisId, value: &NormalizedIndicatorValue) -> AxisValue {
    match axis {
        AxisId::Progression => match progression_stage_of(value) {
            Some(stage) => AxisValue::ordinal(stage),
            None => AxisValue::undefined(core_domain::axes::UndefinedReason::ReferenceMoving {
                reference: format!("{}: stage withheld", value.state_label),
            }),
        },
        _ => {
            if value.normalized.is_finite() {
                AxisValue::signed(value.normalized, domain_for(axis))
            } else {
                // A non-finite normalised score is typed absence, never a number.
                AxisValue::undefined(core_domain::axes::UndefinedReason::DegenerateReference {
                    detail: format!("{}: non-finite normalised value", value.state_label),
                })
            }
        }
    }
}

/// The ordinal stage, when the published label names one of the five anchors.
///
/// Delegates to [`Stage::from_label`], which is the workspace's single
/// sanctioned label→ordinal door (V-108). An unknown label is typed absence,
/// never a guess at the nearest anchor.
fn progression_stage_of(value: &NormalizedIndicatorValue) -> Option<Stage> {
    Stage::from_label(&value.state_label)
}

/// The typed signals each axis can observe, grouped by the axis that filed them.
///
/// A signal is filed under the axis of the **reading that produced it** — the
/// same axis it already contributed its value to — so `readings[]` and the
/// axis `position` can never disagree about which indicator filed what.
///
/// Ordering is `(at_bar, indicator key)`, so the array is oldest-first and
/// deterministic for one set of candles (CH-8).
fn signals_by_axis(
    snapshot: &MarketSnapshot,
    readings: &[Reading],
    current_bar: usize,
) -> SignalsByAxis {
    let mut by_axis: SignalsByAxis = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    // `(source, kind, at_bar)` — the part of the canonical event identity that
    // is resolvable from a single snapshot. The full identity (01-15) also
    // names the subject and the timeframe; those are the axis slot and the
    // anchor, which the caller already separates.
    let mut filed: BTreeSet<(String, SignalKind, usize)> = BTreeSet::new();

    for reading in readings {
        let Some(value) = snapshot.indicators.get(&reading.producer) else {
            continue;
        };
        let slot = match reading.axis {
            AxisId::Direction => 0,
            AxisId::Momentum => 1,
            AxisId::Structure => 2,
            AxisId::Progression => 3,
        };

        for sig in &value.signals {
            // `age_bars` is how long ago it fired; `at_bar` is absolute. A
            // signal older than the retention window is not observable, so it is
            // not filed — the cap is a retention rule, not a silent drop.
            let age = sig.age_bars as usize;
            if age >= core_domain::axes::READINGS_CAP {
                continue;
            }
            let at_bar = current_bar.saturating_sub(age);
            // Event identity is `(source, kind, subject, anchor, timeframe)`
            // (01-15) and is counted **exactly once**. Two indicators firing
            // the same kind on the same bar are two facts; the SAME indicator
            // reporting the SAME kind on the SAME bar twice is one fact
            // reported twice, and filing it twice would let one divergence
            // inflate the axis history (C/Q4).
            if !filed.insert((reading.producer.clone(), sig.kind, at_bar)) {
                continue;
            }
            by_axis[slot].push(core_domain::axes::Signal::Event {
                kind: core_domain::axes::model_signal_kind(sig.kind),
                at_bar,
                anchor: value.raw_value,
                // Typed, not a string: the pipeline kind is preserved verbatim
                // alongside the structural kind it reduced to (V-108).
                subtype: Some(sig.kind),
            });
        }
    }

    for slot in by_axis.iter_mut() {
        // Stable within a bar, so two indicators firing on the same bar have a
        // defined relative order rather than whatever the HashMap yielded.
        slot.sort_by(|a, b| {
            let ka = event_bar(a);
            let kb = event_bar(b);
            ka.cmp(&kb)
                .then_with(|| format!("{a:?}").cmp(&format!("{b:?}")))
        });
    }
    by_axis
}

fn event_bar(signal: &core_domain::axes::Signal) -> usize {
    match signal {
        core_domain::axes::Signal::Event { at_bar, .. } => *at_bar,
        _ => usize::MAX,
    }
}

/// Assemble a duration's `TimeframeState` from its live snapshot.
pub fn timeframe_state_from_snapshot(
    snapshot: &MarketSnapshot,
    candle_count: usize,
    instance_id: &str,
) -> TimeframeState {
    let readings = readings_from_snapshot(snapshot);
    let active_events = count_signals(snapshot);
    let direction_values = direction_history(&readings);
    let progression_stage = readings
        .iter()
        .find(|r| r.axis == AxisId::Progression)
        .and_then(|r| r.value.as_stage());
    // Computed before `readings` is moved into the input below.
    let current_bar = (snapshot.timestamp as usize).saturating_sub(1);
    let axis_signals = signals_by_axis(snapshot, &readings, current_bar);

    let identity = core_domain::axes::TimeframeIdentity {
        instance_id: instance_id.to_string(),
        pair: snapshot.symbol.clone(),
        exchange: snapshot
            .exchange
            .map(|e| format!("{e:?}"))
            .unwrap_or_else(|| "unknown".into()),
        timeframe_secs: snapshot.timeframe_secs,
        timeframe_label: snapshot
            .timeframe_label
            .clone()
            .unwrap_or_else(|| core_domain::duration_label(snapshot.timeframe_secs)),
        candle_count,
        last_closed_at: snapshot.timestamp.to_string(),
        available_at: snapshot.timestamp.to_string(),
        // N4: resolved from the spec line and from git at COMPILE TIME by
        // `core-domain/build.rs`. This used to be
        // `env!("CARGO_PKG_VERSION")` + `option_env!("GIT_BUILD_ID")`, which
        // published "0.1.0" and "dev" respectively -- GIT_BUILD_ID was never
        // set by anything, so "dev" was the only reachable value and a stale
        // tree was indistinguishable from a fresh one (CH-8).
        spec_version: core_domain::SPEC_VERSION.to_string(),
        build_version: core_domain::BUILD_VERSION.to_string(),
    };

    let levels = levels_from(snapshot);

    assemble_timeframe_state(TimeframeInput {
        identity,
        readings,
        events: core_domain::event::EventCounts {
            active: active_events,
            retained: 0,
            emitted: 0,
            ..Default::default()
        },
        levels,
        diagnostics: diagnostics_from(snapshot),
        progression_stage,
        progression_progress: core_domain::leg::Progress::Undefined {
            reason: core_domain::axes::UndefinedReason::NotApplicable {
                detail: "progress is the leg's; the snapshot carries no leg".into(),
            },
        },
        target_version: 0,
        target_stable: false,
        available_at_bar: snapshot.timestamp as usize,
        direction_history: direction_values,
        axis_signals,
    })
}

/// The number of active events the snapshot's indicators declare.
///
/// An event counts once even though several indicators may declare the same
/// kind (E5): distinct `(indicator, kind)` pairs, not signal instances.
///
/// The key is the typed [`SignalKind`], not `format!("{:?}", …)`. That string
/// form was the original defect (V-106): it made the count depend on a
/// variant's *spelling*, so a rename silently changed which events existed.
#[must_use]
pub fn count_signals(snapshot: &MarketSnapshot) -> usize {
    let mut distinct: BTreeSet<(String, SignalKind)> = BTreeSet::new();
    for (key, value) in &snapshot.indicators {
        for signal in &value.signals {
            distinct.insert((key.clone(), signal.kind));
        }
    }
    distinct.len()
}

/// Signal kinds that mark a **zone**, not a reading.
///
/// Typed, therefore total. The previous filter spelled these as strings —
/// `matches!(format!("{:?}", s.kind).as_str(), "LevelTouch" | "Breakout")` —
/// and `LevelTouch` **is not a variant of `SignalKind`** (the real one is
/// `LevelTest`). The `matches!` therefore never fired, on any snapshot, ever,
/// and `model_state.levels` was empty on the live path while the gate suite
/// passed. That is the class of defect V-107 exists to catch.
pub const ZONE_SIGNALS: [SignalKind; 3] = [
    SignalKind::LevelTest,
    SignalKind::BandTouch,
    SignalKind::Breakout,
];

/// Extract the priced zones a snapshot's indicators declare.
///
/// A level is a **band**, so this reads an actual band: the `lower`/`upper`
/// pair an indicator writes into its value map. An indicator that fires a zone
/// signal but declares no band produces **no level** — typed absence (CH-7)
/// rather than a number that means nothing.
///
/// Both previous behaviours were wrong in the same way: the edges were set to
/// `normalized`, a `[-1, 1]` score, so every published level was degenerate
/// *and* was not a price.
fn levels_from(snapshot: &MarketSnapshot) -> Vec<core_domain::axes::Level> {
    let mut out = Vec::new();

    // Ordered by key so the level list is deterministic (CH-8): a `HashMap`
    // iteration order would publish a different array order for the same
    // market on the same candles, which is bit-level non-determinism.
    let mut keys: Vec<&String> = snapshot.indicators.keys().collect();
    keys.sort();

    for key in keys {
        let value = &snapshot.indicators[key];
        if !value.signals.iter().any(|s| ZONE_SIGNALS.contains(&s.kind)) {
            continue;
        }
        let Some(values) = value.values.as_ref() else {
            continue;
        };
        let (Some(&low), Some(&high)) = (values.get("lower"), values.get("upper")) else {
            // Fires a zone signal but declares no band. No level — and no
            // fabricated edges.
            continue;
        };

        let basis = core_domain::axes::LevelBasis::DeclaredBand {
            indicator: key.clone(),
        };
        // `Level::new` refuses a degenerate band, so the ordering is fixed here
        // rather than trusting the producer to have written low-then-high.
        let (low, high) = if low <= high {
            (low, high)
        } else {
            (high, low)
        };
        if let Some(level) = core_domain::axes::Level::new(key.clone(), low, high, basis) {
            out.push(level);
        }
    }
    out
}

/// The direction series the stability measure consumes.
///
/// Built from the **published** direction readings rather than a raw series, so
/// the history is the model's own output and not a parallel calculation.
pub fn direction_history(readings: &[Reading]) -> Vec<f64> {
    readings
        .iter()
        .filter(|r| r.axis == AxisId::Direction)
        .filter_map(|r| r.value.as_signed().map(|(v, _)| v))
        .collect()
}

/// The diagnostic block, which is **not an axis** and never votes.
///
/// Derived from two candle-derived bases and nothing else: the bandwidth
/// percentile and the relative volume. The two "change" figures are *computed*
/// here from those bases rather than carried as free numbers, so a diagnostic
/// cannot report a change nobody computed (CH-1).
fn diagnostics_from(snapshot: &MarketSnapshot) -> core_domain::axes::Diagnostics {
    use core_domain::axes::{DiagnosticBasis, DiagnosticState};

    let bbwp = snapshot.indicators.get("bbwp").map(|v| v.raw_value);
    let rvol = snapshot.indicators.get("rvol").map(|v| v.raw_value);

    let state = match (bbwp, rvol) {
        (Some(bbwp_pct), Some(rvol_pct)) => {
            if bbwp_pct < 25.0 && rvol_pct < 90.0 {
                DiagnosticState::Coiling
            } else if rvol_pct > 160.0 {
                DiagnosticState::Expanding
            } else if bbwp_pct > 45.0 {
                DiagnosticState::Settling
            } else {
                DiagnosticState::Chopping
            }
        }
        // Without both bases the state is the neutral label, and the basis
        // carries zeros so a consumer can see it had nothing to work from.
        _ => DiagnosticState::Chopping,
    };

    let bbwp_pct = bbwp.unwrap_or(0.0);
    let rvol_pct = rvol.unwrap_or(0.0);
    core_domain::axes::Diagnostics {
        state,
        volatility_change_pct: (bbwp_pct - 30.0) / 30.0 * 100.0,
        participation_change_pct: (rvol_pct - 100.0) / 100.0 * 100.0,
        basis: DiagnosticBasis { bbwp_pct, rvol_pct },
    }
}

/// The readiness of the whole duration: solid only when **every** published
/// reading is.
pub fn duration_readiness(readings: &[Reading]) -> Readiness {
    if !readings.is_empty() && readings.iter().all(|r| r.inputs_complete) {
        Readiness::Solid
    } else {
        Readiness::Dashed
    }
}
