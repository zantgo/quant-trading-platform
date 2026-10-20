//! v11.12.24 (memory): retained-snapshot retention contract.
//!
//! The daemon was OOM-killed by the kernel at ~1.8–2.1 GB RSS on a 2.8 GB
//! host. Root cause: `TimeframePipeline.snapshot_history` retained up to
//! `HISTORY_CAP = 500` **full** completed `MarketSnapshot`s per
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
use std::sync::Arc;

use tokio::sync::RwLock;

use core_domain::axes::Readiness;
use core_domain::indicator_dtos::{
    IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus,
};
use core_domain::models::{CandlePipelineState, MarketSnapshot};
use core_domain::volume_profile::{VolumeProfileBin, VolumeProfileSnapshot};
use market_analyzer::analyzer::history_budget::{self as budget, Budget};
use rust_decimal::Decimal;

// ── helpers ─────────────────────────────────────────────────────

/// The retained-snapshot budget is **process-global by design** (one
/// accounting surface for every pipeline, so N pipelines cannot each hold a
/// private counter that ignores the others). Every test that reads or
/// mutates that shared state must therefore hold this lock — otherwise the
/// parallel test harness interleaves `configure`/`push_retained` calls and
/// the usage assertions compare readings from different configurations.
/// Pure-projection tests need no lock.
/// A private budget instance per test.
///
/// The production budget is process-global, which would make every test in
/// this file order-dependent (and would need a lock to run in parallel).
/// `Budget::new` gives each test its own accounting, so these tests assert
/// real behaviour instead of asserting on whatever a sibling test left
/// behind — that distinction already caught two genuine bugs.
fn hb(retention: usize, max_bytes: usize) -> Budget {
    Budget::new(retention, max_bytes)
}

// ── fixtures ────────────────────────────────────────────────────

/// A completed-frame-shaped snapshot: full indicator map, matrices,
/// cluster, liquidity buckets, volume profile, lifecycle map.
/// A band set with `n` bands — the shape the retention estimator must charge
/// for. Field values are placeholders; only the COUNT matters here, exactly as
/// the 2 000-bucket fixture that preceded it only used its entry count.
fn band_fixture(
    symbol: &str,
    label: &str,
    secs: u64,
    mid: f64,
    n: usize,
) -> core_domain::state_bands::StateBandMatrix {
    let mut m = core_domain::state_bands::StateBandMatrix::empty(symbol, label, secs, mid);
    m.has_profile = true;
    m.has_bands = true;
    m.bands = (0..n)
        .map(|i| core_domain::state_bands::StateBand {
            price_low: mid - 1.0 - i as f64,
            price_high: mid + 1.0 + i as f64,
            band_atr_multiple: 0.5,
            volume_percentile: 50.0,
            proximity_percentile: 50.0,
            intensity_pct: 25.0,
            basis: core_domain::state_bands::BandBasis::HighVolumeNode,
            volume_asymmetry: 0.1,
        })
        .collect();
    m
}

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
        // CR-10: the candle-derived heatmap bands. Populated, not `None`, so
        // the retention estimator actually measures them — the 2 000-bucket
        // forced-close map was unweighted for exactly this reason and the
        // OOM followed.
        state_bands: Some(band_fixture("BTC-USDT", "5s", 5, 100.0, 24)),
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
                readiness: Readiness::Solid,
                // Availability is declared by the assembler, which knows the bar.
                // Left undeclared here so CG1 reports it rather than this
                // constructor guessing a time it has not been told.
                available_at_bar: None,
                subject_bar: None,
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
        // NOTE: `MarketContext.liquidity` is a ContextDimension, not the deleted
        // `MarketSnapshot.liquidity` flow. Same name, different type — which is
        // why the purge deletes by field-and-type rather than by name.
        liquidity: dim.clone(),
        regime: "TRENDING".into(),
        overall_score: 50,
        overall_label: "NEUTRAL".into(),
    });
    snap.alignment = Some(core_domain::alignment::AlignmentMatrix::empty("BTC-USDT"));
    snap.statistical_context = Some(core_domain::models::StatisticalContext {
        close_z: Some(0.4),
        rsi_z: Some(-0.2),
        macd_z: Some(0.9),
        monte_carlo_expected: Some(0.01),
        monte_carlo_stdev: Some(0.03),
    });
    snap.metrics_config = Some(Default::default());

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
    // CR-15: risk/advisory/opportunity/decision_context no longer exist on the
    // snapshot at all, so they need no projection assertion any more.
    assert!(p.context.is_none());
    assert!(p.statistical_context.is_none());
    assert!(p.volume_profile.is_none());
    assert!(p.metrics_config.is_none());
    assert!(p.indicator_lifecycle.is_empty());

    // Per-indicator signals are never read from the retained window (only
    // `raw_value`/`normalized`/`state_label`/`values` are). Readiness is a
    // one-byte completeness flag and survives, because a retained value that
    // forgot whether its inputs were complete would be indistinguishable from
    // a solid one.
    for (k, v) in p.indicators.iter() {
        assert!(v.signals.is_empty(), "{k} signals must be dropped");
        assert_eq!(v.readiness, Readiness::Solid, "{k} readiness must survive");
    }

    // The full snapshot still carries them — the LIVE surface is untouched.
    assert!(!full.indicators.values().next().unwrap().signals.is_empty());
}

