//! BTE recorded-decision replay (moved from PAE L5).
//!
//! Deterministic replay of **recorded MME decisions** through the unchanged
//! TAE setup executor + unified paper engine: completed `market_snapshots`
//! (with their persisted `opportunity_json` / `decision_context_json` /
//! `analysis_json` / `advisory_json` matrices) are fed in time order to the
//! same code the live executor runs. The simulated trades then get the full
//! NHST treatment (t-test, 10k Monte Carlo, α = 0.05, edge classification).
//!
//! See docs/engines/backtesting-engine/08-03-historical-runner.md.

use config_models::MinimalTaeConfig;
use core_domain::performance::{StrategyAnalyticsRow, TradeAnalyticsRecord};
use database_storage::queries::snapshots::RecordedSnapshot;
use portfolio_supervisor::execution::ExecutionEngine;
use portfolio_supervisor::paper_trading::FeesConfig;
use portfolio_supervisor::setup_executor::{SetupExecutor, TickContext};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

use performance_analytics::strategy_analytics::compute_setup_analytics;

/// Backtest window + capital parameters.
///
/// Unit contract: the API accepts milliseconds (`from_ms` / `to_ms`) and the
/// gateway converts to **Unix seconds** before constructing these params —
/// `market_snapshots.timestamp` and the candle archive are stored in seconds.
/// `from_secs`/`to_secs` are inclusive bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestParams {
    pub symbol: String,
    pub timeframe_secs: u64,
    pub from_secs: i64,
    pub to_secs: i64,
    #[serde(default = "default_capital")]
    #[serde(alias = "initial_capital")]
    /// v9 (F-07): ONE capital dial — the simulated account seed
    /// (`portfolio_capital_usd`, same field name as paper/live).
    pub portfolio_capital_usd: f64,
}

fn default_capital() -> f64 {
    1000.0
}

/// One simulated close.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTrade {
    pub timestamp: i64,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub exit_reason: String,
    /// v10 enrichment: entry timestamp (secs), hold time, MFE/MAE, ROI.
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
    /// v10.1 cost attribution: entry+exit slippage (bps), exit commission,
    /// funding accrued on the position (negative = paid).
    #[serde(default)]
    pub slippage_bps: f64,
    #[serde(default)]
    pub commission_fees: f64,
    #[serde(default)]
    pub funding_fees: f64,
    /// v10.2 institutional: R-multiple (roi/1%) and symbol attribution
    #[serde(default)]
    pub r_multiple: f64,
    #[serde(default)]
    pub symbol: String,
}

/// The full backtest result: classic metrics + the NHST block + trades +
/// equity curve. Persisted to `backtest_runs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub params: BacktestParams,
    pub total_trades: u32,
    pub win_count: u32,
    pub loss_count: u32,
    pub win_rate: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub profit_factor: Option<f64>,
    pub expectancy: f64,
    pub max_drawdown_pct: f64,
    /// v8.2: true when the run was aborted via the cancel flag (no
    /// persistence should happen for cancelled runs).
    #[serde(default)]
    pub cancelled: bool,
    /// NHST block (t-statistic, p-value, Monte Carlo p, α, significance,
    /// edge classification) over the simulated trades.
    pub stats: StrategyAnalyticsRow,
    /// v10.1: long/short symmetry verdict (Welch t-test on roi_pct;
    /// `None` when either side has < 10 trades).
    #[serde(default)]
    pub direction_symmetry: Option<core_domain::performance::DirectionSymmetryVerdict>,
    pub trades: Vec<BacktestTrade>,
    /// `(timestamp_ms, total_equity)` sampled per snapshot (downsampled).
    pub equity_curve: Vec<(i64, f64)>,
    /// Per-tick synthesized decision snapshots (DS persistence; not on
    /// the wire).
    #[serde(default, skip_serializing)]
    pub signals: Vec<database_storage::queries::backtest_ds::DsSignal>,
    /// Per-tick capital/exposure/drawdown samples (DS persistence; not on
    /// the wire).
    #[serde(default, skip_serializing)]
    pub portfolio: Vec<database_storage::queries::backtest_ds::DsPortfolioPoint>,
}

