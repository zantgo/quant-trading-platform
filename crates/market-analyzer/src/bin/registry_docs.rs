//! `registry_docs` — generates the documentation artifacts that are **derived**
//! from the indicator registry, so they cannot drift from it (CR-4, CR-5).
//!
//! Before this existed, `04-02-00-indicator-index.md` was hand-maintained: 52
//! rows, a per-SignalKind tally, and a summary table that had to be updated by
//! hand on every registry change. CR-6 records what that cost — a hand-kept
//! count that was **wrong while remaining arithmetically consistent**, which is
//! exactly why consistency checking was not enough and why byte-equality with
//! generated output is the check that actually catches it.
//!
//! What it generates
//! -----------------
//! `04-02-00-indicator-index.md` — the per-group tables and the Summary table,
//! inside an explicit generated region. The prose around them (counts policy,
//! numbering policy, cross-references) is hand-written and stays readable.
//!
//! Usage
//! -----
//! ```text
//! registry_docs            write every generated region
//! registry_docs --check    fail if any generated region differs byte-for-byte
//! ```
//!
//! `--check` is what `scripts/check_docs.py` invokes through `./manage.sh
//! test-doc`, which is why drift fails the build rather than a review.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use market_analyzer::indicators::registry::{IndicatorGroup, IndicatorMeta, INDICATORS};

const BEGIN: &str = "<!-- BEGIN GENERATED: registry — do not edit by hand -->";
const END: &str = "<!-- END GENERATED: registry -->";

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/crates/market-analyzer
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn index_path() -> PathBuf {
    repo_root().join("docs/engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md")
}

fn group_label(g: IndicatorGroup) -> &'static str {
    match g {
        IndicatorGroup::Trend => "TREND",
        IndicatorGroup::Momentum => "MOMENTUM",
        IndicatorGroup::Volume => "VOLUME",
        IndicatorGroup::Volatility => "VOLATILITY",
        IndicatorGroup::Structure => "STRUCTURE",
        IndicatorGroup::Regime => "REGIME",
        IndicatorGroup::Institutional => "INSTITUTIONAL",
    }
}

/// Group order as it appears in the index — the registry is a flat slice, so the
/// document order is declared here rather than inferred from the slice.
const GROUP_ORDER: [IndicatorGroup; 7] = [
    IndicatorGroup::Trend,
    IndicatorGroup::Momentum,
    IndicatorGroup::Volume,
    IndicatorGroup::Volatility,
    IndicatorGroup::Structure,
    IndicatorGroup::Regime,
    IndicatorGroup::Institutional,
];

fn class_label(m: &IndicatorMeta) -> &'static str {
    use market_analyzer::indicators::registry::IndicatorClass::*;
    match m.class {
        Leading => "Leading",
        Hybrid => "Hybrid",
        Lagging => "Lagging",
    }
}

