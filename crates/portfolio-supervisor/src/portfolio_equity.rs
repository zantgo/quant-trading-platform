use rust_decimal::prelude::ToPrimitive;
use sqlx::{Row, SqlitePool};
use tokio_util::sync::CancellationToken;

const LOG_INTERVAL_SECS: u64 = 60;
const PURGE_OLDER_THAN_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub async fn insert_equity_snapshot(
    pool: &SqlitePool,
    timestamp_ms: i64,
    total_value: f64,
    cash_balance: f64,
    unrealized_pnl: f64,
) {
    // v10.2: stamp active session id when available (NULL for legacy paths).
    let session_id: Option<i64> = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM sessions WHERE status = 'active' ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|r| r.0);
    if let Err(e) = sqlx::query(
        "INSERT INTO portfolio_equity_history (timestamp, total_value, cash_balance, unrealized_pnl, session_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(timestamp_ms)
    .bind(total_value)
    .bind(cash_balance)
    .bind(unrealized_pnl)
    .bind(session_id)
    .execute(pool)
    .await
    {
        eprintln!("insert equity snapshot failed: {e}");
    }
}

pub async fn fetch_equity_history(
    pool: &SqlitePool,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Vec<(i64, f64)> {
    let default_start: i64 = 0;
    let default_end: i64 = i64::MAX;
    let rows = sqlx::query(
        "SELECT timestamp, total_value FROM portfolio_equity_history
         WHERE timestamp >= ?1 AND timestamp <= ?2
         ORDER BY timestamp ASC",
    )
    .bind(start_ms.unwrap_or(default_start))
    .bind(end_ms.unwrap_or(default_end))
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.iter()
        .map(|r| {
            let ts: i64 = r.get(0);
            let val: f64 = r.get(1);
            (ts, val)
        })
        .collect()
}

pub async fn purge_equity_history(pool: &SqlitePool, older_than_ms: i64) {
    if let Err(e) = sqlx::query("DELETE FROM portfolio_equity_history WHERE timestamp < ?1")
        .bind(older_than_ms)
        .execute(pool)
        .await
    {
        eprintln!("purge equity history failed: {e}");
    }
}

async fn write_snapshot(
    pool: &SqlitePool,
    engine: Option<&crate::execution::engine::ExecutionEngine>,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let (total_value, total_cash, unrealized) = if let Some(eng) = engine {
        let total = eng.get_equity().await;
        let unreal: f64 = {
            let positions = eng.positions.read().await;
            positions
                .values()
                .map(|p| p.unrealized_pnl.to_f64().unwrap_or(0.0))
                .sum()
        };
        let cash = total - unreal;
        (total, cash, unreal)
    } else {
        let total_cash: f64 =
            sqlx::query("SELECT COALESCE(SUM(current_cash), 0.0) FROM paper_balances")
                .fetch_one(pool)
                .await
                .map(|r| r.get(0))
                .unwrap_or(0.0);

        let unrealized: f64 = sqlx::query(
            "SELECT COALESCE(SUM(CAST(unrealized_pnl AS REAL)), 0.0) FROM active_positions",
        )
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0.0);

        let total_value = total_cash + unrealized;
        (total_value, total_cash, unrealized)
    };
    insert_equity_snapshot(pool, now_ms, total_value, total_cash, unrealized).await;
    purge_equity_history(pool, now_ms - PURGE_OLDER_THAN_MS).await;
}

pub async fn run_portfolio_equity_logger_with_engine(
    pool: SqlitePool,
    engine: Option<std::sync::Arc<crate::execution::engine::ExecutionEngine>>,
    cancel: CancellationToken,
) {
    println!(
        "📊 Portfolio Equity Logger: Started (interval: {}s)...",
        LOG_INTERVAL_SECS
    );

    write_snapshot(&pool, engine.as_deref()).await;

    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(LOG_INTERVAL_SECS));

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Portfolio Equity Logger: Cancelled, shutting down.");
                break;
            }
            _ = ticker.tick() => {}
        }

        write_snapshot(&pool, engine.as_deref()).await;
    }
}

pub async fn run_portfolio_equity_logger(pool: SqlitePool, cancel: CancellationToken) {
    run_portfolio_equity_logger_with_engine(pool, None, cancel).await
}
