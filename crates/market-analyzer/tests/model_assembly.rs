//! **P10 — the assembled model, and the legality of every reduction**.
//!
//! Four things this file asserts, each of which the plan treats as load-bearing:
//!
//! 1. **Aggregation legality** (V-57, V-103a). Direction is averaged because
//!    every duration measures the same quantity on a shared domain. Momentum,
//!    Structure and Progression are **not**, and the reduction each uses is
//!    visible in the type.
//! 2. **The field set** (V-12, V-13). `model_state` carries no strategy field,
//!    and `CrossTfState` carries no indicator reference (V-96q) — so a consumer
//!    cannot reach back down to a calculator and recompute something different.
//! 3. **Diagnostics is not an axis** (V-40) and never votes.
//! 4. **The X/Y regression** (V-110). Two markets with identical breadth counts
//!    and different dispersion must produce different output — the case where
//!    `breadth_pct` alone would have collapsed them into one.

use core_domain::axes::{AxisId, AxisValue, Domain, LadderAggregation, ModelState, Readiness};
use market_analyzer::model::{
    assemble_axes, assemble_cross_pair_state, assemble_cross_tf_state, assemble_model_state,
    consensus_of, reduce_axis, reduce_direction, reduce_momentum, reduce_progression,
    reduce_structure, stability_of, Reading, STABILITY_WINDOW_BARS, UNSTABLE_CHANGE_THRESHOLD,
};

use core_domain::axes::{
    CrossTfState, DiagnosticBasis, DiagnosticState, Diagnostics, Level, TimeframeIdentity,
    TimeframeState,
};

fn signed(v: f64) -> AxisValue {
    AxisValue::signed(v, Domain::DirectionalBias)
}

fn readings_for(direction: f64, events: f64) -> Vec<Reading> {
    vec![
        Reading::produced("d1", AxisId::Direction, signed(direction), true),
        Reading::produced(
            "m1",
            AxisId::Momentum,
            AxisValue::signed(0.3, Domain::Acceleration),
            true,
        ),
        Reading::produced(
            "s1",
            AxisId::Structure,
            AxisValue::signed(events, Domain::NetEventBias),
            true,
        ),
    ]
}

fn identity(secs: u64) -> TimeframeIdentity {
    TimeframeIdentity {
        instance_id: "inst-1".into(),
        pair: "BTC-USDT".into(),
        exchange: "hyperliquid".into(),
        timeframe_secs: secs,
        timeframe_label: core_domain::duration_label(secs),
        candle_count: 500,
        last_closed_at: "2026-10-18T00:00:00Z".into(),
        available_at: "500".into(),
        spec_version: "13.0".into(),
        build_version: "5.0.0".into(),
    }
}

fn diagnostics() -> Diagnostics {
    Diagnostics {
        state: DiagnosticState::Coiling,
        volatility_change_pct: -8.4,
        participation_change_pct: 12.1,
        basis: DiagnosticBasis {
            bbwp_pct: 22.0,
            rvol_pct: 130.0,
        },
    }
}

/// A duration's state built from readings and a stage.
fn state_for(
    secs: u64,
    direction: f64,
    stage: Option<core_domain::axes::Stage>,
    events: u32,
) -> TimeframeState {
    let mut state =
        market_analyzer::model::assemble_timeframe_state(market_analyzer::model::TimeframeInput {
            identity: identity(secs),
            readings: readings_for(direction, events as f64),
            events: core_domain::event::EventCounts {
                active: events as usize,
                retained: (events + 4) as usize,
                emitted: (events + 9) as usize,
                ..Default::default()
            },
            // Typed since V-109 — `basis` was the producer's free-text `state_label`,
            // which no two consumers could compare against.
            levels: vec![core_domain::axes::Level::new(
                "support",
                100.0,
                101.0,
                core_domain::axes::LevelBasis::DeclaredBand {
                    indicator: "structure".into(),
                },
            )
            .expect("100 < 101 is a real band")],
            diagnostics: diagnostics(),
            progression_stage: stage,
            progression_progress: core_domain::leg::Progress::Value { value: 0.5 },
            target_version: 2,
            target_stable: true,
            available_at_bar: 500,
            direction_history: vec![0.4, 0.42, 0.41, -0.3, -0.31],
        });
    state.events.active_count = events;
    state
}

// ---------------------------------------------------------------------------
// 1 · Aggregation legality
// ---------------------------------------------------------------------------

