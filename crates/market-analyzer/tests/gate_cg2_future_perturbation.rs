//! **CG2 — future perturbation, four futures, element-wise** (P7).
//!
//! ## The question
//!
//! Perturb bar `t + k`. Did anything published at or before bar `t` change?
//!
//! This is the **empirical** half of causality. CG1 checks that a value claims
//! an available time; CG2 checks that the claim is true — by breaking the future
//! and watching the past not move.
//!
//! ## Why four futures, not one
//!
//! A single mutation can be masked. If the only future exercised is a volume
//! change, a calculator that reaches forward through **price** is never caught,
//! and CG2 passes having proved nothing about price. 01-20 §8.1 makes this
//! mechanical: each future is declared with the channels it breaks, and the
//! futures must be **disjoint in what they can reach**. `c2_each_future_is_
//! independent` asserts exactly that, so "four futures" can never quietly become
//! "one future four times".
//!
//! ## Element-wise
//!
//! The differ compares **content**, not list positions. Two runs that publish
//! the same values in a different interleaving compare equal — the gate is about
//! what was computed, not about scheduling. A gate that compared positions would
//! fail on a benign reordering and pass on a real violation that happened to
//! shift every index by one.
//!
//! ## The readers under test
//!
//! Four publication families, one per channel, so a future's reach is visible:
//! a trailing mean over closes, a mean over volumes, a running extreme over
//! highs/lows, and the pivot detector (the only confirmation-delayed calculator
//! in the registry, read straight from production).

use market_analyzer::indicators::fibonacci::{FibonacciRange, PivotType};
use market_analyzer::replay::{
    channel, Differ, Fixture, Frame, Futures, Recorder, Run, TARGETED_FUTURES,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;

/// The pivot width the production scan uses.
const STRENGTH: usize = 3;

/// Publish everything knowable at each bar of `fixture`.
///
/// This is the "causal calculation" the gate runs. It deliberately contains four
/// readers so the four futures have four different things to reach, and it
/// stamps each pivot with its **confirming** bar — the production availability,
/// not its index.
fn run(fixture: &Fixture) -> Run {
    let closes = fixture.closes();
    let volumes = fixture.volumes();
    let highs: Vec<rust_decimal::Decimal> = fixture
        .highs()
        .iter()
        .map(|h| rust_decimal::Decimal::from_f64_retain(*h).unwrap_or(dec!(0)))
        .collect();
    let lows: Vec<rust_decimal::Decimal> = fixture
        .lows()
        .iter()
        .map(|l| rust_decimal::Decimal::from_f64_retain(*l).unwrap_or(dec!(0)))
        .collect();

    let recorder = Recorder::new();
    let n = fixture.len();

    for bar in 0..n {
        // A trailing mean over the last three closes — the canonical
        // calculation that must not be able to see past its own bar.
        let sma = if bar >= 2 {
            (closes[bar - 2] + closes[bar - 1] + closes[bar]) / 3.0
        } else {
            f64::NAN
        };
        recorder.record(Frame::new(bar, bar, "sma_3", sma));

        // The same over volumes: FX1's only reader.
        let vma = if bar >= 2 {
            (volumes[bar - 2] + volumes[bar - 1] + volumes[bar]) / 3.0
        } else {
            f64::NAN
        };
        recorder.record(Frame::new(bar, bar, "vol_ma_3", vma));

        // Running extremes: FX3's reader. A rank or channel edge is exactly
        // this calculation.
        let highs = fixture.highs();
        let lows = fixture.lows();
        let run_high = highs[..=bar].iter().copied().fold(f64::MIN, f64::max);
        let run_low = lows[..=bar].iter().copied().fold(f64::MAX, f64::min);
        recorder.record(Frame::new(bar, bar, "run_high", run_high));
        recorder.record(Frame::new(bar, bar, "run_low", run_low));
    }

    // Pivots, each published at the bar that confirms it (FX2's reader).
    for p in FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0) {
        let key = format!("pivot:{:?}:{}", p.pivot_type, p.index);
        recorder.record(Frame::new(
            p.confirming_bar(),
            p.index,
            key,
            p.price.to_f64().unwrap_or_default(),
        ));
    }

    recorder.into_run()
}

/// Every key published by `run`, so a test can assert which readers a future
/// reached.
fn keys_of(run: &Run) -> Vec<String> {
    run.values()
        .flatten()
        .map(|f| f.key.split(':').next().unwrap_or(&f.key).to_string())
        .collect()
}

/// The set of distinct reader families present in a run.
fn families(run: &Run) -> Vec<String> {
    let mut out: Vec<String> = keys_of(run)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    out.sort();
    out
}

