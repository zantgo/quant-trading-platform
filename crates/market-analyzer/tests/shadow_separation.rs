//! AC-L2-3 (03-01-04 §5.2): Shadow/live candles must never be sent as
//! completed snapshots in the broadcast channel. Only completed candles
//! (those that have crossed a candle boundary) produce `MarketSnapshot`
//! with `is_completed: Some(true)`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use rust_decimal_macros::dec;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

fn make_test_config() -> TimeframeConfig {
    use config_models::IndicatorsConfig;
    TimeframeConfig {
        candles: config_models::CandlesConfig {
            duration_seconds: 60,
        },
        indicators: IndicatorsConfig {
            ema_fast: 10,
            ema_medium: 50,
            ema_slow: 100,
            ema_long: 200,
            rsi_period: 14,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            adx_period: 14,
            atr_period: 14,
            squeeze_period: 20,
            squeeze_bb_period: 20,
            squeeze_bb_std_dev: 2.0,
            squeeze_kc_period: 20,
            stoch_k_period: 18,
            stoch_d_period: 5,
            stoch_s_period: 9,
            chandemo_period: 12,
            supertrend_period: 10,
            supertrend_multiplier: 3.0,
            keltner_ema_period: 20,
            keltner_atr_period: 10,
            keltner_multiplier: 2.0,
            donchian_period: 20,
            obv_smoothing: 20,
            cmf_period: 20,
            mfi_period: 14,
            hv_period: 20,
            aroon_period: 25,
            chop_period: 14,
            linreg_period: 20,
            zscore_period: 20,
            bbwp_lookback: 252,
            bbwp_period: 20,
            macd_extreme_high_threshold: 1000.0,
            macd_extreme_low_threshold: -1000.0,
            macd_histogram_contraction_threshold: 0.3,
            adx_trend_threshold: 20,
            adx_exhaustion_threshold: 40,
            adx_slope_lookback: 3,
            squeeze_min_duration: 5,
            squeeze_kc_atr_multiplier: 1.5,
            atr_multiplier_coefficient: 2.0,
            atr_target_rr_ratio: 2.5,
            volume_average_period: 20,
            rvol_threshold_institutional: 1.5,
            rvol_threshold_climax: 3.0,
            ichimoku_tenkan: 9,
            ichimoku_kijun: 26,
            ichimoku_senkou_b: 52,
            ichimoku_displacement: 26,
            cci_period: 20,
            psar_af_step: 0.02,
            psar_af_max: 0.2,
            williams_r_period: 14,
            hull_ma_period: 21,
            force_index_smoothing: 13,
            stddev_channel_period: 20,
            smc_lookback: 20,
            volume_profile_bins: 50,
            volume_profile_window: 500,
            volume_profile_value_area: 0.7,
            ..Default::default()
        },
        leverage: Default::default(),
    }
}

#[tokio::test]
async fn shadow_candles_never_completed() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (telemetry_tx, mut telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(500);
    let (broadcast_tx, mut broadcast_rx) =
        tokio::sync::broadcast::channel::<core_domain::models::MarketSnapshot>(200);
    let cancel = CancellationToken::new();

    tokio::spawn(async move { while telemetry_rx.recv().await.is_some() {} });

    let div_det = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(10)));
    let history = Arc::new(RwLock::new(VecDeque::new()));
    let latest = Arc::new(RwLock::new(None));
    let snap_hist = Arc::new(RwLock::new(VecDeque::new()));

    let analyzer_handle = tokio::spawn({
        let cancel = cancel.clone();
        let strategy = config_models::StrategyConfig::default();
        analyzer::run_single(
            event_rx,
            telemetry_tx,
            broadcast_tx,
            make_test_config(),
            FibonacciConfig::default(),
            core_domain::statistics::StatisticsConfig::default(),
            div_det,
            history,
            latest,
            snap_hist,
            "BTC-USDT".to_string(),
            "BTC-USDT".to_string(),
            60,
            "60s".to_string(),
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
    });

    // ── Phase 1: trades within the same 60s candle interval ──────────
    // Interval boundary is at multiples of 60000.
    // Timestamps 65000, 75000, 95000 all fall into interval [60000, 120000).
    // Using the second interval avoids the t_last_hist=0 filter
    // (candle start_time_ms=60000 > 0).
    event_tx
        .send(NormalizedEvent::Trade(NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price: dec!(50_000),
            size: dec!(0.5),
            side: TradeSide::Buy,
            timestamp_ms: 65_000,
            trade_id: "t0".to_string(),
        }))
        .await
        .expect("channel open");
    event_tx
        .send(NormalizedEvent::Trade(NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price: dec!(50_100),
            size: dec!(0.5),
            side: TradeSide::Buy,
            timestamp_ms: 75_000,
            trade_id: "t1".to_string(),
        }))
        .await
        .expect("channel open");
    event_tx
        .send(NormalizedEvent::Trade(NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price: dec!(49_900),
            size: dec!(0.5),
            side: TradeSide::Buy,
            timestamp_ms: 95_000,
            trade_id: "t2".to_string(),
        }))
        .await
        .expect("channel open");

    // ── Phase 2: cross the candle boundary ──────────────────────────
    // timestamp 121000 falls into interval [120000, 180000), which
    // completes the candle for interval [60000, 120000).
    event_tx
        .send(NormalizedEvent::Trade(NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price: dec!(51_000),
            size: dec!(0.5),
            side: TradeSide::Buy,
            timestamp_ms: 121_000,
            trade_id: "t3".to_string(),
        }))
        .await
        .expect("channel open");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Collect all snapshots from the broadcast channel.
    let mut snapshots: Vec<core_domain::models::MarketSnapshot> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match tokio::time::timeout(Duration::from_millis(200), broadcast_rx.recv()).await {
            Ok(Ok(snapshot)) => {
                snapshots.push(snapshot);
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }

    // Assertion 1: there must be at least one completed snapshot.
    let completed_snapshots: Vec<_> = snapshots
        .iter()
        .filter(|s| s.is_completed == Some(true))
        .collect();

    assert!(
        !completed_snapshots.is_empty(),
        "crossing a candle boundary must produce a completed snapshot (is_completed: Some(true)). Got {} snapshots total.",
        snapshots.len()
    );

    // Assertion 2: the completed snapshot must carry full OHLCV.
    let cs = completed_snapshots[0];
    assert!(cs.open.is_some(), "completed snapshot must have open");
    assert!(cs.high.is_some(), "completed snapshot must have high");
    assert!(cs.low.is_some(), "completed snapshot must have low");
    assert!(cs.close.is_some(), "completed snapshot must have close");
    assert!(cs.volume.is_some(), "completed snapshot must have volume");

    // Assertion 3: the completed snapshot timestamp matches the interval start (60s).
    assert_eq!(
        cs.timestamp, 60,
        "completed candle timestamp should be interval start (60s)"
    );

    // Assertion 4: shadow/live snapshots must only carry is_completed: Some(false).
    for s in &snapshots {
        if s.is_completed == Some(true) {
            continue;
        }
        assert_eq!(
            s.is_completed,
            Some(false),
            "live/shadow snapshot must have is_completed: Some(false), got {:?}",
            s.is_completed
        );
    }

    cancel.cancel();
    let _ = analyzer_handle.await;
}
