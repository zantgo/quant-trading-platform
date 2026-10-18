//! **P6 — the REPL runs** (`shell_repl.rs`).
//!
//! The parser is tested as data (`shell_commands.rs`), the parity manifest as a
//! table (`shell_parity.rs`), the payload as a value (`shell_ws_parity.rs`) —
//! none of which proves the loop itself works. This file drives
//! `shell::serve` with a scripted session: the same function the daemon spawns,
//! fed a `VecDeque` instead of a terminal.
//!
//! What it asserts:
//!
//! * `help`, `version` and `exit` work and `exit` stops the loop;
//! * an unknown command is refused, and the loop **continues** — one bad line
//!   must not end a session an operator is using to watch a market;
//! * `--json` prints a parseable document;
//! * nothing the shell prints carries a strategy claim (V-75), which is the
//!   check that CR-15's deletion of the legacy monitor actually held.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use api_gateway::AppState;
use execution_daemon::shell::{serve, Lines};

/// A `Write` sink a test can read back, and `Send` so the loop can be spawned.
#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuf {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("utf-8")
    }
}

/// An empty `AppState` — the REPL must work with no instances, no snapshots and
/// no session, because that is what a shell sees between instance spawns.
async fn empty_state() -> Arc<AppState> {
    {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        let url = "ws://127.0.0.1:1".to_string();
        let (telemetry_tx, _rx) = tokio::sync::mpsc::channel(16);
        Arc::new(AppState {
            workspace: portfolio_supervisor::workspace_state::WorkspaceState::empty(),
            platform: Arc::new(tokio::sync::RwLock::new(
                config_models::PlatformConfig::default(),
            )),
            session: Arc::new(portfolio_supervisor::session::SessionState::new()),
            pool,
            symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
            telemetry_tx,
            connection_quality: Arc::new(
                network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
            ),
            ws_url: url.clone(),
            bitget_ws_url: url.clone(),
            bybit_ws_url: url.clone(),
            clock_monitor: None,
            reliability: Arc::new(
                network_adapters::pipeline_reliability::ReliabilityTracker::new(),
            ),
            exchange_status: Arc::new(
                network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
            ),
            latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
            overview: Arc::new(tokio::sync::RwLock::new(None)),
            automation: None,
            execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
                portfolio_supervisor::paper_trading::FeesConfig::default(),
            )),
            recharge_tx: tokio::sync::broadcast::channel(16).0,
            snapshot_export: Arc::new(tokio::sync::RwLock::new(
                core_domain::snapshot_export::SnapshotExportRuntime::default(),
            )),
            snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
            session_id: Arc::new(tokio::sync::RwLock::new(None)),
            interrupted_session: Arc::new(tokio::sync::RwLock::new(None)),
            boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
            backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
        })
    }
}

async fn drive(state: Arc<AppState>, script: &[&str]) -> String {
    let lines = Lines::Buffered(
        script
            .iter()
            .map(|s| s.to_string())
            .collect::<VecDeque<_>>(),
    );
    let mut out = SharedBuf::default();
    let handle = out.clone();
    serve(state, lines, &mut out, CancellationToken::new()).await;
    handle.text()
}

#[tokio::test]
async fn help_prints_the_command_list() {
    let text = drive(empty_state().await, &["help"]).await;
    assert!(
        text.contains("data status"),
        "help is missing a data command:\n{text}"
    );
    assert!(
        text.contains("model state"),
        "help is missing a model command"
    );
    assert!(
        text.contains("parity manifest"),
        "help does not cite the manifest"
    );
}

#[tokio::test]
async fn version_names_all_three_version_lines_and_the_mode() {
    // V-101/V-102: the shell reports the spec line, the code line and the build
    // SHA. Before this, `build` was a `GIT_BUILD_ID` lookup that nothing set, so
    // it always printed "dev" and the reproducibility banner had nothing to
    // compare against.
    let text = drive(empty_state().await, &["version"]).await;
    assert!(text.contains("spec "), "{text}");
    assert!(text.contains("code "), "{text}");
    assert!(text.contains("build "), "{text}");
    assert!(text.contains("mode observe"), "{text}");
    assert!(!text.contains("dev"), "{text}");
}

#[tokio::test]
async fn exit_stops_the_loop_and_later_lines_never_run() {
    let text = drive(empty_state().await, &["version", "exit", "version"]).await;
    assert_eq!(
        text.matches("spec ").count(),
        1,
        "a command after `exit` still ran:\n{text}"
    );
}

#[tokio::test]
async fn an_unknown_command_is_refused_and_the_session_continues() {
    // One typo must not cost an operator their terminal session.
    let text = drive(empty_state().await, &["nonsense", "version"]).await;
    assert!(text.contains("not a command"), "{text}");
    assert!(
        text.contains("spec "),
        "the session did not continue:\n{text}"
    );
}

#[tokio::test]
async fn an_empty_line_is_ignored() {
    let text = drive(empty_state().await, &["", "   ", "version"]).await;
    assert!(text.contains("spec "));
}

#[tokio::test]
async fn a_json_request_prints_a_parseable_document() {
    let text = drive(empty_state().await, &["data status --json"]).await;
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("`--json` printed unparseable output ({e}):\n{text}"));
    assert!(value["mode"].as_str() == Some("observe"));
    assert!(value.get("instances").is_some());
}

#[tokio::test]
async fn commands_against_an_empty_workspace_say_so_rather_than_invent() {
    for line in [
        "data instances",
        "model instances",
        "data market-data",
        "model overview",
        "data exchange",
    ] {
        let text = drive(empty_state().await, &[line]).await;
        assert!(!text.trim().is_empty(), "`{line}` printed nothing at all");
    }
    let text = drive(empty_state().await, &["data instances"]).await;
    assert!(
        text.contains("no running instance"),
        "an empty workspace must be stated, not implied:\n{text}"
    );
}

#[tokio::test]
async fn a_pair_with_no_instance_is_refused_not_reported_as_empty() {
    let text = drive(empty_state().await, &["data candles BTC-USDT --tf 60"]).await;
    assert!(
        text.contains("no running instance"),
        "`data candles` on an unknown pair must refuse:\n{text}"
    );
}

#[tokio::test]
async fn the_shell_prints_no_strategy_claim() {
    // V-75 / CR-15: the shell describes what the candles are doing. It must not
    // tell the operator what to do with them.
    let script = [
        "help",
        "version",
        "data status",
        "data instances",
        "data overview",
        "data exchange",
        "data connectivity",
        "data distribution",
        "data ntp",
        "model overview",
        "model consensus BTC-USDT",
        "model leg BTC-USDT --tf 60",
        "config show",
    ];
    let text = drive(empty_state().await, &script).await;
    for banned in [
        "risk_reward",
        "risk-reward",
        "take profit",
        "stop loss",
        "entry price",
        "verdict",
        "cascade",
        "opportunity",
        "recommend",
    ] {
        let lowered = text.to_lowercase();
        assert!(
            !lowered.contains(banned),
            "the shell printed `{banned}`, which is a strategy claim:\n{text}"
        );
    }
}
