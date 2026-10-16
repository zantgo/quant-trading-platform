//! v11.14.1: moving the warm replay off the async runtime onto a bounded
//! blocking pool must not change a single value it produces.
//!
//! The change is a scheduling one — the per-duration replay went from a
//! synchronous sequential `.map()` to `spawn_blocking` tasks joined through a
//! semaphore — but it touches the code that seeds every indicator for every
//! pipeline, so "it compiles and the durations are the same length" is not a
//! sufficient check. If parallelism ever leaked state across durations (a
//! shared scratch buffer used by the calculators, an ordering assumption on
//! the result vector), the indicator state every live frame is computed from
//! would be silently wrong, with no error anywhere.
//!
//! The assertions read `latest_snapshot.indicators` — the same map the first
//! live frame is computed from — rather than the internal calculators, whose
//! fields are private.

use core_domain::models::duration_label;
use core_domain::normalized::Exchange;
use core_domain::normalized::NormalizedCandle;
use market_analyzer::active_set::ActiveSet;
use market_analyzer::analyzer::warm::{warm_indicators_for_timeframe, WarmedPipelineState};
use rust_decimal::Decimal;

/// A deterministic synthetic series — no RNG, so a failure is reproducible.
fn candles(n: usize, start: f64, step: f64) -> Vec<NormalizedCandle> {
    (0..n)
        .map(|i| {
            let close = Decimal::from_f64_retain(start + step * i as f64).unwrap_or(Decimal::ZERO);
            NormalizedCandle {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDC".to_string(),
                start_time_ms: 1_700_000_000_000 + (i as u64) * 60_000,
                duration_ms: 60_000,
                open: close,
                high: close * Decimal::from_f64_retain(1.01).unwrap(),
                low: close * Decimal::from_f64_retain(0.99).unwrap(),
                close,
                volume: Decimal::from(100 + i as i64),
                trades_count: 10,
                reconstructed: None,
            }
        })
        .collect()
}

fn config(secs: u64) -> config_models::TimeframeConfig {
    config_models::TimeframeConfig {
        candles: config_models::CandlesConfig {
            duration_seconds: secs,
        },
        ..Default::default()
    }
}

fn warm(candles: Vec<NormalizedCandle>, secs: u64) -> WarmedPipelineState {
    warm_indicators_for_timeframe(
        candles,
        &config(secs),
        &config_models::FibonacciConfig::default(),
        "BTC-USDC",
        secs,
        duration_label(secs),
        500,
        &ActiveSet::all_enabled(),
        Some(Exchange::Hyperliquid),
    )
}

/// Flatten the emitted indicator map into a comparable form.
fn fingerprint(w: &WarmedPipelineState) -> Vec<(String, String)> {
    let Some(snap) = w.latest_snapshot.as_ref() else {
        return Vec::new();
    };
    let mut v: Vec<(String, String)> = snap
        .indicators
        .iter()
        .map(|(k, val)| (k.clone(), format!("{:?}", val.raw_value)))
        .collect();
    v.sort();
    v
}

/// Two runs of the same input must agree exactly.
#[test]
fn the_replay_is_deterministic() {
    let a = warm(candles(300, 80_000.0, 1.5), 60);
    let b = warm(candles(300, 80_000.0, 1.5), 60);

    let fa = fingerprint(&a);
    assert!(
        !fa.is_empty(),
        "precondition: the warm must emit indicators"
    );
    assert_eq!(
        fa,
        fingerprint(&b),
        "the warm replay must be deterministic — a pool that changed values \
         between identical runs would corrupt every indicator from the first \
         live frame onward"
    );
    assert_eq!(a.snapshot_history.len(), b.snapshot_history.len());
}

/// Different durations must NOT influence each other.
///
/// The sequential `.map()` had this isolation property and the semaphore only
/// bounds scheduling, but a shared scratch buffer across calculators is exactly
/// the kind of bug parallelism could introduce, so it is asserted rather than
/// assumed.
#[test]
fn durations_do_not_contaminate_each_other() {
    let solo_1m = fingerprint(&warm(candles(200, 80_000.0, 1.0), 60));
    let solo_5m = fingerprint(&warm(candles(200, 80_000.0, 1.0), 300));

    let (a, b) = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            tokio::join!(
                tokio::task::spawn_blocking(|| warm(candles(200, 80_000.0, 1.0), 60)),
                tokio::task::spawn_blocking(|| warm(candles(200, 80_000.0, 1.0), 300)),
            )
        });

    assert_eq!(
        fingerprint(&a.expect("1m")),
        solo_1m,
        "a 1m warm run concurrently with a 5m warm must equal the 1m warm run \
         in isolation"
    );
    assert_eq!(
        fingerprint(&b.expect("5m")),
        solo_5m,
        "a 5m warm run concurrently with a 1m warm must equal the 5m warm run \
         in isolation"
    );
}

/// v11.14.1: the snapshot/projection pair emitted per candle.
///
/// `latest_snapshot` used to be `Some(snapshot.clone())` with that clone
/// overwritten on every subsequent candle — TWO full deep clones per candle
/// (`history_projection` clones internally too), one of them discarded. The fix
/// projects first, then MOVES. The observable contract must be unchanged: the
/// retained `latest_snapshot` is the LAST candle, and `snapshot_history` holds
/// one projected frame per candle, in order.
#[test]
fn latest_snapshot_is_the_last_candle_and_history_is_complete() {
    let n = 120usize;
    let w = warm(candles(n, 80_000.0, 1.0), 60);

    let last = w
        .latest_snapshot
        .as_ref()
        .expect("a warm must retain a latest snapshot");
    let expected_close =
        Decimal::from_f64_retain(80_000.0 + (n - 1) as f64).unwrap_or(Decimal::ZERO);
    assert_eq!(
        last.close,
        Some(expected_close),
        "latest_snapshot must be the NEWEST warmed candle, not a stale early one"
    );

    assert_eq!(
        w.snapshot_history.len(),
        n,
        "snapshot_history must hold one projected frame per warmed candle"
    );
    assert_eq!(
        w.snapshot_history[0].close,
        Some(Decimal::from_f64_retain(80_000.0).unwrap()),
        "snapshot_history must stay in candle order"
    );
    assert_eq!(
        w.snapshot_history[n - 1].close,
        Some(expected_close),
        "the last history frame must match latest_snapshot"
    );
}
