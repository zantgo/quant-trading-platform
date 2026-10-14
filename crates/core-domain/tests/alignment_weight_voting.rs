//! AUDIT-M16: `trend_agreement_weighted` must actually apply the weights.
//!
//! The vote counters accumulated `weight as u32`. With the default ladder the
//! TF-weight divisor is the SLOWEST active duration (14400), so
//! `(secs / 14400).clamp(0.2, 1.0)` yields 0.2 for every duration below 4 h —
//! and `0.2 as u32 == 0`. With the flag enabled, 11 of 12 durations contributed
//! NO vote and `trend_agreement_pct` pinned to `1/12 ≈ 8%` regardless of the
//! market.
//!
//! That figure feeds `compute_candidate_score`'s `a_mtf` term, so every
//! opportunity was scored against a near-constant, meaningless alignment value.
//! The flag defaults to `false`, but the moment an operator enabled it they got
//! a silent 100% error.

use std::collections::HashMap;

use core_domain::alignment::{compute_alignment, AlignmentParams, TimeframeAlignmentInput};
use core_domain::indicator_dtos::NormalizedIndicatorValue;

/// The default 12-duration ladder (`config_models::DEFAULT_TIMEFRAMES`).
const LADDER: [u64; 12] = [1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400];

/// Interned labels, so the `&'static str` in each input tuple is valid.
static LABELS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
    LADDER
        .iter()
        .map(|&s| core_domain::duration_label(s).leak() as &'static str)
        .collect()
});

/// `MarketContext` has no `Default`/`new`; construct it explicitly, mirroring
/// the `empty_ctx` helper the in-crate tests use.
fn ctx_with_score(score: i32) -> core_domain::market_context::MarketContext {
    use core_domain::market_context::{ContextDimension, MarketContext};
    MarketContext {
        trend: ContextDimension::neutral(),
        momentum: ContextDimension::neutral(),
        volatility: ContextDimension::neutral(),
        volume: ContextDimension::neutral(),
        liquidity: ContextDimension::neutral(),
        regime: "RANGE".to_string(),
        overall_score: score,
        overall_label: if score > 0 { "BULL" } else { "BEAR" }.to_string(),
    }
}

/// Build one input per duration; `scores[i]` is that duration's overall score.
fn inputs<'a>(
    map: &'a HashMap<String, NormalizedIndicatorValue>,
    ctxs: &'a [core_domain::market_context::MarketContext],
    scores: &[f64],
) -> Vec<TimeframeAlignmentInput<'a>> {
    LABELS
        .iter()
        .zip(LADDER.iter())
        .zip(scores.iter())
        .zip(ctxs.iter())
        .map(|(((&label, &secs), &score), ctx)| (label, secs, score, map, ctx))
        .collect()
}

fn empty_map() -> HashMap<String, NormalizedIndicatorValue> {
    HashMap::new()
}

/// The headline regression: with the weighted flag on, sub-4h durations must
/// still be able to contribute a vote.
///
/// Under the old `weight as u32` truncation every duration below 4 h scored
/// 0.2 and was cast to 0, so only the single 4h duration ever voted.
#[test]
fn sub_four_hour_durations_still_vote_when_weighted() {
    let map = empty_map();

    // Every duration bullish -> agreement must be ~100%.
    let scores = vec![1.0f64; LADDER.len()];
    let ctxs: Vec<_> = scores.iter().map(|s| ctx_with_score(*s as i32)).collect();
    let params = AlignmentParams {
        trend_agreement_weighted: true,
        ..Default::default()
    };
    let m = compute_alignment("BTC-USDT", &inputs(&map, &ctxs, &scores), &params);
    assert!(
        m.trend_agreement_pct > 95.0,
        "unanimous bullish must read near 100%, got {}",
        m.trend_agreement_pct
    );

    // Split vote: 11 bullish, 1 bearish. With the old truncation only the
    // 4h duration voted at all, so flipping WHICH duration was bearish changed
    // nothing. Now it must change the result.
    let mut params_split = params.clone();
    params_split.trend_agreement_weighted = true;
    let ctxs_bull: Vec<_> = scores.iter().map(|_| ctx_with_score(1)).collect();
    let m_all_bull = compute_alignment(
        "BTC-USDT",
        &inputs(&map, &ctxs_bull, &scores),
        &params_split,
    );

    // Now make the FASTEST duration (1s, weight 0.2) the lone bear.
    let mut bear_scores = vec![1.0f64; LADDER.len()];
    bear_scores[0] = -1.0;
    let bear_ctxs: Vec<_> = bear_scores
        .iter()
        .map(|s| ctx_with_score(*s as i32))
        .collect();
    let m_fast_bear = compute_alignment(
        "BTC-USDT",
        &inputs(&map, &bear_ctxs, &bear_scores),
        &params_split,
    );

    // ...and the SLOWEST (4h, weight 1.0) the lone bear.
    let mut slow_scores = vec![1.0f64; LADDER.len()];
    let last = slow_scores.len() - 1;
    slow_scores[last] = -1.0;
    let slow_ctxs: Vec<_> = slow_scores
        .iter()
        .map(|s| ctx_with_score(*s as i32))
        .collect();
    let m_slow_bear = compute_alignment(
        "BTC-USDT",
        &inputs(&map, &slow_ctxs, &slow_scores),
        &params_split,
    );

    assert!(
        m_all_bull.trend_agreement_pct > 99.0,
        "baseline must be unanimous, got {}",
        m_all_bull.trend_agreement_pct
    );
    assert!(
        (m_fast_bear.trend_agreement_pct - m_slow_bear.trend_agreement_pct).abs() > 1.0,
        "the WEIGHT of the dissenting duration must matter: fast-bear {} vs \
         slow-bear {} — under the old truncation both were identical because \
         every sub-4h duration voted 0",
        m_fast_bear.trend_agreement_pct,
        m_slow_bear.trend_agreement_pct
    );
    // A bear on the HEAVIER 4h duration (weight 1.0) should leave LESS
    // agreement than a bear on the lightest 1s duration (weight 0.2). With the
    // exact weights the ladder produces -- ten durations at the 0.2 floor,
    // 1h at 0.25, 4h at 1.0, total 3.25 -- these are 2.25/3.25 = 69.23% and
    // 3.05/3.25 = 93.85%.
    assert!(
        m_slow_bear.trend_agreement_pct < m_fast_bear.trend_agreement_pct,
        "a bear on the HEAVIER 4h duration must leave LESS agreement \
         (slow-bear {} vs fast-bear {})",
        m_slow_bear.trend_agreement_pct,
        m_fast_bear.trend_agreement_pct
    );
}

