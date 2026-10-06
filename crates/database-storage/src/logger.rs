use core_domain::models::MarketSnapshot;
use core_domain::normalized::Exchange;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum TelemetryMsg {
    /// One completed snapshot, shared by every sink.
    ///
    /// v11.12.25: this was `Box<MarketSnapshot>` and the fan-out task cloned it
    /// per sink, so the DB and DS queues each held their own ~110 KB frame.
    /// `Arc` makes the fan-out a refcount bump. The contents are unchanged.
    InsertSnapshot(Arc<MarketSnapshot>),
    InsertIndividualLog {
        master_record_id: i64,
        indicator_name: String,
        signal: String,
        reason: String,
        timeframe_secs: u64,
    },
    UpdateMasterRecord {
        master_id: i64,
        general_trend: String,
        support_levels: String,
        resistance_levels: String,
        indicator_synthesis_summary: String,
        indicator_synthesis_evaluation: String,
        recommended_action: String,
        recommendation_rationale: String,
        score_points: Option<i32>,
        signals_json: Option<String>,
    },
    ConsoleLog(String),
    JournalTrade {
        symbol: String,
        direction: String,
        entry_price: f64,
        exit_price: f64,
        entry_timestamp: i64,
        exit_timestamp: i64,
        size: f64,
        realized_pnl: f64,
        roi_pct: f64,
        allocated_usd: f64,
        trigger: String,
    },
    /// Real liquidation event captured from exchange WS (Phase 1+).
    InsertLiquidationEvent {
        exchange: Exchange,
        symbol: String,
        side: String,
        price: f64,
        size_usd: f64,
        timestamp_ms: u64,
        venue_order_id: Option<String>,
    },
}

/// Delete aged rows: `market_snapshots` older than 7 days (timestamp in
/// seconds), `liquidation_events` older than `liq_retention_days`
/// (timestamp in milliseconds; 90-day default per 02-12-liquidity-matrix.md),
/// `liquidation_real_buckets` older than 24h (Block D — the bucketed
/// aggregation's persistence is display-only so a 24h rolling window
/// matches the in-memory cap), and `candle_archive` older than the BTE
/// archive depth (1..=365 days).
async fn run_retention_cleanup(
    pool: &SqlitePool,
    liq_retention_days: u32,
    archive_depth_days: u32,
) {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let snapshot_cutoff = now_secs.saturating_sub(7 * 86400) as i64;
    if let Err(e) = sqlx::query("DELETE FROM market_snapshots WHERE timestamp < ?1")
        .bind(snapshot_cutoff)
        .execute(pool)
        .await
    {
        eprintln!("DB cleanup error (market_snapshots): {}", e);
    }

    // BTE archive retention (prune_candle_archive is idempotent).
    crate::queries::archive::prune_candle_archive(pool, archive_depth_days).await;

    let liq_cutoff_ms = now_secs
        .saturating_sub(liq_retention_days as u64 * 86400)
        .saturating_mul(1000) as i64;
    if let Err(e) = sqlx::query("DELETE FROM liquidation_events WHERE timestamp < ?1")
        .bind(liq_cutoff_ms)
        .execute(pool)
        .await
    {
        eprintln!("DB cleanup error (liquidation_events): {}", e);
    }

    // Block D: 24h rolling retention for the price-bucketed aggregation.
    // last_updated_ms is also in milliseconds (matches `liquidation_events`).
    let bucket_cutoff_ms = now_secs.saturating_sub(86_400).saturating_mul(1000) as i64;
    if let Err(e) = sqlx::query("DELETE FROM liquidation_real_buckets WHERE last_updated_ms < ?1")
        .bind(bucket_cutoff_ms)
        .execute(pool)
        .await
    {
        eprintln!("DB cleanup error (liquidation_real_buckets): {}", e);
    }

    // M5 (production audit): connection-quality samples (written every
    // 60 s × 3 windows × per-(pair, TF) scope) were never pruned — the
    // table grew unbounded. 30-day rolling window.
    let cq_cutoff_ms = now_secs.saturating_sub(30 * 86400).saturating_mul(1000) as i64;
    if let Err(e) = sqlx::query("DELETE FROM connection_quality_samples WHERE timestamp_ms < ?1")
        .bind(cq_cutoff_ms)
        .execute(pool)
        .await
    {
        eprintln!("DB cleanup error (connection_quality_samples): {}", e);
    }
}

