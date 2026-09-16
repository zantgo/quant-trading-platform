use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{
    Exchange, NormalizedCandle, NormalizedEvent, NormalizedTrade, TradeSide,
};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;

#[tokio::test]
async fn test_fixed_ladder_fanout_history_cap_100_and_broadcast() {
    tokio::time::timeout(tokio::time::Duration::from_secs(30), async {
        let symbol = "ZZZ".to_string();
        let pair_key = "Hyperliquid-ZZZ".to_string();

        let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);

        // One event channel per fixed-ladder slot (10 total).
        let mut pipeline_txs: Vec<mpsc::Sender<NormalizedEvent>> = Vec::with_capacity(10);
        let mut pipeline_rxs: Vec<mpsc::Receiver<NormalizedEvent>> = Vec::with_capacity(10);
        for _ in 0..10 {
            let (tx, rx) = mpsc::channel::<NormalizedEvent>(200);
            pipeline_txs.push(tx);
            pipeline_rxs.push(rx);
        }

        // Broadcast channels — subscribe to verify snapshot delivery on
        // the two slots this test tracks (fastest + third slot).
        let broadcast_txs: [broadcast::Sender<MarketSnapshot>; 10] =
            std::array::from_fn(|_| broadcast::channel::<MarketSnapshot>(120).0);
        let mut micro_bcast_rx = broadcast_txs[0].subscribe();
        let mut fast_bcast_rx = broadcast_txs[2].subscribe();

        // Live FIFO cap enforced by `run_single` on every slot (the
        // legacy "cap 100" was the natural data bound of 30s candles over
        // the simulated 50-minute window; the fixed ladder's 1s slot
        // legitimately produces more, and the real invariant is the
        // HIST_BUFFER_MAX trim).
        let cap = analyzer::HIST_BUFFER_MAX;
        let histories: [Arc<RwLock<VecDeque<NormalizedCandle>>>; 10] =
            std::array::from_fn(|_| {
                Arc::new(RwLock::new(VecDeque::<NormalizedCandle>::with_capacity(
                    120,
                )))
            });
        let latests: [Arc<RwLock<Option<MarketSnapshot>>>; 10] =
            std::array::from_fn(|_| Arc::new(RwLock::new(None::<MarketSnapshot>)));

        let cancel = CancellationToken::new();
        let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(10);

        let indicators = config_models::IndicatorsConfig {
            ema_fast: 5,
            ema_medium: 10,
            ema_slow: 20,
            ema_long: 30,
            rsi_period: 5,
            adx_period: 5,
            adx_trend_threshold: 20,
            adx_exhaustion_threshold: 40,
            adx_slope_lookback: 3,
            squeeze_period: 5,
            squeeze_min_duration: 2,
            bbwp_lookback: 10,
            bbwp_period: 5,
            atr_period: 5,
            ..Default::default()
        };

        let fib_config = FibonacciConfig {
            swing_lookback: 5,
            swing_scan_range: 20,
            retracement_coefficients: vec![0.618, 0.660],
            extension_coefficients: vec![1.618, 2.618],
        };

        // Per-slot TimeframeConfigs over the fixed ladder.
        let ladder_cfgs: [TimeframeConfig; 10] = std::array::from_fn(|i| {
            TimeframeConfig::new(config_models::FIXED_TF_LADDER[i], indicators.clone())
        });

        // Event router fanning out to all 10 fixed-ladder timeframes
        let router_cancel = cancel.clone();
        let router_symbol = symbol.clone();
        tokio::spawn(async move {
            analyzer::run_event_router(event_rx, pipeline_txs, router_symbol, router_cancel).await;
        });

        let spawn_analyzer = |rx,
                              broadcast,
                              tf_cfg: TimeframeConfig,
                              fib: FibonacciConfig,
                              div_det: Arc<tokio::sync::Mutex<DivergenceDetector>>,
                              history: Arc<RwLock<VecDeque<NormalizedCandle>>>,
                              latest: Arc<RwLock<Option<MarketSnapshot>>>,
                              symbol: String,
                              pk: String,
                              secs: u64,
                              label: &'static str,
                              slot: core_domain::models::TimeframeSlot,
                              cancel: CancellationToken| {
            let t = telemetry_tx.clone();
            tokio::spawn(async move {
                let strategy = config_models::StrategyConfig::default();
                analyzer::run_single(
                    rx,
                    t,
                    broadcast,
                    tf_cfg,
                    fib,
                    core_domain::statistics::StatisticsConfig::default(),
                    div_det,
                    history,
                    latest,
                    Arc::new(RwLock::new(VecDeque::new())),
                    symbol,
                    pk,
                    secs,
                    label,
                    slot,
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
                    None,
                    OrderBookConfig::default(),
                    strategy,
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
                .await;
            })
        };

        let div_dets: [Arc<tokio::sync::Mutex<DivergenceDetector>>; 10] =
            std::array::from_fn(|_| Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(10))));

        // Long-lived history for async borrow in future (keeps references alive)
        let _ = (&histories[0], &histories[2]);

        // Spawn all 10 fixed-ladder pipelines.
        let mut handles = Vec::new();
        for i in 0..10 {
            let rx = pipeline_rxs.remove(0);
            let handle = spawn_analyzer(
                rx,
                broadcast_txs[i].clone(),
                ladder_cfgs[i].clone(),
                fib_config.clone(),
                div_dets[i].clone(),
                histories[i].clone(),
                latests[i].clone(),
                symbol.clone(),
                pair_key.clone(),
                config_models::FIXED_TF_LADDER[i],
                config_models::FIXED_TF_NAMES[i],
                core_domain::models::FIXED_TF_SLOTS[i],
                cancel.clone(),
            );
            handles.push(handle);
        }

        // Timestamps spaced 60s apart, 50 trades = 3000 seconds → the
        // sub-second slots fill quickly (then evict at the cap) while the
        // ≥60s slots accumulate one candle per trade.
        let base_price = 50000.0f64;
        let mut ts = 0u64;
        let total_trades = 50;
        for i in 0..total_trades {
            let price = base_price + (i as f64 * 0.4).sin() * 200.0 + (i as f64 * 0.05);
            let trade = NormalizedTrade {
                exchange: Exchange::Hyperliquid,
                symbol: format!("{}-USD", symbol),
                price: rust_decimal::Decimal::from_f64_retain(price).unwrap_or(dec!(50000.00)),
                size: dec!(0.5) + rust_decimal::Decimal::from(i % 4),
                side: if i % 3 == 0 {
                    TradeSide::Sell
                } else {
                    TradeSide::Buy
                },
                timestamp_ms: ts,
                trade_id: format!("t_{}", i),
            };
            event_tx.send(NormalizedEvent::Trade(trade)).await.unwrap();
            ts += 60000; // 1-minute step increments
        }

        // Let pipelines process
        tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;

        let micro_count = histories[0].read().await.len();
        let fast_count = histories[2].read().await.len();
        eprintln!(
            "History counts — Micro1({}s): {}, Fast1({}s): {}",
            config_models::FIXED_TF_LADDER[0],
            micro_count,
            config_models::FIXED_TF_LADDER[2],
            fast_count
        );

        assert!(
            micro_count <= cap,
            "Micro1 history capped at {}; got {}",
            cap,
            micro_count
        );
        assert!(
            fast_count <= cap,
            "Fast1 history capped at {}; got {}",
            cap,
            fast_count
        );

        let total = micro_count + fast_count;
        assert!(
            total > 0,
            "At least one timeframe should produce candle history"
        );

        // Broadcast verification
        let micro_snaps = drain_broadcast(&mut micro_bcast_rx);
        let fast_snaps = drain_broadcast(&mut fast_bcast_rx);
        eprintln!(
            "Broadcast snapshots — Micro1: {}, Fast1: {}",
            micro_snaps, fast_snaps
        );

        let total_snaps = micro_snaps + fast_snaps;
        assert!(
            total_snaps > 0,
            "At least one broadcast channel should have delivered snapshots"
        );

        // Verify latest snapshots
        let has_latest =
            latests[0].read().await.is_some() || latests[2].read().await.is_some();
        assert!(
            has_latest,
            "At least one timeframe should have a latest snapshot"
        );

        // FIFO eviction: if at capacity, oldest should have valid timestamps
        if micro_count >= 10 {
            let sh = histories[0].read().await;
            let oldest_ts = sh.front().unwrap().start_time_ms;
            assert!(oldest_ts > 0, "Oldest candle timestamp should be > 0");
        }

        cancel.cancel();
        for h in handles {
            h.abort();
        }
    })
    .await
    .expect("Fixed-ladder integration test timed out");
}

fn drain_broadcast(rx: &mut broadcast::Receiver<MarketSnapshot>) -> usize {
    let mut count = 0;
    loop {
        match rx.try_recv() {
            Ok(_) => count += 1,
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                // Lag proves the sender delivered `skipped` additional
                // snapshots that the buffer could not retain for this
                // consumer (expected with the sub-second ladder slots).
                count += skipped as usize;
            }
            Err(_) => break,
        }
    }
    count
}
