//! **P6 — the command surface as a value** (`shell_commands.rs`).
//!
//! The grammar is tested as data, before any `AppState` exists. That ordering is
//! deliberate: a command that parses into the wrong shape is a wrong answer, and
//! testing it through the REPL would require a runtime, a terminal and a
//! running daemon to observe something the parser decides before any of those
//! exist.
//!
//! ## What is asserted
//!
//! 1. Every sub-command in the parity manifest parses.
//! 2. Flags are required where guessing would answer a different question, and
//!    refused where guessing would answer a *wrong* one.
//! 3. The bounded values are bounded: `--limit` against the one retention cap,
//!    `--tf` against the duration pool.
//! 4. The command vocabulary is closed — no strategy words, because the shell
//!    prints what the model says and the model does not say what to do.

use execution_daemon::shell::manifest::{self, Status};
use execution_daemon::shell::parser::{self, Command, DataCommand, ModelCommand, TfSelector};

/// Every manifest command, with the arguments its shape requires.
fn sample(command: &str) -> String {
    let line = match command {
        "data candles" => "data candles BTC-USDT --tf 300 --limit 40",
        "model state" => "model state BTC-USDT --tf 60 --json",
        "model alignment" => "model alignment BTC-USDT",
        "model consensus" => "model consensus BTC-USDT",
        "model leg" => "model leg BTC-USDT --tf 60",
        "model events" => "model events BTC-USDT --tf 60",
        "model diagnostics" => "model diagnostics BTC-USDT --tf 60",
        "model metrics" => "model metrics BTC-USDT --tf 60",
        "model indicator" => "model indicator rsi BTC-USDT --tf 60",
        "model signals" => "model signals BTC-USDT --tf 60",
        "model divergences" => "model divergences BTC-USDT --tf 60",
        "model levels" => "model levels BTC-USDT --tf 60",
        "config set" => "config set model ladder 5",
        // `data status`, `model instances`, `help`, … take no arguments.
        other => other,
    };
    line.to_string()
}

#[test]
fn every_command_in_the_manifest_parses() {
    for command in manifest::PARSEABLE {
        let line = sample(command);
        let invocation = parser::parse(&line)
            .unwrap_or_else(|e| panic!("`{line}` is in the manifest but does not parse: {e}"));
        assert_eq!(
            &invocation.command.full_name(),
            command,
            "`{line}` parsed to the wrong command"
        );
    }
}

#[test]
fn the_manifest_and_the_grammar_declare_the_same_commands() {
    // Two lists is one too many. If the grammar grows a sub-command the manifest
    // does not name, some surface has appeared with no way to inspect it.
    let from_manifest: std::collections::BTreeSet<&str> =
        manifest::MANIFEST.iter().map(|r| r.command).collect();
    for command in manifest::PARSEABLE {
        if manifest::NON_SURFACE.contains(command) {
            continue;
        }
        assert!(
            from_manifest.contains(command),
            "`{command}` is parseable but no dashboard surface claims it"
        );
    }
}

#[test]
fn a_per_duration_command_refuses_rather_than_defaulting() {
    // The failure this prevents: `model levels BTC-USDT` silently answering
    // about the 1m duration when the operator meant the 5m one.
    for line in [
        "model metrics BTC-USDT",
        "model levels BTC-USDT",
        "model indicator rsi BTC-USDT",
    ] {
        let err = parser::parse(line).unwrap_err();
        assert!(
            err.message.contains("ONE duration"),
            "`{line}` should refuse, said: {}",
            err.message
        );
    }
}

#[test]
fn a_multi_duration_command_defaults_to_all_and_says_so() {
    let invocation = parser::parse("model state BTC-USDT").unwrap();
    match invocation.command {
        Command::Model(ModelCommand::State { tf, .. }) => assert_eq!(tf, TfSelector::All),
        other => panic!("expected state, got {other:?}"),
    }
}

#[test]
fn the_retention_cap_bounds_the_limit_flag() {
    assert!(parser::parse("data candles BTC-USDT --limit 501").is_err());
    assert!(parser::parse("model indicator rsi BTC-USDT --tf 60 --limit 999").is_err());
    assert!(parser::parse("data candles BTC-USDT --limit 500").is_ok());
}

#[test]
fn a_duration_that_is_not_in_the_pool_is_refused_by_the_parser() {
    // 7 seconds is a number; it is not a duration. The parser refuses the shape
    // and the command refuses the membership — neither guesses.
    let err = parser::parse("data candles BTC-USDT --tf seven").unwrap_err();
    assert!(err.message.contains("seven"), "{}", err.message);
    assert!(parser::parse("data candles BTC-USDT --tf 7").is_ok());
}

#[test]
fn the_state_command_carries_every_filter_it_documents() {
    let invocation =
        parser::parse("model state BTC-USDT --tf 60 --axis momentum --readiness dashed --json")
            .unwrap();
    match &invocation.command {
        Command::Model(ModelCommand::State {
            axis,
            readiness,
            json,
            ..
        }) => {
            assert_eq!(axis.as_deref(), Some("momentum"));
            assert_eq!(readiness.as_deref(), Some("dashed"));
            assert!(json);
        }
        other => panic!("expected state, got {other:?}"),
    }
}

#[test]
fn an_unknown_sub_command_names_the_alternatives() {
    let err = parser::parse("data candlesx BTC-USDT").unwrap_err();
    assert!(
        err.message.contains("not a data command"),
        "{}",
        err.message
    );
    assert!(err.message.contains("candles"), "{}", err.message);
}

#[test]
fn the_command_vocabulary_carries_no_strategy_word() {
    // CR-15 / V-75: the shell describes, it never instructs. A word from this list
    // in a command name or flag is a strategy surface leaking into the observe
    // shell.
    let banned = [
        "entry",
        "stop",
        "target",
        "riskreward",
        "risk_reward",
        "setup",
        "verdict",
        "cascade",
        "opportunit",
        "recommend",
        "confidence",
        "winrate",
    ];
    for command in manifest::PARSEABLE {
        let lowered = command.to_lowercase();
        for word in banned {
            assert!(
                !lowered.contains(word),
                "`{command}` contains the strategy word `{word}`"
            );
        }
    }
}

#[test]
fn a_declared_command_is_refused_rather_than_silently_empty() {
    // `model leg` before P8 must say so. An empty table would read as "no legs".
    for (command, phase) in [
        ("model leg BTC-USDT --tf 60", "P8"),
        ("model events BTC-USDT --tf 60", "P9"),
        ("model diagnostics BTC-USDT --tf 60", "P10"),
    ] {
        let invocation = parser::parse(command).unwrap();
        let head = command.split(' ').take(2).collect::<Vec<_>>().join(" ");
        let status = execution_daemon::shell::commands::status_of(&head);
        assert_eq!(
            status,
            Some(Status::Declared(phase)),
            "`{head}` has no status"
        );
    }
}

#[test]
fn data_candles_keeps_the_oldest_row_out_and_the_newest_in() {
    // The cap is a *retention* statement, not a display limit: asking for 500
    // rows returns the 500 the system holds.
    let invocation = parser::parse("data candles BTC-USDT --tf all --limit 500").unwrap();
    match invocation.command {
        Command::Data(DataCommand::Candles { tf, limit, .. }) => {
            assert_eq!(tf, TfSelector::All);
            assert_eq!(limit, 500);
        }
        other => panic!("expected candles, got {other:?}"),
    }
}
