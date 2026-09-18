//! BTE data-science persistence — normalized writers/readers for the
//! `backtest_*` tables (see migration `20260820000001_backtest_ds.sql`).
//!
//! The DS rows mirror the `BacktestResult` the synchronous endpoint
//! returns, split so operators can query trades/equity/signals directly
//! with SQL instead of unpacking JSON blobs.

use sqlx::SqlitePool;

/// One DS signal row (per-tick synthesized decision snapshot).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsSignal {
    pub ts_secs: i64,
    pub timeframe_secs: u64,
    pub label: String,
    pub kind: String,
    pub value: String,
}

/// One DS portfolio sample (per-tick capital/exposure/drawdown).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsPortfolioPoint {
    pub ts_secs: i64,
    pub equity: f64,
    pub cash: f64,
    pub margin_used: f64,
    pub exposure_pct: f64,
    pub drawdown_pct: f64,
    pub positions_open: u32,
}

/// One DS trade row (mirrors the wire `BacktestTrade`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsTrade {
    pub ts_close_secs: i64,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub exit_reason: String,
    /// v10 enrichment.
    #[serde(default)]
    pub ts_entry_secs: i64,
    #[serde(default)]
    pub hold_secs: i64,
    #[serde(default)]
    pub mfe_pct: f64,
    #[serde(default)]
    pub mae_pct: f64,
    #[serde(default)]
    pub roi_pct: f64,
    /// v10.1 cost attribution (entry + exit slippage bps, commission,
    /// funding accrued on the position).
    #[serde(default)]
    pub slippage_bps: f64,
    #[serde(default)]
    pub commission_fees: f64,
    #[serde(default)]
    pub funding_fees: f64,
    /// v10.2 institutional: R-multiple (pnl / 1% risk) and symbol attribution
    #[serde(default)]
    pub r_multiple: f64,
    #[serde(default)]
    pub symbol: String,
}

/// One DS metric (key/value — summary + NHST).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsMetric {
    pub key: String,
    pub value: String,
}

