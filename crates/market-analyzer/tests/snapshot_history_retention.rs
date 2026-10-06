//! v11.12.24 (memory): retained-snapshot retention contract.
//!
//! The daemon was OOM-killed by the kernel at ~1.8–2.1 GB RSS on a 2.8 GB
//! host. Root cause: `TimeframePipeline.snapshot_history` retained up to
//! `HIST_BUFFER_MAX = 1000` **full** completed `MarketSnapshot`s per
//! (instance × duration). A completed frame carries eight synthesis
//! matrices, the cluster matrix, the liquidity flow (whose
//! `recent_real_buckets` map alone reaches 2 000 entries at the shipped
//! 24 h retention), a 100-bin volume profile, the 52-entry lifecycle map
//! and per-indicator signals — measured at 54.9 KB of JSON per frame on
//! the shipped corpus — while `/api/history` reads only `timestamp`,
//! OHLCV, `quality_envelope.is_gap_filled` and
//! `indicators.{raw_value, normalized, state_label, values}`.
//!
//! Three invariants are pinned here:
//!   1. **Projection** — the retained form drops every field no reader
//!      touches, and keeps every field a reader does.
//!   2. **Retention** — the retained window is rolled at
//!      `[candle_buffer].size` (500), the depth CB-02 has always
//!      documented, not at the 1 000 absolute cap.
//!   3. **Budget** — a process-wide byte ceiling evicts oldest-first down
//!      to a 50-entry floor, and the accounting is exact (every push
//!      charges, every pop refunds) so it cannot drift.

use std::collections::VecDeque;

use core_domain::indicator_dtos::{
    IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus,
};
use core_domain::liquidity::{LiquidityFlow, RealLiquidationBucket};
use core_domain::models::{CandlePipelineState, MarketSnapshot};
use core_domain::normalized::LiquidationSide;
use core_domain::volume_profile::{VolumeProfileBin, VolumeProfileSnapshot};
use market_analyzer::analyzer::history_budget as budget;
use rust_decimal::Decimal;

// ── helpers ─────────────────────────────────────────────────────

/// The retained-snapshot budget is **process-global by design** (one
/// accounting surface for every pipeline, so N pipelines cannot each hold a
/// private counter that ignores the others). Every test that reads or
/// mutates that shared state must therefore hold this lock — otherwise the
/// parallel test harness interleaves `configure`/`push_retained` calls and
/// the usage assertions compare readings from different configurations.
/// Pure-projection tests need no lock.
fn budget_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reset to the shipped production configuration after a test that mutates it.
fn reset_budget() {
    budget::configure(
        config_models::CandleBufferConfig::default().size,
        config_models::CandleBufferConfig::default().max_snapshot_history_bytes,
    );
}

// ── fixtures ────────────────────────────────────────────────────

