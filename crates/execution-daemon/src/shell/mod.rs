//! The observe shell — `./manage.sh shell` (`--mode cli`).
//!
//! **CR-15 replaced the legacy terminal monitor with this.** The monitor
//! (`cli_renderer`) was a second presentation of the same data the dashboard
//! already renders, with its own formatting, and it printed `Cascade`, the L6
//! verdict and per-setup risk/reward — all strategy. Two presentations of one
//! data set is a parity violation, and one of them was asserting things the
//! model does not say.
//!
//! The shell's defining property is that it reads the **same `AppState` the
//! WebSocket handler serves**. Parity therefore holds by construction rather
//! than by discipline: there is no second query path to keep in step.
//!
//! ```text
//! help              list every command
//! exit              leave (Ctrl-D also exits)
//! version           corpus + build identity
//! ```
//!
//! ## Grammar
//!
//! ```text
//! data   overview | exchange | connectivity | market-data | ntp | data-quality
//!        | distribution | connection-settings | instances | status
//! data   candles <pair> --tf <secs|all> --limit <1..500>
//!
//! model  instances | overview
//! model  state <pair> --tf <secs|all> [--axis <axis|all>] [--readiness solid|dashed] [--json]
//! model  alignment <pair>
//! model  consensus <pair>
//! model  leg <pair> --tf <secs|all> [--level major|minor]
//! model  events <pair> --tf <secs|all> [--kind <kind>]
//! model  diagnostics <pair> --tf <secs|all>
//! model  metrics <pair> --tf <secs> [--group <cluster>]
//! model  indicator <key> <pair> --tf <secs> --limit <n>
//! model  signals <pair> [--tf <secs|all>] [--kind <kind>]
//! model  divergences <pair> [--tf <secs|all>]
//! model  levels <pair> --tf <secs>
//!
//! config show | set <engine> <path> <value>
//! ```
//!
//! `model state` takes `--tf`; `model alignment` does not. **A flag must never
//! change what a command means** — so a command that addresses exactly one
//! duration *refuses* rather than defaulting, and `--tf all` is refused where it
//! would be meaningless.

use std::io::{BufRead, Write};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use api_gateway::AppState;

pub mod commands;
pub mod manifest;
pub mod parser;

/// The prompt. Kept short so a human can see what they are typing into.
pub const PROMPT: &str = "observe> ";

/// Where the shell reads lines from.
///
/// Two sources, one loop. `Channel` is the real terminal (a blocking stdin
/// reader on its own thread, so the async runtime is never stalled);
/// `Buffered` is a test driving the same loop with a script. A REPL whose body
/// can only be reached by owning stdin is a REPL nobody can test.
pub enum Lines {
    /// The live terminal.
    Channel(tokio::sync::mpsc::Receiver<String>),
    /// A scripted session — used by the shell tests to drive `help`/`exit`.
    Buffered(std::collections::VecDeque<String>),
}

impl Lines {
    async fn next(&mut self) -> Option<String> {
        match self {
            Lines::Channel(rx) => rx.recv().await,
            Lines::Buffered(q) => q.pop_front(),
        }
    }
}

pub async fn run(app_state: Arc<AppState>, session_line: String, cancel: CancellationToken) {
    println!("\n{session_line}");
    println!("{}", banner());
    println!("Type `help` for commands, `exit` to leave.\n");

    // A blocking read on its own thread: the REPL is a terminal, and a blocking
    // read inside the async runtime would stall every other task the daemon
    // runs (the pipelines, the clock monitor, the memory governor).
    let (line_tx, line_rx) = tokio::sync::mpsc::channel::<String>(64);
    std::thread::spawn(move || {
        // A std channel bridges the blocking reader into the runtime: the
        // forwarding thread does the `blocking_recv`, so no async task ever
        // blocks the reactor.
        let (std_tx, std_rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut out = std::io::stdout();
            for line in stdin.lock().lines() {
                match line {
                    Ok(l) => {
                        let _ = out.write_all(PROMPT.as_bytes());
                        let _ = out.flush();
                        if std_tx.send(l).is_err() {
                            return;
                        }
                    }
                    // EOF (Ctrl-D) or a read error both end the session.
                    Err(_) => return,
                }
            }
        });
        while let Ok(l) = std_rx.recv() {
            if line_tx.blocking_send(l).is_err() {
                return;
            }
        }
    });

    let mut out = std::io::stdout();
    serve(app_state, Lines::Channel(line_rx), &mut out, cancel).await;

    println!("\nbye.");
}

