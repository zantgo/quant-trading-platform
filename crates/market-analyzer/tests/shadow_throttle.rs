//! Verifies the per-pipeline shadow-broadcast throttle introduced to
//! prevent the dashboard freeze that occurred when a sub-60s timeframe
//! was configured (the analyzer would emit ~50+ broadcasts/sec per
//! pipeline slot × 4 slots, saturating the broadcast channel and
//! flooding the frontend with redraws).
//!
//! The throttle caps the shadow (live/flickering) broadcast path at
//! `max(100ms, timeframe_secs*1000/4)` — one shadow per quarter-candle
//! (AUDIT-AIU-122: the shipped formula; the earlier
//! `max(100ms, min(250ms, tf*1000/4))` cap and the parity-doc
//! `min(tf/4, 1s)` draft were both superseded). The candle-close
//! path is unaffected and must still fire on every natural close.
//!
//! Test strategy: spawn a real `analyzer::run_single` task with
//! `timeframe_secs = 1`, send 50 trade ticks spaced 10 ms apart
//! (≈ 100 Hz — well above what the throttle can sustain), and assert
//! the broadcast receiver got at most ~8 snapshots in the resulting
//! 500 ms window. Then assert that crossing a candle boundary still
//! produces a completed snapshot (regression guard).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use config_models::{FibonacciConfig, IndicatorsConfig, OrderBookConfig, TimeframeConfig};
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use rust_decimal_macros::dec;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

fn make_test_config(duration_seconds: u64) -> TimeframeConfig {
    TimeframeConfig {
        candles: config_models::CandlesConfig { duration_seconds },
        indicators: IndicatorsConfig::default(),
        leverage: Default::default(),
    }
}

async fn spawn_analyzer(
    duration_seconds: u64,
    event_rx: mpsc::Receiver<NormalizedEvent>,
    broadcast_tx: tokio::sync::broadcast::Sender<core_domain::models::MarketSnapshot>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let (telemetry_tx, mut telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(500);
    tokio::spawn(async move { while telemetry_rx.recv().await.is_some() {} });

    let div_det = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(10)));
    let history = Arc::new(RwLock::new(VecDeque::new()));
    let latest = Arc::new(RwLock::new(None));
    let snap_hist = Arc::new(RwLock::new(VecDeque::new()));

    let slot_label = core_domain::duration_label(duration_seconds);

    tokio::spawn(async move {
        let strategy = config_models::StrategyConfig::default();
        analyzer::run_single(
            event_rx,
            telemetry_tx,
            broadcast_tx,
            make_test_config(duration_seconds),
            FibonacciConfig::default(),
            core_domain::statistics::StatisticsConfig::default(),
            div_det,
            history,
            latest,
            snap_hist,
            "BTC-USDT".to_string(),
            "BTC-USDT".to_string(),
            duration_seconds,
            slot_label,
            cancel,
            None,
            None,
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(VecDeque::with_capacity(60))),
            Arc::new(RwLock::new(VecDeque::with_capacity(8))),
            Arc::new(RwLock::new(None)),
            None,
            None, // heatmap_config (None)
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
            1,
            300,
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(
                core_domain::indicator_dtos::IndicatorLifecycleMap::new(),
            )),
            Arc::new(RwLock::new(
                core_domain::models::CandlePipelineState::Initializing,
            )),
        )
        .await
    })
}

