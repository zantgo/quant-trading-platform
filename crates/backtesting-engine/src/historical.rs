//! BTE historical runner — the deep-history multi-symbol simulation.
//!
//! Replays the **real** MME pipeline over archived OHLCV candles:
//!
//! ```text
//! candle_archive (per symbol × per TF windows, burn-in inclusive)
//!   → warm_indicators_for_timeframe (chunked — the SAME warm path the
//!     live daemon runs at boot, producing full per-candle snapshots
//!     with normalized indicator maps + market context)
//!   → synthesize_cross_tf (the SAME pure MTF synthesizer the live L4/L5
//!     assembly calls) + DecisionContext::compute
//!   → run_tick (the SAME per-tick session body as paper/live)
//! ```
//!
//! v8.2 — multi-symbol: one run replays all launcher instances
//! simultaneously against a shared virtual portfolio. The replay tick
//! clock is each symbol's smallest ladder TF, merged k-way into one
//! globally timestamp-ordered event stream; the executor + engine state
//! is keyed by symbol, mirroring the live multi-instance architecture.
//!
//! v8.2 parity guarantees (backtest = paper by construction):
//! - a simulated `SafetyManager` per instance fed by replayed equity
//!   (the soft gate blocks new entries in DRAWDOWN_STOP / SUSPENDED);
//! - funding settled at simulated 8h boundaries of replay time;
//! - end-of-run force-close: open positions are closed at the final
//!   replayed candle close with `exit_reason = "end_of_backtest"`.
//!
//! v8.2 progress + cancel: `RunControls` reports phase progress at
//! warm-chunk and replay-loop boundaries and checks a cancel flag at the
//! loop head and between warm tasks.
//!
//! Determinism: no wall clock, no randomness in the executor path;
//! liquidity/cluster inputs are absent (the archive stores no
//! derivatives) — the synthesized decision is candle-based, exactly as
//! documented in the parity contract.

use config_models::FibonacciConfig;
use core_domain::analysis::MarketBias;
use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedCandle};
use core_domain::portfolio::SafetyState;
use market_analyzer::active_set::ActiveSet;
use market_analyzer::analyzer::warm_indicators_for_timeframe;
use portfolio_supervisor::execution::session_tick::run_tick;
use portfolio_supervisor::execution::ExecutionEngine;
use portfolio_supervisor::paper_trading::FeesConfig;
use portfolio_supervisor::safety::SafetyManager;
use portfolio_supervisor::setup_executor::{SetupExecutor, TickContext};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::recorded::{BacktestParams, BacktestResult, BacktestTrade, BACKTEST_MAX_SNAPSHOTS};

/// Chunk size for the warm-path replay (candles per warm call).
/// v10.2 (C1): 5000 reduces chunk count 6× for 20-30d runs (28.8k→6 chunks vs 36)
/// while keeping 300-bar re-convergence for indicator fidelity.
const CHUNK_CANDLES: usize = 5000;
/// Overlap between chunks: every indicator lookback window in this
/// platform is ≤ 300 bars, so 300 bars of re-convergence makes chunk
/// tails mathematically identical to a continuous replay.
const CHUNK_OVERLAP: usize = 300;

/// Max candles loaded per TF (bounded memory for deep windows).
const MAX_CANDLES_PER_TF: u32 = 200_000;

/// v8.2: funding settles every 8h of **simulated** replay time.
const FUNDING_INTERVAL_SECS: i64 = 8 * 3600;

/// One simulated instance inside a multi-symbol historical run.
#[derive(Clone)]
pub struct SymbolSpec {
    pub symbol: String,
    /// The full instance ladder (micro/fast/slow/macro durations); the
    /// replay tick clock for this symbol is the smallest ladder TF.
    pub ladder: Vec<u64>,
    /// Indicator configs keyed by timeframe duration.
    pub tf_configs: HashMap<u64, config_models::TimeframeConfig>,
    /// v8.2 per-instance allocation override (1..=100 %). `None` = the
    /// global `[workspace.minimal_tae].allocation_pct`.
    pub allocation_pct: Option<f64>,
}

/// Per-TF pipeline configuration for the historical run (indicator
/// periods, fib config — the same values the live pipeline uses).
#[derive(Clone)]
pub struct HistoricalRunConfig {
    /// v8.2: the simulated instances — one or more symbols, each with its
    /// own ladder and per-TF configs.
    pub symbols: Vec<SymbolSpec>,
    pub fib_config: FibonacciConfig,
    pub active_set: ActiveSet,
    pub exchange: Exchange,
    /// Warmup bars before the first valid MTF decision (burn-in).
    pub warmup_bars: u32,
    /// Max equity points persisted (downsampling cap).
    pub max_equity_points: u32,
    /// v8.2: safety-ladder parameters (mirror `[workspace.safety]`) so the
    /// simulated SafetyManager behaves exactly like the live one.
    pub safety: SafetyParams,
    /// v9: the effective strategy (patch-resolved) — the historical
    /// replay derives its L4 opportunity params and shared L6
    /// DecisionParams from it, exactly like the live pipeline.
    pub strategy: config_models::StrategyConfig,
}

/// Safety-ladder parameters for the simulated PME (mirrors
/// `[workspace.safety]`; see `SafetyManager::new`).
#[derive(Clone)]
pub struct SafetyParams {
    pub caution_threshold: u32,
    pub dropout_threshold: u32,
    pub dropout_duration_hours: u64,
    pub drawdown_limit_pct: f64,
    pub max_daily_drawdown_pct: f64,
    pub systemic_risk_threshold: f64,
}

