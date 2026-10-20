//! **The cross-pair `breadth_pct` is SIGNED — the net bullishness of the
//! watched pairs, not the bullish share.**
//!
//! # The defect
//!
//! `CrossPairBreadth` carries three counts (`bullish_count`, `bearish_count`,
//! `unclear_count`) and one number derived from them. The number was computed
//! as the **bullish share**:
//!
//! ```text
//! breadth_pct = 100 × bullish_count / total
//! ```
//!
//! That quantity cannot go negative. A workspace where 4 pairs are bullish and
//! 5 are bearish reports **+40%** — the same `+40%` a workspace with 4
//! bullish and 1 bearish reports. The bearish pairs are invisible: the number
//! says "40% of my pairs are up" and never says "but five are down".
//!
//! # Why signed is the contract
//!
//! Three independent sources already specify the signed form, and the codebase
//! already implements it in one place:
//!
//! 1. **`core-domain/tests/axes_contract.rs`** asserts the *stored* value
//!    satisfies `breadth_pct / 100 == (bullish − bearish) / total` for the
//!    fixture with 4 bullish / 5 bearish / 1 unclear — i.e. `−10.0`, which the
//!    fixture literally carries. The *producer* disagreed with the *contract
//!    test* for the same data, and the contract test only exercised the
//!    fixture, so the divergence was invisible.
//! 2. **The charter** (`01-22-charter-close-and-axis-filing.md` §`cross_pair_state`)
//!    tabulates the same scenario — "4 bullish | 5 bearish | 1 unclear" — and
//!    gives its count breadth as **−10%**, and states the point: two markets
//!    with an identical breadth *count* and a different spread must stay
//!    distinguishable.
//! 3. **The L7 consumer** (`OverviewMatrix.breadth_pct`) is documented in
//!    `02-09-overview-matrix.md` §3.2 as `(long_count − short_count) / total ×
//!    100` — signed, feeding the −100 %…+100 % UI gauge. The daemon copies
//!    `cross_pair_state.breadth.breadth_pct` straight into it, so the L7 gauge
//!    inherited the unsigned number too: a net-bearish workspace rendered
//!    "0% breadth" at best and "+40% breadth" at worst.
//!
//! The `+40%` case is not a rounding quibble. The L7 gauge and the L7
//! `GlobalBias` classification both read it, so a workspace that is net
//! −10% rendered as a *bullish* 40% and classified its bias from a positive
//! input.
//!
//! # What this test locks
//!
//! The producer and the contract agree, on the exact scenario the charter
//! tabulates.

use core_domain::axes::{AxisId, AxisValue, CrossTfState, Domain, TimeframeState};
use market_analyzer::model::{
    assemble_cross_pair_state, assemble_cross_tf_state, Reading, TimeframeInput,
};

fn identity(secs: u64) -> core_domain::axes::TimeframeIdentity {
    core_domain::axes::TimeframeIdentity {
        instance_id: "i".into(),
        pair: "BTC-USDT".into(),
        exchange: "Hyperliquid".into(),
        timeframe_secs: secs,
        timeframe_label: "1m".into(),
        candle_count: 500,
        last_closed_at: "t".into(),
        available_at: "t".into(),
        spec_version: core_domain::SPEC_VERSION.into(),
        build_version: core_domain::BUILD_VERSION.into(),
    }
}

fn readings_for(direction: f64) -> Vec<Reading> {
    vec![Reading::produced(
        "d1",
        AxisId::Direction,
        AxisValue::signed(direction, Domain::DirectionalBias),
        true,
    )]
}

fn diagnostics() -> core_domain::axes::Diagnostics {
    core_domain::axes::Diagnostics {
        state: core_domain::axes::DiagnosticState::Settling,
        volatility_change_pct: 0.0,
        participation_change_pct: 0.0,
        basis: core_domain::axes::DiagnosticBasis {
            bbwp_pct: 50.0,
            rvol_pct: 1.0,
        },
    }
}

fn state_for(secs: u64, direction: f64) -> TimeframeState {
    let mut s = market_analyzer::model::assemble_timeframe_state(TimeframeInput {
        identity: identity(secs),
        readings: readings_for(direction),
        events: core_domain::event::EventCounts {
            active: 0,
            retained: 4,
            emitted: 9,
            ..Default::default()
        },
        levels: vec![],
        atr: Some(1.0),
        diagnostics: diagnostics(),
        progression_stage: None,
        progression_progress: core_domain::leg::Progress::Value { value: 0.5 },
        target_version: 2,
        target_stable: true,
        available_at_bar: 500,
        direction_history: vec![0.4],
        axis_signals: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        legs: Vec::new(),
    });
    s.events.active_count = 0;
    s
}

