//! **N1 — no assembler without a production caller** (V-216).
//!
//! A pure function that every test exercises and no binary calls is a
//! specification with no product. The three reductions were exactly that
//! through P10-C: `assemble_cross_tf_state`, `assemble_cross_pair_state` and
//! `assemble_model_state` had zero callers outside `#[cfg(test)]`.
//!
//! This scans the **source tree**, not the symbol graph, so deleting the
//! production call while keeping the definition is a failing test rather than a
//! silent regression. `P10-D` wires all three through the daemon's L7 task.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn src_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            src_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Count the call sites outside `#[cfg(test)]` modules in every crate's `src/`.
///
/// A naive substring count is deliberate: it catches the definition too, so the
/// threshold is "definition + at least one production call". The scan excludes
/// `tests/` directories by construction — a test-only caller cannot satisfy N1.
fn production_occurrences(name: &str) -> usize {
    let crates_dir = workspace_root().join("crates");
    let mut files = Vec::new();
    // Only `<crate>/src/**`: integration tests (`<crate>/tests/`) and the
    // target directory can never satisfy N1.
    for entry in std::fs::read_dir(&crates_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let src = entry.path().join("src");
        if src.is_dir() {
            src_files(&src, &mut files);
        }
    }
    let needle = format!("{name}(");
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap_or_default())
        .map(|s| {
            // Remove the test modules so an assertion in a unit test cannot
            // count as a caller. This is a cheap lexical cut, not a parser:
            // every crate keeps unit tests after `#[cfg(test)]`.
            let production = match s.find("#[cfg(test)]") {
                Some(i) => &s[..i],
                None => &s[..],
            };
            production.matches(&needle).count()
        })
        .sum()
}

#[test]
fn every_reduction_has_a_production_caller() {
    for assembler in [
        "assemble_cross_tf_state",
        "assemble_cross_pair_state",
        "assemble_model_state",
    ] {
        let count = production_occurrences(assembler);
        assert!(
            count >= 2,
            "N1: `{assembler}` has the definition but no production call site \
             (found {count}; the threshold is definition + >=1 caller in crates/*/src)"
        );
    }
}
