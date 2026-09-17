// Snapshot roundtrip integration test. The earlier AI-telemetry integration
// test has been removed along with the AI subsystem; this file verifies
// MarketSnapshot <-> SQLite persistence, which is the non-AI path.

use sqlx::SqlitePool;

#[tokio::test]
async fn test_normalized_snapshot_persistence_roundtrip() {
    use core_domain::models::MarketSnapshot;
    use market_analyzer::indicators::normalized::{DivergenceState, NormalizationEngine};
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    // Real schema (including the Phase 3 normalized columns) via migrations.
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations should apply cleanly");

    // Build a snapshot with a populated normalized indicator map.
    let mut indicators = HashMap::new();
    indicators.insert(
        "rsi".to_string(),
        NormalizationEngine::normalize_rsi(25.0, DivergenceState::None),
    );
    indicators.insert(
        "macd".to_string(),
        NormalizationEngine::normalize_macd(-12.4, -17.6, 5.2, 8.0, Some(1)),
    );
    indicators.insert(
        "rvol".to_string(),
        NormalizationEngine::normalize_rvol(2.0, 1.5, 3.0),
    );
    indicators.insert(
        "adx".to_string(),
        NormalizationEngine::normalize_adx(30.0, 30.0, 10.0, 1.0, false),
    );

    let snap = MarketSnapshot {
        timeframe_label: Some("1s".to_string()),
        exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
        timeframe_secs: 60,
        timestamp: 1_718_000_000,
        symbol: "BTC".to_string(),
        is_completed: Some(true),
        mid_price: dec!(50000.00),
        bid_price: dec!(49999.5),
        ask_price: dec!(50000.5),
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open: Some(dec!(49800.0)),
        high: Some(dec!(50200.0)),
        low: Some(dec!(49750.0)),
        close: Some(dec!(50000.0)),
        volume: Some(dec!(150.0)),
        average_volume: Some(dec!(120.0)),
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
    };

    database_storage::insert_snapshot_internal(&pool, &snap).await;

    // Dedicated normalized columns received numeric data.
    let (rsi_norm, rsi_label): (Option<f64>, Option<String>) =
        sqlx::query_as("SELECT rsi_normalized, rsi_state_label FROM market_snapshots LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(rsi_norm.unwrap() >= 0.70, "oversold rsi should map high");
    assert_eq!(rsi_label.as_deref(), Some("OVERSOLD_ACCUMULATION"));

    let aux: Option<String> =
        sqlx::query_scalar("SELECT auxiliary_normalized_data FROM market_snapshots LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(aux.is_some(), "auxiliary JSON blob should be persisted");

    // Full map round-trips through query_latest_snapshot (via auxiliary JSON).
    let loaded = database_storage::query_latest_snapshot(&pool, "BTC", 60)
        .await
        .expect("snapshot should be retrievable");
    let loaded_rsi = loaded.indicators.get("rsi").expect("rsi present");
    assert_eq!(loaded_rsi.state_label, "OVERSOLD_ACCUMULATION");
    assert!(loaded_rsi.normalized >= 0.70);
    let loaded_macd = loaded.indicators.get("macd").expect("macd present");
    assert_eq!(loaded_macd.state_label, "BULLISH_CROSSOVER_ACCELERATING");
    assert!(
        loaded_macd.values.is_some(),
        "macd multi-line values preserved"
    );
    // RVOL is a non-directional gate per the v2.1 contract — `normalized` is
    // always `0.0` and the band value lives in `values.rvol_band`.
    let loaded_rvol = loaded.indicators.get("rvol").expect("rvol present");
    assert_eq!(
        loaded_rvol.normalized, 0.0,
        "rvol normalized is always 0.0 (gate)"
    );
    assert_eq!(
        loaded_rvol.values.as_ref().and_then(|v| v.get("rvol_band")),
        Some(&0.8),
        "rvol band value is preserved in values map"
    );
}
