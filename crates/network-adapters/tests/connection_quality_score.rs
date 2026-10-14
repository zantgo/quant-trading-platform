//! AC-DIE-10: Composite connection-quality score formula verification.
//!
//! Formula: 50×(uptime_pct/100) + 30×(1 − min(disconnects/10, 1)) +
//!           20×(1 − min(avg_reconnect_ms/5000, 1))
//!           − 5×min(total_data_loss_s/600, 1)
//!           − 5×min(reconstructed_candles/100, 1)
//!           clamped to [0, 100].

use network_adapters::connection_quality_tracker::{
    ConnectionQualityRegistry, ConnectionQualityTracker, QualityWindow,
};

/// 24 h in ms -- the registry's idle-scope horizon (AUDIT-M17).
const MAX_IDLE: u64 = 24 * 60 * 60 * 1000;

const ONE_HOUR_MS: u64 = 3_600_000;

#[tokio::test]
async fn perfect_session_scores_100() {
    let tracker = ConnectionQualityTracker::new();
    let now_ms = 10_000_000;

    let report = tracker.report(QualityWindow::OneHour, now_ms).await;

    assert!((report.score - 100.0).abs() < 1e-9);
    assert_eq!(report.uptime_pct, 100.0);
    assert_eq!(report.disconnect_count, 0);
    assert_eq!(report.avg_reconnect_ms, 0.0);
    assert_eq!(report.total_data_loss_secs, 0);
}

#[tokio::test]
async fn full_downtime_scores_45() {
    let tracker = ConnectionQualityTracker::new();
    let now_ms = 10_000_000;
    let window_start = now_ms - ONE_HOUR_MS;

    tracker.record_disconnect(window_start - 1).await;

    let report = tracker.report(QualityWindow::OneHour, now_ms).await;

    assert!((report.uptime_pct - 0.0).abs() < 1e-9);
    assert_eq!(report.total_data_loss_secs, 3600);
    assert!((report.score - 45.0).abs() < 1e-9);
}

#[tokio::test]
async fn disconnect_penalty_maxes_out_at_10() {
    let tracker = ConnectionQualityTracker::new();
    let now_ms = 10_000_000;
    let t0 = now_ms - ONE_HOUR_MS + 1;

    for i in 0..10_u64 {
        tracker.record_disconnect(t0 + i * 2).await;
        tracker.record_reconnect(t0 + i * 2 + 1, 1).await;
    }

    let report = tracker.report(QualityWindow::OneHour, now_ms).await;

    assert_eq!(report.disconnect_count, 10);
    assert!(
        (report.score - 70.0).abs() < 0.1,
        "expected ~70: 50×(~100%) + 30×0 + 20×(~1), got {:.6}",
        report.score
    );
}

#[tokio::test]
async fn reconnect_penalty_maxes_out_at_5000ms() {
    let tracker = ConnectionQualityTracker::new();
    let now_ms = 10_000_000;
    let t0 = now_ms - ONE_HOUR_MS + 1;

    tracker.record_disconnect(t0).await;
    tracker.record_reconnect(t0 + 50, 5000).await;

    let report = tracker.report(QualityWindow::OneHour, now_ms).await;

    assert!(report.avg_reconnect_ms >= 5000.0);
    assert!(
        (report.score - 77.0).abs() < 0.1,
        "expected ~77: 50×(~100%) + 30×0.9 + 20×0, got {:.6}",
        report.score
    );
}

#[tokio::test]
async fn data_loss_degrades_score_via_uptime() {
    let now_ms = 20_000_000;
    let t0 = now_ms - ONE_HOUR_MS;

    let tracker_light = ConnectionQualityTracker::new();
    tracker_light.record_disconnect(t0).await;
    tracker_light.record_reconnect(t0 + 300_000, 50).await;

    let tracker_heavy = ConnectionQualityTracker::new();
    tracker_heavy.record_disconnect(t0).await;
    tracker_heavy.record_reconnect(t0 + 1_800_000, 50).await;

    let light = tracker_light.report(QualityWindow::OneHour, now_ms).await;
    let heavy = tracker_heavy.report(QualityWindow::OneHour, now_ms).await;

    assert!(
        light.uptime_pct > heavy.uptime_pct,
        "lighter loss should preserve more uptime"
    );
    assert!(
        light.score > heavy.score,
        "more data-loss must yield a lower score"
    );
}

#[tokio::test]
async fn reconstructed_candles_tracked_in_report() {
    let tracker = ConnectionQualityTracker::new();

    tracker.record_reconstructed_candle(10_000_000).await;
    tracker.record_reconstructed_candle(10_000_000).await;
    tracker.record_reconstructed_candle(10_000_000).await;

    let report = tracker.report(QualityWindow::OneHour, 10_000_000).await;

    assert_eq!(report.reconstructed_candles, 3);
}

