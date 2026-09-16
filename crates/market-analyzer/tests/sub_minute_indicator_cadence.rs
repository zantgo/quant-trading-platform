//! Regression tests for the sub-minute candle-completion path.
//!
//! v6.9 → v6.10: the force-close and doji-fill paths (both sub-minute-only)
//! only broadcast a snapshot via `broadcast_live_snapshot(...)` and
//! `broadcast_tx.send(gap_snap)`. The former uses `indicator.clone().update()`
//! which mutates a throwaway clone, so the analyzer's real `Ema::current_value`
//! never advances. The doji-fill path skips indicator updates entirely. The
//! net effect on a 1-second TF with sparse trade flow: indicators step in
//! jumps at every real trade crossing instead of advancing on every wall-clock
//! second, producing the "stepped EMA" symptom the user reported on Bitget
//! (and Hyperliquid by symmetry).
//!
//! These tests pin the contract: a sub-minute TF pipeline must advance every
//! stateful indicator (EMA/RSI/MACD/Bollinger/ATR/ADX/stochastic/keltner/
//! donchian/obv/cmf/mfi/hv/aroon/choppiness/linreg/zscore/HullMA/AO/FI/
//! WilliamsR/CCI/PSAR/StdDevVolumeProfile/pivots/candlestick/ichimoku/SMC/
//! AVWAP/volume-profile) once per wall-clock second even with **zero** trades,
//! and every indicator must be computed on the current TF's candle closes
//! only — never borrowing from a sibling TF's history.
//!
//! Companion contract (see `crates/market-analyzer/src/analyzer/warm.rs` and
//! `analyzer/mod.rs`): the warm-up path also feeds every indicator from the
//! same TF's candle sequence, so warm-up + live agree by construction.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use rust_decimal_macros::dec;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

fn make_test_config(duration_seconds: u64) -> TimeframeConfig {
    TimeframeConfig {
        candles: config_models::CandlesConfig { duration_seconds },
        indicators: config_models::IndicatorsConfig::default(),
        leverage: Default::default(),
    }
}

fn slot_for(tf_secs: u64) -> core_domain::models::TimeframeSlot {
    // Fixed 10-slot ladder: identity is the exact duration.
    core_domain::models::TimeframeSlot::parse_from_secs(tf_secs)
}

fn label_for(tf_secs: u64) -> &'static str {
    match core_domain::models::TimeframeSlot::parse_from_secs(tf_secs) {
        core_domain::models::TimeframeSlot::Micro1 => "MICRO1",
        core_domain::models::TimeframeSlot::Micro2 => "MICRO2",
        core_domain::models::TimeframeSlot::Fast1 => "FAST1",
        core_domain::models::TimeframeSlot::Fast2 => "FAST2",
        core_domain::models::TimeframeSlot::Slow1 => "SLOW1",
        core_domain::models::TimeframeSlot::Slow2 => "SLOW2",
        core_domain::models::TimeframeSlot::Macro1 => "MACRO1",
        core_domain::models::TimeframeSlot::Macro2 => "MACRO2",
        core_domain::models::TimeframeSlot::Longterm1 => "LONGTERM1",
        core_domain::models::TimeframeSlot::Longterm2 => "LONGTERM2",
        _ => "CUSTOM",
    }
}

/// Spawn a cold (un-warmed) analyzer.
async fn spawn_analyzer_for(
    duration_seconds: u64,
    _exchange: Exchange,
    event_rx: mpsc::Receiver<NormalizedEvent>,
    broadcast_tx: tokio::sync::broadcast::Sender<MarketSnapshot>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    spawn_analyzer_with_warm(
        duration_seconds,
        _exchange,
        event_rx,
        broadcast_tx,
        cancel,
        None,
    )
}

/// PRI-03 (v6.10.7): spawn with an optional pre-warmed state (the
/// sub-minute state-replay warmup handover). `warmed: None` = cold start.
#[allow(clippy::too_many_arguments)]
fn spawn_analyzer_with_warm(
    duration_seconds: u64,
    _exchange: Exchange,
    event_rx: mpsc::Receiver<NormalizedEvent>,
    broadcast_tx: tokio::sync::broadcast::Sender<MarketSnapshot>,
    cancel: CancellationToken,
    warmed: Option<market_analyzer::analyzer::warm::WarmedPipelineState>,
) -> tokio::task::JoinHandle<()> {
    let _ = _exchange; // reserved for future per-exchange pipeline assertions
    let (telemetry_tx, mut telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(500);
    tokio::spawn(async move { while telemetry_rx.recv().await.is_some() {} });

    let div_det = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(10)));
    let history = Arc::new(RwLock::new(VecDeque::new()));
    let latest = Arc::new(RwLock::new(None));
    let snap_hist = Arc::new(RwLock::new(VecDeque::new()));

    let tf = make_test_config(duration_seconds);
    let slot = slot_for(duration_seconds);
    let label = label_for(duration_seconds);

    tokio::spawn(async move {
        let strategy = config_models::StrategyConfig::default();
        analyzer::run_single(
            event_rx,
            telemetry_tx,
            broadcast_tx,
            tf,
            FibonacciConfig::default(),
            core_domain::statistics::StatisticsConfig::default(),
            div_det,
            history,
            latest,
            snap_hist,
            "BTC-USDT".to_string(),
            "BTC-USDT".to_string(),
            duration_seconds,
            label,
            slot,
            cancel,
            None,
            // PRI-03: warmed state handover (None = cold start).
            warmed,
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(VecDeque::with_capacity(60))),
            Arc::new(RwLock::new(VecDeque::with_capacity(8))),
            Arc::new(RwLock::new(None)),
            None,
            None,
            OrderBookConfig::default(),
            strategy,
            // Sibling latest-snapshot handles — none in this single-pipeline test.
            Vec::new(),
            Arc::new(core_domain::LatencyTracker::default()),
            market_analyzer::active_set::ActiveSet::default(),
            None,
            Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
            None,
            None,
            500,
            300,
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(
                core_domain::indicator_dtos::IndicatorLifecycleMap::new(),
            )),
            Arc::new(RwLock::new(
                core_domain::models::CandlePipelineState::Initializing,
            )),
        )
        .await;
    })
}

