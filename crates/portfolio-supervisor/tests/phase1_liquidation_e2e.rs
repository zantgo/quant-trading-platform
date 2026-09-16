//! Phase 1 integration test: Liquidation event flows through the analyzer
//! into a `MarketSnapshot.liquidity` field.

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{
    Exchange, LiquidationEvent, LiquidationSide, NormalizedEvent, NormalizedTrade, TradeSide,
};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

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
            squeeze_bb_period: 20,
            squeeze_bb_std_dev: 2.0,
            squeeze_kc_period: 20,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn liquidation_event_appears_in_completed_snapshot_liquidity_field() {
    let (telemetry_tx, _telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(1000);
    let (event_tx, event_rx) = mpsc::channel::<NormalizedEvent>(100);
    let (broadcast_tx, _) = broadcast::channel::<MarketSnapshot>(200);
    let history = Arc::new(RwLock::new(VecDeque::with_capacity(50)));
    let latest = Arc::new(RwLock::new(None::<MarketSnapshot>));
    let snap_hist = Arc::new(RwLock::new(VecDeque::with_capacity(50)));

    let cancel = CancellationToken::new();
    let div_det = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20)));
    let (candle_tx, _) = mpsc::channel::<core_domain::normalized::NormalizedCandle>(100);

    let analyzer_handle = tokio::spawn({
        let cancel = cancel.clone();
        let symbol = "BTC-USDT".to_string();
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
            latest.clone(),
            snap_hist,
            symbol,
            "Hyperliquid:BTC".to_string(),
            60,
            "Micro",
            core_domain::models::TimeframeSlot::Micro1,
            cancel,
            Some(candle_tx),
            None,
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
            // oi_history + funding_history (Block B derivatives warmup replay)
            Arc::new(RwLock::new(VecDeque::with_capacity(60))),
            Arc::new(RwLock::new(VecDeque::with_capacity(8))),
            Arc::new(RwLock::new(None)),
            None, // liquidity_config (None → cascade defaults)
            None, // heatmap_config (None → default 0.1% / 24h)
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
    });

    // First, send 25 normal trades to fill the indicator warmup period.
    for i in 0..25u64 {
        let price = dec!(50000.00) + Decimal::from(i) * dec!(10);
        let trade = NormalizedTrade {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            price,
            size: dec!(0.5),
            side: if i % 2 == 0 {
                TradeSide::Buy
            } else {
                TradeSide::Sell
            },
            timestamp_ms: i * 60_000,
            trade_id: format!("t_{}", i),
        };
        event_tx.send(NormalizedEvent::Trade(trade)).await.unwrap();
    }

    // Now send a liquidation event.
    let liq = LiquidationEvent {
        exchange: Exchange::Hyperliquid,
        symbol: "BTC-USDT".to_string(),
        side: LiquidationSide::Long,
        price: dec!(50500.00),
        size: dec!(2.0),
        timestamp_ms: 1_500_000,
        venue_order_id: Some("liq_001".to_string()),
    };
    event_tx
        .send(NormalizedEvent::Liquidation(liq.clone()))
        .await
        .unwrap();

    // One more trade to close the next candle.
    let next_trade = NormalizedTrade {
        exchange: Exchange::Hyperliquid,
        symbol: "BTC-USDT".to_string(),
        price: dec!(50600.00),
        size: dec!(0.5),
        side: TradeSide::Buy,
        timestamp_ms: 1_560_000, // 60s after the liquidation
        trade_id: "t_26".to_string(),
    };
    event_tx
        .send(NormalizedEvent::Trade(next_trade))
        .await
        .unwrap();

    // Wait for processing.
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // The latest snapshot should have liquidity populated.
    let snap = latest.read().await.clone();
    assert!(snap.is_some(), "expected at least one completed snapshot");
    let snap: MarketSnapshot = snap.unwrap();
    assert!(
        snap.liquidity.is_some(),
        "liquidity must be populated on completed snapshot"
    );
    let liq_flow = snap.liquidity.unwrap();
    // The long liquidation we injected (50500 * 2 = 101000) should appear
    // in the per-bar flow.
    assert!(
        liq_flow.long_liquidations_usd > 0.0,
        "long liquidations must be > 0 after the injected event, got {}",
        liq_flow.long_liquidations_usd
    );
    assert!(
        liq_flow.event_count >= 1,
        "expected at least 1 event in this bar"
    );

    drop(event_tx);
    cancel.cancel();
    let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), analyzer_handle).await;
}
