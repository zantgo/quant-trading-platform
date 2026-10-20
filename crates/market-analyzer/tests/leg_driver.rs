//! **The leg reaches the wire** (P10-B, V-205…V-209).
//!
//! P10-A made `readings[]` real. The leg record had the same shape of defect
//! and a worse consequence: [`market_analyzer::leg::LegMachine`] was fully
//! specified and fully tested with **zero production callers**, so the
//! progression axis published a stage derived from a free-text label while the
//! record that owns progression was not in the process.
//!
//! These tests are written from the rules (L1–L27, S-N1…S-N6), not from the
//! driver's implementation:
//!
//! 1. **L2** — an origin is published on the bar that makes it knowable, never
//!    on the bar it describes.
//! 2. **L3** — a repeated or reordered bar is refused, not absorbed.
//! 3. **L8/L9** — one leg per `(instrument, duration, level)`; an active leg
//!    always has an invalidation.
//! 4. **L13/L22** — a stage is withheld while the reference is unstable or the
//!    classification provisional; it is never substituted.
//! 5. **The production bridge** — `timeframe_state_from_snapshot` carries the
//!    leg and derives progression from it, and a stage can never appear from a
//!    label with no leg behind it (the M3 defect).

use core_domain::axes::{AxisId, Stage};
use core_domain::leg::{Direction, LegLifecycle, Progress};
use core_domain::models::MarketSnapshot;
use market_analyzer::leg::{LegDriver, LegPolicy, PIVOT_STRENGTH};
use market_analyzer::model::live::timeframe_state_from_snapshot;
use rust_decimal::Decimal;

fn dec(v: f64) -> Decimal {
    Decimal::try_from(v).expect("representable decimal")
}

fn bar(at_ms: u64, high: f64, low: f64, close: f64) -> MarketSnapshot {
    MarketSnapshot {
        symbol: "BTC-USDT".into(),
        timeframe_secs: 300,
        timeframe_label: Some("5m".into()),
        timestamp: at_ms,
        high: Some(dec(high)),
        low: Some(dec(low)),
        close: Some(dec(close)),
        mid_price: dec(close),
        ..Default::default()
    }
}

fn driver() -> LegDriver {
    LegDriver::new(LegPolicy::default(), 0.01)
}

/// Five bars whose middle (index 2) is the window's high.
fn peak() -> Vec<MarketSnapshot> {
    vec![
        bar(300_000, 10.0, 9.0, 9.5),
        bar(600_000, 11.0, 10.0, 10.5),
        bar(900_000, 15.0, 11.0, 14.0),
        bar(1_200_000, 13.0, 11.0, 12.0),
        bar(1_500_000, 12.0, 10.0, 10.5),
    ]
}

#[test]
fn a_pivot_is_published_only_after_it_is_confirmed() {
    // L2: the origin is bar 2, but bar 2 alone does not know it is the high.
    // Until two later bars have failed to exceed it, there is nothing honest to
    // publish.
    let mut d = driver();
    let bars = peak();
    for (i, snapshot) in bars.iter().take(1 + PIVOT_STRENGTH).enumerate() {
        let legs = d.advance(snapshot, i);
        assert!(
            legs.is_empty(),
            "bar {i} published a leg before the pivot could be known"
        );
    }
    assert!(d.machine().legs().is_empty());
}

#[test]
fn the_origin_is_stamped_with_its_occurrence_and_its_availability() {
    let mut d = driver();
    for (i, snapshot) in peak().iter().enumerate() {
        d.advance(snapshot, i);
    }
    let legs = d.machine().legs();
    assert_eq!(legs.len(), 1, "exactly one confirmed pivot in this window");
    let leg = &legs[0];
    assert_eq!(leg.origin_occurred_at, 2, "the pivot bar");
    assert_eq!(leg.origin_available_at, 4, "the confirming bar");
    assert!(leg.origin_available_at >= leg.origin_occurred_at, "L2");
    assert_eq!(leg.direction, Direction::Short);
    assert_eq!(leg.lifecycle, LegLifecycle::Active, "L9: activation");
    assert!(
        leg.invalidation_price.is_some(),
        "L9: an active leg always declares its death"
    );
    assert!(leg.target_price.is_some(), "a leg states its reference");
}