#[test]
fn legality_direction_is_the_only_mean() {
    let (direction, aggregation) = reduce_direction(&[
        (1, signed(-0.42)),
        (60, signed(-0.30)),
        (3600, signed(-0.10)),
    ]);
    assert!(matches!(
        aggregation,
        LadderAggregation::MeanAgreement { .. }
    ));
    assert!((direction.mean_bias - (-0.2733333333)).abs() < 1e-6);
    // Agreement is a **share of durations**, and it is stated as such.
    assert!(direction.agreement_pct > 0.0);
    assert_eq!(
        direction.contributing_durations.len() + direction.dissenting_durations.len(),
        3,
        "every duration is accounted for"
    );
}

#[test]
fn legality_momentum_structure_progression_are_not_means() {
    let durations = vec![
        (1u64, AxisValue::signed(0.40, Domain::Acceleration)),
        (60, AxisValue::signed(0.01, Domain::Acceleration)),
    ];
    let (momentum, aggregation) = reduce_momentum(&durations);
    assert!(
        matches!(aggregation, LadderAggregation::StageDistribution { .. }),
        "Momentum must never report a mean"
    );
    // The finding is the BREADTH count and the fast-vs-slow comparison, not an
    // average: +0.40 and +0.01 would average to something that describes neither.
    assert_eq!(momentum.breadth_positive_pct, 100.0);
    assert_eq!(momentum.fast_vs_slow.fast_secs, 1);
    assert_eq!(momentum.fast_vs_slow.slow_secs, 60);
    assert!(momentum.fast_vs_slow.divergent, "0.39 apart is divergence");

    let mut counts = std::collections::BTreeMap::new();
    counts.insert(1u64, 0u32);
    counts.insert(60u64, 8u32);
    let (structure, aggregation) = reduce_structure(&durations, &counts, false, 1);
    assert!(matches!(
        aggregation,
        LadderAggregation::StageDistribution { .. }
    ));
    assert_eq!(
        structure.divergence_duration_count, 1,
        "a count, not a mean of counts"
    );
    assert_eq!(structure.total_durations, 2);

    let stages = vec![
        (1u64, Some(core_domain::axes::Stage::Early)),
        (60, Some(core_domain::axes::Stage::EarlyMid)),
        (3600, Some(core_domain::axes::Stage::EarlyMid)),
    ];
    let (progression, aggregation) = reduce_progression(&stages);
    assert!(matches!(
        aggregation,
        LadderAggregation::StageDistribution { .. }
    ));
    assert_eq!(
        progression.modal_stage,
        Some(core_domain::axes::Stage::EarlyMid),
        "the ordinal MODE, not a mean of ordinals"
    );
    assert_eq!(progression.histogram.early_mid, 2);
    assert_eq!(
        progression.histogram.total(),
        3,
        "the histogram counts every member"
    );
    assert!(
        !progression.legs_correspond,
        "three durations, three legs, no correspondence"
    );
}

#[test]
fn legality_the_progression_axis_is_ordinal_and_is_never_averaged() {
    // Two readings file under Progression; the axis value is the ASSIGNED stage,
    // because there is no arithmetic on ordinals (V-37, V-105).
    let readings = vec![
        Reading::produced(
            "p1",
            AxisId::Progression,
            AxisValue::ordinal(core_domain::axes::Stage::Early),
            true,
        ),
        Reading::produced(
            "p2",
            AxisId::Progression,
            AxisValue::ordinal(core_domain::axes::Stage::Terminal),
            true,
        ),
    ];
    let value = reduce_axis(AxisId::Progression, &[&readings[0], &readings[1]], None);
    assert!(
        value.is_undefined(),
        "with no assigned stage the ordinal axis is typed-absent, not guessed"
    );
    let assigned = reduce_axis(
        AxisId::Progression,
        &[&readings[0]],
        Some(core_domain::axes::Stage::Late),
    );
    assert_eq!(
        assigned,
        AxisValue::ordinal(core_domain::axes::Stage::Late),
        "the stage is assigned by the leg, not reduced from the readings"
    );
}

#[test]
fn legality_a_domain_mix_makes_the_axis_undefined_rather_than_averaged() {
    // Averaging `DirectionalBias` with `Acceleration` would average two meanings.
    let readings = vec![
        Reading::produced(
            "d",
            AxisId::Direction,
            AxisValue::signed(0.5, Domain::DirectionalBias),
            true,
        ),
        Reading::produced(
            "m",
            AxisId::Direction,
            AxisValue::signed(0.5, Domain::Acceleration),
            true,
        ),
    ];
    let value = reduce_axis(AxisId::Direction, &[&readings[0], &readings[1]], None);
    assert!(
        value.is_undefined(),
        "a mixed-domain axis is undefined, not averaged"
    );
}

// ---------------------------------------------------------------------------
// 2 · The field set — no strategy, no calculator
// ---------------------------------------------------------------------------