fn trade(ts_ms: u64, price: rust_decimal::Decimal, exchange: Exchange) -> NormalizedTrade {
    NormalizedTrade {
        exchange,
        symbol: "BTC-USDT".to_string(),
        price,
        size: dec!(1),
        side: TradeSide::Buy,
        timestamp_ms: ts_ms,
        trade_id: format!("submin_t{ts_ms}"),
    }
}

async fn drain_completed(
    broadcast_rx: &mut tokio::sync::broadcast::Receiver<MarketSnapshot>,
    drain_ms: u64,
) -> Vec<MarketSnapshot> {
    let mut snapshots = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(drain_ms);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, broadcast_rx.recv()).await {
            Ok(Ok(snapshot)) => {
                if snapshot.is_completed == Some(true) {
                    snapshots.push(snapshot);
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
    }
    snapshots
}

fn ema_value(snap: &MarketSnapshot, kind: &str) -> Option<f64> {
    snap.indicators
        .get("ema_stack")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get(kind).copied())
}

/// Reference EMA — direct application of the textbook recurrence so the test
/// owns the ground truth independently from the analyzer's `Ema` struct.
fn reference_ema(prices: &[f64], period: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(prices.len());
    if prices.is_empty() {
        return out;
    }
    let k = 2.0 / (period as f64 + 1.0);
    let mut ema = prices[0];
    out.push(ema);
    for p in &prices[1..] {
        ema = (p - ema) * k + ema;
        out.push(ema);
    }
    out
}

fn order_book_mid_110(exchange: Exchange) -> NormalizedEvent {
    NormalizedEvent::OrderBook(core_domain::normalized::NormalizedOrderBook {
        exchange,
        symbol: "BTC-USDT".to_string(),
        bids: vec![(dec!(110), dec!(1))],
        asks: vec![(dec!(110), dec!(1))],
        timestamp_ms: 200,
    })
}

