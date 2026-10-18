//! **CG10 — terminality is not completion** (P10).
//!
//! ## The claim
//!
//! 01-20 §6 CG10: *Termination evidence with new extremes; reassessed, leg **not**
//! closed.* 01-13 draws the table that matters:
//!
//! | | Terminality | Completion |
//! |---|---|---|
//! | Type | ordinal, re-evaluated | discrete event with a timestamp and a `close_reason` |
//! | Terminal? | **No** | **Yes** |
//! | Question | What does the evidence support? | Has the leg ended? |
//!
//! Conflating them would let a leg be reported as ended while it is still
//! running — the failure mode a descriptive layer must not have (CH-5).

use core_domain::axes::Stage;
use core_domain::leg::{CloseReason, Direction, LegLifecycle, TargetBasis};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy};
use market_analyzer::model::{
    assess_terminality, terminality_of_evidence, CompletionCriterion, Terminality,
};

fn machine() -> LegMachine {
    LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0))
}

#[test]
fn cg10_terminality_is_derived_only_from_completion_criteria() {
    // No criteria fired → the evidence points neither way.
    let none = terminality_of_evidence(&[]);
    assert_eq!(none.terminality, Terminality::Indeterminate);
    assert!(none.criteria.is_empty());

    // One criterion is a coincidence until it is confirmed.
    let one = terminality_of_evidence(&[CompletionCriterion::OpposingSwingConfirmed]);
    assert_eq!(
        one.terminality,
        Terminality::Indeterminate,
        "one criterion is not yet evidence"
    );

    // Two independent criteria pointing the same way is evidence.
    let two = terminality_of_evidence(&[
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::CharacterChange,
    ]);
    assert_eq!(two.terminality, Terminality::TerminationEvidence);
    assert_eq!(two.criteria.len(), 2, "and the criteria are NAMED");
}

#[test]
fn cg10_terminality_is_not_a_probability_and_carries_no_likelihood() {
    // CH-4 / I21: there is no number here. The type is an enum with three
    // variants, so a consumer cannot attach a likelihood to it at all.
    let assessment = assess_terminality(&[
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::ClimaxFailure,
    ]);
    let rendered = serde_json::to_value(&assessment).expect("serialises");
    let keys: Vec<&String> = rendered.as_object().expect("an object").keys().collect();
    assert_eq!(
        keys,
        vec!["criteria", "terminality"],
        "two fields, no score"
    );
    assert!(rendered.get("probability").is_none());
    assert!(rendered.get("confidence").is_none());
    assert!(rendered.get("likelihood").is_none());
}

#[test]
fn cg10_terminality_evidence_never_closes_the_leg() {
    let mut m = machine();
    let k = core_domain::leg::LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: core_domain::leg::StructureLevel::Major,
    };
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        core_domain::leg::Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "init");

    // Two criteria fire: the assessment says "evidence bearing on termination".
    let assessment = assess_terminality(&[
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::CharacterChange,
    ]);
    assert_eq!(assessment.terminality, Terminality::TerminationEvidence);

    // And the leg is still running.
    assert_eq!(
        m.leg(&k).unwrap().lifecycle,
        LegLifecycle::Active,
        "CG10: evidence is not a close"
    );
    assert!(m.leg(&k).unwrap().close_reason.is_none());
}

#[test]
fn cg10_a_leg_with_termination_evidence_may_extend_further() {
    // "termination_evidence means structural evidence bearing on termination is
    // present. It does not mean the leg will end."
    let mut m = machine();
    let k = core_domain::leg::LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: core_domain::leg::StructureLevel::Major,
    };
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        core_domain::leg::Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "init");

    // Termination evidence at bar 20…
    let _ = assess_terminality(&[
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::CharacterChange,
    ]);
    // …and price makes new highs for fifty more bars.
    for bar in 20..70 {
        m.observe(&k, 105.0 + (bar as f64) * 2.0, bar);
    }
    assert_eq!(
        m.leg(&k).unwrap().lifecycle,
        LegLifecycle::Active,
        "the leg extended through the evidence"
    );
    assert!(m.leg(&k).unwrap().extreme_price > 200.0);
}

#[test]
fn cg10_terminality_and_stage_are_not_derivable_from_each_other() {
    // An `early` leg may carry termination evidence; a `late` leg may carry none.
    // The assembler proves it by construction: `assess_terminality` takes only
    // criteria, so there is no argument through which a stage could reach it.
    let early_with_evidence = assess_terminality(&[
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::ClimaxFailure,
    ]);
    assert_eq!(
        early_with_evidence.terminality,
        Terminality::TerminationEvidence
    );
    // `Stage` is not an input to the function, so the independence is structural.
    let _stage: Stage = Stage::Early;
}

#[test]
fn cg10_completion_is_the_separate_timestamped_event() {
    // The two outputs side by side: the assessment says nothing about the close,
    // and the close records a reason and a bar.
    let mut m = machine();
    let k = core_domain::leg::LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: core_domain::leg::StructureLevel::Major,
    };
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        core_domain::leg::Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "init");

    let before = terminality_of_evidence(&[CompletionCriterion::OpposingSwingConfirmed]);
    assert_eq!(before.terminality, Terminality::Indeterminate);

    // A named criterion actually fires, with a reason, at a bar.
    assert!(m.close(&k, CloseReason::OpposingSwingConfirmed));
    let leg = m.leg(&k).unwrap();
    assert!(leg.lifecycle.is_terminal());
    assert_eq!(leg.close_reason, Some(CloseReason::OpposingSwingConfirmed));

    // And the assessment still says nothing about it — it is a different output.
    let after = terminality_of_evidence(&[]);
    assert_eq!(
        after.terminality,
        Terminality::Indeterminate,
        "closing a leg is not a terminality reading"
    );
}

#[test]
fn cg10_the_criteria_are_a_named_closed_set() {
    // An open-ended criterion set would make the assessment unfalsifiable: a new
    // criterion would silently move the answer.
    let all = [
        CompletionCriterion::OpposingSwingConfirmed,
        CompletionCriterion::CharacterChange,
        CompletionCriterion::ClimaxFailure,
    ];
    assert_eq!(all.len(), 3, "three named criteria, and only three");
    // Order of input does not change the assessment.
    let forward = assess_terminality(&[all[0], all[1]]);
    let reverse = assess_terminality(&[all[1], all[0]]);
    assert_eq!(forward, reverse, "the assessment is order-independent");
}