/// Maximum number of recorded snapshots replayed per run (keeps the
/// synchronous endpoint bounded).
pub const BACKTEST_MAX_SNAPSHOTS: u32 = 50_000;

fn snap_to_market(symbol: &str, rec: &RecordedSnapshot) -> core_domain::models::MarketSnapshot {
    let mid = Decimal::from_f64_retain(rec.mid_price).unwrap_or_default();
    let mut snap = core_domain::models::MarketSnapshot {
        symbol: symbol.to_string(),
        timeframe_secs: rec.timeframe_secs as u64,
        timestamp: rec.timestamp as u64,
        is_completed: Some(true),
        mid_price: mid,
        bid_price: mid,
        ask_price: mid,
        close: rec.close.and_then(Decimal::from_f64_retain),
        // CR-10: the candle-derived heatmap bands.
        state_bands: None,
        timeframe_state: None,
        ..core_domain::models::MarketSnapshot::default()
    };
    // CR-15: only the analysis (model) column is restored. The opportunity /
    // decision / advisory columns are still present in the schema — they are
    // dropped by the P5 forward migration — but they hold NULL for every row
    // written since this phase, and reading them would reintroduce the very
    // strategy surface the boundary removes.
    snap.analysis = rec
        .analysis_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok());
    snap
}

