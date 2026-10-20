//! CG3 — the far side of the model boundary.
//!
//! The companion to `crates/market-analyzer/tests/model_boundary.rs`, which pins
//! the near side (MME emits state and no verdict). This file pins what TAE does
//! with that state: **nothing**.
//!
//! The setup executor used to select the best setup out of
//! `snapshot.opportunity` (entry / target / invalidation zones, R:R, the evaluated
//! profiles) and `snapshot.decision_context` (a `trade_readiness` gate). Both were
//! L4/L6 strategy and neither crosses the boundary any more — so the two inputs
//! the derivation needs are gone, and no derivation is possible from a live
//! snapshot.
//!
//! The consequence is stated plainly here rather than left to be discovered: the
//! setup executor has no setups in this build. Paper and live are WIP and not
//! selectable, which is precisely why the boundary could be drawn here. Restoring
//! the executor means computing setups inside TAE from model state — future TAE
//! work, and not something this phase pretends to have done.
//!
//! These tests are written to FAIL when the executor starts working again. That
//! is the correct direction for them to fail: a restored executor would need its
//! own tests, written against model state, rather than silently resuming the old
//! behaviour behind a passing suite.

use rust_decimal::Decimal;

use core_domain::alignment::AlignmentMatrix;
use core_domain::analysis::AnalysisMatrix;
use core_domain::models::MarketSnapshot;
use portfolio_supervisor::setup_executor::extract_top_setup;

fn model_snapshot() -> MarketSnapshot {
    let mut a = AnalysisMatrix::empty("BTC-USDC");
    a.bias = core_domain::analysis::MarketBias::StrongBullish;
    let mut m = AlignmentMatrix::empty("BTC-USDC");
    m.mtf_overall_score = 88.0;
    m.mtf_stance = core_domain::alignment::CrossTfStance::StrongBull;
    MarketSnapshot {
        symbol: "BTC-USDC".into(),
        timeframe_secs: 60,
        mid_price: Decimal::from_f64_retain(100.0).unwrap(),
        close: Decimal::from_f64_retain(100.0),
        alignment: Some(m),
        analysis: Some(a),
        timeframe_state: None,
        ..MarketSnapshot::default()
    }
}

#[test]
fn cg3_a_fully_resolved_model_state_yields_no_setup() {
    // The strongest possible model input: a StrongBullish bias and near-unanimous
    // cross-duration agreement. The old executor would have emitted a long setup
    // off the top qualifying profile. It cannot now, because the profiles it read
    // are not on the wire — and that is correct, not a defect: a strong bias is
    // not a trade instruction, and an engine that turned it into one would be
    // trading on the model's opinion rather than on TAE's own analysis.
    let s = model_snapshot();
    let snapshots = vec![&s];
    assert!(
        extract_top_setup(&snapshots, 0.5).is_none(),
        "CG3: a resolved model state must not become a setup"
    );
}

#[test]
fn cg3_no_input_shape_yields_a_setup() {
    let s = model_snapshot();
    let refs = vec![&s];
    for min_rr in [0.0, 0.5, 2.0, f64::NAN] {
        assert!(
            extract_top_setup(&refs, min_rr).is_none(),
            "CG3: min_net_rr={min_rr} must not change the outcome — the parameter is \
             accepted for signature stability only"
        );
    }
    assert!(extract_top_setup(&[], 0.5).is_none());
}
