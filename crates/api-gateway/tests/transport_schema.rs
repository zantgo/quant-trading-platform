//! Phase 5 transport-layer schema tests: verify the WebSocket JSON-RPC
//! notification and the /api/history arrays serialize to the nested
//! dual-representation contract.

use api_gateway::types::{HistoricalIndicatorArrays, IndicatorHistoryArrays};
use core_domain::jsonrpc::JsonRpcNotification;
use core_domain::models::MarketSnapshot;
use market_analyzer::indicators::normalized::{DivergenceState, NormalizationEngine};
use std::collections::HashMap;

fn sample_snapshot() -> MarketSnapshot {
    use rust_decimal_macros::dec;
    let mut indicators = HashMap::new();
    indicators.insert(
        "rsi".to_string(),
        NormalizationEngine::normalize_rsi(28.5, DivergenceState::None),
    );
    indicators.insert(
        "macd".to_string(),
        NormalizationEngine::normalize_macd(-12.4, -17.6, 5.2, 8.0, Some(1)),
    );
    MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1_718_000_000,
        symbol: "BTC-USDT".to_string(),
        is_completed: Some(true),
        mid_price: dec!(65300.0),
        bid_price: dec!(65299.0),
        ask_price: dec!(65301.0),
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open: Some(dec!(65000.0)),
        high: Some(dec!(65400.0)),
        low: Some(dec!(64900.0)),
        close: Some(dec!(65300.0)),
        volume: Some(dec!(120.0)),
        average_volume: Some(dec!(100.0)),
        indicators,
        context: None,
        alignment: None,
        analysis: None,
        risk: None,
        advisory: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        statistical_context: None,
        decision_context: None,
        liquidity_signals: vec![],
        metrics_config: None,
        opportunity: None,
        risk_profile: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        quality_envelope: None,
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    }
}

#[test]
fn ws_notification_matches_nested_schema() {
    let snapshot = sample_snapshot();
    let payload = serde_json::to_value(&snapshot).unwrap();
    let notif = JsonRpcNotification::new(
        "broadcast.market_snapshot",
        serde_json::json!({
            "symbol": snapshot.symbol,
            "timeframe_secs": snapshot.timeframe_secs,
            "snapshot": payload,
        }),
    );

    let v = serde_json::to_value(&notif).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["method"], "broadcast.market_snapshot");
    assert_eq!(v["params"]["symbol"], "BTC-USDT");
    assert_eq!(v["params"]["timeframe_secs"], 60);

    // Nested indicators map with dual representation.
    let rsi = &v["params"]["snapshot"]["indicators"]["rsi"];
    assert_eq!(rsi["raw_value"], 28.5);
    assert!(rsi["normalized"].as_f64().unwrap() >= 0.70);
    assert_eq!(rsi["state_label"], "OVERSOLD_ACCUMULATION");

    // Multi-line indicator exposes its `values` sub-map.
    let macd = &v["params"]["snapshot"]["indicators"]["macd"];
    assert_eq!(macd["state_label"], "BULLISH_CROSSOVER_ACCELERATING");
    assert_eq!(macd["values"]["line"], -12.4);
    assert_eq!(macd["values"]["signal"], -17.6);
}

#[test]
fn history_arrays_serialize_nested_and_aligned() {
    let mut indicators: HashMap<String, HistoricalIndicatorArrays> = HashMap::new();
    let mut rsi = HistoricalIndicatorArrays::default();
    // Two aligned time steps: one present, one missing.
    rsi.push_value(&NormalizationEngine::normalize_rsi(
        25.0,
        DivergenceState::None,
    ));
    rsi.push_none();
    indicators.insert("rsi".to_string(), rsi);

    let arrays = IndicatorHistoryArrays {
        symbol: "BTC-USDT".to_string(),
        timeframe_secs: 60,
        times: vec![1000, 1060],
        indicators,
    };

    let v = serde_json::to_value(&arrays).unwrap();
    assert_eq!(v["symbol"], "BTC-USDT");
    assert_eq!(v["timeframe_secs"], 60);
    assert_eq!(v["times"].as_array().unwrap().len(), 2);

    let rsi = &v["indicators"]["rsi"];
    // Parallel arrays aligned to `times` (length 2), with null in the gap.
    assert_eq!(rsi["raw"].as_array().unwrap().len(), 2);
    assert_eq!(rsi["normalized"].as_array().unwrap().len(), 2);
    assert_eq!(rsi["state_label"].as_array().unwrap().len(), 2);
    assert_eq!(rsi["raw"][0], 25.0);
    assert!(rsi["raw"][1].is_null());
    assert_eq!(rsi["state_label"][0], "OVERSOLD_ACCUMULATION");
    assert!(rsi["state_label"][1].is_null());
}
