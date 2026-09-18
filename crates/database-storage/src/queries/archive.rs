//! BTE candle-archive queries — the deep-history backtest data source.
//!
//! `candle_archive` stores lightweight OHLCV rows (Unix seconds, consistent
//! with `market_snapshots.timestamp`) written by the live pipeline and the
//! on-demand backfill job. Retention follows `[workspace.backtest]
//! archive_depth_days` (1..=365, pruned hourly by `run_retention_cleanup`).

use core_domain::normalized::NormalizedCandle;
use rust_decimal::prelude::ToPrimitive;
use sqlx::SqlitePool;

/// One archived candle (OHLCV) — the historical-replay input.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArchivedCandle {
    pub exchange: String,
    pub symbol: String,
    pub timeframe_secs: i64,
    pub ts_secs: i64,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
    pub trades_count: Option<i64>,
    pub source: String,
}

impl ArchivedCandle {
    /// Conversion to the normalized wire candle (reconstruction flag kept
    /// as None — the archive is the honest historical record; consumers
    /// read `source` for provenance).
    pub fn to_normalized(&self) -> NormalizedCandle {
        let d = |v: Option<f64>| {
            v.and_then(rust_decimal::Decimal::from_f64_retain)
                .unwrap_or_default()
        };
        let exchange = match self.exchange.as_str() {
            "Bitget" => core_domain::normalized::Exchange::Bitget,
            _ => core_domain::normalized::Exchange::Hyperliquid,
        };
        NormalizedCandle {
            exchange,
            symbol: self.symbol.clone(),
            start_time_ms: (self.ts_secs * 1000) as u64,
            duration_ms: (self.timeframe_secs * 1000) as u64,
            open: d(self.open),
            high: d(self.high),
            low: d(self.low),
            close: d(self.close),
            volume: d(self.volume),
            trades_count: self.trades_count.unwrap_or(0) as u64,
            reconstructed: None,
        }
    }
}

/// Batch-upsert normalized candles (dedup via the UNIQUE constraint).
/// `source` is `live` / `reconstructed` (live path) or `backfill`.
pub async fn upsert_archive_candles(
    pool: &SqlitePool,
    candles: &[NormalizedCandle],
    source: &str,
) -> u64 {
    if candles.is_empty() {
        return 0;
    }
    let mut stored = 0u64;
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("Archive Error: failed to begin tx: {}", e);
            return 0;
        }
    };
    for c in candles {
        let exchange = c.exchange.to_string();
        let ts_secs = (c.start_time_ms / 1000) as i64;
        // v8.2: exchange candle durations carry off-by-one milliseconds
        // (e.g. Hyperliquid 900s candles are 899_999ms) — round to the
        // nearest second so rows land on the canonical timeframe key.
        let tf_secs = ((c.duration_ms + 500) / 1000).max(1) as i64;
        let res = sqlx::query(
            "INSERT INTO candle_archive
                (exchange, symbol, timeframe_secs, ts_secs, open, high, low, close,
                 volume, trades_count, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (exchange, symbol, timeframe_secs, ts_secs) DO NOTHING",
        )
        .bind(exchange)
        .bind(&c.symbol)
        .bind(tf_secs)
        .bind(ts_secs)
        .bind(c.open.to_f64().map(|v| v.to_string()))
        .bind(c.high.to_f64().map(|v| v.to_string()))
        .bind(c.low.to_f64().map(|v| v.to_string()))
        .bind(c.close.to_f64().map(|v| v.to_string()))
        .bind(c.volume.to_f64().map(|v| v.to_string()))
        .bind(c.trades_count as i64)
        .bind(source)
        .execute(&mut *tx)
        .await;
        match res {
            Ok(r) => stored += r.rows_affected(),
            Err(e) => eprintln!("Archive Error: upsert failed: {}", e),
        }
    }
    if let Err(e) = tx.commit().await {
        eprintln!("Archive Error: commit failed: {}", e);
    }
    stored
}

