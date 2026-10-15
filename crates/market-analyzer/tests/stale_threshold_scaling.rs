//! AUDIT-ILS-09: the staleness budget must scale with the candle interval.
//!
//! `candle_buffer.stale_threshold_secs` is ONE platform-wide value (300 s by
//! default) and both escalations measure "no completed candle for longer than
//! N seconds":
//!
//!   - `derive_pipeline_state_with_staleness` -> Stale at N, Failed at 2N
//!   - ILS-09 `build_indicator_lifecycle_map`  -> Stale at N, Failed at 2N
//!
//! For any duration whose INTERVAL exceeds N, the healthy path is impossible to
//! hold between closes, so the pipeline degrades on a schedule it cannot avoid:
//! a 1h pipeline goes Stale 300 s after each close and Failed at 600 s, then
//! recovers briefly at the top of the hour.
//!
//! Live capture on the default `[1, 60, 300, 3600, 14400]` ladder:
//!   1s LIVE   1m LIVE   5m STALE   1h FAILED   4h FAILED/INITIALIZING
//! The slow columns were permanently marked broken on healthy feeds — and a
//! genuinely dead feed was indistinguishable from a healthy one.

use core_domain::models::CandlePipelineState;
use market_analyzer::analyzer::{
    derive_pipeline_state_with_staleness, effective_stale_threshold_secs,
};

const CONFIGURED: u32 = 300;

fn state_after_gap(tf_secs: u64, gap_secs: u64) -> String {
    let effective = effective_stale_threshold_secs(CONFIGURED, tf_secs);
    let now_ms = 2_000_000_000_u64;
    // A healthy close happened `gap_secs` ago.
    let last = now_ms - gap_secs * 1000;
    format!(
        "{:?}",
        derive_pipeline_state_with_staleness(500, 500, Some(last), now_ms, effective)
    )
}

#[test]
fn the_budget_scales_with_the_interval() {
    // Fast durations are still governed by the configured value.
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 1), 300);
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 60), 300);

    // Slow durations get 1.5 intervals.
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 300), 450);
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 3_600), 5_400);
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 14_400), 21_600);
    assert_eq!(effective_stale_threshold_secs(CONFIGURED, 86_400), 129_600);
}

#[test]
fn the_slow_columns_stay_live_across_a_normal_inter_close_gap() {
    // The defect: a 1h/4h pipeline reported Failed while perfectly healthy,
    // purely because the gap between closes exceeds 2x the 300 s budget.
    for tf_secs in [3_600_u64, 14_400] {
        // Just under a full interval since the last close — the NORMAL state
        // for a healthy slow duration.
        let gap = tf_secs - 1;
        assert_eq!(
            state_after_gap(tf_secs, gap),
            "Live",
            "tf={tf_secs}s: a healthy pipeline {gap}s after its last close must \
             not be declared Failed"
        );
    }
}

#[test]
fn a_genuinely_dead_slow_feed_is_still_detected() {
    // The fix must not blind the detector: after a whole interval plus the
    // margin (and twice that) it must escalate.
    let tf_secs = 3_600_u64;
    let effective = effective_stale_threshold_secs(CONFIGURED, tf_secs) as u64;

    assert_eq!(
        state_after_gap(tf_secs, effective),
        "Live",
        "boundary: age == N is not yet stale"
    );
    assert_eq!(state_after_gap(tf_secs, effective + 1), "Stale");
    assert_eq!(
        state_after_gap(tf_secs, effective * 2),
        "Stale",
        "the Failed edge is exclusive"
    );
    assert_eq!(state_after_gap(tf_secs, effective * 2 + 1), "Failed");
    // And far past it.
    assert_eq!(state_after_gap(tf_secs, effective * 10), "Failed");
}

#[test]
fn fast_durations_keep_their_original_sensitivity() {
    // 1s/1m behaviour is unchanged: the configured 300 s still governs, so a
    // 1m pipeline that misses its close for 10 minutes is still flagged.
    let tf_secs = 60_u64;
    let effective = effective_stale_threshold_secs(CONFIGURED, tf_secs) as u64;
    assert_eq!(effective, 300);
    assert_eq!(state_after_gap(tf_secs, 100), "Live");
    assert_eq!(state_after_gap(tf_secs, 300), "Live");
    assert_eq!(state_after_gap(tf_secs, 301), "Stale");
    assert_eq!(state_after_gap(tf_secs, 600), "Stale");
    assert_eq!(state_after_gap(tf_secs, 601), "Failed");
}

/// The old behaviour, pinned so the regression cannot return unnoticed: with a
/// flat 300 s budget the 1h column fails 600 s after every close.
#[test]
fn the_previous_flat_budget_broke_the_slow_columns() {
    for tf_secs in [3_600_u64, 14_400] {
        let gap = tf_secs - 1;
        let now_ms = 2_000_000_000_u64;
        let last = now_ms - gap * 1000;
        let state = derive_pipeline_state_with_staleness(500, 500, Some(last), now_ms, CONFIGURED);
        assert_eq!(
            state,
            CandlePipelineState::Failed,
            "precondition: the flat budget must fail tf={tf_secs}s at {gap}s, \
             which is what this fix removes"
        );
    }
}
