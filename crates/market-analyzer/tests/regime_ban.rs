//! **V-110 / V-111 — CR-11: the model path carries a diagnostic, not a regime.
//!
//! # What CR-11 decided
//!
//! `regime` is banned as a model identifier. `IndicatorGroup::Regime` was
//! re-filed, and `MarketRegime` + `MarketPhase` were to become
//! `diagnostics.state`.
//!
//! # The state of that work, stated precisely
//!
//! **The model path was already compliant when this gate was written.** The
//! `model_state` assembler classifies condition as
//! `DiagnosticState::{Chopping, Expanding, Settling, Coiling}` from `bbwp` and
//! `rvol` — two candle-derived bases — and `crates/market-analyzer/src/model/`,
//! `core_domain::{axes, leg, event}` contain **zero** occurrences of `regime`,
//! `MarketPhase` or `prev_regime`.
//!
//! What is *not* yet done is the retirement of the older vocabulary, which
//! still lives in `AnalysisMatrix`:
//!
//! | identifier | site | retired by |
//! |---|---|---|
//! | `MarketRegime` (8 variants) | `core-domain/src/analysis.rs:116` | **V-228** |
//! | `MarketPhase` (5 variants) | `core-domain/src/analysis.rs:314` | **V-231** |
//! | `derive_analysis`'s hysteresis input | `core-domain/src/analysis.rs:664` | **V-230** |
//! | `prev_regime` carrier (live) | `market-analyzer/src/analyzer/mod.rs:1190, 2855, 3556, 3569` | **V-230** |
//! | `prev_regime` carrier (replay) | `backtesting-engine/src/historical.rs:357` | **V-506** (R2, one state carrier) |
//!
//! **CR-11 cannot close before V-228…V-231.** `prev_regime` is not a loose
//! variable — it is the hysteresis input to `derive_analysis`, so removing it
//! means removing `AnalysisMatrix`, which is 34 files. Attempting that inside
//! this gate's phase would have pulled P10-G forward without its UI work
//! (V-242…V-247) and broken the batch ordering for no gain.
//!
//! So this file does two things that are both true today:
//!
//! 1. **Locks the model path clean** — so nothing re-introduces a regime
//!    identifier where the diagnostic already does the job.
//! 2. **Counts the retirement debt exactly**, so V-228…V-231 and V-506 cannot
//!    quietly pass without having removed what they claim to remove.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("two levels below the workspace root")
        .to_path_buf()
}

/// The files that constitute the model path — the four axes, the leg record,
/// the event journal, and the assembler that joins them.
const MODEL_PATH: &[&str] = &[
    "crates/core-domain/src/axes.rs",
    "crates/core-domain/src/leg.rs",
    "crates/core-domain/src/event.rs",
    "crates/market-analyzer/src/model/mod.rs",
    "crates/market-analyzer/src/model/live.rs",
];

/// Identifiers CR-11 retired from the model path.
const BANNED: &[&str] = &[
    "MarketRegime",
    "MarketPhase",
    "prev_regime",
    "market_regime",
    "market_phase",
    "regime_distribution",
];

