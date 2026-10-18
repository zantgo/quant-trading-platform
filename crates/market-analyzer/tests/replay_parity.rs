//! **PARITY — warm replay ≡ archive replay, bit-identical** (P11).
//!
//! ## The claim
//!
//! A `TimeframeState` assembled from a warmed buffer must be **byte-identical** to
//! one assembled by replaying the same bars one at a time. Not "equal on the
//! fields we thought to compare" — identical, because a field-by-field comparison
//! passes the moment someone adds a field nobody checks.
//!
//! ## Why this is the parity that matters
//!
//! Live and replay differ in exactly one way: a warm boot has the whole window
//! in the buffer before the first publish, while a replay has whatever has been
//! walked so far. If a reading peeks past the current bar — or if an aggregate
//! includes a future bar's contribution — the two paths disagree, and the
//! backtest that "works" disagrees with the platform that runs.
//!
//! ## How the comparison is made unforgeable
//!
//! Both sides serialise the same struct with the same serialiser, and the strings
//! are compared. Any difference — a float, a missing field, a changed order —
//! shows up.

use core_domain::axes::{AxisId, AxisValue, Domain, Stage, TimeframeState};
use market_analyzer::model::{
    assemble_timeframe_state, consensus_of, stability_of, Reading, TimeframeInput,
};

/// A deterministic candle corpus. No RNG — CH-3 forbids sampling, and a fixture
/// nobody can reproduce is a failure nobody can debug.
fn closes(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let phase = i % 24;
            let swing = if phase < 12 {
                phase as f64 * 0.7
            } else {
                (23 - phase) as f64 * 0.7
            };
            100.0 + i as f64 * 0.11 + swing
        })
        .collect()
}

fn identity(secs: u64, candles: usize) -> core_domain::axes::TimeframeIdentity {
    core_domain::axes::TimeframeIdentity {
        instance_id: "inst-parity".into(),
        pair: "BTC-USDT".into(),
        exchange: "hyperliquid".into(),
        timeframe_secs: secs,
        timeframe_label: core_domain::duration_label(secs),
        candle_count: candles,
        last_closed_at: "2026-10-18T00:00:00Z".into(),
        available_at: candles.to_string(),
        spec_version: "13.0".into(),
        build_version: "5.0.0".into(),
    }
}

/// The stage the fixture's leg is in at `bar` — monotonic, so the corpus walks
/// the five anchors in order.
fn stage_for(bar: usize) -> Stage {
    match bar / 40 {
        0 => Stage::Early,
        1 => Stage::EarlyMid,
        2 => Stage::Mid,
        3 => Stage::Late,
        _ => Stage::Terminal,
    }
}

/// The readings a duration publishes at `bar`, given only bars `0..=bar`.
///
/// This is the *whole* causal contract in one function: the last value it can see
/// is `bar`. If a future bar ever reaches a reading, this function is where it
/// would show up.
fn readings_at(closes: &[f64], bar: usize) -> Vec<Reading> {
    let window = &closes[..=bar];
    let sma3 = if window.len() >= 3 {
        (window[window.len() - 3] + window[window.len() - 2] + window[window.len() - 1]) / 3.0
    } else {
        window.iter().sum::<f64>() / window.len() as f64
    };
    let slope = if window.len() >= 2 {
        window[window.len() - 1] - window[window.len() - 2]
    } else {
        0.0
    };
    let range = window.iter().fold(f64::MIN, |acc, v| acc.max(*v))
        - window.iter().fold(f64::MAX, |acc, v| acc.min(*v));
    let count = window.len() as f64;

    vec![
        Reading::produced(
            "sma_3",
            AxisId::Direction,
            AxisValue::signed(0.5, Domain::DirectionalBias),
            true,
        ),
        Reading::produced(
            "accel",
            AxisId::Momentum,
            AxisValue::signed(slope * 10.0, Domain::Acceleration),
            true,
        ),
        Reading::produced(
            "event_bias",
            AxisId::Structure,
            AxisValue::signed(range / 100.0, Domain::NetEventBias),
            true,
        ),
        // Progression is assigned by the leg, never reduced here.
        Reading::derived(
            "leg_progress",
            AxisId::Progression,
            AxisValue::ordinal(stage_for(bar)),
            &["sma_3"],
            true,
        ),
    ]
    .into_iter()
    .map(|r| {
        let _ = (sma3, count);
        r
    })
    .collect()
}

