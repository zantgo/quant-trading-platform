//! AUDIT-C1 regression: a restart must bring back the environment that
//! actually ran, never silently re-deriving pairs under a different key.
//!
//! The defect: `POST /api/session/init` (the wizard) persists the operator's
//! real exchange/currency to the `sessions` DB row only — it never writes
//! `config.toml`. At boot the daemon called
//! `init_session(cli.resolve_exchange(&workspace.default_exchange), …)`, i.e.
//! the stale template defaults (Hyperliquid / USDC). `add_instance` then FORCED
//! that quote onto every persisted instance, so `BTC-USDT` was re-created as
//! `BTC-USDC` on Hyperliquid and **appended** to `config.toml` as a duplicate.
//! The next restart hit "already exists", which the boot loop could not tell
//! apart from a transient network error, so it retried 20 × 30 s per entry.
//!
//! v11.12.23 had already fixed the Recovery CARD's text (it now reads the
//! session row), but the boot SPAWN path was never repointed at that row —
//! which is why the card advertised "Bitget · USDT" while Hyperliquid/USDC
//! instances were streaming.

use config_models::{InstanceEntry, InstanceStatus, WorkspaceConfig};
use portfolio_supervisor::session::{
    currency_from_str, exchange_from_str, resolve_boot_environment, spawn_error_is_permanent,
    BootEnvSource, Currency, ExchangeChoice,
};

fn entry(symbol: &str) -> InstanceEntry {
    InstanceEntry {
        id: symbol.to_lowercase(),
        symbol: symbol.to_string(),
        quote: symbol
            .rsplit_once('-')
            .map(|(_, q)| q.to_string())
            .unwrap_or_default(),
        status: InstanceStatus::Running,
        timeframes: std::collections::BTreeMap::new(),
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode: config_models::ExecutionMode::Observe,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    }
}

/// The operator's `config.toml` defaults never change — they stay the template
/// values no matter which venue the operator actually used.
const DEFAULT_EXCHANGE: &str = "Hyperliquid";
const DEFAULT_CURRENCY: &str = "USDC";

#[test]
fn string_parsers_round_trip_and_are_case_insensitive() {
    for ex in ["Hyperliquid", "bitget", "BYBIT"] {
        let parsed = exchange_from_str(ex).expect("must parse");
        assert_eq!(
            parsed.as_str().to_ascii_lowercase(),
            ex.to_ascii_lowercase()
        );
    }
    assert!(exchange_from_str("kraken").is_none());

    assert_eq!(currency_from_str("usdt"), Some(Currency::USDT));
    assert_eq!(currency_from_str("USDC"), Some(Currency::USDC));
    assert!(currency_from_str("DAI").is_none());
}

/// THE regression: operator ran Bitget/USDT, workspace defaults still say
/// Hyperliquid/USDC. Boot must return Bitget/USDT with NO mismatches.
#[test]
fn interrupted_session_environment_beats_stale_workspace_defaults() {
    let ws = WorkspaceConfig {
        instances: vec![entry("BTC-USDT"), entry("ETH-USDT")],
        ..Default::default()
    };

    let boot = resolve_boot_environment(
        DEFAULT_EXCHANGE,
        DEFAULT_CURRENCY,
        &ws.instances,
        Some(("Bitget", "USDT")),
    );

    assert_eq!(boot.exchange, ExchangeChoice::Bitget);
    assert_eq!(boot.currency, Currency::USDT);
    assert_eq!(boot.source, BootEnvSource::InterruptedSession);
    assert!(
        boot.mismatches.is_empty(),
        "nothing should mismatch once the right environment is recovered: {:?}",
        boot.mismatches
    );
}

/// Bybit/USDT likewise — Bybit only supports USDT, so a USDT default would have
/// been an outright failure, not just a wrong key.
#[test]
fn bybit_usdt_is_recovered_over_hyperliquid_usdc() {
    let ws = WorkspaceConfig {
        instances: vec![entry("SOL-USDT")],
        ..Default::default()
    };

    let boot = resolve_boot_environment(
        DEFAULT_EXCHANGE,
        DEFAULT_CURRENCY,
        &ws.instances,
        Some(("Bybit", "USDT")),
    );

    assert_eq!(boot.exchange, ExchangeChoice::Bybit);
    assert_eq!(boot.currency, Currency::USDT);
    assert!(boot.mismatches.is_empty());
}