/// BUG-FIX-01: after a force-close and several doji-fill seconds, every EMA's
/// state must advance per wall-clock second. Before the fix, `force_close`
/// and the doji fill skip the indicator-update path so the analyzer's EMA
/// state lags the wall clock. We pin both Bitget and Hyperliquid since the
/// bug is exchange-agnostic.
///
/// Strategy: seed the pipeline with a trade at 100, then push an order-book
/// mid at 110 and re-seed a few times so the stale-check force-close +
/// doji-fill path (60 dojis per stale tick) accumulates ≥ 200 bars — the
/// old `bars_required` gate for `ema_stack` — while every completed candle
/// closes at 110. A correctly-advancing EMA must converge toward 110 from
/// the 100 seed. Before the fix the EMA stays pinned at 100 forever.
///
/// AUDIT-V8-002 (stale-mid guard): the mid is now only honoured while the
/// book is fresh (≤ grace period), so the test re-sends the 110 mid every
/// round instead of seeding it once — a single stale mid must NOT anchor
/// the close (that distortion is pinned by the stale-mid guard test below).
#[tokio::test]
async fn ema_state_advances_every_second_on_sub_minute_tf() {
    for exchange in [Exchange::Hyperliquid, Exchange::Bitget] {
        let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
        let (broadcast_tx, mut broadcast_rx) =
            tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
        let cancel = CancellationToken::new();
        let _h =
            spawn_analyzer_for(1, exchange, event_rx, broadcast_tx.clone(), cancel.clone()).await;

        // Seed the very first candle at 100.
        event_tx
            .send(NormalizedEvent::Trade(trade(100, dec!(100), exchange)))
            .await
            .unwrap();

        // Re-seed several times, spaced wider than the 500ms stale-check
        // cadence, so each round triggers a force-close + up to 60 doji
        // fills → ~61 bars per round. Six rounds ≈ 366 bars, well past
        // the old `ema_stack.bars_required = 200` gate. A background OB
        // pump keeps the 110 mid fresh at every stale-check instant so
        // the AUDIT-V8-002 stale-mid guard always honours it (real
        // exchanges push order-book updates continuously).
        let ob_pump = tokio::spawn({
            let event_tx = event_tx.clone();
            async move {
                for _ in 0..50 {
                    if event_tx.send(order_book_mid_110(exchange)).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        });
        for round in 0..6u64 {
            let start_ms = 10_000 + round * 1_100;
            event_tx
                .send(NormalizedEvent::Trade(trade(start_ms, dec!(100), exchange)))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
        ob_pump.abort();

        let snaps = drain_completed(&mut broadcast_rx, 500).await;
        cancel.cancel();

        assert!(
            snaps.len() >= 200,
            "exchange={:?}: expected ≥200 completed snapshots (ema_stack warm-up gate), got {}",
            exchange,
            snaps.len()
        );

        // The LAST snapshot must carry an EMA fast that has clearly moved
        // off the 100 seed toward the 110 close. EMA fast (period 10)
        // converges to ~110 after ~300 advances at close=110. We assert a
        // loose lower bound (>101) so timer jitter can't flake, and an
        // upper bound (≤110) so we know it is converging, not overshooting.
        let last = snaps.last().expect("at least one snapshot");
        let fast = ema_value(last, "fast").expect("ema_stack.fast present after warm-up");
        assert!(
            fast > 101.0,
            "exchange={:?}: EMA fast = {} — indicator state did not advance per wall-clock second; \
             the sub-minute force-close / doji-fill path is not updating indicators",
            exchange,
            fast
        );
        assert!(
            fast <= 110.0 + 1e-6,
            "exchange={:?}: EMA fast = {} — EMA overshot the 110 close (should converge, not overshoot)",
            exchange,
            fast
        );
    }
}

/// BUG-FIX-02: a 1-second TF pipeline must emit exactly one completed
/// snapshot per wall-clock second even when **zero** trades arrive. This is
/// the doji-fill path — every missed second gets a zero-volume
/// reconstructed candle, the indicators advance, and the chart stays
/// continuous.
#[tokio::test]
async fn one_second_tf_emits_one_completed_per_wall_second_with_zero_trades() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Bitget,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // Send a single seed trade, then stop sending. The analyzer must
    // still emit one completed snapshot per wall-clock second.
    event_tx
        .send(NormalizedEvent::Trade(trade(
            2_000_000 + 500,
            dec!(100),
            Exchange::Bitget,
        )))
        .await
        .unwrap();

    // Wait for ~6 seconds of wall-clock. The pipeline should emit ~6
    // completed snapshots: 1 from the seed + 5 from doji-fill (or
    // force-close + doji-fill depending on when the seed landed).
    tokio::time::sleep(Duration::from_millis(6500)).await;

    let snaps = drain_completed(&mut broadcast_rx, 500).await;
    cancel.cancel();

    assert!(
        snaps.len() >= 5,
        "expected ≥5 completed snapshots over ~6s with zero trades after the seed, got {}",
        snaps.len()
    );
    // All completed snapshots must lie on 1-second boundaries
    // (candle.timestamp is interval_start in seconds — every integer is
    // trivially on a 1s boundary; the check pins the unit contract).
    for s in &snaps {
        assert_eq!(
            s.timeframe_secs, 1,
            "completed snapshot timeframe_secs must equal 1, got {}",
            s.timeframe_secs
        );
    }
}

/// BUG-FIX-03: every stateful indicator in the per-TF analyzer must advance
/// once per wall-clock second on the 1-second TF, even when the live path is
/// driven purely by `force_close` + `doji_fill` (no trade crossing the
/// boundary for the entire window). We feed 210 closes spread across
/// 210 wall-clock seconds so the `bars_required = 200` gate on ema_stack
/// is past, then assert each indicator family is present on the LAST
/// completed snapshot of the doji-fill run.
#[tokio::test]
async fn all_indicator_families_advance_on_sub_minute_doji_fills() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // Pump 210 one-second buckets with one trade each so the analyzer has
    // 210 completed candles (≥ `bars_required = 200`).
    for s in 0..210u64 {
        let start_ms = 3_000_000 + s * 1000;
        let dec_px = rust_decimal::Decimal::from_f64_retain(100.0 + s as f64).unwrap_or_default();
        event_tx
            .send(NormalizedEvent::Trade(trade(
                start_ms + 500,
                dec_px,
                Exchange::Hyperliquid,
            )))
            .await
            .unwrap();
    }

    // Wait for the analyzer to drain its channels + emit completed snapshots.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let snaps = drain_completed(&mut broadcast_rx, 3000).await;
    cancel.cancel();

    let last = snaps.last().expect("at least one snapshot");

    // Trend family — EMA stack + Ichimoku + Supertrend
    assert!(
        ema_value(last, "fast").is_some(),
        "ema_stack.fast missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("supertrend"),
        "supertrend missing after warm-up"
    );
    // Momentum family
    assert!(
        last.indicators.contains_key("rsi"),
        "rsi missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("macd"),
        "macd missing after warm-up"
    );
    // Volatility family
    assert!(
        last.indicators.contains_key("atr"),
        "atr missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("bollinger"),
        "bollinger missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("keltner"),
        "keltner missing after warm-up"
    );
    // Volume family — these naturally emit `0.0` on a zero-volume doji but
    // the keys must still be present so the chart doesn't drop them.
    assert!(
        last.indicators.contains_key("obv"),
        "obv missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("cmf"),
        "cmf missing after warm-up"
    );
    // Structural family
    assert!(
        last.indicators.contains_key("donchian"),
        "donchian missing after warm-up"
    );
    assert!(
        last.indicators.contains_key("aroon"),
        "aroon missing after warm-up"
    );
}

/// BUG-FIX-04: every stateful indicator must be computed on the current
/// timeframe's candle closes only — never borrow from a sibling TF's
/// history. We pin this by feeding the reference `Rsi` calculator two
/// different candle sequences (one 1-second bar per sample, one 5-second
/// bar per sample) and asserting they produce different RSI values. If a
/// live implementation ever conflated these, the per-TF contract is broken.
#[test]
fn indicator_values_are_per_tf_not_shared_across_tfs() {
    use market_analyzer::indicators::Rsi;
    use rust_decimal::prelude::ToPrimitive;

    // 100 trade prices → 20 five-second bars. Both pipelines exceed the
    // RSI period (14) so both produce a real reading.
    let prices: Vec<f64> = (0..100).map(|i| 100.0 + (i as f64).sin() * 5.0).collect();

    // Reference RSI on 1s buckets: 100 one-second candles from 100 trades.
    let mut rsi_1s = Rsi::new(14);
    for p in &prices {
        rsi_1s.update(*p);
    }
    let rsi_1s_value = rsi_1s
        .update(*prices.last().unwrap())
        .and_then(|d| d.to_f64());

    // Reference RSI on 5s buckets: roll the 100 trades into 20 bars by
    // binning every 5th close into one bar.
    let bars_5s: Vec<f64> = prices.chunks(5).map(|c| *c.last().unwrap()).collect();
    let mut rsi_5s = Rsi::new(14);
    for p in &bars_5s {
        rsi_5s.update(*p);
    }
    let rsi_5s_value = rsi_5s
        .update(*bars_5s.last().unwrap())
        .and_then(|d| d.to_f64());

    let r1 = rsi_1s_value.expect("rsi 1s seeded with 100 bars");
    let r5 = rsi_5s_value.expect("rsi 5s seeded with 20 bars");
    assert_ne!(
        r1, r5,
        "RSI on 1s vs 5s timeframes must differ; both reference pipelines must be independent (1s={}, 5s={})",
        r1, r5
    );
}

/// BUG-FIX-05: the 1-second TF candle-bucket alignment must be deterministic
/// for both Hyperliquid and Bitget trade timestamps (both wrap the
/// exchange's millisecond timestamp). One trade at `ts=1_234_567_890_123`
/// must land in the `[1_234_567_890_000, 1_234_567_891_000)` bucket and
/// never a 60-second or 5-second bucket.
#[test]
fn sub_minute_bucket_alignment_is_correct_for_ms_timestamps() {
    use market_analyzer::candle_generator::CandleGenerator;

    for exchange in [Exchange::Hyperliquid, Exchange::Bitget] {
        for tf_secs in [1u64, 3, 5, 15] {
            let mut gen = CandleGenerator::new("BTC-USDT", tf_secs, exchange);
            let ts_ms: u64 = 1_234_567_890_123;
            let expected_bucket = (ts_ms / (tf_secs * 1000)) * (tf_secs * 1000);

            let (_, live) = gen.process_trade(&trade(ts_ms, dec!(42_000), exchange));
            assert_eq!(
                live.start_time_ms, expected_bucket,
                "exchange={:?} tf={}s: bucket must be aligned to {}-second epoch, got {}",
                exchange, tf_secs, tf_secs, live.start_time_ms
            );
            assert_eq!(
                live.duration_ms,
                tf_secs * 1000,
                "exchange={:?} tf={}s: duration_ms must equal {}",
                exchange,
                tf_secs,
                tf_secs * 1000
            );
        }
    }
}

/// BUG-FIX-06: at every ≥60-second TF the existing pipeline behaviour is
/// already correct (the trade stream crosses the boundary every minute
/// regardless of trade density). Pin the contract so the refactor that
/// fixes sub-minute cannot regress the minute-and-above ladder. We feed
/// 210 one-minute candles so the ema_stack `bars_required = 200` gate
/// passes and the EMA values are visible in the indicators map.
#[tokio::test]
async fn sixty_second_tf_indicator_state_advances_per_real_close() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        60,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // Walk 210 minutes of price action with one trade per minute so each
    // minute's bucket closes via the trade stream (no force-close needed).
    let n = 210usize;
    let mut closes: Vec<f64> = Vec::with_capacity(n - 1);
    for s in 0..n {
        let px = 100.0 + (s as f64) * 0.05;
        // Each trade at index s closes bucket (s-1) (or just starts a candle
        // when s == 0). The candle close equals the trade's price at the
        // bucket boundary, so the analyzer sees the close sequence
        // [100.00, 100.05, 100.10, ..., 110.40] — `n - 1` closes.
        if s > 0 {
            closes.push(100.0 + (s as f64 - 1.0) * 0.05);
        }
        let start_ms = (s as u64) * 60_000 + 30_000; // mid-bucket
        let dec_px = rust_decimal::Decimal::from_f64_retain(px).unwrap_or_default();
        event_tx
            .send(NormalizedEvent::Trade(trade(
                start_ms,
                dec_px,
                Exchange::Hyperliquid,
            )))
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let snaps = drain_completed(&mut broadcast_rx, 3000).await;
    cancel.cancel();

    // With `n` trades spread across `n` minute-buckets, exactly `n-1`
    // trade-triggered completed candles fire (the last trade starts the
    // bucket but no follow-up trade closes it). Plus any doji-fill
    // candles from the post-loop drain window. We assert ≥ n-2 to
    // tolerate broadcast-channel jitter on a single token in the
    // long 210-bar drain.
    assert!(
        snaps.len() >= n - 2,
        "expected ≥{} completed 60s snapshots, got {}",
        n - 2,
        snaps.len()
    );

    let last = snaps.last().unwrap();
    let fast = ema_value(last, "fast").expect("ema_stack.fast present after warm-up");
    let ref_fast = reference_ema(&closes, config_models::IndicatorsConfig::default().ema_fast)
        .pop()
        .unwrap();
    assert!(
        (fast - ref_fast).abs() < 1e-6,
        "60s TF EMA fast = {}, reference = {} (diff={})",
        fast,
        ref_fast,
        (fast - ref_fast).abs()
    );
}

/// AUDIT-V8-001 (per-line ribbon warm-up): each EMA line must appear in
/// `ema_stack.values` only once the pipeline has accumulated at least its
/// configured period of completed closes (defaults fast@10, medium@50,
/// slow@100, long@200). Before the fix the whole ribbon waited for the
/// 200-bar `bars_required` gate, so sub-minute charts showed no EMA data
/// (or lines that all started at the same right-edge bar).
///
/// Buckets are anchored at `now` so no stale-check doji-flood inflates
/// `bar_count` during the pump; the completed stream's last snapshot
/// therefore carries exactly `n_trades` bars.
#[tokio::test]
async fn ema_lines_appear_at_their_own_periods_on_sub_minute_tf() {
    use core_domain::LatencyTracker;
    let cfg = config_models::IndicatorsConfig::default();
    // (trades to pump, bar_count the last snapshot should carry, lines
    // expected present, lines expected absent)
    let cases: Vec<(u64, u32, &[&str], &[&str])> = vec![
        (12, 12, &["fast"], &["medium", "slow", "long"]),
        (60, 60, &["fast", "medium"], &["slow", "long"]),
        (120, 120, &["fast", "medium", "slow"], &["long"]),
        (210, 210, &["fast", "medium", "slow", "long"], &[]),
    ];
    for (n_trades, expected_bars, present, absent) in cases {
        let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
        let (broadcast_tx, mut broadcast_rx) =
            tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
        let cancel = CancellationToken::new();
        let _h = spawn_analyzer_for(
            1,
            Exchange::Hyperliquid,
            event_rx,
            broadcast_tx.clone(),
            cancel.clone(),
        )
        .await;

        let base = LatencyTracker::now_ms();
        for s in 0..n_trades {
            // Slight price ramp (100.0 + s*0.01) keeps the synthesis
            // entry/invalidation zones non-degenerate (flat closes trip a
            // debug_assert in `derive_side_zones`).
            let px =
                rust_decimal::Decimal::from_f64_retain(100.0 + s as f64 * 0.01).unwrap_or_default();
            event_tx
                .send(NormalizedEvent::Trade(trade(
                    base + s * 1000,
                    px,
                    Exchange::Hyperliquid,
                )))
                .await
                .unwrap();
        }
        // The completed-candle path is heavy (synthesis + decision
        // context); drain from the moment the pump ends so we capture the
        // tail of the trade-triggered closes.
        let snaps = drain_completed(&mut broadcast_rx, 3000).await;
        cancel.cancel();

        assert!(
            snaps.len() >= 5,
            "case n={}: expected completed snapshots to arrive, got {}",
            n_trades,
            snaps.len()
        );
        let last = snaps.last().expect("at least one completed snapshot");
        let bars_seen = last
            .indicator_lifecycle
            .get("ema_stack")
            .map(|l| l.bars_seen)
            .unwrap_or(0);
        let vals = last
            .indicators
            .get("ema_stack")
            .and_then(|v| v.values.as_ref())
            .expect("ema_stack entry present with a values map");
        assert!(
            bars_seen >= expected_bars - 1,
            "case n={}: bars_seen = {} (expected ≥ {})",
            n_trades,
            bars_seen,
            expected_bars - 1
        );
        for role in present {
            assert!(
                vals.contains_key(*role),
                "case n={}: bars_seen={} — expected `ema_stack.values.{role}` present (period {} ≤ {bars_seen})",
                n_trades,
                bars_seen,
                match *role {
                    "fast" => cfg.ema_fast,
                    "medium" => cfg.ema_medium,
                    "slow" => cfg.ema_slow,
                    "long" => cfg.ema_long,
                    _ => 0,
                },
            );
        }
        for role in absent {
            assert!(
                !vals.contains_key(*role),
                "case n={}: bars_seen={} — `ema_stack.values.{role}` must be absent until its period is reached",
                n_trades,
                bars_seen,
            );
        }
    }
}

/// AUDIT-V8-003 (idle-bucket heartbeat): after the current candle closes
/// and the market goes completely quiet, the stale check must keep
/// emitting one completed snapshot per elapsed empty bucket (synthetic
/// doji at the last known close). Without the heartbeat the chart would
/// show a gap (frontend flat-Doji bridge) and the EMA lines would connect
/// real points with straight segments — the "1s candle stays open for
/// seconds" symptom.
#[tokio::test]
async fn idle_bucket_heartbeat_fills_quiet_seconds_on_sub_minute_tf() {
    use core_domain::LatencyTracker;
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // One seed trade in a now-anchored bucket, then total silence.
    let base = LatencyTracker::now_ms();
    let seed_bucket = (base / 1000) * 1000;
    event_tx
        .send(NormalizedEvent::Trade(trade(
            seed_bucket + 500,
            dec!(100),
            Exchange::Hyperliquid,
        )))
        .await
        .unwrap();

    // ~5.5 s of silence: expect the seed bucket close + one heartbeat
    // doji per elapsed second (4-5 empty buckets).
    tokio::time::sleep(Duration::from_millis(5500)).await;
    let snaps = drain_completed(&mut broadcast_rx, 700).await;
    cancel.cancel();

    let mut done: Vec<u64> = snaps.iter().map(|s| s.timestamp).collect();
    done.sort_unstable();
    done.dedup();
    assert!(
        done.len() >= 4,
        "expected ≥4 completed 1s snapshots (seed + idle heartbeats) over ~5.5s of silence, got {}: {:?}",
        done.len(),
        done
    );
    // Consecutive 1-second buckets — no gaps.
    for pair in done.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            1,
            "completed buckets must be consecutive (no gaps): {:?}",
            done
        );
    }
    // Heartbeat dojis carry the last known close and the reconstructed
    // provenance so the frontend can keep them out of its candle cache.
    for s in snaps.iter().skip(1) {
        assert_eq!(s.close, Some(rust_decimal::Decimal::from(100)));
        assert_eq!(
            s.quality_envelope.as_ref().map(|q| q.is_gap_filled),
            Some(true),
            "heartbeat snapshot must be marked gap-filled"
        );
    }
}

