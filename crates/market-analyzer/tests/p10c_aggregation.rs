//! **P10-C — legal cross-duration aggregation** (V-210…V-214).
//!
//! Each test names the rule it pins, not the fixture it happens to use. The
//! builder is fed deliberately disagreeing members so a caller-supplied or
//! stubbed answer cannot pass.

use std::collections::{BTreeMap, HashMap};

use core_domain::axes::{
    AxisId, AxisValue, DiagnosticBasis, DiagnosticState, Diagnostics, Domain, Level, LevelBasis,
    Readiness, StabilityState, TimeframeState,
};
use core_domain::leg::{
    assess_terminality_from_outcomes, Classification, CompletionCriterion, CriterionOutcome,
    Direction, LegKey, StructureLevel, Terminality, TerminalityAssessment,
};
use core_domain::overview_panel::{build_overview_panel, PanelInstance};
use market_analyzer::indicators::registry;
use market_analyzer::leg::LegMachine;
use market_analyzer::model::{
    assemble_cross_tf_state, assemble_timeframe_state, levels_converge, Reading, TimeframeInput,
};

fn identity(secs: u64) -> core_domain::axes::TimeframeIdentity {
    core_domain::axes::TimeframeIdentity {
        instance_id: "inst-1".into(),
        pair: "BTC-USDT".into(),
        exchange: "hyperliquid".into(),
        timeframe_secs: secs,
        timeframe_label: core_domain::duration_label(secs),
        candle_count: 500,
        last_closed_at: "2026-10-18T00:00:00Z".into(),
        available_at: "500".into(),
        spec_version: core_domain::SPEC_VERSION.into(),
        build_version: core_domain::BUILD_VERSION.into(),
    }
}

fn level(low: f64, high: f64) -> Level {
    Level::new(
        "support",
        low,
        high,
        LevelBasis::DeclaredBand {
            indicator: "structure".into(),
        },
    )
    .expect("a real band")
}

fn diagnostics() -> Diagnostics {
    Diagnostics {
        state: DiagnosticState::Coiling,
        volatility_change_pct: 0.0,
        participation_change_pct: 0.0,
        basis: DiagnosticBasis {
            bbwp_pct: 20.0,
            rvol_pct: 100.0,
        },
    }
}

fn state(
    secs: u64,
    readings: Vec<Reading>,
    levels: Vec<Level>,
    atr: Option<f64>,
    stage: Option<core_domain::axes::Stage>,
    direction_history: Vec<f64>,
) -> TimeframeState {
    assemble_timeframe_state(TimeframeInput {
        identity: identity(secs),
        readings,
        events: core_domain::event::EventCounts::default(),
        levels,
        atr,
        diagnostics: diagnostics(),
        progression_stage: stage,
        progression_progress: core_domain::leg::Progress::Value { value: 0.0 },
        target_version: 0,
        target_stable: false,
        available_at_bar: 500,
        direction_history,
        axis_signals: Default::default(),
        legs: Vec::new(),
    })
}

fn direction_reading(value: f64, complete: bool) -> Reading {
    Reading::produced(
        "d1",
        AxisId::Direction,
        AxisValue::signed(value, Domain::DirectionalBias),
        complete,
    )
}

/// V-210 — convergence is measured against the **slower** duration's ATR.
///
/// The fast duration has a huge ATR (5.0) and the slow one a tiny ATR (0.05).
/// A caller that used "some" ATR — or the fast one — would call these levels
/// converged. The published answer must be `false`.
#[test]
fn convergence_uses_the_slower_durations_atr() {
    let fast = state(
        1,
        vec![direction_reading(0.5, true)],
        vec![level(100.0, 100.9)],
        Some(5.0),
        None,
        vec![],
    );
    let slow = state(
        60,
        vec![direction_reading(0.4, true)],
        vec![level(101.0, 102.0)],
        Some(0.05),
        None,
        vec![],
    );
    let states = vec![(1u64, fast.clone()), (60u64, slow.clone())];

    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &states);
    assert!(
        !cross.cross_duration_agreement.levels_converging,
        "0.1 gap exceeds the 60-second duration's 0.05 ATR; the 1s ATR must not be used"
    );
    assert!(!cross.cross_duration_agreement.levels_converging);
    assert!(!levels_converge(
        &[
            (1, vec![level(100.0, 100.9)]),
            (60, vec![level(101.0, 102.0)])
        ],
        &BTreeMap::from([(1, Some(5.0)), (60, Some(0.05))]),
    ));

    // Raising the SLOW ATR — and nothing else — flips the answer.
    let wide = state(
        60,
        vec![direction_reading(0.4, true)],
        vec![level(101.0, 102.0)],
        Some(0.2),
        None,
        vec![],
    );
    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &[(1, fast), (60, wide)]);
    assert!(
        cross.cross_duration_agreement.levels_converging,
        "0.1 gap is inside the 60-second duration's 0.2 ATR"
    );
}