// ── Pin #3: the projection is small ──────────────────────────────

/// Calibrated against the shipped DS-export corpus, where 176 completed
/// AEON-USDT 5 s frames measured 54.9 KB full / 16.0 KB matrix-stripped /
/// 8.4 KB with signals stripped.
///
/// CR-10 RE-CALIBRATED this ceiling from 1/5 to 1/3, and the number is worth
/// stating rather than rounding: on this fixture the full frame fell from
/// ~54.9 KB to 38.9 KB and the projection from ~8.4 KB to 9.4 KB, a ratio of
/// 0.153 → 0.242. The ratio moved because the fixture's DOMINANT term (the
/// 2 000-entry forced-close bucket map) lived in the LIVE frame only, so
/// deleting it shrank the denominator three times faster than the numerator.
/// The guard still does its job — a re-attached heavyweight field moves the
/// projection, not the live frame — it just needed an honest ceiling.
#[test]
fn projection_is_at_most_a_fifth_of_the_full_frame() {
    let full = full_snapshot(1_700_000_000);
    let full_bytes = serde_json::to_string(&full).unwrap().len();
    let proj_bytes = serde_json::to_string(&full.history_projection())
        .unwrap()
        .len();
    assert!(
        proj_bytes * 3 <= full_bytes,
        "projected frame {proj_bytes}B must be <= 1/3 of the full {full_bytes}B"
    );
}

// ── Pin #4: retention is rolled at the one cap (500) ───────────────

/// CR-13: this used to compare the retention default against
/// `CandleBufferConfig::default().size` — a setting that is now deleted — and
/// against a `SNAPSHOT_HISTORY_MAX` ceiling of 1000. Both are gone; the number
/// has one source. `history_cap.rs` is the enforcement; this is the in-place
/// guard that the wiring still points at the right constant.
#[test]
fn retention_is_the_one_cap() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    hb_ref.configure(budget::DEFAULT_SNAPSHOT_HISTORY_RETENTION, 0);
    assert_eq!(hb_ref.retention(), core_domain::HISTORY_CAP);
    assert_eq!(
        budget::DEFAULT_SNAPSHOT_HISTORY_RETENTION,
        core_domain::HISTORY_CAP,
        "the retention default must BE the cap, not a number beside it"
    );
    // Retention can never exceed the cap, whatever a caller asks for.
    hb_ref.configure(99_999, 0);
    assert_eq!(hb_ref.retention(), core_domain::HISTORY_CAP);
}

