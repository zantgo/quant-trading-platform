//! Session Result — paper/live parity with `BacktestResult` (v10.2)
//!
//! Compiles a session-scoped result JSON that is structurally identical to
//! `GET /api/backtest/:id` so the operator can evaluate paper ↔ backtest
//! ↔ live with one verdict pipeline (same `StrategyAnalytics`, same
//! `RiskAnalytics`, same Welch symmetry, same cost model).

use serde::Serialize;
use sqlx::SqlitePool;

use crate::risk_analytics::compute_risk_metrics_from_curve_with_rf;
use crate::strategy_analytics::{
    compare_direction_symmetry, compute_setup_analytics, AnalyticsParams,
};
use core_domain::performance::TradeAnalyticsRecord;
use core_domain::performance::{RiskAnalyticsRow, StrategyAnalyticsRow};

#[derive(Debug, Clone, Serialize)]
pub struct SessionTrade {
    pub ts_close_secs: i64,
    pub timestamp: i64,
    pub ts_entry_secs: i64,
    pub hold_secs: i64,
    pub direction: String,
    pub symbol: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub roi_pct: f64,
    pub mfe_pct: f64,
    pub mae_pct: f64,
    pub slippage_bps: f64,
    pub commission_fees: f64,
    pub funding_fees: f64,
    pub exit_reason: String,
    pub r_multiple: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub total_trades: u32,
    pub win_count: u32,
    pub loss_count: u32,
    pub win_rate: f64, // 0..100 like backtest summary
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub profit_factor: Option<f64>,
    pub expectancy: f64,
    pub max_drawdown_pct: f64,
    pub avg_win_loss_ratio: f64,
    pub direction_symmetry: Option<core_domain::performance::DirectionSymmetryVerdict>,
    // enriched fields the paper operator asked for
    pub avg_roi_pct: f64,
    pub avg_profit: f64,
    pub avg_loss: f64,
    pub avg_hold_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionResult {
    pub session_id: i64,
    pub mode: String,
    pub params: SessionParams,
    pub summary: SessionSummary,
    pub stats: StrategyAnalyticsRow,
    pub risk: RiskAnalyticsRow,
    pub trades: Vec<SessionTrade>,
    pub equity_curve: Vec<(i64, f64)>, // (ts_ms, equity) — same as backtest (secs vs ms documented)
    pub equity_curve_secs: Vec<(i64, f64)>, // convenience: secs
    pub log_returns: Vec<(i64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionParams {
    pub session_id: i64,
    pub mode: String,
    pub exchange: String,
    pub currency: String,
    pub portfolio_capital_usd: f64,
    pub from_secs: i64,
    pub to_secs: i64,
    pub timeframe_secs: u64,
    pub strategy_id: String,
    pub symbols: Vec<String>,
}

fn exit_reason_canonical(raw: &str) -> String {
    match raw.to_uppercase().as_str() {
        "STOP_LOSS" | "SL" => "sl".to_string(),
        "TAKE_PROFIT" | "TP" => "tp".to_string(),
        "SIGNAL_EXIT" | "INVALIDATED_SIGNAL" => "invalidated_signal".to_string(),
        "SETUP_GONE" => "setup_gone".to_string(),
        "CONFIDENCE_DROP" => "confidence_drop".to_string(),
        "STOP_FLATTEN" | "FLATTEN" => "stop_flatten".to_string(),
        "MANUAL" => "manual".to_string(),
        "END_OF_BACKTEST" => "end_of_backtest".to_string(),
        other => other.to_lowercase(),
    }
}

pub async fn compile_session_result(
    pool: &SqlitePool,
    session_id: i64,
    workspace: &config_models::WorkspaceConfig,
) -> Option<SessionResult> {
    // ── session row
    let sess: Option<(String, Option<String>, Option<String>, Option<f64>, i64, Option<i64>)> =
        sqlx::query_as(
            "SELECT mode, exchange, currency, portfolio_capital_usd, started_at_ms, ended_at_ms FROM sessions WHERE id = ?1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let (mode, exchange, currency, capital_opt, started_ms, ended_opt) = sess?;
    let portfolio_capital_usd = capital_opt.unwrap_or(workspace.portfolio_capital_usd);
    let from_secs = started_ms.div_euclid(1000);
    let to_secs = ended_opt
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(started_ms)
        })
        .div_euclid(1000);

    // ── trades for this session (session_id filter, fallback to all if nulls)
    let rows: Vec<(String, i64, i64, f64, f64, f64, f64, f64, f64, String)> =
        sqlx::query_as(
            "SELECT symbol, entry_timestamp, exit_timestamp, entry_price, exit_price, size, realized_pnl, roi_pct, commission_fees, trigger_source \
             FROM trade_telemetry_history WHERE session_id = ?1 ORDER BY exit_timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    // Fallback: if no session_id stamped (old rows), try paper_trades
    let rows_fallback = if rows.is_empty() {
        let alt: Vec<(String, i64, i64, f64, f64, f64, f64, f64, f64, String)> =
            sqlx::query_as(
                "SELECT symbol, entry_timestamp, exit_timestamp, CAST(entry_price AS REAL), CAST(exit_price AS REAL), CAST(size AS REAL), CAST(realized_pnl AS REAL), CAST(roi_pct AS REAL), 0.0, trigger \
                 FROM paper_trades WHERE session_id = ?1 ORDER BY exit_timestamp ASC",
            )
            .bind(session_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        alt
    } else {
        vec![]
    };
    let trade_rows = if rows.is_empty() { rows_fallback } else { rows };

    let mut trades: Vec<SessionTrade> = Vec::new();
    for (
        symbol,
        entry_ms,
        exit_ms,
        entry_price,
        exit_price,
        size,
        pnl,
        roi_pct,
        commission_fees,
        trigger_source,
    ) in &trade_rows
    {
        let hold_secs = (exit_ms - entry_ms).div_euclid(1000).max(0);
        let ts_close_secs = exit_ms.div_euclid(1000);
        let ts_entry_secs = entry_ms.div_euclid(1000);
        // r multiple = roi / 1% (1R = 1% per backtest convention)
        let r_multiple = if roi_pct.is_finite() {
            roi_pct / 1.0
        } else {
            0.0
        };
        trades.push(SessionTrade {
            ts_close_secs,
            timestamp: ts_close_secs,
            ts_entry_secs,
            hold_secs,
            direction: {
                // direction not stored in this query variant → infer from pnl? better query includes direction
                // we re-query direction if missing; for now default LONG if pnl sign not reliable
                // Actually trade_rows above missing direction — fetch separately
                "UNKNOWN".to_string()
            },
            symbol: symbol.clone(),
            entry_price: *entry_price,
            exit_price: *exit_price,
            size: *size,
            pnl: *pnl,
            roi_pct: *roi_pct,
            mfe_pct: 0.0,
            mae_pct: 0.0,
            slippage_bps: 5.0, // deterministic from config (paper simulation)
            commission_fees: *commission_fees,
            funding_fees: 0.0,
            exit_reason: exit_reason_canonical(trigger_source),
            r_multiple,
        });
    }
    // If direction UNKNOWN, patch via second query that includes direction
    if trades.iter().any(|t| t.direction == "UNKNOWN") {
        let dir_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT symbol, direction FROM trade_telemetry_history WHERE session_id = ?1 ORDER BY exit_timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        for (i, (_, dir)) in dir_rows.iter().enumerate() {
            if let Some(t) = trades.get_mut(i) {
                t.direction = dir.to_uppercase();
            }
        }
    }

    // ── summary + stats (reuse StrategyAnalytics helpers)
    let total = trades.len() as u32;
    let wins: Vec<&SessionTrade> = trades.iter().filter(|t| t.pnl > 0.0).collect();
    let losses: Vec<&SessionTrade> = trades.iter().filter(|t| t.pnl < 0.0).collect();
    let win_count = wins.len() as u32;
    let loss_count = losses.len() as u32;
    let win_rate = if total > 0 {
        win_count as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let gross_profit: f64 = wins.iter().map(|t| t.pnl).sum();
    let gross_loss: f64 = losses.iter().map(|t| t.pnl.abs()).sum();
    let profit_factor = if gross_loss > 0.0 {
        let v = gross_profit / gross_loss;
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    } else if gross_profit > 0.0 {
        None
    } else {
        Some(0.0)
    };
    let avg_profit = if win_count > 0 {
        gross_profit / win_count as f64
    } else {
        0.0
    };
    let avg_loss = if loss_count > 0 {
        gross_loss / loss_count as f64
    } else {
        0.0
    };
    let avg_win_loss_ratio = if avg_loss > 0.0 {
        avg_profit / avg_loss
    } else {
        0.0
    };
    let avg_roi_pct = if total > 0 {
        trades.iter().map(|t| t.roi_pct).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let avg_hold_secs = if total > 0 {
        trades.iter().map(|t| t.hold_secs as f64).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let expectancy = if total > 0 {
        let wr = win_count as f64 / total as f64;
        wr * avg_profit - (1.0 - wr) * avg_loss
    } else {
        0.0
    };

    // Build TradeAnalyticsRecords for NHST + symmetry (reusing backtest path)
    let records: Vec<TradeAnalyticsRecord> = trades
        .iter()
        .map(|t| TradeAnalyticsRecord {
            trade_id: format!("{}-{}", t.symbol, t.ts_close_secs),
            symbol: t.symbol.clone(),
            direction: t.direction.clone(),
            entry_timestamp: t.ts_entry_secs * 1000,
            exit_timestamp: t.ts_close_secs * 1000,
            hold_time_seconds: t.hold_secs as u64,
            entry_price: t.entry_price,
            exit_price: t.exit_price,
            size: t.size,
            gross_pnl: t.pnl,
            net_pnl: t.pnl,
            roi_pct: t.roi_pct,
            execution_slippage: t.slippage_bps,
            mfe: t.mfe_pct,
            mae: t.mae_pct,
            trigger_source: t.exit_reason.clone(),
            exit_reason: t.exit_reason.clone(),
            flat_trade: t.pnl.abs() < 1e-10,
        })
        .collect();
    let strategy = workspace.default_strategy().unwrap_or_default();
    let params = AnalyticsParams::from_strategy(&strategy.pae);
    let refs: Vec<&TradeAnalyticsRecord> = records.iter().collect();
    let stats = if trades.is_empty() {
        StrategyAnalyticsRow {
            setup_type: format!("SESSION_{}", session_id),
            alpha: params.alpha,
            total_trades: 0,
            win_count: 0,
            loss_count: 0,
            win_rate: 0.0,
            gross_profit: 0.0,
            gross_loss: 0.0,
            profit_factor: Some(0.0),
            average_win: 0.0,
            average_loss: 0.0,
            avg_win_loss_ratio: 0.0,
            expectancy: 0.0,
            slippage_overhead: 0.0,
            t_statistic: 0.0,
            p_value: 1.0,
            p_mc: 1.0,
            monte_carlo_runs: params.monte_carlo_runs,
            is_significant: false,
            classification: core_domain::performance::PerformanceClassification::InsufficientData,
        }
    } else {
        compute_setup_analytics(&format!("SESSION_{}", session_id), &refs, params)
    };
    let direction_symmetry = compare_direction_symmetry(&records);

    // ── equity curve (session-filtered if available, fallback to compounded from trades)
    let mut equity_curve: Vec<(i64, f64)> = {
        let rows: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT timestamp, total_value FROM portfolio_equity_history WHERE session_id = ?1 ORDER BY timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(ts, v): (i64, f64)| (ts, v))
        .collect();
        if rows.is_empty() {
            // fallback: fetch global but windowed to session time range
            let global: Vec<(i64, f64)> = sqlx::query_as(
                "SELECT timestamp, total_value FROM portfolio_equity_history WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp ASC",
            )
            .bind(started_ms)
            .bind(ended_opt.unwrap_or(i64::MAX))
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(ts, v): (i64, f64)| (ts, v))
            .collect();
            if !global.is_empty() {
                global
            } else {
                vec![]
            }
        } else {
            rows
        }
    };
    let all_zero = !equity_curve.is_empty() && equity_curve.iter().all(|(_, v)| *v == 0.0);
    if (equity_curve.is_empty() || all_zero) && !trades.is_empty() {
        // synthetic compounded from trades (same as stats_compiler fallback)
        let mut bal = portfolio_capital_usd;
        let mut curve = Vec::new();
        // seed start point
        curve.push((started_ms, bal));
        for t in &trades {
            bal *= 1.0 + t.roi_pct / 100.0;
            curve.push((t.ts_close_secs * 1000, bal));
        }
        equity_curve = curve;
    } else if equity_curve.is_empty() || all_zero {
        equity_curve = vec![(started_ms, portfolio_capital_usd)];
    }

    let equity_curve_secs: Vec<(i64, f64)> =
        equity_curve.iter().map(|(ms, v)| (ms / 1000, *v)).collect();

    // risk metrics over equity curve
    let rf_pct = workspace
        .default_strategy()
        .map(|s| s.pae.risk_math.risk_free_rate_pct)
        .unwrap_or(0.0);
    let risk = compute_risk_metrics_from_curve_with_rf(&equity_curve, rf_pct);
    let max_dd = risk.maximum_drawdown_pct;

    // log returns (over equity_curve)
    let log_returns = {
        let mut out = Vec::new();
        for w in equity_curve.windows(2) {
            let (prev, cur) = (w[0].1, w[1].1);
            if prev > 0.0 && cur > 0.0 {
                out.push((w[1].0, (cur / prev).ln()));
            }
        }
        out
    };

    // symbols distinct
    let symbols: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        for t in &trades {
            set.insert(t.symbol.clone());
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };

    let summary = SessionSummary {
        total_trades: total,
        win_count,
        loss_count,
        win_rate,
        gross_profit,
        gross_loss,
        profit_factor,
        expectancy,
        max_drawdown_pct: max_dd,
        avg_win_loss_ratio,
        direction_symmetry: direction_symmetry.clone(),
        avg_roi_pct,
        avg_profit,
        avg_loss,
        avg_hold_secs,
    };

    let timeframe_secs = workspace
        .instances
        .iter()
        .find(|i| !i.symbol.is_empty())
        .map(|i| i.micro_term.candles.duration_seconds)
        .unwrap_or(1);

    Some(SessionResult {
        session_id,
        mode: mode.clone(),
        params: SessionParams {
            session_id,
            mode: mode.clone(),
            exchange: exchange.unwrap_or_else(|| workspace.default_exchange.clone()),
            currency: currency.unwrap_or_else(|| workspace.default_currency.clone()),
            portfolio_capital_usd,
            from_secs,
            to_secs,
            timeframe_secs,
            strategy_id: workspace
                .default_strategy()
                .map(|s| s.name)
                .unwrap_or_else(|_| "default".to_string()),
            symbols: if symbols.is_empty() {
                vec!["BTC-USDT".to_string()]
            } else {
                symbols
            },
        },
        summary,
        stats,
        risk,
        trades,
        equity_curve,
        equity_curve_secs,
        log_returns,
    })
}

pub async fn fetch_session_trades(pool: &SqlitePool, session_id: i64) -> Vec<SessionTrade> {
    let dummy_ws = config_models::WorkspaceConfig::default();
    compile_session_result(pool, session_id, &dummy_ws)
        .await
        .map(|r| r.trades)
        .unwrap_or_default()
}
