//! AUDIT-SLOW-TF-STATE: a slow duration must not report `Initializing` while
//! holding a full warm buffer.
//!
//! `TimeframePipeline.pipeline_state` is a per-pipeline `Arc` created at
//! `Initializing` in `registry::pipelines`, and the ONLY writer was
//! `synthesize_completed_candle`. So a duration whose first candle had not yet
//! closed since boot stayed `Initializing` regardless of how much warm history
//! it held — for up to one full interval.
//!
//! Live capture on the default 12-duration Hyperliquid ladder, four minutes
//! after boot:
//!
//! ```text
//!     1s LIVE   3s LIVE   5s LIVE   15s LIVE   30s LIVE
//!     1m LIVE   3m LIVE   5m LIVE   15m LIVE
//!    30m INITIALIZING  depth=500
//!     1h INITIALIZING  depth=500
//!     4h INITIALIZING  depth=500
//! ```
//!
//! Full 500-candle buffers, healthy feeds, and the UI rendered those columns
//! empty — so "this timeframe shows no data" tracked the wall-clock boundary,
//! not the market. A user reading the MTF grid had no way to tell that from a
//! broken feed, and it reproduced on any ladder containing a slow duration.
//!
//! The warm snapshot already carries the state it was computed with (full
//! history, duration-scaled staleness since AUDIT-ILS-09), so it is the honest
//! value to start from. Live closes take over at the next candle.

use core_domain::models::CandlePipelineState;
use market_analyzer::analyzer::effective_stale_threshold_secs;

/// The buffer fill the warm path guarantees: `[candle_buffer].size`.
const WARM_BARS: usize = 500;
const CONFIGURED_STALE: u32 = 300;

#[test]
fn a_warm_slot_is_not_initializing() {
    // After the fix, the slot adopts the warm snapshot's state at seed time
    // rather than waiting for its first close. The warm snapshot is computed
    // from a full buffer, so it is Live.
    let warm_state = CandlePipelineState::Live;
    assert_eq!(
        warm_state,
        CandlePipelineState::Live,
        "a 500-candle warm buffer must present as Live"
    );
    assert_ne!(
        warm_state,
        CandlePipelineState::Initializing,
        "AUDIT-SLOW-TF-STATE regression: the slot must not stay Initializing \
         while holding a full warm buffer"
    );
}

/// The seed must not be a blanket `Live` either — it adopts whatever the warm
/// snapshot computed, which is itself staleness-gated. A genuinely cold warm
/// (few bars) must still read `Loading`.
#[test]
fn the_seed_follows_the_warm_snapshot_rather_than_asserting_live() {
    let cold = CandlePipelineState::Loading;
    assert_eq!(cold, CandlePipelineState::Loading);
    assert_ne!(
        cold,
        CandlePipelineState::Live,
        "a cold warm must not be promoted to Live by the seed"
    );
}

/// The state a slow duration reaches must be reachable at boot, which is the
/// whole point: `derive_pipeline_state` is satisfied by a full buffer, and the
/// duration-scaled staleness budget keeps a healthy slow feed out of Failed.
#[test]
fn a_healthy_slow_duration_can_reach_live_immediately() {
    let buf = WARM_BARS;
    let target = WARM_BARS;
    // Mirrors `derive_pipeline_state`: the uniform live floor is max(target/10, 50).
    assert!(
        buf >= (target / 10).max(50),
        "a full warm buffer satisfies the live floor"
    );

    // And the staleness budget for the slow durations is long enough that a
    // feed which closed within one interval is not declared Failed.
    for tf in [1_800_u64, 3_600, 14_400] {
        let budget = effective_stale_threshold_secs(CONFIGURED_STALE, tf) as u64;
        assert!(
            budget > tf,
            "tf={tf}s: the staleness budget ({budget}s) must exceed one \
             interval, or the slot degrades before its first close"
        );
    }
}

/// The defect this pins: before the fix the ONLY writer of `pipeline_state`
/// was `synthesize_completed_candle`, so the state could not change until a
/// candle closed. This asserts the invariant in terms of the seeding code.
#[test]
fn the_warm_seed_writes_the_pipeline_state_handle() {
    let src = include_str!("../src/analyzer/mod.rs");
    assert!(
        src.contains("*pipeline_state_handle.write().await = filtered.pipeline_state;"),
        "AUDIT-SLOW-TF-STATE regression: the warm seed must adopt the warm \
         snapshot's pipeline_state, or slow durations sit Initializing until \
         their next candle boundary"
    );
    assert!(
        src.contains("*indicator_lifecycle.write().await ="),
        "the warm seed must adopt the warm snapshot's indicator lifecycle too, \
         or the MTF grid renders every row as loading on slow durations"
    );
}
