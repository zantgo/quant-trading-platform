use core_domain::models::MarketSnapshot;
use core_domain::normalized::Exchange;
use core_domain::TriggerType;
use market_analyzer::indicators::normalized::{NormalizationEngine, NormalizedIndicatorValue};
use rust_decimal_macros::dec;
use std::collections::HashMap;

fn sample_indicators() -> HashMap<String, NormalizedIndicatorValue> {
    let mut map = HashMap::new();
    map.insert(
        "rsi".to_string(),
        NormalizationEngine::normalize_rsi(65.0, Default::default()),
    );
    map.insert(
        "macd".to_string(),
        NormalizationEngine::normalize_macd(20.0, 15.0, 5.0, 8.0, None),
    );
    map.insert(
        "rvol".to_string(),
        NormalizationEngine::normalize_rvol(1.25, 1.5, 3.0),
    );
    map
}

#[test]
fn test_market_snapshot_json_roundtrip() {
    let snap = MarketSnapshot {
        timeframe_label: Some("1m".to_string()),
        exchange: Some(Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1718000000,
        symbol: "BTC".to_string(),
        is_completed: Some(true),
        mid_price: dec!(50000.00),
        bid_price: dec!(49999.50),
        ask_price: dec!(50000.50),
        bid_size: Some(dec!(1.5)),
        ask_size: Some(dec!(2.0)),
        funding_rate: Some(dec!(0.0001)),
        open: Some(dec!(49800.00)),
        high: Some(dec!(50200.00)),
        low: Some(dec!(49750.00)),
        close: Some(dec!(50000.00)),
        volume: Some(dec!(150.0)),
        average_volume: Some(dec!(120.0)),
        indicators: sample_indicators(),
        context: None,
        alignment: None,
        analysis: None,
        risk: None,
        advisory: None,
        decision_context: None,
        statistical_context: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        liquidity_signals: vec![],
        metrics_config: None,
        opportunity: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        risk_profile: None,
        quality_envelope: None,
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    };

    let json = serde_json::to_string(&snap).expect("serialization should succeed");
    let parsed: MarketSnapshot =
        serde_json::from_str(&json).expect("deserialization should succeed");

    assert_eq!(parsed.symbol, "BTC");
    assert_eq!(parsed.mid_price, dec!(50000.00));
    assert_eq!(parsed.exchange, Some(Exchange::Hyperliquid));

    let rsi = parsed.indicators.get("rsi").expect("rsi present");
    assert_eq!(rsi.raw_value, 65.0);
    assert!(rsi.normalized <= 1.0 && rsi.normalized >= -1.0);

    let macd = parsed.indicators.get("macd").expect("macd present");
    let macd_vals = macd.values.as_ref().expect("macd carries line/signal");
    assert_eq!(macd_vals.get("line"), Some(&20.0));
    assert_eq!(macd_vals.get("signal"), Some(&15.0));

    let rvol = parsed.indicators.get("rvol").expect("rvol present");
    assert_eq!(rvol.state_label, "NORMAL_PARTICIPATION_VOLUME");
}

#[test]
fn test_market_snapshot_empty_indicators() {
    let snap = MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: None,
        timeframe_secs: 0,
        timestamp: 0,
        symbol: "EMPTY".to_string(),
        is_completed: None,
        mid_price: dec!(0.0),
        bid_price: dec!(0.0),
        ask_price: dec!(0.0),
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open: None,
        high: None,
        low: None,
        close: None,
        volume: None,
        average_volume: None,
        indicators: HashMap::new(),
        context: None,
        alignment: None,
        analysis: None,
        risk: None,
        advisory: None,
        decision_context: None,
        statistical_context: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        liquidity_signals: vec![],
        metrics_config: None,
        opportunity: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        risk_profile: None,
        quality_envelope: None,
        pipeline_state: core_domain::models::CandlePipelineState::default(),
        indicator_lifecycle: std::collections::HashMap::new(),
    };

    let json = serde_json::to_string(&snap).expect("serialization of empty snap should succeed");
    let parsed: MarketSnapshot =
        serde_json::from_str(&json).expect("deserialization of empty snap should succeed");

    assert_eq!(parsed.symbol, "EMPTY");
    assert_eq!(parsed.mid_price, dec!(0.0));
    assert!(parsed.exchange.is_none());
    assert!(parsed.indicators.is_empty());
}

#[test]
fn test_trigger_type_manual_serde() {
    let manual = TriggerType::Manual;
    let json = serde_json::to_string(&manual).unwrap();
    assert_eq!(json, "\"Manual\"");
    let parsed: TriggerType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, TriggerType::Manual);
}

#[test]
fn test_trigger_type_automated_serde() {
    let auto = TriggerType::Automated;
    let json = serde_json::to_string(&auto).unwrap();
    assert_eq!(json, "\"Automated\"");
    let parsed: TriggerType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, TriggerType::Automated);
}