#[test]
fn convergence_needs_two_durations_with_a_published_level() {
    let one = state(
        60,
        vec![direction_reading(0.4, true)],
        vec![level(100.0, 101.0)],
        Some(1.0),
        None,
        vec![],
    );
    let bare = state(
        1,
        vec![direction_reading(0.5, true)],
        Vec::new(),
        Some(1.0),
        None,
        vec![],
    );
    assert!(!levels_converge(
        &[(1, bare.levels), (60, one.levels)],
        &BTreeMap::from([(1, Some(1.0)), (60, Some(1.0))]),
    ));
}

/// V-211 — the cross-duration consensus counts axes defined **somewhere** on the
/// ladder, and only names a weakest axis when one is actually missing.
#[test]
fn consensus_counts_axes_defined_by_any_duration() {
    let only_direction = state(
        1,
        vec![direction_reading(0.5, true)],
        vec![],
        None,
        None,
        vec![],
    );
    let only_momentum = state(
        60,
        vec![Reading::produced(
            "m1",
            AxisId::Momentum,
            AxisValue::signed(0.3, Domain::Acceleration),
            true,
        )],
        vec![],
        None,
        None,
        vec![],
    );
    let (cross, _) =
        assemble_cross_tf_state("BTC-USDT", &[(1, only_direction), (60, only_momentum)]);
    assert_eq!(cross.derived.consensus.agreeing_axes, 2);
    assert_eq!(cross.derived.consensus.of, 4);
    assert_eq!(
        cross.derived.consensus.axes,
        vec![AxisId::Direction, AxisId::Momentum]
    );
    assert_eq!(
        cross.derived.consensus.weakest_axis,
        Some(AxisId::Structure),
        "the first axis absent everywhere is the weakest"
    );

    // All four axes defined on both durations: no weak axis to name.
    let full = || {
        state(
            60,
            vec![
                direction_reading(0.4, true),
                Reading::produced(
                    "m1",
                    AxisId::Momentum,
                    AxisValue::signed(0.3, Domain::Acceleration),
                    true,
                ),
                Reading::produced(
                    "s1",
                    AxisId::Structure,
                    AxisValue::signed(0.2, Domain::NetEventBias),
                    true,
                ),
            ],
            vec![],
            None,
            Some(core_domain::axes::Stage::Early),
            vec![],
        )
    };
    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &[(1, full()), (60, full())]);
    assert_eq!(cross.derived.consensus.agreeing_axes, 4);
    assert_eq!(cross.derived.consensus.weakest_axis, None);
}

/// V-211 — the ladder is as stable as its least-stable member, and the change
/// count is a maximum, never a sum (01-13 I42).
#[test]
fn stability_is_the_least_stable_member_and_changes_do_not_sum() {
    let flicker: Vec<f64> = (0..50)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    let calm = vec![0.5; 50];
    let unstable = state(
        1,
        vec![direction_reading(0.5, true)],
        vec![],
        None,
        None,
        flicker,
    );
    let holding = state(
        60,
        vec![direction_reading(0.5, true)],
        vec![],
        None,
        None,
        calm,
    );

    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &[(1, unstable), (60, holding.clone())]);
    assert_eq!(cross.derived.stability.state, StabilityState::Unstable);
    assert_eq!(
        cross.derived.stability.changes_last_50_bars, 49,
        "one flickering member is 49 changes; summing two members would double it"
    );

    let (cross, _) = assemble_cross_tf_state("BTC-USDT", &[(60, holding.clone()), (1, holding)]);
    assert_eq!(cross.derived.stability.state, StabilityState::Holding);
    assert_eq!(cross.derived.stability.changes_last_50_bars, 0);
}

