//! **CG7 — leg identity and nesting** (P8).
//!
//! ## The claim
//!
//! 01-20 §6 G7: *major/minor records distinct; scale policy selects; identity ≠
//! relevance.* 01-14 §2.5 gives the reason nesting exists: "a minor upswing
//! inside a larger downswing both have valid origins, valid targets and valid
//! progress values. Nothing in the arithmetic distinguishes them."
//!
//! ## What "identity ≠ relevance" means here
//!
//! A leg's identity is `(instrument, timeframe_secs, structure_level)`. The
//! *relevance* is which one a consumer asked about. If a lookup silently fell
//! back to "the most interesting leg", a reading about a minor pullback could be
//! answered with the major trend — unfalsifiable, because both are valid.
//!
//! ## The mutation
//!
//! **M7 — byte-budget eviction reaching a Tier D record** — is caught here and by
//! CG9: a record that can be evicted has no identity to preserve, and a leg
//! missing from one snapshot and present in the next is exactly that.

use core_domain::leg::{
    Classification, Direction, LegKey, LegLifecycle, StructureLevel, TargetBasis,
};
use market_analyzer::leg::{noise_floor_from_range, LegMachine, LegPolicy, LEG_BOUND_DESCRIPTION};

fn key(instrument: &str, secs: u64, level: StructureLevel) -> LegKey {
    LegKey {
        instrument: instrument.into(),
        timeframe_secs: secs,
        structure_level: level,
    }
}

fn machine() -> LegMachine {
    LegMachine::new(LegPolicy::default(), noise_floor_from_range(50.0))
}

fn open_at(m: &mut LegMachine, k: &LegKey, origin: f64, direction: Direction, bar: usize) {
    m.open(
        k.clone(),
        direction,
        origin,
        bar,
        bar + 3,
        Classification::Settled,
        90.0,
    );
    m.confirm_origin(k, bar + 4, origin * 0.98);
    m.revise_target(k, origin * 1.10, bar + 5, TargetBasis::Volatility, "init");
}

#[test]
fn cg7_a_major_and_a_minor_leg_are_distinct_records() {
    let mut m = machine();
    let major = key("BTC-USDT", 300, StructureLevel::Major);
    let minor = key("BTC-USDT", 300, StructureLevel::Minor);
    open_at(&mut m, &major, 100.0, Direction::Long, 10);
    open_at(&mut m, &minor, 104.0, Direction::Short, 12);

    assert_ne!(
        m.leg(&major).unwrap().leg_id,
        m.leg(&minor).unwrap().leg_id,
        "nesting is not a label on one record; it is two records"
    );
    assert_eq!(m.legs().len(), 2);
    assert_eq!(m.leg(&major).unwrap().origin_price, 100.0);
    assert_eq!(m.leg(&minor).unwrap().origin_price, 104.0);
    assert_eq!(m.leg(&major).unwrap().direction, Direction::Long);
    assert_eq!(m.leg(&minor).unwrap().direction, Direction::Short);
}

#[test]
fn cg7_a_leg_never_spans_durations() {
    let mut m = machine();
    let five = key("BTC-USDT", 300, StructureLevel::Major);
    let sixty = key("BTC-USDT", 60, StructureLevel::Major);
    open_at(&mut m, &five, 100.0, Direction::Long, 10);
    open_at(&mut m, &sixty, 100.0, Direction::Long, 10);
    assert_eq!(
        m.legs().len(),
        2,
        "the same instrument on two durations is two legs"
    );
    assert_ne!(m.leg(&five).unwrap().leg_id, m.leg(&sixty).unwrap().leg_id);
}

#[test]
fn cg7_a_leg_never_spans_instruments() {
    let mut m = machine();
    let btc = key("BTC-USDT", 300, StructureLevel::Major);
    let eth = key("ETH-USDC", 300, StructureLevel::Major);
    open_at(&mut m, &btc, 100.0, Direction::Long, 10);
    open_at(&mut m, &eth, 100.0, Direction::Long, 10);
    assert_eq!(m.legs().len(), 2);
    // Identical prices and identical bars still yield different records.
    assert_ne!(m.leg(&btc).unwrap().leg_id, m.leg(&eth).unwrap().leg_id);
}

#[test]
fn cg7_a_new_origin_at_the_same_key_supersedes_rather_than_accumulating() {
    // L8: one current leg per triple; a new leg REPLACES, the old one is
    // discarded (Tier D, O(1)) — not archived alongside.
    let mut m = machine();
    let k = key("BTC-USDT", 300, StructureLevel::Major);
    open_at(&mut m, &k, 100.0, Direction::Long, 10);
    let first = m.leg(&k).unwrap().leg_id.clone();

    open_at(&mut m, &k, 120.0, Direction::Short, 40);
    assert_eq!(m.legs().len(), 1, "the old record was replaced, not kept");
    assert_ne!(m.leg(&k).unwrap().leg_id, first);
    assert_eq!(m.leg(&k).unwrap().origin_price, 120.0);
}

