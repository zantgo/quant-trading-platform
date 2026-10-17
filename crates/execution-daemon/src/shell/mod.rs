//! The observe shell — `--mode cli`.
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
//! Phase **P6** fills in the `die *` and `mme *` command families, together
//! with the parity manifest and the `StatesView` renderer. Until then the shell
//! says so rather than pretending to answer questions it cannot.

use std::io::{BufRead, Write};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use api_gateway::AppState;

/// The prompt. Kept short so a human can see what they are typing into.
const PROMPT: &str = "observe> ";

pub async fn run(app_state: Arc<AppState>, session_line: String, cancel: CancellationToken) {
    // `app_state` is unused until P6 wires the data commands. It is bound here
    // rather than dropped so the signature — and therefore the spawn site in
    // main.rs — is already the one P6 needs, and so nobody re-introduces a
    // second data path in the meantime.
    let _ = app_state;

    println!("\n{session_line}");
    println!("{}", observe_shell_banner());
    println!("Type `help` for commands, `exit` to leave.\n");

    // A blocking read on its own thread: the REPL is a terminal, and a blocking
    // read inside the async runtime would stall every other task the daemon
    // runs (the pipelines, the clock monitor, the memory governor).
    let (line_tx, line_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut out = std::io::stdout();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    let _ = out.write_all(PROMPT.as_bytes());
                    let _ = out.flush();
                    if line_tx.send(l).is_err() {
                        return;
                    }
                }
                // EOF (Ctrl-D) or a read error both end the session.
                Err(_) => return,
            }
        }
    });

    while !cancel.is_cancelled() {
        let line = match line_rx.recv() {
            Ok(l) => l,
            // the reader thread ended: EOF on stdin
            Err(_) => break,
        };
        let cmd = line.trim();
        if cmd.is_empty() {
            continue;
        }
        match cmd {
            "exit" | "quit" | "\\q" => break,
            "help" | "?" => print_help(),
            "version" => println!("{}", version_line()),
            other => println!(
                "  `{}` is not available yet. The data commands (`die …`, `mme …`) are\n  \
                 wired in phase P6; the legacy terminal monitor was removed in P3.",
                other
            ),
        }
    }

    println!("\nbye.");
}

fn observe_shell_banner() -> String {
    "─".repeat(72).to_string()
}

fn version_line() -> String {
    format!(
        "corpus {} · build {} · mode observe",
        env!("CARGO_PKG_VERSION"),
        option_env!("GIT_BUILD_ID").unwrap_or("dev")
    )
}

fn print_help() {
    println!(
        r#"
commands
  help                 this list
  exit                 leave the shell (Ctrl-D also exits)
  version              corpus + build identity

not yet wired (phase P6)
  die  overview | exchange | connectivity | market-data | ntp | data-quality
       | distribution | connection-settings | instances | status
  die  candles <pair> --tf <secs|all> --limit <1..500>
  mme  instances | overview | states | state | leg | events | diagnostics
       | consensus | alignment | metrics | indicator | signals
       | divergences | levels

the shell reads the same AppState the WebSocket handler serves, so its output
and the dashboard's cannot disagree. See 06-05 CLI Shell Contract.
"#
    );
}

/// The prompt, for tests.
pub fn prompt() -> &'static str {
    PROMPT
}