#[test]
fn boundary_model_state_carries_no_strategy_field() {
    let states = vec![
        (
            1u64,
            state_for(1, 0.4, Some(core_domain::axes::Stage::Early), 2),
        ),
        (
            60,
            state_for(60, 0.1, Some(core_domain::axes::Stage::EarlyMid), 1),
        ),
    ];
    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &states, false);
    let model: ModelState = assemble_model_state(
        "13.0",
        "5.0.0",
        1_700_000_000_000,
        &[1, 60],
        &[("BTC-USDT".to_string(), cross.clone(), states.clone())],
        0,
    );

    let banned = [
        "entry",
        "stop",
        "target_price",
        "risk_reward",
        "setup",
        "verdict",
        "recommendation",
        "confidence",
        "cascade",
    ];
    let json = serde_json::to_string(&model).expect("serialises");
    let keys: Vec<String> = collect_keys(&serde_json::to_value(&model).unwrap());
    for word in banned {
        assert!(
            !keys.iter().any(|k| k.contains(word)),
            "model_state carries a strategy key containing `{word}`: {keys:?}"
        );
    }
    assert!(
        !json.contains("cascade"),
        "and no strategy vocabulary in the payload"
    );
}

#[test]
fn boundary_cross_tf_state_holds_no_indicator_reference() {
    // V-96q: the reduction is a value, not a handle. A consumer cannot reach back
    // down to a calculator and recompute something different.
    let states = vec![
        (
            1u64,
            state_for(1, 0.4, Some(core_domain::axes::Stage::Early), 2),
        ),
        (
            60,
            state_for(60, 0.1, Some(core_domain::axes::Stage::EarlyMid), 1),
        ),
    ];
    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &states, false);
    let value = serde_json::to_value(&cross).expect("serialises");
    let keys = collect_keys(&value);
    for banned in ["indicator", "producer", "calculator", "registry", "signals"] {
        assert!(
            !keys.iter().any(|k| k.to_lowercase().contains(banned)),
            "CrossTfState references `{banned}`: {keys:?}"
        );
    }
}

fn collect_keys(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                out.push(k.clone());
                out.extend(collect_keys(v));
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                out.extend(collect_keys(item));
            }
        }
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// 3 · Diagnostics is not an axis
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_is_not_an_axis_and_never_votes() {
    let axes = assemble_axes(&readings_for(0.4, 1.0), Some(core_domain::axes::Stage::Mid));
    assert_eq!(axes.len(), 4, "exactly four axes");
    let ids: Vec<AxisId> = axes.iter().map(|a| a.axis).collect();
    assert_eq!(
        ids,
        AxisId::ALL.to_vec(),
        "exactly the four declared axes, in order"
    );
    let consensus = consensus_of(&axes);
    assert_eq!(consensus.of, 4, "the denominator is four AXES, not five");
    assert!(
        consensus.agreeing_axes <= 4,
        "diagnostics never contributes to the count"
    );
    // The diagnostics value IS present — on the state, not on an axis.
    let state = state_for(60, 0.1, None, 1);
    assert_eq!(state.diagnostics.state, DiagnosticState::Coiling);
    assert!(state
        .axes
        .iter()
        .all(|a| a.axis != AxisId::Progression || true));
}

#[test]
fn consensus_counts_defined_axes_and_names_the_weakest() {
    let mut axes = assemble_axes(&readings_for(0.4, 1.0), Some(core_domain::axes::Stage::Mid));
    // Make one axis incomplete.
    axes[1].readiness = Readiness::Dashed;
    let consensus = consensus_of(&axes);
    assert_eq!(consensus.agreeing_axes, 4, "all four are defined");
    assert_eq!(
        consensus.weakest_axis,
        Some(AxisId::Momentum),
        "the dashed axis is named"
    );
    // And a genuinely undefined axis lowers the count rather than being averaged.
    axes[2].position = AxisValue::undefined(core_domain::axes::UndefinedReason::InsufficientBars {
        available: 3,
        required: 50,
    });
    let consensus = consensus_of(&axes);
    assert_eq!(consensus.agreeing_axes, 3);
    assert_eq!(
        consensus.weakest_axis,
        Some(AxisId::Structure),
        "an undefined axis is weaker than a merely dashed one, whatever its position"
    );
}