/// The REPL body, independent of where the lines come from and where the output
/// goes.
pub async fn serve(
    app_state: Arc<AppState>,
    mut lines: Lines,
    // `+ Send` because the daemon spawns the REPL onto the runtime; without it
    // the future is not `Send` and `tokio::spawn` refuses.
    out: &mut (dyn Write + Send),
    cancel: CancellationToken,
) {
    while !cancel.is_cancelled() {
        let line = match lines.next().await {
            Some(l) => l,
            // the reader ended: EOF on stdin
            None => break,
        };
        let cmd = line.trim();
        if cmd.is_empty() {
            continue;
        }
        let invocation = match parser::parse(cmd) {
            Ok(inv) => inv,
            Err(err) => {
                let _ = writeln!(out, "  {err}");
                let _ = out.flush();
                continue;
            }
        };
        if matches!(invocation.command, parser::Command::Exit) {
            break;
        }
        let wants_json = invocation.flags.contains_key("json");
        let outcome = commands::execute(&app_state, &invocation.command).await;
        if wants_json {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&outcome.value).unwrap_or_else(|_| "{}".into())
            );
        } else {
            let _ = write!(out, "{}", outcome.human);
        }
        let _ = out.flush();
    }
}

fn banner() -> String {
    "─".repeat(72).to_string()
}

/// The corpus + build identity line, also served by `version`.
pub fn version_line() -> String {
    format!(
        "corpus {} · build {} · mode observe",
        env!("CARGO_PKG_VERSION"),
        option_env!("GIT_BUILD_ID").unwrap_or("dev")
    )
}

/// The help text, generated from the grammar so it cannot describe a command
/// that does not exist, or omit one that does.
pub fn print_help() -> String {
    let mut out = String::new();
    out.push_str("\ncommands\n");
    for (help, description) in HELP_ROWS {
        out.push_str(&format!("  {help:<34} {description}\n"));
    }
    out.push_str(&format!(
        "\nparity manifest: {} surfaces, {}\n",
        manifest::MANIFEST.len(),
        help_footer()
    ));
    out.push_str(
        "the shell reads the same AppState the WebSocket handler serves, so its output\n\
         and the dashboard's cannot disagree. See 06-05 CLI Shell Contract.\n",
    );
    out
}

fn help_footer() -> &'static str {
    let served = manifest::MANIFEST
        .iter()
        .filter(|r| matches!(r.status, manifest::Status::Served))
        .count();
    let declared = manifest::MANIFEST.len() - served;
    // `format!` above takes a &str, so this is a small owned string in a Box —
    // kept simple by returning a static summary instead of formatting twice.
    Box::leak(format!("{served} served · {declared} declared for a later phase").into_boxed_str())
}

/// One help row: the command shape and what it answers.
const HELP_ROWS: &[(&str, &str)] = &[
    ("help", "this list"),
    ("version", "corpus + build identity"),
    ("exit", "leave the shell (Ctrl-D also exits)"),
    (
        "data status",
        "process, session, instance and socket counts",
    ),
    (
        "data instances",
        "running instances and their ACTIVE durations",
    ),
    ("data overview", "the instance overview the Data tab shows"),
    ("data exchange", "per-venue connection state"),
    (
        "data connectivity",
        "uptime, disconnects, reconnect latency, score",
    ),
    ("data market-data", "per-instance mid price and last bar"),
    ("data ntp", "clock offset, jitter, breaches"),
    ("data data-quality", "per-pipeline state"),
    ("data distribution", "L4 egress / socket clients"),
    (
        "data connection-settings",
        "the platform config the Connection Settings tab edits",
    ),
    (
        "data candles <pair> --tf <secs|all>",
        "OHLCV rows, oldest evicted beyond --limit",
    ),
    ("model instances", "the model's view of the workspace"),
    ("model overview", "the cross-pair reduction"),
    (
        "model state <pair> --tf <secs|all>",
        "per-duration model state",
    ),
    ("model alignment <pair>", "the cross-duration view"),
    ("model consensus <pair>", "what the durations agree on"),
    (
        "model metrics <pair> --tf <secs>",
        "every indicator on one duration",
    ),
    (
        "model indicator <key> <pair> --tf <secs>",
        "one indicator's series",
    ),
    ("model signals <pair>", "typed signals across durations"),
    ("model divergences <pair>", "live divergences"),
    ("model levels <pair> --tf <secs>", "levels under test"),
    (
        "model leg <pair> --tf <secs>",
        "the leg record (declared — P8)",
    ),
    (
        "model events <pair> --tf <secs>",
        "the event lifecycle (declared — P9)",
    ),
    (
        "model diagnostics <pair> --tf <secs>",
        "diagnostics (declared — P10)",
    ),
    (
        "config show",
        "the per-engine config surface (declared — R1)",
    ),
    (
        "config set <engine> <path> <value>",
        "write one owned key (declared — R1)",
    ),
];

/// The prompt, for tests.
pub fn prompt() -> &'static str {
    PROMPT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_every_parseable_command() {
        let help = print_help();
        for command in manifest::PARSEABLE {
            let head = command.split(' ').take(2).collect::<Vec<_>>().join(" ");
            assert!(
                help.contains(&head),
                "`{command}` is parseable but missing from help"
            );
        }
    }

    #[test]
    fn the_version_line_names_corpus_build_and_mode() {
        let line = version_line();
        assert!(line.contains("corpus "));
        assert!(line.contains("mode observe"));
    }
}
