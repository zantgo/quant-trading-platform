//! # Data-Science export layer (v10)
//!
//! NDJSON mirrors of every artifact the GUI renders, written to the
//! `[data_science].output_path` tree (default `./ds/`). One producer,
//! three sinks: SQLite, the WebSocket/GUI, and these files — so `ds/`
//! rows are byte-identical to what the dashboards render.
//!
//! Layout:
//! ```text
//! ./ds/
//! ├── sessions/S0007_paper/
//! │   ├── session.json                      # pretty — mode, capital, config, timestamps
//! │   ├── market/BTC-USDT.60.ndjson         # full MarketSnapshot per candle (all MME)
//! │   └── trading/
//! │       ├── trades.ndjson | liquidation_events.ndjson | equity.ndjson
//! │       ├── activity.ndjson | risk_events.ndjson
//! │       └── analytics/strategy.ndjson | risk.ndjson | performance.ndjson
//! └── backtests/BT0042_historical/
//!     ├── run.json                          # pretty — params + summary + NHST stats
//!     └── trades.ndjson | equity.ndjson | portfolio.ndjson | signals.ndjson
//!         + input_bars/BTC-USDT.60.ndjson
//! ```

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

/// Directory helpers (shared by the live exporter and the backtest writer).
pub fn session_dir(root: &Path, session_id: i64, mode: &str) -> PathBuf {
    root.join(format!(
        "sessions/S{:04}_{}",
        session_id,
        mode.replace(' ', "_")
    ))
}

pub fn backtest_dir(root: &Path, backtest_id: i64, mode: &str) -> PathBuf {
    root.join(format!(
        "backtests/BT{:04}_{}",
        backtest_id,
        mode.replace(' ', "_")
    ))
}

/// Synchronous NDJSON writer registry (per-file append, buffered). The
/// exporter task holds one registry; flushes happen on the configured
/// interval.
pub struct DsWriter {
    writers: HashMap<String, BufWriter<std::fs::File>>,
}

impl DsWriter {
    pub fn new() -> Self {
        Self {
            writers: HashMap::new(),
        }
    }

    /// Append one JSON line to `rel` (relative to the DS root).
    pub fn write_line(&mut self, root: &Path, rel: &str, value: &serde_json::Value) {
        if !self.writers.contains_key(rel) {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(f) => {
                    self.writers.insert(rel.to_string(), BufWriter::new(f));
                }
                Err(e) => {
                    eprintln!("DS export: cannot open {}: {e}", path.display());
                    return;
                }
            }
        }
        if let Some(w) = self.writers.get_mut(rel) {
            if let Ok(line) = serde_json::to_string(value) {
                let _ = w.write_all(line.as_bytes());
                let _ = w.write_all(b"\n");
            }
        }
    }

    pub fn flush_all(&mut self) {
        for w in self.writers.values_mut() {
            let _ = w.flush();
        }
    }
}

impl Default for DsWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Write a pretty JSON artifact (session.json / run.json).
pub async fn write_pretty(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let mut f = tokio::fs::File::create(path).await?;
    f.write_all(pretty.as_bytes()).await?;
    f.write_all(b"\n").await?;
    Ok(())
}

