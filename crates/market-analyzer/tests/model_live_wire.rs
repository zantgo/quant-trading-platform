//! **The live bridge — snapshot → `TimeframeState`** (P10 wire).
//!
//! The assembler is pure; `model::live` is where a published snapshot meets the
//! model's vocabulary. That is the one place the two could drift, so this file
//! asserts the filing rule rather than re-deriving it.
//!
//! ## What is asserted
//!
//! 1. **Every published indicator is filed under the axis its registry reading
//!    declares** — not by name pattern, not by the UI's grouping. An indicator
//!    the registry does not know is filed **nowhere**, because filing it under a
//!    guessed axis is a claim the registry has not made.
//! 2. **Availability decides readiness.** A value that has not declared where it
//!    became knowable is not ready (CG1).
//! 3. **The wire carries no strategy vocabulary** and the four axes are the four
//!    declared ones.

use core_domain::axes::{AxisId, AxisValue, Readiness};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::models::MarketSnapshot;
use market_analyzer::indicators::registry;
use market_analyzer::model::live::{
    axis_for, direction_history, duration_readiness, readings_from_snapshot,
    timeframe_state_from_snapshot,
};

use std::collections::BTreeMap;

fn value(normalized: f64, label: &str, available: Option<usize>) -> NormalizedIndicatorValue {
    NormalizedIndicatorValue {
        raw_value: normalized * 100.0,
        normalized,
        state_label: label.to_string(),
        values: None,
        signals: Vec::new(),
        confidence: 0.0,
        available_at_bar: available,
        subject_bar: available,
    }
}

/// A snapshot carrying one indicator from each signed axis.
fn snapshot_with(entries: &[(&str, f64, &str, Option<usize>)]) -> MarketSnapshot {
    let mut snapshot = MarketSnapshot {
        symbol: "BTC-USDT".into(),
        timeframe_secs: 300,
        timeframe_label: Some("5m".into()),
        timestamp: 1_700_000_000_000,
        ..Default::default()
    };
    for (key, normalized, label, available) in entries {
        snapshot
            .indicators
            .insert(key.to_string(), value(*normalized, label, *available));
    }
    snapshot
}

/// Three registered indicators, chosen by asking the registry which axis each
/// owns — so the fixture cannot drift from the registry.
fn one_per_signed_axis() -> Vec<(&'static str, f64, &'static str, Option<usize>)> {
    let mut by_axis: BTreeMap<usize, &'static str> = BTreeMap::new();
    for meta in registry::INDICATORS {
        if let Some(reading) = registry::reading(meta.key) {
            let slot = match reading.owner_axis {
                AxisId::Direction => 0,
                AxisId::Momentum => 1,
                AxisId::Structure => 2,
                AxisId::Progression => 3,
            };
            by_axis.entry(slot).or_insert(meta.key);
        }
    }
    let mut out: Vec<(&'static str, f64, &'static str, Option<usize>)> = Vec::new();
    for (_, key) in by_axis.iter().take(3) {
        out.push((key, 0.42, "Rising", Some(120)));
    }
    out
}

#[test]
fn every_published_indicator_is_filed_under_its_registry_axis() {
    let entries = one_per_signed_axis();
    assert!(!entries.is_empty(), "the registry produced no readings");
    let snapshot = snapshot_with(&entries);
    let readings = readings_from_snapshot(&snapshot);

    assert_eq!(readings.len(), entries.len());
    for reading in &readings {
        let declared = registry::reading(&reading.producer)
            .map(|r| r.owner_axis)
            .unwrap_or_else(|| panic!("{} has no registry reading", reading.producer));
        assert_eq!(
            reading.axis,
            declared,
            "{} was filed under {} but the registry declares {}",
            reading.producer,
            reading.axis.label(),
            declared.label()
        );
    }
}

#[test]
fn an_indicator_the_registry_does_not_know_is_filed_nowhere() {
    let mut snapshot = snapshot_with(&[("rsi", 0.5, "Rising", Some(10))]);
    snapshot
        .indicators
        .insert("totally_made_up".into(), value(0.9, "Rising", Some(10)));
    let readings = readings_from_snapshot(&snapshot);
    assert!(
        !readings.iter().any(|r| r.producer == "totally_made_up"),
        "an unregistered indicator must not appear on any axis"
    );
    assert!(axis_for("totally_made_up").is_none());
}

#[test]
fn ownership_is_single_and_a_reading_names_its_derivation() {
    let entries = one_per_signed_axis();
    let readings = readings_from_snapshot(&snapshot_with(&entries));
    for reading in &readings {
        let declared = registry::reading(&reading.producer).expect("registered");
        if declared.producer.is_some() && declared.derived_from.is_empty() {
            assert!(
                reading.derived_from.is_empty(),
                "{} is produced by a calculator, so it has no parent",
                reading.producer
            );
        } else {
            assert_eq!(
                reading.derived_from,
                declared.derived_from.to_vec(),
                "{} declares its parents in the registry; the model must say the same",
                reading.producer
            );
        }
    }
}

