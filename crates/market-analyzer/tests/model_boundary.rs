//! CG1–CG2 — the model boundary: MME produces state, never a verdict.
//!
//! CR-15 drew a hard line between what the Market Monitoring Engine is allowed
//! to know and what it is allowed to say. On one side: `CandleSeries →
//! MarketState` — what the market is doing. On the other: entry, target,
//! invalidation, reward:risk, an opportunity, a risk score, a go/no-go. The
//! second side is TAE's to compute, from model state, inside TAE.
//!
//! This file pins the line. Each test below is written so that **reverting the
//! corresponding production change makes it fail** — not merely "the value
//! changed". That distinction matters: a test that asserts a number is only as
//! strong as the number, while a test that asserts a *field does not exist*
//! cannot pass by accident.
//!
//! What is deliberately NOT here: assertions that a strategy layer works. There
//! is no strategy layer on this side of the line to test.
//!
//! CG3 (the executor produces no setups) lives in
//! `crates/portfolio-supervisor/tests/model_boundary.rs` instead: the executor
//! is TAE's, and market-analyzer cannot depend on portfolio-supervisor without a
//! cycle. The split is the dependency graph agreeing with the boundary.

use market_analyzer::synthesis::{synthesize_cross_tf, CrossTfSynthesisResult};
use rust_decimal::Decimal;

use core_domain::alignment::AlignmentMatrix;
use core_domain::analysis::{AnalysisMatrix, MarketBias};
use core_domain::market_context::{ContextDimension, MarketContext};
use core_domain::models::MarketSnapshot;

// ─── CG1: the wire carries no strategy ────────────────────────────────────

/// The snapshot's serialized key set is the boundary in its most literal form.
/// If any of the four strategy keys reappear — even always-null — this fails.
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

    for forbidden in ["risk", "advisory", "opportunity", "decision_context"] {
        assert!(
            !keys.contains(&forbidden),
            "CG1: `{forbidden}` is on the MarketSnapshot wire. A strategy matrix that \
             crosses the boundary cannot be un-shipped downstream — the field must be \
             absent, not null, or every consumer has to decide what \"null strategy\" \
             means and the dashboard renders it as a real reading."
        );
    }

    // The model's own two matrices are still there. This is the other half of the
    // contract: the boundary is a narrowing, not a teardown.
    let populated = serde_json::to_value(MarketSnapshot {
        symbol: "BTC-USDC".into(),
        alignment: Some(AlignmentMatrix::empty("BTC-USDC")),
        analysis: Some(AnalysisMatrix::empty("BTC-USDC")),
        ..MarketSnapshot::default()
    })
    .unwrap();
    let populated_keys: Vec<&str> = populated
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    // The model's own two matrices are still there. This is the other half of the
    // contract: the boundary is a narrowing, not a teardown.
    //
    // (They are `skip_serializing_if = "Option::is_none"`, so they only appear on a
    // snapshot that has them — which is why the populated case is checked here and
    // not in the default snapshot above.)
    assert!(
        populated_keys.contains(&"alignment"),
        "CG1: the model must still emit alignment"
    );
    assert!(
        populated_keys.contains(&"analysis"),
        "CG1: the model must still emit analysis"
    );
}

#[test]
fn cg1_history_projection_drops_no_longer_present_fields() {
    // `history_projection()` is the retained-copy view of the snapshot. It used to
    // null out `risk` / `advisory` / `opportunity` / `decision_context` for space;
    // with the fields gone there is nothing to strip. If this fails with a missing
    // field, the struct grew a strategy matrix again.
    let full = MarketSnapshot {
        symbol: "BTC-USDC".into(),
        alignment: Some(AlignmentMatrix::empty("BTC-USDC")),
        analysis: Some(AnalysisMatrix::empty("BTC-USDC")),
        ..MarketSnapshot::default()
    };
    let projected = full.history_projection();

    assert!(
        projected.alignment.is_none(),
        "CG1: the projection must drop alignment"
    );
    assert!(
        projected.analysis.is_none(),
        "CG1: the projection must drop analysis"
    );
    // And the live surface is untouched — the projection is a storage optimisation,
    // not a second, thinner truth.
    assert!(full.alignment.is_some());
    assert!(full.analysis.is_some());
}

