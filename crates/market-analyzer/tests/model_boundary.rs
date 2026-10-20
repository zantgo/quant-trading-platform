//! CG1–CG2 — the model boundary: MME produces state, never a verdict.
//!
//! CR-15 drew a hard line between what the Market Monitoring Engine is allowed
//! to know and what it is allowed to say. On one side: `CandleSeries →
//! MarketState` — what the market is doing. On the other: entry, target,
//! invalidation, reward:risk, an opportunity, a risk score, a go/no-go. The
//! second side is TAE's to compute, from model state, inside TAE.
//!
//! P10-G removed the last derived matrix: `synthesize_cross_tf` returns the
//! alignment alone.
//!
//! CG3 (the executor produces no setups) lives in
//! `crates/portfolio-supervisor/tests/model_boundary.rs` instead: the executor
//! is TAE's, and market-analyzer cannot depend on portfolio-supervisor without a
//! cycle. The split is the dependency graph agreeing with the boundary.

use market_analyzer::synthesis::synthesize_cross_tf;
use rust_decimal::Decimal;

use core_domain::alignment::AlignmentMatrix;
use core_domain::market_context::{ContextDimension, MarketContext};
use core_domain::models::MarketSnapshot;

// ─── CG1: the wire carries no strategy ────────────────────────────────────

/// The snapshot's serialized key set is the boundary in its most literal form.
/// If any strategy key reappears — even always-null — this fails. The retired
/// `analysis` matrix is included: it was a second, derived copy of the model
/// state and must not come back as a parallel surface.
#[test]
fn cg1_the_snapshot_wire_carries_no_strategy_matrix() {
    let snap = MarketSnapshot {
        symbol: "BTC-USDC".into(),
        timeframe_secs: 60,
        mid_price: Decimal::from_f64_retain(100.0).unwrap(),
        ..MarketSnapshot::default()
    };
    let json = serde_json::to_value(&snap).unwrap();
    let keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();

    for forbidden in [
        "risk",
        "advisory",
        "opportunity",
        "decision_context",
        "analysis",
    ] {
        assert!(
            !keys.contains(&forbidden),
            "CG1: `{forbidden}` is on the MarketSnapshot wire. A strategy matrix that \
             crosses the boundary cannot be un-shipped downstream — the field must be \
             absent, not null, or every consumer has to decide what \"null strategy\" \
             means and the dashboard renders it as a real reading."
        );
    }

    // The model's own matrix is still there. This is the other half of the
    // contract: the boundary is a narrowing, not a teardown.
    let populated = serde_json::to_value(MarketSnapshot {
        symbol: "BTC-USDC".into(),
        alignment: Some(AlignmentMatrix::empty("BTC-USDC")),
        ..MarketSnapshot::default()
    })
    .unwrap();
    let populated_keys: Vec<&str> = populated
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert!(
        populated_keys.contains(&"alignment"),
        "CG1: the model must still emit alignment"
    );
}

#[test]
fn cg1_history_projection_drops_no_longer_present_fields() {
    let full = MarketSnapshot {
        symbol: "BTC-USDC".into(),
        alignment: Some(AlignmentMatrix::empty("BTC-USDC")),
        ..MarketSnapshot::default()
    };
    let projected = full.history_projection();

    assert!(
        projected.alignment.is_none(),
        "CG1: the projection must drop alignment"
    );
    // And the live surface is untouched — the projection is a storage
    // optimisation, not a second, thinner truth.
    assert!(full.alignment.is_some());
}

// ─── CG2: synthesis produces the alignment and stops ──────────────────────

fn ctx(trend: f64, overall: i32) -> MarketContext {
    MarketContext {
        trend: ContextDimension {
            score: trend,
            confidence: 0.7,
            label: "WEAK_BULL".into(),
        },
        momentum: ContextDimension {
            score: trend,
            confidence: 0.6,
            label: "WEAK_BULL".into(),
        },
        volatility: ContextDimension {
            score: 0.0,
            confidence: 0.5,
            label: "NORMAL".into(),
        },
        volume: ContextDimension {
            score: 0.0,
            confidence: 0.5,
            label: "NORMAL".into(),
        },
        liquidity: ContextDimension::neutral(),
        regime: "TRENDING_BULL".into(),
        overall_score: overall,
        overall_label: if overall > 20 {
            "BULLISH".into()
        } else {
            "NEUTRAL".into()
        },
    }
}

fn snap(secs: u64, price: f64, context: MarketContext) -> MarketSnapshot {
    let mut m = MarketSnapshot {
        symbol: "BTC-USDC".into(),
        timeframe_secs: secs,
        timeframe_label: Some(format!("{secs}s")),
        mid_price: Decimal::from_f64_retain(price).unwrap(),
        close: Decimal::from_f64_retain(price),
        indicators: Default::default(),
        ..MarketSnapshot::default()
    };
    m.context = Some(context);
    m
}

fn synth(pairs: &[(u64, MarketSnapshot)]) -> AlignmentMatrix {
    let refs: Vec<(u64, &MarketSnapshot)> = pairs.iter().map(|(s, m)| (*s, m)).collect();
    synthesize_cross_tf(
        "BTC-USDC",
        &refs,
        &core_domain::alignment::AlignmentParams::default(),
    )
}

#[test]
fn cg2_synthesis_yields_alignment_and_nothing_else() {
    let a = snap(60, 100.0, ctx(0.8, 60));
    let r = synth(&[(60, a)]);
    assert_eq!(r.symbol, "BTC-USDC");
    assert_eq!(r.timeframes_present, 1);
}

#[test]
fn cg2_a_flat_market_reads_flat_rather_than_actionable() {
    let a = snap(60, 100.0, ctx(0.0, 0));
    let r = synth(&[(60, a)]);
    assert_eq!(
        r.mtf_stance,
        core_domain::alignment::CrossTfStance::Neutral,
        "CG2: a market with no directional lean reads Neutral — there is no other token \
         available, and that is the point"
    );
}

#[test]
fn cg2_the_score_is_aggregated_across_durations_not_read_off_one() {
    // The synthesis must genuinely combine the ladder. The check: adding a
    // second, agreeing duration changes the aggregate, which can only happen if
    // the added duration is contributing.
    let fast_bull = snap(60, 100.0, ctx(0.9, 80));
    let slow_bull = snap(3600, 100.0, ctx(0.9, 80));
    let both = synth(&[(60, fast_bull.clone()), (3600, slow_bull)]);
    let fast_only = synth(&[(60, fast_bull)]);

    assert!(
        both.timeframes_present == 2 && fast_only.timeframes_present == 1,
        "CG2: the fixture must differ in duration count for this test to mean anything"
    );
    assert!(
        both.mtf_overall_score != fast_only.mtf_overall_score,
        "CG2: the cross-duration score must aggregate — adding a duration that agrees \
         cannot leave the aggregate untouched (two durations: {}, one: {})",
        both.mtf_overall_score,
        fast_only.mtf_overall_score
    );
}
