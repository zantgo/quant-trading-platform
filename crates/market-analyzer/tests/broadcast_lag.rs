//! AC-L4-4 (03-01-05 §5.1): a lagged consumer receives `RecvError::Lagged(n)`
//! within 1 frame of falling behind and can resubscribe at the current head;
//! the producer is never blocked (AC-L4-3 non-blocking fan-out).

use core_domain::normalized::{Exchange, NormalizedCandle};
use rust_decimal_macros::dec;
use tokio::sync::broadcast;

fn candle(i: u64) -> NormalizedCandle {
    NormalizedCandle {
        exchange: Exchange::Hyperliquid,
        symbol: "BTC-USDT".to_string(),
        start_time_ms: i * 60_000,
        duration_ms: 60_000,
        open: dec!(100),
        high: dec!(101),
        low: dec!(99),
        close: dec!(100),
        volume: dec!(1),
        trades_count: 1,
        reconstructed: None,
    }
}

#[tokio::test]
async fn lagged_consumer_gets_lagged_error_and_resyncs() {
    let (tx, mut slow_rx) = broadcast::channel::<NormalizedCandle>(4);

    // Producer outruns the capacity-4 channel while the consumer sleeps.
    for i in 0..10u64 {
        tx.send(candle(i)).expect("send never blocks");
    }

    // First recv surfaces the explicit lag signal.
    match slow_rx.recv().await {
        Err(broadcast::error::RecvError::Lagged(n)) => {
            assert!(n >= 1, "lag count reported (got {n})");
        }
        other => panic!("expected Lagged, got {other:?}"),
    }

    // After the lag signal the consumer resumes from the oldest retained
    // frame and can drain to the head.
    let mut received = Vec::new();
    while let Ok(c) = slow_rx.try_recv() {
        received.push(c.start_time_ms / 60_000);
    }
    assert!(!received.is_empty(), "consumer resynchronized");
    assert_eq!(*received.last().unwrap(), 9, "caught up to the head");
    // Frames are in production order.
    assert!(
        received.windows(2).all(|w| w[0] < w[1]),
        "ordering preserved"
    );
}

#[tokio::test]
async fn slow_subscriber_never_blocks_producer_or_peers() {
    let (tx, mut fast_rx) = broadcast::channel::<NormalizedCandle>(4);
    let _slow_rx = tx.subscribe(); // never polled — permanently slow

    let start = std::time::Instant::now();
    let producer = tokio::spawn({
        let tx = tx.clone();
        async move {
            for i in 0..1_000u64 {
                let _ = tx.send(candle(i));
            }
        }
    });

    // The fast consumer keeps receiving (possibly with lag skips) while the
    // slow subscriber exists.
    let mut seen = 0u32;
    while let Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) = fast_rx.recv().await {
        seen += 1;
        if seen >= 4 {
            break;
        }
    }
    producer.await.unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "producer completed without ever blocking on the dead subscriber"
    );
}

#[tokio::test]
async fn resubscribe_starts_at_current_head() {
    let (tx, _keepalive) = broadcast::channel::<NormalizedCandle>(4);
    for i in 0..100u64 {
        let _ = tx.send(candle(i));
    }
    // A fresh subscription sees only frames sent after it was created.
    let mut fresh = tx.subscribe();
    let _ = tx.send(candle(1_000));
    let got = fresh.recv().await.expect("head frame");
    assert_eq!(got.start_time_ms, 1_000 * 60_000);
}

/// v11.12.24 (memory): the per-duration WS fan-out ring is 32, not 200.
///
/// The WS handler treats `Lagged` as "resume at the newest frame" — it never
/// replays the backlog — so the ring only needs to cover a brief consumer
/// stall. At 200 the worst case was 200 retained snapshots per subscribed
/// channel, and the dashboard subscribes one channel per (instance ×
/// duration): ~24 MB × 30 channels ≈ 700 MB of pure ring retention on a
/// 2.8 GB host. This pins that a consumer stalling past 32 frames is told so
/// (and recovers) rather than being allowed to pin the memory.
#[tokio::test]
async fn production_ring_capacity_bounds_retained_frames_per_channel() {
    const PRODUCTION_RING: usize = 32;
    let (tx, mut slow_rx) = broadcast::channel::<NormalizedCandle>(PRODUCTION_RING);

    // A tab stalls for far longer than the ring can hold (e.g. backgrounded).
    for i in 0..500u64 {
        tx.send(candle(i)).expect("send never blocks");
    }

    // It is told it fell behind rather than silently drifting.
    match slow_rx.recv().await {
        Err(broadcast::error::RecvError::Lagged(n)) => {
            assert!(n >= 1, "lag is signalled, got {n}");
        }
        other => panic!("expected Lagged after a long stall, got {other:?}"),
    }

    // The ring holds AT MOST `capacity` frames — the ceiling is real, not
    // proportional to how long the consumer slept.
    let mut retained = Vec::new();
    while let Ok(c) = slow_rx.try_recv() {
        retained.push(c.start_time_ms / 60_000);
    }
    assert!(
        retained.len() <= PRODUCTION_RING,
        "ring must retain at most {PRODUCTION_RING} frames, got {}",
        retained.len()
    );
    assert_eq!(
        *retained.last().unwrap(),
        499,
        "the consumer still lands on the newest frame"
    );
}

/// Zero receivers must retain nothing (tokio returns `SendError` before the
/// ring is touched), so a pair with no open tab costs no snapshot memory.
#[tokio::test]
async fn channel_without_receivers_retains_nothing() {
    let (tx, _rx_dropped) = broadcast::channel::<NormalizedCandle>(32);
    drop(_rx_dropped);
    for i in 0..1_000u64 {
        // Err(SendError) — the value is handed back, never parked in the ring.
        assert!(tx.send(candle(i)).is_err(), "no receiver ⇒ no retention");
    }
}