/// Run a backtest over recorded snapshots with the unchanged executor +
/// unified paper engine. Deterministic: identical inputs ⇒ identical result.
///
/// v10: the run's bound strategy flows into the executor tick (parity with
/// the historical runner and live) — the TAE lifecycle dials replay exactly.
pub async fn run_backtest(
    pool: &SqlitePool,
    params: &BacktestParams,
    tae_cfg: &MinimalTaeConfig,
    fees: &FeesConfig,
    cross_leverage: u32,
    analytics: performance_analytics::strategy_analytics::AnalyticsParams,
    strategy: &config_models::StrategyConfig,
) -> BacktestResult {
    let records = database_storage::queries::snapshots::query_backtest_snapshots(
        pool,
        &params.symbol,
        params.timeframe_secs,
        params.from_secs,
        params.to_secs,
        BACKTEST_MAX_SNAPSHOTS,
    )
    .await;

    let engine = ExecutionEngine::new(fees.clone());
    engine
        .set_initial_equity(
            Decimal::from_f64_retain(params.portfolio_capital_usd).unwrap_or(dec!(1000)),
        )
        .await;
    engine.set_cross_leverage(cross_leverage).await;
    let engine = std::sync::Arc::new(engine);
    let executor = SetupExecutor::new(engine.clone(), tae_cfg);

    let mut trades: Vec<BacktestTrade> = Vec::new();
    let mut equity_points: Vec<(i64, f64)> = Vec::new();
    let mut entry_ts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut signals: Vec<database_storage::queries::backtest_ds::DsSignal> = Vec::new();
    let mut portfolio: Vec<database_storage::queries::backtest_ds::DsPortfolioPoint> = Vec::new();
    let mut peak_equity: f64 = params.portfolio_capital_usd;

    // v10.1 (F4): simulated 8h funding clock — settle at every multiple of
    // FUNDING_INTERVAL_SECS crossed by the replay clock (mirrors the
    // historical runner; direction-aware settlement accrues per position).
    const FUNDING_INTERVAL_SECS: i64 = 8 * 3600;
    let first_ts = records
        .first()
        .map(|r| r.timestamp)
        .unwrap_or(params.from_secs);
    let mut next_funding = first_ts
        .div_euclid(FUNDING_INTERVAL_SECS)
        .saturating_mul(FUNDING_INTERVAL_SECS)
        .saturating_add(FUNDING_INTERVAL_SECS);

    for rec in &records {
        let snap = snap_to_market(&params.symbol, rec);
        let mid = snap.mid_price;

        while rec.timestamp >= next_funding {
            engine.settle_funding().await;
            next_funding = next_funding.saturating_add(FUNDING_INTERVAL_SECS);
        }

        // BTE v8 parity: the replay drives the SAME per-tick session body
        // the daemon runs (`run_tick`); `capture_last_close` snapshots the
        // close outcome between fills and the executor tick so the trade
        // log records every simulated close. The executor consumes
        // `take_last_close` afterwards exactly as it does live.
        if !entry_ts.contains_key(&params.symbol)
            && engine.get_position(&params.symbol).await.is_some()
        {
            entry_ts.insert(params.symbol.clone(), rec.timestamp);
        }
        let outcome = portfolio_supervisor::execution::session_tick::run_tick(
            &engine,
            &executor,
            "backtest",
            &params.symbol,
            &[&snap],
            mid,
            TickContext {
                safety_allows_entry: true,
                lifecycle_running: true,
                market_filter_allows_entry: true,
                entry_block_reason: None,
                candle_ts: rec.timestamp as u64,
                safety: None,
                dispatch: true,
                allocation_pct: None,
                strategy: Some(strategy.clone()),
            },
            None,
            true,
        )
        .await;

        if let Some(close) = &outcome.last_close {
            let direction = match outcome.last_close_direction {
                Some(config_models::Direction::Long) => "LONG",
                Some(config_models::Direction::Short) => "SHORT",
                None => "UNKNOWN",
            };
            let entry = outcome
                .last_close_entry
                .and_then(|d| d.to_f64())
                .unwrap_or(0.0);
            let size = outcome
                .last_close_size
                .and_then(|d| d.to_f64())
                .unwrap_or(0.0);
            let exit = if size > 0.0 {
                let pnl = close.pnl.to_f64().unwrap_or(0.0);
                match outcome.last_close_direction {
                    Some(config_models::Direction::Short) => entry - pnl / size,
                    _ => entry + pnl / size,
                }
            } else {
                entry
            };
            // Fallback: a position opened and closed within THIS bar was
            // never observed by the per-tick tracker — stamp the current bar.
            let ts_entry = entry_ts.remove(&params.symbol).unwrap_or(rec.timestamp);
            let hold_secs = if ts_entry > 0 {
                rec.timestamp - ts_entry
            } else {
                0
            };
            let roi_pct = if entry > 0.0 && size > 0.0 {
                close.pnl.to_f64().unwrap_or(0.0) / (entry * size) * 100.0
            } else {
                0.0
            };
            trades.push(BacktestTrade {
                timestamp: rec.timestamp,
                direction: direction.to_string(),
                entry_price: entry,
                exit_price: exit,
                size,
                pnl: close.pnl.to_f64().unwrap_or(0.0),
                exit_reason: close.exit_reason.clone(),
                ts_entry_secs: ts_entry,
                hold_secs,
                mfe_pct: outcome.last_close_mfe_pct.unwrap_or(0.0),
                mae_pct: outcome.last_close_mae_pct.unwrap_or(0.0),
                roi_pct,
                slippage_bps: close.slippage_bps,
                commission_fees: close.commission_fees.to_f64().unwrap_or(0.0),
                funding_fees: close.funding_fees.to_f64().unwrap_or(0.0),
                r_multiple: roi_pct / 1.0,
                symbol: params.symbol.clone(),
            });
        }

        // Equity curve sample: ledger + unrealized.
        let unrealized: Decimal = {
            let positions = engine.positions.read().await;
            positions.values().map(|p| p.unrealized_pnl).sum()
        };
        let total = engine.get_equity_decimal().await + unrealized;
        equity_points.push((rec.timestamp, total.to_f64().unwrap_or(0.0)));

        capture_tick_ds(
            rec.timestamp,
            params.timeframe_secs,
            &snap,
            &engine,
            &mut signals,
            &mut portfolio,
            &mut peak_equity,
        )
        .await;
    }

    // ── Classic + NHST statistics over the simulated trades ──
    finalize_result(
        params,
        trades,
        equity_points,
        2000,
        analytics,
        signals,
        portfolio,
    )
}