/// Fetch archived candles for (symbol, timeframe) within [from_secs, to_secs]
/// ascending — the historical runner's input window.
pub async fn query_archive_window(
    pool: &SqlitePool,
    symbol: &str,
    timeframe_secs: u64,
    from_secs: i64,
    to_secs: i64,
    limit: u32,
) -> Vec<ArchivedCandle> {
    sqlx::query_as::<_, ArchivedCandle>(
        "SELECT exchange, symbol, timeframe_secs, ts_secs,
                CAST(open AS REAL) as open, CAST(high AS REAL) as high,
                CAST(low AS REAL) as low, CAST(close AS REAL) as close,
                CAST(volume AS REAL) as volume, trades_count, source
         FROM candle_archive
         WHERE symbol = ?1 AND timeframe_secs = ?2
           AND ts_secs >= ?3 AND ts_secs <= ?4
         ORDER BY ts_secs ASC
         LIMIT ?5",
    )
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .bind(from_secs)
    .bind(to_secs)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Per (symbol, timeframe) archive coverage: count + earliest/latest.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArchiveCoverageRow {
    pub symbol: String,
    pub timeframe_secs: i64,
    pub candle_count: i64,
    pub earliest_secs: Option<i64>,
    pub latest_secs: Option<i64>,
}

/// Aggregate archive coverage grouped by symbol × timeframe.
pub async fn query_archive_coverage(pool: &SqlitePool) -> Vec<ArchiveCoverageRow> {
    sqlx::query_as::<_, ArchiveCoverageRow>(
        "SELECT symbol, timeframe_secs,
                COUNT(*) as candle_count,
                MIN(ts_secs) as earliest_secs,
                MAX(ts_secs) as latest_secs
         FROM candle_archive
         GROUP BY symbol, timeframe_secs
         ORDER BY symbol, timeframe_secs",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Earliest archived candle ts for (exchange, symbol, timeframe) — the
/// resumable backfill cursor anchor.
pub async fn query_archive_earliest_secs(
    pool: &SqlitePool,
    exchange: &str,
    symbol: &str,
    timeframe_secs: u64,
) -> Option<i64> {
    // v9 fix: `MIN(ts_secs)` over an empty archive decodes as Some(0)
    // under sqlx's SQLite driver (NULL → 0 coercion) — an empty archive
    // must report None, otherwise the CLI's coverage check believes the
    // archive is covered since epoch 0 and never backfills. Query the
    // nullable tuple instead.
    let row: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT MIN(ts_secs) FROM candle_archive
         WHERE exchange = ?1 AND symbol = ?2 AND timeframe_secs = ?3",
    )
    .bind(exchange)
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .fetch_one(pool)
    .await
    .ok();
    row.and_then(|(v,)| v)
}

/// Delete archive rows older than `now - depth_days` (hourly retention).
pub async fn prune_candle_archive(pool: &SqlitePool, depth_days: u32) {
    let now_secs = chrono::Utc::now().timestamp();
    let cutoff = now_secs.saturating_sub(depth_days as i64 * 86400);
    if let Err(e) = sqlx::query("DELETE FROM candle_archive WHERE ts_secs < ?1")
        .bind(cutoff)
        .execute(pool)
        .await
    {
        eprintln!("Archive Error: prune failed: {}", e);
    }
}

/// One `backfill_jobs` row — the persisted shadow of the in-memory
/// progress registry (resumable after daemon restart).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BackfillJobRow {
    pub id: i64,
    pub instance_id: String,
    pub symbol: String,
    pub exchange: String,
    pub depth_days: i64,
    pub status: String,
    pub pages_fetched: i64,
    pub candles_stored: i64,
    pub earliest_ts_secs: Option<i64>,
    pub latest_ts_secs: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create a backfill job row; returns its id.
pub async fn insert_backfill_job(
    pool: &SqlitePool,
    instance_id: &str,
    symbol: &str,
    exchange: &str,
    depth_days: u32,
) -> Option<i64> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO backfill_jobs
            (instance_id, symbol, exchange, depth_days, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?5)
         RETURNING id",
    )
    .bind(instance_id)
    .bind(symbol)
    .bind(exchange)
    .bind(depth_days as i64)
    .bind(now)
    .fetch_one(pool)
    .await
    .ok()
}

/// Update a backfill job's progress/status row.
#[allow(clippy::too_many_arguments)]
pub async fn update_backfill_job(
    pool: &SqlitePool,
    job_id: i64,
    status: &str,
    pages_fetched: u64,
    candles_stored: u64,
    earliest_ts_secs: Option<i64>,
    latest_ts_secs: Option<i64>,
    error: Option<&str>,
) {
    let now = chrono::Utc::now().timestamp();
    if let Err(e) = sqlx::query(
        "UPDATE backfill_jobs
         SET status = ?2, pages_fetched = ?3, candles_stored = ?4,
             earliest_ts_secs = ?5, latest_ts_secs = ?6, error = ?7,
             updated_at = ?8
         WHERE id = ?1",
    )
    .bind(job_id)
    .bind(status)
    .bind(pages_fetched as i64)
    .bind(candles_stored as i64)
    .bind(earliest_ts_secs)
    .bind(latest_ts_secs)
    .bind(error)
    .bind(now)
    .execute(pool)
    .await
    {
        eprintln!("DB persist failed: {e}");
    }
}