/// V-212 — the four bars are the four axes, derived from readiness coverage.
#[test]
fn market_health_bars_are_derived_from_the_four_axes() {
    let solid = state(
        60,
        vec![direction_reading(0.5, true)],
        vec![],
        None,
        None,
        vec![],
    );
    let dashed = state(
        1,
        vec![direction_reading(0.5, false)],
        vec![],
        None,
        None,
        vec![],
    );

    let with_state = core_domain::models::MarketSnapshot {
        model_state: Some(solid),
        ..Default::default()
    };
    let with_dashed = core_domain::models::MarketSnapshot {
        model_state: Some(dashed),
        ..Default::default()
    };
    let without = core_domain::models::MarketSnapshot::default();

    let instances = vec![
        PanelInstance {
            symbol: "BTC-USDT".into(),
            snapshots: vec![with_state],
            is_active: true,
            rank_score: 0.0,
        },
        PanelInstance {
            symbol: "ETH-USDT".into(),
            snapshots: vec![with_dashed],
            is_active: true,
            rank_score: 0.0,
        },
        PanelInstance {
            symbol: "SOL-USDT".into(),
            snapshots: vec![without],
            is_active: true,
            rank_score: 0.0,
        },
    ];
    let panel = build_overview_panel(&instances, &HashMap::new());
    let dims = panel
        .market_health_dims
        .expect("two instances publish a model state");
    assert_eq!(dims.active_instance_count, 2);
    assert_eq!(dims.bars.len(), 4);
    assert_eq!(
        dims.bars
            .iter()
            .map(|b| b.label.as_str())
            .collect::<Vec<_>>(),
        vec!["DIRECTION", "MOMENTUM", "STRUCTURE", "PROGRESSION"]
    );
    let direction = &dims.bars[0];
    assert_eq!(direction.contributing_instances, 1);
    assert_eq!(direction.value, 50.0);
    assert!(direction.available);
    for absent in &dims.bars[1..] {
        assert_eq!(absent.contributing_instances, 0);
        assert!(!absent.available, "an unmeasured axis is not a zero");
    }

    let no_model = vec![PanelInstance {
        symbol: "BTC-USDT".into(),
        snapshots: vec![core_domain::models::MarketSnapshot::default()],
        is_active: true,
        rank_score: 0.0,
    }];
    assert!(build_overview_panel(&no_model, &HashMap::new())
        .market_health_dims
        .is_none());
}

/// V-213 — a declared family contributes one vote, split across its members.
#[test]
fn family_members_share_one_vote() {
    let stochastic = Reading::produced(
        "stochastic",
        AxisId::Momentum,
        AxisValue::signed(0.3, Domain::Acceleration),
        true,
    );
    let williams = Reading::produced(
        "williams_r",
        AxisId::Momentum,
        AxisValue::signed(0.3, Domain::Acceleration),
        true,
    );
    let rsi = Reading::produced(
        "rsi",
        AxisId::Momentum,
        AxisValue::signed(0.3, Domain::Acceleration),
        true,
    );

    assert!(matches!(
        stochastic.aggregation(),
        core_domain::axes::AxisAggregation::Family { weight } if (weight - 0.5).abs() < 1e-12
    ));
    assert!(matches!(
        williams.aggregation(),
        core_domain::axes::AxisAggregation::Family { weight } if (weight - 0.5).abs() < 1e-12
    ));
    assert_eq!(
        rsi.aggregation(),
        core_domain::axes::AxisAggregation::Owned { weight: 1.0 }
    );
    assert!(
        (stochastic.weight() + williams.weight() - 1.0).abs() < 1e-12,
        "the family cannot outweigh an owned reading"
    );
    assert_eq!(rsi.weight(), 1.0);
}

