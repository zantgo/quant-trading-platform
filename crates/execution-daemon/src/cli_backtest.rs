//! CLI backtest runner (v8.2) — the headless/in-terminal sibling of the
//! GUI Backtest Launcher.
//!
//! Two entry paths:
//! - **Headless** (`--backtest --exchange … --symbols …`): validates,
//!   auto-backfills missing archive coverage (progress on stderr), runs
//!   the multi-symbol historical simulation (progress on stderr), persists
//!   the result through the identical API write path, and prints one JSON
//!   envelope on stdout with exit code 0 (ok) / 1 (failure) / 130
//!   (cancelled). This is the E2E matrix harness hook.
//! - **Interactive** (the CLI launch prompt's Backtest choice): the same
//!   flow with stdin prompts and a terminal progress bar; Ctrl+C cancels
//!   cleanly.
//!
//! Timeframe contract (v8.2): every ladder slot must be one of the 14
//! standard dropdown tiers; slots below 60 s are rejected (exchange
//! history granularity — the archive floor).

use config_models::WorkspaceConfig;
use core_domain::normalized::Exchange;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The 14 standard timeframe tiers (mirrors `TIMEFRAME_OPTIONS` in the UI).
pub const TIMEFRAME_TIERS: [u64; 14] = [
    1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400,
];

/// Archive floor: exchange history is 1-minute granular.
pub const ARCHIVE_FLOOR_SECS: u64 = 60;

/// CLI backtest arguments (headless flags or interactive answers).
#[derive(Debug, Clone)]
pub struct CliBacktestArgs {
    pub exchange: String,
    pub symbols: Vec<String>,
    pub tf: Vec<u64>,
    pub depth_days: u32,
    /// v9 (F-07): the simulated account seed - the same
    /// `portfolio_capital_usd` dial as paper/live.
    pub portfolio_capital: f64,
    /// v9: the strategy bound to the run (default "default").
    pub strategy_name: Option<String>,
    pub allocation: f64,
}

impl Default for CliBacktestArgs {
    fn default() -> Self {
        Self {
            exchange: "Hyperliquid".to_string(),
            symbols: vec!["BTC".to_string()],
            tf: vec![60, 180, 300, 900],
            depth_days: 180,
            portfolio_capital: 1000.0,
            strategy_name: None,
            allocation: 10.0,
        }
    }
}

/// Parse the `--tf` argument ("60,180,300,900") — each slot must be a
/// standard tier; slots below the archive floor are rejected.
pub fn parse_tf(raw: &str) -> Result<Vec<u64>, String> {
    let parts: Vec<u64> = raw
        .split(',')
        .map(|p| p.trim().parse::<u64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("--tf '{raw}' is not a comma-separated list of seconds"))?;
    if parts.is_empty() || parts.len() > 4 {
        return Err(format!(
            "--tf must contain 1..4 ascending timeframes, got {}",
            parts.len()
        ));
    }
    for p in &parts {
        if !TIMEFRAME_TIERS.contains(p) {
            return Err(format!(
                "--tf value {p}s is not one of the 14 standard dropdown tiers"
            ));
        }
    }
    if parts.windows(2).any(|w| w[0] >= w[1]) {
        return Err("--tf timeframes must be strictly ascending".to_string());
    }
    if parts.iter().any(|p| *p < ARCHIVE_FLOOR_SECS) {
        return Err(format!(
            "timeframes below {ARCHIVE_FLOOR_SECS}s cannot be backfilled from exchange history"
        ));
    }
    Ok(parts)
}

/// Resolve the exchange name to the normalized exchange enum.
pub fn resolve_exchange(name: &str) -> Result<Exchange, String> {
    if name.eq_ignore_ascii_case("bitget") {
        Ok(Exchange::Bitget)
    } else if name.eq_ignore_ascii_case("hyperliquid") || name.eq_ignore_ascii_case("hl") {
        Ok(Exchange::Hyperliquid)
    } else {
        Err("exchange must be 'Hyperliquid' (hl) or 'Bitget'".to_string())
    }
}