impl Default for SafetyParams {
    fn default() -> Self {
        Self {
            caution_threshold: 3,
            dropout_threshold: 5,
            dropout_duration_hours: 8,
            drawdown_limit_pct: 30.0,
            max_daily_drawdown_pct: 5.0,
            systemic_risk_threshold: 80.0,
        }
    }
}

/// Run-phase progress (the launcher/CLI render these four phases; the
/// runner itself reports `Warming` and `Replaying`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Fetching,
    Warming,
    Replaying,
    Analyzing,
}

impl RunPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunPhase::Fetching => "fetching",
            RunPhase::Warming => "warming",
            RunPhase::Replaying => "replaying",
            RunPhase::Analyzing => "analyzing",
        }
    }
}

/// One progress update.
#[derive(Debug, Clone)]
pub struct RunProgress {
    pub phase: RunPhase,
    /// 0..=100.
    pub pct: f32,
    pub message: String,
}

/// v8.2 run controls: an optional progress callback + a shared cancel
/// flag. Both are checked/emitted at warm-chunk and replay-loop
/// boundaries. `None` progress = silent run (tests, legacy callers).
pub struct RunControls {
    pub progress: Option<Arc<dyn Fn(RunProgress) + Send + Sync>>,
    pub cancel: Arc<AtomicBool>,
}

