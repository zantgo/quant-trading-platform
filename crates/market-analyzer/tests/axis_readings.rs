//! **V-201 … V-204 — `readings[]` is populated from the registry, not stubbed.**
//!
//! # The defect
//!
//! `assemble_axes` hardcoded `readings: Vec::new()`:
//!
//! ```ignore
//! AxisState { axis, position, size, readiness, readings: Vec::new() }
//! ```
//!
//! `AxisState::push_reading` existed, was documented, was capped at 50 — and had
//! **zero production callers**. So a published `TimeframeState` asserted that
//! each axis had *no observable typed signal history*, on every candle, forever,
//! while all thirteen causal gates passed. Every gate asserted on the
//! assembler's *inputs*; none asserted on what it *emitted*.
//!
//! **Mutation M8** restores the hardcoded empty and demonstrates that
//! `v201_the_published_axis_carries_its_signal_history` fires. See
//! `M8_READINGS_EMPTY.md`.

use core_domain::axes::{AxisId, Signal, READINGS_CAP};
use core_domain::indicator_dtos::{
    IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalStatus,
};
use core_domain::models::MarketSnapshot;
use std::collections::HashMap;

fn sig(kind: core_domain::indicator_dtos::SignalKind, age: u32) -> IndicatorSignal {
    let mut s = IndicatorSignal::new(kind, SignalDirection::Neutral, SignalStatus::Active, "t");
    s.age_bars = age;
    s
}

fn value_with(raw: f64, signals: Vec<IndicatorSignal>) -> NormalizedIndicatorValue {
    let mut v = NormalizedIndicatorValue::scalar(raw, 0.4, "");
    v.signals = signals;
    v
}

fn snapshot_with(indicators: Vec<(String, NormalizedIndicatorValue)>) -> MarketSnapshot {
    MarketSnapshot {
        symbol: "BTC-USDT".to_string(),
        timeframe_secs: 60,
        indicators: indicators.into_iter().collect::<HashMap<_, _>>(),
        ..Default::default()
    }
}

/// A registry reading key that the assembler files under `Direction`.
///
/// Read from the registry's own `READINGS` table rather than hardcoded, so this
/// test fails loudly if the registry is re-filed instead of quietly exercising
/// an indicator nothing claims (which is exactly how a `readings[]` test can pass
/// while the shipped document is empty).
fn a_filed_direction_indicator() -> String {
    use core_domain::axes::AxisId;
    let r = market_analyzer::indicators::registry::READINGS
        .iter()
        .find(|r| r.owner_axis == AxisId::Direction)
        .expect("the registry files at least one Direction reading");
    r.id.to_string()
}

fn snapshot_with_signals_on_one_indicator() -> MarketSnapshot {
    snapshot_with(vec![(
        a_filed_direction_indicator(),
        value_with(
            55.0,
            vec![
                sig(core_domain::indicator_dtos::SignalKind::Crossover, 0),
                sig(core_domain::indicator_dtos::SignalKind::Threshold, 3),
                sig(core_domain::indicator_dtos::SignalKind::Divergence, 7),
            ],
        ),
    )])
}

fn axis_of(state: &core_domain::axes::TimeframeState, id: AxisId) -> &core_domain::axes::AxisState {
    state
        .axes
        .iter()
        .find(|a| a.axis == id)
        .unwrap_or_else(|| panic!("axis {id:?} missing from the published state"))
}

// ─────────────────────────────────────────────────────────────────────────────
// V-201 — the published axis carries its history
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v201_the_published_axis_carries_its_signal_history() {
    let snap = snapshot_with_signals_on_one_indicator();
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    // The named regression. Before this, every axis published an empty history
    // and `push_reading` was dead code.
    let total: usize = state.axes.iter().map(|a| a.readings.len()).sum();
    assert!(
        total > 0,
        "no axis carried any signal history — `readings[]` is hardcoded empty again (V-201)"
    );

    // The three signals fired on the direction-filed indicator appear under the
    // axis that indicator was filed under.
    let direction = axis_of(&state, AxisId::Direction);
    assert_eq!(
        direction.readings.len(),
        3,
        "expected the three fired signals under Direction, got {:?}",
        direction.readings
    );
}

#[test]
fn v201_every_signal_is_a_typed_variant_never_a_bare_number() {
    let snap = snapshot_with_signals_on_one_indicator();
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    for axis in &state.axes {
        for reading in &axis.readings {
            // The type is closed by construction; this asserts that the *value*
            // is one of the three named variants and not a silently-defaulted
            // struct.
            assert!(
                matches!(
                    reading,
                    Signal::Level { .. } | Signal::Event { .. } | Signal::Transition { .. }
                ),
                "readings[] holds a value outside the closed vocabulary: {reading:?} (V-94q)"
            );
        }
    }
}

