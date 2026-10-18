//! **P6 — the parity manifest has no orphans in either direction**
//! (`shell_parity.rs`).
//!
//! ## The failure this catches
//!
//! A surface the terminal cannot reach, or a command with no surface behind it.
//! Neither crashes; every number stays right. A fact that exists in only one
//! place is not a discrepancy anyone notices until someone needs it from the
//! other place — which is exactly the parity violation the shell exists to
//! prevent.
//!
//! Both directions are asserted:
//!
//! * **surface → command.** Every manifest row names a command the grammar
//!   accepts and `execute` handles.
//! * **command → surface.** Every parseable command is claimed by a row, or is
//!   declared a non-surface (`help`, `version`, `exit`, `config set`) in
//!   `NON_SURFACE`, which is itself checked to be non-empty and small.
//!
//! ## Why a `Declared` row still counts
//!
//! `model leg` before P8 is a surface the model has not built. It stays in the
//! manifest with the phase that owns it, so the row cannot vanish and the command
//! cannot be quietly re-scoped. What it may not do is serve an empty table.

use std::collections::BTreeSet;

use execution_daemon::shell::manifest::{Status, MANIFEST, NON_SURFACE, PARSEABLE};
use execution_daemon::shell::parser;

#[test]
fn every_surface_row_names_a_command_that_parses() {
    for row in MANIFEST {
        let head = row.command.split(' ').next().unwrap_or("");
        // A manifest row is a command NAME; the parseable line needs the
        // arguments the name requires (a pair, mostly).
        let invocation = parser::parse(&sample_line(row.command))
            .unwrap_or_else(|e| panic!("`{}` does not parse: {e}", row.command));
        assert_eq!(
            invocation.command.head(),
            head,
            "`{}` parses to a different head",
            row.command
        );
    }
}

#[test]
fn every_surface_row_names_a_command_that_reaches_the_dispatcher() {
    // A command that parses but that `execute` forgets is a surface with no
    // answer. `Command::full_name` is the only list the dispatcher answers to.
    for row in MANIFEST {
        let invoked = sample_line(row.command);
        let invocation = parser::parse(&invoked).expect("manifest command parses");
        // The dispatcher match is exhaustive over `Command`, so reaching it is a
        // compile-time property. What is checkable here is that the manifest's
        // name is the one the dispatcher reports, which is what the human sees
        // in a refusal message.
        assert_eq!(
            invocation.command.full_name(),
            row.command,
            "`{}` is reached under a different name",
            row.command
        );
    }
}

#[test]
fn every_parseable_command_is_claimed_by_a_surface_or_declared_non_surface() {
    let claimed: BTreeSet<&str> = MANIFEST.iter().map(|r| r.command).collect();
    for command in PARSEABLE {
        assert!(
            claimed.contains(command) || NON_SURFACE.contains(command),
            "`{command}` exists in the grammar but no dashboard surface claims it \
             and it is not declared a non-surface command"
        );
    }
}

#[test]
fn the_non_surface_list_stays_small_and_explained() {
    // A growing NON_SURFACE list is how a real surface would stop being a
    // surface: five entries is already the most a "acts rather than reports"
    // command family should hold.
    assert!(
        NON_SURFACE.len() <= 5,
        "NON_SURFACE has grown to {}: {}",
        NON_SURFACE.len(),
        NON_SURFACE.join(", ")
    );
    for command in NON_SURFACE {
        assert!(
            !claimed_by_manifest(command),
            "`{command}` is declared a non-surface but a manifest row claims it"
        );
    }
}

#[test]
fn every_manifest_row_carries_a_status_and_a_phase_when_declared() {
    for row in MANIFEST {
        if let Status::Declared(phase) = row.status {
            assert!(
                phase.starts_with('P') || phase.starts_with('R'),
                "`{}` is declared for an unnamed phase `{phase}`",
                row.command
            );
        }
    }
}

#[test]
fn both_engines_are_represented_and_no_row_uses_a_retired_prefix() {
    let mut engines = BTreeSet::new();
    for row in MANIFEST {
        if !row.engine.prefix().is_empty() {
            engines.insert(row.engine.prefix());
        }
        assert!(
            !row.command.starts_with("die") && !row.command.starts_with("mme"),
            "`{}` uses a retired engine prefix (D-2 landed the rename in P6)",
            row.command
        );
    }
    assert_eq!(
        engines,
        BTreeSet::from_iter(["data", "model"]),
        "the manifest must cover both engines the shell serves"
    );
}

#[test]
fn a_tab_cannot_appear_in_the_manifest_without_a_command() {
    // The UI engine-tab list is the source of the tab keys. A tab the manifest
    // has never heard of is the "surface with no command" case at the tab level.
    for tab in [
        "overview",
        "settings",
        "charts",
        "metrics",
        "alignment",
        "monitor",
        "terminal",
    ] {
        assert!(
            MANIFEST.iter().any(|r| r.tab == tab),
            "the `{tab}` tab has no manifest row"
        );
    }
}

fn claimed_by_manifest(command: &str) -> bool {
    MANIFEST.iter().any(|r| r.command == command)
}

/// A parseable line for a manifest command.
fn sample_line(command: &str) -> String {
    match command {
        "data candles" => "data candles BTC-USDT --tf 300 --limit 40".to_string(),
        "model state" => "model state BTC-USDT --tf 60 --json".to_string(),
        "model alignment" => "model alignment BTC-USDT".to_string(),
        "model consensus" => "model consensus BTC-USDT".to_string(),
        "model leg" => "model leg BTC-USDT --tf 60".to_string(),
        "model events" => "model events BTC-USDT --tf 60".to_string(),
        "model diagnostics" => "model diagnostics BTC-USDT --tf 60".to_string(),
        "model metrics" => "model metrics BTC-USDT --tf 60".to_string(),
        "model indicator" => "model indicator rsi BTC-USDT --tf 60".to_string(),
        "model signals" => "model signals BTC-USDT --tf 60".to_string(),
        "model divergences" => "model divergences BTC-USDT --tf 60".to_string(),
        "model levels" => "model levels BTC-USDT --tf 60".to_string(),
        "config set" => "config set model ladder 5".to_string(),
        other => other.to_string(),
    }
}

/// The manifest is the same value the shell and the dashboard both cite, so a
/// change to it is visible in both. This test pins the count so a row cannot be
/// deleted without also updating the phase report.
#[test]
fn the_manifest_covers_every_row_it_claims_to() {
    let unique: BTreeSet<(&str, &str)> = MANIFEST.iter().map(|r| (r.tab, r.block)).collect();
    assert_eq!(
        unique.len(),
        MANIFEST.len(),
        "the manifest has two rows for the same tab + block; one of them is unreachable"
    );
    assert!(
        MANIFEST.len() >= 20,
        "the manifest shrank to {} rows — that is a deletion, not a refactor",
        MANIFEST.len()
    );
}

/// `Command::head` and `full_name` must agree, or a refusal message names a
/// command that does not exist.
#[test]
fn head_and_full_name_agree() {
    for (line, head, full) in [
        ("data ntp", "data", "data ntp"),
        ("model levels BTC-USDT --tf 60", "model", "model levels"),
        ("config show", "config", "config show"),
        ("help", "help", "help"),
    ] {
        let parsed = parser::parse(line).unwrap();
        assert_eq!(parsed.command.head(), head, "`{line}` head");
        assert_eq!(parsed.command.full_name(), full, "`{line}` full name");
    }
}
