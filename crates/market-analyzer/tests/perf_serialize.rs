//! AC-L4-1 (per-frame serialization p95 < 1ms under nominal load).
//!
//! Verifies that `MarketSnapshot` serialization to JSON completes in under 1ms
//! for a typical payload with 52 indicators and sub-matrices populated.
//! Uses a generous 50ms threshold to account for debug/test-profile overhead
//! in CI environments; in optimized release builds this is well under 1ms.

use std::collections::HashMap;
use std::time::Instant;

use core_domain::advisory::AdvisoryMatrix;
use core_domain::alignment::AlignmentMatrix;
use core_domain::analysis::AnalysisMatrix;
use core_domain::decision_context::DecisionContext;
use core_domain::indicator_dtos::NormalizedIndicatorValue;
use core_domain::market_context::{ContextDimension, MarketContext};
use core_domain::models::{
    CandleQualityEnvelope, MarketSnapshot, SequenceIntegrity, TimeframeSlot,
};
use core_domain::normalized::Exchange;
use core_domain::risk::RiskMatrix;
use rust_decimal_macros::dec;

const INDICATOR_KEYS: &[&str] = &[
    "rsi",
    "macd",
    "adx",
    "atr",
    "squeeze",
    "ema_stack",
    "vwap",
    "fibonacci",
    "patterns",
    "support_resistance",
    "stochastic",
    "chandemo",
    "supertrend",
    "keltner",
    "donchian",
    "obv",
    "cmf",
    "mfi",
    "hv",
    "aroon",
    "choppiness",
    "linreg_slope",
    "zscore",
    "bollinger",
    "bbwp",
    "rvol",
    "ichimoku",
    "cci",
    "psar",
    "williams_r",
    "hull_ma",
    "force_index",
    "stddev_channel",
    "smc",
    "volume_profile",
    "market_facilitation",
    "elder_ray",
    "dema",
    "tema",
    "trix",
    "mass_index",
    "kst",
    "tsi",
    "uo",
    "adosc",
    "natr",
    "trange",
    "typprice",
    "medprice",
    "wcprice",
];

fn neutral_dimension() -> ContextDimension {
    ContextDimension {
        score: 0.0,
        confidence: 0.0,
        label: "NEUTRAL".into(),
    }
}

fn build_realistic_snapshot() -> MarketSnapshot {
    let mut indicators: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
    for (i, key) in INDICATOR_KEYS.iter().enumerate() {
        let norm = (i as f64 - 25.0) / 25.0;
        indicators.insert(
            key.to_string(),
            NormalizedIndicatorValue::scalar(i as f64 * 10.0, norm.clamp(-1.0, 1.0), "NEUTRAL"),
        );
    }

    let dim = neutral_dimension();
    let context = MarketContext {
        trend: dim.clone(),
        momentum: dim.clone(),
        volatility: dim.clone(),
        volume: dim.clone(),
        liquidity: dim.clone(),
        regime: "TRENDING".into(),
        overall_score: 50,
        overall_label: "BULLISH".into(),
    };

    let decision = DecisionContext {
        score: 45.0,
        bias: "Bullish".into(),
        score_confidence: 0.68,
        entry_danger: core_domain::risk::RiskDimension::from_score(35.0),
        expected_reward_risk_ratio: 2.1,
        trade_readiness: "WATCH".into(),
        contributing_indicators: vec!["rsi".into(), "macd".into(), "adx".into()],
        long_probability: 0.0,
        short_probability: 0.0,
        hold_probability: 0.0,
        net_bias_pct: 0.0,
        lean_floor_applied: false,
    };

    MarketSnapshot {
        timeframe_slot: Some(TimeframeSlot::Micro1),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1700000000,
        symbol: "BTC-USDT".into(),
        is_completed: Some(true),
        mid_price: dec!(50000.0),
        bid_price: dec!(49995.0),
        ask_price: dec!(50005.0),
        bid_size: Some(dec!(1.5)),
        ask_size: Some(dec!(2.0)),
        funding_rate: Some(dec!(0.0001)),
        open: Some(dec!(49800.0)),
        high: Some(dec!(50200.0)),
        low: Some(dec!(49750.0)),
        close: Some(dec!(50000.0)),
        volume: Some(dec!(100.0)),
        average_volume: Some(dec!(85.0)),
        indicators,
        context: Some(context),
        alignment: Some(AlignmentMatrix::empty("BTC-USDT")),
        analysis: Some(AnalysisMatrix::empty("BTC-USDT")),
        risk: Some(RiskMatrix::empty("BTC-USDT")),
        advisory: Some(AdvisoryMatrix::empty("BTC-USDT")),
        open_interest: Some(dec!(15000000.0)),
        oi_delta_1h: Some(dec!(500000.0)),
        mark_price: Some(dec!(50002.0)),
        index_price: Some(dec!(50000.0)),
        mark_index_spread_pct: Some(0.004),
        prev_day_px: Some(dec!(49500.0)),
        statistical_context: None,
        decision_context: Some(decision),
        opportunity: None,
        liquidity_signals: vec![],
        metrics_config: None,
        risk_profile: Some(1),
        liquidity: None,
        cluster: None,
        volume_profile: None,
        quality_envelope: Some(CandleQualityEnvelope {
            quality_score: 95.0,
            is_valid: true,
            is_gap_filled: false,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: SequenceIntegrity::Valid,
            gap_since_last: 60,
            validated_at: 1700000000000,
        }),
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    }
}

#[test]
fn per_frame_serialization_p95_under_threshold() {
    let snapshot = build_realistic_snapshot();

    let json = serde_json::to_string(&snapshot).expect("serialization must succeed");
    assert!(!json.is_empty(), "serialized payload must be non-empty");

    let mut sum_ns: u64 = 0;
    for _ in 0..10 {
        let start = Instant::now();
        let _ = serde_json::to_string(&snapshot).unwrap();
        sum_ns += start.elapsed().as_nanos() as u64;
    }

    const ITERATIONS: usize = 200;
    // AUDIT-TEST: the documented SLA (AC-L4-1) is p95 < 1 ms; the old
    // test asserted 50 ms — 50× the contract. Enforce the documented SLA
    // in release builds; keep a generous debug-build allowance.
    #[cfg(debug_assertions)]
    const THRESHOLD_MS: f64 = 50.0;
    #[cfg(not(debug_assertions))]
    const THRESHOLD_MS: f64 = 1.0;

    let mut times: Vec<f64> = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = serde_json::to_string(&snapshot).unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        times.push(elapsed);
    }

    times.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((ITERATIONS as f64) * 0.95).ceil() as usize - 1;
    let p95 = times[p95_idx];
    let min = times[0];
    let max = times[ITERATIONS - 1];
    let median = times[ITERATIONS / 2];

    eprintln!(
        "Serialization stats (ms): min={min:.4} median={median:.4} p95={p95:.4} max={max:.4} warmup_ns={}",
        sum_ns / 10
    );
    eprintln!("Serialized payload size: {} bytes", json.len());

    assert!(
        p95 < THRESHOLD_MS,
        "p95 serialization time ({p95:.4}ms) must be below {THRESHOLD_MS}ms"
    );
}