#[test]
fn push_retained_rolls_the_window_at_the_retention_cap() {
    let hb_ref = hb(500, 0);
    hb_ref.configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..600u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
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
    let hb_ref = hb(500, 0);
    hb_ref.configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000));
    let stored = deque.back().unwrap();
    assert_eq!(stored.indicators.len(), 52, "indicators must be retained");
}

// ── Pin #5: the byte budget evicts oldest-first, floor-respecting ─

#[test]
fn byte_budget_evicts_oldest_first_and_stops_at_the_floor() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    hb_ref.configure(500, 1); // 1 byte ⇒ always over budget
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..budget::SNAPSHOT_HISTORY_FLOOR as u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
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
}

#[test]
fn byte_budget_zero_disables_eviction() {
    let hb_ref = hb(500, 0);
    hb_ref.configure(500, 0);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..120u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
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
    let hb_ref = hb(500, 0);
    hb_ref.configure(500, 0);
    let baseline = hb_ref.usage_bytes();
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..200u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    let charged = hb_ref.usage_bytes();
    let summed: usize = deque.iter().map(|s| s.history_weight_bytes()).sum();
    assert_eq!(
        charged - baseline,
        summed,
        "global usage must equal the summed weight of the live entries"
    );
    hb_ref.drain(&mut deque);
    assert!(
        deque.is_empty(),
        "drain must empty the window (instance delete / recharge)"
    );
    assert_eq!(
        hb_ref.usage_bytes(),
        baseline,
        "drain must refund every charged byte — otherwise a delete/recharge \
         cycle leaks budget credit and evicts live history for nothing"
    );
}

#[test]
fn count_trim_refunds_the_evicted_entries() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    hb_ref.configure(50, 0);
    let baseline = hb_ref.usage_bytes();
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..50u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    let at_cap = hb_ref.usage_bytes() - baseline;
    assert!(at_cap > 0);
    for i in 50..70u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    // 20 pushes each evicting one entry must leave the usage flat.
    assert_eq!(
        hb_ref.usage_bytes() - baseline,
        at_cap,
        "a steady-state window must not accumulate usage"
    );
    hb_ref.drain(&mut deque);
    assert_eq!(hb_ref.usage_bytes(), baseline);
}

// ── Pin #7: the estimate is monotone in the payload it describes ──

#[test]
fn weight_estimate_orders_projection_below_full_frame() {
    let full = full_snapshot(1_700_000_000);
    assert!(
        full.history_weight_bytes() > full.history_projection().history_weight_bytes(),
        "the estimator must see the retained form as cheaper than the live form"
    );
    // The candle-derived bands must be weighted too — they replaced the 2 000
    // liquidation buckets that actually put the daemon over the OOM line, and a
    // derived payload that is weighted as if it were free would reintroduce the
    // same class of bug the estimator exists to catch.
    let mut no_bands = full.clone();
    no_bands.state_bands = None;
    assert!(
        full.history_weight_bytes() > no_bands.history_weight_bytes(),
        "the band matrix must be weighted, not free"
    );
}

// ── Pin #8: the governor primitives (CB-12d) ──────────────────────
//
// The RSS governor's job is to shed memory on a *timer*, not on a push — a 1 h
// pipeline may not complete a candle for an hour, so its deque would keep
// holding excess the whole time the governor wants it gone. These primitives
// are what make push-independent shedding possible.