pub async fn run_telemetry_logger(
    pool: SqlitePool,
    mut rx: tokio::sync::mpsc::Receiver<TelemetryMsg>,
    liquidation_retention_days: u32,
    archive_depth_days: u32,
    session_id: Option<i64>,
) {
    println!("Telemetry & Logging Worker: Background log thread running.");

    // Initial cleanup on startup — snapshots older than 7 days, the BTE
    // candle archive past its depth, and liquidation events past their
    // retention window.
    run_retention_cleanup(&pool, liquidation_retention_days, archive_depth_days).await;

    let mut last_cleanup = tokio::time::Instant::now();

    while let Some(msg) = rx.recv().await {
        // Periodic cleanup every hour.
        if last_cleanup.elapsed() >= tokio::time::Duration::from_secs(3600) {
            run_retention_cleanup(&pool, liquidation_retention_days, archive_depth_days).await;
            last_cleanup = tokio::time::Instant::now();
        }
        match msg {
            TelemetryMsg::InsertSnapshot(snapshot) => {
                // v10: stamp the session id on every persisted snapshot.
                crate::queries::snapshots::insert_snapshot_with_session(
                    &pool, &snapshot, session_id,
                )
                .await;
            }
            TelemetryMsg::InsertIndividualLog { .. } => {
                // No-op: master records migrated out
            }
            TelemetryMsg::UpdateMasterRecord { .. } => {
                // No-op: master records migrated out
            }
            TelemetryMsg::ConsoleLog(log_text) => {
                println!("{}", log_text);
            }
            TelemetryMsg::JournalTrade {
                symbol,
                direction,
                entry_price,
                exit_price,
                entry_timestamp,
                exit_timestamp,
                size,
                realized_pnl,
                roi_pct,
                allocated_usd,
                trigger,
            } => {
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    run_journaling_task(
                        &pool_clone,
                        &symbol,
                        &direction,
                        entry_price,
                        exit_price,
                        entry_timestamp,
                        exit_timestamp,
                        size,
                        realized_pnl,
                        roi_pct,
                        allocated_usd,
                        &trigger,
                    )
                    .await;
                });
            }
            TelemetryMsg::InsertLiquidationEvent {
                exchange,
                symbol,
                side,
                price,
                size_usd,
                timestamp_ms,
                venue_order_id,
            } => {
                let exchange_str = match exchange {
                    Exchange::Hyperliquid => "Hyperliquid",
                    Exchange::Bitget => "Bitget",
                };
                if let Err(e) = sqlx::query(
                    "INSERT INTO liquidation_events
                        (exchange, symbol, side, price, size_usd, timestamp, venue_order_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .bind(exchange_str)
                .bind(&symbol)
                .bind(&side)
                .bind(price)
                .bind(size_usd)
                .bind(timestamp_ms as i64)
                .bind(&venue_order_id)
                .execute(&pool)
                .await
                {
                    eprintln!("DB error inserting liquidation event: {}", e);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_journaling_task(
    pool: &SqlitePool,
    symbol: &str,
    direction: &str,
    _entry_price: f64,
    _exit_price: f64,
    _entry_timestamp: i64,
    _exit_timestamp: i64,
    _size: f64,
    realized_pnl: f64,
    roi_pct: f64,
    allocated_usd: f64,
    trigger: &str,
) {
    let roe_pct = if allocated_usd > 0.0 {
        (realized_pnl / allocated_usd) * 100.0
    } else {
        roi_pct
    };

    let entry_date = format_ts(_entry_timestamp);
    let exit_date = format_ts(_exit_timestamp);
    let asset = symbol.to_string();

    let entry_reason = String::new();

    let notes = format!("[Trading Platform trade journal. Trigger: {}]", trigger);

    crate::queries::journals::insert_trade_journal(
        pool,
        0,
        &entry_date,
        &exit_date,
        &asset,
        direction,
        &entry_reason,
        roe_pct,
        &notes,
        5.0,
    )
    .await;

    println!("Trade Journal: {} {} recorded", symbol, direction);
}

fn format_ts(ms: i64) -> String {
    use chrono::TimeZone;
    let secs = ms / 1000;
    match chrono::Utc.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => secs.to_string(),
    }
}