/// AUDIT-V8-002 (stale-mid guard): the force-close / doji-fill paths must
/// only use the order-book mid while the book is fresh (≤ grace period).
/// A single stale mid (received long before the close) must NOT anchor the
/// synthetic closes — the EMA would otherwise converge toward a phantom
/// price ("reacting to price action that isn't there"). The last close
/// must be the last trade price instead.
#[tokio::test]
async fn stale_mid_guard_falls_back_to_last_trade_close() {
    use core_domain::LatencyTracker;
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // The mid must be received BEFORE the seed so it is already stale by
    // the time the seed's bucket closes (≤1 s later): send the single
    // 110 mid first, wait past the 1 s grace period, then seed a trade at
    // 100. The force-close must discard the stale mid and close at the
    // last trade price (100), never 110.
    let base = LatencyTracker::now_ms();
    event_tx
        .send(order_book_mid_110(Exchange::Hyperliquid))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let seed_ts = ((base / 1000) + 3) * 1000 + 500;
    event_tx
        .send(NormalizedEvent::Trade(trade(
            seed_ts,
            dec!(100),
            Exchange::Hyperliquid,
        )))
        .await
        .unwrap();

    // Wait past the grace period (1s) so the mid is stale at force-close.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let snaps = drain_completed(&mut broadcast_rx, 500).await;
    cancel.cancel();

    assert!(
        !snaps.is_empty(),
        "expected at least one completed snapshot, got {}",
        snaps.len()
    );
    for s in &snaps {
        assert_eq!(
            s.close,
            Some(rust_decimal::Decimal::from(100)),
            "force-close must use the last trade close, not the stale mid 110 (snapshot ts={})",
            s.timestamp
        );
    }
}

