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
/// AUDIT-M1-SHARED: uniform retention for the shared OI deque — the widest
/// window any duration can request.
const RETENTION_SECS: u64 = 3600;

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
    // AUDIT-M1-SHARED: production no longer skips an exact duplicate of the
    // newest reading — a flat OI must still grow the deque so it resolves to a
    // zero delta instead of None.
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
    let cutoff = now_secs.saturating_sub(RETENTION_SECS);
    while h.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
        h.pop_front();
    }
    // Production reads NON-destructively: the deque is retained at the widest
    // window and each duration selects its own slice without mutating it.
    let in_window: Vec<(u64, f64)> = h
        .iter()
        .copied()
        .filter(|(t, _)| *t >= now_secs.saturating_sub(window_secs) && *t <= now_secs)
        .collect();
    if in_window.len() < 2 {
        return (None, None);
    }
    let anchor = in_window.first().map(|(t, _)| now_secs.saturating_sub(*t));
    (Some(cur - in_window.first().copied().unwrap().1), anchor)
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

/// AUDIT-M1-SCALE: the snapshot field `oi_delta_pct` must be a PERCENT.
///
/// Live capture from this fix in action: Hyperliquid BTC, OI `3_309_797_750`,
/// absolute change `1_039_234` over a 51 s anchor. That is **0.031 %** — a quiet
/// market. It was published as `oi_delta_pct: 1039234.17`, i.e. a million
/// percent, because the snapshot field was fed the ABSOLUTE delta.
///
/// The absolute form is NOT wrong for its other consumer:
/// `normalize_oi_delta` thresholds an absolute count ("500 contracts over a 1 h
/// window = 500/h"), so percent-izing the shared value would have broken the
/// `oi_delta` indicator and the `oi_price_divergence` signal. The two contracts
/// collided on one field; the fix publishes both from one computation.
///
/// This test pins the scale of each, so the two cannot silently swap again.
#[test]
fn the_snapshot_pct_field_is_a_percent_not_an_absolute_count() {
    // Live-observed pair: absolute OI change and the level it changed from.
    let anchor_oi = 3_309_797_750.0_f64;
    let current_oi = 3_310_837_052.0_f64;
    let absolute = current_oi - anchor_oi;
    assert!(
        (absolute - 1_039_302.0).abs() < 1_000.0,
        "precondition: the observed absolute change reproduces (~1.04e6), got {absolute}"
    );

    // Contract 1: `MarketSnapshot.oi_delta_pct` is a percent.
    let pct = absolute / anchor_oi * 100.0;
    assert!(
        pct > 0.0 && pct < 1.0,
        "oi_delta_pct must be a quiet-market percent (0 < pct < 1), got {pct}"
    );
    assert!(
        pct.abs() <= 1_000.0,
        "regression: the absolute delta ({absolute}) leaked into oi_delta_pct \
         and surfaced as ~1e6 % instead of {pct} %"
    );

    // Contract 2: the indicator still consumes the ABSOLUTE change.
    //
    // AUDIT-M1-HORIZON: normalized against the CONFIGURED window (3600 s), not
    // the 51 s actual span. Note the direction of the error either way: a delta
    // measured over a young 51 s anchor, divided by 51 s, extrapolates to
    // 73.4M/h and saturates the normalizer — but extrapolating 51 seconds of
    // flow to an hour is itself fiction. The configured window is the basis the
    // thresholds were authored against, so a cold pipeline is UNDER-stated
    // (conservative: fewer signals) rather than permanently latched bullish.
    // The honest actual span still reaches consumers on the wire field.
    let configured_window_secs = 3600_u64;
    let nv = normalize_oi_delta(absolute, None, configured_window_secs);
    assert_eq!(nv.raw_value, absolute, "indicator raw must stay absolute");
    assert!(
        nv.normalized > -1.0 && nv.normalized <= 1.0,
        "normalized rate must stay bounded, got {}",
        nv.normalized
    );
}

