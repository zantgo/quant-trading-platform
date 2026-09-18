//! # Live data-science exporter (v10)
//!
//! Consumes the same telemetry stream the DB logger consumes (fan-out in
//! `main.rs`) and mirrors it into the `./ds/sessions/Sxxxx_mode/` NDJSON
//! tree. D-tier artifacts come straight from the channel; I-tier analytics
//! snapshots are appended on the flush cadence from the SQLite tables the
//! PAE already writes (offset-based, restart-safe).

use config_models::DataScienceConfig;
use database_storage::ds_export::{session_dir, write_pretty, DsWriter};
use sqlx::Row as _;
use sqlx::SqlitePool;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct DsSessionMeta {
    pub session_id: i64,
    pub mode: String,
    pub exchange: String,
    pub currency: String,
    pub capital: f64,
    pub started_at_ms: i64,
    pub config_snapshot: serde_json::Value,
    /// v10.1: TAE-activation intent at launch (`--tae-on` / CLI prompt).
    pub tae_activated: bool,
}

/// Run the exporter until the channel closes or the task is aborted.
pub async fn run_ds_exporter(
    pool: SqlitePool,
    mut rx: mpsc::Receiver<database_storage::TelemetryMsg>,
    cfg: DataScienceConfig,
    meta: DsSessionMeta,
) {
    if !cfg.enabled {
        // Drain the channel silently so the forwarder never blocks.
        while rx.recv().await.is_some() {}
        return;
    }
    let root = PathBuf::from(&cfg.output_path);
    let sdir = session_dir(&root, meta.session_id, &meta.mode);
    let mut w = DsWriter::new();

    // session.json — the session's identity + config snapshot.
    let _ = write_pretty(
        &sdir.join("session.json"),
        &serde_json::json!({
            "session_id": meta.session_id,
            "mode": meta.mode,
            "exchange": meta.exchange,
            "currency": meta.currency,
            "portfolio_capital_usd": meta.capital,
            "started_at_ms": meta.started_at_ms,
            "tae_activated": meta.tae_activated,
            "config_snapshot": meta.config_snapshot,
        }),
    )
    .await;
    println!(
        "🧪 DS Export: writing session S{:04} to {}",
        meta.session_id,
        sdir.display()
    );

    // Offset bookkeeping for the DB-driven I-tier appends.
    let mut last_ids: std::collections::HashMap<&'static str, i64> =
        std::collections::HashMap::new();

    let mut tick = tokio::time::interval(std::time::Duration::from_secs(
        cfg.flush_interval_secs.max(1),
    ));

    loop {
        tokio::select! {
            msg = rx.recv() => {
                let Some(msg) = msg else { break };
                match msg {
                    database_storage::TelemetryMsg::InsertSnapshot(snap)
                        if cfg.capture_market => {
                            let value = serde_json::to_value(&*snap).unwrap_or(serde_json::Value::Null);
                            let rel = format!("market/{}.{}.ndjson", snap.symbol, snap.timeframe_secs);
                            w.write_line(&sdir, &rel, &value);
                        }
                    database_storage::TelemetryMsg::JournalTrade {
                        symbol, direction, entry_price, exit_price, entry_timestamp,
                        exit_timestamp, size, realized_pnl, roi_pct, allocated_usd, trigger,
                    }
                        if cfg.capture_trading => {
                            let value = serde_json::json!({
                                "symbol": symbol, "direction": direction,
                                "entry_price": entry_price, "exit_price": exit_price,
                                "entry_timestamp": entry_timestamp, "exit_timestamp": exit_timestamp,
                                "size": size, "realized_pnl": realized_pnl,
                                "roi_pct": roi_pct, "allocated_usd": allocated_usd,
                                "trigger": trigger,
                            });
                            w.write_line(&sdir, "trading/trades.ndjson", &value);
                        }
                    database_storage::TelemetryMsg::InsertLiquidationEvent {
                        exchange,
                        symbol,
                        side,
                        price,
                        size_usd,
                        timestamp_ms,
                        venue_order_id,
                    }
                        if cfg.capture_trading => {
                            let value = serde_json::json!({
                                "exchange": exchange, "symbol": symbol, "side": side,
                                "price": price, "size_usd": size_usd,
                                "timestamp_ms": timestamp_ms, "venue_order_id": venue_order_id,
                            });
                            w.write_line(&sdir, "trading/liquidation_events.ndjson", &value);
                        }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                w.flush_all();
                if cfg.capture_analytics || cfg.capture_trading {
                    append_db_table(&pool, &sdir, &mut w, &mut last_ids, meta.session_id).await;
                    w.flush_all();
                }
            }
        }
    }
    w.flush_all();
    println!(
        "🧪 DS Export: session S{:04} exporter stopped",
        meta.session_id
    );
}

/// Append rows newer than the recorded offset from the I-tier tables
/// (session-scoped where the column exists).
async fn append_db_table(
    pool: &SqlitePool,
    sdir: &std::path::Path,
    w: &mut DsWriter,
    last_ids: &mut std::collections::HashMap<&'static str, i64>,
    session_id: i64,
) {
    let tables: [(&'static str, &'static str, bool); 6] = [
        // (key, table, has_session_id)
        ("equity", "portfolio_equity_history", true),
        ("activity", "automation_activity", true),
        ("risk_events", "risk_control_events", true),
        ("strategy", "strategy_analytics_history", false),
        ("risk", "risk_analytics_history", false),
        ("performance", "performance_matrix_summaries", false),
    ];
    for (key, table, session_scoped) in tables {
        let since = *last_ids.get(key).unwrap_or(&0);
        let sql = if session_scoped {
            format!("SELECT * FROM {table} WHERE id > ?1 AND session_id = ?2 ORDER BY id")
        } else {
            format!("SELECT * FROM {table} WHERE id > ?1 ORDER BY id")
        };
        let mut q = sqlx::query(&sql).bind(since);
        if session_scoped {
            q = q.bind(session_id);
        }
        let rows: Vec<sqlx::sqlite::SqliteRow> = match q.fetch_all(pool).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut max_id = since;
        for row in &rows {
            let id: i64 = row.try_get("id").unwrap_or(0);
            max_id = max_id.max(id);
            let value: serde_json::Value = sqlite_row_to_json(row);
            let rel = match key {
                "equity" => "trading/equity.ndjson".to_string(),
                "activity" => "trading/activity.ndjson".to_string(),
                "risk_events" => "trading/risk_events.ndjson".to_string(),
                "strategy" => "trading/analytics/strategy.ndjson".to_string(),
                "risk" => "trading/analytics/risk.ndjson".to_string(),
                _ => "trading/analytics/performance.ndjson".to_string(),
            };
            w.write_line(sdir, &rel, &value);
        }
        last_ids.insert(key, max_id);
    }
}

fn sqlite_row_to_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    use sqlx::{Column as _, Row as _};
    let mut map = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let v: Option<String> = row.try_get(name.as_str()).ok().flatten();
        let parsed = v
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or_else(|| {
                v.map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null)
            });
        map.insert(name, parsed);
    }
    serde_json::Value::Object(map)
}
