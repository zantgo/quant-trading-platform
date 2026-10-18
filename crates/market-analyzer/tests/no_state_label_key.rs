//! **V-108 — `state_label` is display output, never a decision input.**
//!
//! # The defect class
//!
//! `NormalizedIndicatorValue::state_label` is a `String` a normaliser writes
//! "for frontend rendering / logging". Six production sites were branching on it:
//!
//! ```
//! v.state_label.contains("CROSSOVER")              api-core/src/types.rs
//! v.state_label.contains("COMPRESSION")            core-domain/src/risk.rs
//! sr.state_label.contains("DEMAND") || … "SUPPORT" core-domain/src/alignment.rs
//! v.state_label == "COMPRESSION_COILING"           market-analyzer/analyzer/mod.rs
//! match value.state_label…as_str() { … }           market-analyzer/model/live.rs
//! ```
//!
//! Every one of those is a decision made on a **word**, not on a value. Rename
//! the word in the normaliser and the decision silently changes meaning; the
//! type system sees nothing because the type is `String`.
//!
//! # What this file locks
//!
//! The Model crates are swept. `state_label` may still be *produced* and
//! *displayed*, but in `market-analyzer` — the crate that owns the model — it may
//! not be compared, matched, or used as a branch condition, with **one**
//! sanctioned exception: [`Stage::from_label`], the workspace's single
//! label→ordinal door.
//!
//! The remaining sites in `api-core`, `core-domain::risk`, `core-domain::alignment`
//! and `analyzer/mod.rs` are **not** swept here. They are retired by their own
//! phases — V-225 replaces `alignment.rs` wholesale and V-228…V-231 dissolve
//! `AnalysisMatrix` — and sweeping them now would assert a claim that is not yet
//! true rather than a claim that is.
//!
//! **Mutation M11** re-introduces a `state_label` comparison in the model path
//! and demonstrates that this file fires. See `M11_STATE_LABEL_KEY.md`.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("two levels below the workspace root")
        .to_path_buf()
}

/// Model-crate sources that are governed.
///
/// `model/live.rs` is included on purpose — it is the file that HAD the
/// inlined stage-label `match`, and it remains the natural place for someone to
/// add another one.
fn governed() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut out = Vec::new();
    for sub in ["model", "leg", "replay", "indicators"] {
        let dir = root.join("crates/market-analyzer/src").join(sub);
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        walk(&dir, &mut out);
    }
    out
}

/// Line-comment and block-comment stripper, so a doc comment *describing* the
/// defect does not trip the sweep that forbids it.
fn code_only(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut block = false;
    let mut in_string = false;
    while i < bytes.len() {
        let b = bytes[i];
        if block {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block = false;
                i += 2;
            } else {
                if b == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                block = true;
                i += 2;
            }
            b'"' => {
                in_string = true;
                i += 1;
            }
            _ => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

/// The shapes that make a free-text label into a decision.
const COMPARISON_SHAPES: &[(&str, &str)] = &[
    (".state_label ==", "`==` on state_label"),
    ("== .state_label", "`==` on state_label"),
    (".state_label.contains(", "`.contains()` on state_label"),
    (
        ".state_label.starts_with(",
        "`.starts_with()` on state_label",
    ),
    ("match .*state_label", "`match` on state_label"),
    (".state_label.split(", "`.split()` on state_label"),
];

#[test]
fn v108_no_model_code_branches_on_a_state_label() {
    let mut offenders = Vec::new();
    for path in governed() {
        let rel = path.strip_prefix(workspace_root()).unwrap_or(&path);
        let code = code_only(&std::fs::read_to_string(&path).unwrap_or_default());
        for (shape, why) in COMPARISON_SHAPES {
            // `state_label` in a call that only *reports* it is fine; the sweep
            // is for the shapes that decide.
            if let Some(line_no) = code.lines().position(|l| l.contains(shape)) {
                let line = code.lines().nth(line_no).unwrap_or("").trim();
                offenders.push(format!(
                    "{}:{} — {why}: `{line}`",
                    rel.display(),
                    line_no + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "state_label is display output, not a decision input (V-108). Offenders:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn v108_the_one_sanctioned_reader_is_on_the_type() {
    // There is exactly one door: `Stage::from_label`. It lives on `Stage`, so
    // the reader of a label is a property of the ordinal type rather than an
    // accident of a caller's module.
    let axes = std::fs::read_to_string(workspace_root().join("crates/core-domain/src/axes.rs"))
        .expect("axes.rs readable");
    let start = axes
        .find("pub fn from_label(label: &str) -> Option<Stage>")
        .expect("Stage::from_label exists — it is the single label→ordinal door");
    let body = &axes[start..];
    assert!(
        body.contains("\"early_mid\""),
        "the parser must name the five anchors"
    );
    for anchor in ["early", "mid", "late", "terminal"] {
        assert!(
            body.contains(&format!("\"{anchor}\"")),
            "the parser lost the {anchor} anchor"
        );
    }
}

#[test]
fn v108_stage_labels_round_trip_and_unknown_is_absent() {
    use core_domain::axes::Stage;

    // Every anchor parses from its own canonical spelling.
    for s in Stage::ALL {
        assert_eq!(
            Stage::from_label(s.label()),
            Some(s),
            "{s:?} does not round-trip through its published label"
        );
    }
    // Producer spelling variants still land, because they are normalised once.
    assert_eq!(Stage::from_label("Early Mid"), Some(Stage::EarlyMid));
    assert_eq!(Stage::from_label("early-mid"), Some(Stage::EarlyMid));
    assert_eq!(Stage::from_label("  MID  "), Some(Stage::Mid));
    assert_eq!(Stage::from_label("middle"), Some(Stage::Mid));

    // Anything outside the closed set is typed absence, never a nearest guess.
    for unknown in ["", "coiling", "compressing", "late_stage", "5", "NaN"] {
        assert_eq!(
            Stage::from_label(unknown),
            None,
            "{unknown:?} must be absence — guessing the nearest anchor is how an ordinal \
             silently becomes wrong"
        );
    }
}

#[test]
fn v108_the_closed_set_is_exactly_five() {
    use core_domain::axes::Stage;
    assert_eq!(Stage::ALL.len(), 5, "the ordinal anchors are five, always");
    // Distinct canonical labels, so no two anchors are spelled the same way.
    let mut labels: Vec<&str> = Stage::ALL.iter().map(|s| s.label()).collect();
    labels.sort_unstable();
    let before = labels.len();
    labels.dedup();
    assert_eq!(labels.len(), before, "two anchors share a published label");
}