/// A completed-frame-shaped snapshot: full indicator map, matrices,
/// cluster, liquidity buckets, volume profile, lifecycle map.
fn full_snapshot(timestamp: u64) -> MarketSnapshot {
    let mut snap = MarketSnapshot {
        timeframe_secs: 5,
        timestamp,
        symbol: "BTC-USDT".to_string(),
        is_completed: Some(true),
        open: Some(Decimal::from(100u64)),
        high: Some(Decimal::from(101u64)),
        low: Some(Decimal::from(99u64)),
        close: Some(Decimal::from(100u64)),
        volume: Some(Decimal::from(10u64)),
        mid_price: Decimal::from(100u64),
        pipeline_state: CandlePipelineState::Live,
        quality_envelope: Some(core_domain::models::CandleQualityEnvelope {
            quality_score: 95.0,
            is_valid: true,
            is_gap_filled: false,
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            sequence_integrity: core_domain::models::SequenceIntegrity::Valid,
            gap_since_last: 0,
            validated_at: timestamp * 1000,
        }),
        ..Default::default()
    };

    // 52-indicator-shaped map with multi-line `values` sub-maps and
    // per-bar `signals` — the shape `/api/history` actually walks.
    for i in 0..52 {
        let key = format!("indicator_{i:02}");
        let mut values = std::collections::HashMap::new();
        for line in ["fast", "medium", "slow", "long"] {
            values.insert(line.to_string(), i as f64 + line.len() as f64);
        }
        snap.indicators.insert(
            key.clone(),
            NormalizedIndicatorValue {
                raw_value: i as f64,
                normalized: (i as f64 / 52.0) * 2.0 - 1.0,
                state_label: "ESTABLISHED".to_string(),
                values: Some(values),
                signals: vec![IndicatorSignal::new(
                    SignalKind::Crossover,
                    SignalDirection::Bullish,
                    SignalStatus::Active,
                    "crossover",
                )],
                confidence: 0.87,
            },
        );
        snap.indicator_lifecycle.insert(
            key,
            core_domain::indicator_dtos::IndicatorLifecycleStatus::live(300, 300, 0, 300, false),
        );
    }

    // The eight synthesis matrices + statistical context.
    let dim = core_domain::market_context::ContextDimension {
        score: 0.0,
        confidence: 0.0,
        label: "NEUTRAL".into(),
    };
    snap.context = Some(core_domain::market_context::MarketContext {
        trend: dim.clone(),
        momentum: dim.clone(),
        volatility: dim.clone(),
        volume: dim.clone(),
        liquidity: dim,
        regime: "TRENDING".into(),
        overall_score: 50,
        overall_label: "NEUTRAL".into(),
    });
    snap.alignment = Some(core_domain::alignment::AlignmentMatrix::empty("BTC-USDT"));
    snap.analysis = Some(core_domain::analysis::AnalysisMatrix::empty("BTC-USDT"));
    snap.risk = Some(core_domain::risk::RiskMatrix::empty("BTC-USDT"));
    snap.advisory = Some(core_domain::advisory::AdvisoryMatrix::empty("BTC-USDT"));
    snap.opportunity = Some(core_domain::opportunity::OpportunityMatrix {
        symbol: "BTC-USDT".into(),
        opportunity_score: 60.0,
        long_entry_zone: core_domain::analysis::PriceRange {
            low: 90.0,
            high: 100.0,
        },
        long_target_zone: core_domain::analysis::PriceRange {
            low: 120.0,
            high: 130.0,
        },
        long_invalidation_level: 85.0,
        long_expected_rr_internal: 2.0,
        ..Default::default()
    });
    snap.decision_context = Some(core_domain::decision_context::DecisionContext {
        score: 45.0,
        bias: "Bullish".into(),
        score_confidence: 0.68,
        entry_danger: core_domain::risk::RiskDimension::from_score(35.0),
        expected_reward_risk_ratio: 2.1,
        trade_readiness: "WATCH".into(),
        contributing_indicators: vec!["rsi".into(), "adx".into()],
        long_probability: 0.0,
        short_probability: 0.0,
        hold_probability: 0.0,
        net_bias_pct: 0.0,
        lean_floor_applied: false,
    });
    snap.statistical_context = Some(core_domain::models::StatisticalContext {
        close_z: Some(0.4),
        rsi_z: Some(-0.2),
        macd_z: Some(0.9),
        monte_carlo_expected: Some(0.01),
        monte_carlo_stdev: Some(0.03),
    });
    snap.metrics_config = Some(Default::default());

    // Liquidity flow with a realistically-sized bucket map.
    let mut flow = LiquidityFlow::default();
    for b in 0..2000i64 {
        flow.recent_real_buckets.insert(
            b,
            RealLiquidationBucket {
                bucket_index: b,
                side: LiquidationSide::Short,
                price_low: 99.0,
                price_high: 100.0,
                peak_price: 99.5,
                notional_usd: 1_000.0,
                event_count: 3,
                last_updated_ms: 1_700_000_000_000,
            },
        );
    }
    snap.liquidity = Some(flow);
    snap.liquidity_signals = Vec::new();

    // 100-bin volume profile.
    snap.volume_profile = Some(VolumeProfileSnapshot {
        symbol: "BTC-USDT".to_string(),
        timeframe_label: "5s".to_string(),
        timeframe_secs: 5,
        bins: (0..100)
            .map(|i| VolumeProfileBin {
                price_low: i as f64,
                price_high: i as f64 + 1.0,
                volume: 1.0,
                buy_volume: 0.5,
                sell_volume: 0.5,
                is_poc: i == 50,
                is_value_area: (25..=75).contains(&i),
            })
            .collect(),
        poc_price: 50.0,
        value_area_high: 75.0,
        value_area_low: 25.0,
        total_volume: 100.0,
        range_low: 0.0,
        range_high: 100.0,
        num_bins: 100,
        timestamp_ms: 1_700_000_000_000,
    });

    snap
}

// ── Pin #1: projection keeps what /api/history reads ──────────────