#[test]
fn a_repeated_bar_is_refused_not_absorbed() {
    // L3: replaying a bar would reorder history inside an aggregate.
    let mut d = driver();
    let bars = peak();
    for (i, snapshot) in bars.iter().enumerate() {
        d.advance(snapshot, i);
    }
    let before = d.machine().legs()[0].clone();
    let refused = d.advance(&bars[4], 4);
    let after = &d.machine().legs()[0];
    assert_eq!(
        before.origin_available_at, after.origin_available_at,
        "the refused bar must not move anything"
    );
    assert_eq!(refused.len(), 1, "the projection is unchanged, not empty");
}

#[test]
fn one_leg_per_instrument_duration_and_level() {
    // L8: a new leg at a key replaces the record; it never accumulates.
    let mut d = driver();
    let bars = peak();
    for (i, snapshot) in bars.iter().enumerate() {
        d.advance(snapshot, i);
    }
    let keys = d
        .machine()
        .legs()
        .iter()
        .map(|l| (l.instrument.clone(), l.timeframe_secs, l.structure_level));
    let mut seen = Vec::new();
    for key in keys {
        assert!(!seen.contains(&key), "two legs share {key:?}");
        seen.push(key);
    }
    assert!(d.machine().legs().len() <= 2, "two structural levels");
}

#[test]
fn the_wire_carries_the_leg() {
    // V-205: `TimeframeState.legs` is populated on the live bridge, not just in
    // the machine's own memory.
    let mut d = driver();
    let bars = peak();
    let mut last = None;
    for (i, snapshot) in bars.iter().enumerate() {
        last = Some(timeframe_state_from_snapshot(
            snapshot, 500, "inst-1", &mut d,
        ));
        let _ = i;
    }
    let state = last.expect("a state");
    assert_eq!(state.legs.len(), 1);
    assert_eq!(state.legs[0].lifecycle, LegLifecycle::Active);
}

#[test]
fn the_driver_is_the_single_carrier_across_candles() {
    // V-206: a fresh driver per bar would forget the leg; the production call
    // site threads one driver for the whole run (analyzer/mod.rs). The
    // signature makes the carrier explicit, and this pins the continuity.
    let mut d = driver();
    let bars = peak();
    for (i, snapshot) in bars.iter().enumerate() {
        d.advance(snapshot, i);
    }
    // A later bar, same driver: the leg is still there.
    let after = d.advance(&bar(1_800_000, 11.0, 9.5, 10.0), 5);
    assert_eq!(after.len(), 1, "one carrier keeps one leg across bars");
    // A fresh driver starts with no history.
    assert!(driver()
        .advance(&bar(1_800_000, 11.0, 9.5, 10.0), 5)
        .is_empty());
}

#[test]
fn progression_comes_from_the_leg_and_is_withheld_honestly() {
    // V-207: the fields the assembler publishes are the leg's own.
    let mut d = driver();
    for (i, snapshot) in peak().iter().enumerate() {
        d.advance(snapshot, i);
    }
    let state = timeframe_state_from_snapshot(&peak()[4], 500, "inst-1", &mut d);
    let leg = &state.legs[0];

    // Progress is a real number against the reference, not a substituted zero.
    match &leg.progress {
        Progress::Value { value } => assert!(value.is_finite(), "L14: never NaN"),
        Progress::Undefined { reason } => panic!("progress should be defined: {reason:?}"),
    }
    assert_eq!(leg.target_version, 0, "the first target is not a revision");
    // L10/L22: with one observation the classification is provisional, so the
    // stage is withheld — `None` is the answer, never a substitute.
    assert_eq!(
        leg.stage, None,
        "a provisional classification publishes no stage"
    );
}