/// Shared per-tick DS capture for both runners: decision snapshots +
/// the capital/exposure/drawdown sample.
pub async fn capture_tick_ds(
    ts_secs: i64,
    timeframe_secs: u64,
    snap: &core_domain::models::MarketSnapshot,
    engine: &Arc<ExecutionEngine>,
    signals: &mut Vec<database_storage::queries::backtest_ds::DsSignal>,
    portfolio: &mut Vec<database_storage::queries::backtest_ds::DsPortfolioPoint>,
    peak_equity: &mut f64,
) {
    // CR-15: the decision / opportunity / advisory signal rows are gone with
    // the layers themselves. What remains below is the MODEL's own record —
    // bias and regime — which is still what a replay can honestly capture.
    if let Some(analysis) = snap.analysis.as_ref() {
        signals.push(database_storage::queries::backtest_ds::DsSignal {
            ts_secs,
            timeframe_secs,
            label: "analysis".to_string(),
            kind: "market_regime".to_string(),
            value: format!("{:?}", analysis.market_regime),
        });
        signals.push(database_storage::queries::backtest_ds::DsSignal {
            ts_secs,
            timeframe_secs,
            label: "analysis".to_string(),
            kind: "bias".to_string(),
            value: format!("{:?}", analysis.bias),
        });
    }

    let (margin_used, unrealized): (Decimal, Decimal) = {
        let positions = engine.positions.read().await;
        let margin: Decimal = positions.values().map(|p| p.size * p.entry_price).sum();
        let unreal: Decimal = positions.values().map(|p| p.unrealized_pnl).sum();
        (margin, unreal)
    };
    let ledger = engine.get_equity_decimal().await;
    let equity = ledger + unrealized;
    let equity_f = equity.to_f64().unwrap_or(0.0);
    let initial = *peak_equity;
    if equity_f > *peak_equity {
        *peak_equity = equity_f;
    }
    let drawdown_pct = if *peak_equity > 0.0 {
        ((*peak_equity - equity_f) / *peak_equity * 100.0).max(0.0)
    } else {
        0.0
    };
    let margin_f = margin_used.to_f64().unwrap_or(0.0);
    let exposure_pct = if initial > 0.0 {
        (margin_f / initial * 100.0).max(0.0)
    } else {
        0.0
    };
    portfolio.push(database_storage::queries::backtest_ds::DsPortfolioPoint {
        ts_secs,
        equity: equity_f,
        cash: ledger.to_f64().unwrap_or(0.0),
        margin_used: margin_f,
        exposure_pct,
        drawdown_pct,
        positions_open: engine.positions.read().await.len() as u32,
    });
}