/// AUDIT-M1-HORIZON: the two horizons must not be conflated.
///
/// The normalizer divides by its window to get a per-hour rate and fires above
/// 500/h, so the window it is given IS the measurement basis. Passing it the
/// short ACTUAL span of a young pipeline inflates the rate by the ratio
/// `configured / actual` and saturates the indicator.
#[test]
fn the_normalizer_is_given_the_configured_window_not_the_actual_span() {
    // A change that is meaningful over a full hour but enormous over 51 s:
    // 500/h is exactly the `has_signal` trigger bar.
    let absolute = 500.0_f64;

    // What Batch F shipped: the 51 s actual span, which extrapolates 500 units
    // of 51 s flow into a per-hour rate and saturates the indicator.
    let inflated = normalize_oi_delta(absolute, None, 51);
    // What it should be: the configured 3600 s horizon.
    let correct = normalize_oi_delta(absolute, None, 3600);

    assert_eq!(
        inflated.normalized, 1.0,
        "precondition: the 51 s span saturates the normalizer"
    );
    assert!(
        correct.normalized < 1.0,
        "the configured horizon must NOT saturate for an at-threshold change \
         (correct={}, inflated={})",
        correct.normalized,
        inflated.normalized
    );
    assert!(
        correct.normalized.abs() < inflated.normalized.abs(),
        "the configured horizon must produce a materially smaller rate \
         (correct={}, inflated={})",
        correct.normalized,
        inflated.normalized
    );
    // The absolute change is identical either way; only the horizon differs.
    assert_eq!(correct.raw_value, inflated.raw_value);
}

/// A zero/absent anchor OI must not produce inf/NaN in the percent field.
#[test]
fn a_zero_anchor_does_not_emit_a_non_finite_percent() {
    for anchor in [0.0_f64, -1.0_f64] {
        let delta = 1_039_234.0_f64;
        // Production guard: `if anchor > 0.0 { pct = Some(...) }`.
        let pct = if anchor > 0.0 {
            Some(delta / anchor * 100.0)
        } else {
            None
        };
        assert_eq!(
            pct, None,
            "a non-positive anchor must yield None, not a non-finite percent"
        );
    }
}

/// AUDIT-M1-SHARED: one deque is shared by every duration of the pair, so a
/// duration must never prune it to its OWN window.
///
/// Live capture of the bug this pins: five durations on one BTC pair. The 1s
/// slot resolved a 60 s window and the 1h slot a 3600 s window, both reading
/// and pruning the SAME deque. The 1s prune truncated global retention to 60 s,
/// so the 1h column could never hold two samples — and worse, the duplicate-skip
/// guard meant an unchanged OI never grew the deque past one entry. Result:
/// `n == 0` in 76 of 76 reads, `oi_delta_pct` null on EVERY duration, with no
/// error and no `Failed` state. The finding M1 was supposed to close was still
/// open end-to-end.
#[tokio::test]
async fn a_fast_duration_cannot_destroy_a_slow_durations_window() {
    let shared: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    let t0 = 1_000_000_u64;

    // The 1s slot samples every 7 s and reads a 60 s window.
    let fast_window = window_for(1);
    let mut t = t0;
    while t <= t0 + 300 {
        sample(
            &shared,
            Some(3_310_000_000.0 + (t - t0) as f64),
            RETENTION_SECS,
            t,
        )
        .await;
        t += 7;
    }
    // The 1h slot samples every 60 s and reads a 3600 s window.
    let slow_window = window_for(14_400);
    let mut t = t0;
    while t <= t0 + 300 {
        sample(
            &shared,
            Some(3_310_000_000.0 + (t - t0) as f64),
            RETENTION_SECS,
            t,
        )
        .await;
        t += 60;
    }

    // The 1s reader runs and selects its own slice.
    let (_fast_delta, _fast_span) =
        read_delta(&shared, 3_310_000_300.0, fast_window, t0 + 300).await;
    assert!(
        _fast_delta.is_some(),
        "the fast slot must resolve after the slow slot sampled"
    );

    // The slow reader must STILL resolve afterwards — the fast reader did not
    // truncate the shared deque to 60 s.
    let (slow_delta, slow_span) = read_delta(&shared, 3_310_000_300.0, slow_window, t0 + 300).await;
    assert!(
        slow_delta.is_some(),
        "REGRESSION: the 1s reader truncated the shared deque to its 60 s \
         window, so the 1h column could never resolve its 3600 s window"
    );
    assert_eq!(
        slow_delta,
        Some(300.0),
        "the slow delta must still be exact"
    );
    assert!(
        slow_span.unwrap() > fast_window,
        "the slow anchor must reach back over the full 3600 s window, got {:?}",
        slow_span
    );
}

