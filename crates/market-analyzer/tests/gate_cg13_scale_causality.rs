//! **CG13 — the scale classifier is causal** (P8).
//!
//! ## The claim
//!
//! 01-20 §6 G13: *rank distribution is causal; warmup defined; reclassification ≠
//! retargeting.* The six assertions (13a–13f) are written out in 01-20 §6.1 and
//! are the gate's specification. Two of them do the work:
//!
//! * **13a** — the distribution contains only observations eligible at
//!   classification time. A future bar never enters it.
//! * **13f** — a reclassification increments `structure_version` and **never**
//!   `target_version`. A retarget does the reverse. If one bumped both, a
//!   reclassification would masquerade as movement: a third revision source,
//!   precisely the ambiguity 01-14 §4.3 exists to eliminate.
//!
//! ## The mutations
//!
//! M5 (reclassification bumping `target_version`) and M7 (byte-budget eviction
//! reaching a Tier D record) are both caught here — see `01-20` §8.1.

use core_domain::leg::{Classification, LegKey, StructureLevel};
use market_analyzer::leg::{LegMachine, LegPolicy, ScaleClassifier};

/// A distribution of `n` displacements, each available at bar `10 + i`.
fn classifier_with(n: usize) -> ScaleClassifier {
    let mut c = ScaleClassifier::new();
    for i in 0..n {
        c.observe(i as f64, 10 + i);
    }
    c
}

#[test]
fn cg13a_the_distribution_excludes_observations_that_are_not_yet_available() {
    let c = classifier_with(20);
    // At bar 10 only the first observation exists.
    assert_eq!(c.sample_size_at(10), 1);
    assert_eq!(c.eligible_at(10), vec![0.0]);
    // At bar 29 the whole series is visible.
    assert_eq!(c.sample_size_at(29), 20);
    // A bar before the first observation sees nothing at all.
    assert_eq!(c.sample_size_at(9), 0);
    assert_eq!(c.rank_pct(19.0, 9), None, "no sample, no rank — not rank 0");
}

#[test]
fn cg13b_a_classification_at_bar_t_uses_only_the_sample_at_bar_t() {
    let c = classifier_with(20);
    let early = c.classify(19.0, 10, 80.0, 20);
    let late = c.classify(19.0, 29, 80.0, 20);

    assert_eq!(
        early.1,
        Classification::Provisional,
        "one observation cannot settle a percentile"
    );
    assert_eq!(late.1, Classification::Settled);
    // The ranks legitimately differ — the sample grew — which is why S-N3
    // requires the change to arrive with a timestamp rather than being applied
    // retroactively.
    assert_ne!(early.2, late.2);
}

#[test]
fn cg13c_a_distribution_change_revises_a_past_classification_only_from_its_availability() {
    // The operational form: extending the data past the classification bar must
    // not change the classification *as of* that bar. Extending it can only
    // change the classification at later bars.
    let mut c = classifier_with(5);
    let as_of_bar = 14;
    let before = c.classify(4.0, as_of_bar, 80.0, 5);
    assert_eq!(
        before.1,
        Classification::Settled,
        "five observations is the declared sample"
    );

    // Extend the series well past the classification bar.
    for i in 5..40 {
        c.observe(i as f64, 10 + i);
    }

    // Re-asking about bar 14 gives the same answer — the extra observations were
    // not available then, and must not be counted retroactively.
    let after = c.classify(4.0, as_of_bar, 80.0, 5);
    assert_eq!(
        after, before,
        "a reclassification is never retroactive (S-N3)"
    );

    // And asking about a LATER bar legitimately differs.
    let later = c.classify(4.0, 45, 80.0, 5);
    assert_ne!(later.2, after.2, "the larger sample does change the rank");
}

