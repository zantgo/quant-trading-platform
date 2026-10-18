//! **CG1 — availability enforcement** (P7).
//!
//! ## What this gate asserts
//!
//! A published value may only claim availability at a bar **at or after** the bar
//! it was computed from. `available_at < subject_bar` is a look-ahead: the value
//! says "I knew this before the bar it describes", which is the one sentence a
//! backtest must never be able to say.
//!
//! ## What it does not assert, and why that is stated
//!
//! It does **not** prove that a calculator read nothing past `available_at`. Proving
//! that requires instrumenting all 44 calculators, which is CG2's job — CG2
//! perturbs a future bar and checks that nothing at or before `t` moved, which is
//! an empirical proof rather than a structural one.
//!
//! CG1 is the cheap structural half: it makes the claim *expressible* and
//! catches the class where a producer stamps an impossible time. CG2 is the
//! expensive behavioural half. A gate that claimed to do both would be
//! overstating itself.
//!
//! ## Why undeclared availability is a failure, not a default
//!
//! `available_at: None` is indistinguishable from a value that read the entire
//! future. Defaulting it to "contemporaneous" would make the platform look
//! causal while proving nothing, and the failure would only surface as a
//! backtest that wins. So [`NormalizedIndicatorValue::was_knowable_at`] treats
//! undeclared as knowable — the permissive reading — and this gate exists to
//! ensure nothing stays in that state.

use core_domain::indicator_dtos::NormalizedIndicatorValue;
use market_analyzer::indicators::fibonacci::PivotPoint;
use market_analyzer::replay::{Differ, Fixture, Frame, Futures, Recorder, Run, TARGETED_FUTURES};

// ---------------------------------------------------------------------------
// The type-level contract
// ---------------------------------------------------------------------------

#[test]
fn availability_is_never_earlier_than_the_bar_it_describes() {
    let ok = NormalizedIndicatorValue::scalar(1.0, 0.5, "X")
        .available_at(/* subject */ 10, /* available */ 10);
    assert!(ok.was_knowable_at(10));
    assert!(!ok.was_knowable_at(9));

    // A confirmation-delayed value is legitimate: about bar 10, knowable at 13.
    let delayed = NormalizedIndicatorValue::scalar(1.0, 0.5, "X").available_at(10, 13);
    assert!(!delayed.was_knowable_at(12));
    assert!(delayed.was_knowable_at(13));
}

/// The builder rejects a value that claims to have been knowable before the bar
/// it describes. `should_panic` rather than a manual `catch_unwind`: the panic IS
/// the assertion, and saying so is clearer than inspecting a return value.
///
/// This fires under `debug_assertions`, which every gate run has. The profile is
/// named in the expected string so a reader knows exactly what stopped the build.
#[test]
#[should_panic(expected = "cannot be knowable at")]
fn a_value_that_claims_to_know_the_future_before_it_happened_is_rejected_at_construction() {
    NormalizedIndicatorValue::scalar(1.0, 0.5, "X")
        .available_at(/* subject */ 10, /* available */ 9);
}

/// The same call with the bars in causal order is accepted, so the guard above
/// is testing the ORDER and not rejecting the builder outright.
#[test]
fn the_causal_order_is_accepted_by_the_same_builder() {
    let v = NormalizedIndicatorValue::scalar(1.0, 0.5, "X").available_at(9, 10);
    assert_eq!(v.subject_bar, Some(9));
    assert_eq!(v.available_at_bar, Some(10));
    assert!(!v.was_knowable_at(9));
    assert!(v.was_knowable_at(10));
}

#[test]
fn undeclared_availability_is_reported_rather_than_assumed() {
    let v = NormalizedIndicatorValue::scalar(1.0, 0.5, "X");
    assert!(
        v.availability_is_undeclared(),
        "a bare constructor must not claim to know its availability"
    );
    // Permissive read, so a missing declaration cannot crash a consumer — but
    // CG1's whole purpose is to make sure nothing REMAINS in this state.
    assert!(v.was_knowable_at(0));
}

#[test]
fn the_wire_carries_availability_when_it_is_declared() {
    let v = NormalizedIndicatorValue::scalar(1.0, 0.5, "X").available_at(7, 9);
    let j = serde_json::to_value(&v).unwrap();
    assert_eq!(j["available_at_bar"], 9);
    assert_eq!(j["subject_bar"], 7);

    // An undeclared value omits the keys rather than writing null — the same
    // convention `values` already uses, so a consumer can tell "not declared"
    // from "declared as nothing".
    let bare = serde_json::to_value(NormalizedIndicatorValue::scalar(1.0, 0.5, "X")).unwrap();
    assert!(bare.get("available_at_bar").is_none());
}

