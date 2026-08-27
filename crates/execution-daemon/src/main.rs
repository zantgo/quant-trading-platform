//! # Execution Daemon
//!
//! Headless orchestrator binary. Reads configuration, initializes the
//! SQLite database, builds the Axum `AppState`, spawns background tasks,
//! then runs the launch surface.
//!
//! ## Launch modes
//!
//! - `--mode web` (default): starts the Axum server + serves the Svelte
//!   dashboard. The Launch Setup wizard prompts the user to choose a mode
//!   (Observe / Simulate / Execute), exchange, currency, and instances
//!   before entering the workspace.
//! - `--mode cli`: interactive terminal launch (exchange, currency,
//!   instances with per-TF durations), then a live terminal monitor that
//!   redraws the L7 overview + per-instance rows. Supports
//!   `--trading observe|paper|live` (default observe); paper reuses the
//!   same `PaperSimulation` engine as `--mode web`. No HTTP server is bound;
//!   the SQLite telemetry DB is still used for analytics.
//!
//! ## CLI flags
//!
//! - `--exchange <hyperliquid|bitget>` — pre-fills the interactive prompt
//!   (both modes).
//! - `--currency <USDC|USDT>` — pre-fills the interactive prompt.
//! - `--config <path>` — path to `config.toml`. Overrides the
//!   `MARKET_MONITOR_CONFIG` env var.
//! - `--interval <secs>` — CLI monitor redraw interval (default 5).
//! - `--save` — CLI mode: enable snapshot-export JSON dumps.
//!
//! ## Config sharing workflow
//!
//! 1. GUI machine: `./manage.sh run`, configure settings + instances via
//!    the dashboard, click "Download Config" → `config.toml` saved.
//! 2. Headless machine: `scp config.toml ec2-user@host:/app/`
//! 3. Start: `cargo run --bin execution-daemon -- --mode cli --config config.toml`
//!
//! See `docs/conceptual-foundations/01-07-data-model-hierarchy.md` for the
//! canonical design document.

use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use api_gateway::{build_router, AppState};
use config_models::{load_platform, load_workspace, ClockMonitorBreachAction};
use core_domain::portfolio::SafetyState;
use database_storage::{init_db, run_telemetry_logger, verify_encryption_or_panic};
use network_adapters::{
    clock_monitor::{BreachAction, ClockMonitor, ClockMonitorConfig},
    connection_quality_tracker::ConnectionQualityRegistry,
    exchange_status_tracker::ExchangeStatusTracker,
    pipeline_reliability::ReliabilityTracker,
};
use performance_analytics::{performance_evaluator, strategy_optimizer};
use portfolio_supervisor::{
    portfolio_equity, registry,
    session::{Currency, ExchangeChoice},
    workspace_state::WorkspaceState,
};

// `snapshot_export` is owned by `lib.rs` so `api-gateway` can re-use
// its types without the daemon's CLI surface.

mod cli_backtest;
mod cli_compare;
mod cli_ds;
mod cli_ops;

// ─── CLI argument parsing ────────────────────────────────────────────

struct CliArgs {
    mode: LaunchMode,
    exchange: Option<String>,
    currency: Option<String>,
    config_path: Option<String>,
    /// CLI monitor redraw interval in seconds (default 5).
    interval_secs: u64,
    /// CLI mode: enable snapshot-export JSON dumps at boot.
    save_snapshots: bool,
    /// v8.2: run a headless backtest (E2E harness hook).
    backtest: bool,
    bt_symbols: Option<String>,
    bt_tf: Option<String>,
    bt_depth: Option<u32>,
    bt_capital: Option<f64>,
    bt_allocation: Option<f64>,
    /// v9: strategy bound to the backtest (default "default").
    bt_strategy: Option<String>,
    /// v10: headless data-science commands.
    sessions: bool,
    session_report: Option<i64>,
    backtest_show: Option<i64>,
    backtest_gates: Option<i64>,
    /// v9: headless strategy/account/instance ops.
    ops: Vec<cli_ops::StrategyOp>,
    account_ops: Vec<cli_ops::AccountOp>,
    lifecycle_op: Option<(String, String)>,
    instance_bind: Option<(String, String)>,
    /// v10.1: HTTP bind override — `--port`/`--bind` beat `PLATFORM_PORT`/
    /// `PLATFORM_BIND` env, which beat `[server]` in config.toml. Per-folder
    /// sessions run side by side on one machine via distinct ports.
    port: Option<u16>,
    bind: Option<String>,
    /// v10.1: cross-folder comparison — one or more folder roots whose
    /// `ds/` trees (backtests + sessions) are aggregated into one table.
    compare_folders: Vec<String>,
    /// v10.1: TAE activation for CLI-launched instances (`--tae-on`).
    /// Default OFF — the instance runs but the TAE does not activate
    /// unless explicitly specified.
    tae_on: bool,
    /// v10.2: CLI trading mode — `observe` (default), `paper`, or `live`.
    /// Controls `ExecutionMode` and `dispatch` for `--mode cli`.
    trading: Option<String>,
}

enum LaunchMode {
    Web,
    Cli,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut mode = LaunchMode::Web; // default
    let mut exchange = None;
    let mut currency = None;
    let mut config_path = None;
    let mut interval_secs = 5u64;
    let mut save_snapshots = false;
    let mut backtest = false;
    let mut bt_symbols = None;
    let mut bt_tf = None;
    let mut bt_depth = None;
    let mut bt_capital = None;
    let mut bt_allocation = None;
    let mut bt_strategy = None;
    let mut sessions = false;
    let mut session_report: Option<i64> = None;
    let mut backtest_show: Option<i64> = None;
    let mut backtest_gates: Option<i64> = None;
    let mut ops: Vec<cli_ops::StrategyOp> = Vec::new();
    let mut account_ops: Vec<cli_ops::AccountOp> = Vec::new();
    let mut lifecycle_op = None;
    let mut instance_bind = None;
    let mut port: Option<u16> = None;
    let mut bind: Option<String> = None;
    let mut compare_folders: Vec<String> = Vec::new();
    let mut tae_on = false;
    let mut trading: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                if i < args.len() {
                    mode = match args[i].as_str() {
                        "cli" | "monitor" => LaunchMode::Cli,
                        // Legacy modes retired in v7.2 — map to the new CLI
                        // terminal monitor with an explicit notice instead of
                        // silently booting the web server.
                        "headless" | "setup" => {
                            eprintln!(
                                "⚠️  `--mode {}` was retired. Use `--mode cli` (terminal monitor) or `--mode web` (dashboard).",
                                args[i]
                            );
                            LaunchMode::Cli
                        }
                        _ => LaunchMode::Web,
                    };
                }
            }
            "--exchange" => {
                i += 1;
                if i < args.len() {
                    exchange = Some(args[i].clone());
                }
            }
            "--currency" => {
                i += 1;
                if i < args.len() {
                    currency = Some(args[i].clone());
                }
            }
            "--config" => {
                i += 1;
                if i < args.len() {
                    config_path = Some(args[i].clone());
                }
            }
            "--interval" => {
                i += 1;
                if i < args.len() {
                    if let Ok(n) = args[i].parse::<u64>() {
                        interval_secs = n.max(1);
                    }
                }
            }
            "--save" => save_snapshots = true,
            "--backtest" => {
                backtest = true;
                mode = LaunchMode::Cli;
            }
            "--symbols" => {
                i += 1;
                if i < args.len() {
                    bt_symbols = Some(args[i].clone());
                }
            }
            "--tf" => {
                i += 1;
                if i < args.len() {
                    bt_tf = Some(args[i].clone());
                }
            }
            "--depth" => {
                i += 1;
                if i < args.len() {
                    bt_depth = args[i].parse::<u32>().ok();
                }
            }
            "--portfolio-capital" => {
                i += 1;
                if i < args.len() {
                    bt_capital = args[i].parse::<f64>().ok();
                }
            }
            "--strategy" => {
                i += 1;
                if i < args.len() {
                    bt_strategy = Some(args[i].clone());
                }
            }
            "--sessions" => sessions = true,
            "--session-report" => {
                i += 1;
                if i < args.len() {
                    session_report = args[i].parse::<i64>().ok();
                }
            }
            "--backtest-show" => {
                i += 1;
                if i < args.len() {
                    backtest_show = args[i].parse::<i64>().ok();
                }
            }
            "--backtest-gates" => {
                i += 1;
                if i < args.len() {
                    backtest_gates = args[i].parse::<i64>().ok();
                }
            }
            "--strategy-list" => ops.push(cli_ops::StrategyOp::List),
            "--strategy-export" => {
                i += 1;
                if i < args.len() {
                    let name = args[i].clone();
                    let path = if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                        i += 1;
                        Some(args[i].clone())
                    } else {
                        None
                    };
                    ops.push(cli_ops::StrategyOp::Export { name, path });
                }
            }
            "--strategy-create" | "--strategy-update" => {
                let is_create = args[i].starts_with("--strategy-create");
                i += 1;
                if i + 1 < args.len() {
                    let name = args[i].clone();
                    let path = args[i + 1].clone();
                    i += 1;
                    let _ = is_create;
                    ops.push(cli_ops::StrategyOp::Upsert { name, path });
                }
            }
            "--strategy-delete" => {
                i += 1;
                if i < args.len() {
                    ops.push(cli_ops::StrategyOp::Delete {
                        name: args[i].clone(),
                    });
                }
            }
            "--strategy-clone" => {
                i += 1;
                if i + 1 < args.len() {
                    ops.push(cli_ops::StrategyOp::Clone {
                        source: args[i].clone(),
                        target: args[i + 1].clone(),
                    });
                    i += 1;
                }
            }
            "--account-summary" => account_ops.push(cli_ops::AccountOp::Summary),
            "--account-set-capital" => {
                i += 1;
                if i < args.len() {
                    if let Ok(usd) = args[i].parse::<f64>() {
                        account_ops.push(cli_ops::AccountOp::SetCapital { usd });
                    }
                }
            }
            "--account-reset" => account_ops.push(cli_ops::AccountOp::Reset),
            "--instance-set-strategy" => {
                i += 1;
                if i + 1 < args.len() {
                    instance_bind = Some((args[i].clone(), args[i + 1].clone()));
                    i += 1;
                }
            }
            "--instance-start" | "--instance-pause" | "--instance-terminate" => {
                let action = args[i].trim_start_matches("--instance-").to_string();
                i += 1;
                if i < args.len() {
                    lifecycle_op = Some((args[i].clone(), action));
                }
            }
            "--allocation" => {
                i += 1;
                if i < args.len() {
                    bt_allocation = args[i].parse::<f64>().ok();
                }
            }
            "--web" | "--gui" => {
                mode = LaunchMode::Web;
            }
            "--port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse::<u16>().ok();
                    if port.is_none() {
                        eprintln!("⚠️  Invalid --port value '{}' — ignoring.", args[i]);
                    }
                }
            }
            "--bind" => {
                i += 1;
                if i < args.len() {
                    bind = Some(args[i].clone());
                }
            }
            "--tae-on" => {
                tae_on = true;
            }
            "--trading" => {
                i += 1;
                if i < args.len() {
                    let v = args[i].to_lowercase();
                    match v.as_str() {
                        "observe" | "paper" | "live" => trading = Some(v),
                        _ => eprintln!("⚠️  Invalid --trading value '{}' — expected observe|paper|live, ignoring.", args[i]),
                    }
                }
            }
            "--compare-folders" => {
                // Consume every following non-flag arg (shell glob expands
                // `experiments/*` into one arg per folder).
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    compare_folders.push(args[i].clone());
                    i += 1;
                }
                i -= 1;
            }
            _ => { /* ignore unknown args */ }
        }
        i += 1;
    }

    CliArgs {
        mode,
        exchange,
        currency,
        config_path,
        interval_secs,
        save_snapshots,
        backtest,
        bt_symbols,
        bt_tf,
        bt_depth,
        bt_capital,
        bt_allocation,
        bt_strategy,
        ops,
        account_ops,
        lifecycle_op,
        instance_bind,
        port,
        bind,
        compare_folders,
        sessions,
        session_report,
        backtest_show,
        backtest_gates,
        tae_on,
        trading,
    }
}

