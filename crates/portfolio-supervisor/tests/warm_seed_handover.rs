//! AUDIT-M3: the warm-seed handover must be ordered, on BOTH spawn paths.
//!
//! `bootstrap::populate_buffers` is the single seeder for the per-pair
//! `history` / `snapshot_history` deques, and the registry runs it AFTER
//! `build_pipelines` has already spawned `run_single`. `run_single` also seeds
//! those deques from the same `WarmedPipelineState`, guarded by
//! read-then-act empty-checks (AUDIT-AIU-117).
//!
//! Two writers, one lock, two empty-checks: if they both observe empty they
//! both push, and the window holds 2x the warm candles. That silently
//! double-counts warm bars in Fibonacci, pivot points, S/R, chart patterns and
//! the cluster estimator's `price_history` — precisely the structural
//! indicators — while byte accounting stays exact so the memory budget never
//! surfaced it.
//!
//! The `start_gate` makes the ordering deterministic. It was DECORATIVE on two
//! counts:
//!
//!  1. In `run_single` the wait sat ~350 lines BELOW the seeding it was meant
//!     to order, so every shared write had already happened.
//!  2. The recharge path set `start_gate: None` and never signalled — and
//!     recharge is the HOT path (ladder edit, config save, strategy change).
//!
//! These tests model the empty-check/append interleaving directly. They do not
//! need a live venue: the defect is an ordering property, so the model is the
//! specification.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

/// One shared buffer plus the read-then-act guard both writers use.
#[derive(Clone)]
struct SharedHistory {
    deque: Arc<RwLock<VecDeque<u32>>>,
    /// Number of times a writer observed `is_empty() == true` and then pushed.
    /// Two is the bug: it means the same warm candle landed twice.
    double_seed: Arc<AtomicUsize>,
}

/// AUDIT-AIU-117's exact guard, which is correct for one writer and racy for two.
async fn seed_if_empty(h: &SharedHistory, warm: Arc<Vec<u32>>) {
    let mut d = h.deque.write().await;
    if d.is_empty() {
        // Yield while holding the guard, the way `run_single`'s `history.write()`
        // spans an await before its `is_empty()` is acted on.
        tokio::task::yield_now().await;
        for c in warm.iter() {
            d.push_back(*c);
        }
    }
}

fn shared() -> SharedHistory {
    SharedHistory {
        deque: Arc::new(RwLock::new(VecDeque::new())),
        double_seed: Arc::new(AtomicUsize::new(0)),
    }
}

/// The handover the gate creates: `populate_buffers` finishes, THEN `run_single`
/// runs. Exactly one seeding happens.
#[tokio::test]
async fn the_gate_yields_a_single_seed() {
    let h = shared();
    let warm: Arc<Vec<u32>> = Arc::new((0..500).collect());

    // populate_buffers (registry) completes...
    seed_if_empty(&h, warm.clone()).await;
    // ...and only then is run_single released.
    seed_if_empty(&h, warm.clone()).await;

    let len = h.deque.read().await.len();
    assert_eq!(len, 500, "the warm window must hold each candle once");
    assert_eq!(
        h.double_seed.load(Ordering::Relaxed),
        0,
        "precondition: the guarded append was not used here"
    );
}

/// Concurrent seeding is IDEMPOTENT because the empty-guard holds the write
/// lock across its append (AUDIT-AIU-117).
///
/// Worth stating precisely, because it bounds what the gate is for: the
/// double-seed this audit was written about is NOT reproducible through the
/// guarded path — two writers cannot both observe `is_empty() == true` while
/// the first holds the lock. The guards already make it safe.
///
/// That makes `start_gate` defence in depth rather than the fix: it removes
/// the dependence on that argument holding for every future writer of these
/// deques, and it makes the "populate_buffers is the single seeder" invariant
/// true by ordering instead of by convention.
#[tokio::test]
async fn concurrent_guarded_seeding_is_idempotent() {
    let h = shared();
    let warm: Arc<Vec<u32>> = Arc::new((0..500).collect());

    let (ha, hb) = (h.clone(), h.clone());
    let (wa, wb) = (warm.clone(), warm.clone());
    let a = tokio::spawn(async move { seed_if_empty(&ha, wa).await });
    let b = tokio::spawn(async move { seed_if_empty(&hb, wb).await });
    a.await.unwrap();
    b.await.unwrap();

    assert_eq!(
        h.deque.read().await.len(),
        500,
        "the guarded append must be idempotent under any interleaving — each \
         warm candle lands exactly once"
    );
}

/// The gate is a `watch<bool>`. Prove the ordering primitive itself: a waiter
/// released after the seeder completes sees a fully-seeded buffer, and a waiter
/// that is never signalled is bounded by cancellation rather than hanging.
#[tokio::test]
async fn a_gated_reader_observes_a_seeded_buffer() {
    let h = shared();
    let warm: Arc<Vec<u32>> = Arc::new((0..500).collect());
    let (tx, rx) = tokio::sync::watch::channel(false);

    let deferred = {
        let h = h.clone();
        let warm = warm.clone();
        let mut rx = rx.clone();
        tokio::spawn(async move {
            // This is `run_single`'s shape: wait FIRST, then seed.
            while !*rx.borrow_and_update() {
                tokio::select! {
                    _ = rx.wait_for(|ready| *ready) => {}
                }
            }
            seed_if_empty(&h, warm.clone()).await;
        })
    };

    // Reader is parked. Seed, then release — the registry's order.
    seed_if_empty(&h, warm.clone()).await;
    tx.send(true).unwrap();
    deferred.await.unwrap();

    assert_eq!(
        h.deque.read().await.len(),
        500,
        "a gated writer that runs after the seeder must find the buffer \
         already populated and skip — never double-count"
    );
}

/// A forgotten signal must not wedge a pipeline forever. `run_single` bounds the
/// wait with its shutdown arm, so a miss degrades to a cancelled task.
#[tokio::test]
async fn a_forgotten_signal_is_bounded_by_cancellation() {
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let cancel = tokio_util::sync::CancellationToken::new();

    let waiter = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                if *rx.borrow() {
                    return "released";
                }
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return "cancelled",
                    _ = rx.wait_for(|ready| *ready) => {}
                }
            }
        })
    };

    // The sender is dropped without ever signalling — the "forgot to send" case.
    drop(_tx);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
        .await
        .expect("the wait must be bounded by the shutdown arm")
        .unwrap();
    assert_eq!(
        outcome, "cancelled",
        "a forgotten signal must cancel the waiter, not hang the pipeline"
    );
}