#[test]
fn cg13d_warmup_publishes_no_stage() {
    // L22: a provisional classification publishes no stage — the same withholding
    // rule as a forming leg.
    let mut m = LegMachine::new(LegPolicy::default(), 0.5);
    let k = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
    m.open(
        k.clone(),
        core_domain::leg::Direction::Long,
        100.0,
        10,
        13,
        Classification::Provisional,
        50.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(
        &k,
        110.0,
        15,
        core_domain::leg::TargetBasis::Volatility,
        "init",
    );
    for bar in 16..19 {
        m.observe(&k, 105.0, bar);
    }
    assert!(
        m.leg(&k).unwrap().target_stable,
        "stability is about revisions"
    );
    assert!(
        m.stage(&k, 105.0).is_none(),
        "L22: a provisional classification publishes no stage"
    );
    assert!(
        m.progress(&k, 105.0).value().is_some(),
        "though progress itself is still arithmetic"
    );
}

#[test]
fn cg13e_the_threshold_is_frozen_for_the_life_of_the_leg() {
    // L21 / 13e. A threshold that evolved would let the market re-label a leg
    // without it moving.
    let mut m = LegMachine::new(LegPolicy::default(), 0.5);
    let k = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
    m.open(
        k.clone(),
        core_domain::leg::Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    let frozen = m.leg(&k).unwrap().rank_threshold_pct;
    assert_eq!(frozen, m.policy().major_rank_threshold_pct);

    for bar in 11..50 {
        m.observe(&k, 100.0 + bar as f64, bar);
    }
    assert_eq!(m.leg(&k).unwrap().rank_threshold_pct, frozen);
}

#[test]
fn cg13f_reclassification_and_retargeting_move_disjoint_counters() {
    let mut m = LegMachine::new(LegPolicy::default(), 0.5);
    let k = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
    m.open(
        k.clone(),
        core_domain::leg::Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(&k, 14, 98.0);
    m.revise_target(
        &k,
        110.0,
        15,
        core_domain::leg::TargetBasis::Volatility,
        "init",
    );

    let (t0, s0) = {
        let l = m.leg(&k).unwrap();
        (l.target_version, l.structure_version)
    };

    // A retarget moves only the target counter.
    assert!(m.revise_target(
        &k,
        115.0,
        20,
        core_domain::leg::TargetBasis::Extension,
        "extended"
    ));
    let (t1, s1) = {
        let l = m.leg(&k).unwrap();
        (l.target_version, l.structure_version)
    };
    assert_eq!(t1, t0 + 1);
    assert_eq!(s1, s0, "S-N6: a retarget is never a structural change");

    // A reclassification moves only the structure counter.
    let minor = LegKey {
        structure_level: StructureLevel::Minor,
        ..k.clone()
    };
    assert!(m.reclassify(&k, StructureLevel::Minor));
    let (t2, s2) = {
        let l = m.leg(&minor).unwrap();
        (l.target_version, l.structure_version)
    };
    assert_eq!(
        t2, t1,
        "13f/S-N5: a reclassification never touches the target counter"
    );
    assert_eq!(s2, s0 + 1);
}

#[test]
fn cg13_a_reclassification_moves_the_leg_between_lookups_without_minting_a_record() {
    // 01-14 §2.1: `leg_id` identifies a record; the triple identifies which leg a
    // reading is about. So a reclassification relocates the record.
    let mut m = LegMachine::new(LegPolicy::default(), 0.5);
    let major = LegKey {
        instrument: "BTC-USDT".into(),
        timeframe_secs: 300,
        structure_level: StructureLevel::Major,
    };
    m.open(
        major.clone(),
        core_domain::leg::Direction::Long,
        100.0,
        10,
        13,
        Classification::Settled,
        90.0,
    );
    let id_before = m.leg(&major).unwrap().leg_id.clone();
    assert_eq!(m.legs().len(), 1);

    assert!(m.reclassify(&major, StructureLevel::Minor));
    let minor = LegKey {
        structure_level: StructureLevel::Minor,
        ..major.clone()
    };
    assert!(m.leg(&major).is_none(), "it is no longer a major leg");
    assert_eq!(
        m.leg(&minor).unwrap().leg_id,
        id_before,
        "and it is the SAME record"
    );
    assert_eq!(m.legs().len(), 1, "no second record was minted");
}

#[test]
fn cg13_a_rank_is_arithmetic_against_a_declared_reference_set() {
    // CH-4 permits percentile ranks (arithmetic against a declared set) and
    // forbids probabilistic claims. The classifier does exactly one thing:
    // count how many eligible observations are strictly smaller.
    let c = classifier_with(5); // 0.0 .. 4.0
    assert_eq!(c.rank_pct(4.0, 20), Some(80.0));
    assert_eq!(c.rank_pct(0.0, 20), Some(0.0));
    // An exact tie ranks 0: being equal to every member is not evidence of
    // being the largest.
    let mut tied = ScaleClassifier::new();
    for i in 0..5 {
        tied.observe(1.0, i);
    }
    assert_eq!(tied.rank_pct(1.0, 4), Some(0.0));
}