#[test]
fn family_declarations_agree_with_the_reading_registry() {
    for (family, members) in registry::FAMILIES {
        assert!(!members.is_empty(), "family {family} has no members");
        let mut total = 0.0;
        for member in *members {
            let declared = registry::reading(member).unwrap_or_else(|| {
                panic!("{member} is declared in FAMILIES but not in the registry")
            });
            assert_eq!(
                declared.family,
                Some(*family),
                "{member} must name its family in the registry"
            );
            assert_eq!(
                registry::family_members(family).as_slice(),
                *members,
                "the family table and the reverse lookup must agree"
            );
            total += Reading::produced(
                *member,
                AxisId::Momentum,
                AxisValue::signed(0.0, Domain::Acceleration),
                true,
            )
            .weight();
        }
        assert!(
            (total - 1.0).abs() < 1e-12,
            "family {family} must contribute exactly one vote, got {total}"
        );
    }
    for declared in registry::READINGS {
        if let Some(family) = declared.family {
            assert!(
                registry::family_members(family).contains(&declared.id),
                "{} declares family {family} but is absent from FAMILIES",
                declared.id
            );
        }
    }
}

/// V-214 — `ContinuationSupported` is reachable, but only when every criterion
/// was inspected and none fired.
#[test]
fn continuation_supported_requires_every_criterion_not_fired() {
    let all = CompletionCriterion::ALL;
    let outcomes = |values: &[CriterionOutcome]| -> Vec<(CompletionCriterion, CriterionOutcome)> {
        all.iter()
            .zip(values.iter())
            .map(|(c, o)| (*c, *o))
            .collect()
    };

    assert_eq!(
        assess_terminality_from_outcomes(&outcomes(&[
            CriterionOutcome::NotFired,
            CriterionOutcome::NotFired,
            CriterionOutcome::NotFired,
        ])),
        TerminalityAssessment {
            terminality: Terminality::ContinuationSupported,
            criteria: Vec::new(),
        }
    );
    assert_eq!(
        assess_terminality_from_outcomes(&outcomes(&[
            CriterionOutcome::NotFired,
            CriterionOutcome::Unknown,
            CriterionOutcome::NotFired,
        ])),
        TerminalityAssessment::indeterminate(),
        "an uninspected criterion cannot support continuation"
    );
    assert_eq!(
        assess_terminality_from_outcomes(&outcomes(&[
            CriterionOutcome::Fired,
            CriterionOutcome::NotFired,
            CriterionOutcome::NotFired,
        ]))
        .terminality,
        Terminality::Indeterminate,
        "one fired criterion is evidence, but not yet two"
    );
    let evidence = assess_terminality_from_outcomes(&outcomes(&[
        CriterionOutcome::Fired,
        CriterionOutcome::Fired,
        CriterionOutcome::NotFired,
    ]));
    assert_eq!(evidence.terminality, Terminality::TerminationEvidence);
    assert_eq!(evidence.criteria.len(), 2);
}

#[test]
fn the_leg_machine_consumes_its_own_criteria() {
    let mut machine = LegMachine::new(Default::default(), 0.01);
    let key = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 60,
        structure_level: StructureLevel::Major,
    };
    machine.open(
        key.clone(),
        Direction::Long,
        100.0,
        10,
        10,
        Classification::Settled,
        90.0,
    );

    assert_eq!(
        machine.leg(&key).expect("open").terminality,
        TerminalityAssessment::indeterminate(),
        "nothing has been inspected yet"
    );

    machine.note_evaluated(&key, &CompletionCriterion::ALL);
    assert_eq!(
        machine.leg(&key).expect("open").terminality.terminality,
        Terminality::ContinuationSupported,
        "the machine must consume the assessment it stores"
    );

    machine.note_criterion(&key, CompletionCriterion::CharacterChange);
    assert_eq!(
        machine.leg(&key).expect("open").terminality.terminality,
        Terminality::Indeterminate,
        "one fired criterion is not two"
    );
    machine.note_criterion(&key, CompletionCriterion::ClimaxFailure);
    assert_eq!(
        machine.leg(&key).expect("open").terminality.terminality,
        Terminality::TerminationEvidence
    );
    assert_eq!(
        machine.leg(&key).expect("open").lifecycle,
        core_domain::leg::LegLifecycle::Forming,
        "I23: terminality evidence never closes the leg"
    );
}

#[test]
fn readiness_values_are_mechanical() {
    // The health bars read `Solid`; this pins that a partial input set is not
    // silently promoted (V-212 rests on this contract).
    let partial = state(
        60,
        vec![direction_reading(0.5, false)],
        vec![],
        None,
        None,
        vec![],
    );
    let direction = partial
        .axes
        .iter()
        .find(|a| a.axis == AxisId::Direction)
        .expect("all four axes are published");
    assert_eq!(direction.readiness, Readiness::Dashed);
}