/// Write the DS artifact tree for a finished backtest run (called from
/// `persist_backtest_run` so web and CLI runs share the exact path).
#[allow(clippy::too_many_arguments)]
pub async fn write_backtest_ds(
    root: &std::path::Path,
    backtest_id: i64,
    mode: &str,
    params_json: &str,
    summary_json: &str,
    stats_json: &str,
    trades: &[serde_json::Value],
    equity: &[serde_json::Value],
    portfolio: &[serde_json::Value],
    signals: &[serde_json::Value],
    input_bars: &std::collections::HashMap<String, Vec<serde_json::Value>>,
) {
    let bdir = crate::ds_export::backtest_dir(root, backtest_id, mode);
    // v10.1 robustness: a backtest id maps to exactly one run per DB
    // lifetime. If the directory already holds artifacts (a DB reset
    // reused ids, or an interrupted run), truncate it — appending would
    // mix old rows into the new run and break the DS invariants.
    let _ = std::fs::remove_dir_all(&bdir);
    let run: serde_json::Value = serde_json::json!({
        "backtest_id": backtest_id,
        "mode": mode,
        "params": serde_json::from_str::<serde_json::Value>(params_json).unwrap_or(serde_json::Value::Null),
        "summary": serde_json::from_str::<serde_json::Value>(summary_json).unwrap_or(serde_json::Value::Null),
        "stats": serde_json::from_str::<serde_json::Value>(stats_json).unwrap_or(serde_json::Value::Null),
    });
    let _ = write_pretty(&bdir.join("run.json"), &run).await;
    let mut w = DsWriter::new();
    for t in trades {
        w.write_line(&bdir, "trades.ndjson", t);
    }
    for e in equity {
        w.write_line(&bdir, "equity.ndjson", e);
    }
    for p in portfolio {
        w.write_line(&bdir, "portfolio.ndjson", p);
    }
    for s in signals {
        w.write_line(&bdir, "signals.ndjson", s);
    }
    for (symbol_tf, bars) in input_bars {
        for b in bars {
            w.write_line(&bdir, &format!("input_bars/{symbol_tf}.ndjson"), b);
        }
    }
    w.flush_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_appends_parseable_ndjson_lines() {
        let tmp = std::env::temp_dir().join(format!("ds_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mut w = DsWriter::new();
        for i in 0..5 {
            w.write_line(
                &tmp,
                "market/BTC-USDC.60.ndjson",
                &serde_json::json!({
                    "i": i, "price": 100.0 + i as f64,
                }),
            );
        }
        w.flush_all();
        let content = std::fs::read_to_string(tmp.join("market/BTC-USDC.60.ndjson")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 5, "exactly 5 NDJSON lines");
        for (i, line) in lines.iter().enumerate() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["i"], i);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dir_names_carry_identifiers() {
        let root = Path::new("/tmp/ds");
        assert_eq!(
            session_dir(root, 7, "paper"),
            PathBuf::from("/tmp/ds/sessions/S0007_paper")
        );
        assert_eq!(
            backtest_dir(root, 42, "historical"),
            PathBuf::from("/tmp/ds/backtests/BT0042_historical")
        );
    }

    #[tokio::test]
    async fn backtest_writer_overwrites_on_rerun() {
        let tmp = std::env::temp_dir().join(format!("ds_bt_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let trades = |n: i64| {
            (0..n)
                .map(|i| serde_json::json!({ "seq": i, "pnl": 1.0 }))
                .collect::<Vec<_>>()
        };
        let eq = |n: i64| {
            (0..n)
                .map(|i| serde_json::json!({ "ts_secs": i, "equity": 1000.0 }))
                .collect::<Vec<_>>()
        };
        // First write: 5 trades, 3 equity points.
        write_backtest_ds(
            &tmp,
            1,
            "historical",
            "{}",
            "{\"total_trades\":5}",
            "{}",
            &trades(5),
            &eq(3),
            &[],
            &[],
            &std::collections::HashMap::new(),
        )
        .await;
        // Rerun with the same id (DB reset scenario): 2 trades, 2 equity.
        write_backtest_ds(
            &tmp,
            1,
            "historical",
            "{}",
            "{\"total_trades\":2}",
            "{}",
            &trades(2),
            &eq(2),
            &[],
            &[],
            &std::collections::HashMap::new(),
        )
        .await;
        let bdir = backtest_dir(&tmp, 1, "historical");
        let trades_lines = std::fs::read_to_string(bdir.join("trades.ndjson")).unwrap();
        let eq_lines = std::fs::read_to_string(bdir.join("equity.ndjson")).unwrap();
        assert_eq!(trades_lines.lines().count(), 2, "rerun truncates trades");
        assert_eq!(eq_lines.lines().count(), 2, "rerun truncates equity");
        assert!(
            std::fs::read_to_string(bdir.join("run.json"))
                .unwrap()
                .contains("\"total_trades\": 2"),
            "run.json reflects the rerun"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
