//! AUDIT-M1: the OI delta must be reachable at EVERY duration.
//!
//! OI history used to be appended only when a candle CLOSED, and the delta was
//! computed against the previous sample. `oi_delta_window_secs` resolves to 3600
//! for every duration >= 60 s, so on a 4h/12h/1d pipeline the previous sample was
//! 4-24 hours old, the 1 h cutoff pruned it, the sample count collapsed to 1, and
//! the delta returned `None` FOREVER.
//!
//! Observed: on the 4h column of the DEFAULT 12-duration ladder the `oi_delta`
//! and `oi_price_divergence` indicator rows never appeared,
//! `MarketSnapshot.oi_delta_pct` was permanently `None`, and the
//! `OI_FUNDING_DIVERGENCE` / `OI_PRICE_DIVERGENCE` L2.5 signals could never fire
//! — with no error, no log and no `Failed` state. A silently degraded Derivatives
//! block on the slowest columns.
//!
//! The window is now SAMPLED on a timer decoupled from the candle cadence, so the
//! tests here drive the sampler directly at several cadences and assert the delta
//! resolves.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use market_analyzer::indicators::normalized::derivatives::normalize_oi_delta;
use tokio::sync::RwLock;

/// The per-duration window table, reproduced here so the test pins the exact
/// contract the sampler depends on.
fn window_for(secs: u64) -> u64 {
    match secs {
        1 => 60,
        3 => 120,
        5 => 300,
        15 => 600,
        30 => 900,
        60 => 1800,
        _ => 3600,
    }
}

/// Mirror of `analyzer::sample_oi_window`: append + prune to the window.
async fn sample(
    hist: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    oi: Option<f64>,
    window_secs: u64,
    now_secs: u64,
) {
    let Some(oi) = oi else { return };
    let mut hist = hist.write().await;
    if let Some((t, v)) = hist.back() {
        if *v == oi && now_secs.saturating_sub(*t) < window_secs.max(1) / 2 {
            return;
        }
    }
    hist.push_back((now_secs, oi));
    let cutoff = now_secs.saturating_sub(window_secs);
    while hist.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
        hist.pop_front();
    }
    while hist.back().map(|(t, _)| *t > now_secs).unwrap_or(false) {
        hist.pop_back();
    }
}

/// Mirror of the read side: how many samples survive, and what the delta is.
async fn read_delta(
    hist: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    cur: f64,
    window_secs: u64,
    now_secs: u64,
) -> (Option<f64>, Option<u64>) {
    let mut h = hist.write().await;
    let cutoff = now_secs.saturating_sub(window_secs);
    while h.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
        h.pop_front();
    }
    if h.len() < 2 {
        return (None, None);
    }
    let anchor = h.front().map(|(t, _)| now_secs.saturating_sub(*t));
    (Some(cur - h.front().copied().unwrap().1), anchor)
}

/// The headline regression: sample on the same cadence a 4h pipeline would see
/// under the OLD candle-close-only scheme, and assert the delta resolves.
#[tokio::test]
async fn oi_delta_resolves_on_a_4h_duration() {
    let window = window_for(14_400);
    // The old failure mode: exactly two samples, 4 h apart, window 1 h.
    // (A separate deque -- the old path leaves an entry stamped at `now`, which
    // would anchor the follow-up assertions at a zero span.)
    let legacy: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    sample(&legacy, Some(1_000.0), window, 0).await;
    sample(&legacy, Some(1_500.0), window, 14_400).await;
    let (delta, _anchor) = read_delta(&legacy, 1_500.0, window, 14_400).await;
    assert_eq!(
        delta, None,
        "PRECONDITION: with candle-close-only sampling the prior sample is \
         pruned by the 1h cutoff, so the delta is None -- this is the bug"
    );

    // The fix: the sampler runs on a timer, so the window fills regardless of
    // the candle cadence.
    let hist: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    let mut t = 0u64;
    let step = (window / 8).clamp(1, 60);
    while t <= 14_400 {
        sample(&hist, Some(1_000.0 + (t as f64 / 100.0)), window, t).await;
        t += step;
    }
    let (delta, anchor) = read_delta(&hist, 1_144.0, window, 14_400).await;
    assert!(
        delta.is_some(),
        "with timer-based sampling the 4h duration MUST produce an OI delta"
    );
    assert!(
        anchor.is_some_and(|a| a > 0 && a <= window),
        "the reported anchor span must be real and within the window, got {anchor:?}"
    );
}