#[test]
fn projection_keeps_every_field_history_reads() {
    let full = full_snapshot(1_700_000_000);
    let p = full.history_projection();

    // `serve_history` reads exactly these (handlers/history.rs + types.rs).
    assert_eq!(p.timestamp, full.timestamp);
    assert_eq!(p.symbol, full.symbol);
    assert_eq!(p.timeframe_secs, full.timeframe_secs);
    assert_eq!(p.open, full.open);
    assert_eq!(p.high, full.high);
    assert_eq!(p.low, full.low);
    assert_eq!(p.close, full.close);
    assert_eq!(p.volume, full.volume);
    assert_eq!(p.average_volume, full.average_volume);
    // The gap-fill provenance flag the chart's `candleReconstructed`
    // filter depends on.
    assert_eq!(
        p.quality_envelope.as_ref().map(|q| q.is_gap_filled),
        full.quality_envelope.as_ref().map(|q| q.is_gap_filled),
    );

    // Indicator identity + the four fields `push_value` reads.
    assert_eq!(p.indicators.len(), full.indicators.len());
    assert_eq!(
        p.indicators
            .keys()
            .collect::<std::collections::HashSet<_>>(),
        full.indicators
            .keys()
            .collect::<std::collections::HashSet<_>>()
    );
    for (k, v) in full.indicators.iter() {
        let pv = p.indicators.get(k).expect("every indicator key survives");
        assert_eq!(pv.raw_value, v.raw_value, "{k} raw_value");
        assert_eq!(pv.normalized, v.normalized, "{k} normalized");
        assert_eq!(pv.state_label, v.state_label, "{k} state_label");
        assert_eq!(
            pv.values.as_ref().map(|m| m.len()),
            v.values.as_ref().map(|m| m.len()),
            "{k} values sub-key count"
        );
        if let (Some(a), Some(b)) = (pv.values.as_ref(), v.values.as_ref()) {
            for line in a.keys() {
                assert_eq!(a.get(line), b.get(line), "{k}.{line}");
            }
        }
    }
}

// ── Pin #2: projection drops everything no reader touches ────────

#[test]
fn projection_drops_every_unread_field() {
    let full = full_snapshot(1_700_000_000);
    let p = full.history_projection();

    assert!(p.alignment.is_none());
    assert!(p.analysis.is_none());
    assert!(p.risk.is_none());
    assert!(p.advisory.is_none());
    assert!(p.opportunity.is_none());
    assert!(p.decision_context.is_none());
    assert!(p.context.is_none());
    assert!(p.statistical_context.is_none());
    assert!(p.cluster.is_none());
    assert!(p.liquidity.is_none());
    assert!(p.volume_profile.is_none());
    assert!(p.metrics_config.is_none());
    assert!(p.liquidity_signals.is_empty());
    assert!(p.indicator_lifecycle.is_empty());
    assert!(p.open_interest.is_none());
    assert!(p.oi_delta_pct.is_none());
    assert!(p.mark_price.is_none());
    assert!(p.index_price.is_none());
    assert!(p.funding_rate.is_none());

    // Per-indicator signals + confidence are never read from the retained
    // window (only `raw_value`/`normalized`/`state_label`/`values` are).
    for (k, v) in p.indicators.iter() {
        assert!(v.signals.is_empty(), "{k} signals must be dropped");
        assert_eq!(v.confidence, 0.0, "{k} confidence must be dropped");
    }

    // The full snapshot still carries them — the LIVE surface is untouched.
    assert!(full.advisory.is_some());
    assert!(full.liquidity.is_some());
    assert!(!full.indicators.values().next().unwrap().signals.is_empty());
}

// ── Pin #3: the projection is small ──────────────────────────────

/// Calibrated against the shipped DS-export corpus, where 176 completed
/// AEON-USDT 5 s frames measured 54.9 KB full / 16.0 KB matrix-stripped /
/// 8.4 KB with signals stripped. A 5× ceiling leaves generous headroom for
/// fixture shape while still failing loudly if a future change re-attaches a
/// heavyweight field to the retained copy.
#[test]
fn projection_is_at_most_a_fifth_of_the_full_frame() {
    let full = full_snapshot(1_700_000_000);
    let full_bytes = serde_json::to_string(&full).unwrap().len();
    let proj_bytes = serde_json::to_string(&full.history_projection())
        .unwrap()
        .len();
    assert!(
        proj_bytes * 5 <= full_bytes,
        "projected frame {proj_bytes}B must be <= 1/5 of the full {full_bytes}B"
    );
}

// ── Pin #4: retention is rolled at candle_buffer.size (500) ───────

/// Guards the number against the `[candle_buffer].size` default the
/// `CandleBufferConfig` serde default supplies, and against the documented
/// `SNAPSHOT_HISTORY_MAX` ceiling.
#[test]
fn retention_matches_the_documented_candle_buffer_tier() {
    let _guard = budget_guard();
    budget::configure(budget::DEFAULT_SNAPSHOT_HISTORY_RETENTION, 0);
    assert_eq!(budget::retention(), 500);
    assert_eq!(
        budget::DEFAULT_SNAPSHOT_HISTORY_RETENTION,
        config_models::CandleBufferConfig::default().size,
        "retention default must equal the [candle_buffer].size serde default"
    );
    // Retention can never exceed the absolute in-memory cap.
    budget::configure(99_999, 0);
    assert_eq!(budget::retention(), budget::SNAPSHOT_HISTORY_MAX);
    reset_budget();
}

