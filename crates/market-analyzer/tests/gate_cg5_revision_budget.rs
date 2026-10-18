//! **CG5 — the revision budget is unrecoverable** (P8).
//!
//! ## The claim
//!
//! 01-20 §6 G5: *`unviable`; stability unrecoverable for that leg; no stage;
//! `progress` provisional when defined.* 01-14 §4.6 adds the reason: "a target
//! revised every few bars is not an estimate that is settling… calling it stable
//! would manufacture certainty the system does not have."
//!
//! ## The mutation this catches
//!
//! M3 — the stability boundary off-by-one — also trips this gate, because a
//! budget that counts the revision bar reaches the threshold one revision early
//! and unviables a leg that still had a converging reference. Two gates, one
//! defect, two independent failures: that is what makes a gate trustworthy.

use core_domain::leg::{Classification, Direction, LegKey, StructureLevel, TargetBasis};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy, REVISION_BUDGET};

fn key() -> LegKey {
    LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    }
}

fn referenced(m: &mut LegMachine) -> LegKey {
    let k = key();
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "init");
    k
}

#[test]
fn cg5_the_budget_is_reached_and_the_basis_becomes_unviable() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let budget = m.policy().revision_budget;

    // Revise until the budget is hit, observing between revisions so stability
    // would otherwise be granted. The budget bounds ACCEPTED revisions, so the
    // loop stops at the first refusal — which is itself the assertion.
    let mut bar = 20;
    let mut accepted = 0;
    let mut refused_at: Option<usize> = None;
    for i in 0..(budget + 4) {
        let i = i as usize;
        let ok = m.revise_target(&k, 111.0 + i as f64, bar, TargetBasis::Volatility, "churn");
        if !ok {
            refused_at = Some(i);
            break;
        }
        accepted += 1;
        for _ in 0..m.policy().stability_window {
            m.observe(&k, 105.0, bar);
            bar += 1;
        }
        bar += 1;
    }

    let leg = m.leg(&k).unwrap();
    assert_eq!(
        leg.target_revision_count, budget,
        "the budget bounds the count"
    );
    assert_eq!(leg.target_basis, TargetBasis::Unviable);
    assert!(leg.is_unviable());
    assert!(!leg.target_stable);
    assert_eq!(accepted, budget as usize, "exactly the budget was accepted");
    assert_eq!(
        refused_at,
        Some(budget as usize),
        "and the very next revision was refused"
    );
}

#[test]
fn cg5_unviable_is_permanent_across_arbitrarily_many_bars() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let budget = m.policy().revision_budget;

    let mut bar = 20;
    for i in 0..budget {
        m.revise_target(&k, 111.0 + i as f64, bar, TargetBasis::Volatility, "churn");
        for _ in 0..4 {
            m.observe(&k, 105.0, bar);
            bar += 1;
        }
        bar += 1;
    }
    assert!(m.leg(&k).unwrap().is_unviable());

    // A thousand quiet bars must not rehabilitate it.
    for b in bar..(bar + 1000) {
        m.observe(&k, 106.0, b);
    }
    assert!(
        !m.leg(&k).unwrap().target_stable,
        "L17: stability never returns"
    );
    assert_eq!(m.leg(&k).unwrap().target_basis, TargetBasis::Unviable);
    assert!(
        m.stage(&k, 106.0).is_none(),
        "and no stage is ever published again"
    );
}

#[test]
fn cg5_a_further_revision_is_refused_after_unviable() {
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let budget = m.policy().revision_budget;
    let mut bar = 20;
    for i in 0..budget {
        m.revise_target(&k, 111.0 + i as f64, bar, TargetBasis::Volatility, "churn");
        bar += 1;
    }
    let before = m.leg(&k).unwrap().target_version;
    assert!(
        !m.revise_target(&k, 999.0, bar, TargetBasis::Swing, "one more"),
        "a reference that has failed cannot be restated into a fresh estimate"
    );
    assert_eq!(m.leg(&k).unwrap().target_version, before);
}

#[test]
fn cg5_progress_remains_available_while_the_stage_does_not() {
    // §4.6 item 3: progress is published but explicitly provisional. The
    // reference failed; the movement did not.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let budget = m.policy().revision_budget;
    let mut bar = 20;
    for i in 0..budget {
        m.revise_target(&k, 111.0 + i as f64, bar, TargetBasis::Volatility, "churn");
        bar += 1;
    }
    let reading = m.observe(&k, 105.0, bar).expect("a reading");
    assert!(
        reading.progress.value().is_some(),
        "progress is still arithmetic: the reference is merely unviable"
    );
    assert_eq!(reading.stage, None, "but no stage is published from it");
    assert!(!reading.target_stable, "and the reading says so");
}

#[test]
fn cg5_a_reachable_budget_is_reachable_exactly() {
    // The budget must be reachable by legitimate churn — a gate that only passes
    // because the budget is 1000 is not testing anything.
    assert!(
        REVISION_BUDGET <= 16,
        "a budget of {REVISION_BUDGET} is not reachable inside the revision log's life"
    );
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let budget = m.policy().revision_budget as usize;
    let mut bar = 20;
    for i in 0..budget {
        assert!(
            m.revise_target(&k, 111.0 + i as f64, bar, TargetBasis::Volatility, "c"),
            "revision {i} of {budget} must be accepted"
        );
        bar += 1;
    }
    assert_eq!(m.leg(&k).unwrap().target_revision_count as usize, budget);
}

#[test]
fn cg5_an_under_budget_reference_still_stabilises_on_exactly_w_observations() {
    // CG5 shares the stability boundary with CG4, and the mutation matrix says
    // so: M3 (the revision bar counted) must break BOTH gates. This is the half
    // that lives here — a leg churning *under* the budget is still a leg whose
    // reference settles, and it settles on exactly W observations.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = referenced(&mut m);
    let w = m.policy().stability_window as usize;

    for bar in 16..(16 + w - 1) {
        m.observe(&k, 105.0, bar);
        assert!(
            !m.leg(&k).unwrap().target_stable,
            "{bar}: {w}-1 observations under the budget is still not stable"
        );
        assert!(
            !m.leg(&k).unwrap().is_unviable(),
            "and a churning-but-under-budget leg is not unviable"
        );
    }
    m.observe(&k, 105.0, 16 + w - 1);
    assert!(m.leg(&k).unwrap().target_stable);
    assert!(!m.leg(&k).unwrap().is_unviable());
}

#[test]
fn cg5_the_first_target_is_not_a_revision() {
    // The budget measures a reference that cannot settle. A reference that has
    // never been stated has not failed to settle.
    let mut m = LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0));
    let k = key();
    m.open(
        k.clone(),
        Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    assert!(m.revise_target(&k, 110.0, 15, TargetBasis::Volatility, "first"));
    assert_eq!(
        m.leg(&k).unwrap().target_revision_count,
        0,
        "stating a reference is not revising one"
    );
    assert_eq!(
        m.leg(&k).unwrap().target_version,
        0,
        "and it does not consume a version"
    );
}