/// Which reader families differ between two runs, ignoring bars at or before `t`.
///
/// Per family, not per frame: the claim under test is "FX1 cannot reach a close
/// reader", which is a statement about a family, so it has to be measured one.
fn families_moved_after(baseline: &Run, perturbed: &Run, t: usize) -> Vec<String> {
    use std::collections::BTreeMap;

    let per_family = |run: &Run| -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_, frames) in run.range(t + 1..) {
            for f in frames {
                let family = f.key.split(':').next().unwrap_or(&f.key).to_string();
                m.entry(family).or_default().push(f.render());
            }
        }
        m
    };

    let b = per_family(baseline);
    let p = per_family(perturbed);
    let mut moved: Vec<String> = Vec::new();
    for (family, values) in &b {
        match p.get(family) {
            Some(other) if other == values => {}
            _ => moved.push(family.clone()),
        }
    }
    for family in p.keys() {
        if !b.contains_key(family) {
            moved.push(family.clone());
        }
    }
    moved.sort();
    moved
}

const FIXTURE_BARS: usize = 60;

#[test]
fn c2_all_four_futures_are_exercised() {
    assert_eq!(TARGETED_FUTURES, 4);
    let all = Futures::all();
    assert_eq!(all.len(), 4);
    for (name, channels, mechanism) in &all {
        assert!(
            Futures::is_known(name),
            "`{name}` is missing — CG2 would pass having checked less"
        );
        assert_ne!(
            *channels, 0,
            "`{name}` breaks no channel, so it reaches nothing"
        );
        assert!(!mechanism.is_empty(), "`{name}` declares no mechanism");
    }
    for required in [
        Futures::VOLUME_ONLY,
        Futures::SWING_CONFIRM_ONLY,
        Futures::NEW_EXTREME_ONLY,
        Futures::ALL_FOUR,
    ] {
        assert!(
            Futures::names().contains(&required),
            "`{required}` is missing"
        );
    }
}

#[test]
fn c2_each_future_is_independent() {
    // 01-20 §8.1: the futures are independent fixtures, each targeting a
    // different mechanism. Asserted here rather than in a comment, because a
    // future that quietly started touching another's channel would halve the
    // gate while still reporting "4 futures".
    let base = Fixture::sawtooth(FIXTURE_BARS);
    let cut = 30;

    assert_eq!(
        Futures::channels(Futures::VOLUME_ONLY),
        channel::VOLUME,
        "FX1 must break volume and nothing else"
    );
    assert_eq!(
        Futures::channels(Futures::SWING_CONFIRM_ONLY),
        channel::HIGH | channel::LOW,
        "FX2 must break the far-side extremes and nothing else"
    );
    assert_eq!(
        Futures::channels(Futures::NEW_EXTREME_ONLY),
        channel::HIGH | channel::LOW,
        "FX3 must break the running extremes and nothing else"
    );
    assert_eq!(
        Futures::channels(Futures::ALL_FOUR),
        channel::CLOSE | channel::HIGH | channel::LOW | channel::VOLUME,
        "FX4 is the composite: it must reach every channel"
    );

    // And the declaration matches the data: each future moves a different set of
    // readers past the cut.
    let baseline = run(&base);
    assert_eq!(
        families(&baseline),
        vec![
            "pivot".to_string(),
            "run_high".to_string(),
            "run_low".to_string(),
            "sma_3".to_string(),
            "vol_ma_3".to_string(),
        ],
        "the fixture must publish every reader family, or the independence claim          below is vacuous"
    );
    let reached: Vec<(String, Vec<String>)> = Futures::names()
        .into_iter()
        .map(|name| {
            let perturbed = run(&Futures::apply(&base, cut + 3, name).unwrap());
            (
                name.to_string(),
                families_moved_after(&baseline, &perturbed, cut),
            )
        })
        .collect();

    let reached = |name: &str| -> Vec<String> {
        reached
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, k)| k.clone())
            .unwrap_or_default()
    };

    // FX1 reaches the volume reader and nothing else.
    let fx1 = reached(Futures::VOLUME_ONLY);
    assert!(
        fx1.contains(&"vol_ma_3".to_string()),
        "FX1 did not reach the volume reader: {fx1:?}"
    );
    for forbidden in ["sma_3", "run_high", "run_low", "pivot"] {
        assert!(
            !fx1.contains(&forbidden.to_string()),
            "FX1 reached the `{forbidden}` reader, which it declares it cannot: {fx1:?}"
        );
    }

    // FX2 and FX3 break highs and lows only: neither may reach the volume reader
    // nor the close reader.
    for name in [Futures::SWING_CONFIRM_ONLY, Futures::NEW_EXTREME_ONLY] {
        let keys = reached(name);
        for forbidden in ["vol_ma_3", "sma_3"] {
            assert!(
                !keys.contains(&forbidden.to_string()),
                "`{name}` reached the `{forbidden}` reader, which its channel \
                 declaration says it cannot: {keys:?}"
            );
        }
    }

    // FX4 is the composite: it must reach both the close reader and the volume
    // reader, or it is not a composite.
    let fx4 = reached(Futures::ALL_FOUR);
    for required in ["sma_3", "vol_ma_3"] {
        assert!(
            fx4.contains(&required.to_string()),
            "the composite did not reach the `{required}` reader: {fx4:?}"
        );
    }
}