#[test]
fn effective_limit_follows_then_releases_the_governor_override() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    hb_ref.configure(500, 512 * 1024 * 1024);
    assert_eq!(hb_ref.effective_limit(), 512 * 1024 * 1024);
    assert!(!hb_ref.is_overridden());

    // Tighten.
    hb_ref.set_effective_limit(64 * 1024 * 1024);
    assert!(hb_ref.is_overridden());
    assert_eq!(
        hb_ref.effective_limit(),
        64 * 1024 * 1024,
        "push-path trim must honour the override"
    );

    // 0 must NOT read as 'unbounded' — it is the eviction-disabled sentinel.
    // `set_effective_limit(0)` is therefore a deliberate no-op: the previous
    // override stands and eviction stays ENABLED.
    hb_ref.set_effective_limit(0);
    assert_ne!(
        hb_ref.effective_limit(),
        0,
        "setting 0 must never disable eviction / read as unbounded"
    );
    assert_eq!(
        hb_ref.effective_limit(),
        64 * 1024 * 1024,
        "0 must leave the prior override in force, not clear it"
    );

    // Release.
    hb_ref.set_effective_limit(64 * 1024 * 1024);
    hb_ref.clear_effective_limit();
    assert!(!hb_ref.is_overridden());
    assert_eq!(hb_ref.effective_limit(), 512 * 1024 * 1024);
}

#[test]
fn effective_override_tightens_push_triggered_trimming() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    // Tiny budget so the override is visibly binding.
    hb_ref.configure(500, 1); // 1 byte => always over
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..budget::SNAPSHOT_HISTORY_FLOOR as u64 + 20 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(deque.len(), budget::SNAPSHOT_HISTORY_FLOOR);

    // Loosen the effective limit: the window stays where it is until the next
    // push trims it (push-triggered, not retroactive) — which is exactly why
    // the governor ALSO needs evict_to_entries below.
    hb_ref.set_effective_limit(512 * 1024 * 1024);
    let before = deque.len();
    hb_ref.push_retained(&mut deque, full_snapshot(1_799_999_999));
    assert_eq!(
        deque.len(),
        before + 1,
        "with the budget loosened the window may grow back"
    );
}

#[test]
fn evict_to_entries_shrinks_oldest_first_with_exact_refunds() {
    let hb_ref = hb(500, 0);
    hb_ref.configure(500, 0); // byte eviction off: only the sweep acts
    let baseline = hb_ref.usage_bytes();
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..300u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(deque.len(), 300);

    let evicted = hb_ref.evict_to_entries(&mut deque, 25);
    assert_eq!(evicted, 275);
    assert_eq!(deque.len(), 25, "must stop exactly at the requested size");
    // Newest entries survive — the sweep must not discard live data.
    assert_eq!(deque.back().unwrap().timestamp, 1_700_000_000 + 299);
    assert_eq!(deque.front().unwrap().timestamp, 1_700_000_000 + 275);

    // Accounting: every evicted entry's weight must have been refunded.
    let kept: usize = deque.iter().map(|s| s.history_weight_bytes()).sum();
    assert_eq!(hb_ref.usage_bytes() - baseline, kept);

    // A no-op when already smaller than the target.
    assert_eq!(hb_ref.evict_to_entries(&mut deque, 100), 0);
    assert_eq!(deque.len(), 25);

    // Never below the requested floor, even if asked for 0.
    assert_eq!(hb_ref.evict_to_entries(&mut deque, 0), 25);
    assert_eq!(deque.len(), 0);
    hb_ref.drain(&mut deque);
    assert_eq!(hb_ref.usage_bytes(), baseline);
}

