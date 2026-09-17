//! AC-DIE-3 / AC-L4-2: End-to-end observation loop latency test.
//! Raw frame → completed snapshot broadcast p95 < 500ms (debug CI;
//! spec target < 25ms in release). Also verifies completed-snapshot
//! schema and quality_envelope presence.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
use database_storage::TelemetryMsg;
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use rust_decimal::Decimal;
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

fn verify_completed_snapshot(snapshot: &MarketSnapshot) {
    assert!(
        snapshot.is_completed == Some(true),
        "expected completed snapshot"
    );
    assert_eq!(snapshot.symbol, "BTC-USDT", "symbol must match");
    assert_eq!(snapshot.timeframe_secs, 60, "timeframe must be 60s");

    assert!(snapshot.open.is_some(), "completed snapshot must have open");
    assert!(snapshot.high.is_some(), "completed snapshot must have high");
    assert!(snapshot.low.is_some(), "completed snapshot must have low");
    assert!(
        snapshot.close.is_some(),
        "completed snapshot must have close"
    );
    assert!(
        snapshot.volume.is_some(),
        "completed snapshot must have volume"
    );

    let envelope = snapshot
        .quality_envelope
        .as_ref()
        .expect("completed snapshot must carry quality_envelope");
    assert!(envelope.is_valid, "envelope must be valid");
}

#[tokio::test]
async fn observation_loop_latency_p95_below_threshold() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (telemetry_tx, mut telemetry_rx) = mpsc::channel::<TelemetryMsg>(500);
    let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel::<MarketSnapshot>(200);
    let cancel = CancellationToken::new();

    tokio::spawn(async move { while telemetry_rx.recv().await.is_some() {} });

    let div_det = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(10)));
    let history = Arc::new(RwLock::new(VecDeque::new()));
    let latest = Arc::new(RwLock::new(None));
    let snap_hist = Arc::new(RwLock::new(VecDeque::new()));

    let analyzer_handle = tokio::spawn({
        let cancel = cancel.clone();
        analyzer::run_single(
            event_rx,
            telemetry_tx,
            broadcast_tx.clone(),
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
            "1m".to_string(),
            cancel,
            None,
            None,
            // latest_oi, latest_funding, latest_mark_px, latest_index_px
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            // oi_history, funding_history (derivatives warmup replay)
            Arc::new(RwLock::new(VecDeque::with_capacity(60))),
            Arc::new(RwLock::new(VecDeque::with_capacity(8))),
            // cluster_matrix
            Arc::new(RwLock::new(None)),
            None,
            None, // heatmap_config (None → defaults)
            OrderBookConfig::default(),
            config_models::StrategyConfig::default(),
            // cross_tf_snapshots (no siblings wired in this test)
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

    let timestamp_step = make_test_config().candles.duration_seconds * 1000;

    // Open the first candle at ts=60000 so start_time_ms > 0 (avoids
    // the t_last_hist filter in analyzer::run_single that discards the
    // epoch-aligned candle at ts=0).
    let t0 = NormalizedTrade {
        exchange: Exchange::Hyperliquid,
        symbol: "BTC-USDT".to_string(),
        price: dec!(50_000),
        size: dec!(0.5),
        side: TradeSide::Buy,
        timestamp_ms: timestamp_step,
        trade_id: "t0".to_string(),
    };
    event_tx
        .send(NormalizedEvent::Trade(t0))
        .await
        .expect("event channel open");

    let mut latencies: Vec<Duration> = Vec::with_capacity(30);

    for i in 1u64..=30 {
        let send_instant = Instant::now();
        let trade = NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price: dec!(50_000) + Decimal::from(i),
            size: dec!(0.5),
            side: if i % 2 == 0 {
                TradeSide::Buy
            } else {
                TradeSide::Sell
            },
            timestamp_ms: (i + 1) * timestamp_step,
            trade_id: format!("t{i}"),
        };
        event_tx
            .send(NormalizedEvent::Trade(trade))
            .await
            .expect("event channel open");

        loop {
            match tokio::time::timeout(Duration::from_secs(5), broadcast_rx.recv()).await {
                Ok(Ok(snapshot)) => {
                    if snapshot.is_completed == Some(true) {
                        let latency = send_instant.elapsed();
                        latencies.push(latency);
                        verify_completed_snapshot(&snapshot);
                        break;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                Err(_) => break,
            }
        }
    }

    assert!(
        !latencies.is_empty(),
        "expected at least one completed snapshot latency measurement"
    );
    assert_eq!(
        latencies.len(),
        30,
        "expected 30 completed snapshots, got {}",
        latencies.len()
    );

    latencies.sort_unstable();
    let p95_idx = ((latencies.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = latencies[p95_idx];
    let max = latencies.last().unwrap();
    let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;

    println!(
        "observation loop latency: n={} avg={avg:?} p95={p95:?} max={max:?}",
        latencies.len(),
    );

    assert!(
        p95 < Duration::from_millis(500),
        "p95 latency {p95:?} exceeds 500ms debug threshold (spec: <25ms release)"
    );

    cancel.cancel();
    let _ = analyzer_handle.await;
}
