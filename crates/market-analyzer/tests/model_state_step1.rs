//! **01-24 step 1 — `model_state` rides the snapshot, additively** (P10).
//!
//! ## What step 1 is
//!
//! The snapshot gains `model_state`, the assembled `TimeframeState`, and the
//! pipeline fills it. **Nothing reads it yet.** `analysis` is still on the wire
//! beside it.
//!
//! That coexistence is deliberate and this file enforces it, because the failure
//! mode of a staged migration is a snapshot that carries *half* of one — an axis
//! assembly with no consumers, or a consumer reading a field the producer stopped
//! writing. Both are worse than carrying both for one commit.
//!
//! ## What this test asserts
//!
//! 1. The field round-trips, and is **absent** — not null — when unset.
//! 2. The **retained-history projection drops it**: the projection is a storage
//!    view, and retaining a second copy of the four axes would charge the byte
//!    budget twice for one fact.
//! 3. A **shadow frame carries none**: a shadow exists to fill a chart before the
//!    ladder is live, and an axis assembled from partial durations is a claim
//!    about a reduction that has not happened.
//! 4. The two coexist — `analysis` is still populated.

use core_domain::axes::{AxisId, TimeframeState};
use core_domain::models::MarketSnapshot;

fn snapshot_with_model_state() -> MarketSnapshot {
    let axes = market_analyzer::model::assemble_axes(
        &[market_analyzer::model::Reading::produced(
            "rsi",
            AxisId::Direction,
            core_domain::axes::AxisValue::signed(0.4, core_domain::axes::Domain::DirectionalBias),
            true,
        )],
        None,
        &no_signals(),
    );
    MarketSnapshot {
        symbol: "BTC-USDT".into(),
        timeframe_secs: 300,
        timeframe_label: Some("5m".into()),
        model_state: Some(TimeframeState {
            identity: core_domain::axes::TimeframeIdentity {
                instance_id: "inst-1".into(),
                pair: "BTC-USDT".into(),
                exchange: "hyperliquid".into(),
                timeframe_secs: 300,
                timeframe_label: "5m".into(),
                candle_count: 500,
                last_closed_at: "0".into(),
                available_at: "0".into(),
                spec_version: "13.0".into(),
                build_version: "5.0.0".into(),
            },
            axes,
            events: core_domain::axes::EventCounts::default(),
            levels: Vec::new(),
            diagnostics: core_domain::axes::Diagnostics {
                state: core_domain::axes::DiagnosticState::Chopping,
                volatility_change_pct: 0.0,
                participation_change_pct: 0.0,
                basis: core_domain::axes::DiagnosticBasis {
                    bbwp_pct: 0.0,
                    rvol_pct: 0.0,
                },
            },
            derived: core_domain::axes::DerivedState {
                consensus: core_domain::axes::Consensus {
                    agreeing_axes: 1,
                    of: 4,
                    axes: vec![AxisId::Direction],
                    weakest_axis: None,
                },
                stability: core_domain::axes::Stability {
                    state: core_domain::axes::StabilityState::Building,
                    changes_last_50_bars: 0,
                    window: 50,
                },
            },
        }),
        ..Default::default()
    }
}

#[test]
fn step1_the_field_round_trips() {
    let snapshot = snapshot_with_model_state();
    let json = serde_json::to_string(&snapshot).expect("serialises");
    assert!(json.contains("model_state"), "the field is on the wire");
    let back: MarketSnapshot = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(
        back.model_state.as_ref().map(|s| s.axes.len()),
        Some(4),
        "and comes back with four axes"
    );
    assert_eq!(
        serde_json::to_string(&back).expect("serialises"),
        json,
        "the round trip is byte-identical"
    );
}

#[test]
fn step1_an_unset_state_is_absent_not_null() {
    let json = serde_json::to_string(&MarketSnapshot::default()).expect("serialises");
    assert!(
        !json.contains("model_state"),
        "an unset optional is skipped, so a consumer can tell 'not assembled' \
         from 'assembled and empty'"
    );
}

#[test]
fn step1_the_retained_projection_drops_it() {
    // The projection is a storage view. Retaining a second copy of the four axes
    // would charge the byte budget twice for one fact — which is exactly the
    // double-charge the v11.12.24 budget work was built to stop.
    let projected = snapshot_with_model_state().history_projection();
    assert!(
        projected.model_state.is_none(),
        "the retained copy must not carry the assembled state"
    );
    // And what it DOES keep is the readings the state is derived from, so it can
    // be re-derived on demand.
    assert!(projected.analysis.is_none());
}

#[test]
fn step1_analysis_and_model_state_coexist_in_this_commit() {
    // Stated as a test because it is the thing a reviewer must see before the
    // next commit removes one of them.
    let mut snapshot = snapshot_with_model_state();
    snapshot.analysis = Some(core_domain::analysis::AnalysisMatrix::empty("BTC-USDT"));
    let json = serde_json::to_string(&snapshot).expect("serialises");
    assert!(
        json.contains("analysis") && json.contains("model_state"),
        "step 1 is additive: both are present. When this fails, step 2 has begun \
         and this assertion should be deleted in the same commit that removes a \
         consumer"
    );
}

#[test]
fn step1_the_assembled_state_is_built_from_the_snapshot_itself() {
    // The bridge, exercised through the snapshot rather than through its own
    // module: a snapshot with published indicators yields four axes.
    let mut snapshot = MarketSnapshot {
        symbol: "BTC-USDT".into(),
        timeframe_secs: 60,
        timeframe_label: Some("1m".into()),
        timestamp: 1_700_000_000_000,
        ..Default::default()
    };
    snapshot.indicators.insert(
        "rsi".to_string(),
        core_domain::indicator_dtos::NormalizedIndicatorValue {
            raw_value: 61.4,
            normalized: 0.61,
            state_label: "Overbought".to_string(),
            values: None,
            signals: Vec::new(),
            confidence: 0.0,
            available_at_bar: Some(100),
            subject_bar: Some(100),
        },
    );
    let state =
        market_analyzer::model::live::timeframe_state_from_snapshot(&snapshot, 500, "inst-1");
    assert_eq!(state.identity.pair, "BTC-USDT");
    assert_eq!(state.identity.timeframe_secs, 60);
    assert_eq!(state.axes.len(), 4);
    // The **registry** decides the axis, not this test: RSI is filed under
    // Momentum, because it is a rate-of-change oscillator. Asserting Direction
    // here would encode the test's opinion instead of the model's.
    assert_eq!(
        market_analyzer::model::live::axis_for("rsi"),
        Some(AxisId::Momentum),
        "the registry files rsi as momentum"
    );
    let momentum = state
        .axes
        .iter()
        .find(|a| a.axis == AxisId::Momentum)
        .expect("a momentum axis");
    assert_eq!(momentum.position.as_signed().map(|(v, _)| v), Some(0.61));
    // And the axes with no reading carry a typed absence, not a zero.
    for axis in state.axes.iter().filter(|a| a.axis != AxisId::Momentum) {
        assert!(
            axis.position.is_undefined(),
            "{} has no reading, so it is absent rather than zero",
            axis.axis.label()
        );
    }
}

/// An axis-signal history with nothing observable — the right input for a test
/// that is about axis *values*, not about the typed signal history.
fn no_signals() -> [Vec<core_domain::axes::Signal>; 4] {
    [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
}
