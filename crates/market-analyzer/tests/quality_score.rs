//! AC-L3-6 (03-01-04 §7.1): `CandleQualityEnvelope.quality_score` for a
//! fully-valid candle (no gap, no spike, no staleness, valid integrity)
//! equals 100. Exercises the full DIE L2→L3→L4 path through
//! `analyzer::run_single`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
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

#[tokio::test]
async fn pristine_candle_scores_100() {
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(500);
    let (telemetry_tx, mut telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(500);
    let (broadcast_tx, mut broadcast_rx) =
        tokio::sync::broadcast::channel::<core_domain::models::MarketSnapshot>(200);
    let cancel = CancellationToken::new();

    // Drain telemetry so the channel never fills.
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
            "Micro",
            core_domain::models::TimeframeSlot::Micro1,
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
            5,
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

    // Clean, gap-free tick stream: one trade per minute crossing boundaries.
    for i in 0..30u64 {
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
            timestamp_ms: i * 60_000,
            trade_id: format!("t{i}"),
        };
        event_tx
            .send(NormalizedEvent::Trade(trade))
            .await
            .expect("event channel open");
    }

    // Collect completed snapshots from the L4 broadcast.
    let mut completed_seen = 0u32;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while completed_seen < 5 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv()).await {
            Ok(Ok(snapshot)) => {
                if snapshot.is_completed == Some(true) {
                    completed_seen += 1;
                    let envelope = snapshot
                        .quality_envelope
                        .as_ref()
                        .expect("completed snapshot carries CandleQualityEnvelope");
                    assert!(envelope.is_valid, "pristine candle must be valid");
                    assert!(!envelope.is_gap_filled, "no gap in this stream");
                    assert!(!envelope.had_outliers_rejected, "no spikes in this stream");
                    assert_eq!(
                        envelope.quality_score, 100.0,
                        "fully-valid candle scores exactly 100"
                    );
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
    }

    assert!(
        completed_seen >= 3,
        "expected several completed snapshots, saw {completed_seen}"
    );

    cancel.cancel();
    let _ = analyzer_handle.await;
}