/// The run outcome for the JSON envelope / exit code.
pub struct CliBacktestOutcome {
    pub status: &'static str, // "ok" | "cancelled" | "failed"
    pub backtest_id: Option<i64>,
    pub total_trades: u32,
    pub win_rate: f64,
    pub profit_factor: Option<f64>,
    pub max_drawdown_pct: f64,
    pub exit_reasons: Vec<(String, usize)>,
    pub error: Option<String>,
}

/// Validate the request against config (depth, ceiling, allocation).
fn validate(workspace: &WorkspaceConfig, args: &CliBacktestArgs) -> Result<Exchange, String> {
    let exchange = resolve_exchange(&args.exchange)?;
    if args.symbols.is_empty() {
        return Err("at least one symbol is required".to_string());
    }
    if args.symbols.len() > 100 {
        return Err("at most 100 instances per backtest".to_string());
    }
    if !(1..=365).contains(&args.depth_days) {
        return Err("--depth must be in 1..=365".to_string());
    }
    // Burn-in: the scored window must be positive (depth > warmup span).
    let burn_in_secs =
        workspace.backtest.warmup_bars as i64 * args.tf.iter().copied().max().unwrap_or(900) as i64;
    if (args.depth_days as i64) * 86400 <= burn_in_secs {
        return Err(format!(
            "--depth {}d is too small for the warm-up window (needs > {:.1}d)",
            args.depth_days,
            burn_in_secs as f64 / 86400.0
        ));
    }
    if !args.portfolio_capital.is_finite() || args.portfolio_capital <= 0.0 {
        return Err("--portfolio-capital must be a positive number".to_string());
    }
    if !args.allocation.is_finite() || !(1.0..=100.0).contains(&args.allocation) {
        return Err("--allocation must be in 1..=100".to_string());
    }
    let allocation_sum = args.allocation * args.symbols.len() as f64;
    if allocation_sum > 100.0 + 1e-9 {
        return Err(format!(
            "Σ allocations = {allocation_sum:.2}% across {} instances (must be <= 100%)",
            args.symbols.len()
        ));
    }
    // Per-TF depth ceilings: Hyperliquid's 5,000-candle endpoint window
    // and Bitget's per-granularity retention (measured) — hard clamp on
    // smallest TF (fewest days) — operator knows before run, never silent
    // truncate. Smallest TF rules because it needs most candles per day.
    let exchange_name = if exchange == Exchange::Bitget {
        "Bitget"
    } else {
        "Hyperliquid"
    };
    // Precompute smallest TF's ceiling for clear hard-clamp message
    let smallest_tf = args.tf.iter().copied().min().unwrap_or(60);
    let smallest_max = backtesting_engine::backfill::exchange_max_depth_secs(
        exchange_name,
        smallest_tf,
        &workspace.backtest,
    );
    for tf in &args.tf {
        let max_depth_secs = backtesting_engine::backfill::exchange_max_depth_secs(
            exchange_name,
            *tf,
            &workspace.backtest,
        );
        if max_depth_secs > 0 && (args.depth_days as i64) * 86400 > max_depth_secs {
            let limit = if exchange == Exchange::Bitget {
                format!(
                    "Bitget's {tf}s history (retention ≈ {} days)",
                    max_depth_secs / 86400
                )
            } else {
                format!(
                    "Hyperliquid's {}-candle ceiling for the {tf}s timeframe (max ≈ {} days)",
                    workspace.backtest.hyperliquid.max_candles_per_tf,
                    max_depth_secs / 86400
                )
            };
            let hint = if *tf == smallest_tf {
                format!(
                    " — smallest TF {}s limits all to {}d (hard clamp). Use coarser micro (e.g. 300s→{}d, 900s→{}d) or reduce --depth to {}d",
                    smallest_tf,
                    smallest_max / 86400,
                    backtesting_engine::backfill::exchange_max_depth_secs(exchange_name, 300, &workspace.backtest) / 86400,
                    backtesting_engine::backfill::exchange_max_depth_secs(exchange_name, 900, &workspace.backtest) / 86400,
                    smallest_max / 86400
                )
            } else {
                format!(
                    " — smallest TF {}s max {}d hard clamps entire ladder",
                    smallest_tf,
                    smallest_max / 86400
                )
            };
            return Err(format!(
                "--depth {}d exceeds {limit}{hint}",
                args.depth_days
            ));
        }
    }
    Ok(exchange)
}