impl CliArgs {
    /// Resolve the exchange from CLI arg or workspace config default.
    fn resolve_exchange(&self, default_exchange: &str) -> ExchangeChoice {
        let raw = self.exchange.as_deref().unwrap_or(default_exchange);
        match raw.to_lowercase().as_str() {
            "bitget" => ExchangeChoice::Bitget,
            _ => ExchangeChoice::Hyperliquid,
        }
    }

    /// Resolve the currency from CLI arg or workspace config default.
    fn resolve_currency(&self, default_currency: &str) -> Currency {
        let raw = self.currency.as_deref().unwrap_or(default_currency);
        match raw.to_uppercase().as_str() {
            "USDT" => Currency::USDT,
            _ => Currency::USDC,
        }
    }
}

// ─── CLI launch flow (--mode cli) ─────────────────────────────────────
//
// Hand-rolled minimal stdin/stdout prompts. We deliberately avoid
// pulling in `inquire` or `dialoguer` for this single-purpose flow —
// the surface is small (text input + confirm) and staying dep-free
// keeps the binary slim. If more interactive flows are added later,
// consider migrating to `inquire`.
//
// The flow mirrors the GUI Launch Setup wizard (observe-only for now):
//   1. Exchange          — pre-filled from --exchange or workspace default.
//   2. Settlement currency — forced per exchange (HL=USDC, Bitget=USDT).
//   3. Instances         — one or more (base symbol + per-TF durations),
//                          pre-filled from existing workspace.instances[].
//   4. Confirm           — then the instances just run in the terminal
//                          monitor. No config.toml is rewritten by the
//                          prompts; created instances persist via the
//                          registry's normal save_workspace path.
//
// Paper/live parity is planned — `--mode cli` currently boots an
// observe session (no orders ever dispatched).

/// One prompt of an interactive CLI flow — printed to stdout, the
/// answer is read from stdin.
fn prompt(label: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{}", label);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    } else {
        print!("{} [{}]: ", label, default);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap_or_else(|e| {
        eprintln!("stdin read error: {}", e);
        0
    });
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Confirm prompt — returns `true` for `y`/`yes` (case-insensitive),
/// `false` for `n`/`no`/empty.
fn confirm(label: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{} {}: ", label, hint);
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    let trimmed = buf.trim().to_lowercase();
    match trimmed.as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    }
}

const TIMEFRAME_FLOOR_SECS: u64 = 1;
const TIMEFRAME_CEIL_SECS: u64 = 86_400;

fn prompt_timeframe_secs(label: &str, default_secs: u64) -> u64 {
    loop {
        let raw = prompt(label, &default_secs.to_string());
        match raw.parse::<u64>() {
            Ok(n) if (TIMEFRAME_FLOOR_SECS..=TIMEFRAME_CEIL_SECS).contains(&n) => return n,
            Ok(n) => {
                eprintln!(
                    "  ⚠️  {}s is outside the allowed range [{}, {}].",
                    n, TIMEFRAME_FLOOR_SECS, TIMEFRAME_CEIL_SECS
                );
            }
            Err(_) => {
                eprintln!("  ⚠️  '{}' is not a number.", raw);
            }
        }
    }
}

/// One instance configured in the CLI launch prompt.
struct CliInstance {
    base: String,
    micro: u64,
    fast: u64,
    slow: Option<u64>,
    r#macro: Option<u64>,
}

/// The full CLI launch plan: exchange + currency + instances.
struct CliLaunchPlan {
    exchange: String,
    currency: String,
    instances: Vec<CliInstance>,
    /// v10.1: TAE activation requested at launch (default OFF — the
    /// instance runs but the TAE is not activated unless specified).
    tae_on: bool,
    /// v10.2: trading mode for CLI (observe|paper|live). Controls
    /// ExecutionMode and session.
    trading: String,
}