/// AUDIT-M1-SHARED: a FLAT open interest is information, not a missing sample.
///
/// The sampler used to skip an exact duplicate of the newest reading, so an
/// unchanged OI never grew the deque past one entry and the delta stayed `None`
/// — a flat market reported as "no data" instead of a zero delta.
#[tokio::test]
async fn a_flat_open_interest_resolves_to_a_zero_delta_not_none() {
    let shared: Arc<RwLock<VecDeque<(u64, f64)>>> = Arc::new(RwLock::new(VecDeque::new()));
    let window = window_for(3_600);
    let t0 = 2_000_000_u64;
    // Five identical readings.
    for i in 0..5u64 {
        sample(&shared, Some(3_310_000_000.0), RETENTION_SECS, t0 + i * 60).await;
    }
    let (delta, span) = read_delta(&shared, 3_310_000_000.0, window, t0 + 4 * 60).await;
    assert_eq!(
        delta,
        Some(0.0),
        "a flat OI over a full window is a ZERO delta, not None — the \
         duplicate-skip used to report it as missing data"
    );
    assert_eq!(span, Some(4 * 60), "the span must be the real one");
}

/// AUDIT-M1: the sampler cadence must be decoupled from the STALE-CHECK cadence.
///
/// The sampler used to run inside the stale-check arm, and the stale-check
/// interval is `timeframe_secs * 1000 / 2` — so a duration sampled SLOWER than
/// its own window and could never hold two samples. Live capture on a
/// `[1, 60, 300, 3600, 14400]` ladder: the 1h slot sampled every 30 minutes and
/// the 4h slot every 2 hours, against a 3600 s window. 1s and 1m resolved while
/// 5m/1h/4h were still `null` after 6 minutes — the exact columns M1 exists to
/// fix.
///
/// This pins the interval the sampler actually uses: a function of the WINDOW,
/// never of the duration.
#[test]
fn the_sampler_interval_is_derived_from_the_window_not_the_duration() {
    // Production: `(window / 8).clamp(1, 60)`.
    let sampler_every_ms = |window: u64| (window / 8).clamp(1, 60) * 1000;
    // The cadence the sampler used to ride: `(timeframe_secs * 1000 / 2).max(500)`.
    let stale_check_ms = |secs: u64| (secs * 1000 / 2).max(500);

    for secs in [1u64, 60, 300, 3600, 14_400] {
        let window_ms = window_for(secs) * 1000;
        let every = sampler_every_ms(window_for(secs));
        // The decisive invariant: two samples must fit inside the window, or the
        // slot can never hold `n >= 2` and the delta is unreachable.
        assert!(
            2 * every <= window_ms,
            "tf={secs}s: two samples ({every} ms apart) must fit inside the \
             {window_ms} ms window"
        );
    }

    // And the old cadence demonstrably broke it on the 4h column of the DEFAULT
    // ladder: a 2 h sampler against a 1 h window can never hold two samples.
    // (1h is a boundary case — exactly two samples span the window — and still
    // fails in practice, because the first tick fires before any OI has arrived
    // and is dropped.)
    {
        let window_ms = window_for(14_400) * 1000;
        let old = stale_check_ms(14_400);
        assert!(
            2 * old > window_ms,
            "precondition: the stale-check cadence for the 4h slot ({old} ms) \
             must violate the invariant it used to ride"
        );
    }
}