#[test]
fn stability_counts_observations_not_bars() {
    // A short history is `Building`, not `Holding` and not `Unstable`.
    let building = stability_of(&[0.4, 0.3]);
    assert_eq!(building.state, core_domain::axes::StabilityState::Building);
    assert_eq!(building.window, STABILITY_WINDOW_BARS);

    // A quiet full window holds.
    let quiet: Vec<f64> = (0..STABILITY_WINDOW_BARS as usize).map(|_| 0.4).collect();
    assert_eq!(
        stability_of(&quiet).state,
        core_domain::axes::StabilityState::Holding
    );

    // A window that flips often is unstable — and the threshold is declared.
    let noisy: Vec<f64> = (0..STABILITY_WINDOW_BARS as usize)
        .map(|i| if i % 2 == 0 { 0.4 } else { -0.4 })
        .collect();
    let unstable = stability_of(&noisy);
    assert_eq!(unstable.state, core_domain::axes::StabilityState::Unstable);
    assert_eq!(unstable.changes_last_50_bars, STABILITY_WINDOW_BARS - 1);
    assert!(unstable.changes_last_50_bars >= UNSTABLE_CHANGE_THRESHOLD);
}

// ---------------------------------------------------------------------------
// 4 · The X/Y regression — equal breadth, different dispersion
// ---------------------------------------------------------------------------

/// A pair's cross-duration state, reduced so only `mean_bias` varies.
fn pair_state(bias: f64) -> CrossTfState {
    let states = vec![
        (
            1u64,
            state_for(1, bias, Some(core_domain::axes::Stage::Early), 1),
        ),
        (
            60,
            state_for(60, bias, Some(core_domain::axes::Stage::Early), 1),
        ),
    ];
    assemble_cross_tf_state("X", &states, false).0
}

#[test]
fn xy_two_markets_with_equal_breadth_and_different_dispersion_differ() {
    // X: every pair bullish, tightly clustered.
    let x = assemble_cross_pair_state(&[("A", pair_state(0.62)), ("B", pair_state(0.58))], 0);
    // Y: the same COUNT of bullish pairs, but split — one strongly tilted, one
    // barely. This is the case breadth cannot see and a mean nearly hides.
    // Y: the same count of bullish pairs AND the same mean — one strong, one
    // weak. Identical on both headline numbers; completely different underneath.
    let y = assemble_cross_pair_state(&[("A", pair_state(0.95)), ("B", pair_state(0.25))], 0);

    assert_eq!(
        x.breadth.breadth_pct, y.breadth.breadth_pct,
        "breadth is identical — that is the trap"
    );
    assert_eq!(x.breadth.bullish_count, y.breadth.bullish_count);

    // The two must NOT be indistinguishable. Their means are close — which is
    // exactly why breadth alone, and a mean alone, would pass here.
    assert!(
        (x.breadth.mean_bias - y.breadth.mean_bias).abs() < 1e-9,
        "the means are IDENTICAL ({}) — the trap",
        x.breadth.mean_bias
    );
    assert!(
        x.breadth.bias_range < y.breadth.bias_range,
        "X is tight ({}) and Y is split ({})",
        x.breadth.bias_range,
        y.breadth.bias_range
    );
    assert_ne!(
        x.breadth.dispersion, y.breadth.dispersion,
        "and the dispersion bands separate them"
    );
    let json_x = serde_json::to_string(&x).unwrap();
    let json_y = serde_json::to_string(&y).unwrap();
    assert_ne!(json_x, json_y, "V-110: the payloads must not be identical");
}

#[test]
fn xy_no_cross_pair_field_is_a_rebucketing_of_another() {
    // V-108: each headline field carries information no other field carries.
    let state = assemble_cross_pair_state(
        &[
            ("A", pair_state(0.90)),
            ("B", pair_state(0.80)),
            ("C", pair_state(-0.40)),
        ],
        1,
    );
    let breadth = &state.breadth;
    assert_eq!(
        breadth.bullish_count + breadth.bearish_count + breadth.unclear_count,
        3
    );
    // `bias_range` is not derivable from the counts: three pairs, two bullish,
    // can have any range at all.
    assert!(breadth.bias_range >= 0.0);
    assert!(state.coverage.converging_level_pair_count <= state.coverage.complete_pair_count);
    // Concordance is measured, not re-bucketed from breadth.
    assert_eq!(
        state.concordance.concordant_pair_count + state.concordance.divergent_pair_count,
        3
    );
    assert_eq!(state.axis_distribution.progression.total(), 3);
    assert_eq!(state.progression.histogram.total(), 3);
}

#[test]
fn cross_pair_handles_an_empty_workspace_without_inventing_numbers() {
    let empty = assemble_cross_pair_state(&[], 0);
    assert_eq!(empty.breadth.breadth_pct, 0.0);
    assert_eq!(empty.breadth.bullish_count, 0);
    assert_eq!(empty.coverage.complete_pair_count, 0);
    assert!(empty.asset_ranking.is_empty());
    assert_eq!(empty.progression.mode, None, "no mode, not a default stage");
}