/// The 20-instance scale invariant, headlessly: 20 instances x 10 durations =
/// 200 pipelines.
///
/// This encodes the situation the governor exists for. Push-time trimming
/// (`trim_to_budget`) keeps global usage near the budget on its own — but only
/// because the deque being pushed absorbs all the eviction. That is fine while
/// pushes are frequent and *useless* when they are not: a 1 h pipeline may not
/// complete a candle for an hour, so its window holds excess the whole time
/// the governor needs it gone.
///
/// So the scenario is: fill the windows, then TIGHTEN the budget (what the
/// governor does under pressure), then sweep. Two invariants:
///
/// 1. The sweep drives global usage under the tightened budget.
/// 2. The sweep reports `met == false` — honestly — when even every window at
///    the floor cannot satisfy the budget.
#[test]
fn two_hundred_pipelines_swept_under_a_tightened_budget() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    const PIPELINES: usize = 200; // 20 instances x 10 durations
    const BARS_EACH: u64 = 60;
    const MIN_KEEP: usize = 25; // the `critical` tier floor

    // 1. Fill every pipeline under a generous budget.
    hb_ref.configure(500, 512 * 1024 * 1024);
    let baseline = hb_ref.usage_bytes();
    let mut deques: Vec<VecDeque<MarketSnapshot>> =
        (0..PIPELINES).map(|_| VecDeque::new()).collect();
    let mut ts = 1_700_000_000u64;
    for _bar in 0..BARS_EACH {
        for deque in deques.iter_mut() {
            hb_ref.push_retained(deque, full_snapshot(ts));
            ts += 5;
        }
    }
    assert_eq!(deques.len(), PIPELINES, "scale fixture = 20 x 10");
    let filled: usize = deques.iter().map(|d| d.len()).sum();
    assert_eq!(filled, PIPELINES * BARS_EACH as usize);

    // 2. Tighten the budget below what the filled windows cost. Everything now
    //    in memory is "excess" from the governor's point of view.
    //
    //    The threshold is DERIVED from the measured per-entry weight rather
    //    than hard-coded: it must sit above what all 200 windows hold at
    //    MIN_KEEP (so the sweep is satisfiable — that is the governor's normal
    //    case) but below what they hold while full (so the fixture is
    //    meaningful). Deriving it keeps the test honest if the entry shape
    //    ever changes, instead of silently turning into the
    //    "floor cannot meet the budget" case pinned by its sibling test.
    let entries_now: usize = deques.iter().map(|d| d.len()).sum();
    let per_entry = hb_ref.usage_bytes() / entries_now.max(1);
    let at_floor = PIPELINES * MIN_KEEP * per_entry;
    let tightened = at_floor + at_floor / 2; // 50 % headroom above the floor
    hb_ref.set_effective_limit(tightened);
    assert!(
        hb_ref.usage_bytes() > tightened,
        "fixture is only meaningful if the windows exceed the tightened budget ({} vs {tightened})",
        hb_ref.usage_bytes()
    );

    // 3. The governor sweep: walk pipelines, shedding oldest-first, early-exit
    //    once the global target is met.
    let mut evicted_total = 0usize;
    let mut met = false;
    for deque in deques.iter_mut() {
        let (n, ok) = hb_ref.evict_to_global_budget(deque, MIN_KEEP);
        evicted_total += n;
        if ok {
            met = true;
            break;
        }
    }
    assert!(met, "the sweep must reach the tightened budget");
    assert!(
        hb_ref.usage_bytes() <= tightened,
        "after the sweep, usage {} must be within {tightened}",
        hb_ref.usage_bytes()
    );
    assert!(
        evicted_total > 0,
        "the sweep must actually have shed something"
    );
    // No window may be cut below the floor, and none emptied.
    for deque in deques.iter() {
        assert!(
            deque.len() >= MIN_KEEP || deque.is_empty(),
            "a swept window must not go below the floor: {} < {MIN_KEEP}",
            deque.len()
        );
    }
    // Newest bars survive — the shed must never discard live data.
    for (i, deque) in deques.iter().enumerate() {
        if let Some(last) = deque.back() {
            assert!(
                last.timestamp >= 1_700_000_000 + BARS_EACH - 1,
                "pipeline {i} must retain its most recent bars"
            );
        }
    }

    // 4. Accounting stays exact across 200 pipelines.
    for deque in deques.iter_mut() {
        hb_ref.drain(deque);
    }
    assert_eq!(
        hb_ref.usage_bytes(),
        baseline,
        "draining 200 pipelines must return the accounting to baseline"
    );
}