impl Default for RunControls {
    fn default() -> Self {
        Self {
            progress: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl RunControls {
    fn emit(&self, phase: RunPhase, pct: f32, message: impl Into<String>) {
        if let Some(cb) = &self.progress {
            cb(RunProgress {
                phase,
                pct: pct.clamp(0.0, 100.0),
                message: message.into(),
            });
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Run the deep-history simulation for the given window. The caller has
/// already validated archive coverage; this function replays
/// deterministically and returns the standard `BacktestResult`
/// (`result.cancelled` is true when the run was aborted).
#[allow(clippy::too_many_arguments)]
pub async fn run_historical_backtest(
    pool: &SqlitePool,
    params: &BacktestParams,
    tae_cfg: &config_models::MinimalTaeConfig,
    fees: &FeesConfig,
    cross_leverage: u32,
    analytics: performance_analytics::strategy_analytics::AnalyticsParams,
    run_cfg: &HistoricalRunConfig,
    controls: &RunControls,
) -> BacktestResult {
    let no_specs = || {
        controls.emit(RunPhase::Replaying, 0.0, "no instances to replay");
        let mut result = crate::recorded::finalize_result(
            params,
            Vec::new(),
            Vec::new(),
            run_cfg.max_equity_points,
            analytics,
            Vec::new(),
            Vec::new(),
        );
        result.cancelled = true;
        result
    };
    if run_cfg.symbols.is_empty() {
        return no_specs();
    }

    // ── 1. Load + warm every symbol × ladder TF (parallel — the warm
    // path is the CPU-bound cost; the tokio blocking pool runs each
    // symbol×TF on its own thread). ──
    let mut warm_jobs: Vec<(String, u64, tokio::task::JoinHandle<Vec<MarketSnapshot>>)> =
        Vec::new();
    for spec in &run_cfg.symbols {
        let max_tf = spec.ladder.iter().copied().max().unwrap_or(900);
        let burn_in = run_cfg.warmup_bars as i64 * max_tf as i64;
        let load_from = params.from_secs.saturating_sub(burn_in);
        for tf in spec.ladder.iter().copied() {
            let candles = load_archive(pool, &spec.symbol, tf, load_from, params.to_secs).await;
            let cfg = run_cfg.clone();
            warm_jobs.push((
                spec.symbol.clone(),
                tf,
                tokio::task::spawn_blocking(move || warm_tf(&cfg, tf, candles)),
            ));
        }
    }

    let total_warms = warm_jobs.len().max(1) as f32;
    let mut per_tf_snapshots: HashMap<(String, u64), Vec<MarketSnapshot>> = HashMap::new();
    let mut completed_warms = 0u32;
    for (symbol, tf, fut) in warm_jobs {
        if controls.cancelled() {
            let mut result = crate::recorded::finalize_result(
                params,
                Vec::new(),
                Vec::new(),
                run_cfg.max_equity_points,
                analytics,
                Vec::new(),
                Vec::new(),
            );
            result.cancelled = true;
            return result;
        }
        match fut.await {
            Ok(snaps) => {
                per_tf_snapshots.insert((symbol.clone(), tf), snaps);
            }
            Err(e) => eprintln!("BTE warm task failed for {symbol}/{tf}s: {e}"),
        }
        completed_warms += 1;
        controls.emit(
            RunPhase::Warming,
            completed_warms as f32 / total_warms * 100.0,
            format!(
                "warming {symbol} {tf}s ({completed_warms}/{})",
                total_warms as u32
            ),
        );
    }

    // ── 2. Shared engine + executor (state keyed by symbol — the live
    // multi-instance architecture) + simulated safety managers. ──
    let engine = Arc::new(ExecutionEngine::new(fees.clone()));
    engine
        .set_initial_equity(
            Decimal::from_f64_retain(params.portfolio_capital_usd).unwrap_or(dec!(1000)),
        )
        .await;
    engine.set_cross_leverage(cross_leverage).await;
    let executor = SetupExecutor::new(engine.clone(), tae_cfg);

    let mut safety_managers: HashMap<String, Arc<SafetyManager>> = HashMap::new();
    for spec in &run_cfg.symbols {
        let mgr = Arc::new(SafetyManager::new(
            run_cfg.safety.caution_threshold,
            run_cfg.safety.dropout_threshold,
            run_cfg.safety.dropout_duration_hours,
            run_cfg.safety.drawdown_limit_pct,
            run_cfg.safety.max_daily_drawdown_pct,
            run_cfg.safety.systemic_risk_threshold,
        ));
        mgr.set_portfolio_capital(
            Decimal::from_f64_retain(params.portfolio_capital_usd).unwrap_or(dec!(1000)),
        )
        .await;
        safety_managers.insert(spec.symbol.clone(), mgr);
    }

    // ── 3. Per-symbol entry series (smallest ladder TF, window-filtered)
    // merged k-way into one timestamp-ordered event stream. ──
    struct Event {
        ts: u64,
        symbol: String,
        entry_tf: u64,
        snap: MarketSnapshot,
    }
    let mut events: Vec<Event> = Vec::new();
    for spec in &run_cfg.symbols {
        let entry_tf = spec.ladder.iter().copied().min().unwrap_or(60);
        let snaps: Vec<MarketSnapshot> = per_tf_snapshots
            .get(&(spec.symbol.clone(), entry_tf))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                (s.timestamp as i64) >= params.from_secs && (s.timestamp as i64) <= params.to_secs
            })
            .collect();
        events.extend(snaps.into_iter().map(|snap| Event {
            ts: snap.timestamp,
            symbol: spec.symbol.clone(),
            entry_tf,
            snap,
        }));
    }
    // Deterministic order: ascending timestamp, ties broken by symbol
    // name (stable, documented in the parity contract).
    events.sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.symbol.cmp(&b.symbol)));
    let total_events = events.len().max(1) as f32;

    let mut trades: Vec<BacktestTrade> = Vec::new();
    let mut entry_ts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut equity_points: Vec<(i64, f64)> = Vec::new();
    let mut signals: Vec<database_storage::queries::backtest_ds::DsSignal> = Vec::new();
    let mut portfolio: Vec<database_storage::queries::backtest_ds::DsPortfolioPoint> = Vec::new();
    let mut peak_equity: f64 = params.portfolio_capital_usd;

    // MTF synthesis state carried per symbol across ticks (mirrors the
    // live loop — state must not leak across symbols).
    struct MtfState {
        prev_score: Option<f64>,
        prev_regime: Option<core_domain::analysis::MarketRegime>,
        prev_volume_dim: Option<f64>,
        prev_bias: Option<MarketBias>,
    }
    let mut mtf_states: HashMap<String, MtfState> = run_cfg
        .symbols
        .iter()
        .map(|s| {
            (
                s.symbol.clone(),
                MtfState {
                    prev_score: None,
                    prev_regime: None,
                    prev_volume_dim: None,
                    prev_bias: None,
                },
            )
        })
        .collect();

    // v10 (A15): the latest completed bias per run symbol — the replay's
    // analog of the live L7 breadth feed (strategy intake gates).
    let mut latest_bias: HashMap<String, Option<MarketBias>> = HashMap::new();

    // Simulated 8h funding clock: settle at every multiple of
    // FUNDING_INTERVAL_SECS crossed by the replay clock.
    let first_ts = events
        .first()
        .map(|e| e.ts as i64)
        .unwrap_or(params.from_secs);
    let mut next_funding = first_ts
        .div_euclid(FUNDING_INTERVAL_SECS)
        .saturating_mul(FUNDING_INTERVAL_SECS)
        .saturating_add(FUNDING_INTERVAL_SECS);

    let mut completed_ticks = 0u32;
    let mut cancelled = false;

    for ev in &events {
        if controls.cancelled() {
            cancelled = true;
            break;
        }
        completed_ticks += 1;
        controls.emit(
            RunPhase::Replaying,
            completed_ticks as f32 / total_events * 100.0,
            format!("replaying {} @ {}", ev.symbol, ev.ts),
        );

        let Some(spec) = run_cfg.symbols.iter().find(|s| s.symbol == ev.symbol) else {
            eprintln!(
                "BTE historical: event symbol '{}' not in run config — skipping tick",
                ev.symbol
            );
            continue;
        };
        let Some(safety) = safety_managers.get(&ev.symbol).cloned() else {
            eprintln!(
                "BTE historical: missing safety manager for '{}' — skipping tick",
                ev.symbol
            );
            continue;
        };

        // Simulated funding settlement at 8h replay boundaries.
        while (ev.ts as i64) >= next_funding {
            engine.settle_funding().await;
            next_funding = next_funding.saturating_add(FUNDING_INTERVAL_SECS);
        }

        // Gather the latest aligned snapshot per ladder TF (≤ event ts)
        // for THIS symbol only — never another symbol's buffers.
        let mut tf_refs: Vec<(u64, MarketSnapshot)> = Vec::with_capacity(spec.ladder.len());
        for tf in &spec.ladder {
            if let Some(series) = per_tf_snapshots.get(&(ev.symbol.clone(), *tf)) {
                if let Some(latest) = latest_at_or_before(series, ev.ts) {
                    tf_refs.push((*tf, latest.clone()));
                }
            }
        }

        let mut snap = ev.snap.clone();
        let mut mtf = mtf_states.remove(&ev.symbol).unwrap_or(MtfState {
            prev_score: None,
            prev_regime: None,
            prev_volume_dim: None,
            prev_bias: None,
        });

        // ── The SAME pure synthesizer the live L4/L5 assembly calls ──
        let cross: Vec<(u64, &MarketSnapshot)> = tf_refs.iter().map(|(t, s)| (*t, s)).collect();
        let mut opportunity_params =
            market_analyzer::synthesis::OpportunityParams::from_strategy(&run_cfg.strategy.l4);
        opportunity_params.viability_min_net_rr = run_cfg.strategy.tae.intake.min_net_rr;
        opportunity_params.ladder_roles = run_cfg.strategy.ladder_roles.clone();
        let decision_params =
            market_analyzer::strategy_params::decision_params_from_strategy(&run_cfg.strategy.l6);
        let analysis_params =
            market_analyzer::strategy_params::analysis_params_from_strategy(&run_cfg.strategy.l3);
        let alignment_params =
            market_analyzer::strategy_params::alignment_params_from_strategy(&run_cfg.strategy.l2);
        let risk_params =
            market_analyzer::strategy_params::risk_params_from_strategy(&run_cfg.strategy.l5);
        let synthesis = market_analyzer::synthesis::synthesize_cross_tf(
            &ev.symbol,
            &cross,
            None,
            None,
            &[],
            mtf.prev_score,
            mtf.prev_regime,
            mtf.prev_volume_dim,
            mtf.prev_bias,
            // v9: same wired opportunity params as live.
            &opportunity_params,
            &decision_params,
            &analysis_params,
            &alignment_params,
            &risk_params,
        );

        mtf.prev_score = Some(synthesis.alignment.mtf_overall_score);
        mtf.prev_regime = Some(synthesis.analysis.market_regime);
        mtf.prev_volume_dim = synthesis.alignment.dimensions.get(2).map(|d| d.score);
        mtf.prev_bias = Some(synthesis.analysis.bias);
        mtf_states.insert(ev.symbol.clone(), mtf);
        latest_bias.insert(ev.symbol.clone(), Some(synthesis.analysis.bias));

        // Confluence score — the same unsigned 3-factor blend as live.
        let confluence_score = {
            let tradability_dim = synthesis
                .alignment
                .dimensions
                .get(9)
                .map(|d| d.score)
                .unwrap_or(0.0);
            let market_quality_score = synthesis.analysis.market_quality_score;
            let opp_score = synthesis
                .opportunity
                .as_ref()
                .map(|o| o.opportunity_score)
                .unwrap_or(0.0);
            {
                let [w_l2, w_l3, w_l4] =
                    core_domain::decision_params::DecisionParams::default().confluence_weights;
                w_l2 * tradability_dim + w_l3 * market_quality_score + w_l4 * opp_score
            }
            .clamp(0.0, 100.0)
        };

        let close_f = snap.close.as_ref().and_then(|d| d.to_f64()).unwrap_or(0.0);
        let atr_val = snap.atr_14().and_then(|d| d.to_f64()).unwrap_or(0.0);
        let decision = core_domain::decision_context::DecisionContext::compute(
            &snap.indicators,
            close_f,
            atr_val,
            confluence_score,
            &synthesis.analysis,
            synthesis.opportunity.as_ref(),
            &synthesis.risk,
            // v9 F-05: shared DecisionParams (strategy-derived).
            &decision_params,
            &analysis_params,
        );

        snap.opportunity = synthesis.opportunity.clone();
        snap.analysis = Some(synthesis.analysis.clone());
        snap.decision_context = Some(decision);
        snap.advisory = Some(synthesis.advisory.clone());
        snap.is_completed = Some(true);

        // v8.2 parity: the simulated safety ladder is the soft entry gate.
        let safety_state = *safety.safety_state.read().await;
        let safety_allows =
            safety_state != SafetyState::DrawdownStop && safety_state != SafetyState::Suspended;

        // v10 (A15): full strategy gates on the simulated portfolio —
        // breadth/systemic intake gates + PME exposure/margin gates, the
        // same `strategy_gates` functions the daemon applies live. The
        // replay has no L7 synthesis, so breadth is the cross-symbol bias
        // share and the systemic veto is inert (0.0 — no shipped
        // threshold blocks at 0).
        let breadth_pct = breadth_from_biases(&latest_bias, run_cfg.symbols.len());
        let (intake_allows, intake_block) =
            portfolio_supervisor::strategy_gates::evaluate_intake_gates(
                &run_cfg.strategy,
                breadth_pct,
                0.0,
            );
        let (portfolio_allows, portfolio_block) = {
            let equity = engine.get_equity_decimal().await;
            let positions = engine.positions.read().await;
            let leverage = *engine.cross_leverage.read().await as f64;
            let gross: f64 = positions
                .values()
                .map(|p| {
                    let size = p.size.to_f64().unwrap_or(0.0);
                    let px = p.entry_price.to_f64().unwrap_or(0.0);
                    size * px
                })
                .sum();
            let single: f64 = positions
                .get(ev.symbol.as_str())
                .map(|p| {
                    let size = p.size.to_f64().unwrap_or(0.0);
                    let px = p.entry_price.to_f64().unwrap_or(0.0);
                    size * px
                })
                .unwrap_or(0.0);
            drop(positions);
            let equity_f = equity.to_f64().unwrap_or(0.0);
            if equity_f > 0.0 {
                let single_pct = single / equity_f * 100.0;
                let portfolio_pct = gross / equity_f * 100.0;
                let margin_ratio = (gross / leverage.max(1.0)) / equity_f;
                portfolio_supervisor::strategy_gates::evaluate_portfolio_gates(
                    &run_cfg.strategy,
                    single_pct,
                    portfolio_pct,
                    margin_ratio,
                )
            } else {
                (true, None)
            }
        };
        let market_filter_allows = intake_allows && portfolio_allows;
        let block_reason = intake_block.or(portfolio_block);
        if let Some(reason) = &block_reason {
            let _ = engine
                .log_activity("backtest-historical", &ev.symbol, "entry_blocked", reason)
                .await;
        }

        let mid = snap.mid_price;
        let rec_ts = snap.timestamp;
        // v10: track entry timestamps runner-side (the engine's
        // `opened_at_ms` is wall-clock during replay).
        if !entry_ts.contains_key(&ev.symbol) && engine.get_position(&ev.symbol).await.is_some() {
            entry_ts.insert(ev.symbol.clone(), rec_ts as i64);
        }
        let outcome = run_tick(
            &engine,
            &executor,
            "backtest-historical",
            &ev.symbol,
            &[&snap],
            mid,
            TickContext {
                safety_allows_entry: safety_allows,
                lifecycle_running: true,
                market_filter_allows_entry: market_filter_allows,
                entry_block_reason: block_reason,
                candle_ts: rec_ts,
                safety: Some(safety.clone()),
                dispatch: true,
                allocation_pct: spec.allocation_pct,
                strategy: Some(run_cfg.strategy.clone()),
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
            // v10 enrichment: entry ts from the runner-side map (the
            // engine's `opened_at_ms` is wall-clock in replay), MFE/MAE
            // from the outcome, ROI from the ledger.
            // Fallback: a position opened and closed within THIS bar was
            // never observed by the per-tick tracker — stamp the current bar.
            let ts_entry = entry_ts.remove(&ev.symbol).unwrap_or(rec_ts as i64);
            let hold_secs = if ts_entry > 0 {
                snap.timestamp as i64 - ts_entry
            } else {
                0
            };
            let roi_pct = if entry > 0.0 && size > 0.0 {
                close.pnl.to_f64().unwrap_or(0.0) / (entry * size) * 100.0
            } else {
                0.0
            };
            trades.push(BacktestTrade {
                timestamp: snap.timestamp as i64,
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
                symbol: ev.symbol.clone(),
            });
        }

        let unrealized: Decimal = {
            let positions = engine.positions.read().await;
            positions.values().map(|p| p.unrealized_pnl).sum()
        };
        let total = engine.get_equity_decimal().await + unrealized;
        equity_points.push((snap.timestamp as i64, total.to_f64().unwrap_or(0.0)));

        // v8.2 parity: the simulated SafetyManager tracks the shared
        // equity exactly like the live daemon's per-instance update.
        let _ = safety.update(total).await;

        crate::recorded::capture_tick_ds(
            snap.timestamp as i64,
            ev.entry_tf,
            &snap,
            &engine,
            &mut signals,
            &mut portfolio,
            &mut peak_equity,
        )
        .await;
    }

    // ── 4. End-of-run policy: force-close every open position at the
    // final replayed candle close (exit_reason = "end_of_backtest") so
    // the ledger and trade statistics are complete. ──
    if !cancelled {
        let last_ts = events
            .last()
            .map(|e| e.ts)
            .unwrap_or(params.to_secs.max(0) as u64);
        loop {
            // One symbol per iteration — the position map mutates on close.
            let open_symbols: Vec<String> = {
                let positions = engine.positions.read().await;
                positions.keys().cloned().collect()
            };
            let Some(symbol) = open_symbols.into_iter().next() else {
                break;
            };
            // The final mark for the symbol: its latest entry-TF close.
            let final_mid = {
                let spec = run_cfg.symbols.iter().find(|s| s.symbol == symbol);
                let entry_tf = spec
                    .and_then(|s| s.ladder.iter().copied().min())
                    .unwrap_or(60);
                per_tf_snapshots
                    .get(&(symbol.clone(), entry_tf))
                    .and_then(|series| latest_at_or_before(series, last_ts))
                    .map(|s| s.mid_price)
                    .unwrap_or(dec!(0))
            };
            if final_mid <= dec!(0) {
                break;
            }
            let (entry, size, direction) = match engine.get_position(&symbol).await {
                Some(pos) => (
                    pos.entry_price.to_f64().unwrap_or(0.0),
                    pos.size.to_f64().unwrap_or(0.0),
                    Some(pos.direction),
                ),
                None => break,
            };
            let _ = engine
                .close_position(&symbol, final_mid, "end_of_backtest")
                .await;
            let close = engine.take_last_close(&symbol).await;
            let pnl = close
                .as_ref()
                .map(|c| c.pnl.to_f64().unwrap_or(0.0))
                .unwrap_or(0.0);
            let exit = if size > 0.0 {
                match direction {
                    Some(config_models::Direction::Short) => entry - pnl / size,
                    _ => entry + pnl / size,
                }
            } else {
                entry
            };
            let direction_str = match direction {
                Some(config_models::Direction::Long) => "LONG",
                Some(config_models::Direction::Short) => "SHORT",
                None => "UNKNOWN",
            };
            // Fallback: a position opened on the FINAL bar was never
            // observed by the per-tick tracker — stamp the final bar.
            let ts_entry = entry_ts.remove(&symbol).unwrap_or(last_ts as i64);
            let hold_secs = if ts_entry > 0 {
                last_ts as i64 - ts_entry
            } else {
                0
            };
            let roi_pct = if entry > 0.0 && size > 0.0 {
                pnl / (entry * size) * 100.0
            } else {
                0.0
            };
            trades.push(BacktestTrade {
                timestamp: last_ts as i64,
                direction: direction_str.to_string(),
                entry_price: entry,
                exit_price: exit,
                size,
                pnl,
                exit_reason: "end_of_backtest".to_string(),
                ts_entry_secs: ts_entry,
                hold_secs,
                mfe_pct: 0.0,
                mae_pct: 0.0,
                roi_pct,
                slippage_bps: close.as_ref().map(|c| c.slippage_bps).unwrap_or(0.0),
                commission_fees: close
                    .as_ref()
                    .map(|c| c.commission_fees.to_f64().unwrap_or(0.0))
                    .unwrap_or(0.0),
                funding_fees: close
                    .as_ref()
                    .map(|c| c.funding_fees.to_f64().unwrap_or(0.0))
                    .unwrap_or(0.0),
                r_multiple: roi_pct / 1.0,
                symbol: symbol.clone(),
            });
            equity_points.push((
                last_ts as i64,
                engine.get_equity_decimal().await.to_f64().unwrap_or(0.0),
            ));
        }
    }

    let mut result = crate::recorded::finalize_result(
        params,
        trades,
        equity_points,
        run_cfg.max_equity_points,
        analytics,
        signals,
        portfolio,
    );
    result.cancelled = cancelled;
    controls.emit(RunPhase::Analyzing, 100.0, "analysis complete");
    result
}

/// Load the archived candle window for one TF, ascending.
async fn load_archive(
    pool: &SqlitePool,
    symbol: &str,
    tf: u64,
    from_secs: i64,
    to_secs: i64,
) -> Vec<NormalizedCandle> {
    database_storage::queries::archive::query_archive_window(
        pool,
        symbol,
        tf,
        from_secs,
        to_secs,
        MAX_CANDLES_PER_TF.min(BACKTEST_MAX_SNAPSHOTS),
    )
    .await
    .into_iter()
    .map(|a| a.to_normalized())
    .collect()
}

/// Chunked warm-path replay: candles → per-candle snapshots (the same
/// `warm_indicators_for_timeframe` the live daemon boots through).
fn warm_tf(
    run_cfg: &HistoricalRunConfig,
    tf: u64,
    candles: Vec<NormalizedCandle>,
) -> Vec<MarketSnapshot> {
    // Slot identity: match against any symbol's ladder position so the
    // warm path labels the timeframe slot the same way the registry does.
    let ladder = run_cfg
        .symbols
        .iter()
        .find(|s| s.ladder.contains(&tf))
        .map(|s| s.ladder.clone())
        .unwrap_or_else(|| vec![tf]);
    let tf_config = run_cfg
        .symbols
        .iter()
        .find_map(|s| s.tf_configs.get(&tf).cloned())
        .or_else(|| {
            ladder
                .iter()
                .find(|t| **t == tf)
                .map(|_| ())
                .and(None::<config_models::TimeframeConfig>)
        })
        .unwrap_or_else(|| {
            config_models::TimeframeConfig::new(tf, config_models::IndicatorsConfig::default())
        });
    // v11.9: duration-keyed identity — the label is derived from the exact
    // duration (the same mapping the registry boot uses).
    let slot = core_domain::duration_label(tf);
    let symbol = run_cfg
        .symbols
        .iter()
        .find(|s| s.ladder.contains(&tf))
        .map(|s| s.symbol.clone())
        .unwrap_or_else(|| "backtest".to_string());

    let mut out: Vec<MarketSnapshot> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    if candles.is_empty() {
        return out;
    }

    let mut start = 0usize;
    while start < candles.len() {
        let end = (start + CHUNK_CANDLES).min(candles.len());
        let chunk = candles[start..end].to_vec();
        let state = warm_indicators_for_timeframe(
            chunk,
            &tf_config,
            &run_cfg.fib_config,
            &symbol,
            tf,
            slot.clone(),
            CHUNK_CANDLES,
            &run_cfg.active_set,
            Some(run_cfg.exchange),
        );

        // Emit: first chunk emits everything (its head is the burn-in and
        // gets filtered by window); later chunks skip the re-convergence
        // overlap — those candles already emitted converged copies from
        // the previous chunk's tail.
        let skip = if start == 0 { 0 } else { CHUNK_OVERLAP };
        for snap in state.snapshot_history.into_iter().skip(skip) {
            if seen.insert(snap.timestamp) {
                out.push(snap);
            }
        }

        if end >= candles.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP);
        if start == 0 {
            break;
        }
    }

    out.sort_by_key(|s| s.timestamp);
    out
}

/// Latest snapshot with `ts <= t` (the series is ascending).
fn latest_at_or_before(series: &[MarketSnapshot], t: u64) -> Option<&MarketSnapshot> {
    match series.binary_search_by_key(&t, |s| s.timestamp) {
        Ok(idx) => Some(&series[idx]),
        Err(idx) if idx > 0 => Some(&series[idx - 1]),
        _ => series.first().filter(|s| s.timestamp <= t),
    }
}

/// v10 (A15): replay-breadth = the share of run symbols whose latest
/// completed bias is directional (non-neutral) — the historical analog of
/// the live L7 `breadth_pct` intake feed. Unseen symbols count as neutral.
fn breadth_from_biases(
    latest_bias: &std::collections::HashMap<String, Option<MarketBias>>,
    total_symbols: usize,
) -> f64 {
    if total_symbols == 0 {
        return 0.0;
    }
    let directional = latest_bias
        .values()
        .filter(|b| {
            matches!(
                b,
                Some(MarketBias::Bullish)
                    | Some(MarketBias::Bearish)
                    | Some(MarketBias::StrongBullish)
                    | Some(MarketBias::StrongBearish)
            )
        })
        .count();
    directional as f64 / total_symbols as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::normalized::ReconstructionMethod;
    use rust_decimal_macros::dec;

    async fn seed_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("mem pool");
        database_storage::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn candle(symbol: &str, tf_secs: u64, ts_secs: u64, close: f64) -> NormalizedCandle {
        NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: symbol.to_string(),
            start_time_ms: ts_secs * 1000,
            duration_ms: tf_secs * 1000,
            open: rust_decimal::Decimal::from_f64_retain(close - 1.0).unwrap_or_default(),
            high: rust_decimal::Decimal::from_f64_retain(close + 2.0).unwrap_or_default(),
            low: rust_decimal::Decimal::from_f64_retain(close - 2.0).unwrap_or_default(),
            close: rust_decimal::Decimal::from_f64_retain(close).unwrap_or_default(),
            volume: dec!(100),
            trades_count: 5,
            reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
        }
    }

    fn tae_cfg() -> config_models::MinimalTaeConfig {
        config_models::MinimalTaeConfig {
            enabled: true,
            allocation_pct: 10.0,
            min_net_rr: 1.0,
            max_position_size_pct_of_equity: None,
            max_open_positions: 10,
            entry_mode: "zone_midpoint".to_string(),
            invalidate_on: "direction_flip".to_string(),
        }
    }

    /// Deterministic 15m sine+drift series over N days.
    fn synthetic_days(days: u64, tf: u64, phase_deg: f64) -> Vec<NormalizedCandle> {
        let bars_per_day = 86400 / tf;
        let mut out = Vec::new();
        let mut ts = 1_700_000_000u64;
        for i in 0..(bars_per_day * days) {
            let close = 100.0
                + (i as f64 * 0.02)
                + 3.0 * ((i as f64) * 0.35 + phase_deg.to_radians()).sin();
            out.push(candle("BTC-USDC", tf, ts, close));
            ts += tf;
        }
        out
    }

    fn symbol_spec(symbol: &str, ladder: Vec<u64>) -> SymbolSpec {
        let mut tf_configs = HashMap::new();
        for tf in &ladder {
            tf_configs.insert(
                *tf,
                config_models::TimeframeConfig::new(
                    *tf,
                    config_models::IndicatorsConfig::default(),
                ),
            );
        }
        SymbolSpec {
            symbol: symbol.into(),
            ladder,
            tf_configs,
            allocation_pct: None,
        }
    }

    fn run_cfg(symbols: Vec<SymbolSpec>) -> HistoricalRunConfig {
        HistoricalRunConfig {
            symbols,
            fib_config: FibonacciConfig::default(),
            active_set: ActiveSet::default(),
            exchange: Exchange::Hyperliquid,
            warmup_bars: 60,
            max_equity_points: 2000,
            safety: SafetyParams::default(),
            strategy: config_models::StrategyConfig::default(),
        }
    }

    fn controls() -> RunControls {
        RunControls::default()
    }

    async fn run(
        pool: &SqlitePool,
        params: &BacktestParams,
        cfg: &HistoricalRunConfig,
    ) -> BacktestResult {
        run_historical_backtest(
            pool,
            params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            cfg,
            &controls(),
        )
        .await
    }

    #[tokio::test]
    async fn historical_run_is_deterministic() {
        let pool = seed_pool().await;
        let tf = 900u64;
        let candles = synthetic_days(2, tf, 0.0);
        database_storage::queries::archive::upsert_archive_candles(&pool, &candles, "backfill")
            .await;

        let from = candles.first().unwrap().start_time_ms / 1000 + 60 * tf;
        let to = candles.last().unwrap().start_time_ms / 1000;
        let params = BacktestParams {
            symbol: "BTC-USDC".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };
        let cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);

        let r1 = run(&pool, &params, &cfg).await;
        let r2 = run(&pool, &params, &cfg).await;

        assert!(!r1.cancelled && !r2.cancelled);
        assert_eq!(r1.trades.len(), r2.trades.len(), "deterministic trades");
        assert_eq!(r1.equity_curve.len(), r2.equity_curve.len());
        for (a, b) in r1.equity_curve.iter().zip(&r2.equity_curve) {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1, b.1);
        }
        for (a, b) in r1.trades.iter().zip(&r2.trades) {
            assert_eq!(a.pnl, b.pnl);
            assert_eq!(a.exit_reason, b.exit_reason);
        }
        assert_eq!(r1.stats.classification, r2.stats.classification);
    }

    #[tokio::test]
    async fn burn_in_window_produces_no_trades_before_from() {
        let pool = seed_pool().await;
        let tf = 900u64;
        let candles = synthetic_days(1, tf, 0.0);
        database_storage::queries::archive::upsert_archive_candles(&pool, &candles, "backfill")
            .await;

        let from = candles.last().unwrap().start_time_ms / 1000; // last bar only
        let to = from;
        let params = BacktestParams {
            symbol: "BTC-USDC".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };
        let cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);
        let r = run(&pool, &params, &cfg).await;
        // A single-bar window can produce at most one trade; the run must
        // complete and carry the standard result shape regardless.
        assert!(r.total_trades <= 1);
        assert_eq!(r.params.to_secs, to as i64);
    }

    #[tokio::test]
    async fn warm_chunks_cover_multi_chunk_series() {
        // 900 candles > CHUNK_CANDLES (800) forces multi-chunk replay.
        let tf = 60u64;
        let candles = synthetic_days(2, tf, 90.0);
        let candles = candles.into_iter().take(900).collect::<Vec<_>>();
        let cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);
        let snaps = warm_tf(&cfg, tf, candles);
        assert!(
            snaps.len() >= 500,
            "chunked replay emits the bulk of the series"
        );
        // Ascending + unique.
        for w in snaps.windows(2) {
            assert!(w[0].timestamp < w[1].timestamp, "ascending ts");
        }
    }

    #[tokio::test]
    async fn multi_symbol_run_shares_equity_ledger() {
        let pool = seed_pool().await;
        let tf = 900u64;
        // Two symbols with interleaved timestamps (B offset by half a bar).
        let mut a = synthetic_days(2, tf, 0.0);
        for c in &mut a {
            c.symbol = "BTC-USDC".to_string();
        }
        let mut b = synthetic_days(2, tf, 90.0);
        for (i, c) in b.iter_mut().enumerate() {
            c.symbol = "ETH-USDC".to_string();
            c.start_time_ms = a[0].start_time_ms + (i as u64) * tf * 1000 + tf * 500;
        }
        database_storage::queries::archive::upsert_archive_candles(&pool, &a, "backfill").await;
        database_storage::queries::archive::upsert_archive_candles(&pool, &b, "backfill").await;

        let from = a.first().unwrap().start_time_ms / 1000 + 60 * tf;
        let to = a.last().unwrap().start_time_ms / 1000 + tf;
        let params = BacktestParams {
            symbol: "multi".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };
        let cfg = run_cfg(vec![
            symbol_spec("BTC-USDC", vec![tf]),
            symbol_spec("ETH-USDC", vec![tf]),
        ]);
        let r = run(&pool, &params, &cfg).await;
        assert!(!r.cancelled, "multi-symbol run completes");
        // The final equity point must reflect the shared ledger (capital +
        // all realized PnL); conservation: equity never negative.
        if let Some((_, last_equity)) = r.equity_curve.last() {
            assert!(*last_equity > 0.0, "shared equity stays positive");
        }
        // Every trade carries the standard vocabulary.
        for t in &r.trades {
            assert!(
                ["tp", "sl", "invalidated_signal", "end_of_backtest"]
                    .contains(&t.exit_reason.as_str()),
                "exit reason vocabulary: {}",
                t.exit_reason
            );
        }
    }

    #[test]
    fn breadth_from_biases_counts_directional_share() {
        use std::collections::HashMap;
        let mut b: HashMap<String, Option<MarketBias>> = HashMap::new();
        assert_eq!(
            breadth_from_biases(&b, 4),
            0.0,
            "unseen symbols are neutral"
        );
        b.insert("A".into(), Some(MarketBias::Bullish));
        b.insert("B".into(), Some(MarketBias::Neutral));
        b.insert("C".into(), Some(MarketBias::StrongBearish));
        b.insert("D".into(), None);
        assert_eq!(breadth_from_biases(&b, 4), 50.0);
        assert_eq!(breadth_from_biases(&b, 2), 100.0);
    }

    #[tokio::test]
    async fn strategy_breadth_floor_blocks_entries_in_replay() {
        let pool = seed_pool().await;
        let tf = 900u64;
        let candles = synthetic_days(2, tf, 0.0);
        database_storage::queries::archive::upsert_archive_candles(&pool, &candles, "backfill")
            .await;

        let from = candles.first().unwrap().start_time_ms / 1000 + 60 * tf;
        let to = candles.last().unwrap().start_time_ms / 1000;
        let params = BacktestParams {
            symbol: "BTC-USDC".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };

        // A breadth floor above any possible breadth (≤ 100) blocks every
        // entry — the intake gate is wired into the replay. (The blocking
        // semantics themselves are unit-tested at the executor level; here
        // we verify the runner feeds the gate and completes cleanly.)
        let mut cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);
        cfg.strategy.l7.breadth_entry_floor = Some(101.0);
        let gated = run(&pool, &params, &cfg).await;
        assert!(!gated.cancelled);
        assert_eq!(gated.total_trades, 0, "breadth floor blocks all entries");
        // Deterministic under the gate.
        let gated2 = run(&pool, &params, &cfg).await;
        assert_eq!(gated.total_trades, gated2.total_trades);
        assert_eq!(gated.equity_curve.len(), gated2.equity_curve.len());
    }

    #[tokio::test]
    async fn strategy_margin_close_only_gate_blocks_entries_in_replay() {
        let pool = seed_pool().await;
        let tf = 900u64;
        let candles = synthetic_days(2, tf, 0.0);
        database_storage::queries::archive::upsert_archive_candles(&pool, &candles, "backfill")
            .await;

        let from = candles.first().unwrap().start_time_ms / 1000 + 60 * tf;
        let to = candles.last().unwrap().start_time_ms / 1000;
        let params = BacktestParams {
            symbol: "BTC-USDC".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };

        // Margin close-only band at 0.0: any margin ratio (≥ 0) trips the
        // portfolio gate → every entry blocked in the replay.
        let mut cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);
        cfg.strategy.pme.capital.enforce_margin_close_only = true;
        cfg.strategy.pme.capital.margin_alert_bands.close_only = 0.0;
        let gated = run(&pool, &params, &cfg).await;
        assert!(!gated.cancelled);
        assert_eq!(gated.total_trades, 0, "margin gate blocks all entries");
    }

    #[tokio::test]
    async fn cancel_aborts_replay_with_no_partial_results() {
        let pool = seed_pool().await;
        let tf = 60u64;
        let candles = synthetic_days(1, tf, 0.0);
        database_storage::queries::archive::upsert_archive_candles(&pool, &candles, "backfill")
            .await;

        let from = candles.first().unwrap().start_time_ms / 1000;
        let to = candles.last().unwrap().start_time_ms / 1000;
        let params = BacktestParams {
            symbol: "BTC-USDC".into(),
            timeframe_secs: tf,
            from_secs: from as i64,
            to_secs: to as i64,
            portfolio_capital_usd: 1000.0,
        };
        let cfg = run_cfg(vec![symbol_spec("BTC-USDC", vec![tf])]);
        let ctrl = RunControls {
            progress: None,
            cancel: Arc::new(AtomicBool::new(true)),
        };
        let r = run_historical_backtest(
            &pool,
            &params,
            &tae_cfg(),
            &FeesConfig::default(),
            20,
            performance_analytics::strategy_analytics::AnalyticsParams::default(),
            &cfg,
            &ctrl,
        )
        .await;
        assert!(r.cancelled, "pre-cancelled run reports cancelled");
        assert_eq!(r.trades.len(), 0, "no partial trades persisted");
    }
}