async fn drain(
    broadcast_rx: &mut tokio::sync::broadcast::Receiver<core_domain::models::MarketSnapshot>,
) -> Vec<core_domain::models::MarketSnapshot> {
    let mut snapshots = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        match tokio::time::timeout(Duration::from_millis(100), broadcast_rx.recv()).await {
            Ok(Ok(snapshot)) => snapshots.push(snapshot),
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    snapshots
}

#[tokio::test]
async fn one_second_timeframe_shadow_broadcast_is_throttled() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) =
        tokio::sync::broadcast::channel::<core_domain::models::MarketSnapshot>(200);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer(1, event_rx, broadcast_tx.clone(), cancel.clone()).await;

    // Send 50 trades over ~500 ms with timestamps that all fall in the
    // same 1-second candle interval (timestamps 100..1500 ms, anchored at
    // 1000 ms so the first interval is [1000, 2000) and ticks at 100,
    // 110, 120, … 590 all land inside it).
    for i in 0..50u64 {
        let ts_ms = 100 + i * 10;
        event_tx
            .send(NormalizedEvent::Trade(NormalizedTrade {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".to_string(),
                price: dec!(50_000) + rust_decimal::Decimal::from(i),
                size: dec!(0.01),
                side: TradeSide::Buy,
                timestamp_ms: ts_ms,
                trade_id: format!("throttle_t{i}"),
            }))
            .await
            .expect("channel open");
    }

    tokio::time::sleep(Duration::from_millis(600)).await;
    let snapshots = drain(&mut broadcast_rx).await;

    // The throttle caps the SHADOW (live/flickering) broadcast path only.
    // Completed-candle closes — including the sub-minute force-close +
    // doji-fill path (v6.11), which now emits completed snapshots with a
    // populated indicators map so the EMA/Rsi/etc. advance per wall-clock
    // second — are NOT throttled and may legitimately exceed the shadow
    // budget. Count only `is_completed == Some(false)` frames.
    let shadow_frames: Vec<_> = snapshots
        .iter()
        .filter(|s| s.is_completed == Some(false))
        .collect();

    // At 1-s TF the throttle is 250 ms → at most 4 shadow broadcasts in a
    // 500 ms drain window, plus maybe one initial tick before the throttle
    // first fires. Allow generous headroom (≤ 8) to absorb timer jitter
    // without flaking on slower CI.
    assert!(
        shadow_frames.len() <= 8,
        "shadow broadcasts at 1-s TF must be throttled: got {} shadow frames for 50 ticks in 500 ms",
        shadow_frames.len()
    );
    // And we must have seen at least one — the throttle must not have
    // suppressed all broadcasts.
    assert!(
        !shadow_frames.is_empty(),
        "shadow broadcast must fire at least once per 1-s candle at 1-s TF"
    );

    cancel.cancel();
}

#[tokio::test]
async fn completed_candle_close_still_fires_under_throttle() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) =
        tokio::sync::broadcast::channel::<core_domain::models::MarketSnapshot>(200);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer(1, event_rx, broadcast_tx.clone(), cancel.clone()).await;

    // Two trades inside the first 1-s interval, then one trade in the
    // next interval. Crossing the boundary must produce is_completed=true.
    for (i, ts_ms) in [1100u64, 1300, 2100].iter().enumerate() {
        event_tx
            .send(NormalizedEvent::Trade(NormalizedTrade {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".to_string(),
                price: dec!(50_000) + rust_decimal::Decimal::from(i as u64),
                size: dec!(0.01),
                side: TradeSide::Buy,
                timestamp_ms: *ts_ms,
                trade_id: format!("close_t{i}"),
            }))
            .await
            .expect("channel open");
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshots = drain(&mut broadcast_rx).await;

    let completed: Vec<_> = snapshots
        .iter()
        .filter(|s| s.is_completed == Some(true))
        .collect();
    assert!(
        !completed.is_empty(),
        "crossing a candle boundary must still produce a completed snapshot under throttle: got {} snapshots total",
        snapshots.len()
    );
    assert_eq!(
        completed[0].timestamp, 1,
        "completed candle must carry the interval-start timestamp (1s)"
    );

    cancel.cancel();
}

#[tokio::test]
async fn sixty_second_timeframe_unaffected_by_throttle() {
    // Regression guard: at ≥60-s TFs the throttle interval (15 s) is
    // much smaller than the natural candle cadence (60 s), so the
    // shadow throttle has no effect. A 60-s pipeline with a few ticks
    // should still produce at least one shadow broadcast per candle
    // (and the throttle must not turn off the flickering path
    // entirely).
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (broadcast_tx, mut broadcast_rx) =
        tokio::sync::broadcast::channel::<core_domain::models::MarketSnapshot>(200);
    let cancel = CancellationToken::new();
    let _h = spawn_analyzer(60, event_rx, broadcast_tx.clone(), cancel.clone()).await;

    // Three trades inside the same 60-s interval.
    for (i, ts_ms) in [65_000u64, 70_000, 95_000].iter().enumerate() {
        event_tx
            .send(NormalizedEvent::Trade(NormalizedTrade {
                exchange: Exchange::Hyperliquid,
                symbol: "BTC-USDT".to_string(),
                price: dec!(50_000) + rust_decimal::Decimal::from(i as u64),
                size: dec!(0.01),
                side: TradeSide::Buy,
                timestamp_ms: *ts_ms,
                trade_id: format!("tf60_t{i}"),
            }))
            .await
            .expect("channel open");
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshots = drain(&mut broadcast_rx).await;
    assert!(
        !snapshots.is_empty(),
        "60-s TF must continue to emit shadow broadcasts (throttle interval is 15s, well below the 60s cadence)"
    );

    cancel.cancel();
}