/// v7.2 parity: the default timeframe ladder is the registry's ladder
/// (`registry::add_instance` fallback): micro 60s, fast 180s, slow/macro
/// from the workspace defaults — one canonical source for CLI, GUI, and
/// registry.
fn tf_default(slot: &str, workspace: &config_models::WorkspaceConfig) -> u64 {
    let (micro, fast, slow, r#macro) = workspace.tf_ladder_defaults();
    match slot {
        "micro" => micro,
        "fast" => fast,
        "slow" => slow,
        "macro" => r#macro,
        _ => slow,
    }
}

fn tf_label(secs: u64) -> String {
    if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

/// Collect the launch configuration from the operator. Exchange/currency
/// pre-fill from CLI args or workspace defaults; the instance list
/// pre-seeds from `workspace.instances[]` when present. Mirrors the GUI
/// wizard's Review step: the summary is printed, then the operator
/// confirms before anything starts. Returns `None` when the operator
/// declines to start.
fn cli_launch_plan(
    cli: &CliArgs,
    workspace: &config_models::WorkspaceConfig,
) -> Option<CliLaunchPlan> {
    println!("╔════════════════════════════════════════════╗");
    println!("║  Trading Platform — CLI Launch Setup       ║");
    println!("╚════════════════════════════════════════════╝");
    println!();
    // Resolve trading mode: CLI flag beats config default, then observe.
    let cli_trading = cli
        .trading
        .clone()
        .unwrap_or_else(|| "observe".to_string())
        .to_lowercase();
    let trading_label = match cli_trading.as_str() {
        "paper" => "paper (simulated orders, same engine as live)",
        "live" => "live (real orders — requires keys)",
        _ => "observe (monitoring only — no orders dispatched)",
    };
    println!(
        "Mode: {} — Press <Enter> to accept each default.",
        trading_label
    );
    println!();

    // 1. Exchange
    let default_exchange = cli
        .exchange
        .clone()
        .unwrap_or_else(|| workspace.default_exchange.clone());
    let exchange = {
        loop {
            let raw = prompt("Exchange (hyperliquid / bitget)", &default_exchange);
            match raw.to_lowercase().as_str() {
                "hyperliquid" | "hl" => break "hyperliquid".to_string(),
                "bitget" | "bg" => break "bitget".to_string(),
                _ => eprintln!("  ⚠️  Expected 'hyperliquid' or 'bitget'."),
            }
        }
    };
    let currency = if exchange.eq_ignore_ascii_case("bitget") {
        "USDT".to_string()
    } else {
        "USDC".to_string()
    };
    println!("  → Settlement currency forced to {}", currency);

    // 2. Instances — pre-seed from workspace.instances[].
    let mut instances: Vec<CliInstance> = workspace
        .instances
        .iter()
        .filter(|e| !e.symbol.is_empty() && e.status == config_models::InstanceStatus::Running)
        .filter_map(|e| {
            let base = e
                .symbol
                .split_once('-')
                .map(|(b, _)| b.to_string())
                .unwrap_or_default();
            if base.is_empty() {
                return None;
            }
            Some(CliInstance {
                base,
                micro: e.micro_term.candles.duration_seconds,
                fast: e.fast_term.candles.duration_seconds,
                slow: e.slow_term.as_ref().map(|t| t.candles.duration_seconds),
                r#macro: e.macro_term.as_ref().map(|t| t.candles.duration_seconds),
            })
        })
        .collect();

    println!(
        "\nInstances (blank base = finish). {} configured in config.toml — press Enter to keep.",
        instances.len()
    );
    loop {
        let default_base = if instances.is_empty() {
            "BTC".to_string()
        } else {
            String::new()
        };
        let raw = prompt(
            &format!(
                "Instance #{} base symbol (blank = done)",
                instances.len() + 1
            ),
            &default_base,
        );
        let cleaned = raw.trim().to_uppercase();
        if cleaned.is_empty() {
            break;
        }
        if cleaned.len() > 10 || !cleaned.chars().all(|c| c.is_ascii_alphanumeric()) {
            eprintln!("  ⚠️  Symbol must be 1-10 alphanumeric characters.");
            continue;
        }
        if instances.iter().any(|i| i.base == cleaned) {
            eprintln!("  ⚠️  {} is already in the list.", cleaned);
            continue;
        }
        let micro = prompt_timeframe_secs(
            &format!(
                "  micro timeframe_secs (default {}s)",
                tf_default("micro", workspace)
            ),
            tf_default("micro", workspace),
        );
        let fast = loop {
            let v = prompt_timeframe_secs(
                &format!(
                    "  fast timeframe_secs (default {}s)",
                    tf_default("fast", workspace)
                ),
                tf_default("fast", workspace),
            );
            if v == micro {
                eprintln!(
                    "  ⚠️  fast must differ from micro ({}s) — pick a distinct duration.",
                    micro
                );
                continue;
            }
            break v;
        };
        let slow = if confirm("  Enable slow timeframe? Y/n", true) {
            loop {
                let v = prompt_timeframe_secs(
                    &format!(
                        "  slow timeframe_secs (default {}s)",
                        tf_default("slow", workspace)
                    ),
                    tf_default("slow", workspace),
                );
                if v == micro || v == fast {
                    eprintln!(
                        "  ⚠️  slow must be distinct from micro ({}s) and fast ({}s).",
                        micro, fast
                    );
                    continue;
                }
                break Some(v);
            }
        } else {
            None
        };
        let r#macro = if confirm("  Enable macro timeframe? Y/n", true) {
            loop {
                let v = prompt_timeframe_secs(
                    &format!(
                        "  macro timeframe_secs (default {}s)",
                        tf_default("macro", workspace)
                    ),
                    tf_default("macro", workspace),
                );
                let mut seen = vec![micro, fast];
                if let Some(s) = slow {
                    seen.push(s);
                }
                if seen.contains(&v) {
                    eprintln!(
                        "  ⚠️  macro must be distinct from {} — pick another.",
                        seen.iter()
                            .map(|s| format!("{}s", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    continue;
                }
                break Some(v);
            }
        } else {
            None
        };
        // Validate 2–4 distinct: micro+fast always, slow/macro optional
        let active_count = 2 + slow.is_some() as usize + r#macro.is_some() as usize;
        if active_count < 2 {
            eprintln!("  ⚠️  At least 2 timeframes required (micro+fast).");
            continue;
        }
        instances.push(CliInstance {
            base: cleaned,
            micro,
            fast,
            slow,
            r#macro,
        });
    }

    if instances.is_empty() {
        println!(
            "\n⚠️  No instances configured — the terminal monitor will show an empty workspace."
        );
        println!("   Add instances later from the dashboard, or restart with `--mode cli`.");
    }

    // 3. TAE activation — must be specified explicitly (default OFF).
    let tae_on = cli.tae_on || confirm("Activate TAE (trade automation)? y/N", false);

    // 4. Summary + confirm
    println!("\n──────────────────────────────────────────────");
    println!("Trading Platform — CLI Launch Summary");
    println!("──────────────────────────────────────────────");
    println!(
        "  Mode                 : {} ({})",
        cli_trading, trading_label
    );
    println!(
        "  TAE                  : {}",
        if tae_on { "ON" } else { "OFF" }
    );
    println!("  Exchange             : {}", exchange);
    println!("  Settlement currency  : {}", currency);
    for inst in &instances {
        let slow_s = inst.slow.map(tf_label).unwrap_or_else(|| "—".to_string());
        let macro_s = inst
            .r#macro
            .map(tf_label)
            .unwrap_or_else(|| "—".to_string());
        println!(
            "  Instance             : {}-{} — micro {} · fast {} · slow {} · macro {} ({}, {} TFs)",
            inst.base,
            currency,
            tf_label(inst.micro),
            tf_label(inst.fast),
            slow_s,
            macro_s,
            cli_trading,
            2 + inst.slow.is_some() as usize + inst.r#macro.is_some() as usize,
        );
    }
    println!("──────────────────────────────────────────────\n");

    if !confirm("Start the monitor now?", true) {
        println!("Aborted — nothing was started and config.toml was not modified.");
        return None;
    }

    Some(CliLaunchPlan {
        exchange,
        currency,
        instances,
        tae_on,
        trading: cli_trading,
    })
}

/// Spawn one CLI instance with retry + backoff (same policy as the boot
/// auto-spawn loop). The instance is created through the registry so it
/// persists into config.toml via `save_workspace` and carries the
/// observe session default.
async fn spawn_cli_instance(
    ctx: &portfolio_supervisor::registry_context::RegistryContext,
    base: &str,
    quote: &str,
) {
    let mut attempt = 0u32;
    loop {
        match portfolio_supervisor::registry::add_instance(
            ctx,
            (base.to_string(), quote.to_string()),
        )
        .await
        {
            Ok(_inst) => {
                println!("✅ Instance spawned: {}-{}", base, quote);
                return;
            }
            Err(e) => {
                attempt += 1;
                if attempt >= 20 {
                    eprintln!(
                        "⚠️  Failed to spawn instance {}-{} after {} attempts: {} — retry on next restart",
                        base, quote, attempt, e
                    );
                    return;
                }
                eprintln!(
                    "⚠️  Failed to spawn instance {}-{} (attempt {}): {} — retrying in 30 s",
                    base, quote, attempt, e
                );
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    }
}
// ─── Main ────────────────────────────────────────────────────────────

/// v6.10.19a: the canonical per-pair risk feeding the L7 risk mean is the
/// MICRO window's L5 score — the same number the Risk panel, the dashboard
/// KPI, and the asset rows show. The previous TF-window MEAN drifted
/// upward whenever the macro window scored high (MAX_COMPRESSION / thin
/// participation) and rendered "HIGH_RISK" environments next to a visible
/// avg-risk of 41–46. Falls back to the window mean during warmup (micro
/// risk matrix not present yet), then 50 (moderate).
fn canonical_overall_risk(micro_risk: Option<f64>, risk_count: u32, risk_sum: f64) -> f64 {
    micro_risk.unwrap_or_else(|| {
        if risk_count > 0 {
            risk_sum / risk_count as f64
        } else {
            50.0
        }
    })
}

#[tokio::main]
async fn main() {
    let cli = parse_args();

    // If --config is provided, set the env var that config-models reads.
    if let Some(ref path) = cli.config_path {
        std::env::set_var("MARKET_MONITOR_CONFIG", path);
    }

    let _ = rustls::crypto::ring::default_provider().install_default();

    println!("⚙️  Trading Platform: Loading Master Configuration...");

    // AUDIT-V7-300 (08-08 §CB-01): one-shot startup warning for stale
    // legacy `analysis_limit` keys — the canonical number is
    // `candle_buffer.size` and stale keys are silently ignored.
    if let Ok(raw_toml) = std::fs::read_to_string("config.toml") {
        if config_models::detect_legacy_analysis_limit_keys(&raw_toml) {
            eprintln!(
                "⚠️  config.toml contains legacy `analysis_limit` key(s) — \
                 ignored; the canonical candle-buffer number is `[candle_buffer] size` \
                 (see docs/operations-and-compliance/08-08-candle-buffer-spec.md)"
            );
        }
    }

    let platform = match load_platform() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "❌ Configuration Error: failed to parse platform config from config.toml: {e}"
            );
            eprintln!("   Hint: copy config.default.toml → config.toml and retry, or fix the TOML syntax.");
            std::process::exit(1);
        }
    };
    let workspace = match load_workspace() {
        Ok(w) => w,
        Err(e) => {
            eprintln!(
                "❌ Configuration Error: failed to parse workspace config from config.toml: {e}"
            );
            eprintln!("   Hint: copy config.default.toml → config.toml and retry, or fix the TOML syntax.");
            std::process::exit(1);
        }
    };

    // v10.1: resolve the HTTP bind/port — CLI flag → env → [server] config
    // → defaults. Per-folder sessions run side by side on one machine via
    // distinct ports (each folder carries its own config.toml + telemetry.db).
    let server_bind = cli.bind.clone().unwrap_or_else(|| {
        std::env::var("PLATFORM_BIND")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| platform.server.bind.clone())
    });
    let server_port = cli.port.unwrap_or_else(|| {
        std::env::var("PLATFORM_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(platform.server.port)
    });
    // K1 single-operator: final bind must be loopback even when supplied via
    // --bind / PLATFORM_BIND env. Fail fast before binding the socket.
    {
        let bind_trim = server_bind.trim();
        const ALLOWED_BINDS: &[&str] = &["127.0.0.1", "::1", "localhost"];
        if !ALLOWED_BINDS.contains(&bind_trim) {
            eprintln!(
                "❌ Configuration Error: --bind / PLATFORM_BIND = '{bind_trim}' is not loopback — only {} allowed (single-operator local deployment; use ssh -L tunnel for remote access).",
                ALLOWED_BINDS.join(", ")
            );
            std::process::exit(1);
        }
    }
    let allowed_origins = api_gateway::default_allowed_origins(&server_bind, server_port);
    println!(
        "✅ Configuration Loaded: platform + workspace ({} instance{})",
        workspace.instances.len(),
        if workspace.instances.len() == 1 {
            ""
        } else {
            "s"
        }
    );

    match cli.mode {
        LaunchMode::Cli => println!("🖥️  Launch mode: CLI (terminal monitor, observe-only)"),
        LaunchMode::Web => {
            println!(
                "🖥️  Launch mode: WEB (Launch Setup wizard will prompt for mode/exchange/currency)"
            )
        }
    }

    println!("🗄️  Initializing local SQLite telemetry database...");
    let db_pool = match init_db().await {
        Ok(pool) => {
            println!(
                "✅ Database Setup: Connected to local telemetry.db file and verified schema."
            );
            pool
        }
        Err(e) => {
            eprintln!("❌ Database Setup: {e}");
            eprintln!("   Hint: check file permissions for ./telemetry.db and disk space, or remove a corrupted db and restart.");
            std::process::exit(1);
        }
    };

    // ── v10: headless data-science commands (read-only, then exit) ──
    if !cli.compare_folders.is_empty() {
        std::process::exit(cli_compare::compare_folders(&cli.compare_folders));
    }
    if cli.sessions {
        std::process::exit(cli_ds::print_sessions(&db_pool).await);
    }
    if let Some(id) = cli.session_report {
        std::process::exit(cli_ds::print_session_report(&db_pool, id).await);
    }
    if let Some(id) = cli.backtest_show {
        std::process::exit(cli_ds::print_backtest_show(&db_pool, &workspace, id).await);
    }
    if let Some(id) = cli.backtest_gates {
        std::process::exit(cli_ds::print_backtest_gates(&db_pool, id).await);
    }

    // ── v10 session identity: one monotonic session number per boot ──
    // Mode resolution: web sessions carry their default from
    // `[workspace.session]`; CLI launches use --trading flag (default observe).
    let session_mode = if matches!(cli.mode, LaunchMode::Cli) {
        cli.trading
            .clone()
            .unwrap_or_else(|| "observe".to_string())
            .to_lowercase()
    } else {
        // Web boot: the first instance's persisted mode (Observe/Paper/
        // Live) describes the session; the Launch Setup wizard persists it
        // before instances spawn.
        match workspace.instances.first().map(|i| &i.mode) {
            Some(config_models::ExecutionMode::Observe) => "observe".to_string(),
            Some(config_models::ExecutionMode::Live) => "live".to_string(),
            _ => "paper".to_string(),
        }
    };
    let session_started_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let config_snapshot = serde_json::to_string(&workspace).ok();
    let session_number = database_storage::queries::sessions::create_session(
        &db_pool,
        &session_mode,
        Some(workspace.default_exchange.as_str()),
        Some(&workspace.default_currency),
        Some(workspace.portfolio_capital_usd),
        session_started_ms,
        config_snapshot.as_deref(),
    )
    .await
    .ok();
    let session_id_arc: Arc<RwLock<Option<i64>>> = Arc::new(RwLock::new(session_number));
    println!(
        "🧪 Session identity: {}",
        session_number
            .map(|n| format!("SESSION #{:04}", n))
            .unwrap_or_else(|| "SESSION unavailable".to_string())
    );

    // ── v8.2: headless CLI backtest (no engine boot, no web server) ──
    if cli.backtest {
        let args = cli_backtest::CliBacktestArgs {
            exchange: cli
                .exchange
                .clone()
                .unwrap_or_else(|| workspace.default_exchange.clone()),
            symbols: cli
                .bt_symbols
                .clone()
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_uppercase())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_else(|| vec!["BTC".to_string()]),
            tf: match cli.bt_tf.as_deref() {
                Some(raw) => match cli_backtest::parse_tf(raw) {
                    Ok(tf) => tf,
                    Err(e) => {
                        eprintln!("⚠️  {e}");
                        std::process::exit(1);
                    }
                },
                None => vec![60, 180, 300, 900],
            },
            depth_days: cli
                .bt_depth
                .unwrap_or(workspace.backtest.archive_depth_days),
            portfolio_capital: cli.bt_capital.unwrap_or(1000.0),
            allocation: cli
                .bt_allocation
                .unwrap_or(workspace.minimal_tae.allocation_pct),
            strategy_name: cli.bt_strategy.clone(),
        };
        let outcome = cli_backtest::run_cli_backtest(&db_pool, &workspace, args).await;
        let code = cli_backtest::print_outcome(&outcome);
        std::process::exit(code);
    }

    // v9: headless strategy / account / instance ops (GUI parity).
    if !cli.ops.is_empty()
        || !cli.account_ops.is_empty()
        || cli.lifecycle_op.is_some()
        || cli.instance_bind.is_some()
    {
        let mut ws = config_models::load_workspace().unwrap_or_else(|e| {
            eprintln!("config load failed: {e}");
            std::process::exit(1);
        });
        let mut code = 0;
        for op in cli.ops {
            let c = cli_ops::run_strategy_op(&mut ws, &op);
            if c != 0 {
                code = c;
            }
        }
        for op in cli.account_ops {
            let c = cli_ops::run_account_op(&mut ws, &op);
            if c != 0 {
                code = c;
            }
        }
        if let Some((id, strategy)) = cli.instance_bind {
            let c = cli_ops::run_instance_bind(&mut ws, &id, &strategy);
            if c != 0 {
                code = c;
            }
        }
        if let Some((id, action)) = cli.lifecycle_op {
            let c = cli_ops::run_lifecycle_op(&id, &action);
            if c != 0 {
                code = c;
            }
        }
        std::process::exit(code);
    }

    if let Ok(secret) = std::env::var("EXCHANGE_SECRET_KEY") {
        let secret = secret.trim().to_string();
        if !secret.is_empty() {
            database_storage::crypto::init_master_key(&secret);
        }
    }
    // K1 / observe-paper allowance: only hard-fail on missing master key
    // when a live instance exists. Observe/paper sessions must boot even if
    // stale encrypted rows remain and EXCHANGE_SECRET_KEY is not set (operator
    // may be running read-only monitors).
    let any_live_early = workspace
        .instances
        .iter()
        .any(|i| i.mode == config_models::ExecutionMode::Live);
    if any_live_early {
        verify_encryption_or_panic(&db_pool).await;
    } else if !database_storage::crypto::master_key_available() {
        // Warn but do not block observe/paper boot — keys are inert.
        if let Ok((count,)) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM exchange_keys")
            .fetch_one(&db_pool)
            .await
        {
            if count > 0 {
                eprintln!(
                    "⚠️  {} encrypted exchange key(s) exist but EXCHANGE_SECRET_KEY is not set — live trading will fail until the key is provided (observe/paper continues).",
                    count
                );
            }
        }
    }

    let (telemetry_tx, telemetry_rx) = mpsc::channel::<database_storage::TelemetryMsg>(10000);
    // v10 fan-out: one producer → DB logger + DS exporter (three sinks:
    // DB, WS/GUI, ./ds files).
    let (db_tx, db_rx) = mpsc::channel::<database_storage::TelemetryMsg>(10000);
    let (ds_tx, ds_rx) = mpsc::channel::<database_storage::TelemetryMsg>(10000);
    // Read the liquidation-event retention window from the user's
    // `[workspace.liquidity]` config. The legacy hardcoded `7u32` was
    // 5x shorter than the configured 90 days and prematurely aged out
    // event rows that operators still wanted to query.
    let liq_retention_days = workspace.liquidity.event_retention_days.max(1);
    // BTE archive retention from [workspace.backtest].archive_depth_days.
    let archive_depth_days = workspace.backtest.archive_depth_days.max(1);
    tokio::spawn(async move {
        let mut rx = telemetry_rx;
        while let Some(msg) = rx.recv().await {
            let _ = db_tx.send(msg.clone()).await;
            let _ = ds_tx.send(msg).await;
        }
    });
    let logger_handle = tokio::spawn({
        let pool = db_pool.clone();
        async move {
            run_telemetry_logger(
                pool,
                db_rx,
                liq_retention_days,
                archive_depth_days,
                session_number,
            )
            .await;
        }
    });

    let symbol_mapper = Arc::new(core_domain::normalized::SymbolMapper::new());
    let connection_quality = Arc::new(ConnectionQualityRegistry::new());
    let reliability = Arc::new(ReliabilityTracker::new());
    let exchange_status = Arc::new(ExchangeStatusTracker::new());
    let latency_tracker = Arc::new(core_domain::LatencyTracker::default());

    let execution_engine = Arc::new({
        let mut engine = portfolio_supervisor::execution::engine::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig {
                maker_fee_pct: workspace.fees.maker_fee_pct,
                taker_fee_pct: workspace.fees.taker_fee_pct,
                funding_rate_8h: workspace.fees.funding_rate_8h,
                simulated_spread_pct: 0.01,
                // v10.1: the session's default strategy execution dial —
                // the engine is shared across instances, so one cost
                // model applies per session.
                slippage_bps: workspace
                    .default_strategy()
                    .map(|s| s.tae.execution.slippage_bps)
                    .unwrap_or(0.0),
            },
        );
        engine.set_db(Arc::new(db_pool.clone()));
        if let Some(sid) = session_number {
            engine.set_session_id(sid).await;
        }
        engine
            .set_cross_leverage(workspace.leverage.cross_leverage)
            .await;
        engine
    });

    // v7 TAE setup executor (shared with the API surface).
    let setup_executor = Arc::new(portfolio_supervisor::setup_executor::SetupExecutor::new(
        execution_engine.clone(),
        &workspace.minimal_tae,
    ));

    let platform_arc = Arc::new(RwLock::new(platform));
    let workspace_state = WorkspaceState::new(workspace.clone());
    let session = Arc::new(portfolio_supervisor::session::SessionState::new());
    let (recharge_tx, _) = tokio::sync::broadcast::channel::<api_gateway::RechargeNotice>(64);

    let hl_ws_url = platform_arc.read().await.hyperliquid.ws_url.clone();
    let bg_ws_url = platform_arc.read().await.bitget.ws_url.clone();
    let use_hl = workspace
        .default_exchange
        .eq_ignore_ascii_case("hyperliquid");
    let use_bg = workspace.default_exchange.eq_ignore_ascii_case("bitget");
    if use_hl {
        exchange_status.seed_single("Hyperliquid", &hl_ws_url).await;
    }
    if use_bg {
        exchange_status.seed_single("Bitget", &bg_ws_url).await;
    }
    println!("📡 Hyperliquid WS endpoint: {}", hl_ws_url);
    println!("📡 Bitget WS endpoint: {}", bg_ws_url);

    // ── Snapshot Export runtime (hydrated from `[snapshot_export]`) ─
    let snapshot_export_cfg = platform_arc.read().await.snapshot_export.clone();
    let snapshot_export_runtime = Arc::new(RwLock::new(
        execution_daemon::snapshot_export::runtime_from_config(&snapshot_export_cfg),
    ));
    let snapshot_export_manual_tick = Arc::new(tokio::sync::Notify::new());

    let mut app_state = Arc::new(AppState {
        workspace: workspace_state.clone(),
        session: session.clone(),
        platform: platform_arc.clone(),
        pool: db_pool.clone(),
        symbol_mapper: symbol_mapper.clone(),
        telemetry_tx: telemetry_tx.clone(),
        connection_quality: connection_quality.clone(),
        clock_monitor: None,
        reliability: reliability.clone(),
        exchange_status: exchange_status.clone(),
        latency_tracker: latency_tracker.clone(),
        ws_url: hl_ws_url.clone(),
        bitget_ws_url: bg_ws_url.clone(),
        allowed_origins: allowed_origins.clone(),
        overview: Arc::new(RwLock::new(None)),
        execution_engine: execution_engine.clone(),
        automation: if workspace.minimal_tae.enabled {
            Some(setup_executor.clone())
        } else {
            None
        },
        recharge_tx: recharge_tx.clone(),
        snapshot_export: snapshot_export_runtime.clone(),
        snapshot_export_manual_tick: snapshot_export_manual_tick.clone(),
        session_id: session_id_arc.clone(),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });

    // ── Launch plan (CLI mode: interactive; web mode: config-driven) ──
    //
    // CLI mode prompts the operator for exchange/currency/instances BEFORE
    // spawning anything — the plan is written into the workspace config so
    // `registry::add_instance` resolves the per-TF durations, then every
    // instance is spawned with the boot retry policy.
    // v8.2: the prompt offers a Backtest choice — the interactive sibling
    // of the GUI launcher (runs the simulation, then exits).
    let cli_plan: Option<CliLaunchPlan> = if matches!(cli.mode, LaunchMode::Cli) {
        let launch_type = prompt(
            "Launch type — [1] Terminal monitor (observe)  [2] Backtest",
            "1",
        );
        if launch_type.trim() == "2" {
            let args = cli_backtest::prompt_backtest_args(&workspace);
            let outcome = cli_backtest::run_cli_backtest(&db_pool, &workspace, args).await;
            std::process::exit(cli_backtest::print_outcome(&outcome));
        }
        cli_launch_plan(&cli, &workspace)
    } else {
        None
    };
    // Aborted at the Review step — exit cleanly before any heavy boot.
    let cli_start = matches!(cli.mode, LaunchMode::Cli) && cli_plan.is_some();
    if matches!(cli.mode, LaunchMode::Cli) && !cli_start {
        std::process::exit(0);
    }

    // ── Session auto-init (web and cli mode) ──────────────────────
    //
    // CLI mode uses the plan's trading mode (observe|paper|live); web mode
    // re-selects via Launch Setup wizard. The session default drives every
    // instance's ExecutionMode via registry::add_instance.
    {
        let (exchange, currency, trading) = match &cli_plan {
            Some(plan) => {
                let ex = if plan.exchange.eq_ignore_ascii_case("bitget") {
                    ExchangeChoice::Bitget
                } else {
                    ExchangeChoice::Hyperliquid
                };
                let cur = if plan.currency.eq_ignore_ascii_case("USDT") {
                    Currency::USDT
                } else {
                    Currency::USDC
                };
                (ex, cur, plan.trading.clone())
            }
            None => (
                cli.resolve_exchange(&workspace.default_exchange),
                cli.resolve_currency(&workspace.default_currency),
                cli.trading.clone().unwrap_or_else(|| "observe".to_string()),
            ),
        };
        // v7.2 parity: CLI mode pins the session defaults FIRST then
        // initialises — same ordering as POST /api/session/init.
        if cli_plan.is_some() {
            app_state
                .session
                .set_session_defaults(Some(trading.clone()), None)
                .await;
        }
        if let Err(e) = app_state.init_session(currency, exchange).await {
            eprintln!("⚠️  Session auto-init failed: {}", e);
        } else {
            println!(
                "✅ Session auto-initialised: {} on {}",
                currency.as_str(),
                exchange.as_str(),
            );
        }
        if cli_plan.is_some() {
            let desc = match trading.as_str() {
                "paper" => "paper (simulated orders)",
                "live" => "live (real orders)",
                _ => "observe (monitoring only — no orders dispatched)",
            };
            println!("✅ Session mode: {} — {}", trading, desc);
        }
    }

    // ── CLI plan → workspace config (per-instance TF durations) ────
    if let Some(plan) = &cli_plan {
        let mut cfg = workspace_state.config().await;
        let exec_mode = match plan.trading.as_str() {
            "paper" => config_models::ExecutionMode::Paper,
            "live" => config_models::ExecutionMode::Live,
            _ => config_models::ExecutionMode::Observe,
        };
        for inst in &plan.instances {
            let symbol = format!("{}-{}", inst.base, plan.currency);
            if cfg.instances.iter().any(|e| e.symbol == symbol) {
                continue;
            }
            let indicators = config_models::IndicatorsConfig::default();
            let tf = |secs: u64| config_models::TimeframeConfig::new(secs, indicators.clone());
            cfg.instances.push(config_models::InstanceEntry {
                id: format!("inst_{}", inst.base.to_lowercase()),
                symbol,
                quote: plan.currency.clone(),
                status: config_models::InstanceStatus::Running,
                micro_term: tf(inst.micro),
                fast_term: tf(inst.fast),
                slow_term: inst.slow.map(|s| tf(s)),
                macro_term: inst.r#macro.map(|s| tf(s)),
                automation: config_models::AutomationConfig::default(),
                operational_mode: config_models::OperationalMode::Advisory,
                mode: exec_mode.clone(),
                strategy: None,
                allocation_pct: None,
                weight_overrides: None,
                activation: None,
                custom_pipelines: std::collections::HashMap::new(),
            });
        }
        let _ = config_models::save_workspace(&cfg);
        workspace_state.set_config(cfg).await;
        println!(
            "✅ CLI launch plan applied to workspace config ({} instance{})",
            plan.instances.len(),
            if plan.instances.len() == 1 { "" } else { "s" }
        );
    }

    // ── Instance auto-spawn (web and cli mode) ─────────────────────
    //
    // Web mode: every entry in workspace.instances[] is spawned
    // automatically; the user may additionally add more pairs via the GUI.
    // CLI mode: the interactive plan's instances are spawned with the same
    // retry policy.
    //
    // **v6.5 fix (AUDIT-V7-306):** the session is left `active = true`
    // through the auto-spawn loop. The Launch Setup wizard only needs the
    // session fields (exchange, currency), not the `active` flag — and
    // `add_instance` rejects inactive sessions. We flip to inactive
    // **after** the loop completes, so configured instances bootstrap on
    // cold start in `--web` mode just like `--cli`.
    {
        let ctx = app_state.registry_context();
        if let Some(plan) = &cli_plan {
            for inst in &plan.instances {
                spawn_cli_instance(&ctx, &inst.base, &plan.currency).await;
            }
            // v10.2: --tae-on must actually START the lifecycle (paper/live boot PAUSED by default).
            // Previously this flag was only recorded in DS meta / header but never called lifecycle.start().
            if plan.tae_on {
                // Give the spawn loop a moment to persist instances before starting lifecycle
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let instances = workspace_state.list().await;
                for inst in instances {
                    if inst.execution_mode().await == config_models::ExecutionMode::Observe {
                        continue;
                    }
                    let id = inst.id.clone();
                    match portfolio_supervisor::registry::start_instance(&ctx, &id).await {
                        Ok(()) => eprintln!("▶️  TAE activated for {} (lifecycle → RUNNING)", id),
                        Err(e) if e.contains("already RUNNING") => {}
                        Err(e) => eprintln!("⚠️  Failed to activate TAE for {}: {}", id, e),
                    }
                }
            }
        } else {
            for entry in &workspace.instances {
                if entry.symbol.is_empty() {
                    continue;
                }
                // M7 (production audit): honor the persisted lifecycle status —
                // paused/stopped instances were force-started on every restart.
                if entry.status != config_models::InstanceStatus::Running {
                    eprintln!(
                        "⏸️  Instance {} skipped at boot (status = {:?})",
                        entry.symbol, entry.status
                    );
                    continue;
                }
                let (base, quote) = match entry.symbol.split_once('-') {
                    Some((b, q)) => (b.to_string(), q.to_string()),
                    None => {
                        eprintln!("⚠️  Skipping malformed symbol: {}", entry.symbol);
                        continue;
                    }
                };
                // M7 (production audit): instance creation is gated on a live
                // exchange symbol_exists REST call — an offline boot previously
                // failed every instance with NO retry, leaving an empty
                // deployment. Retry with backoff for up to ~10 minutes so a
                // boot-time network blip self-heals.
                let mut attempt = 0u32;
                loop {
                    match registry::add_instance(&ctx, (base.clone(), quote.clone())).await {
                        Ok(_inst) => {
                            println!("✅ Instance spawned: {}", entry.symbol);
                            break;
                        }
                        Err(e) => {
                            attempt += 1;
                            if attempt >= 20 {
                                eprintln!(
                                    "⚠️  Failed to spawn instance {} after {} attempts: {} — retry on next restart",
                                    entry.symbol, attempt, e
                                );
                                break;
                            }
                            eprintln!(
                                "⚠️  Failed to spawn instance {} (attempt {}): {} — retrying in 30 s",
                                entry.symbol, attempt, e
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        }
                    }
                }
            }
        }

        // v6.5 (AUDIT-V7-306): in web mode, mark session inactive AFTER
        // auto-spawn so the Launch Setup wizard still appears on first page
        // load but cold-start bootstrap is no longer skipped.
        if matches!(cli.mode, LaunchMode::Web) {
            app_state
                .session
                .active
                .store(false, std::sync::atomic::Ordering::Relaxed);
            println!("   (session marked inactive for Launch Setup wizard)");
        }
    }

    // ── Safety pool initialization ─────────────────────────────────
    {
        let instances = workspace_state.list().await;
        let pool_arc = Arc::new(db_pool.clone());
        for inst in &instances {
            inst.safety.set_db_pool(Arc::clone(&pool_arc)).await;
        }
    }

    // ── TAE v7: unified execution engine — equity seeding ─────────────
    // v9 (F-07): ONE portfolio-wide capital dial — the ledger seeds from
    // `[workspace] portfolio_capital_usd` (no per-instance capital).
    {
        let total_capital: f64 = workspace.portfolio_capital_usd;
        if total_capital > 0.0 {
            execution_engine
                .set_initial_equity(
                    rust_decimal::Decimal::from_f64_retain(total_capital)
                        .unwrap_or(rust_decimal_macros::dec!(10000)),
                )
                .await;
        }
    }

    let tae_enabled = workspace.minimal_tae.enabled;
    if tae_enabled {
        println!(
            "⚡ TAE v8.2: Setup executor enabled (allocation={}%, min_rr={}, max_positions={})",
            workspace.minimal_tae.allocation_pct,
            workspace.minimal_tae.min_net_rr,
            workspace.minimal_tae.max_open_positions
        );
    }

    // ── Phase E1 (v7.1): live trading bootstrap ──────────────────────
    // When any instance declares `mode = "live"`, load the active credential
    // for the workspace exchange from the encrypted `exchange_keys` table and
    // swap the engine onto the venue broker (Hyperliquid or Bitget). Fills
    // then arrive from the venue (REST-polled in the executor loop) instead
    // of the simulation.
    let any_live = workspace
        .instances
        .iter()
        .any(|i| i.mode == config_models::ExecutionMode::Live);
    if any_live {
        let live_quote = workspace
            .instances
            .iter()
            .find(|i| i.mode == config_models::ExecutionMode::Live)
            .map(|i| i.quote.clone())
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| workspace.default_currency.clone());

        let (address_or_key, secret_enc, passphrase_opt) = match workspace.default_exchange.as_str()
        {
            "Hyperliquid" => {
                let key_row = match sqlx::query_as::<_, (String, String, String)>(
                    "SELECT api_key, api_secret, COALESCE(passphrase, '') FROM exchange_keys \
                         WHERE exchange = 'Hyperliquid' AND is_active = 1 ORDER BY id DESC LIMIT 1",
                )
                .fetch_optional(&db_pool)
                .await
                {
                    Ok(row) => row,
                    Err(e) => {
                        eprintln!("❌ Live-mode key query failed: {e}");
                        std::process::exit(1);
                    }
                };
                match key_row {
                    Some((key, secret, _pass)) => (key, secret, None),
                    None => {
                        eprintln!(
                            "❌ mode = \"live\" requires an active Hyperliquid API key \
                             (POST /api/keys with EXCHANGE_SECRET_KEY set)"
                        );
                        std::process::exit(1);
                    }
                }
            }
            "Bitget" => {
                let key_row = match sqlx::query_as::<_, (String, String, String)>(
                    "SELECT api_key, api_secret, COALESCE(passphrase, '') FROM exchange_keys \
                         WHERE exchange = 'Bitget' AND is_active = 1 ORDER BY id DESC LIMIT 1",
                )
                .fetch_optional(&db_pool)
                .await
                {
                    Ok(row) => row,
                    Err(e) => {
                        eprintln!("❌ Live-mode key query failed: {e}");
                        std::process::exit(1);
                    }
                };
                match key_row {
                    Some((key, secret, pass)) => {
                        if pass.is_empty() {
                            eprintln!(
                                "❌ mode = \"live\" (Bitget) requires a passphrase — \
                                     re-add the key with a passphrase"
                            );
                            std::process::exit(1);
                        }
                        (key, secret, Some(pass))
                    }
                    None => {
                        eprintln!(
                            "❌ mode = \"live\" requires an active Bitget API key \
                             (POST /api/keys with EXCHANGE_SECRET_KEY set)"
                        );
                        std::process::exit(1);
                    }
                }
            }
            other => {
                eprintln!(
                    "❌ mode = \"live\" is not supported for exchange '{}' (Hyperliquid and Bitget only)",
                    other
                );
                std::process::exit(1);
            }
        };

        let secret = match database_storage::crypto::decrypt_field(&secret_enc) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ Failed to decrypt the live API secret (is EXCHANGE_SECRET_KEY correct?): {e}");
                std::process::exit(1);
            }
        };

        let broker: Box<dyn portfolio_supervisor::execution::ExecutionBackend> = if workspace
            .default_exchange
            .eq_ignore_ascii_case("Hyperliquid")
        {
            Box::new(portfolio_supervisor::execution::backend::LiveBroker::new(
                address_or_key,
                secret,
                true,
                None,
            ))
        } else {
            let product_type =
                network_adapters::adapters::bitget_live::product_type_from_quote(&live_quote)
                    .to_string();
            Box::new(
                portfolio_supervisor::execution::backend::BitgetLiveBroker::new(
                    address_or_key,
                    secret,
                    passphrase_opt.unwrap_or_default(),
                    product_type,
                ),
            )
        };
        execution_engine.set_live_backend(broker).await;
        println!(
            "🔴 TAE v7.1: LIVE mode active — orders dispatch to {}",
            workspace.default_exchange
        );
    }

    // ── Clock-drift monitor (NTP-based) — must run BEFORE build_router
    //    so the Arc is stored in AppState before the router clones it.
    if let Some(clock_cfg) = platform_arc.read().await.clock_monitor.clone() {
        if clock_cfg.is_active() {
            let monitor_cfg = ClockMonitorConfig {
                ntp_servers: clock_cfg.ntp_servers.clone(),
                poll_interval: std::time::Duration::from_secs(clock_cfg.poll_interval_secs),
                threshold: std::time::Duration::from_micros(
                    clock_cfg.threshold_micros.max(0) as u64
                ),
                breach_action: match clock_cfg.breach_action {
                    ClockMonitorBreachAction::Warn => BreachAction::Warn,
                    ClockMonitorBreachAction::Panic => BreachAction::Panic,
                },
                warn_on_breach: clock_cfg.warn_on_breach,
                jitter_window_size: clock_cfg.jitter_window_size,
                query_timeout: std::time::Duration::from_secs(clock_cfg.query_timeout_secs),
            };
            let monitor = Arc::new(ClockMonitor::new(monitor_cfg));
            Arc::get_mut(&mut app_state)
                .expect("AppState not yet shared — clock monitor must be set before build_router")
                .clock_monitor = Some(monitor.clone());
            println!(
                "🕒 Clock Monitor: starting NTP polling ({} servers, threshold={}µs)",
                clock_cfg.ntp_servers.len(),
                clock_cfg.threshold_micros
            );
        } else {
            println!("🕒 Clock Monitor: disabled by config");
        }
    } else {
        println!("🕒 Clock Monitor: no [clock_monitor] section — drift enforcement disabled");
    }

    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    handles.push(logger_handle);

    // ── v10 DS export layer (./ds/) ────────────────────────────────────
    {
        let ds_cfg = workspace.data_science.clone();
        let ds_pool = db_pool.clone();
        let ds_meta = execution_daemon::ds_exporter::DsSessionMeta {
            session_id: session_number.unwrap_or(0),
            mode: session_mode.clone(),
            exchange: workspace.default_exchange.clone(),
            currency: workspace.default_currency.clone(),
            capital: workspace.portfolio_capital_usd,
            started_at_ms: session_started_ms,
            config_snapshot: serde_json::to_value(&workspace).unwrap_or(serde_json::Value::Null),
            // v10.1: the operator's TAE-activation intent at launch
            // (CLI prompt / --tae-on; web sessions activate per instance).
            tae_activated: cli_plan.as_ref().map(|p| p.tae_on).unwrap_or(false),
        };
        handles.push(tokio::spawn(async move {
            execution_daemon::ds_exporter::run_ds_exporter(ds_pool, ds_rx, ds_cfg, ds_meta).await;
        }));
    }

    // ── TAE v7: Setup Executor loop ───────────────────────────────────
    if tae_enabled {
        let tae_engine = execution_engine.clone();
        let tae_executor = setup_executor.clone();
        let tae_workspace = workspace_state.clone();
        let tae_overview = app_state.overview.clone();
        let tae_cancel = CancellationToken::new();
        handles.push(tokio::spawn(async move {
            let executor = tae_executor;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            let mut funding_tick: u64 = 0;
            let funding_interval_secs: u64 = 8 * 3600; // 8h funding settlement

            // Boot-time recovery: log recovery-flatten for persisted open state.
            {
                let instances = tae_workspace.list().await;
                for inst in &instances {
                    executor.recover(&inst.id, &inst.symbol()).await;
                }
            }

            loop {
                tokio::select! {
                    _ = tae_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        funding_tick += 1;
                        if funding_tick >= funding_interval_secs {
                            tae_engine.settle_funding().await;
                            funding_tick = 0;
                        }

                        let instances = tae_workspace.list().await;
                        for inst in instances {
                            let symbol = inst.symbol();

                            // Gather the latest completed snapshot of each TF.
                            let mut guards: Vec<tokio::sync::RwLockReadGuard<
                                Option<core_domain::models::MarketSnapshot>,
                            >> = Vec::new();
                            for buf in [&inst.micro, &inst.fast, &inst.slow, &inst.r#macro] {
                                guards.push(buf.latest.read().await);
                            }
                            let snaps: Vec<&core_domain::models::MarketSnapshot> =
                                guards.iter().filter_map(|g| g.as_ref()).collect();
                            if snaps.is_empty() {
                                continue;
                            }

                            let mid = snaps
                                .iter()
                                .find_map(|s| {
                                    if s.mid_price > rust_decimal_macros::dec!(0) {
                                        Some(s.mid_price)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_else(|| snaps[0].mid_price);

                            // Lifecycle automation conditions (auto start/pause/stop).
                            {
                                let mut lifecycle = inst.lifecycle.write().await;
                                let current_price = snaps
                                    .iter()
                                    .find_map(|s| s.close.as_ref())
                                    .and_then(|c| c.to_string().parse::<f64>().ok());
                                let auto_actions = lifecycle.evaluate_automation(current_price);
                                drop(lifecycle);
                                for action in auto_actions {
                                    let mut lc = inst.lifecycle.write().await;
                                    match action {
                                        portfolio_supervisor::lifecycle::AutomationAction::Start => {
                                            let _ = lc
                                                .start("automation", Some("Auto start condition".into()))
                                                .await;
                                            eprintln!("🤖 LIFECYCLE: Auto-started instance {}", inst.id);
                                        }
                                        portfolio_supervisor::lifecycle::AutomationAction::Pause => {
                                            let _ = lc
                                                .pause("automation", Some("Auto pause condition".into()))
                                                .await;
                                            eprintln!("🤖 LIFECYCLE: Auto-paused instance {}", inst.id);
                                        }
                                        portfolio_supervisor::lifecycle::AutomationAction::Stop => {
                                            let _ = lc
                                                .stop("automation", Some("Auto stop condition".into()))
                                                .await;
                                            eprintln!("🤖 LIFECYCLE: Auto-stopped instance {}", inst.id);
                                        }
                                    }
                                }
                            }

                            let lifecycle_state = {
                                let lc = inst.lifecycle.read().await;
                                lc.state
                            };

                            // STOPPING → STOPPED: flatten at market, then complete.
                            if lifecycle_state == config_models::LifecycleState::Stopping {
                                tae_engine.cancel_orders_for_symbol(&symbol).await;
                                let _ = tae_engine
                                    .close_position(&symbol, mid, "stop_flatten")
                                    .await;
                                let has_position =
                                    tae_engine.get_position(&symbol).await.is_some();
                                if !has_position {
                                    let mut lc = inst.lifecycle.write().await;
                                    let _ = lc.complete_stop().await;
                                    eprintln!("🛑 LIFECYCLE: Instance {} → STOPPED (flatten confirmed)", inst.id);
                                }
                                continue;
                            }

                            let lifecycle_running =
                                lifecycle_state == config_models::LifecycleState::Running;

                            // Safety soft gate: no new entries in DRAWDOWN_STOP/SUSPENDED.
                            let safety_state = *inst.safety.safety_state.read().await;
                            let safety_allows = safety_state != SafetyState::DrawdownStop
                                && safety_state != SafetyState::Suspended;

                            let candle_ts = snaps.iter().map(|s| s.timestamp).max().unwrap_or(0);

                            // v7.1+: Observe instances are market-monitoring
                            // only — no fills processing. The setup executor
                            // still evaluates in ghost mode (`dispatch: false`)
                            // so the radar can show would-be setups, but it
                            // never dispatches orders for them.
                            let is_observe = inst.execution_mode().await
                                == config_models::ExecutionMode::Observe;

                            // BTE v8 parity: the per-tick session body lives
                            // in `run_tick` — the SAME function the backtest
                            // runner drives with recorded/archived snapshots.
                            // The daemon only decides the fill source.
                            let live_fills = if !is_observe
                                && tae_engine.mode().await == config_models::ExecutionMode::Live
                            {
                                match tae_engine.backend.read().await.poll_fills().await {
                                    Ok(fills) => Some(fills),
                                    Err(e) => {
                                        eprintln!("⚠️  LIVE poll_fills transient error (retry next tick): {e}");
                                        None
                                    }
                                }
                            } else {
                                None
                            };
                            // v9: strategy intake gates — breadth floor,
                            // systemic veto (enforced when the strategy's
                            // `pme.enforce_systemic_veto` is on), margin
                            // close-only, exposure caps. All veto OFF by
                            // default (default strategy), configurable per
                            // strategy. The gates follow the INSTANCE's
                            // bound strategy (`instances[].strategy`), not
                            // the workspace default.
                            let strategy_now = {
                                let cfg = tae_workspace.config().await;
                                let bound = workspace
                                    .instances
                                    .iter()
                                    .find(|e| e.symbol == symbol)
                                    .and_then(|e| e.strategy.clone())
                                    .unwrap_or_else(|| "default".to_string());
                                cfg.resolve_strategy(&bound).unwrap_or_default()
                            };
                            let overview = tae_overview.read().await.clone();
                            let breadth_pct = overview.as_ref().map(|o| o.breadth_pct).unwrap_or(0.0);
                            let systemic_risk = overview
                                .as_ref()
                                .map(|o| o.systemic_risk_score)
                                .unwrap_or(0.0);
                            let (market_filter_allows, block_reason) =
                                portfolio_supervisor::strategy_gates::evaluate_intake_gates(
                                    &strategy_now,
                                    breadth_pct,
                                    systemic_risk,
                                );
                            // v9 PME portfolio-state gates (enforced only
                            // when the strategy's `pme.exposure.enforce.*` /
                            // `pme.capital.enforce_margin_close_only`
                            // flags are on).
                            let (portfolio_allows, portfolio_block) = {
                                let positions = tae_engine.positions.read().await;
                                let equity = tae_engine.get_equity().await.max(1.0);
                                let leverage = (*tae_engine.cross_leverage.read().await)
                                    .to_f64()
                                    .unwrap_or(1.0);
                                let gross: f64 = positions
                                    .values()
                                    .map(|p| {
                                        let size = p.size.to_f64().unwrap_or(0.0);
                                        let px = p.entry_price.to_f64().unwrap_or(0.0);
                                        size * px
                                    })
                                    .sum();
                                let single: f64 = positions
                                    .get(symbol.as_str())
                                    .map(|p| {
                                        let size = p.size.to_f64().unwrap_or(0.0);
                                        let px = p.entry_price.to_f64().unwrap_or(0.0);
                                        size * px
                                    })
                                    .unwrap_or(0.0);
                                drop(positions);
                                let single_pct = single / equity * 100.0;
                                let portfolio_pct = gross / equity * 100.0;
                                let margin_ratio =
                                    (gross / leverage.max(1.0)) / equity;
                                portfolio_supervisor::strategy_gates::evaluate_portfolio_gates(
                                    &strategy_now,
                                    single_pct,
                                    portfolio_pct,
                                    margin_ratio,
                                )
                            };
                            let market_filter_allows = market_filter_allows && portfolio_allows;
                            let block_reason = block_reason.or(portfolio_block);
                            let outcome = portfolio_supervisor::execution::session_tick::run_tick(
                                &tae_engine,
                                &executor,
                                &inst.id,
                                &symbol,
                                &snaps,
                                mid,
                                portfolio_supervisor::setup_executor::TickContext {
                                    safety_allows_entry: safety_allows,
                                    lifecycle_running,
                                    candle_ts,
                                    safety: Some(inst.safety.clone()),
                                    dispatch: !is_observe,
                                    market_filter_allows_entry: market_filter_allows,
                                    entry_block_reason: block_reason,
                                    // v8.2: per-instance allocation override
                                    // (falls back to the global allocation_pct).
                                    allocation_pct: workspace
                                        .instances
                                        .iter()
                                        .find(|e| e.symbol == symbol)
                                        .and_then(|e| e.allocation_pct),
                                    // v9: bound strategy snapshot (frozen at
                                    // entry; drives intake gates + exits).
                                    strategy: Some(strategy_now.clone()),
                                },
                                live_fills,
                                false,
                            )
                            .await;

                            // Equity sync + PME informational safety update
                            // (peak equity, daily PnL, WARN / DRAWDOWN_STOP).
                            let current_equity = outcome.equity;
                            inst.set_current_equity(current_equity).await;
                            inst.safety
                                .update(
                                    rust_decimal::Decimal::from_f64_retain(current_equity)
                                        .unwrap_or_default(),
                                )
                                .await;
                        }
                    }
                }
            }
        }));
        println!("⚡ TAE v7: Setup executor loop started (1s cadence)");
    }

    // ── L7 Overview aggregation task ──────────────────────────
    {
        let overview_ref = app_state.overview.clone();
        let workspace = workspace_state.clone();
        let overview_cancel = CancellationToken::new();
        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = overview_cancel.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
                }
                let instances = workspace.list().await;
                let mut advisories: Vec<core_domain::advisory::AdvisoryMatrix> = Vec::new();
                let mut alignments: Vec<core_domain::alignment::AlignmentMatrix> = Vec::new();
                let mut metas: Vec<core_domain::overview::InstanceMeta> = Vec::new();
                // v7.2 parity: the panel builder consumes the SAME per-TF
                // snapshots below (fastest-present reference window) so the
                // hero / rows / buckets are derived from the identical data
                // the L7 aggregates read.
                let mut panel_instances: Vec<core_domain::overview_panel::PanelInstance> =
                    Vec::new();
                for inst in &instances {
                    let snapshots = inst.active_pair.latest_snapshots_all_tf().await;
                    // v6.10.18 (I-2): the L7 aggregates ALL FOUR timeframe
                    // windows (micro / fast / slow / macro) — per-window
                    // advisories feed the breadth / bias / opportunity /
                    // regime tallies. The previous slow-300s-only basis
                    // made the dashboard headline contradict every panel
                    // (e.g. HIGH_RISK next to an avg-risk of 41, or a
                    // stale "Pullback" while the Opportunity tab shows
                    // Scalp).
                    // v6.10.19a: the canonical per-pair risk is the MICRO
                    // window's L5 score (the same number the Risk panel,
                    // the dashboard KPI, and the asset rows show) — the
                    // TF-window MEAN drifted upward whenever the macro
                    // window scored high (MAX_COMPRESSION / thin
                    // participation) and rendered "HIGH_RISK" next to a
                    // visible avg-risk of 41–46. The window mean remains
                    // the warmup fallback when micro has no risk matrix
                    // yet.
                    let snaps = [
                        snapshots.0.as_ref(),
                        snapshots.1.as_ref(),
                        snapshots.2.as_ref(),
                        snapshots.3.as_ref(),
                    ];
                    // Audit fix (M6): `instance_count`/`is_active` must
                    // reflect actual monitoring — the previous `!cancel`
                    // check only flipped on delete/recharge, so a
                    // lifecycle-STOPPED instance kept counting as active
                    // and its stale TF windows kept feeding breadth/bias.
                    // STOPPED/STOPPING instances are no longer active;
                    // PAUSED instances stay counted (pipelines keep
                    // running, the operator can resume).
                    let lifecycle_state = inst.lifecycle.read().await.current();
                    let is_active = !inst.cancel.is_cancelled()
                        && lifecycle_state != config_models::LifecycleState::Stopped
                        && lifecycle_state != config_models::LifecycleState::Stopping;
                    // v6.10.19 (P7): per-TF-window risk pairs with decay
                    // weights (micro 0.1 / fast 0.2 / slow 0.3 / macro
                    // 0.4) — the L7 SYSTEMIC path must stay anchored to
                    // macro stability so a transient micro risk spike can
                    // never fire the PME safety veto.
                    let tf_weights = [0.1_f64, 0.2, 0.3, 0.4];
                    let mut risk_windows: Vec<(f64, f64)> = Vec::new();
                    let mut risk_sum = 0.0;
                    let mut risk_count = 0u32;
                    let mut alignment_pushed = false;
                    for (i, snap) in snaps.iter().enumerate() {
                        if let Some(snap) = snap {
                            if let Some(adv) = snap.advisory.clone() {
                                advisories.push(adv);
                            }
                            if let Some(r) = snap.risk.as_ref() {
                                risk_windows.push((tf_weights[i], r.overall_risk.score));
                                risk_sum += r.overall_risk.score;
                                risk_count += 1;
                            }
                            // The MTF AlignmentMatrix is one per symbol — push
                            // it once, from the fastest present window.
                            if !alignment_pushed {
                                if let Some(aln) = snap.alignment.clone() {
                                    alignments.push(aln);
                                    alignment_pushed = true;
                                }
                            }
                        }
                    }
                    metas.push(core_domain::overview::InstanceMeta {
                        // Audit fix (C1): `inst.pair.1` is the QUOTE currency
                        // ("USDT"/"USDC"), not the symbol. compute_overview
                        // keys risk bins / AssetRank / active_symbols by this
                        // value, so the quote-keyed symbol made every risk
                        // lookup miss (fallback 50.0 → always MODERATE /
                        // HIGH_RISK) and injected the quote into
                        // active_symbols. The canonical runtime symbol is
                        // `inst.symbol()` (active_pair.symbol, e.g.
                        // "BTC-USDT").
                        symbol: inst.symbol(),
                        timeframe_secs: 300,
                        timeframe_label: "tf-average".into(),
                        is_active,
                        // L7-A (v6.10.13): the per-symbol L5 overall risk —
                        // the canonical aggregate the L7 risk distribution
                        // bins on. v6.10.19a: the MICRO window's L5 score
                        // (operator-visible); falls back to the window mean,
                        // then 50 (moderate) during warmup.
                        overall_risk: canonical_overall_risk(
                            snaps[0]
                                .and_then(|s| s.risk.as_ref())
                                .map(|r| r.overall_risk.score),
                            risk_count,
                            risk_sum,
                        ),
                        risk_windows,
                    });
                    // v7.2 parity: feed the panel builder the present
                    // snapshots (fastest first) + lifecycle-active flag.
                    let present_snaps: Vec<core_domain::models::MarketSnapshot> = snaps
                        .iter()
                        .filter_map(|s| s.as_ref().map(|x| (*x).clone()))
                        .collect();
                    panel_instances.push(core_domain::overview_panel::PanelInstance {
                        symbol: inst.symbol(),
                        snapshots: present_snaps,
                        is_active,
                        // rank score is attached after compute_overview.
                        rank_score: 0.0,
                    });
                }
                // v9: the L7 params come from the effective default strategy.
                let overview_params = {
                    let ws = workspace.config().await;
                    ws.default_strategy()
                        .map(|st| {
                            market_analyzer::strategy_params::overview_params_from_strategy(&st.l7)
                        })
                        .unwrap_or_default()
                };
                let mut overview = core_domain::overview::compute_overview(
                    &advisories,
                    &metas,
                    &alignments,
                    &overview_params,
                );
                // v7.2 parity: canonical AssetRank scores → the panel rows,
                // then merge the server-computed panel payload into the
                // matrix. Both the dashboard and the CLI renderer read the
                // merged fields — one producer, one result.
                let ranks: std::collections::HashMap<String, f64> = overview
                    .asset_ranking
                    .iter()
                    .map(|r| (r.symbol.clone(), r.score))
                    .collect();
                for pi in &mut panel_instances {
                    pi.rank_score = ranks.get(&pi.symbol).copied().unwrap_or(0.0);
                }
                let panel =
                    core_domain::overview_panel::build_overview_panel(&panel_instances, &ranks);
                overview.hero = panel.hero;
                overview.overview_rows = panel.rows;
                overview.signal_quality = panel.signal_quality;
                overview.direction_distribution = panel.direction_distribution;
                overview.market_health_dims = panel.market_health_dims;
                *overview_ref.write().await = Some(overview);
            }
        }));
    }

    // ── Snapshot Export periodic task ─────────────────────────────
    {
        let runtime = snapshot_export_runtime.clone();
        let workspace = workspace_state.clone();
        let manual_tick = snapshot_export_manual_tick.clone();
        handles.push(tokio::spawn(async move {
            execution_daemon::snapshot_export::run_snapshot_exporter(
                runtime,
                workspace,
                CancellationToken::new(),
                manual_tick,
            )
            .await;
        }));
    }

    // ── Launch surface: CLI terminal monitor vs web server ──────────
    //
    // CLI mode binds NO HTTP server (lighter — no GUI, no WS surface) and
    // runs the periodic terminal monitor over the same L7 overview the
    // dashboard would render. `--save` flips the snapshot-export runtime on.
    if matches!(cli.mode, LaunchMode::Cli) {
        if cli.save_snapshots {
            let mut rt = snapshot_export_runtime.write().await;
            rt.enabled = true;
            if rt.output_path.is_empty() {
                rt.output_path = "./snapshots".to_string();
            }
            println!("💾 Snapshot export ENABLED (--save) → {}", rt.output_path);
        }
        let (ex_label, cur_label) = match &cli_plan {
            Some(plan) => (plan.exchange.as_str(), plan.currency.as_str()),
            None => (
                workspace.default_exchange.as_str(),
                workspace.default_currency.as_str(),
            ),
        };
        let tae_marker = if cli_plan.as_ref().map(|p| p.tae_on).unwrap_or(false) {
            "TAE: ON"
        } else {
            "TAE: OFF"
        };
        let cli_mode_label = cli_plan
            .as_ref()
            .map(|p| p.trading.as_str())
            .unwrap_or(cli.trading.as_deref().unwrap_or("observe"));
        let session_line = match session_number {
            Some(n) => format!(
                "SESSION #{:04} · {} · {tae_marker} · {} · {}",
                n, cli_mode_label, ex_label, cur_label
            ),
            None => format!(
                "SESSION #---- · {} · {tae_marker} · {} · {}",
                cli_mode_label, ex_label, cur_label
            ),
        };
        handles.push(tokio::spawn(
            execution_daemon::cli_renderer::run_terminal_monitor(
                app_state.overview.clone(),
                workspace_state.clone(),
                db_pool.clone(),
                cli.interval_secs,
                session_line,
                CancellationToken::new(),
            ),
        ));
    } else {
        let app = build_router(app_state.clone());
        let bind_addr = format!("{server_bind}:{server_port}");
        let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "❌ Web Server Setup: Failed to bind {bind_addr} ({e}). \
                     Another session may already use this port — set a distinct \
                     `[server] port` (or PLATFORM_PORT / --port) per folder."
                );
                std::process::exit(1);
            }
        };

        println!("🌐 Web Server Setup: Dashboard live at http://{bind_addr}");

        let server_handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("❌ Web Server Setup: Fatal crash running Axum HTTP server: {e}");
                std::process::exit(1);
            }
        });
        handles.push(server_handle);
    }

    let eval_cancel = CancellationToken::new();
    let eval_cancel1 = eval_cancel.clone();
    let pae_pool = db_pool.clone();
    // v10.1: the live pipeline honors the configured verdict bar + risk-free
    // rate from the default strategy's `pae` section (was hardcoded defaults).
    // (Re-reads the config here — `workspace` was moved into earlier tasks.)
    let pae_analytics_params = {
        let ws = match load_workspace() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("❌ PAE: failed to reload workspace config: {e}");
                std::process::exit(1);
            }
        };
        performance_analytics::strategy_analytics::AnalyticsParams::from_strategy(
            &ws.default_strategy().unwrap_or_default().pae,
        )
    };
    let pae_rf_pct = load_workspace()
        .map(|ws| {
            ws.default_strategy()
                .map(|s| s.pae.risk_math.risk_free_rate_pct)
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0);
    handles.push(tokio::spawn(async move {
        performance_evaluator::run_performance_evaluator(performance_evaluator::EvaluatorConfig {
            pool: pae_pool,
            cancel: eval_cancel1,
            eval_interval_secs: 300,
            analytics_params: pae_analytics_params,
            risk_free_rate_pct: pae_rf_pct,
        })
        .await;
    }));

    // Spawn clock monitor background task NOW (after storing its Arc).
    if let Some(clock_mon) = &app_state.clock_monitor {
        let monitor_clone = clock_mon.clone();
        let clock_cancel = CancellationToken::new();
        handles.push(tokio::spawn(async move {
            monitor_clone.run_until_cancelled(clock_cancel).await;
        }));
    }

    let quality_pool = db_pool.clone();
    let quality_registry = (*connection_quality).clone();
    let quality_cancel = eval_cancel.clone();
    handles.push(tokio::spawn(async move {
        quality_registry
            .run_persistence_loop(quality_pool, quality_cancel)
            .await;
    }));

    let eq_pool = db_pool.clone();
    let eq_cancel = eval_cancel.clone();
    let eq_engine = execution_engine.clone();
    handles.push(tokio::spawn(async move {
        portfolio_equity::run_portfolio_equity_logger_with_engine(
            eq_pool,
            Some(eq_engine),
            eq_cancel,
        )
        .await;
    }));

    let opt_pool = db_pool.clone();
    handles.push(tokio::spawn(async move {
        strategy_optimizer::run_strategy_optimizer(strategy_optimizer::OptimizerConfig {
            pool: opt_pool,
            cancel: eval_cancel,
            interval_secs: 3600,
        })
        .await;
    }));

    // K4 (production audit): graceful shutdown on SIGINT/SIGTERM.
    // Previously the process was killed abruptly — up to 10,000 queued
    // telemetry messages lost, the WAL tail lost, snapshot-export files
    // left partial. The signal path cancels every pipeline (same teardown
    // as POST /api/session/quit) then lets the SQLite logger drain the
    // queue before exiting. NOTE: unlike `quit_session`, the workspace
    // config.toml is NOT cleared — a signal stop is an operator restart,
    // not a session quit.
    let shutdown_state = app_state.clone();
    let shutdown_pool = db_pool.clone();
    let shutdown_session = session_number;
    let shutdown = async move {
        let ctrl_c = tokio::signal::ctrl_c();
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
        eprintln!("📥 Signal received — graceful shutdown");
        let live = shutdown_state.workspace.list().await;
        for inst in &live {
            inst.cancel.cancel();
        }
        // Let cancellation propagate + the logger drain the telemetry queue.
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        // v10: close the session row (ended_at + status).
        if let Some(sid) = shutdown_session {
            let ended = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let _ = database_storage::queries::sessions::close_session(&shutdown_pool, sid, ended)
                .await;
        }
        eprintln!("✅ Exiting cleanly");
        std::process::exit(0);
    };
    tokio::select! {
        _ = shutdown => {}
        _ = futures_util::future::join_all(handles) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_overall_risk;

    // v6.10.19a: the L7 canonical risk is the MICRO window's L5 score —
    // the visible number. The window-mean drift (macro MAX_COMPRESSION
    // windows) produced "HIGH_RISK" environments next to an avg-risk of
    // 41–46; regression tests for all three input shapes.
    #[test]
    fn micro_risk_wins_over_window_mean() {
        // Micro 41.45 vs windows averaging 58.6 (macro-heavy) — the old
        // mean would have crossed the ≥50 HIGH_RISK line.
        assert!((canonical_overall_risk(Some(41.45), 4, 234.4) - 41.45).abs() < 1e-9);
    }

    #[test]
    fn window_mean_is_the_warmup_fallback() {
        assert!((canonical_overall_risk(None, 4, 234.4) - 58.6).abs() < 1e-9);
        assert!((canonical_overall_risk(None, 2, 90.0) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn no_risk_matrices_yet_falls_back_to_moderate() {
        assert_eq!(canonical_overall_risk(None, 0, 0.0), 50.0);
    }
}