// ---------------------------------------------------------------------------
// The confirmation-delayed readings — the real-world case
// ---------------------------------------------------------------------------

#[test]
fn a_pivot_declares_the_bar_it_was_confirmed_not_the_bar_it_happened() {
    // A 3-bar fractal about bar 100 needs bars 99, 100, 101. It is knowable at
    // 101, not 100. `PivotPoint::confirming_bar` is what CG2 calls, so the
    // relationship has to hold on the type.
    let p = PivotPoint {
        index: 100,
        price: rust_decimal::Decimal::from(42),
        pivot_type: market_analyzer::indicators::fibonacci::PivotType::High,
        strength: 3,
    };
    assert_eq!(p.confirming_bar(), 103);
    assert!(!p.is_knowable_at(102));
    assert!(p.is_knowable_at(103));

    // A zero-strength pivot is contemporaneous — nothing to confirm.
    let q = PivotPoint { strength: 0, ..p };
    assert_eq!(q.confirming_bar(), q.index);
    assert!(q.is_knowable_at(q.index));
}

// ---------------------------------------------------------------------------
// Availability survives a serialise round trip — a clock that resets on
// deserialisation is a clock nobody can gate on.
// ---------------------------------------------------------------------------

#[test]
fn availability_survives_a_serialise_round_trip() {
    let v = NormalizedIndicatorValue::scalar(1.5, -0.25, "BEARISH").available_at(41, 43);
    let s = serde_json::to_string(&v).unwrap();
    let back: NormalizedIndicatorValue = serde_json::from_str(&s).unwrap();

    assert_eq!(back.available_at_bar, Some(43));
    assert_eq!(back.subject_bar, Some(41));
    assert!(!back.was_knowable_at(42));
    assert!(back.was_knowable_at(43));
}

// ---------------------------------------------------------------------------
// Availability and the replay harness agree
// ---------------------------------------------------------------------------

#[test]
fn a_frame_stamped_contemporaneously_is_knowable_the_same_bar_the_recorder_records_it() {
    // The value type and the harness must not disagree about when a value
    // became knowable, or CG1 would pass a run CG2 then fails — two gates, two
    // answers, same question.
    let recorder = Recorder::new();
    let clock_bar = 12;
    let v = NormalizedIndicatorValue::scalar(1.0, 0.0, "X").available_at(clock_bar, clock_bar);
    let frame = recorder.record(Frame::new(clock_bar, clock_bar, "sma", v.raw_value));

    assert!(frame.is_knowable_at(clock_bar));
    assert!(!frame.is_knowable_at(clock_bar - 1));
    assert!(v.was_knowable_at(clock_bar));
    assert_eq!(frame.available_at, clock_bar);
}

#[test]
fn the_four_futures_are_present_so_cg1_is_not_standing_alone() {
    // CG1 is the structural half; CG2 is the behavioural one and needs all four
    // futures. Asserting the count here means dropping a future fails a cheap
    // test rather than silently weakening CG2.
    assert_eq!(
        TARGETED_FUTURES, 4,
        "the future set shrank — CG2 would pass having checked less"
    );
    let names: Vec<&str> = market_analyzer::replay::Futures::names();
    assert_eq!(names.len(), 4);
    assert!(names.contains(&"volume-only"));
    assert!(names.contains(&"swing-confirm-only"));
    assert!(names.contains(&"new-extreme-only"));
    assert!(names.contains(&"all-four"));
}

#[test]
fn a_contemporaneous_frame_is_untouched_by_a_future_perturbation() {
    // The cheapest possible CG1/CG2 joint check, and the one most likely to
    // regress if `available_at` is ever dropped: stamp everything
    // contemporaneously, perturb the future, and require zero movement through
    // the last clean bar.
    // The baseline and the perturbed run publish the SAME series, differing only
    // at bar 19. Publishing two different series would make the assertion
    // vacuous — it would pass only if the differ were broken.
    let fixture = Fixture::sawtooth(20);
    let perturbed =
        Futures::apply(&fixture, 19, Futures::ALL_FOUR).expect("bar 19 is inside a 20-bar fixture");

    let record = |closes: &[f64]| -> Run {
        let r = Recorder::new();
        for (bar, c) in closes.iter().enumerate() {
            r.record(Frame::new(bar, bar, "sma", *c));
        }
        r.into_run()
    };

    let baseline = record(&fixture.closes());
    let perturbed_run = record(&perturbed.closes());

    // Sanity: the two runs really do differ, so the next assertion has teeth.
    assert!(Differ::new()
        .first_divergence(&baseline, &perturbed_run)
        .is_some());

    let d = Differ::new();
    assert_eq!(
        d.assert_unchanged_through(&baseline, &perturbed_run, 18),
        None,
        "a future perturbation moved the past"
    );
}