/// Fetch missing archive coverage for every symbol × ladder TF (the same
/// paging machinery the API backfill uses). Progress on stderr.
async fn ensure_archive(
    pool: &SqlitePool,
    workspace: &WorkspaceConfig,
    exchange: Exchange,
    symbols: &[String],
    tf_ladder: &[u64],
    depth_days: u32,
) -> Result<(), String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("clock: {e}"))?
        .as_millis() as u64;
    let depth_ms = depth_days as u64 * 86400 * 1000;
    let from_ms = now_ms - depth_ms;
    let exchange_name = if exchange == Exchange::Bitget {
        "Bitget"
    } else {
        "Hyperliquid"
    };

    for symbol in symbols {
        for tf in tf_ladder {
            // Covered already? The archive must reach the window start
            // (an earliest-anchored check — partial coverage resumes).
            let earliest = database_storage::queries::archive::query_archive_earliest_secs(
                pool,
                exchange_name,
                symbol,
                *tf,
            )
            .await;
            let covered = earliest
                .map(|e| (e as u64) <= from_ms / 1000)
                .unwrap_or(false);
            if covered {
                continue;
            }
            let cfg = backtesting_engine::backfill::BackfillJobConfig {
                instance_id: format!("cli:{}:{}", exchange_name.to_lowercase(), symbol),
                exchange: exchange_name.to_string(),
                symbol: symbol.clone(),
                timeframes: vec![*tf],
                depth_days,
                backtest: workspace.backtest.clone(),
                fetcher: build_fetcher(exchange, symbol, *tf, exchange_name),
            };
            let progress = Arc::new(tokio::sync::Mutex::new(
                backtesting_engine::backfill::BackfillProgress::new(
                    0,
                    format!("cli:{}", symbol),
                    symbol.clone(),
                    exchange_name.to_string(),
                    depth_days,
                ),
            ));
            let cancel = Arc::new(AtomicBool::new(false));
            let progress_for_ui = progress.clone();
            let symbol_label = symbol.clone();
            let tf_label = *tf;
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    let p = progress_for_ui.lock().await.clone();
                    eprintln!(
                        "[fetching {} pages · {} candles] {symbol_label} {tf_label}s",
                        p.pages_fetched, p.candles_stored
                    );
                    if p.status != backtesting_engine::backfill::BackfillStatus::Running {
                        break;
                    }
                }
            });
            backtesting_engine::backfill::run_backfill(pool.clone(), cfg, progress.clone(), cancel)
                .await;
            // Loud failures: a ceiling rejection or a fetch error must stop
            // the run — never silently continue with truncated data.
            let p = progress.lock().await.clone();
            match p.status {
                backtesting_engine::backfill::BackfillStatus::Failed => {
                    return Err(format!(
                        "backfill failed for {symbol} {tf}s: {}",
                        p.error.unwrap_or_default()
                    ));
                }
                backtesting_engine::backfill::BackfillStatus::Cancelled => {
                    return Err(format!("backfill cancelled for {symbol} {tf}s"));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Exchange-native page fetcher (mirrors the API handler closures).
fn build_fetcher(
    exchange: Exchange,
    symbol: &str,
    _tf: u64,
    _exchange_name: &str,
) -> backtesting_engine::backfill::PageFetcher {
    let quote = if symbol.ends_with("USDT") {
        portfolio_supervisor::session::Currency::USDT
    } else {
        portfolio_supervisor::session::Currency::USDC
    };
    let base = symbol
        .rsplit_once('-')
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| symbol.to_string());
    let choice = if exchange == Exchange::Bitget {
        portfolio_supervisor::session::ExchangeChoice::Bitget
    } else {
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid
    };
    let raw = choice.raw_symbol(&base, &quote);
    let internal_symbol = symbol.to_string();
    if exchange == Exchange::Bitget {
        let product_type = if raw.ends_with("USDT") {
            "USDT-FUTURES"
        } else {
            "USDC-FUTURES"
        }
        .to_string();
        let rest_url = {
            let platform = config_models::load_platform().ok();
            platform
                .map(|p| p.bitget.rest_url())
                .unwrap_or_else(|| "https://api.bitget.com".to_string())
        };
        let internal = internal_symbol.clone();
        Arc::new(move |tf_secs, start_ms, end_ms| {
            let raw = raw.clone();
            let internal = internal.clone();
            let product_type = product_type.clone();
            let rest_url = rest_url.clone();
            Box::pin(async move {
                let interval =
                    network_adapters::adapters::bitget_rest::timeframe_secs_to_interval(tf_secs);
                network_adapters::adapters::bitget_rest::fetch_historical_candles_page(
                    &raw,
                    &internal,
                    &product_type,
                    interval,
                    start_ms,
                    end_ms,
                    200,
                    &rest_url,
                )
                .await
            })
        })
    } else {
        let rest_url = {
            let platform = config_models::load_platform().ok();
            platform
                .map(|p| p.hyperliquid.rest_url())
                .unwrap_or_else(|| "https://api.hyperliquid.xyz".to_string())
        };
        let internal = internal_symbol;
        Arc::new(move |tf_secs, start_ms, end_ms| {
            let raw = raw.clone();
            let internal = internal.clone();
            let rest_url = rest_url.clone();
            Box::pin(async move {
                let interval =
                    network_adapters::adapters::hyperliquid_rest::timeframe_secs_to_interval(
                        tf_secs,
                    );
                network_adapters::adapters::hyperliquid_rest::fetch_historical_candles(
                    &raw, &internal, interval, start_ms, end_ms, &rest_url,
                )
                .await
            })
        })
    }
}

/// Run the full CLI backtest: validate → backfill → replay → persist →
/// JSON envelope. `interactive` enables the Ctrl+C prompt line.
pub async fn run_cli_backtest(
    pool: &SqlitePool,
    workspace: &WorkspaceConfig,
    args: CliBacktestArgs,
) -> CliBacktestOutcome {
    let exchange = match validate(workspace, &args) {
        Ok(e) => e,
        Err(msg) => {
            return CliBacktestOutcome {
                status: "failed",
                backtest_id: None,
                total_trades: 0,
                win_rate: 0.0,
                profit_factor: None,
                max_drawdown_pct: 0.0,
                exit_reasons: Vec::new(),
                error: Some(msg),
            }
        }
    };

    // Internal symbols: "BTC" → "BTC-USDC" / "BTC-USDT" per exchange.
    let quote = if exchange == Exchange::Bitget {
        "-USDT"
    } else {
        "-USDC"
    };
    let symbols: Vec<String> = args
        .symbols
        .iter()
        .map(|s| {
            if s.contains('-') {
                s.clone()
            } else {
                format!("{s}{quote}")
            }
        })
        .collect();

    eprintln!(
        "🧪 CLI backtest: exchange={} symbols={} tf={:?} depth={}d capital={} allocation={}%",
        args.exchange,
        symbols.join(","),
        args.tf,
        args.depth_days,
        args.portfolio_capital,
        args.allocation,
    );

    if let Err(e) = ensure_archive(
        pool,
        workspace,
        exchange,
        &symbols,
        &args.tf,
        args.depth_days,
    )
    .await
    {
        return CliBacktestOutcome {
            status: "failed",
            backtest_id: None,
            total_trades: 0,
            win_rate: 0.0,
            profit_factor: None,
            max_drawdown_pct: 0.0,
            exit_reasons: Vec::new(),
            error: Some(e),
        };
    }

    // The window: [now − depth + burn_in, now].
    let burn_in_secs =
        workspace.backtest.warmup_bars as i64 * args.tf.iter().copied().max().unwrap_or(900) as i64;
    let to_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1_800_000_000);
    let from_secs = to_secs - (args.depth_days as i64 * 86400 - burn_in_secs);

    let params = backtesting_engine::recorded::BacktestParams {
        symbol: symbols.join(","),
        timeframe_secs: args.tf.iter().copied().min().unwrap_or(60),
        from_secs,
        to_secs,
        portfolio_capital_usd: args.portfolio_capital,
    };

    let active_set = market_analyzer::active_set::ActiveSet::from_config(
        &workspace.activation,
        None,
        workspace.config_version,
        workspace.liquidity.enabled,
    );
    let specs: Vec<backtesting_engine::historical::SymbolSpec> = symbols
        .iter()
        .map(|sym| {
            let mut tf_configs = std::collections::HashMap::new();
            for tf in &args.tf {
                tf_configs.insert(
                    *tf,
                    config_models::TimeframeConfig::new(*tf, workspace.indicators.clone()),
                );
            }
            backtesting_engine::historical::SymbolSpec {
                symbol: sym.clone(),
                ladder: args.tf.clone(),
                tf_configs,
                allocation_pct: Some(args.allocation),
            }
        })
        .collect();
    let run_cfg = backtesting_engine::historical::HistoricalRunConfig {
        symbols: specs,
        fib_config: workspace.fibonacci.clone(),
        active_set,
        exchange,
        warmup_bars: workspace.backtest.warmup_bars,
        max_equity_points: workspace.backtest.max_equity_points,
        safety: backtesting_engine::historical::SafetyParams {
            caution_threshold: workspace.safety.consecutive_loss_caution,
            dropout_threshold: workspace.safety.consecutive_loss_dropout,
            dropout_duration_hours: workspace.safety.dropout_duration_hours,
            drawdown_limit_pct: workspace.safety.drawdown_limit_pct,
            max_daily_drawdown_pct: workspace.safety.max_daily_drawdown_pct,
            systemic_risk_threshold: workspace.safety.systemic_risk_threshold,
        },
        // v9: the named strategy (CLI --strategy; default "default").
        strategy: workspace
            .resolve_strategy(args.strategy_name.as_deref().unwrap_or("default"))
            .unwrap_or_else(|e| {
                eprintln!("strategy resolution failed ({e}); using built-in default");
                config_models::StrategyConfig::default()
            }),
    };
    let fees = portfolio_supervisor::paper_trading::FeesConfig {
        maker_fee_pct: workspace.fees.maker_fee_pct,
        taker_fee_pct: workspace.fees.taker_fee_pct,
        funding_rate_8h: workspace.fees.funding_rate_8h,
        simulated_spread_pct: 0.01,
        // v10.1: the run's bound strategy execution dial (parity with
        // live/paper — same cost model).
        slippage_bps: run_cfg.strategy.tae.execution.slippage_bps,
    };
    // v9: the verdict bar comes from the run's bound strategy's `pae` section.
    let analytics = {
        let st = workspace
            .resolve_strategy(args.strategy_name.as_deref().unwrap_or("default"))
            .unwrap_or_default();
        performance_analytics::strategy_analytics::AnalyticsParams::from_strategy(&st.pae)
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let controls = backtesting_engine::historical::RunControls {
        progress: Some(Arc::new(move |p| {
            eprintln!("[{:<9} {:3.0}%] {}", p.phase.as_str(), p.pct, p.message);
        })),
        cancel: cancel.clone(),
    };

    // Ctrl+C cancels cleanly (the runner checks the flag between chunks).
    let run_future = backtesting_engine::historical::run_historical_backtest(
        pool,
        &params,
        &workspace.minimal_tae,
        &fees,
        workspace.leverage.cross_leverage,
        analytics,
        &run_cfg,
        &controls,
    );
    tokio::pin!(run_future);
    let result = tokio::select! {
        r = &mut run_future => r,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\n⚠️  cancel requested — aborting cleanly…");
            cancel.store(true, Ordering::SeqCst);
            run_future.await
        }
    };

    if result.cancelled {
        return CliBacktestOutcome {
            status: "cancelled",
            backtest_id: None,
            total_trades: 0,
            win_rate: 0.0,
            profit_factor: None,
            max_drawdown_pct: 0.0,
            exit_reasons: Vec::new(),
            error: None,
        };
    }

    // Persist through the identical API write path.
    let payload = api_gateway::handlers::analytics::BacktestRequest {
        symbol: params.symbol.clone(),
        timeframe_secs: params.timeframe_secs,
        from_ms: from_secs * 1000,
        to_ms: to_secs * 1000,
        portfolio_capital_usd: Some(args.portfolio_capital),
        instance_id: None,
        mode: "historical".to_string(),
        exchange: Some(args.exchange.clone()),
        symbols: None,
        strategy_id: args.strategy_name.clone(),
    };
    let input_bars_target: Option<(String, Vec<u64>)> = Some((symbols[0].clone(), args.tf.clone()));
    match api_gateway::handlers::analytics::persist_backtest_run(
        pool,
        workspace,
        0,
        "historical",
        &params,
        &payload,
        &result,
        None,
        input_bars_target.as_ref(),
    )
    .await
    {
        Ok(backtest_id) => {
            let mut exit_reasons: Vec<(String, usize)> = Vec::new();
            for t in &result.trades {
                match exit_reasons.iter_mut().find(|(r, _)| *r == t.exit_reason) {
                    Some((_, n)) => *n += 1,
                    None => exit_reasons.push((t.exit_reason.clone(), 1)),
                }
            }
            CliBacktestOutcome {
                status: "ok",
                backtest_id: Some(backtest_id),
                total_trades: result.total_trades,
                win_rate: result.win_rate,
                profit_factor: result.profit_factor,
                max_drawdown_pct: result.max_drawdown_pct,
                exit_reasons,
                error: None,
            }
        }
        Err(e) => CliBacktestOutcome {
            status: "failed",
            backtest_id: None,
            total_trades: 0,
            win_rate: 0.0,
            profit_factor: None,
            max_drawdown_pct: 0.0,
            exit_reasons: Vec::new(),
            error: Some(e),
        },
    }
}