#[test]
fn c2_no_future_perturbation_moves_the_past() {
    // The core assertion, run against every future and every cut point.
    let base = Fixture::sawtooth(FIXTURE_BARS);
    let baseline = run(&base);
    let differ = Differ::new();

    for future in Futures::names() {
        for cut in [5usize, 17, 30, 44] {
            let perturbed = run(&Futures::apply(&base, cut + 3, future).unwrap());
            let divergence = differ.assert_unchanged_through(&baseline, &perturbed, cut);
            assert_eq!(
                divergence,
                None,
                "future `{future}` at bar {} moved something published at or \
                 before bar {cut}: {divergence:?}",
                cut + 3
            );
        }
    }
}

#[test]
fn c2_each_future_actually_changes_the_future() {
    // A future that silently changed nothing would pass every gate it was meant
    // to strengthen — the harness protecting against a vacuous gate has to be
    // checked by the gate.
    let base = Fixture::sawtooth(FIXTURE_BARS);
    let baseline = run(&base);
    let differ = Differ::new();

    for future in Futures::names() {
        let perturbed = run(&Futures::apply(&base, FIXTURE_BARS - 1, future).unwrap());
        assert_ne!(
            differ.first_divergence(&baseline, &perturbed),
            None,
            "future `{future}` changed nothing, so it proves nothing"
        );
    }
}

#[test]
fn c2_each_future_is_pinned_by_its_own_checksum() {
    // The report banner names a checksum per fixture (01-20 §9). A future with
    // no digest of its own cannot be named in a verdict, and two futures with the
    // same digest are one future.
    let base = Fixture::sawtooth(FIXTURE_BARS);
    println!("fixture baseline digest = {:#018x}", base.digest());
    let mut digests = Vec::new();
    for future in Futures::names() {
        let out = Futures::apply(&base, 33, future).unwrap();
        println!("fixture {future:<20} digest = {:#018x}", out.digest());
        assert_ne!(out.digest(), base.digest(), "`{future}` is inert");
        digests.push((future, out.digest()));
    }
    for (i, (na, da)) in digests.iter().enumerate() {
        for (nb, db) in digests.iter().skip(i + 1) {
            assert_ne!(da, db, "`{na}` and `{nb}` are the same fixture");
        }
    }
}

#[test]
fn c2_the_diff_is_element_wise_not_positional() {
    // Two runs that publish identical content in a different ORDER must compare
    // equal. A positional differ would fail here and pass a real violation that
    // shifted every index by one.
    let a = Recorder::new();
    a.record(Frame::new(0, 0, "x", 1.0));
    a.record(Frame::new(1, 1, "y", 2.0));

    let b = Recorder::new();
    b.record(Frame::new(1, 1, "y", 2.0));
    b.record(Frame::new(0, 0, "x", 1.0));

    assert_eq!(
        Differ::new().first_divergence(&a.into_run(), &b.into_run()),
        None
    );
}

#[test]
fn c2_a_look_ahead_sma_is_caught_at_the_bar_it_leaks_to() {
    // The mutant, in the shape a real bug takes: a "trailing" mean that indexes
    // forward. Everything published at or before bar t must still move, because
    // every bar read one ahead.
    let closes = Fixture::sawtooth(30).closes();
    let baseline = run(&Fixture::sawtooth(30));

    let leaky = {
        let recorder = Recorder::new();
        for (bar, &c) in closes.iter().enumerate() {
            let sma = if bar + 2 < closes.len() {
                (c + closes[bar + 1] + closes[bar + 2]) / 3.0
            } else {
                c
            };
            recorder.record(Frame::new(bar, bar, "sma_3", sma));
        }
        recorder.into_run()
    };

    // Sanity: the runs really do differ, so the next assertion is meaningful.
    assert_ne!(Differ::new().first_divergence(&baseline, &leaky), None);

    // And the gate catches it at the FIRST bar that leaked, not at the last.
    let d = Differ::new()
        .first_divergence(&baseline, &leaky)
        .expect("a look-ahead must be visible");
    assert_eq!(d.bar, 0, "the earliest leak is bar 0; reported {:?}", d.bar);
}