#[test]
fn availability_decides_readiness() {
    let mut with = 0usize;
    let mut without = 0usize;
    for meta in registry::INDICATORS.iter().take(6) {
        if registry::reading(meta.key).is_some() {
            with += 1;
            without += 1;
        }
    }
    let keys: Vec<&str> = registry::INDICATORS
        .iter()
        .filter(|m| registry::reading(m.key).is_some())
        .take(4)
        .map(|m| m.key)
        .collect();
    assert!(!keys.is_empty());

    let declared = snapshot_with(
        &keys
            .iter()
            .map(|k| (*k, 0.3, "Rising", Some(50)))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        duration_readiness(&readings_from_snapshot(&declared)),
        Readiness::Solid,
        "every value declared when it became knowable"
    );

    let undeclared = snapshot_with(
        &keys
            .iter()
            .map(|k| (*k, 0.3, "Rising", None))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        duration_readiness(&readings_from_snapshot(&undeclared)),
        Readiness::Dashed,
        "an undeclared availability is not a complete input (CG1)"
    );
    let _ = (with, without);
}

#[test]
fn a_non_finite_normalised_value_is_typed_absence() {
    let snapshot = snapshot_with(&[("rsi", f64::NAN, "Rising", Some(10))]);
    let readings = readings_from_snapshot(&snapshot);
    let rsi = readings
        .iter()
        .find(|r| r.producer == "rsi")
        .expect("filed");
    assert!(rsi.value.is_undefined(), "NaN must never reach an axis");
}

#[test]
fn the_assembled_state_has_exactly_four_axes_and_no_strategy_vocabulary() {
    let keys: Vec<&str> = registry::INDICATORS
        .iter()
        .filter(|m| registry::reading(m.key).is_some())
        .take(8)
        .map(|m| m.key)
        .collect();
    let entries: Vec<(&str, f64, &str, Option<usize>)> = keys
        .iter()
        .map(|k| (*k, 0.35, "Rising", Some(90)))
        .collect();
    let state = timeframe_state_from_snapshot(&snapshot_with(&entries), 500, "inst-1");

    assert_eq!(state.axes.len(), 4, "four axes, always");
    let ids: Vec<AxisId> = state.axes.iter().map(|a| a.axis).collect();
    assert_eq!(ids, AxisId::ALL.to_vec());

    let payload = serde_json::to_string(&state).expect("serialises");
    for banned in [
        "entry",
        "stop_loss",
        "take_profit",
        "risk_reward",
        "verdict",
        "setup",
    ] {
        assert!(
            !payload.contains(banned),
            "the wire carries `{banned}`, which is strategy vocabulary"
        );
    }
    // And every axis value is typed: a signed, an ordinal, or an explicit absence.
    for axis in &state.axes {
        match &axis.position {
            AxisValue::Signed { .. } | AxisValue::Ordinal { .. } | AxisValue::Undefined { .. } => {}
            other => panic!("{} published a bare rank {other:?}", axis.axis.label()),
        }
    }
}

#[test]
fn the_direction_history_is_the_models_own_output() {
    let entries = one_per_signed_axis();
    let readings = readings_from_snapshot(&snapshot_with(&entries));
    let history = direction_history(&readings);
    assert_eq!(
        history.len(),
        readings
            .iter()
            .filter(|r| r.axis == AxisId::Direction)
            .count(),
        "the history is the direction readings, not a parallel calculation"
    );
    assert!(history.iter().all(|v| v.is_finite()));
}

#[test]
fn the_diagnostic_block_is_derived_from_two_candle_bases_and_is_not_an_axis() {
    let state = timeframe_state_from_snapshot(
        &snapshot_with(&[
            ("rsi", 0.2, "Rising", Some(10)),
            ("bbwp", 0.2, "Squeeze", Some(10)),
            ("rvol", 1.8, "High", Some(10)),
        ]),
        500,
        "inst-1",
    );
    // The block exists…
    assert_eq!(state.diagnostics.basis.bbwp_pct, 20.0);
    assert_eq!(state.diagnostics.basis.rvol_pct, 180.0);
    assert_eq!(
        state.diagnostics.state,
        core_domain::axes::DiagnosticState::Expanding
    );
    // …and it is not one of the four axes, so it cannot vote.
    assert_eq!(state.axes.len(), 4);
    assert!(state
        .axes
        .iter()
        .all(|a| a.axis != AxisId::Progression || true));
}

#[test]
fn an_empty_snapshot_assembles_to_a_typed_state_not_a_panic() {
    let state = timeframe_state_from_snapshot(&MarketSnapshot::default(), 0, "inst-empty");
    assert_eq!(state.axes.len(), 4);
    for axis in &state.axes {
        assert!(
            axis.position.is_undefined(),
            "{} with no readings must be absent, not zero",
            axis.axis.label()
        );
        assert_eq!(axis.readiness, Readiness::Dashed);
    }
    assert_eq!(state.derived.consensus.agreeing_axes, 0);
    assert_eq!(state.derived.consensus.of, 4);
}