/// No interrupted session (a clean `closed` one, or a first boot). The
/// persisted instance's OWN quote suffix is still better evidence than the
/// template defaults.
#[test]
fn persisted_instance_quote_beats_template_defaults() {
    let ws = WorkspaceConfig {
        instances: vec![entry("BTC-USDT")],
        ..Default::default()
    };

    let boot = resolve_boot_environment(DEFAULT_EXCHANGE, DEFAULT_CURRENCY, &ws.instances, None);

    assert_eq!(
        boot.currency,
        Currency::USDT,
        "the persisted instance's own quote suffix must win over the USDC template default"
    );
    assert_eq!(boot.exchange, ExchangeChoice::Hyperliquid);
    assert_eq!(boot.source, BootEnvSource::PersistedInstance);
    assert!(boot.mismatches.is_empty());
}

/// The plain Hyperliquid/USDC case: defaults are correct, nothing changes.
#[test]
fn a_workspace_matching_the_defaults_is_unchanged() {
    let ws = WorkspaceConfig {
        instances: vec![entry("BTC-USDC")],
        ..Default::default()
    };

    let boot = resolve_boot_environment(
        DEFAULT_EXCHANGE,
        DEFAULT_CURRENCY,
        &ws.instances,
        Some(("Hyperliquid", "USDC")),
    );

    assert_eq!(boot.exchange, ExchangeChoice::Hyperliquid);
    assert_eq!(boot.currency, Currency::USDC);
    assert!(boot.mismatches.is_empty());
}

/// The fail-safe: when the recovered environment CANNOT derive a persisted key,
/// it is reported rather than spawned (which would append a duplicate).
#[test]
fn a_key_mismatch_is_reported_not_silently_spawned() {
    let ws = WorkspaceConfig {
        instances: vec![entry("BTC-USDT")],
        ..Default::default()
    };

    // The operator ran Hyperliquid/USDC, but config still holds a USDT entry.
    let boot = resolve_boot_environment(
        DEFAULT_EXCHANGE,
        DEFAULT_CURRENCY,
        &ws.instances,
        Some(("Hyperliquid", "USDC")),
    );

    assert_eq!(boot.currency, Currency::USDC);
    assert_eq!(
        boot.mismatches.len(),
        1,
        "the USDT entry must be reported as unspawnable"
    );
    assert_eq!(boot.mismatches[0].declared, "BTC-USDT");
    assert_eq!(boot.mismatches[0].derived, "BTC-USDC");
}

/// Only RUNNING entries matter — a paused/stopped one must not skew the
/// resolved environment or generate a mismatch.
#[test]
fn non_running_entries_are_ignored() {
    let mut stopped = entry("BTC-USDT");
    stopped.status = InstanceStatus::Stopped;
    let ws = WorkspaceConfig {
        instances: vec![stopped],
        ..Default::default()
    };

    let boot = resolve_boot_environment(DEFAULT_EXCHANGE, DEFAULT_CURRENCY, &ws.instances, None);

    assert_eq!(boot.source, BootEnvSource::WorkspaceDefaults);
    assert!(
        boot.mismatches.is_empty(),
        "a stopped entry must not produce a mismatch: {:?}",
        boot.mismatches
    );
}

/// A HIP-3 `xyz:TLT` entry must compare against `TLT-USDC` — the registry
/// strips the dex qualifier before deriving the key.
#[test]
fn hip3_qualified_symbols_compare_on_the_stripped_key() {
    let mut hip3 = entry("TLT-USDC");
    hip3.id = "tlt".into();
    let ws = WorkspaceConfig {
        instances: vec![hip3],
        ..Default::default()
    };

    let boot = resolve_boot_environment(
        DEFAULT_EXCHANGE,
        DEFAULT_CURRENCY,
        &ws.instances,
        Some(("Hyperliquid", "USDC")),
    );

    assert!(
        boot.mismatches.is_empty(),
        "a plain persisted key must not be flagged: {:?}",
        boot.mismatches
    );
}