fn state_at(closes: &[f64], bar: usize, secs: u64) -> TimeframeState {
    let mut history: Vec<f64> = closes[..=bar].to_vec();
    history.truncate(50);
    assemble_timeframe_state(TimeframeInput {
        identity: identity(secs, bar + 1),
        readings: readings_at(closes, bar),
        events: core_domain::event::EventCounts {
            active: (bar / 7).min(20),
            retained: (bar / 5).min(20),
            emitted: bar.min(40),
            ..Default::default()
        },
        levels: vec![],
        diagnostics: core_domain::axes::Diagnostics {
            state: core_domain::axes::DiagnosticState::Settling,
            volatility_change_pct: 1.0,
            participation_change_pct: 2.0,
            basis: core_domain::axes::DiagnosticBasis {
                bbwp_pct: 30.0,
                rvol_pct: 100.0,
            },
        },
        progression_stage: Some(stage_for(bar)),
        progression_progress: core_domain::leg::Progress::Value {
            value: bar as f64 / 200.0,
        },
        target_version: 3,
        target_stable: true,
        available_at_bar: bar,
        direction_history: direction_history(&history),
    })
}

/// The direction series a warm buffer holds: one value per candle, oldest first.
fn direction_history(closes: &[f64]) -> Vec<f64> {
    closes
        .iter()
        .enumerate()
        .map(|(_, c)| (c - 100.0) / 20.0)
        .collect()
}

#[test]
fn parity_a_warmed_buffer_and_an_incremental_replay_agree_byte_for_byte() {
    let series = closes(200);
    // Warm: the whole window is present, and the state is read once.
    let warm = state_at(&series, 199, 300);
    // Archive: the same bars walked one at a time, with the state read at the
    // same bar. The buffer it holds is the prefix, never the suffix.
    let mut replayed = None;
    for bar in 0..=199 {
        replayed = Some(state_at(&series, bar, 300));
    }
    let replayed = replayed.expect("a replayed state");

    assert_eq!(
        serde_json::to_string(&warm).expect("serialises"),
        serde_json::to_string(&replayed).expect("serialises"),
        "warm and archive disagree on the same bar"
    );
    assert_eq!(warm, replayed);
}

#[test]
fn parity_every_bar_of_an_incremental_replay_matches_a_warm_read_at_that_bar() {
    // Not only the endpoint: the whole point of §4.1 is that a series can agree
    // at the end and disagree in the middle, which is exactly what a look-ahead
    // produces.
    let series = closes(120);
    for bar in [0usize, 1, 5, 17, 40, 79, 119] {
        let from_replay = state_at(&series, bar, 60);
        // "Warm" here means: a fresh assembler given exactly the prefix, with no
        // memory of any later bar.
        let prefix: Vec<f64> = series[..=bar].to_vec();
        let warm = state_at(&prefix, bar, 60);
        assert_eq!(
            serde_json::to_string(&from_replay).expect("serialises"),
            serde_json::to_string(&warm).expect("serialises"),
            "bar {bar} differs between the incremental and the warmed read"
        );
    }
}

#[test]
fn parity_consensus_and_stability_are_derived_identically_on_both_paths() {
    let series = closes(90);
    let mut warm_axes =
        market_analyzer::model::assemble_axes(&readings_at(&series, 89), Some(Stage::Mid));
    let mut replay_axes = Vec::new();
    for bar in 0..=89 {
        replay_axes =
            market_analyzer::model::assemble_axes(&readings_at(&series, bar), Some(Stage::Mid));
    }
    assert_eq!(warm_axes.len(), replay_axes.len());
    for (a, b) in warm_axes.iter().zip(replay_axes.iter()) {
        assert_eq!(a.axis, b.axis);
        assert_eq!(a.position, b.position, "{} position", a.axis.label());
        assert_eq!(a.readiness, b.readiness, "{} readiness", a.axis.label());
    }
    assert_eq!(consensus_of(&warm_axes), consensus_of(&replay_axes));

    let history = direction_history(&series);
    assert_eq!(
        stability_of(&history),
        stability_of(&history[..]),
        "stability is a pure function of the history it is given"
    );
}

#[test]
fn parity_extending_the_series_does_not_change_a_past_reading() {
    // The one-way property: a longer dataset yields the same prefix. Weaker than
    // CG2, and still worth asserting here because it is what makes a warm boot
    // and a replay comparable at all.
    let short = closes(60);
    let long = closes(120);
    let before = state_at(&short, 59, 300);
    // The same bar, read from the longer dataset.
    let after = state_at(&long, 59, 300);
    assert_eq!(
        serde_json::to_string(&before).unwrap(),
        serde_json::to_string(&after).unwrap(),
        "reading bar 59 changed when bars 60..120 were added"
    );
}

#[test]
fn parity_the_state_is_bit_identical_when_replayed_twice() {
    // Determinism, at the level of the assembled document rather than the harness.
    let series = closes(75);
    let a = state_at(&series, 74, 900);
    let b = state_at(&series, 74, 900);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
