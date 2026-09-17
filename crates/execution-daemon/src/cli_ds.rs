//! # CLI data-science commands (v10)
//!
//! Headless JSON surfaces for the coding agent / Jupyter workflow:
//! `--sessions`, `--session-report <id>`, `--backtest-show <id>`.
//! Every payload is the SAME server-computed struct the GUI renders —
//! parity by construction (one producer, three sinks).

use config_models::WorkspaceConfig;
use sqlx::SqlitePool;

/// `--sessions` — list persisted sessions, newest first.
pub async fn print_sessions(pool: &SqlitePool) -> i32 {
    match database_storage::queries::sessions::list_sessions(pool).await {
        Ok(rows) => {
            let out: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "session_id": r.id,
                        "mode": r.mode,
                        "exchange": r.exchange,
                        "currency": r.currency,
                        "portfolio_capital_usd": r.portfolio_capital_usd,
                        "started_at_ms": r.started_at_ms,
                        "ended_at_ms": r.ended_at_ms,
                        "status": r.status,
                    })
                })
                .collect();
            println!("{}", serde_json::json!({ "sessions": out }));
            0
        }
        Err(e) => {
            eprintln!("sessions query failed: {e}");
            1
        }
    }
}

/// `--session-report <id>` — unified session result (parity with backtest).
/// Produces the same fields as `GET /api/backtest/:id`: params, summary,
/// NHST stats, risk, trades (enriched), equity curves, log returns, symmetry.
/// Session-scoped by construction (WHERE session_id).
pub async fn print_session_report(pool: &SqlitePool, session_id: i64) -> i32 {
    let workspace = config_models::load_workspace().unwrap_or_default();
    if let Some(res) =
        performance_analytics::session_result::compile_session_result(pool, session_id, &workspace)
            .await
    {
        // Counts for quick sanity
        let snapshots: i64 = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM market_snapshots WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| r.0)
        .unwrap_or(0);
        let telemetry_trades: i64 = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM trade_telemetry_history WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| r.0)
        .unwrap_or(0);

        let out = serde_json::json!({
            "session_id": res.session_id,
            "mode": res.mode,
            "params": res.params,
            "summary": res.summary,
            "stats": res.stats,
            "risk": res.risk,
            "trades": res.trades,
            "equity_curve": res.equity_curve,
            "equity_curve_secs": res.equity_curve_secs,
            "log_returns": res.log_returns,
            "counts": {
                "market_snapshots": snapshots,
                "telemetry_trades": telemetry_trades,
            },
            // Convenience headline — mirrors backtest_show top-level keys
            "roi_pct_avg": res.summary.avg_roi_pct,
            "profit_factor": res.summary.profit_factor,
            "win_rate": res.summary.win_rate,
            "edge": format!("{:?}", res.stats.classification),
            "expectancy": res.summary.expectancy,
            "avg_profit": res.summary.avg_profit,
            "avg_loss": res.summary.avg_loss,
            "avg_hold_secs": res.summary.avg_hold_secs,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
        );
        0
    } else {
        eprintln!("session {session_id} not found");
        1
    }
}

/// `--backtest-gates <id>` — gate diagnostics: why no trades?
/// Prints one table: READY%, stance histogram, confidence vs ready_min, stop-floor/tp-cap counts.
pub async fn print_backtest_gates(pool: &SqlitePool, id: i64) -> i32 {
    let signals = database_storage::queries::backtest_ds::query_backtest_signals(pool, id).await;
    if signals.is_empty() {
        eprintln!("no signals for backtest {id}");
        return 1;
    }
    let mut readiness: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    let mut stance: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    let mut primary: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    for s in &signals {
        match (s.label.as_str(), s.kind.as_str()) {
            ("decision", "trade_readiness") => *readiness.entry(s.value.clone()).or_insert(0) += 1,
            ("decision", "market_stance") => *stance.entry(s.value.clone()).or_insert(0) += 1,
            ("opportunity", "primary") => *primary.entry(s.value.clone()).or_insert(0) += 1,
            _ => {}
        }
    }
    // Count gate signals if present
    let gate_counts: Vec<_> = signals
        .iter()
        .filter(|s| s.label == "gate")
        .map(|s| format!("{}:{}", s.kind, s.value))
        .collect();
    let out = serde_json::json!({
        "backtest_id": id,
        "total_signals": signals.len(),
        "readiness": readiness,
        "market_stance": stance,
        "primary": primary,
        "gate_samples": gate_counts.iter().take(20).collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
    );
    0
}

/// `--backtest-show <id>` — the full run: params, summary, NHST stats,
/// metrics, trades (enriched), equity + the ds/ file paths.
pub async fn print_backtest_show(pool: &SqlitePool, workspace: &WorkspaceConfig, id: i64) -> i32 {
    let run = database_storage::query_backtest_run(pool, id).await;
    let Some((params, summary, stats, _trades_json, equity)) = run else {
        eprintln!("backtest run {id} not found");
        return 1;
    };
    let metrics = database_storage::queries::backtest_ds::query_backtest_metrics(pool, id).await;
    let trades =
        database_storage::queries::backtest_ds::query_backtest_trades(pool, id, 5000, 0).await;
    let ds_root = std::path::PathBuf::from(&workspace.data_science.output_path);
    let ds_dir = database_storage::ds_export::backtest_dir(&ds_root, id, "historical");
    let out = serde_json::json!({
        "backtest_id": id,
        "params": serde_json::from_str::<serde_json::Value>(&params).unwrap_or(serde_json::Value::Null),
        "summary": serde_json::from_str::<serde_json::Value>(&summary).unwrap_or(serde_json::Value::Null),
        "stats": serde_json::from_str::<serde_json::Value>(&stats).unwrap_or(serde_json::Value::Null),
        "metrics": metrics.iter().map(|m| serde_json::json!({ "key": m.key, "value": m.value })).collect::<Vec<_>>(),
        "trades": trades,
        "equity": equity,
        "ds_files": {
            "run_json": format!("{}/run.json", ds_dir.display()),
            "trades_ndjson": format!("{}/trades.ndjson", ds_dir.display()),
            "equity_ndjson": format!("{}/equity.ndjson", ds_dir.display()),
            "input_bars_dir": format!("{}/input_bars/", ds_dir.display()),
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
    );
    0
}