#[test]
fn cg7_a_lookup_never_falls_back_to_another_leg() {
    // The unfalsifiable case: a question about a leg that does not exist must be
    // answered with absence, never with the nearest record that does.
    let mut m = machine();
    let major = key("BTC-USDT", 300, StructureLevel::Major);
    open_at(&mut m, &major, 100.0, Direction::Long, 10);

    let absent_minor = key("BTC-USDT", 300, StructureLevel::Minor);
    let absent_duration = key("BTC-USDT", 3600, StructureLevel::Major);
    let absent_instrument = key("ETH-USDC", 300, StructureLevel::Major);

    for absent in [absent_minor, absent_duration, absent_instrument] {
        assert!(
            m.leg(&absent).is_none(),
            "{} must not resolve",
            absent.instrument
        );
        assert!(m.progress(&absent, 100.0).is_undefined());
        assert!(m.stage(&absent, 100.0).is_none());
    }
}

#[test]
fn cg7_relevance_is_chosen_by_the_level_not_by_size() {
    // A minor leg can have the larger move; the level is the classification, not
    // a ranking of importance. Reading the "biggest" leg would be relevance
    // leaking into identity.
    let mut m = machine();
    let major = key("BTC-USDT", 300, StructureLevel::Major);
    let minor = key("BTC-USDT", 300, StructureLevel::Minor);
    open_at(&mut m, &major, 100.0, Direction::Long, 10);
    open_at(&mut m, &minor, 108.0, Direction::Long, 12);

    assert_eq!(
        m.leg(&major).unwrap().structure_level,
        StructureLevel::Major
    );
    assert_eq!(
        m.leg(&minor).unwrap().structure_level,
        StructureLevel::Minor
    );
    // Both are addressable, both report, neither is privileged by the lookup.
    for k in [&major, &minor] {
        let reading = m.observe(k, 105.0, 20).expect("a reading");
        assert!(reading.progress.value().is_some());
        assert_eq!(reading.structure_level, k.structure_level, "L11");
    }
}

#[test]
fn cg7_the_level_set_is_fixed_and_small() {
    // L12 + L27: an unbounded level set would make Tier D unbounded.
    assert_eq!(StructureLevel::ALL.len(), 2);
    assert!(
        LEG_BOUND_DESCRIPTION.contains("no byte-budget eviction path"),
        "the bound is stated where the code lives, so it cannot be quietly relaxed"
    );
    // And every leg carries the policy version it was admitted under.
    let mut m = machine();
    let k = key("BTC-USDT", 300, StructureLevel::Major);
    open_at(&mut m, &k, 100.0, Direction::Long, 10);
    assert_eq!(
        m.leg(&k).unwrap().swing_policy_version,
        m.policy().swing_policy_version,
        "L27: legs under different policies are not comparable, so the version is stored"
    );
}

#[test]
fn cg7_there_is_no_code_path_that_can_evict_a_leg() {
    // **M7** lives here. A byte-budget eviction reaching a Tier D record is
    // caught by this gate (and by CG9, which P9 adds). It is checked against
    // the module's own source rather than by calling an eviction function and
    // asserting it did nothing — because the defect being prevented is the
    // *existence* of that function, and a test that calls it would have to
    // exist first.
    let source = include_str!("../src/leg/mod.rs");
    // Strip comments so prose about eviction is not mistaken for code doing it.
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "evict_to",
        "truncate(",
        "legs.truncate",
        "remove(0)",
        "pop_front",
        "shrink_to",
    ] {
        assert!(
            !code.contains(forbidden),
            "the leg machine contains `{forbidden}`: Tier D records are never \
             evicted by the byte budget (01-16 T2, L24)"
        );
    }
    // The one removal that IS allowed is the documented replace-at-same-key.
    assert!(
        code.contains("self.legs.remove(existing)"),
        "L8's replacement path must still be there — the gate above forbids \
         removals in general, so this pins the one legitimate one"
    );
}

#[test]
fn cg7_a_closed_leg_is_not_resurrected_by_a_new_observation() {
    let mut m = machine();
    let k = key("BTC-USDT", 300, StructureLevel::Major);
    open_at(&mut m, &k, 100.0, Direction::Long, 10);
    assert!(m.close(&k, core_domain::leg::CloseReason::InvalidationTouched));
    assert_eq!(m.leg(&k).unwrap().lifecycle, LegLifecycle::Invalidated);
    assert_eq!(
        m.leg(&k).unwrap().close_reason.unwrap().label(),
        "invalidation_touched"
    );

    // A closed leg keeps reporting its final reading, and cannot be reopened.
    let reading = m
        .observe(&k, 90.0, 30)
        .expect("the final reading is retained");
    assert_eq!(reading.lifecycle, LegLifecycle::Invalidated);
    assert!(!m.revise_target(&k, 120.0, 31, TargetBasis::Volatility, "x"));
    assert!(!m.revise_origin(&k, 95.0, 31));
    assert!(!m.close(&k, core_domain::leg::CloseReason::Superseded));
}