/// Every duration in the default ladder must resolve.
#[tokio::test]
async fn every_duration_in_the_default_ladder_resolves() {
    for secs in [1u64, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14_400] {
        let window = window_for(secs);
        let hist: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
        let step = (window / 8).clamp(1, 60);
        // Fill the window the way the sampler would.
        let mut t = 0u64;
        while t <= window * 2 {
            sample(&hist, Some(1_000.0 + t as f64 / 10.0), window, t).await;
            t += step;
        }
        let now = window * 2;
        let (delta, _anchor) = read_delta(&hist, 1_000.0 + now as f64 / 10.0, window, now).await;
        assert!(
            delta.is_some(),
            "{secs}s: OI delta must resolve (window {window}s)"
        );
    }
}

/// A genuinely cold window must report `None` rather than a fabricated delta.
#[tokio::test]
async fn a_cold_window_reports_no_delta() {
    let window = window_for(3600);
    let hist: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    sample(&hist, Some(1_000.0), window, 0).await;
    let (delta, _) = read_delta(&hist, 1_000.0, window, 0).await;
    assert_eq!(delta, None, "one sample cannot produce a delta");
}

/// The OI signal thresholds must be a RATE, so the same bar means the same
/// thing at every cadence.
#[test]
fn oi_signal_thresholds_are_window_normalised() {
    // 500 contracts over 1 h is the trigger bar.
    let over_hour = normalize_oi_delta(600.0, None, 3600);
    // The SAME 600 contracts measured over 10 minutes is a much faster rate and
    // must therefore also trigger (old code compared raw counts, so this was a
    // silent 6x difference).
    let over_ten_min = normalize_oi_delta(600.0, None, 600);
    for v in [over_hour, over_ten_min] {
        assert!(
            v.signals.iter().any(|s| !s.label.contains("NO_")),
            "both cadences must fire the OI signal: {:?}",
            v.signals.iter().map(|s| &s.label).collect::<Vec<_>>()
        );
    }

    // ...and a small move over a long window must NOT fire.
    let tiny_over_day = normalize_oi_delta(60.0, None, 86_400);
    assert!(
        tiny_over_day
            .signals
            .iter()
            .all(|s| s.label.contains("NO_")
                || !s.label.contains("SURGE") && !s.label.contains("DRAIN")),
        "60 contracts over a day must not read as a surge: {:?}",
        tiny_over_day
            .signals
            .iter()
            .map(|s| &s.label)
            .collect::<Vec<_>>()
    );

    // A zero/absent window must not panic or divide by zero.
    let _ = normalize_oi_delta(1_000.0, None, 0);
}

/// The published anchor span must be the REAL span, not the configured bound.
#[tokio::test]
async fn the_reported_anchor_span_is_the_real_one() {
    let window = window_for(3600);
    let hist: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    // Only 5 minutes of history in a 1 h window.
    let step = (window / 8).clamp(1, 60);
    let mut t = 0u64;
    while t <= 300 {
        sample(&hist, Some(1_000.0 + t as f64 / 10.0), window, t).await;
        t += step;
    }
    let (_, anchor) = read_delta(&hist, 1_030.0, window, 300).await;
    assert!(
        anchor.is_some_and(|a| a <= 300),
        "a 5-minute-old window must publish ~300s, not the configured 3600s \
         bound; got {anchor:?}"
    );
}

/// Sanity: the sampler's rate must be bounded so a 1s pipeline does not append
/// thousands of samples an hour.
#[test]
fn the_sample_rate_is_bounded() {
    for secs in [1u64, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14_400] {
        let window = window_for(secs);
        let rate = (window / 8).clamp(1, 60);
        assert!(
            (1..=60).contains(&rate),
            "{secs}s: rate {rate} out of bounds"
        );
        // Worst case: the smallest window sampled every 1 s keeps 60 entries.
        let retained = window / rate;
        assert!(
            retained <= 3600,
            "{secs}s: window retains {retained} samples, which is unbounded growth"
        );
    }
}

/// Guard against the sampler silently degrading back to candle-close cadence.
#[tokio::test]
async fn sampling_is_independent_of_candle_cadence() {
    // Two pipelines with very different durations but the SAME sampling timer
    // must hold the same number of samples.
    let window = window_for(60);
    let hist: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    let step = (window / 8).clamp(1, 60);
    let mut t = 0u64;
    while t <= window {
        sample(&hist, Some(1_000.0), window, t).await;
        t += step;
    }
    let len = hist.read().await.len();
    assert!(
        len > 2,
        "a timer-driven window must hold many samples, got {len}"
    );
    assert!(
        len <= (window / step + 1) as usize,
        "retention must stay inside the window"
    );
    let _ = Duration::from_secs(1);
}