#[test]
fn push_retained_rolls_the_window_at_the_retention_cap() {
    let _guard = budget_guard();
    budget::configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..600u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(
        deque.len(),
        500,
        "retained window must stop at the retention cap, never grow past it"
    );
    // Oldest evicted, newest kept, and the ORDER is preserved (FIFO).
    assert_eq!(
        deque.front().unwrap().timestamp,
        1_700_000_000 + 100,
        "the 100 oldest of 600 pushes must be evicted"
    );
    assert_eq!(deque.back().unwrap().timestamp, 1_700_000_000 + 599);
    let timestamps: Vec<u64> = deque.iter().map(|s| s.timestamp).collect();
    assert!(
        timestamps.windows(2).all(|w| w[0] < w[1]),
        "retained window must stay chronologically ascending"
    );
}

#[test]
fn push_retained_stores_the_projection_not_the_full_frame() {
    let _guard = budget_guard();
    budget::configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    budget::push_retained(&mut deque, full_snapshot(1_700_000_000));
    let stored = deque.back().unwrap();
    assert!(stored.advisory.is_none(), "matrices must not be retained");
    assert!(
        stored.liquidity.is_none(),
        "bucket map must not be retained"
    );
    assert_eq!(stored.indicators.len(), 52, "indicators must be retained");
}

// ── Pin #5: the byte budget evicts oldest-first, floor-respecting ─

#[test]
fn byte_budget_evicts_oldest_first_and_stops_at_the_floor() {
    let _guard = budget_guard();
    budget::configure(500, 1); // 1 byte ⇒ always over budget
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..budget::SNAPSHOT_HISTORY_FLOOR as u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(
        deque.len(),
        budget::SNAPSHOT_HISTORY_FLOOR,
        "eviction must stop at the floor so a chart always has something to render"
    );
    // Whatever survived must be the NEWEST slice.
    let newest = deque.back().unwrap().timestamp;
    assert_eq!(
        newest,
        1_700_000_000 + budget::SNAPSHOT_HISTORY_FLOOR as u64 - 1
    );
    reset_budget();
}

#[test]
fn byte_budget_zero_disables_eviction() {
    let _guard = budget_guard();
    budget::configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..120u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(
        deque.len(),
        120,
        "max_bytes = 0 must rely on the per-duration cap alone"
    );
}

// ── Pin #6: the accounting is exact (charge/refund invariant) ────

#[test]
fn accounting_returns_to_zero_after_a_drain() {
    let _guard = budget_guard();
    budget::configure(500, 0);
    let baseline = budget::usage_bytes();
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..200u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    let charged = budget::usage_bytes();
    let summed: usize = deque.iter().map(|s| s.history_weight_bytes()).sum();
    assert_eq!(
        charged - baseline,
        summed,
        "global usage must equal the summed weight of the live entries"
    );
    budget::drain(&mut deque);
    assert!(
        deque.is_empty(),
        "drain must empty the window (instance delete / recharge)"
    );
    assert_eq!(
        budget::usage_bytes(),
        baseline,
        "drain must refund every charged byte — otherwise a delete/recharge \
         cycle leaks budget credit and evicts live history for nothing"
    );
}

#[test]
fn count_trim_refunds_the_evicted_entries() {
    let _guard = budget_guard();
    budget::configure(50, 0);
    let baseline = budget::usage_bytes();
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..50u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    let at_cap = budget::usage_bytes() - baseline;
    assert!(at_cap > 0);
    for i in 50..70u64 {
        budget::push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    // 20 pushes each evicting one entry must leave the usage flat.
    assert_eq!(
        budget::usage_bytes() - baseline,
        at_cap,
        "a steady-state window must not accumulate usage"
    );
    budget::drain(&mut deque);
    assert_eq!(budget::usage_bytes(), baseline);
    reset_budget();
}

// ── Pin #7: the estimate is monotone in the payload it describes ──

#[test]
fn weight_estimate_orders_projection_below_full_frame() {
    let full = full_snapshot(1_700_000_000);
    assert!(
        full.history_weight_bytes() > full.history_projection().history_weight_bytes(),
        "the estimator must see the retained form as cheaper than the live form"
    );
    // 2 000 liquidity buckets must dominate the full-frame estimate —
    // this is the term that actually put the daemon over the OOM line.
    let mut no_buckets = full.clone();
    no_buckets.liquidity = None;
    assert!(
        full.history_weight_bytes() > no_buckets.history_weight_bytes(),
        "the bucket map must be weighted, not free"
    );
}