/// Recent backfill jobs, newest first.
pub async fn query_backfill_jobs(pool: &SqlitePool, limit: u32) -> Vec<BackfillJobRow> {
    sqlx::query_as::<_, BackfillJobRow>(
        "SELECT id, instance_id, symbol, exchange, depth_days, status,
                pages_fetched, candles_stored, earliest_ts_secs, latest_ts_secs,
                error, created_at, updated_at
         FROM backfill_jobs
         ORDER BY id DESC
         LIMIT ?1",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Active (running) job for an instance — the duplicate-job guard.
pub async fn query_active_backfill_job(
    pool: &SqlitePool,
    instance_id: &str,
) -> Option<BackfillJobRow> {
    sqlx::query_as::<_, BackfillJobRow>(
        "SELECT id, instance_id, symbol, exchange, depth_days, status,
                pages_fetched, candles_stored, earliest_ts_secs, latest_ts_secs,
                error, created_at, updated_at
         FROM backfill_jobs
         WHERE instance_id = ?1 AND status = 'running'
         ORDER BY id DESC
         LIMIT 1",
    )
    .bind(instance_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::normalized::{Exchange, ReconstructionMethod};
    use rust_decimal_macros::dec;

    async fn seed_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("mem pool");
        crate::run_migrations(&pool).await.expect("migrations");
        pool
    }

    fn candle(symbol: &str, ts_secs: u64, close: f64) -> NormalizedCandle {
        NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: symbol.to_string(),
            start_time_ms: ts_secs * 1000,
            duration_ms: 60_000,
            open: dec!(100),
            high: dec!(110),
            low: dec!(90),
            close: rust_decimal::Decimal::from_f64_retain(close).unwrap_or_default(),
            volume: dec!(50),
            trades_count: 3,
            reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
        }
    }

    #[tokio::test]
    async fn upsert_dedup_and_window_query() {
        let pool = seed_pool().await;
        let batch = vec![
            candle("BTC-USDC", 1000, 105.0),
            candle("BTC-USDC", 1001, 106.0),
        ];
        assert_eq!(upsert_archive_candles(&pool, &batch, "backfill").await, 2);
        // Duplicate upsert stores nothing new.
        assert_eq!(upsert_archive_candles(&pool, &batch, "backfill").await, 0);

        let rows = query_archive_window(&pool, "BTC-USDC", 60, 0, 10_000, 10).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].ts_secs, 1000);
        assert_eq!(rows[1].source, "backfill");
        assert_eq!(rows[0].close, Some(105.0));
        assert_eq!(rows[0].to_normalized().start_time_ms, 1_000_000);
    }

    #[tokio::test]
    async fn coverage_and_earliest_anchor() {
        let pool = seed_pool().await;
        let batch = vec![
            candle("BTC-USDC", 1000, 105.0),
            candle("BTC-USDC", 1002, 106.0),
        ];
        upsert_archive_candles(&pool, &batch, "live").await;

        let cov = query_archive_coverage(&pool).await;
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].candle_count, 2);
        assert_eq!(cov[0].earliest_secs, Some(1000));
        assert_eq!(cov[0].latest_secs, Some(1002));

        let anchor = query_archive_earliest_secs(&pool, "Hyperliquid", "BTC-USDC", 60).await;
        assert_eq!(anchor, Some(1000));
    }

    #[tokio::test]
    async fn prune_respects_depth_days() {
        let pool = seed_pool().await;
        let now = chrono::Utc::now().timestamp();
        let old = candle("BTC-USDC", (now - 10 * 86400) as u64, 100.0);
        let fresh = candle("BTC-USDC", (now - 3600) as u64, 101.0);
        upsert_archive_candles(&pool, &[old, fresh], "live").await;

        prune_candle_archive(&pool, 7).await;
        let cov = query_archive_coverage(&pool).await;
        assert_eq!(cov[0].candle_count, 1, "10-day-old row must be pruned");
        assert_eq!(cov[0].earliest_secs, Some(now - 3600));
    }
}