/// Shared result assembly for the recorded and historical runners:
/// equity downsampling + classic metrics + the NHST block.
pub fn finalize_result(
    params: &BacktestParams,
    trades: Vec<BacktestTrade>,
    mut equity_points: Vec<(i64, f64)>,
    max_equity_points: u32,
    analytics: performance_analytics::strategy_analytics::AnalyticsParams,
    signals: Vec<database_storage::queries::backtest_ds::DsSignal>,
    portfolio: Vec<database_storage::queries::backtest_ds::DsPortfolioPoint>,
) -> BacktestResult {
    // Down-sample the equity curve to the configured cap.
    let cap = max_equity_points.max(10) as usize;
    if equity_points.len() > cap {
        let step = equity_points.len() / cap;
        equity_points = equity_points.iter().step_by(step.max(1)).cloned().collect();
        if equity_points.len() < 2 {
            if let Some(last) = equity_points.last().cloned() {
                equity_points.push(last);
            }
        }
    }

    let total = trades.len() as u32;
    let win_count = trades.iter().filter(|t| t.pnl > 0.0).count() as u32;
    let loss_count = total - win_count;
    let win_rate = if total > 0 {
        win_count as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let gross_profit: f64 = trades.iter().filter(|t| t.pnl > 0.0).map(|t| t.pnl).sum();
    let gross_loss: f64 = trades
        .iter()
        .filter(|t| t.pnl < 0.0)
        .map(|t| t.pnl.abs())
        .sum();
    let profit_factor = if gross_loss > 0.0 {
        Some(gross_profit / gross_loss)
    } else if gross_profit > 0.0 {
        Some(1_000_000.0)
    } else {
        None
    };
    let expectancy = if total > 0 {
        trades.iter().map(|t| t.pnl).sum::<f64>() / total as f64
    } else {
        0.0
    };

    let max_drawdown_pct = max_drawdown(&equity_points);

    let records: Vec<TradeAnalyticsRecord> = trades
        .iter()
        .map(|t| TradeAnalyticsRecord {
            trade_id: format!("bt_{}_{}", t.timestamp, t.direction),
            symbol: params.symbol.clone(),
            direction: t.direction.clone(),
            entry_timestamp: t.timestamp,
            exit_timestamp: t.timestamp,
            hold_time_seconds: 0,
            entry_price: t.entry_price,
            exit_price: t.exit_price,
            size: t.size,
            gross_pnl: t.pnl,
            net_pnl: t.pnl,
            roi_pct: t.roi_pct,
            execution_slippage: t.slippage_bps,
            mfe: t.mfe_pct,
            mae: t.mae_pct,
            trigger_source: "BACKTEST".to_string(),
            exit_reason: t.exit_reason.clone(),
            flat_trade: false,
        })
        .collect();
    let stats = compute_setup_analytics("BACKTEST", &records.iter().collect::<Vec<_>>(), analytics);
    let direction_symmetry =
        performance_analytics::strategy_analytics::compare_direction_symmetry(&records);

    BacktestResult {
        params: params.clone(),
        total_trades: total,
        win_count,
        loss_count,
        win_rate,
        gross_profit,
        gross_loss,
        profit_factor,
        expectancy,
        max_drawdown_pct,
        cancelled: false,
        stats,
        direction_symmetry,
        trades,
        equity_curve: equity_points,
        signals,
        portfolio,
    }
}

fn max_drawdown(points: &[(i64, f64)]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    let mut peak = points[0].1;
    let mut max_dd = 0.0;
    for (_, equity) in points {
        if *equity > peak {
            peak = *equity;
        }
        if peak > 0.0 {
            let dd = (peak - equity) / peak * 100.0;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::analysis::{AnalysisMatrix, MarketBias};
    use core_domain::models::MarketSnapshot;
    use rust_decimal_macros::dec;

    fn long_snapshot(ts: u64, mid: f64) -> MarketSnapshot {
        let mut snap = MarketSnapshot::default();
        snap.symbol = "BTC-USDC".to_string();
        snap.timeframe_secs = 60;
        snap.timestamp = ts;
        snap.is_completed = Some(true);
        snap.mid_price = Decimal::from_f64_retain(mid).unwrap_or_default();
        snap.bid_price = snap.mid_price;
        snap.ask_price = snap.mid_price;
        snap.close = Some(snap.mid_price);

        let mut a = AnalysisMatrix::empty("BTC-USDC");
        a.bias = MarketBias::Bullish;
        snap.analysis = Some(a);
        snap
    }

    async fn seed_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("mem pool");
        database_storage::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn tae_cfg() -> MinimalTaeConfig {
        MinimalTaeConfig {
            enabled: true,
            allocation_pct: 10.0,
            min_net_rr: 1.0,
            max_position_size_pct_of_equity: None,
            max_open_positions: 1,
            entry_mode: "zone_midpoint".to_string(),
            invalidate_on: "direction_flip".to_string(),
        }
    }

    #[tokio::test]
    async fn replay_records_trade_and_stats() {
        let pool = seed_pool().await;
        // Accept at 105 → fill at 94 → TP at 126.
        for (ts, mid) in [(1000u64, 105.0f64), (1001, 94.0), (1002, 126.0)] {
            database_storage::insert_snapshot_internal(&pool, &long_snapshot(ts, mid)).await;
        }

        let params = BacktestParams {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            from_secs: 0,
            to_secs: 10_000,
            portfolio_capital_usd: 1000.0,
        };
        let result = run_backtest(
            &pool,
            &params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            &config_models::StrategyConfig::default(),
        )
        .await;

        assert_eq!(result.total_trades, 1, "one simulated close expected");
        assert_eq!(result.win_count, 1);
        assert!(result.profit_factor.unwrap() > 1.0);
        assert_eq!(result.equity_curve.len(), 3);
        assert!(result.max_drawdown_pct >= 0.0);
        assert_eq!(result.trades[0].exit_reason, "tp");
        // NHST block present with the fixed α.
        assert_eq!(result.stats.alpha, 0.05);
        assert!(result.stats.total_trades >= 1);
    }

    #[tokio::test]
    async fn no_setups_yields_empty_result() {
        let pool = seed_pool().await;
        // Snapshots without any decision matrix → nothing to trade.
        let snap = MarketSnapshot {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            timestamp: 1000,
            is_completed: Some(true),
            mid_price: dec!(100),
            bid_price: dec!(100),
            ask_price: dec!(100),
            close: Some(dec!(100)),
            ..MarketSnapshot::default()
        };
        database_storage::insert_snapshot_internal(&pool, &snap).await;

        let params = BacktestParams {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            from_secs: 0,
            to_secs: 10_000,
            portfolio_capital_usd: 1000.0,
        };
        let result = run_backtest(
            &pool,
            &params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            &config_models::StrategyConfig::default(),
        )
        .await;
        assert_eq!(result.total_trades, 0);
        assert_eq!(result.equity_curve.len(), 1);
        assert_eq!(
            result.stats.classification,
            core_domain::performance::PerformanceClassification::InsufficientData
        );
    }

    #[tokio::test]
    async fn strict_setup_gone_closes_with_setup_gone_reason() {
        let pool = seed_pool().await;
        for (ts, mid) in [(1000u64, 105.0f64), (1001, 94.0)] {
            database_storage::insert_snapshot_internal(&pool, &long_snapshot(ts, mid)).await;
        }
        // Neutral candle: the actionable setup disappears.
        let mut neutral = long_snapshot(1002, 94.0);
        neutral.analysis.as_mut().unwrap().bias = MarketBias::Neutral;
        database_storage::insert_snapshot_internal(&pool, &neutral).await;

        let params = BacktestParams {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            from_secs: 0,
            to_secs: 10_000,
            portfolio_capital_usd: 1000.0,
        };
        let mut strategy = config_models::StrategyConfig::default();
        strategy.tae.risk.setup_gone_policy = "strict".into();
        let result = run_backtest(
            &pool,
            &params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            &strategy,
        )
        .await;
        assert_eq!(
            result.total_trades, 1,
            "strict posture closes the open position"
        );
        assert_eq!(result.trades[0].exit_reason, "setup_gone");
    }

    #[tokio::test]
    async fn recorded_replay_settles_direction_aware_funding() {
        let pool = seed_pool().await;
        // Entry accepts at 105 → fills at 94. Then a snapshot past the first
        // 8h boundary (28800) while the position is open → funding settles.
        // Then TP at 126 closes.
        for (ts, mid) in [
            (1000u64, 105.0f64),
            (1001, 94.0),
            (30_000, 94.0),
            (30_001, 126.0),
        ] {
            database_storage::insert_snapshot_internal(&pool, &long_snapshot(ts, mid)).await;
        }

        let params = BacktestParams {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            from_secs: 0,
            to_secs: 100_000,
            portfolio_capital_usd: 1000.0,
        };
        let result = run_backtest(
            &pool,
            &params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            &config_models::StrategyConfig::default(),
        )
        .await;

        assert_eq!(result.total_trades, 1);
        // The position crossed the 8h boundary → long pays the default
        // 0.01% rate (negative funding_fees, ≈ −0.01 on ~100 notional).
        let funding = result.trades[0].funding_fees;
        assert!(
            funding < 0.0,
            "long must pay funding when the replay crosses an 8h boundary, got {funding}"
        );
        assert!(
            (funding + 0.01).abs() < 0.005,
            "expected ~-0.01 funding on ~100 notional at 0.01%/8h, got {funding}"
        );
        // Cost columns populated on the trade.
        assert!(result.trades[0].commission_fees > 0.0);
        assert!(result.trades[0].slippage_bps >= 0.0);
    }
}