/// A fresh workspace with no instances still initialises a session rather than
/// panicking.
#[test]
fn an_empty_workspace_still_resolves() {
    let boot = resolve_boot_environment(DEFAULT_EXCHANGE, DEFAULT_CURRENCY, &[], None);

    assert_eq!(boot.exchange, ExchangeChoice::Hyperliquid);
    assert_eq!(boot.currency, Currency::USDC);
    assert!(boot.mismatches.is_empty());
}

/// Unparseable persisted text must not panic or produce nonsense.
#[test]
fn unparseable_environment_text_degrades_to_defaults() {
    let boot = resolve_boot_environment("kraken", "DAI", &[], Some(("nope", "nope")));

    assert_eq!(boot.exchange, ExchangeChoice::Hyperliquid);
    assert_eq!(boot.currency, Currency::USDC);
    assert_eq!(boot.source, BootEnvSource::WorkspaceDefaults);
}

/// A settlement combination the venue cannot host must NOT cause the resolver
/// to silently substitute a different venue behind the operator's back.
///
/// Precedence is deliberately strict, so this combination IS selected — and
/// `add_instance` then rejects it with a permanent error ("does not support USDC
/// settlement"), which `spawn_error_is_permanent` fast-fails instead of retrying
/// 20 times. Swapping the venue here would be worse: the operator would be
/// watching an asset they never asked for.
#[test]
fn an_unhostable_currency_is_selected_then_fast_failed_not_substituted() {
    let ws = WorkspaceConfig {
        instances: vec![entry("BTC-USDT")],
        ..Default::default()
    };

    let boot = resolve_boot_environment(
        "Hyperliquid",
        "USDT",
        &ws.instances,
        Some(("Hyperliquid", "USDT")),
    );

    // Strict precedence: the interrupted row wins even though Hyperliquid
    // cannot settle USDT.
    assert_eq!(boot.exchange, ExchangeChoice::Hyperliquid);
    assert_eq!(boot.currency, Currency::USDT);
    assert_eq!(boot.source, BootEnvSource::InterruptedSession);

    // ...and the key still derives, so no mismatch — the failure surfaces at
    // add_instance as a PERMANENT error, immediately.
    assert!(boot.mismatches.is_empty());
    assert!(
        spawn_error_is_permanent("Hyperliquid does not support USDT settlement."),
        "a settlement mismatch must not be retried 20 times"
    );
}

/// The permanent/transient split that stops a boot from stalling for ~9.5
/// minutes per bad entry, serially.
#[test]
fn spawn_errors_classify_into_permanent_and_transient() {
    // Permanent: retrying cannot help.
    for msg in [
        "Instance for pair BTC-USDC already exists",
        "No active session. Initialize a session (select exchange) before adding pairs.",
        "Hyperliquid does not support USDT settlement.",
        "No saved config for pair BTC-USDT",
        "Symbol too long",
        "'TLT' isn't available on Hyperliquid",
    ] {
        assert!(
            spawn_error_is_permanent(msg),
            "must be permanent (fast-fail): {msg}"
        );
    }

    // Transient: a network blip or venue timeout genuinely heals.
    for msg in [
        "Failed to spawn instance: connection timed out",
        "hyperliquid request failed after 3 attempts: error sending request",
        "Failed to resolve venue market: transport error",
        "Timed out waiting for symbol_exists",
    ] {
        assert!(
            !spawn_error_is_permanent(msg),
            "must be transient (retryable): {msg}"
        );
    }
}

/// An unrecognised message must default to RETRY. A new error string from a
/// future venue must never be able to make a recoverable boot failure give up
/// early and silently drop an instance.
#[test]
fn an_unknown_spawn_error_is_treated_as_transient() {
    assert!(!spawn_error_is_permanent(
        "Something nobody has ever seen before"
    ));
    assert!(!spawn_error_is_permanent(""));
}