/// Unweighted mode must stay count-based: one bear out of 12 is 11/12 ≈ 91.7%.
#[test]
fn unweighted_mode_is_still_count_based() {
    let map = empty_map();
    let mut scores = vec![1.0f64; LADDER.len()];
    scores[0] = -1.0;
    let ctxs: Vec<_> = scores.iter().map(|s| ctx_with_score(*s as i32)).collect();
    let params = AlignmentParams {
        trend_agreement_weighted: false,
        ..Default::default()
    };
    let m = compute_alignment("BTC-USDT", &inputs(&map, &ctxs, &scores), &params);
    let expected = 11.0 / 12.0 * 100.0;
    assert!(
        (m.trend_agreement_pct - expected).abs() < 1.0,
        "unweighted 11/12 must be ~{expected}%, got {}",
        m.trend_agreement_pct
    );
}

/// Whatever the mode, the result must stay inside 0..100 and be finite.
#[test]
fn agreement_stays_bounded_in_every_configuration() {
    let map = empty_map();
    for weighted in [true, false] {
        for pattern in 0..4 {
            let scores: Vec<f64> = (0..LADDER.len())
                .map(|i| match pattern {
                    0 => 1.0,
                    1 => -1.0,
                    2 => {
                        if i % 2 == 0 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    _ => 0.0, // neutral: votes nothing
                })
                .collect();
            let ctxs: Vec<_> = scores.iter().map(|s| ctx_with_score(*s as i32)).collect();
            let params = AlignmentParams {
                trend_agreement_weighted: weighted,
                ..Default::default()
            };
            let m = compute_alignment("BTC-USDT", &inputs(&map, &ctxs, &scores), &params);
            assert!(
                m.trend_agreement_pct.is_finite() && (0.0..=100.0).contains(&m.trend_agreement_pct),
                "weighted={weighted} pattern={pattern}: agreement {} must be in 0..=100",
                m.trend_agreement_pct
            );
        }
    }
}

/// A single duration must not divide by zero.
#[test]
fn a_single_duration_does_not_divide_by_zero() {
    let map = empty_map();
    let ctx = ctx_with_score(1);
    for weighted in [true, false] {
        let params = AlignmentParams {
            trend_agreement_weighted: weighted,
            ..Default::default()
        };
        let data = vec![("1s", 1u64, 1.0f64, &map, &ctx)];
        let m = compute_alignment("BTC-USDT", &data, &params);
        assert!(
            m.trend_agreement_pct.is_finite(),
            "weighted={weighted}: single-duration agreement must be finite"
        );
    }
}

/// Neutral durations vote nothing; if EVERY duration is neutral the result is 0.
#[test]
fn all_neutral_yields_zero_agreement() {
    let map = empty_map();
    let ctx = ctx_with_score(0);
    let params = AlignmentParams {
        trend_agreement_weighted: true,
        ..Default::default()
    };
    let data: Vec<TimeframeAlignmentInput<'_>> = LABELS
        .iter()
        .zip(LADDER.iter())
        .map(|(&label, &secs)| (label, secs, 0.0f64, &map, &ctx))
        .collect();
    let m = compute_alignment("BTC-USDT", &data, &params);
    assert_eq!(
        m.trend_agreement_pct, 0.0,
        "no bullish or bearish votes must not produce NaN"
    );
}
