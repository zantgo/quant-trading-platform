//! CG3 — the far side of the model boundary.
//!
//! MME emits state, never a trade instruction. TAE now derives its own setup
//! from that state: per-duration bias/confidence plus the candle-derived ATR.
//! These tests pin that the derivation is real, bounded, and refuses neutral or
//! incomplete input.

use rust_decimal::Decimal;

use core_domain::alignment::AlignmentMatrix;
use core_domain::axes::{
    AxisId, AxisState, AxisValue, DiagnosticBasis, DiagnosticState, Diagnostics, Domain,
    EventCounts, Readiness, TimeframeIdentity, TimeframeState,
};
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::models::MarketSnapshot;
use portfolio_supervisor::setup_executor::extract_top_setup;

fn state_snapshot(direction: f64, diagnostic: DiagnosticState) -> MarketSnapshot {
    let axis = |axis, position| AxisState {
        axis,
        position,
        size: AxisValue::rank(100.0),
        readiness: Readiness::Solid,
        readings: Vec::new(),
    };
    let state = TimeframeState {
        identity: TimeframeIdentity {
            instance_id: "inst".into(),
            pair: "BTC-USDC".into(),
            exchange: "hyperliquid".into(),
            timeframe_secs: 60,
            timeframe_label: "1m".into(),
            candle_count: 500,
            last_closed_at: "2026-10-20T00:00:00Z".into(),
            available_at: "500".into(),
            spec_version: core_domain::SPEC_VERSION.into(),
            build_version: core_domain::BUILD_VERSION.into(),
        },
        axes: vec![
            axis(
                AxisId::Direction,
                AxisValue::signed(direction, Domain::DirectionalBias),
            ),
            axis(
                AxisId::Momentum,
                AxisValue::signed(0.1, Domain::Acceleration),
            ),
            axis(
                AxisId::Structure,
                AxisValue::signed(0.1, Domain::NetEventBias),
            ),
            axis(
                AxisId::Progression,
                AxisValue::ordinal(core_domain::axes::Stage::Mid),
            ),
        ],
        events: EventCounts::default(),
        levels: Vec::new(),
        atr: Some(2.0),
        legs: Vec::new(),
        diagnostics: Diagnostics {
            state: diagnostic,
            volatility_change_pct: 0.0,
            participation_change_pct: 0.0,
            basis: DiagnosticBasis {
                bbwp_pct: 20.0,
                rvol_pct: 100.0,
            },
        },
        derived: core_domain::axes::DerivedState {
            consensus: core_domain::axes::Consensus {
                agreeing_axes: 0,
                of: 4,
                axes: Vec::new(),
                weakest_axis: None,
            },
            stability: core_domain::axes::Stability {
                state: core_domain::axes::StabilityState::Holding,
                changes_last_50_bars: 0,
                window: 50,
            },
        },
    };
    let mut indicators = std::collections::HashMap::new();
    indicators.insert(
        "atr".into(),
        NormalizedIndicatorValue::scalar(2.0, 0.0, "X"),
    );
    MarketSnapshot {
        symbol: "BTC-USDC".into(),
        timeframe_secs: 60,
        timestamp: 1_700_000_000,
        is_completed: Some(true),
        mid_price: Decimal::from_f64_retain(100.0).unwrap(),
        close: Decimal::from_f64_retain(100.0),
        alignment: Some(AlignmentMatrix::empty("BTC-USDC")),
        indicators,
        timeframe_state: Some(state),
        ..MarketSnapshot::default()
    }
}

#[test]
fn cg3_tae_reads_timeframe_state() {
    let s = state_snapshot(0.88, DiagnosticState::Expanding);
    let plan = extract_top_setup(&[&s], 0.5).expect("state alone is tradable");
    assert_eq!(plan.direction, "LONG");
    assert_eq!(plan.setup_type, "MOMENTUM_BREAKOUT");
    assert_eq!(plan.confidence, 1.0);
    assert_eq!(plan.source_tf_secs, 60);
}

#[test]
fn cg3_tae_derives_a_bracketed_setup_from_model_state() {
    let s = state_snapshot(0.88, DiagnosticState::Expanding);
    let plan = extract_top_setup(&[&s], 0.5).expect("a directional model state is tradable");
    assert_eq!(plan.direction, "LONG");
    assert_eq!(plan.source_tf_secs, 60);
    assert_eq!(plan.symbol, "BTC-USDC");
    assert!(plan.entry_zone_low < plan.entry_mid);
    assert!(plan.entry_zone_high > plan.entry_mid);
    assert!(plan.sl < plan.entry_mid);
    assert!(plan.tp > plan.entry_mid);
    assert!(plan.net_rr >= 0.5);
    assert_eq!(plan.confidence, 1.0);
    assert!(!plan.fingerprint.is_empty());
}

#[test]
fn cg3_neutral_and_incomplete_shapes_yield_no_setup() {
    let neutral = state_snapshot(0.0, DiagnosticState::Coiling);
    assert!(extract_top_setup(&[&neutral], 0.5).is_none());

    let mut incomplete = state_snapshot(0.55, DiagnosticState::Settling);
    incomplete.is_completed = Some(false);
    assert!(extract_top_setup(&[&incomplete], 0.5).is_none());

    assert!(extract_top_setup(&[], 0.5).is_none());
}

#[test]
fn cg3_the_rr_gate_is_enforced() {
    let s = state_snapshot(0.55, DiagnosticState::Settling);
    assert!(extract_top_setup(&[&s], 2.5).is_none());
    assert!(extract_top_setup(&[&s], f64::NAN).is_none());
}

#[test]
fn cg3_an_invalid_candidate_does_not_abort_the_scan() {
    let mut invalid = state_snapshot(0.88, DiagnosticState::Expanding);
    invalid.close = None;
    invalid.mid_price = Decimal::from_f64_retain(0.0).unwrap();

    let mut valid = state_snapshot(0.88, DiagnosticState::Expanding);
    valid.timeframe_secs = 300;
    valid.timestamp += 1;

    let plan = extract_top_setup(&[&invalid, &valid], 0.5)
        .expect("a later valid candidate must still be considered");
    assert_eq!(plan.source_tf_secs, 300);
    assert_eq!(plan.source_candle_ts, valid.timestamp);
}
