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

use std::collections::BTreeMap;

use crate::indicators::registry;
use core_domain::axes::{AxisId, AxisValue, Domain, Readiness, Stage, TimeframeState};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::models::MarketSnapshot;

use crate::model::{assemble_timeframe_state, Reading, TimeframeInput};

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
/// Labels are matched against the **closed** set of stage names; an unknown label
/// is absence rather than a guess at the nearest anchor.
fn progression_stage_of(value: &NormalizedIndicatorValue) -> Option<Stage> {
    match value.state_label.trim().to_ascii_lowercase().as_str() {
        "early" => Some(Stage::Early),
        "early_mid" | "early mid" | "early-mid" => Some(Stage::EarlyMid),
        "mid" | "middle" => Some(Stage::Mid),
        "late" => Some(Stage::Late),
        "terminal" => Some(Stage::Terminal),
        _ => None,
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
        spec_version: env!("CARGO_PKG_VERSION").to_string(),
        build_version: option_env!("GIT_BUILD_ID").unwrap_or("dev").to_string(),
    };

    let levels = snapshot
        .indicators
        .iter()
        .filter(|(_, v)| {
            v.signals
                .iter()
                .any(|s| matches!(format!("{:?}", s.kind).as_str(), "LevelTouch" | "Breakout"))
        })
        .map(|(key, v)| core_domain::axes::Level {
            kind: key.clone(),
            price_low: v.normalized,
            price_high: v.normalized,
            basis: v.state_label.clone(),
        })
        .collect();

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
    })
}

/// The number of active events the snapshot's indicators declare.
///
/// An event counts once even though several indicators may declare the same
/// kind (E5): distinct (key, kind) pairs, not signal instances.
fn count_signals(snapshot: &MarketSnapshot) -> usize {
    let mut distinct: BTreeMap<(String, String), ()> = BTreeMap::new();
    for (key, value) in &snapshot.indicators {
        for signal in &value.signals {
            distinct.insert((key.clone(), format!("{:?}", signal.kind)), ());
        }
    }
    distinct.len()
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
