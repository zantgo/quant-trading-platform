// TEST-CORE: Configurable Data Activation (CA-06 / CA-15) — end-to-end.
//
// Regression for the audit finding that `ActiveSet::from_config` had no
// production callers: the `[activation]` surface (disabled indicators,
// disabled signal kinds, liquidity sub-toggles) was parsed but never
// applied. These tests drive the real `run_single` pipeline with an
// ActiveSet built from a config and assert the wire contract:
//   - disabled indicator keys are ABSENT from the snapshot `indicators`
//     map (CA-06: disabled ≡ absent ≡ NO_DATA);
//   - `metrics_config` is attached and attributes the config_version;
//   - `liquidity_signals_enabled = false` suppresses LiquiditySignal
//     emission while `liquidity` (LiquidityFlow) stays present (CA-15).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use config_models::{FibonacciConfig, OrderBookConfig, TimeframeConfig};
use core_domain::normalized::{Exchange, NormalizedEvent, NormalizedTrade, TradeSide};
use market_analyzer::active_set::ActiveSet;
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

async fn drive_pipeline(
    active_set: ActiveSet,
) -> (
    Vec<core_domain::models::MarketSnapshot>,
    tokio::task::JoinHandle<()>,
    CancellationToken,
) {
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
            "SLOW2",
            core_domain::models::TimeframeSlot::Slow2,
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
            // Sibling latest-snapshot handles — none in this single-pipeline test.
            Vec::new(),
            Arc::new(core_domain::LatencyTracker::default()),
            active_set,
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

    // Clean gap-free tick stream: one trade per minute crossing boundaries.
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

    let mut completed: Vec<core_domain::models::MarketSnapshot> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while completed.len() < 5 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv()).await {
            Ok(Ok(snapshot)) => {
                if snapshot.is_completed == Some(true) {
                    completed.push(snapshot);
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
    }

    (completed, analyzer_handle, cancel)
}

#[tokio::test]
async fn disabled_indicators_are_absent_and_metrics_config_attached() {
    // CA-06: `disabled_indicators = ["rsi"]` ⇒ the RSI key is absent from
    // the emitted `indicators` map (not a WARMING placeholder, not a zero
    // entry), and the snapshot carries `metrics_config` with the change
    // attribution.
    let mut active = ActiveSet::all_enabled();
    active.disabled_indicators.insert("rsi".to_string());
    active.config_version = 42;

    let (completed, _handle, _cancel) = drive_pipeline(active).await;
    assert!(
        completed.len() >= 2,
        "expected completed snapshots, got {}",
        completed.len()
    );

    let snap = completed.first().unwrap();
    assert!(
        !snap.indicators.contains_key("rsi"),
        "disabled indicator 'rsi' must be ABSENT from the indicators map"
    );
    for s in &completed {
        assert!(
            !s.indicators.contains_key("rsi"),
            "disabled indicator 'rsi' must never appear, even on later bars"
        );
    }
    // The live map is sparse during warmup (bars_required gates), so pin
    // presence on an indicator with bars_required == 1 (ema_stack), which
    // rides every frame after the first bar.
    assert!(
        completed
            .iter()
            .any(|s| s.indicators.contains_key("ema_stack")),
        "enabled indicators must still be present (ema_stack)"
    );

    let mc = snap
        .metrics_config
        .as_ref()
        .expect("metrics_config must be attached when indicators are disabled");
    assert_eq!(mc.disabled_indicators, vec!["rsi".to_string()]);
    assert_eq!(mc.config_version, 42);
}

#[tokio::test]
async fn signals_sub_toggle_suppresses_emission_but_keeps_flow() {
    // CA-15: `liquidity_signals_enabled = false` (master liquidity still
    // enabled) ⇒ `liquidity_signals` is empty on the wire while the
    // `liquidity` LiquidityFlow payload stays attached.
    let mut active = ActiveSet::all_enabled();
    active.liquidity_signals_enabled = false;

    let (completed, _handle, _cancel) = drive_pipeline(active).await;
    assert!(
        completed.len() >= 2,
        "expected completed snapshots, got {}",
        completed.len()
    );

    let snap = completed.first().unwrap();
    assert!(
        snap.liquidity.is_some(),
        "LiquidityFlow must stay present when only the signals sub-toggle is off"
    );
    assert!(
        snap.liquidity_signals.is_empty(),
        "LiquiditySignal emission must be suppressed"
    );
}

#[tokio::test]
async fn disabled_signal_kinds_and_pairs_are_filtered_from_wire() {
    // AUDIT-H2: the `[activation]` signal denylist was parsed and
    // advertised in `metrics_config` but never enforced. A globally
    // disabled kind and a (indicator, kind) pair must both be absent
    // from every emitted snapshot, while unaffected signals survive.
    let mut active = ActiveSet::all_enabled();
    active
        .disabled_signal_kinds
        .insert("VolumeClimax".to_string());
    active
        .disabled_signals
        .insert(("rsi".to_string(), "Threshold".to_string()));
    active.config_version = 7;

    let (completed, _handle, _cancel) = drive_pipeline(active).await;
    assert!(
        completed.len() >= 2,
        "expected completed snapshots, got {}",
        completed.len()
    );

    let any_signal = |snap: &core_domain::models::MarketSnapshot| -> bool {
        snap.indicators.values().any(|v| !v.signals.is_empty())
    };
    assert!(
        completed.iter().any(any_signal),
        "unrelated signals must survive the denylist filter"
    );

    for snap in &completed {
        for entry in snap.indicators.values() {
            for sig in &entry.signals {
                assert!(
                    !matches!(
                        sig.kind,
                        core_domain::indicator_dtos::SignalKind::VolumeClimax
                    ),
                    "disabled kind VolumeClimax must never reach the wire"
                );
            }
        }
        if let Some(rsi) = snap.indicators.get("rsi") {
            assert!(
                rsi.signals.iter().all(|sig| !matches!(
                    sig.kind,
                    core_domain::indicator_dtos::SignalKind::Threshold
                )),
                "disabled pair rsi:Threshold must never reach the wire"
            );
        }
    }

    let mc = completed
        .first()
        .and_then(|s| s.metrics_config.as_ref())
        .expect("metrics_config must be attached when signals are disabled");
    assert_eq!(mc.config_version, 7);
}

#[tokio::test]
async fn liquidity_master_off_absents_the_whole_chain() {
    // CA-15: master `enabled = false` ⇒ `liquidity`/`cluster`/
    // `liquidity_signals` all absent from the snapshot.
    let mut active = ActiveSet::all_enabled();
    active.liquidity_enabled = false;

    let (completed, _handle, _cancel) = drive_pipeline(active).await;
    assert!(
        completed.len() >= 2,
        "expected completed snapshots, got {}",
        completed.len()
    );

    let snap = completed.first().unwrap();
    assert!(
        snap.liquidity.is_none(),
        "liquidity must be absent (master off)"
    );
    assert!(
        snap.cluster.is_none(),
        "cluster must be absent (master off)"
    );
    assert!(
        snap.liquidity_signals.is_empty(),
        "liquidity_signals must be absent (master off)"
    );
}