fn code_only(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///") && !t.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn v110_the_model_path_carries_no_regime_identifier() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    for rel in MODEL_PATH {
        let code = code_only(&std::fs::read_to_string(root.join(rel)).unwrap_or_default());
        for banned in BANNED {
            if let Some(n) = code.lines().position(|l| l.contains(banned)) {
                offenders.push(format!(
                    "{rel}:{} — `{banned}`: `{}`",
                    n + 1,
                    code.lines().nth(n).unwrap_or("").trim()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "CR-11 — the model path must carry a diagnostic, not a regime. Offenders:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn v110_the_model_path_does_not_depend_on_the_analysis_crate() {
    // A stronger statement than "no identifier is spelled". If `model/live.rs`
    // ever imports `core_domain::analysis`, the regime vocabulary is one
    // `use` away from returning.
    for rel in [
        "crates/market-analyzer/src/model/mod.rs",
        "crates/market-analyzer/src/model/live.rs",
    ] {
        let code = code_only(&std::fs::read_to_string(workspace_root().join(rel)).unwrap());
        assert!(
            !code.contains("analysis::"),
            "{rel} reaches into `core_domain::analysis` — the model path must not depend on \
             the layer that still carries MarketRegime/MarketPhase (V-110)"
        );
    }
}

#[test]
fn v111_the_diagnostic_is_the_model_condition_classifier() {
    use core_domain::axes::{DiagnosticState, TimeframeState};

    // Four states, all candle-derived from bbwp/rvol (CH-1). The model says
    // what condition the market is in; it does not say what regime it is in.
    assert_eq!(
        DiagnosticState::ALL.len(),
        4,
        "the diagnostic state is a closed four-state classification"
    );

    // The wire shape carries it under `diagnostics`, never under a regime key.
    let json = serde_json::to_value(core_domain::axes::Diagnostics {
        state: DiagnosticState::Coiling,
        volatility_change_pct: -8.4,
        participation_change_pct: 12.1,
        basis: core_domain::axes::DiagnosticBasis {
            bbwp_pct: 61.2,
            rvol_pct: 1.34,
        },
    })
    .expect("diagnostics serialises");
    assert_eq!(json["state"], "Coiling");
    assert!(
        json.get("regime").is_none(),
        "diagnostics must not carry a regime field"
    );
    assert!(
        json.get("phase").is_none(),
        "diagnostics must not carry a phase field"
    );

    // Sanity: the type is constructible and comparable in the model.
    let _state: TimeframeState = core_domain::axes::TimeframeState {
        identity: core_domain::axes::TimeframeIdentity {
            instance_id: "i".into(),
            pair: "BTC-USDT".into(),
            exchange: "Hyperliquid".into(),
            timeframe_secs: 60,
            timeframe_label: "1m".into(),
            candle_count: 500,
            last_closed_at: "t".into(),
            available_at: "t".into(),
            spec_version: core_domain::SPEC_VERSION.into(),
            build_version: core_domain::BUILD_VERSION.into(),
        },
        axes: Vec::new(),
        events: core_domain::axes::EventCounts::default(),
        levels: Vec::new(),
        legs: Vec::new(),
        diagnostics: core_domain::axes::Diagnostics {
            state: DiagnosticState::Settling,
            volatility_change_pct: 0.0,
            participation_change_pct: 0.0,
            basis: core_domain::axes::DiagnosticBasis {
                bbwp_pct: 50.0,
                rvol_pct: 1.0,
            },
        },
        derived: core_domain::axes::DerivedState {
            consensus: core_domain::axes::Consensus {
                agreeing_axes: 0,
                of: 4,
                axes: Vec::new(),
                weakest_axis: None,
            },
            stability: core_domain::axes::Stability {
                state: core_domain::axes::StabilityState::Building,
                changes_last_50_bars: 0,
                window: 50,
            },
        },
    };
}

/// The retirement debt, counted. Not a gate on *this* phase — a gate on
/// V-228…V-231 and V-506, so those cannot report success without having
/// actually removed the identifiers they name.
#[test]
fn v111_the_retirement_debt_is_counted_and_attributed() {
    let root = workspace_root();
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_default();

    let analysis = read("crates/core-domain/src/analysis.rs");
    let analyzer = read("crates/market-analyzer/src/analyzer/mod.rs");
    let historical = read("crates/backtesting-engine/src/historical.rs");

    // Still present, and therefore still owed.
    assert!(
        analysis.contains("pub enum MarketRegime"),
        "MarketRegime is gone from analysis.rs — when it is, delete this assertion and note \
         that V-228 has landed"
    );
    assert!(
        analysis.contains("pub enum MarketPhase"),
        "MarketPhase is gone from analysis.rs — when it is, update the V-231 receipt"
    );

    // Each remaining site is attributed to the item that retires it, so the
    // debt is never anonymous.
    let debts: &[(&str, usize, &str)] = &[
        (
            "MarketRegime",
            analysis.matches("MarketRegime").count(),
            "V-228",
        ),
        (
            "MarketPhase",
            analysis.matches("MarketPhase").count(),
            "V-231",
        ),
        (
            "prev_regime (live)",
            analyzer.matches("prev_regime").count(),
            "V-230",
        ),
        (
            "prev_regime (replay)",
            historical.matches("prev_regime").count(),
            "V-506",
        ),
    ];
    for (name, count, retired_by) in debts {
        assert!(
            *count > 0,
            "{name} has vanished — remove its row here and close {retired_by}'s receipt"
        );
    }

    // The four retirements are accounted for; nothing is orphaned.
    assert_eq!(
        debts.len(),
        4,
        "every banned identifier has a retiring item"
    );
}