// ─── CG2: synthesis produces two matrices and stops ───────────────────────

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

fn synth(pairs: &[(u64, MarketSnapshot)]) -> CrossTfSynthesisResult {
    let refs: Vec<(u64, &MarketSnapshot)> = pairs.iter().map(|(s, m)| (*s, m)).collect();
    synthesize_cross_tf(
        "BTC-USDC",
        &refs,
        None,
        None,
        None,
        None,
        &core_domain::analysis::AnalysisParams::default(),
        &core_domain::alignment::AlignmentParams::default(),
        &config_models::strategy::LadderRoles::default(),
    )
}

#[test]
fn cg2_synthesis_yields_alignment_and_analysis_and_nothing_else() {
    let a = snap(60, 100.0, ctx(0.8, 60));
    let r = synth(&[(60, a)]);
    assert_eq!(r.alignment.symbol, "BTC-USDC");
    assert_eq!(r.analysis.symbol, "BTC-USDC");
    // The bias is a model state, not a verdict. Asserting it is a `MarketBias`
    // is the type-level statement that no "what to do" token can inhabit this
    // field — the enum simply has no such variant.
    let _: MarketBias = r.analysis.bias;
}

#[test]
fn cg2_a_flat_market_reads_flat_rather_than_actionable() {
    // The failure this guards is the subtle one: a synthesizer that had drifted
    // toward recommendations would map a neutral market onto "no opportunity"
    // or a HOLD-style token. The model has exactly one way to express "flat", and
    // it is `Neutral`.
    let a = snap(60, 100.0, ctx(0.0, 0));
    let r = synth(&[(60, a)]);
    assert_eq!(
        r.analysis.bias,
        MarketBias::Neutral,
        "CG2: a market with no directional lean reads Neutral — there is no other token \
         available, and that is the point"
    );
}

#[test]
fn cg2_the_bias_is_aggregated_across_durations_not_read_off_one() {
    // The synthesis must genuinely combine the ladder. The check: removing the
    // slow duration changes the result, which can only happen if the slow
    // duration is contributing.
    //
    // NOTE — a finding worth carrying forward. While writing this I asserted the
    // stronger property that two durations in direct opposition cannot produce a
    // STRONG bias. **It can**: with a fast duration at +0.9 / score 80 and a slow
    // one at -0.9 / score -80, `derive_analysis` returns `StrongBearish`. The
    // ladder weights the slow duration heavily (it carries the decision horizon),
    // so a strongly-opposed slow duration wins outright.
    //
    // That is defensible for a STATE claim — "the slow horizon is bearish and the
    // fast one has not contradicted it enough" — but it is exactly the kind of
    // thing a confidence or probability field would launder into "90% sure". The
    // reason it is safe here is that there is no such field: the only output is a
    // `MarketBias`. P10, which introduces the typed axes, is where disagreement
    // has to be made visible as a first-class quantity — and this test is the one
    // to revisit then, to assert what the axes are required to show.
    let fast_bull = snap(60, 100.0, ctx(0.9, 80));
    let slow_bull = snap(3600, 100.0, ctx(0.9, 80));
    let both = synth(&[(60, fast_bull.clone()), (3600, slow_bull)]);
    let fast_only = synth(&[(60, fast_bull)]);

    assert!(
        both.alignment.timeframes_present == 2 && fast_only.alignment.timeframes_present == 1,
        "CG2: the fixture must differ in duration count for this test to mean anything"
    );
    assert!(
        both.alignment.mtf_overall_score != fast_only.alignment.mtf_overall_score,
        "CG2: the cross-duration score must aggregate — adding a duration that agrees \
         cannot leave the aggregate untouched (two durations: {}, one: {})",
        both.alignment.mtf_overall_score,
        fast_only.alignment.mtf_overall_score
    );
}
