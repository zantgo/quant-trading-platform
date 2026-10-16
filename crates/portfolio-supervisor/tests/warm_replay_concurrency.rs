//! v11.14.1: the warm replay is bounded-parallel, and bounded.
//!
//! The REST candle fetch was always concurrent (one task per duration), but the
//! CPU replay that consumed it was a synchronous sequential `.map()`. On a
//! 12-duration ladder that meant 12 x 500 x 52 indicator computations IN
//! SERIES, on a Tokio worker thread.
//!
//! Two consequences, and the second is what the operator actually saw:
//!
//!   1. wall time was the SUM of the durations, not the maximum;
//!   2. running inline on the runtime it STALLED the runtime — the HTTP server,
//!      the WebSockets and the next instance's spawn could not progress while
//!      one instance replayed.
//!
//! That produced the reported symptom exactly: sub-minute slots (which bypass
//! REST via HFP-03 and fill from live trades) appeared immediately while every
//! duration >= 1m stayed blank, and the wizard's readiness probe passed on the
//! sub-minute ones.
//!
//! This file pins the SCHEDULING properties — that fan-out is bounded, that the
//! lower bound cannot deadlock, and that parallelism is strictly faster. The
//! replay's *result* is unchanged by parallelism; that equivalence is pinned in
//! `market-analyzer/tests/warm_replay_determinism.rs`, next to the code that
//! produces it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

/// A stand-in for `warm_indicators_for_timeframe`: real enough to observe
/// concurrency, cheap enough that the test measures scheduling rather than
/// indicator arithmetic. Returns its input so a test can assert nothing was
/// reordered or dropped.
fn replay(candles: Vec<u32>) -> Vec<u32> {
    std::thread::sleep(StdDuration::from_millis(40));
    candles
}

/// The scheduling shape `fetch_and_warm_bootstrap` now uses: a semaphore permit
/// acquired on the ASYNC side (so waiting never parks a blocking-pool thread)
/// and moved into the task.
async fn bounded_replay(
    inputs: Vec<Vec<u32>>,
    warm_concurrency: usize,
    peak: Arc<AtomicUsize>,
) -> Vec<Vec<u32>> {
    let permits = warm_concurrency.max(1);
    let slots = Arc::new(tokio::sync::Semaphore::new(permits));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::with_capacity(inputs.len());
    for candles in inputs {
        let permit = slots.clone().acquire_owned().await.expect("semaphore open");
        let in_flight = in_flight.clone();
        let peak = peak.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            let out = replay(candles);
            in_flight.fetch_sub(1, Ordering::SeqCst);
            out
        }));
    }
    let mut out = Vec::with_capacity(tasks.len());
    for t in tasks {
        out.push(t.await.expect("replay task joined"));
    }
    out
}

fn ladder(n: u32) -> Vec<Vec<u32>> {
    (0..n).map(|i| vec![i; 4]).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fan_out_is_bounded_by_the_configured_concurrency() {
    let peak = Arc::new(AtomicUsize::new(0));
    let out = bounded_replay(ladder(12), 4, peak.clone()).await;

    assert_eq!(out.len(), 12, "every active duration must still be warmed");
    assert!(
        peak.load(Ordering::SeqCst) <= 4,
        "warm_concurrency = 4 must never exceed 4 concurrent replays, saw {}",
        peak.load(Ordering::SeqCst)
    );
    assert!(
        peak.load(Ordering::SeqCst) > 1,
        "precondition: the replays must actually overlap, or this test proves \
         nothing (peak was {})",
        peak.load(Ordering::SeqCst)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrency_of_one_is_strictly_sequential() {
    let peak = Arc::new(AtomicUsize::new(0));
    bounded_replay(ladder(6), 1, peak.clone()).await;

    assert_eq!(
        peak.load(Ordering::SeqCst),
        1,
        "warm_concurrency = 1 must restore the old strictly-sequential replay"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_zero_value_cannot_deadlock_the_spawn() {
    // `0` is rejected by `validate_platform`, but the semaphore is also
    // defensive: a hand-edited config reaching the replay directly must still
    // make progress rather than hang forever.
    let done = tokio::time::timeout(
        StdDuration::from_secs(10),
        bounded_replay(ladder(4), 0, Arc::new(AtomicUsize::new(0))),
    )
    .await;
    assert!(
        done.is_ok(),
        "warm_concurrency = 0 must not deadlock the spawn"
    );
    assert_eq!(done.expect("completed").len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallelism_shortens_the_wall_clock_and_preserves_order() {
    let inputs = ladder(8);

    let t0 = Instant::now();
    let mut sequential = Vec::new();
    for candles in inputs.clone() {
        sequential.push(replay(candles));
    }
    let seq_time = t0.elapsed();

    let t1 = Instant::now();
    let parallel = bounded_replay(inputs, 4, Arc::new(AtomicUsize::new(0))).await;
    let par_time = t1.elapsed();

    assert_eq!(
        parallel, sequential,
        "bounded parallelism is a scheduling change: the per-duration results \
         must be identical and in the same order"
    );
    assert!(
        par_time < seq_time,
        "bounded parallelism must beat the sequential baseline \
         (parallel {par_time:?} vs sequential {seq_time:?})"
    );
}