/// A pair's cross-duration state, reduced so only `mean_bias` varies.
fn pair_state(bias: f64) -> CrossTfState {
    let states = vec![(1u64, state_for(1, bias)), (60, state_for(60, bias))];
    assemble_cross_tf_state("X", &states).0
}

#[test]
fn the_charter_scenario_reports_negative_breadth() {
    // The charter's own table: 4 bullish, 5 bearish, 1 unclear → −10%.
    // Ten pairs, four of them tilted up.
    let mut pairs: Vec<(&str, CrossTfState)> = Vec::new();
    for i in 0..4 {
        pairs.push((Box::leak(format!("bull{i}").into_boxed_str()), pair_state(0.60)));
    }
    for i in 0..5 {
        pairs.push((Box::leak(format!("bear{i}").into_boxed_str()), pair_state(-0.50)));
    }
    pairs.push(("flat", pair_state(0.0)));

    let cp = assemble_cross_pair_state(&pairs, 0);

    assert_eq!(cp.breadth.bullish_count, 4);
    assert_eq!(cp.breadth.bearish_count, 5);
    assert_eq!(cp.breadth.unclear_count, 1);

    assert!(
        (cp.breadth.breadth_pct - (-10.0)).abs() < 1e-9,
        "4 bullish / 5 bearish / 1 unclear must read as −10% (the charter's own \
         number), but the producer emitted {}. The bullish SHARE (100 × 4/10 = \
         +40%) cannot go negative, so it hides every bearish pair.",
        cp.breadth.breadth_pct
    );
}

#[test]
fn breadth_is_signed_in_both_directions() {
    // Two bullish, one bearish → +33.33%, not the bullish share (+66.67%).
    let cp = assemble_cross_pair_state(
        &[
            ("A", pair_state(0.50)),
            ("B", pair_state(0.50)),
            ("C", pair_state(-0.50)),
        ],
        0,
    );
    assert_eq!(cp.breadth.bullish_count, 2);
    assert_eq!(cp.breadth.bearish_count, 1);
    assert!(
        (cp.breadth.breadth_pct - (1.0 / 3.0 * 100.0)).abs() < 1e-9,
        "2 bullish / 1 bearish must read as +33.33%, but the producer emitted {} — \
         that is the bullish SHARE (+66.67%), which cannot express a bearish tilt.",
        cp.breadth.breadth_pct
    );

    // The mirror: one bullish, two bearish → −33.33%. The bullish share would
    // report +33.33% here — the *opposite sign* of the truth.
    let cp = assemble_cross_pair_state(
        &[
            ("A", pair_state(0.50)),
            ("B", pair_state(-0.50)),
            ("C", pair_state(-0.50)),
        ],
        0,
    );
    assert!(
        (cp.breadth.breadth_pct - (-1.0 / 3.0 * 100.0)).abs() < 1e-9,
        "1 bullish / 2 bearish must read as −33.33%, but the producer emitted {} — \
         a net-bearish workspace reporting a POSITIVE breadth is the whole defect.",
        cp.breadth.breadth_pct
    );
}

#[test]
fn breadth_pct_agrees_with_its_own_counts() {
    // The invariant `axes_contract.rs` already asserts against the fixture:
    // the stored number must be the signed net of the stored counts.
    for (bull, bear) in [(4u32, 5u32), (2, 1), (1, 2), (3, 3), (0, 1)] {
        let mut pairs: Vec<(&str, CrossTfState)> = Vec::new();
        for i in 0..bull {
            pairs.push((
                Box::leak(format!("b{i}").into_boxed_str()),
                pair_state(0.50),
            ));
        }
        for i in 0..bear {
            pairs.push((
                Box::leak(format!("r{i}").into_boxed_str()),
                pair_state(-0.50),
            ));
        }
        let cp = assemble_cross_pair_state(&pairs, 0);
        let total = (bull + bear) as f64;
        let derived = (bull as f64 - bear as f64) / total * 100.0;
        assert!(
            (cp.breadth.breadth_pct - derived).abs() < 1e-9,
            "{bull} bullish / {bear} bearish: stored breadth_pct {} != the signed net \
             derived from its own counts {derived}",
            cp.breadth.breadth_pct
        );
    }
}