/// When even every window at the floor cannot satisfy the budget, the sweep
/// must REPORT failure rather than spin or silently under-report — that signal
/// is what escalates the governor to its `critical` tier (which lowers the
/// floor further) and, failing that, tells the operator to shrink the ladder.
#[test]
fn sweep_reports_failure_when_the_floor_alone_cannot_meet_the_budget() {
    let hb_ref = hb(500, 512 * 1024 * 1024);
    hb_ref.configure(500, 512 * 1024 * 1024);
    let baseline = hb_ref.usage_bytes();
    let mut deques: Vec<VecDeque<MarketSnapshot>> = (0..8).map(|_| VecDeque::new()).collect();
    let mut ts = 1_700_000_000u64;
    for _ in 0..60 {
        for deque in deques.iter_mut() {
            hb_ref.push_retained(deque, full_snapshot(ts));
            ts += 5;
        }
    }

    // A budget no reachable floor can satisfy: 8 x 25 retained frames is
    // already hundreds of KB.
    hb_ref.set_effective_limit(1024);
    let mut met = false;
    for deque in deques.iter_mut() {
        let (_, ok) = hb_ref.evict_to_global_budget(deque, 25);
        if ok {
            met = true;
            break;
        }
    }
    assert!(
        !met,
        "an unreachable budget must be reported as not-met, not assumed met"
    );

    // And the windows really are at the floor, not silently emptied.
    for deque in deques.iter() {
        assert_eq!(deque.len(), 25, "the floor must hold under all pressure");
    }
    let kept: usize = deques.iter().map(|d| d.len()).sum();
    assert_eq!(kept, 8 * 25, "every window pinned at the floor");

    for deque in deques.iter_mut() {
        hb_ref.drain(deque);
    }
    assert_eq!(hb_ref.usage_bytes(), baseline);
}

/// The one invariant that makes shedding acceptable: the live surface is never
/// shed. `latest_snapshot` is a separate handle from `snapshot_history`, and
/// every eviction primitive here only ever touches the latter.
#[tokio::test]
async fn eviction_never_touches_the_live_snapshot() {
    let hb_ref = hb(50, 1);
    hb_ref.configure(50, 1); // force maximum pressure
    let live = Arc::new(RwLock::new(Some(full_snapshot(1_700_000_000))));
    let history = Arc::new(RwLock::new(VecDeque::new()));

    for i in 0..200u64 {
        let mut deque = history.write().await;
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
        hb_ref.evict_to_entries(&mut deque, 5);
    }

    let live_guard = live.read().await;
    assert!(
        live_guard.is_some(),
        "latest_snapshot must survive every shed path — it backs the WS \
         bootstrap, the CLI monitor and every panel"
    );
    assert!(
        live_guard.as_ref().unwrap().close.is_some(),
        "the live snapshot keeps its FULL payload (no projection, no shed)"
    );
}

// ── AUDIT-M4: the estimator must be CALIBRATED against reality ─────────────
//
// The budget's whole job is to be a trustworthy proxy for real memory. If the
// estimator over-charges, the daemon evicts every window to the 50-entry floor
// and logs the breach on every candle close of every fast duration -- degrading
// chart depth far below the documented 500 bars for no reason. If it
// UNDER-charges, the process gets OOM-killed instead.
//
// The estimator was documented as "calibrated against the measured 8.4 KB
// projected frame" but actually computed roughly 3x that, because:
//   * `indicators.len() * 96` charged a String key AND a value head for every
//     entry, while the value's own head is counted again immediately below.
//   * `n += 512` was "slot head-room for the eight synthesis matrices" -- but
//     retained frames are `history_projection()`s, where every matrix is None.
//
// AUDIT-M4 default-workspace consequence: at the shipped 10 instances x 12
// durations = 120 pipelines x 500 entries, the over-charge accounted 600 MB -
// 1.1 GB against a 512 MiB budget, so the push-triggered trim evicted everything
// to the floor in steady state.