#[test]
fn v201_the_pipeline_kind_is_preserved_typed_not_as_a_word() {
    let snap = snapshot_with_signals_on_one_indicator();
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    let subtypes: Vec<Option<core_domain::indicator_dtos::SignalKind>> =
        axis_of(&state, AxisId::Direction)
            .readings
            .iter()
            .map(|r| match r {
                Signal::Event { subtype, .. } => *subtype,
                _ => None,
            })
            .collect();

    assert!(
        subtypes.contains(&Some(core_domain::indicator_dtos::SignalKind::Crossover)),
        "the pipeline kind must survive onto the model signal as a typed value: {subtypes:?}"
    );
    assert!(
        subtypes.contains(&Some(core_domain::indicator_dtos::SignalKind::Divergence)),
        "divergence must survive: {subtypes:?}"
    );
}

#[test]
fn v201_the_structural_kind_is_the_reduced_vocabulary() {
    let snap = snapshot_with_signals_on_one_indicator();
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    let kinds: Vec<core_domain::axes::SignalKind> = axis_of(&state, AxisId::Direction)
        .readings
        .iter()
        .map(|r| match r {
            Signal::Event { kind, .. } => *kind,
            _ => panic!("expected an Event"),
        })
        .collect();

    assert!(kinds.contains(&core_domain::axes::SignalKind::CenterlineCross));
    assert!(kinds.contains(&core_domain::axes::SignalKind::Threshold));
    assert!(kinds.contains(&core_domain::axes::SignalKind::Divergence));
}

