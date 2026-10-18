//! **CG12 — deterministic replay** (P7).
//!
//! ## The claim
//!
//! 01-20 §6 G12: *same immutable input twice; IDs, identities, transitions,
//! values, timestamps match exactly.* The `Recorder` is append-only and the
//! `Clock` is a bar index, so nothing in the harness can depend on the wall
//! clock, on hash iteration order, or on the order two frames were pushed.
//!
//! ## Why this is a gate and not a unit test
//!
//! Nondeterminism is not a bug that shows up once — it is a property that is
//! either true of the harness or is not. A system whose replay differs between
//! two runs cannot produce a verdict that means anything, because every other
//! gate's result becomes a function of *when* it ran. So the claim is asserted
//! from four directions: repeat runs, reordered publication, concurrent
//! execution, and a static ban on wall-clock reads inside the harness.
//!
//! ## The static check is not decoration
//!
//! A test cannot observe a wall clock read that happens to produce the same
//! value twice. `SystemTime::now()` in the harness would be invisible to every
//! other assertion in this file and would surface later as a backtest whose
//! result depends on the phase of the moon. So the harness's own source is read
//! with `include_str!` and any wall-clock token fails the gate.

use market_analyzer::indicators::fibonacci::FibonacciRange;
use market_analyzer::replay::{Differ, Fixture, Frame, Futures, Recorder, Run};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const FIXTURE_BARS: usize = 60;
const STRENGTH: usize = 3;

/// The publication pass under test — the same one CG2 runs, kept here so CG12
/// and CG2 judge the *same* work rather than two similar workloads.
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
    for bar in 0..fixture.len() {
        let sma = if bar >= 2 {
            (closes[bar - 2] + closes[bar - 1] + closes[bar]) / 3.0
        } else {
            f64::NAN
        };
        recorder.record(Frame::new(bar, bar, "sma_3", sma));
        let vma = if bar >= 2 {
            (volumes[bar - 2] + volumes[bar - 1] + volumes[bar]) / 3.0
        } else {
            f64::NAN
        };
        recorder.record(Frame::new(bar, bar, "vol_ma_3", vma));
    }
    for p in FibonacciRange::detect_pivots(&highs, &lows, STRENGTH, 0) {
        recorder.record(Frame::new(
            p.confirming_bar(),
            p.index,
            format!("pivot:{:?}:{}", p.pivot_type, p.index),
            p.price.to_f64().unwrap_or_default(),
        ));
    }
    recorder.into_run()
}

/// A stable digest of a whole run: every field, in canonical order.
///
/// Hashes the rendered frames rather than a subset, so an added or dropped frame
/// changes the digest. `DefaultHasher` is `SipHash` with a **fixed** key — not
/// `RandomState` — so this number is comparable across processes.
fn digest(run: &Run) -> u64 {
    let mut h = DefaultHasher::new();
    for (bar, frames) in run {
        bar.hash(&mut h);
        for f in frames {
            f.render().hash(&mut h);
        }
    }
    h.finish()
}

#[test]
fn cg12_the_same_input_replayed_twice_is_identical() {
    let fixture = Fixture::sawtooth(FIXTURE_BARS);
    let first = run(&fixture);
    let second = run(&fixture);

    assert_eq!(
        Differ::new().first_divergence(&first, &second),
        None,
        "two replays of one input diverged"
    );
    assert_eq!(digest(&first), digest(&second));
    assert_eq!(first, second, "runs are not structurally equal");
}

#[test]
fn cg12_publication_order_does_not_change_the_result() {
    // Two runs that publish the same content in a different interleaving must
    // compare equal. If they did not, every gate result would depend on the
    // order an async pipeline happened to complete its futures in.
    let fixture = Fixture::sawtooth(FIXTURE_BARS);
    let canonical = run(&fixture);

    let reversed = Recorder::new();
    for (_, frames) in canonical.iter().rev() {
        for f in frames.iter().rev() {
            reversed.record(f.clone());
        }
    }
    let reordered = reversed.into_run();

    assert_eq!(Differ::new().first_divergence(&canonical, &reordered), None);
    assert_eq!(digest(&canonical), digest(&reordered));
}