/// The JSON envelope (headless stdout) + human summary.
pub fn print_outcome(outcome: &CliBacktestOutcome) -> i32 {
    if outcome.status == "failed" {
        let msg = outcome.error.clone().unwrap_or_default();
        eprintln!("❌ CLI backtest failed: {msg}");
        println!(
            "{}",
            serde_json::json!({
                "status": "failed",
                "error": msg,
            })
        );
        return 1;
    }
    if outcome.status == "cancelled" {
        println!("{}", serde_json::json!({ "status": "cancelled" }));
        return 130;
    }
    let exit_reasons: serde_json::Value = outcome
        .exit_reasons
        .iter()
        .map(|(r, n)| serde_json::json!({ r: n }))
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "run_id": outcome.backtest_id,
            "total_trades": outcome.total_trades,
            "win_rate": outcome.win_rate,
            "profit_factor": outcome.profit_factor,
            "max_drawdown_pct": outcome.max_drawdown_pct,
            "exit_reasons": exit_reasons,
        })
    );
    println!();
    println!("── Backtest complete ─────────────────────────────");
    println!("  run id        : {}", outcome.backtest_id.unwrap_or(0));
    println!("  trades        : {}", outcome.total_trades);
    println!("  win rate      : {:.2}%", outcome.win_rate);
    println!(
        "  profit factor : {}",
        outcome
            .profit_factor
            .map(|p| format!("{p:.2}"))
            .unwrap_or_else(|| "—".to_string())
    );
    println!("  max drawdown  : -{:.2}%", outcome.max_drawdown_pct);
    for (reason, n) in &outcome.exit_reasons {
        println!("  exit {reason:<20}: {n}");
    }
    0
}