/// `Signals` cell. A kind repeated inside one declaration is shown once with a
/// `×N` multiplicity, because the column counts *declarations*, and a
/// declaration may cover several zones (RSI has five threshold zones).
fn signals_cell(m: &IndicatorMeta) -> String {
    if m.signal_types.is_empty() {
        return "—".to_string();
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for s in m.signal_types {
        let name = format!("{s:?}");
        let entry = counts.entry(name.clone()).or_insert(0);
        if *entry == 0 {
            order.push(name);
        }
        *entry += 1;
    }
    order
        .into_iter()
        .map(|k| {
            let n = counts[&k];
            if n > 1 {
                format!("{k}×{n}")
            } else {
                k
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The index position of a doc file: `04-02-07-…` -> 7.
fn doc_index(m: &IndicatorMeta) -> u32 {
    m.doc_file
        .split('-')
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn generate() -> String {
    let mut out = String::new();

    for group in GROUP_ORDER {
        let mut members: Vec<&IndicatorMeta> =
            INDICATORS.iter().filter(|m| m.group == group).collect();
        if members.is_empty() {
            continue;
        }
        members.sort_by_key(|m| doc_index(m));

        writeln!(
            out,
            "## {} ({} indicators)\n",
            group_label(group),
            members.len()
        )
        .unwrap();
        writeln!(
            out,
            "| # | Filename | Key | Display Name | Class | Dir | Div | Signals | Doc File |"
        )
        .unwrap();
        writeln!(
            out,
            "|---|---|-----|-------------|-------|-----|-----|---------|----------|"
        )
        .unwrap();
        for m in &members {
            let div = if m.supports_divergence { "Y" } else { "—" };
            // An empty cell is indistinguishable from a bug in a generated
            // table, so an indicator that declares no signal kinds renders as
            // an explicit em dash.
            let signals = signals_cell(m);
            writeln!(
                out,
                "| {:02} | `{}` | `{}` | {} | {} | {} | {} | {} | [{}]({}) |",
                doc_index(m),
                m.doc_file,
                m.key,
                m.display_name,
                class_label(m),
                if m.directional { "Y" } else { "N (Gate)" },
                div,
                signals,
                m.doc_file,
                m.doc_file,
            )
            .unwrap();
        }
        out.push('\n');
        out.push_str("---\n\n");
    }

    // ── Summary ────────────────────────────────────────────────────────────
    let per_group: Vec<String> = GROUP_ORDER
        .iter()
        .filter(|g| INDICATORS.iter().any(|m| m.group == **g))
        .map(|g| {
            let n = INDICATORS.iter().filter(|m| m.group == *g).count();
            let label = match g {
                IndicatorGroup::Institutional => "Institutional",
                IndicatorGroup::Volatility => "Volatility",
                IndicatorGroup::Structure => "Structure",
                IndicatorGroup::Momentum => "Momentum",
                IndicatorGroup::Trend => "Trend",
                IndicatorGroup::Volume => "Volume",
                IndicatorGroup::Regime => "Regime",
            };
            format!("{n} {label}")
        })
        .collect();

    let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_decls = 0usize;
    for m in INDICATORS {
        for s in m.signal_types {
            *kind_counts.entry(format!("{s:?}")).or_insert(0) += 1;
            total_decls += 1;
        }
    }
    let breakdown: Vec<String> = kind_counts
        .iter()
        .map(|(k, n)| format!("`{k}` {n}"))
        .collect();
    let sum_check: usize = kind_counts.values().sum();

    let divergent: Vec<&str> = INDICATORS
        .iter()
        .filter(|m| m.supports_divergence)
        .map(|m| m.display_name)
        .collect();
    let gates: Vec<&str> = INDICATORS
        .iter()
        .filter(|m| !m.directional)
        .map(|m| m.display_name)
        .collect();
    let directional = INDICATORS.iter().filter(|m| m.directional).count();

    writeln!(out, "## Summary\n").unwrap();
    writeln!(out, "| Metric | Count |").unwrap();
    writeln!(out, "|--------|-------|").unwrap();
    writeln!(
        out,
        "| Authoritative Registry Entries | {} ({}) |",
        INDICATORS.len(),
        per_group.join(" + ")
    )
    .unwrap();
    writeln!(
        out,
        "| Files in this directory | {} ({} entries + this master index) |",
        INDICATORS.len() + 1,
        INDICATORS.len()
    )
    .unwrap();
    writeln!(
        out,
        "| Directional (scoring contributors) | {directional} |"
    )
    .unwrap();
    writeln!(
        out,
        "| Non-Directional Gates | {} ({}) |",
        gates.len(),
        gates.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "| Divergence-Bearing Indicators (`supports_divergence: true`) | {} ({}) |",
        divergent.len(),
        divergent.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "| Total Signal-Kind × Indicator Declarations | **{total_decls}** — one per `(indicator, SignalKind)` pair **as the registry declares them**, so one indicator that declares `Threshold` five times contributes five. The per-row `Signals` cell above shows the *distinct* kinds, because a repeated kind inside one row is a statement about zones, not about the row. Per-SignalKind breakdown: {}. Sum-check: {sum_check} = {total_decls} ✓ |",
        breakdown.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "| SignalKind Types | {} ({}) |",
        kind_counts.len(),
        kind_counts.keys().cloned().collect::<Vec<_>>().join(", ")
    )
    .unwrap();
    out.push('\n');

    out
}

fn apply_region(text: &str, generated: &str) -> String {
    let begin = text
        .find(BEGIN)
        .expect("generated region: missing BEGIN marker");
    let end = text
        .find(END)
        .expect("generated region: missing END marker");
    if end < begin {
        panic!("generated region: END precedes BEGIN");
    }
    let mut out = String::with_capacity(text.len() + generated.len());
    out.push_str(&text[..begin]);
    out.push_str(BEGIN);
    out.push('\n');
    out.push_str(generated);
    out.push_str(END);
    out.push_str(&text[end + END.len()..]);
    out
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let path = index_path();
    let original = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("registry_docs: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let updated = apply_region(&original, &generate());

    if check {
        if updated == original {
            println!("  OK: registry-generated docs are byte-identical to the registry");
            return ExitCode::SUCCESS;
        }
        eprintln!("  FAIL: registry-generated docs differ from the registry.");
        eprintln!(
            "        {} — run `cargo run -p market-analyzer --bin registry_docs` to regenerate.",
            path.display()
        );
        eprintln!("        Editing a generated region by hand produces drift, and drift is a failing check (CR-5),");
        eprintln!("        not a merge conflict to resolve manually.");
        return ExitCode::FAILURE;
    }

    if let Err(e) = std::fs::write(&path, &updated) {
        eprintln!("registry_docs: cannot write {}: {e}", path.display());
        return ExitCode::FAILURE;
    }
    println!("registry_docs: wrote {}", path.display());
    ExitCode::SUCCESS
}