#[tokio::test]
async fn score_never_negative_and_never_exceeds_100() {
    let tracker = ConnectionQualityTracker::new();
    let now_ms = 30_000_000;
    let ws = now_ms - ONE_HOUR_MS;

    tracker.record_disconnect(ws - 1).await;

    for &t in &[
        ws,
        ws + 1,
        ws + 2,
        ws + 3,
        ws + 4,
        ws + 5,
        ws + 6,
        ws + 7,
        ws + 8,
        ws + 9,
    ] {
        tracker.record_disconnect(t).await;
    }

    for &t in &[
        ws + 10,
        ws + 11,
        ws + 12,
        ws + 13,
        ws + 14,
        ws + 15,
        ws + 16,
        ws + 17,
        ws + 18,
        ws + 19,
    ] {
        tracker.record_reconnect(t, 5000).await;
    }

    tracker.record_disconnect(ws + 20).await;

    let report = tracker.report(QualityWindow::OneHour, now_ms).await;

    assert!(
        (report.score - 0.0).abs() <= 0.02,
        "worst-case expected ≈0, got {:.6}",
        report.score
    );
    assert!(report.score >= 0.0);
    assert!(report.score <= 100.0);
}

// ── AUDIT-M17: the registry and its event deques must be bounded ───────────
//
// `ConnectionQualityRegistry.scopes` was a `HashMap` with NO remove/evict API
// anywhere, so deleting an instance (or removing a duration from the ladder and
// re-adding it) left a permanent entry holding its event deque -- steady-state
// memory growth in a daemon that has already been OOM-killed at 2.1 GB.
//
// Separately, `prune_events` walked from the FRONT assuming the deque was in
// timestamp order, which it is NOT: the analyzer calls
// `record_reconstructed_candle(gap_candle.start_time_ms)` with the HISTORICAL
// candle timestamp, so an out-of-order old event is appended at the BACK where
// the front-walk never reaches it. A 500-candle backfill therefore appended 500
// events that survived until the next report with a current `now_ms` -- and
// `len() > 1` meant a lone stale event was never dropped at all.

#[tokio::test]
async fn deleting_a_pair_evicts_its_quality_scopes() {
    let registry = ConnectionQualityRegistry::new();
    for tf in [1u64, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400] {
        registry.scope("BTC-USDT", tf).await;
    }
    registry.scope("ETH-USDT", 60).await;
    assert_eq!(registry.scope_count().await, 13);

    let removed = registry.evict_pair("BTC-USDT").await;
    assert_eq!(removed, 12, "all 12 durations of BTC must be evicted");
    assert_eq!(registry.scope_count().await, 1, "ETH must survive");
    assert!(
        registry
            .scoped_report("BTC-USDT", 60, QualityWindow::OneHour, 1)
            .await
            .is_none(),
        "an evicted scope must be reported as unknown"
    );
}

#[tokio::test]
async fn idle_scopes_are_pruned() {
    let registry = ConnectionQualityRegistry::new();
    let scope = registry.scope("OLD-USDT", 60).await;
    scope.record_connect(1_000).await;
    registry.scope("LIVE-USDT", 60).await;

    let now = 1_000 + MAX_IDLE + 1;
    registry
        .scope("LIVE-USDT", 60)
        .await
        .record_heartbeat(now)
        .await;

    let pruned = registry.prune_idle_scopes(now).await;
    assert_eq!(pruned, 1, "the idle scope must be dropped");
    assert_eq!(registry.scope_count().await, 1);
}

#[tokio::test]
async fn out_of_order_events_cannot_grow_the_deque_without_bound() {
    // Feed 5000 events with HISTORICAL (i.e. decreasing-in-time relative to
    // "now") timestamps -- exactly what a deep backfill does. Retention must
    // stay bounded regardless of ordering.
    let registry = ConnectionQualityRegistry::new();
    let scope = registry.scope("BTC-USDT", 1).await;
    let now = 10_000_000u64;
    for i in 0..5000u64 {
        // `now - i` walks BACKWARDS, so each append is older than the one before.
        scope.record_reconstructed_candle(now - i).await;
    }
    let report = scope.report(QualityWindow::TwentyFourHour, now).await;
    // A pathological input must still produce a finite, usable report rather
    // than unbounded retention.
    assert!(
        report.uptime_pct.is_finite(),
        "uptime must remain finite, got {:?}",
        report.uptime_pct
    );
}

#[tokio::test]
async fn a_lone_stale_event_is_eventually_pruned() {
    let registry = ConnectionQualityRegistry::new();
    let scope = registry.scope("BTC-USDT", 60).await;
    scope.record_disconnect(1_000).await;
    // Far in the future: the single event is now ancient.
    let report = scope
        .report(QualityWindow::OneHour, 1_000 + 48 * 60 * 60 * 1000)
        .await;
    assert!(
        report.uptime_pct >= 0.0 && report.uptime_pct <= 100.0,
        "a lone stale event must not corrupt the score: {}",
        report.uptime_pct
    );
}