#[test]
fn a_stage_is_never_published_from_a_label_with_no_leg() {
    // The M3 defect: `progression_stage` derived from a published label while
    // the leg record was absent. With an EMPTY driver, no stage may appear —
    // whatever the snapshot's labels say.
    let mut snapshot = bar(0, 10.0, 9.0, 9.5);
    snapshot.indicators.insert(
        "smart_money".into(),
        core_domain::indicator_dtos::NormalizedIndicatorValue {
            raw_value: 1.0,
            normalized: 1.0,
            state_label: "early".into(),
            values: None,
            signals: Vec::new(),
            readiness: core_domain::axes::Readiness::Solid,
            available_at_bar: Some(1),
            subject_bar: Some(1),
        },
    );
    let mut d = driver();
    let state = timeframe_state_from_snapshot(&snapshot, 500, "inst-1", &mut d);
    assert!(state.legs.is_empty(), "no leg has been confirmed");
    let progression = state
        .axes
        .iter()
        .find(|a| a.axis == AxisId::Progression)
        .expect("the axis exists even when it has nothing to say");
    assert_eq!(
        progression.position.as_stage(),
        None,
        "a label is not a leg; the stage must be withheld"
    );
    assert!(progression.position.is_undefined(), "no leg means no stage");
}

#[test]
fn a_confirmed_leg_holds_the_wire_contract() {
    // V-208: the published subset of L1–L27. Each assertion is a rule, not a
    // description of the fixture.
    let mut d = driver();
    for (i, snapshot) in peak().iter().enumerate() {
        d.advance(snapshot, i);
    }
    let state = timeframe_state_from_snapshot(&peak()[4], 500, "inst-1", &mut d);
    for leg in &state.legs {
        assert!(
            leg.lifecycle != LegLifecycle::Forming,
            "L6: a forming leg publishes no reading"
        );
        assert!(
            leg.stage.is_none() || leg.target_stable,
            "L10: a stage requires a stable reference"
        );
        assert!(
            !matches!(leg.progress, Progress::Value { value } if !value.is_finite()),
            "L14: never NaN"
        );
        assert!(leg.target_version <= 1, "S-N6: only targets bump");
    }
    // The four axes are untouched by the leg: progression is not a fifth axis.
    assert_eq!(state.axes.len(), 4);
}

#[test]
fn the_leg_is_not_a_stage_machine_only_its_record_is_published() {
    // CH-8: the projection is deterministic — two drivers fed the same bars
    // publish the same reading, including the stage withholding.
    let run = || {
        let mut d = driver();
        for (i, snapshot) in peak().iter().enumerate() {
            d.advance(snapshot, i);
        }
        timeframe_state_from_snapshot(&peak()[4], 500, "inst-1", &mut d).legs
    };
    assert_eq!(run(), run());
}

#[test]
fn stage_values_are_the_ordinal_anchors_only_when_proven() {
    // L22 + the ontology: a stage is one of the five anchors, never a free
    // string. This is the type's contract, asserted here so a future change to
    // `as_stage` cannot quietly admit a label.
    for stage in [
        Stage::Early,
        Stage::EarlyMid,
        Stage::Mid,
        Stage::Late,
        Stage::Terminal,
    ] {
        assert_eq!(Stage::from_label(stage.label()), Some(stage));
    }
    assert_eq!(Stage::from_label("coiling"), None);
}

#[test]
fn a_settled_leg_eventually_publishes_a_stage() {
    // The withholding rules must not become permanent withholding. A smooth
    // oscillation confirms pivots at every extreme; once the classifier has
    // twenty observations behind it, a stable leg with defined progress MUST
    // publish an anchor — otherwise the axis is decorative and every test above
    // would still pass.
    let mut d = driver();
    let mut staged = 0usize;
    for i in 0..600u64 {
        let phase = (i as f64) * std::f64::consts::PI / 5.0;
        let mid = 100.0 + 10.0 * phase.sin();
        let snapshot = bar((i + 1) * 300_000, mid + 1.0, mid - 1.0, mid);
        for reading in d.advance(&snapshot, i as usize) {
            if reading.stage.is_some() {
                staged += 1;
                assert!(
                    !matches!(reading.progress, Progress::Undefined { .. }),
                    "L13: a stage requires defined progress"
                );
            }
        }
    }
    assert!(
        staged > 0,
        "600 bars of oscillation produced no published stage — the leg is \
         withholding forever, which is not the same as withholding honestly"
    );
}