#[test]
fn v201_the_reduction_from_pipeline_to_model_vocabulary_is_total() {
    use core_domain::axes::{model_signal_kind, SignalKind};
    use core_domain::indicator_dtos::SignalKind as K;

    // Every pipeline kind decides. A new variant fails to compile here rather
    // than silently producing no model signal (V-106's totality principle,
    // applied to the vocabulary bridge).
    let expected: [(K, SignalKind); 12] = [
        (K::Divergence, SignalKind::Divergence),
        (K::Threshold, SignalKind::Threshold),
        (K::ZeroLineCross, SignalKind::ZeroLineCross),
        (K::Crossover, SignalKind::CenterlineCross),
        (K::Breakout, SignalKind::LevelTouch),
        (K::BandTouch, SignalKind::LevelTouch),
        (K::LevelTest, SignalKind::LevelTouch),
        (K::CompressionRelease, SignalKind::StateChange),
        (K::TrendFlip, SignalKind::StateChange),
        (K::VolumeClimax, SignalKind::StateChange),
        (K::StackChange, SignalKind::StateChange),
        (K::PatternForming, SignalKind::StateChange),
    ];
    for (pipeline, model) in expected {
        assert_eq!(
            model_signal_kind(pipeline),
            model,
            "{pipeline:?} reduces to the wrong structural kind"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// V-203 — `at_bar` and `anchor` are carried, so a renderer can place a reading
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v203_every_reading_carries_the_bar_it_fired_on_and_its_anchor() {
    // A timestamp far enough from zero that the saturating subtraction is visible.
    let mut snap = snapshot_with_signals_on_one_indicator();
    snap.timestamp = 1_000;
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    let mut seen = Vec::new();
    for r in &axis_of(&state, AxisId::Direction).readings {
        let Signal::Event { at_bar, anchor, .. } = r else {
            panic!("expected an Event, got {r:?}");
        };
        assert!(
            *at_bar <= 1_000,
            "a signal cannot be filed on a bar after the current one: {at_bar}"
        );
        seen.push((*at_bar, *anchor));
    }

    // age_bars 0, 3, 7 on bar 999 → three distinct absolute bars, and the
    // anchor is the indicator's raw value.
    let mut bars: Vec<usize> = seen.iter().map(|(b, _)| *b).collect();
    bars.sort_unstable();
    bars.dedup();
    assert_eq!(
        bars.len(),
        3,
        "three signals with three different ages: {seen:?}"
    );
    assert!(
        seen.iter().all(|(_, a)| *a == 55.0),
        "the anchor is the producer's raw value: {seen:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// V-202 — one cap, 50, and it is a constant
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v202_the_cap_is_fifty_and_is_a_declared_constant() {
    assert_eq!(READINGS_CAP, 50, "the retention cap is 50 bars");
    // It must be reachable from outside the module, so a second cap cannot be
    // introduced privately alongside it.
    let v: usize = core_domain::axes::READINGS_CAP;
    assert_eq!(v, 50);
}

#[test]
fn v202_a_signal_older_than_the_cap_is_not_filed() {
    // 60 signals with ages 0..59 — the ones at or beyond the cap must not appear.
    let signals: Vec<IndicatorSignal> = (0..60u32)
        .map(|age| sig(core_domain::indicator_dtos::SignalKind::Crossover, age))
        .collect();
    let mut snap = snapshot_with(vec![(
        a_filed_direction_indicator(),
        value_with(50.0, signals),
    )]);
    snap.timestamp = 1_000;
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    let filed = axis_of(&state, AxisId::Direction).readings.len();
    assert!(
        filed <= READINGS_CAP,
        "{filed} readings published, cap is {READINGS_CAP} (V-202)"
    );
    // The oldest signals are the ones dropped — retention, not arbitrary.
    let bars: Vec<usize> = axis_of(&state, AxisId::Direction)
        .readings
        .iter()
        .filter_map(|r| match r {
            Signal::Event { at_bar, .. } => Some(*at_bar),
            _ => None,
        })
        .collect();
    let oldest = bars.iter().min().copied().unwrap_or(999);
    assert!(
        oldest >= 1_000 - READINGS_CAP,
        "a signal older than the cap was filed: bar {oldest}"
    );
}

#[test]
fn v202_push_reading_trims_from_the_oldest_end() {
    // Test the cap at the choke point, not only through the assembler.
    let mut axis = core_domain::axes::AxisState {
        axis: AxisId::Direction,
        position: core_domain::axes::AxisValue::rank(0.0),
        size: core_domain::axes::AxisValue::rank(0.0),
        readiness: core_domain::axes::Readiness::Solid,
        readings: Vec::new(),
    };
    for bar in 0..READINGS_CAP + 25 {
        axis.push_reading(Signal::Event {
            kind: core_domain::axes::SignalKind::Threshold,
            at_bar: bar,
            anchor: bar as f64,
            subtype: None,
        });
    }
    assert_eq!(axis.readings.len(), READINGS_CAP, "the cap is not enforced");
    // Oldest evicted, newest kept.
    match &axis.readings[0] {
        Signal::Event { at_bar, .. } => assert_eq!(
            *at_bar, 25,
            "the 501st push must evict the oldest, not the newest"
        ),
        _ => unreachable!(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// V-204 — C/Q4: a duplicate adds no information; a new one does
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v204_a_duplicate_signal_does_not_inflate_the_history() {
    // The same (indicator, kind, age) fired twice is one occurrence, not two.
    let k = core_domain::indicator_dtos::SignalKind::Crossover;
    let snap = snapshot_with(vec![(
        a_filed_direction_indicator(),
        value_with(50.0, vec![sig(k, 2), sig(k, 2)]),
    )]);
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");
    assert_eq!(
        axis_of(&state, AxisId::Direction).readings.len(),
        1,
        "the same signal at the same bar is one occurrence"
    );
}

#[test]
fn v204_a_genuinely_new_signal_is_retained() {
    let snap = snapshot_with(vec![(
        a_filed_direction_indicator(),
        value_with(
            50.0,
            vec![
                sig(core_domain::indicator_dtos::SignalKind::Crossover, 1),
                sig(core_domain::indicator_dtos::SignalKind::Divergence, 2),
            ],
        ),
    )]);
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");
    assert_eq!(
        axis_of(&state, AxisId::Direction).readings.len(),
        2,
        "two different signals on two bars are two facts"
    );
}

#[test]
fn v204_an_axis_with_no_readings_is_empty_not_stubbed() {
    // Typed absence (CH-7): an axis nothing filed under has an empty history.
    // That is different from the defect, which was an axis with signals filed
    // under it publishing an empty history.
    let snap = snapshot_with(vec![(
        a_filed_direction_indicator(),
        value_with(
            50.0,
            vec![sig(core_domain::indicator_dtos::SignalKind::Crossover, 0)],
        ),
    )]);
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    let direction = axis_of(&state, AxisId::Direction);
    assert!(
        !direction.readings.is_empty(),
        "the filed axis must have history"
    );
    // Every OTHER axis, with nothing filed, is legitimately empty.
    for id in [AxisId::Momentum, AxisId::Structure, AxisId::Progression] {
        let a = axis_of(&state, id);
        // They may or may not be empty depending on what the registry files
        // there; what must hold is that emptiness is honest — never a signal
        // attributed to the wrong axis.
        for r in &a.readings {
            if let Signal::Event {
                subtype: Some(s), ..
            } = r
            {
                assert!(
                    *s != core_domain::indicator_dtos::SignalKind::Crossover
                        || id == AxisId::Direction,
                    "{id:?} carries a signal filed under Direction"
                );
            }
        }
    }
}