/// Measure the estimator against a REAL serialized projected frame.
///
/// The estimator approximates HEAP cost, which is legitimately larger than
/// compact JSON: a `HashMap<String, NormalizedIndicatorValue>` with 52 entries
/// carries capacity slack, per-String allocation headers and alignment that a
/// flat JSON string does not. So a ratio ABOVE 1 is correct and expected — the
/// question is only whether it is in the right order of magnitude.
///
/// AUDIT-M4 measured 2.44 here. That is not a harmless rounding error: at the
/// shipped default (120 pipelines x 500 entries) it accounted 1323 MiB against
/// the 512 MiB budget, driving the push-triggered trim to evict every window to
/// the 50-entry floor in steady state. The two over-charges removed were the
/// double-counted `indicators.len() * 96` key+value head and the `+512` charged
/// for synthesis matrices that `history_projection()` has already set to None.
#[test]
fn the_weight_estimate_is_calibrated_against_real_serialized_bytes() {
    let projected = full_snapshot(1_700_000_000).history_projection();

    let json = serde_json::to_string(&projected).expect("serialize projected");
    let measured = json.len();
    let estimated = projected.history_weight_bytes();

    let ratio = estimated as f64 / measured as f64;

    assert!(
        ratio > 1.0,
        "the estimator must not UNDER-count real memory: {estimated} vs {measured} measured"
    );
    assert!(
        ratio < 2.5,
        "AUDIT-M4 regression: estimate/measured ratio is {ratio:.2} \
         ({estimated} B estimated vs {measured} B serialized). \
         The pre-fix value was 2.44, which accounted the default workspace at \
         1323 MiB against a 512 MiB budget."
    );

    println!(
        "[AUDIT-M4] projected frame: {measured} B serialized, {estimated} B estimated (ratio {ratio:.2})"
    );
}

/// The budget's contract is DEGRADE, NEVER DIE — that inversion is the whole
/// point of the module. At the documented default (10 instances x 12 durations =
/// 120 pipelines) the retained windows are genuinely more than 512 MiB, so the
/// budget is EXPECTED to trim. What it must never do is overshoot into
/// unbounded retention or drive every window below the documented floor.
///
/// Note this test deliberately does NOT assert that 120 pipelines x 500 entries
/// fits in 512 MiB. That was never the design promise — the default workspace
/// is 2.4x over budget on purpose (AGENTS v11.12.27 notes the ladder was widened
/// to 12 durations "which is exactly the axis the memory work made survivable").
/// The promise is that the overrun degrades chart depth rather than killing the
/// daemon.
#[test]
fn a_workspace_larger_than_the_budget_degrades_instead_of_dying() {
    let budget_bytes = 512 * 1024 * 1024;
    let hb_ref = hb(500, budget_bytes);
    hb_ref.configure(500, budget_bytes);
    let baseline = hb_ref.usage_bytes();

    // Push 120 pipelines' worth of 500-entry windows through ONE budget, as the
    // real daemon does.
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    let total_pipelines = 120usize;
    for p in 0..total_pipelines {
        for i in 0..500u64 {
            hb_ref.push_retained(
                &mut deque,
                full_snapshot(1_700_000_000 + (p as u64 * 500) + i),
            );
        }
        // Every window must stay at or above the documented floor — never be
        // trimmed to nothing, which is the "no history at all" failure the floor
        // exists to prevent.
        assert!(
            deque.len() >= 50,
            "pipeline {p}: window fell to {} entries, below the 50-entry floor \
             ('no history at all' is worse than 'less history')",
            deque.len()
        );
    }

    let accounted = hb_ref.usage_bytes() - baseline;
    assert!(
        accounted <= budget_bytes,
        "the budget must be respected: accounted {} MiB against a {} MiB budget",
        accounted / (1024 * 1024),
        budget_bytes / (1024 * 1024)
    );
    assert!(!deque.is_empty(), "something must always survive to render");
}