/// Interactive prompts for the CLI launch flow's Backtest choice.
pub fn prompt_backtest_args(workspace: &WorkspaceConfig) -> CliBacktestArgs {
    let exchange = crate::prompt("Exchange (Hyperliquid/Bitget)", "Hyperliquid");
    let symbols_raw = crate::prompt("Symbols (comma-separated, e.g. BTC,ETH)", "BTC");
    let symbols: Vec<String> = symbols_raw
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    let tf_raw = crate::prompt(
        "Timeframe ladder (1..4 ascending seconds)",
        "60,180,300,900",
    );
    let tf = parse_tf(&tf_raw).unwrap_or_else(|e| {
        eprintln!("  ⚠️  {e} — using the default ladder");
        vec![60, 180, 300, 900]
    });
    let depth_days = crate::prompt(
        "Archive depth (1–365 days)",
        &workspace.backtest.archive_depth_days.to_string(),
    )
    .parse::<u32>()
    .unwrap_or(workspace.backtest.archive_depth_days);
    let capital = crate::prompt("Starting capital (USD)", "1000")
        .parse::<f64>()
        .unwrap_or(1000.0);
    let allocation = crate::prompt("Allocation % per instance (1–100)", "10")
        .parse::<f64>()
        .unwrap_or(10.0);

    CliBacktestArgs {
        exchange,
        symbols,
        tf,
        depth_days,
        portfolio_capital: capital,
        allocation,
        strategy_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tf_accepts_standard_tiers() {
        let tf = parse_tf("60,180,300,900").expect("default ladder");
        assert_eq!(tf, vec![60, 180, 300, 900]);
    }

    #[test]
    fn parse_tf_rejects_sub_minute_tiers() {
        let err = parse_tf("15,60,300,900").unwrap_err();
        assert!(err.contains("below 60s"), "{err}");
    }

    #[test]
    fn parse_tf_rejects_non_ascending_ladder() {
        let err = parse_tf("900,60,300,180").unwrap_err();
        assert!(err.contains("ascending"), "{err}");
    }

    #[test]
    fn parse_tf_rejects_non_tier_values() {
        let err = parse_tf("60,240,300,900").unwrap_err();
        assert!(err.contains("dropdown tiers"), "{err}");
    }

    #[test]
    fn parse_tf_accepts_one_to_four_slots() {
        assert_eq!(parse_tf("60").unwrap(), vec![60]);
        assert_eq!(parse_tf("60,180").unwrap(), vec![60, 180]);
        assert_eq!(parse_tf("60,180,300").unwrap(), vec![60, 180, 300]);
        assert_eq!(parse_tf("60,180,300,900").unwrap(), vec![60, 180, 300, 900]);
        assert!(parse_tf("").is_err());
        let err = parse_tf("60,180,300,900,1800").unwrap_err();
        assert!(err.contains("1..4"), "{err}");
    }

    #[test]
    fn resolve_exchange_aliases() {
        assert_eq!(resolve_exchange("bitget").unwrap(), Exchange::Bitget);
        assert_eq!(
            resolve_exchange("Hyperliquid").unwrap(),
            Exchange::Hyperliquid
        );
        assert_eq!(resolve_exchange("hl").unwrap(), Exchange::Hyperliquid);
        assert!(resolve_exchange("binance").is_err());
    }

    #[test]
    fn validate_allocation_sum_cap() {
        let ws = config_models::WorkspaceConfig::default();
        let mut args = CliBacktestArgs {
            exchange: "Bitget".into(),
            symbols: vec!["BTC".into(), "ETH".into()],
            tf: vec![60, 180, 300, 900],
            depth_days: 30,
            portfolio_capital: 1000.0,
            strategy_name: None,
            allocation: 60.0,
        };
        assert!(validate(&ws, &args).is_err(), "120% total must be rejected");
        args.allocation = 50.0;
        assert!(validate(&ws, &args).is_ok(), "100% total is allowed");
    }

    #[test]
    fn validate_hyperliquid_ceiling() {
        let ws = config_models::WorkspaceConfig::default();
        let args = CliBacktestArgs {
            exchange: "Hyperliquid".into(),
            symbols: vec!["BTC".into()],
            tf: vec![60, 180, 300, 900],
            depth_days: 30,
            portfolio_capital: 1000.0,
            strategy_name: None,
            allocation: 10.0,
        };
        let err = validate(&ws, &args).unwrap_err();
        assert!(err.contains("5000-candle ceiling"), "{err}");
    }
}

#[cfg(test)]
mod fetch_debug_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn debug_hl_fetch_rows() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let interval =
            network_adapters::adapters::hyperliquid_rest::timeframe_secs_to_interval(900);
        let rows = network_adapters::adapters::hyperliquid_rest::fetch_historical_candles(
            "BTC",
            "BTC-USDC",
            interval,
            now_ms - 10 * 86400 * 1000,
            now_ms,
            "https://api.hyperliquid.xyz/info",
        )
        .await;
        match rows {
            Ok(r) => {
                eprintln!("DEBUG rows: {}", r.len());
                if let Some(c) = r.first() {
                    eprintln!(
                        "DEBUG first: start={} dur={} close={:?}",
                        c.start_time_ms, c.duration_ms, c.close
                    );
                }
            }
            Err(e) => eprintln!("DEBUG err: {e}"),
        }
    }
}