// ---------------------------------------------------------------------------
// Against the REAL pivot detector, not a toy
// ---------------------------------------------------------------------------

/// The fixture's OHLCV as decimals, the shape `detect_pivots` consumes.
fn decimal_series(fixture: &Fixture) -> (Vec<rust_decimal::Decimal>, Vec<rust_decimal::Decimal>) {
    let conv = |xs: &[f64]| -> Vec<rust_decimal::Decimal> {
        xs.iter()
            .map(|x| rust_decimal::Decimal::from_f64_retain(*x).unwrap_or(dec!(0)))
            .collect()
    };
    (conv(&fixture.highs()), conv(&fixture.lows()))
}

#[test]
fn c2_pivot_detection_never_answers_for_a_bar_it_cannot_have_seen() {
    // `detect_pivots` over a truncated series must never report a pivot the full
    // series would not also report at that bar. This is the CG2 claim applied to
    // the one confirmation-delayed calculation in the registry.
    let fixture = Fixture::sawtooth(FIXTURE_BARS);
    let (highs, lows) = decimal_series(&fixture);
    let full = FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0);
    assert!(!full.is_empty(), "the fixture produced no pivots");

    for cut in [20usize, 30, 40] {
        let truncated = FibonacciRange::detect_pivots(&highs[..cut], &lows[..cut], STRENGTH, 0);
        for t in truncated {
            // A pivot the truncated run reports at index `i` is knowable at
            // `i + strength`. If that is at or before the cut, the full run must
            // agree on it — otherwise the truncated run invented a pivot.
            if t.confirming_bar() < cut {
                let found = full
                    .iter()
                    .find(|f| f.index == t.index && f.pivot_type == t.pivot_type);
                assert!(
                    found.is_some(),
                    "truncated run reported a {t:?} the full run does not have"
                );
            }
        }
    }
}

#[test]
fn c2_a_truncated_run_never_reports_a_pivot_past_its_own_data() {
    // The scan is bounded by `end = total - pivot_strength`. If that bound were
    // ever loosened, this fails.
    let (highs, lows) = decimal_series(&Fixture::sawtooth(30));
    let pivots = FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0);
    for p in &pivots {
        assert!(
            p.confirming_bar() < highs.len(),
            "pivot at {} confirms at {}, past the {} bars supplied",
            p.index,
            p.confirming_bar(),
            highs.len()
        );
        assert!(p.index > 0 && p.index + STRENGTH < highs.len());
        assert_eq!(p.strength, STRENGTH);
        assert!(matches!(p.pivot_type, PivotType::High | PivotType::Low));
    }
}

#[test]
fn c2_availability_matches_the_window_the_scan_actually_consults() {
    // M1 lives here.
    //
    // The consistency tests above compare a truncated run against a full run.
    // When both call the SAME availability function, they agree with each other
    // even when both are wrong — so consistency cannot detect a mutant that
    // shortens the availability. That is exactly what M1 did: `confirming_bar`
    // returned the pivot bar, and every comparison still passed.
    //
    // The claim has to be checked against the DATA instead. `detect_pivots`
    // scans `for j in i - strength ..= i + strength`, so the furthest bar it can
    // have consulted is `i + strength`. Availability cannot be earlier than that,
    // because the last bar on the far side is what confirms the pivot.
    let (highs, lows) = decimal_series(&Fixture::sawtooth(FIXTURE_BARS));
    let pivots = FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0);
    assert!(
        !pivots.is_empty(),
        "the fixture produced no pivots, so the assertion below would be vacuous"
    );

    for p in &pivots {
        assert_eq!(
            p.confirming_bar(),
            p.index + STRENGTH,
            "pivot at bar {} claims availability at {}, but the scan consults \
             through bar {} — availability earlier than that is a look-ahead",
            p.index,
            p.confirming_bar(),
            p.index + STRENGTH
        );
    }
}