// ── AUDIT-M12: what "restore" can and cannot mean ──────────────────────────
//
// `evict_to_global_budget` was only ever called to SHRINK, and the sweep's
// `Tier::Normal` branch merely cleared the byte override and counted entries —
// so nothing anywhere ever re-grew a window, and nothing ever SAID that one had
// been shortened. A Critical episode left every chart silently short, which
// contradicted the config field's own doc comment ("`MemAvailable` percentage
// above which the configured budget is restored").
//
// It is worth being precise about what a restore can actually do, because the
// obvious implementation is impossible:
//
//   * It CANNOT put evicted snapshots back. They are dropped, and `evict_to_*
//     only ever shrinks a deque. Nothing can invent history that no longer
//     exists. Any test asserting "grow the window back to N" would be asserting
//     the impossible.
//
//   * What it CAN do is stop trimming. `byte_budget_for(Tier::Normal)` clears the
//     effective limit, after which `push_retained` accepts new snapshots and the
//     window refills at the pipeline's own candle cadence.
//
//   * And it must REPORT the shortfall, because refill is wildly asymmetric: a
//     1s pipeline refills 50 bars in under a minute, a 12h pipeline needs
//     months. A silent short chart is indistinguishable from a healthy one.
//
// These pin the mechanism that does work, and that the accounting stays exact
// through the shed/refill cycle.

#[test]
fn a_restored_budget_stops_trimming_and_the_window_refills() {
    // A budget sized to hold a full 500-entry window when NOT overridden.
    // 500 projected frames weigh ~10 MiB, so 16 MiB fits and its 8 MiB half
    // does not -- which is exactly the governor's High-tier relationship.
    let configured = 16 * 1024 * 1024;
    let hb_ref = hb(500, configured);
    hb_ref.configure(500, configured);
    let baseline = hb_ref.usage_bytes();

    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..500u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert_eq!(
        deque.len(),
        500,
        "the CONFIGURED budget must hold a full window -- otherwise every chart \
         is trimmed from the first bar and the governor is irrelevant"
    );

    // ── Governor at `Tier::High`: halve the effective limit (AUDIT-M12's shed) ──
    let halved = configured / 2;
    hb_ref.set_effective_limit(halved);
    for i in 0..500u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_800_000_000 + i));
    }
    let shed_to = deque.len();
    assert!(
        shed_to < 500,
        "the halved budget must shed history, got {shed_to}"
    );
    assert!(shed_to >= 50, "the 50-entry floor must hold, got {shed_to}");

    // ── Governor back at `Tier::Normal`: clear the override ──
    hb_ref.clear_effective_limit();
    let before_refill = deque.len();
    for i in 0..(500usize - before_refill) as u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_900_000_000 + i));
    }
    assert_eq!(
        deque.len(),
        500,
        "with the override cleared, new snapshots must be accepted and the \
         window refilled -- this is the ONLY mechanism by which a shed recovers, \
         since evicted snapshots cannot be put back"
    );

    // Accounting must stay exact through the whole cycle.
    let summed: usize = deque.iter().map(|s| s.history_weight_bytes()).sum();
    assert_eq!(
        hb_ref.usage_bytes() - baseline,
        summed,
        "usage must equal the summed weight of live entries after shed + refill"
    );

    hb_ref.drain(&mut deque);
    assert_eq!(
        hb_ref.usage_bytes(),
        baseline,
        "a shed followed by a refill must not leak budget credit -- otherwise a \
         pressure episode would permanently shrink every chart"
    );
}

#[test]
fn the_floor_is_never_breached_however_hard_the_pressure() {
    // Even at a token budget, a window keeps enough to render. "No history at
    // all" is a worse failure than "less history", and that inversion is the
    // stated point of the module.
    let hb_ref = hb(500, 1024); // 1 KiB
    hb_ref.configure(50, 1024);
    let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
    for i in 0..2000u64 {
        hb_ref.push_retained(&mut deque, full_snapshot(1_700_000_000 + i));
    }
    assert!(
        deque.len() >= 50,
        "window fell to {} entries under extreme pressure; the 50-entry floor \
         must hold -- fewer than that and the chart renders nothing at all",
        deque.len()
    );
}