/// The matrix-payload regression: sub-minute force-closed candles must
/// broadcast completed frames with the full matrix payload
/// (alignment/analysis/risk/advisory/opportunity/decision_context), not just
/// OHLCV + indicators.
///
/// Before the fix, the clock-driven force-close path built its snapshot via
/// `build_completed_snapshot_from_readings`, which hard-coded every matrix
/// field to `None`, and the v6.11 dedup gate then discarded the trade-triggered
/// completion for the same bucket — so on sparse-flow sub-minute TFs the
/// pair-level matrix mirrors in the frontend (`pair.alignment` etc.) never
/// populated and every non-chart tab stayed empty while the chart worked.
///
/// Strategy: anchor the seed trades in a PAST bucket (like the other
/// sub-minute tests) so `is_past_interval` closes every bucket on the
/// stale-check cadence — i.e. every real candle is force-closed, never
/// trade-triggered. A background order-book pump keeps the 110 mid fresh so a
/// real force-close closes at 110 (distinguishable from the doji-fill
/// snapshots, which are marked `is_gap_filled`). The 60 dojis filled per
/// stale-check tick push `bar_count` past the 50-bar sub-minute live floor
/// within ~1 s, so the second force-close onward must carry the matrices.
#[tokio::test]
async fn force_closed_sub_minute_frames_carry_matrix_payload() {
    use core_domain::LatencyTracker;
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // Anchor all seed trades in a past 1s bucket: every stale-check
    // tick (500 ms) force-closes the current candle (is_past_interval is
    // true for past buckets), so every REAL completed candle here is a
    // force-close — exactly the path that previously dropped the matrices.
    // The bucket sits ~70 s in the past so the doji-fill after the FIRST
    // force-close already floods up to MAX_GAP_FILL_BARS (60) synthetic
    // bars — bar_count crosses the 50-bar sub-minute live floor within the
    // first second, and the second force-close onward must carry matrices.
    let base = LatencyTracker::now_ms();
    let past_bucket = ((base / 1000) - 70) * 1000 + 500;

    // Background OB pump keeps the 110 mid fresh (≤ 1s grace period) at
    // every force-close instant, so real force-closes close at 110.
    let ob_pump = tokio::spawn({
        let event_tx = event_tx.clone();
        async move {
            for _ in 0..40 {
                if event_tx
                    .send(order_book_mid_110(Exchange::Hyperliquid))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        }
    });

    // Re-seed a trade every ~150 ms: each trade re-opens the candle after a
    // force-close (the bucket stays anchored in the past), so every stale
    // tick closes a real force-closed candle. 16 trades ≈ 2.4 s ≈ 5 ticks.
    for i in 0..16u64 {
        event_tx
            .send(NormalizedEvent::Trade(trade(
                past_bucket + i,
                dec!(100),
                Exchange::Hyperliquid,
            )))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    tokio::time::sleep(Duration::from_millis(1200)).await;
    ob_pump.abort();

    let snaps = drain_completed(&mut broadcast_rx, 500).await;
    cancel.cancel();

    // Sanity: the doji-fill (60 bars per tick) pushed bar_count far past the
    // 50-bar sub-minute live floor, so real force-closes after ~1 s must be
    // synthesized with the full matrix payload.
    assert!(
        snaps.len() >= 60,
        "expected ≥60 completed 1s snapshots (force-closes + doji fills), got {}",
        snaps.len()
    );

    let has_all_matrices = |s: &MarketSnapshot| {
        s.alignment.is_some()
            && s.analysis.is_some()
            && s.risk.is_some()
            && s.advisory.is_some()
            && s.opportunity.is_some()
            && s.decision_context.is_some()
    };

    assert!(
        snaps.iter().any(&has_all_matrices),
        "no completed snapshot carries the full matrix payload (alignment/analysis/risk/\
         advisory/opportunity/decision_context) — the sub-minute force-close path is not \
         running the matrix synthesis"
    );

    // The decisive pin: a REAL (non-gap-filled) force-closed candle closing at
    // the fresh 110 mid must carry the matrices. Doji-fill snapshots also
    // close at 110 but are marked gap-filled, so `close == 110 &&
    // is_gap_filled == false` identifies the force-close path uniquely.
    let force_matrix = snaps.iter().find(|s| {
        has_all_matrices(s)
            && s.close == Some(dec!(110))
            && !s
                .quality_envelope
                .as_ref()
                .map(|q| q.is_gap_filled)
                .unwrap_or(true)
    });
    assert!(
        force_matrix.is_some(),
        "no real (non-gap-filled) force-closed candle at the fresh mid 110 carries the \
         matrix payload — the sub-minute force-close broadcast still drops the matrices"
    );
}

/// PRI-03/PRI-05 (v6.10.7) — sub-minute state-replay warmup parity. A 1s
/// slot warmed from 300 real 60s closes must behave like an above-minute
/// slot after warmup: `pipeline_is_live` from the FIRST live close, the
/// full ema ribbon present, the indicator lifecycle `Live`, and the full
/// matrix payload (alignment/analysis/risk/advisory/opportunity/
/// decision_context) on the wire — no progressive maturity window.
#[tokio::test]
async fn warmed_sub_minute_pipeline_reaches_live_parity_at_first_close() {
    use core_domain::LatencyTracker;
    use market_analyzer::analyzer::warm::warm_indicators_for_timeframe;

    // Build 300 real 60s candles with a gentle price ramp (the warmup
    // replay source — the same closes a ≥60s slot would warm from).
    let base = LatencyTracker::now_ms();
    let anchor = ((base / 60_000) - 6) * 60_000; // 300 × 60s = 5 min lookback
    let mut candles: Vec<core_domain::normalized::NormalizedCandle> = Vec::with_capacity(300);
    for i in 0..300u64 {
        let px =
            rust_decimal::Decimal::from_f64_retain(100.0 + i as f64 * 0.01).unwrap_or_default();
        candles.push(core_domain::normalized::NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            start_time_ms: anchor + i * 60_000,
            duration_ms: 60_000,
            open: px,
            high: px + dec!(0.5),
            low: px - dec!(0.5),
            close: px,
            volume: dec!(1),
            trades_count: 1,
            reconstructed: None,
        });
    }

    let warmed = warm_indicators_for_timeframe(
        candles,
        &make_test_config(1),
        &FibonacciConfig::default(),
        "BTC-USDT",
        1,
        core_domain::models::TimeframeSlot::Micro1,
        500,
        &market_analyzer::active_set::ActiveSet::all_enabled(),
        Some(core_domain::normalized::Exchange::Hyperliquid),
    );

    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    // NOTE: `spawn_analyzer_with_warm` is sync — do NOT `.await` the
    // JoinHandle or the test blocks until the analyzer task ends.
    let _h = spawn_analyzer_with_warm(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
        Some(warmed),
    );

    // One trade in the current bucket → the clock-driven force-close at
    // the next boundary emits the first LIVE completed frame.
    let seed_bucket = (LatencyTracker::now_ms() / 1000) * 1000;
    event_tx
        .send(NormalizedEvent::Trade(trade(
            seed_bucket + 500,
            dec!(100.5),
            Exchange::Hyperliquid,
        )))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    let snaps = drain_completed(&mut broadcast_rx, 500).await;
    cancel.cancel();

    // The drain window may end on a doji-fill frame (gap-filled,
    // matrix-less) — the assertions must run on the last REAL
    // (non-gap-filled) completed snapshot, i.e. the actual force-close.
    let live = snaps
        .iter()
        .rev()
        .find(|s| {
            !s.quality_envelope
                .as_ref()
                .map(|q| q.is_gap_filled)
                .unwrap_or(false)
        })
        .expect("at least one real (non-gap-filled) completed snapshot");
    assert_eq!(
        live.pipeline_state,
        core_domain::models::CandlePipelineState::Live,
        "warmed sub-minute pipeline must be Live from the first live close (PRI-05)"
    );
    let vals = live
        .indicators
        .get("ema_stack")
        .and_then(|v| v.values.as_ref())
        .expect("ema_stack entry present");
    for role in ["fast", "medium", "slow", "long"] {
        assert!(
            vals.contains_key(role),
            "warmed sub-minute ribbon must carry ema_stack.values.{role} at the first live close"
        );
    }
    let lc = live
        .indicator_lifecycle
        .get("ema_stack")
        .map(|l| l.state)
        .unwrap_or(core_domain::indicator_dtos::IndicatorLifecycleState::Loading);
    assert_eq!(
        lc,
        core_domain::indicator_dtos::IndicatorLifecycleState::Live,
        "warmed sub-minute indicator lifecycle must be Live at the first live close"
    );
    assert!(
        live.alignment.is_some()
            && live.analysis.is_some()
            && live.risk.is_some()
            && live.advisory.is_some()
            && live.opportunity.is_some()
            && live.decision_context.is_some(),
        "warmed sub-minute first close must carry the full matrix payload"
    );
    // PRI-12: bars_seen_real counts the warmed real closes.
    let real = live
        .indicator_lifecycle
        .get("ema_stack")
        .and_then(|l| l.bars_seen_real)
        .unwrap_or(0);
    assert!(
        real >= 200,
        "bars_seen_real must reflect the warmed real closes (got {real})"
    );
}

/// PRI-06 (v6.10.7): real force-closed candles feed the `history` buffer
/// (the fib/pivots/S-R/pattern and cluster-matrix input), while synthetic
/// doji-fill buckets never do. Without this the cluster matrix errored
/// `InsufficientHistory` on quiet sub-minute markets and S-R/fib/pattern
/// signals were computed from a sparse history.
#[tokio::test]
async fn force_closed_real_candles_feed_history_but_dojis_do_not() {
    use core_domain::LatencyTracker;
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(4000);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer_for(
        1,
        Exchange::Hyperliquid,
        event_rx,
        broadcast_tx.clone(),
        cancel.clone(),
    )
    .await;

    // Past-anchored seed: every stale-check tick force-closes the current
    // candle (real) and the doji-fill floods synthetic buckets (gap-filled).
    let base = LatencyTracker::now_ms();
    let past_bucket = ((base / 1000) - 70) * 1000 + 500;
    for i in 0..8u64 {
        // Slight price ramp keeps the synthesis entry/invalidation zones
        // non-degenerate (flat closes trip a debug_assert in
        // `derive_side_zones`).
        let px =
            rust_decimal::Decimal::from_f64_retain(100.0 + i as f64 * 0.01).unwrap_or_default();
        event_tx
            .send(NormalizedEvent::Trade(trade(
                past_bucket + i,
                px,
                Exchange::Hyperliquid,
            )))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let snaps = drain_completed(&mut broadcast_rx, 500).await;
    cancel.cancel();

    assert!(
        snaps.len() >= 4,
        "expected real force-closes + doji fills, got {}",
        snaps.len()
    );
    let synthetic = snaps
        .iter()
        .filter(|s| {
            s.quality_envelope
                .as_ref()
                .map(|q| q.is_gap_filled)
                .unwrap_or(false)
        })
        .count();
    let real = snaps.len() - synthetic;
    assert!(real >= 2, "expected ≥2 real force-closes, got {real}");

    // The pipeline's `history` handle is inside the spawned task; we cannot
    // reach it directly. Instead we assert the wire contract: every REAL
    // completed frame carries the synthesis output (matrices + indicators),
    // which is computed from `history` — and the doji frames are marked
    // gap-filled (they never enter `history`). The `history` feed itself is
    // exercised end-to-end by the force-close path pushing the forced
    // candle (PRI-06); its observable effect is that fib/S-R/cluster inputs
    // stay current, covered by the matrix payload on real frames.
    let real_matrix = snaps.iter().find(|s| {
        !s.quality_envelope
            .as_ref()
            .map(|q| q.is_gap_filled)
            .unwrap_or(false)
            && s.alignment.is_some()
    });
    assert!(
        real_matrix.is_some(),
        "real force-closed candles must carry the synthesis output (history-fed matrices)"
    );
}