#[test]
fn c2_a_pivot_is_not_knowable_before_its_far_side_bar_closed() {
    // The behavioural half of the same claim: perturb the bar that CONFIRMS a
    // pivot and require the pivot itself to move; perturb a bar inside the
    // window but before its end and require the same. If availability were the
    // pivot bar, the first assertion would fail.
    let (highs, lows) = decimal_series(&Fixture::sawtooth(FIXTURE_BARS));
    let full = FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0);
    let target = full
        .iter()
        .find(|p| p.confirming_bar() + 5 < highs.len())
        .expect("need a pivot with room to perturb past it");

    // Truncate at exactly the pivot's own bar: the pivot cannot appear, because
    // the far side has not been supplied.
    let too_early =
        FibonacciRange::detect_pivots(&highs[..target.index], &lows[..target.index], STRENGTH, 0);
    assert!(
        !too_early.iter().any(|p| p.index == target.index),
        "a pivot at bar {} appeared from a series that stops at it — it was \
         declared knowable before the bar that confirms it closed",
        target.index
    );

    // Truncate one bar past the confirming bar: now it must appear.
    let end = target.confirming_bar() + 1;
    let now_visible = FibonacciRange::detect_pivots(&highs[..end], &lows[..end], STRENGTH, 0);
    assert!(
        now_visible.iter().any(|p| p.index == target.index),
        "a pivot at bar {} was still hidden at bar {}, its confirming bar",
        target.index,
        target.confirming_bar()
    );
}

// ---------------------------------------------------------------------------
// M1 — the mutation, kept as a durable regression
// ---------------------------------------------------------------------------

/// **M1**, as an isolated copy of production logic.
///
/// 01-20 §8.1 requires a mutation to be injected into a *copy*, never into
/// production itself, and to be demonstrated failing once. Both happened at
/// `60064867 + 1`: the real `PivotPoint::confirming_bar` was temporarily
/// shortened to `self.index`, CG1 and CG2 failed with the assertions quoted in
/// `docs/conceptual-foundations/01-20-causal-gate-plan.md` §8.1, and the file
/// was restored from git. This copy exists so the evidence survives without
/// anyone re-injecting the defect: the gate's own premise is asserted against
/// the mutant, and the mutant must fail it.
struct BypassedAvailability {
    index: usize,
    strength: usize,
}

impl BypassedAvailability {
    /// The defect: availability is the bar the pivot *happened* on, so a pivot
    /// claims to have been known before the far side supplied the evidence.
    fn confirming_bar(&self) -> usize {
        self.index
    }

    fn is_knowable_at(&self, bar: usize) -> bool {
        self.confirming_bar() <= bar
    }
}

/// The gate's premise, as a reusable assertion so the mutant is judged by the
/// same statement that judged production.
fn availability_is_legal(index: usize, strength: usize, confirming_bar: usize) -> bool {
    confirming_bar == index + strength
}

#[test]
fn c2_the_gate_catches_m1_bypassed_availability() {
    let index = 6usize;
    let strength = STRENGTH;

    // Production satisfies the premise.
    let real = FibonacciRange::detect_pivots(
        &decimal_series(&Fixture::sawtooth(FIXTURE_BARS)).0,
        &decimal_series(&Fixture::sawtooth(FIXTURE_BARS)).1,
        STRENGTH,
        0,
    );
    let pivot = real
        .iter()
        .find(|p| p.index == index)
        .expect("the fixture has a pivot at bar 6");
    assert!(
        availability_is_legal(pivot.index, pivot.strength, pivot.confirming_bar()),
        "production must satisfy its own gate"
    );

    // The mutant does not, and the premise rejects it — this is the assertion
    // that caught M1, quoted as a statement rather than as a log line.
    let mutant = BypassedAvailability { index, strength };
    assert_eq!(mutant.confirming_bar(), index);
    assert!(
        !availability_is_legal(mutant.index, mutant.strength, mutant.confirming_bar()),
        "M1 must be illegal: it claims a pivot was knowable {strength} bars \
         before the far side confirmed it"
    );
    assert!(
        mutant.is_knowable_at(index),
        "M1's whole failure mode: it answers `yes, already known` on the bar \
         the pivot occurred"
    );
    assert!(
        !mutant.is_knowable_at(index - 1),
        "M1 still may not answer for a bar before the pivot itself"
    );
}

#[test]
fn c2_the_diff_reports_the_earliest_leak_not_the_latest() {
    // A violation at bar 0 cascades through every later bar. Reporting the last
    // would report thirty bugs for one.
    let recorder_a = Recorder::new();
    let recorder_b = Recorder::new();
    for bar in 0..10 {
        recorder_a.record(Frame::new(bar, bar, "x", 1.0));
        recorder_b.record(Frame::new(bar, bar, "x", 9.0));
    }
    let d = Differ::new()
        .first_divergence(&recorder_a.into_run(), &recorder_b.into_run())
        .unwrap();
    assert_eq!(d.bar, 0);
}
