use core_domain::models::MarketSnapshot;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Barrier;

async fn setup_concurrent_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite database");

    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS market_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            mid_price REAL NOT NULL,
            timeframe_secs INTEGER NOT NULL,
            is_completed INTEGER NOT NULL,
            bid_price REAL NOT NULL,
            ask_price REAL NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn test_concurrent_snapshot_writes_no_panic() {
    let pool = setup_concurrent_db().await;
    let pool = Arc::new(pool);
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    for thread_idx in 0..10 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        let handle = tokio::spawn(async move {
            barrier.wait().await;
            for i in 0..5 {
                let snap = MarketSnapshot {
                    timeframe_slot: Some(core_domain::models::TimeframeSlot::Micro1),
                    exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
                    timeframe_secs: 60,
                    timestamp: (thread_idx * 1000 + i) as u64,
                    symbol: format!("SYM-{}", thread_idx),
                    is_completed: Some(true),
                    mid_price: dec!(50000.00),
                    bid_price: dec!(49999.50),
                    ask_price: dec!(50000.50),
                    bid_size: None,
                    ask_size: None,
                    funding_rate: None,
                    open: Some(dec!(49800.00)),
                    high: Some(dec!(50200.00)),
                    low: None,
                    close: None,
                    volume: None,
                    average_volume: None,
                    indicators: std::collections::HashMap::new(),
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

                let result = sqlx::query(
                    "INSERT INTO market_snapshots (symbol, timestamp, mid_price, timeframe_secs, is_completed, bid_price, ask_price)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
                )
                .bind(&snap.symbol)
                .bind(snap.timestamp as i64)
                .bind(snap.mid_price.to_f64().unwrap_or(0.0))
                .bind(snap.timeframe_secs as i64)
                .bind(1i64)
                .bind(snap.bid_price.to_f64().unwrap_or(0.0))
                .bind(snap.ask_price.to_f64().unwrap_or(0.0))
                .execute(&*pool)
                .await;

                assert!(
                    result.is_ok(),
                    "Thread {} insert {} failed: {:?}",
                    thread_idx,
                    i,
                    result.err()
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Thread should not panic");
    }

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM market_snapshots")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(count.0, 50, "All 10 threads × 5 inserts = 50 rows expected");
}

#[tokio::test]
async fn test_read_during_write_completes() {
    let pool = setup_concurrent_db().await;

    // Pre-populate one row
    sqlx::query("INSERT INTO market_snapshots (symbol, timestamp, mid_price, timeframe_secs, is_completed, bid_price, ask_price)
                 VALUES ('PRE', 1, 100.0, 60, 1, 99.0, 101.0)")
        .execute(&pool)
        .await
        .unwrap();

    let pool = Arc::new(pool);
    let pool_read = pool.clone();

    // Writer holds a transaction briefly
    let write_handle = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO market_snapshots (symbol, timestamp, mid_price, timeframe_secs, is_completed, bid_price, ask_price)
                     VALUES ('WRITE', 2, 200.0, 60, 1, 199.0, 201.0)")
            .execute(&mut *tx)
            .await
            .unwrap();
        // Small delay while holding the write lock
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        tx.commit().await.unwrap();
    });

    // Give writer a head start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let read_handle = tokio::spawn(async move {
        let row: (String,) =
            sqlx::query_as("SELECT symbol FROM market_snapshots WHERE symbol = 'PRE'")
                .fetch_one(&*pool_read)
                .await
                .unwrap();
        assert_eq!(row.0, "PRE");
    });

    write_handle.await.unwrap();
    read_handle.await.unwrap();
}