/// Persist the full DS row set for a run in one transaction.
pub async fn insert_backtest_ds_rows(
    pool: &SqlitePool,
    run_id: i64,
    trades: &[DsTrade],
    equity: &[(i64, f64)],
    portfolio: &[DsPortfolioPoint],
    signals: &[DsSignal],
    metrics: &[DsMetric],
) {
    if run_id <= 0 {
        return;
    }
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("BTE DS: tx begin failed: {e}");
            return;
        }
    };

    for (seq, t) in trades.iter().enumerate() {
        if let Err(e) = sqlx::query(
            "INSERT OR IGNORE INTO backtest_trades
                (run_id, seq, ts_close_secs, direction, entry_price, exit_price, size, pnl, exit_reason,
                 ts_entry_secs, hold_secs, mfe_pct, mae_pct, roi_pct, slippage_bps, commission_fees, funding_fees)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )
        .bind(run_id)
        .bind(seq as i64)
        .bind(t.ts_close_secs)
        .bind(&t.direction)
        .bind(t.entry_price)
        .bind(t.exit_price)
        .bind(t.size)
        .bind(t.pnl)
        .bind(&t.exit_reason)
        .bind(t.ts_entry_secs)
        .bind(t.hold_secs)
        .bind(t.mfe_pct)
        .bind(t.mae_pct)
        .bind(t.roi_pct)
        .bind(t.slippage_bps)
        .bind(t.commission_fees)
        .bind(t.funding_fees)
        .execute(&mut *tx)
        .await {
                eprintln!("DB persist failed: {e}");
            }
    }

    for (ts, eq) in equity {
        if let Err(e) = sqlx::query(
            "INSERT OR IGNORE INTO backtest_equity (run_id, ts_secs, equity) VALUES (?1, ?2, ?3)",
        )
        .bind(run_id)
        .bind(ts)
        .bind(eq)
        .execute(&mut *tx)
        .await
        {
            eprintln!("DB persist failed: {e}");
        }
    }

    for p in portfolio {
        if let Err(e) = sqlx::query(
            "INSERT OR IGNORE INTO backtest_portfolio
                (run_id, ts_secs, equity, cash, margin_used, exposure_pct, drawdown_pct, positions_open)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(run_id)
        .bind(p.ts_secs)
        .bind(p.equity)
        .bind(p.cash)
        .bind(p.margin_used)
        .bind(p.exposure_pct)
        .bind(p.drawdown_pct)
        .bind(p.positions_open as i64)
        .execute(&mut *tx)
        .await {
                eprintln!("DB persist failed: {e}");
            }
    }

    for s in signals {
        if let Err(e) = sqlx::query(
            "INSERT OR IGNORE INTO backtest_signals
                (run_id, ts_secs, timeframe_secs, label, kind, value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(run_id)
        .bind(s.ts_secs)
        .bind(s.timeframe_secs as i64)
        .bind(&s.label)
        .bind(&s.kind)
        .bind(&s.value)
        .execute(&mut *tx)
        .await
        {
            eprintln!("DB persist failed: {e}");
        }
    }

    for m in metrics {
        if let Err(e) = sqlx::query(
            "INSERT OR IGNORE INTO backtest_metrics (run_id, metric_key, value) VALUES (?1, ?2, ?3)",
        )
        .bind(run_id)
        .bind(&m.key)
        .bind(&m.value)
        .execute(&mut *tx)
        .await {
                eprintln!("DB persist failed: {e}");
            }
    }

    if let Err(e) = tx.commit().await {
        eprintln!("BTE DS: tx commit failed: {e}");
    }
}

/// Stamp the run metadata (instance binding + mode) on the legacy run row.
pub async fn update_backtest_run_meta(
    pool: &SqlitePool,
    run_id: i64,
    instance_id: Option<&str>,
    mode: &str,
) {
    if let Err(e) =
        sqlx::query("UPDATE backtest_runs SET instance_id = ?2, mode = ?3 WHERE id = ?1")
            .bind(run_id)
            .bind(instance_id)
            .bind(mode)
            .execute(pool)
            .await
    {
        eprintln!("DB persist failed: {e}");
    }
}

/// Fetch the DS trades for a run (paginated).
pub async fn query_backtest_trades(
    pool: &SqlitePool,
    run_id: i64,
    limit: u32,
    offset: u32,
) -> Vec<DsTrade> {
    sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            f64,
            f64,
            f64,
            f64,
            String,
            i64,
            i64,
            f64,
            f64,
            f64,
            f64,
            f64,
            f64,
        ),
    >(
        "SELECT ts_close_secs, direction, COALESCE(symbol, ''), entry_price, exit_price, size, pnl, exit_reason,
                COALESCE(ts_entry_secs, 0), COALESCE(hold_secs, 0),
                COALESCE(mfe_pct, 0), COALESCE(mae_pct, 0), COALESCE(roi_pct, 0),
                COALESCE(slippage_bps, 0), COALESCE(commission_fees, 0), COALESCE(funding_fees, 0)
         FROM backtest_trades WHERE run_id = ?1 ORDER BY seq ASC LIMIT ?2 OFFSET ?3",
    )
    .bind(run_id)
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(
        |(
            ts,
            direction,
            symbol,
            entry,
            exit,
            size,
            pnl,
            reason,
            ts_entry,
            hold,
            mfe,
            mae,
            roi,
            slippage,
            commission,
            funding,
        )| DsTrade {
            ts_close_secs: ts,
            direction,
            symbol,
            entry_price: entry,
            exit_price: exit,
            size,
            pnl,
            exit_reason: reason,
            ts_entry_secs: ts_entry,
            hold_secs: hold,
            mfe_pct: mfe,
            mae_pct: mae,
            roi_pct: roi,
            slippage_bps: slippage,
            commission_fees: commission,
            funding_fees: funding,
            r_multiple: if roi.is_finite() { roi / 1.0 } else { 0.0 },
        },
    )
    .collect()
}

/// Fetch the DS equity curve for a run.
pub async fn query_backtest_equity(pool: &SqlitePool, run_id: i64) -> Vec<(i64, f64)> {
    sqlx::query_as::<_, (i64, f64)>(
        "SELECT ts_secs, equity FROM backtest_equity WHERE run_id = ?1 ORDER BY ts_secs ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Fetch the DS portfolio samples for a run.
pub async fn query_backtest_portfolio(pool: &SqlitePool, run_id: i64) -> Vec<DsPortfolioPoint> {
    sqlx::query_as::<_, (i64, f64, f64, f64, f64, f64, i64)>(
        "SELECT ts_secs, equity, cash, margin_used, exposure_pct, drawdown_pct, positions_open
         FROM backtest_portfolio WHERE run_id = ?1 ORDER BY ts_secs ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(
        |(ts_secs, equity, cash, margin_used, exposure_pct, drawdown_pct, positions_open)| {
            DsPortfolioPoint {
                ts_secs,
                equity,
                cash,
                margin_used,
                exposure_pct,
                drawdown_pct,
                positions_open: positions_open.max(0) as u32,
            }
        },
    )
    .collect()
}

/// Fetch the DS signals for a run.
pub async fn query_backtest_signals(pool: &SqlitePool, run_id: i64) -> Vec<DsSignal> {
    sqlx::query_as::<_, (i64, i64, String, String, String)>(
        "SELECT ts_secs, timeframe_secs, label, kind, value
         FROM backtest_signals WHERE run_id = ?1 ORDER BY ts_secs ASC, label ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(ts_secs, timeframe_secs, label, kind, value)| DsSignal {
        ts_secs,
        timeframe_secs: timeframe_secs.max(0) as u64,
        label,
        kind,
        value,
    })
    .collect()
}

/// Fetch the DS metrics for a run.
pub async fn query_backtest_metrics(pool: &SqlitePool, run_id: i64) -> Vec<DsMetric> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT metric_key, value FROM backtest_metrics WHERE run_id = ?1 ORDER BY metric_key ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(key, value)| DsMetric { key, value })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("mem pool");
        crate::run_migrations(&pool).await.expect("migrations");
        pool
    }

    async fn run_row(pool: &SqlitePool) -> i64 {
        crate::queries::analytics::insert_backtest_run(pool, "{}", "{}", "{}", "{}", "{}").await
    }

    #[tokio::test]
    async fn ds_rows_round_trip() {
        let pool = seed_pool().await;
        let run_id = run_row(&pool).await;
        assert!(run_id > 0);

        insert_backtest_ds_rows(
            &pool,
            run_id,
            &[DsTrade {
                ts_close_secs: 1000,
                direction: "LONG".into(),
                entry_price: 100.0,
                exit_price: 110.0,
                size: 0.5,
                pnl: 5.0,
                exit_reason: "tp".into(),
                ts_entry_secs: 0,
                hold_secs: 0,
                mfe_pct: 0.0,
                mae_pct: 0.0,
                roi_pct: 0.0,
                slippage_bps: 5.0,
                commission_fees: 0.01,
                funding_fees: -0.02,
                r_multiple: 0.0,
                symbol: "BTC-USDT".into(),
            }],
            &[(1000, 1000.0), (1001, 1005.0)],
            &[DsPortfolioPoint {
                ts_secs: 1001,
                equity: 1005.0,
                cash: 995.0,
                margin_used: 10.0,
                exposure_pct: 1.0,
                drawdown_pct: 0.0,
                positions_open: 0,
            }],
            &[DsSignal {
                ts_secs: 1000,
                timeframe_secs: 60,
                label: "decision".into(),
                kind: "bias".into(),
                value: "Bullish".into(),
            }],
            &[DsMetric {
                key: "total_trades".into(),
                value: "1".into(),
            }],
        )
        .await;

        let trades = query_backtest_trades(&pool, run_id, 10, 0).await;
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].exit_reason, "tp");
        assert_eq!(trades[0].slippage_bps, 5.0);
        assert_eq!(trades[0].commission_fees, 0.01);
        assert_eq!(trades[0].funding_fees, -0.02);

        let equity = query_backtest_equity(&pool, run_id).await;
        assert_eq!(equity.len(), 2);

        let portfolio = query_backtest_portfolio(&pool, run_id).await;
        assert_eq!(portfolio.len(), 1);
        assert_eq!(portfolio[0].positions_open, 0);

        let signals = query_backtest_signals(&pool, run_id).await;
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].value, "Bullish");

        let metrics = query_backtest_metrics(&pool, run_id).await;
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].value, "1");
    }
}