#[test]
fn cg12_concurrent_replays_agree() {
    // Four threads, one input. Catches anything that leaks scheduling into a
    // published value: a `HashMap` iteration, an address-derived seed, a lazily
    // initialised global.
    let fixture = std::sync::Arc::new(Fixture::sawtooth(FIXTURE_BARS));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let f = std::sync::Arc::clone(&fixture);
            std::thread::spawn(move || digest(&run(&f)))
        })
        .collect();
    let digests: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(
        digests.windows(2).all(|w| w[0] == w[1]),
        "concurrent replays disagreed: {digests:?}"
    );
}

#[test]
fn cg12_pivot_identities_are_stable() {
    // Identities are part of the claim, not just values: a pivot that changed
    // its index or its confirming bar between runs would break every leg that
    // references it while leaving the value series untouched.
    let fixture = Fixture::sawtooth(FIXTURE_BARS);
    let ident = |f: &Fixture| -> Vec<(usize, String, usize, usize, String)> {
        let conv = |xs: &[f64]| -> Vec<rust_decimal::Decimal> {
            xs.iter()
                .map(|x| rust_decimal::Decimal::from_f64_retain(*x).unwrap_or(dec!(0)))
                .collect()
        };
        let (h, l) = (conv(&f.highs()), conv(&f.lows()));
        FibonacciRange::detect_pivots(&h, &l, STRENGTH, 0)
            .into_iter()
            .map(|p| {
                (
                    p.index,
                    format!("{:?}", p.pivot_type),
                    p.strength,
                    p.confirming_bar(),
                    p.price.to_string(),
                )
            })
            .collect()
    };
    let a = ident(&fixture);
    let b = ident(&fixture);
    assert!(!a.is_empty(), "the fixture produced no pivots to compare");
    assert_eq!(a, b);
}

#[test]
fn cg12_the_four_futures_are_reproducible_across_calls() {
    // The perturbation side is part of the harness too: a future that produced a
    // different fixture on a second call would make CG2's green run unrepeatable.
    let base = Fixture::sawtooth(FIXTURE_BARS);
    for name in Futures::names() {
        let first = Futures::apply(&base, 33, name).unwrap();
        let second = Futures::apply(&base, 33, name).unwrap();
        assert_eq!(first, second, "`{name}` is not reproducible");
        assert_eq!(first.digest(), second.digest());
        assert_eq!(base.prefix_digest(32), first.prefix_digest(32));
    }
}

#[test]
fn cg12_the_harness_reads_no_wall_clock() {
    // Static, because a behavioural test cannot see a clock read that happens to
    // return the same value twice.
    let sources: [(&str, &str); 4] = [
        ("mod.rs", include_str!("../src/replay/mod.rs")),
        ("snapshot.rs", include_str!("../src/replay/snapshot.rs")),
        ("differ.rs", include_str!("../src/replay/differ.rs")),
        ("perturb.rs", include_str!("../src/replay/perturb.rs")),
    ];
    for banned in [
        "SystemTime",
        "Instant::now",
        "Utc::now",
        "Local::now",
        "elapsed(",
        "thread_rng",
        "random(",
    ] {
        for (name, src) in sources.iter() {
            // The doc comments are allowed to *name* the banned constructs in
            // order to forbid them; only code counts.
            let code: String = src
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !code.contains(banned),
                "replay/{name} references `{banned}`: a published value must not \
                 depend on a wall clock or an RNG (CH-3, CH-8)"
            );
        }
    }
}

#[test]
fn cg12_the_recorder_is_append_only() {
    // A recorder that could revise a run would let a bug hide itself: the
    // recording that proves causality must not be editable after the fact.
    let recorder = Recorder::new();
    for bar in 0..5 {
        recorder.record(Frame::new(bar, bar, "x", bar as f64));
    }
    let snapshot = recorder.run();
    recorder.record(Frame::new(5, 5, "x", 5.0));

    // The earlier snapshot is unchanged by the later publication.
    assert_eq!(snapshot.len(), 5);
    let after = recorder.run();
    assert_eq!(after.len(), 6);
    let prefix: Run = after.iter().take(5).map(|(k, v)| (*k, v.clone())).collect();
    assert_eq!(
        digest(&snapshot),
        digest(&prefix),
        "a later publication revised an earlier recording"
    );
}
