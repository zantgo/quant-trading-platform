use config_models::{FibonacciConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{NormalizedCandle, NormalizedEvent};
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use network_adapters::adapters;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_per_pair_ws_and_analyzer_cancellation_loop() {
    tokio::time::timeout(tokio::time::Duration::from_secs(10), async {
        let symbol = "BTC".to_string();
        let pair_key = "BTC-USDT".to_string();

        let (snapshot_tx, snapshot_rx) = mpsc::channel::<NormalizedEvent>(100);
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<MarketSnapshot>(100);
        let history = Arc::new(tokio::sync::RwLock::new(
            VecDeque::<NormalizedCandle>::with_capacity(100),
        ));
        let latest_snap = Arc::new(tokio::sync::RwLock::new(None::<MarketSnapshot>));
        let cancel = CancellationToken::new();
        let divergence_detector = Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20)));

        let (telemetry_tx, _telemetry_rx) = mpsc::channel(10);

        let test_workspace = config_models::WorkspaceConfig {
            active_timeframes: 10,
            id: "test".into(),
            name: "Test".into(),
            default_currency: "USDC".into(),
            default_exchange: "Hyperliquid".into(),
            portfolio_capital_usd: 1000.0,
            strategies: vec![config_models::StrategyConfig::default()],
            candles: config_models::CandlesConfig {
                duration_seconds: 60,
            },
            indicators: Default::default(),
            fast_timeframe: Default::default(),
            slow_timeframe: Default::default(),
            macro_timeframe: Default::default(),
            fibonacci: Default::default(),
            pivots: Default::default(),
            leverage: Default::default(),
            fees: Default::default(),
            defaults: Default::default(),
            safety: Default::default(),
            intervals: Default::default(),
            liquidity: Default::default(),
            heatmap: Default::default(),
            activation: Default::default(),
            opportunity_matrix: Default::default(),
            order_book: Default::default(),
            config_version: 1,
            api_failover: Default::default(),
            instances: Vec::new(),
            minimal_tae: Default::default(),
            analytics: Default::default(),
            risk_limits: Default::default(),
            execution: Default::default(),
            backtest: Default::default(),
            data_science: Default::default(),
        };
        let indicators = test_workspace.indicators.clone();
        let tf_cfg = TimeframeConfig::new(60, indicators);
        let fib_config = FibonacciConfig::default();

        let analyzer_cancel = cancel.clone();
        let analyzer_history = history.clone();
        let analyzer_latest_snap = latest_snap.clone();
        let analyzer_broadcast = broadcast_tx.clone();
        let analyzer_telemetry = telemetry_tx.clone();
        let analyzer_symbol = symbol.clone();
        let analyzer_pair_key = pair_key.clone();
        let analyzer_div_det = divergence_detector.clone();
        let analyzer_handle = tokio::spawn(async move {
            let strategy = config_models::StrategyConfig::default();
            analyzer::run_single(
                snapshot_rx,
                analyzer_telemetry,
                analyzer_broadcast,
                tf_cfg,
                fib_config,
                core_domain::statistics::StatisticsConfig::default(),
                analyzer_div_det,
                analyzer_history,
                analyzer_latest_snap,
                Arc::new(RwLock::new(VecDeque::new())),
                analyzer_symbol,
                analyzer_pair_key,
                60,
                "Micro",
                core_domain::models::TimeframeSlot::Micro1,
                analyzer_cancel,
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
                config_models::OrderBookConfig::default(),
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
        });

        let ws_cancel = cancel.clone();
        let ws_tx = snapshot_tx.clone();
        let ws_symbol = symbol.clone();
        let ws_internal = format!("{}-USDT", symbol);
        let ws_handle = tokio::spawn(async move {
            adapters::hyperliquid::run_for_symbol(
                ws_symbol,
                ws_internal,
                ws_tx,
                ws_cancel,
                "ws://127.0.0.1:1",
            )
            .await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        cancel.cancel();

        let ws_result = tokio::time::timeout(tokio::time::Duration::from_secs(5), ws_handle).await;

        assert!(
            ws_result.is_ok(),
            "WS ingestion task should exit cleanly when cancellation is triggered"
        );

        let analyzer_result =
            tokio::time::timeout(tokio::time::Duration::from_secs(5), analyzer_handle).await;

        assert!(
            analyzer_result.is_ok(),
            "Analysis task should exit cleanly when cancellation is triggered"
        );
    })
    .await
    .expect("Per-pair cancellation test timed out after 10 seconds");
}
