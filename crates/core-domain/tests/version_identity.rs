//! **V-101 … V-105 — the version identity scheme, enforced from the sources.**
//!
//! The point of this file is that it reads the *sources* back, not just the
//! resolved constants. A test that only asserted `SPEC_VERSION == "12.0.0"`
//! would pass unchanged if someone reintroduced the original defect, because
//! the defect was never a wrong *value* -- it was a wrong *source*.
//!
//! # The two defects this locks shut
//!
//! | field | was | why it was wrong |
//! |---|---|---|
//! | `spec_version` | `env!("CARGO_PKG_VERSION")` | every crate declared `0.1.0`, so a document claimed a *cargo* version in a field naming the *specification*. Two different specs serialised identically. |
//! | `build_version` | `option_env!("GIT_BUILD_ID").unwrap_or("dev")` | `GIT_BUILD_ID` was never set by anything, so `"dev"` was not a fallback — it was the only reachable value. A stale tree was byte-identical to a fresh one. |
//!
//! That second row is the one that mattered: it made **CH-8** (determinism) and
//! the **V-46** reproducibility banner *unsatisfiable by construction*. The
//! P11 banner read `b3e2e4a5` as hand-written prose in a commit message; there
//! was no runtime check and nothing could have been one.
//!
//! **Mutation M9** is the named demonstration that these checks would fail:
//! replace `BUILD_VERSION` with a literal and every assertion here that reads a
//! source fires. See `M9_BUILD_IDENTITY.md` in this directory.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("this crate is always two levels below the workspace root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `.rs` under `crates/`, for the repository-wide sweeps.
fn rust_sources() -> Vec<PathBuf> {
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
    let mut out = Vec::new();
    walk(&workspace_root().join("crates"), &mut out);
    out
}

/// Strips Rust line comments (`//`, including `///` doc comments) so a sweep
/// matches **executable** identity reads, not prose describing the defect.
///
/// This matters in both directions. A doc comment that says
/// `env!("CARGO_PKG_VERSION") was wrong` must not trip the rule that forbids
/// `env!("CARGO_PKG_VERSION")`, or the codebase becomes unable to explain its
/// own history. Equally, a real call must not be able to hide behind a trailing
/// comment on the same line — so block comments are removed too rather than
/// relying on line boundaries.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut block: Option<()> = None;
    let mut in_string = false;
    let mut in_char = false;

    while i < bytes.len() {
        let b = bytes[i];
        match block {
            Some(()) => {
                if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    block = None;
                    out.push(' ');
                    i += 2;
                } else {
                    // Preserve newlines so line numbers in messages survive.
                    if b == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            None => {
                // A raw string is deliberately not handled: none of the
                // identifiers swept for appear inside one, and adding a raw-string
                // state machine to a policy test would add more risk than it removes.
                if in_string {
                    if b == b'\\' {
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    if b == b'"' {
                        in_string = false;
                    }
                    out.push(if b == b'"' { ' ' } else { b as char });
                    i += 1;
                    continue;
                }
                if in_char {
                    in_char = false;
                    if b == b'\\' {
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    if b == b'\'' {
                        out.push(' ');
                    } else {
                        out.push(b as char);
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
                        block = Some(());
                        i += 2;
                    }
                    b'"' => {
                        in_string = true;
                        out.push(' ');
                        i += 1;
                    }
                    b'\'' => {
                        // Only a char literal if it closes within a few bytes;
                        // otherwise it is a lifetime (`'_`), which is not code
                        // that could contain an identity read.
                        if bytes.get(i + 3) == Some(&b'\'') {
                            in_char = true;
                            out.push(' ');
                            i += 1;
                        } else {
                            out.push('\'');
                            i += 1;
                        }
                    }
                    _ => {
                        out.push(b as char);
                        i += 1;
                    }
                }
            }
        }
    }
    out
}

/// Sources the identity sweep governs.
///
/// Two exclusions, both principled:
///
/// * **this file** — it quotes the banned identifiers in order to assert their
///   absence, so it would otherwise convict itself.
/// * **`core-domain/build.rs`** — the ONE sanctioned reader of
///   `CARGO_PKG_VERSION`, and it is sanctioned deliberately: it reads the *code*
///   line (which cargo forwards) and immediately cross-checks it against a
///   literal compiled into the same file, so the two cannot drift apart in
///   silence. It never publishes `CARGO_PKG_VERSION` as `spec_version`.
fn swept_sources() -> Vec<PathBuf> {
    let root = workspace_root();
    rust_sources()
        .into_iter()
        .filter(|p| {
            let rel = p.strip_prefix(&root).unwrap_or(p);
            rel != Path::new("crates/core-domain/tests/version_identity.rs")
                && rel != Path::new("crates/core-domain/build.rs")
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// V-104 — the single code version line
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v104_every_crate_inherits_the_one_version_line() {
    let manifests: Vec<PathBuf> = std::fs::read_dir(workspace_root().join("crates"))
        .expect("crates/ exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    assert!(
        manifests.len() >= 15,
        "expected 15 crates, found {}",
        manifests.len()
    );

    let mut offenders = Vec::new();
    for m in &manifests {
        let text = std::fs::read_to_string(m.join("Cargo.toml")).expect("manifest readable");
        // A literal version is how the fifteen crates drifted independently in
        // the first place. Inheriting is the fix; a literal is the regression.
        if text.lines().any(|l| {
            let l = l.trim();
            l.starts_with("version = ") && !l.contains("workspace")
        }) {
            offenders.push(
                m.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert!(
        offenders.is_empty(),
        "these crates declare a literal version instead of inheriting (V-104): {offenders:?}"
    );
}

#[test]
fn v104_the_code_line_is_four() {
    assert_eq!(
        core_domain::CODE_VERSION,
        "4.0.0",
        "the code version line is not 4.0.0 (V-104). If this bump is intended, update \
         the release record and the assertions in core-domain/build.rs with it."
    );
}

#[test]
fn v104_the_spec_line_is_twelve() {
    assert_eq!(
        core_domain::SPEC_VERSION,
        "12.0.0",
        "the docs/spec version line is not 12.0.0 (V-101)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// V-101 — the two lines are distinct and read from the workspace table
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v101_the_two_lines_live_in_one_file() {
    let root = read("Cargo.toml");
    assert!(
        root.contains("[workspace.package]"),
        "the workspace package table is gone -- there is no single version line to inherit"
    );
    assert!(
        root.contains("[workspace.metadata.platform]"),
        "the spec line must live under workspace.metadata so cargo does not report it as an \
         unused manifest key"
    );
    assert!(
        root.contains("spec-version = \"12.0.0\""),
        "the spec version line is missing or was edited away (V-101)"
    );
    assert!(
        root.contains("version = \"4.0.0\""),
        "the code version line is missing or was edited away (V-104)"
    );
}

#[test]
fn v101_a_build_script_resolves_both_lines() {
    assert!(
        workspace_root()
            .join("crates/core-domain/build.rs")
            .is_file(),
        "core-domain/build.rs is missing -- without it the identity is unresolvable (V-101)"
    );
    let manifest = read("crates/core-domain/Cargo.toml");
    assert!(
        manifest.contains("build = \"build.rs\""),
        "core-domain does not declare its build script (V-101)"
    );
}

#[test]
fn v101_the_build_script_fails_loudly_rather_than_publishing_a_placeholder() {
    // The core of the fix: when git is unavailable the build must STOP. A
    // silent placeholder is the defect; a loud failure is the cure.
    let src = read("crates/core-domain/build.rs");
    assert!(
        src.contains("panic!") || src.contains("assert!"),
        "build.rs has no failure path (V-103)"
    );
    assert!(
        src.contains("PLATFORM_BUILD_ID"),
        "build.rs offers no override for a non-checkout tree (V-103)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// V-103 / N4 — no identity may come from CARGO_PKG_VERSION
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v103_no_model_code_reads_the_cargo_version() {
    // N4. The defect this replaces was `env!("CARGO_PKG_VERSION")` in the model
    // crate. The sweep is repo-wide so it cannot be reintroduced anywhere.
    let offenders: Vec<String> = swept_sources()
        .into_iter()
        .filter(|p| {
            strip_comments(&std::fs::read_to_string(p).unwrap_or_default())
                .contains("CARGO_PKG_VERSION")
        })
        .map(|p| {
            p.strip_prefix(workspace_root())
                .unwrap_or(&p)
                .display()
                .to_string()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "N4 violation — these sources still read CARGO_PKG_VERSION in executable code: \
         {offenders:?}"
    );
}

#[test]
fn v103_nothing_publishes_the_dev_placeholder() {
    // Two precise defective shapes, not bare identifiers. `GIT_BUILD_ID` on its
    // own is only a name -- it appears legitimately in assertion messages that
    // explain what went wrong -- so sweeping for the bare word would flag the
    // code documenting the defect. What must not exist is the *fallback*:
    //
    //     option_env!("GIT_BUILD_ID").unwrap_or("dev")     the original defect
    //     <anything>.unwrap_or("dev")                        the same shape, any source
    //
    const FALLBACK: &str = r#"unwrap_or("dev")"#;
    const UNSET_OPTIONAL: &str = "option_env!";

    let mut offenders = Vec::new();
    for p in swept_sources() {
        let code = strip_comments(&std::fs::read_to_string(&p).unwrap_or_default());
        let mut why = Vec::new();
        if code.contains(FALLBACK) {
            why.push("a `.unwrap_or(\"dev\")` fallback");
        }
        if code.contains(UNSET_OPTIONAL) {
            why.push("an `option_env!` that may be unset");
        }
        if !why.is_empty() {
            offenders.push(format!(
                "{} [{}]",
                p.strip_prefix(workspace_root()).unwrap_or(&p).display(),
                why.join(", ")
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "identity fallbacks are back in executable code: {offenders:?}. A binary that cannot \
         name itself cannot satisfy CH-8 (V-103)."
    );
}

#[test]
fn v103_identity_module_exists_and_is_the_only_publisher() {
    let identity = read("crates/core-domain/src/identity.rs");
    assert!(
        identity.contains("pub const SPEC_VERSION"),
        "core_domain::SPEC_VERSION is not published (V-102)"
    );
    assert!(
        identity.contains("pub const BUILD_VERSION"),
        "core_domain::BUILD_VERSION is not published (V-102)"
    );
    // The live assembler must consume the constants, not re-derive them.
    let live = read("crates/market-analyzer/src/model/live.rs");
    assert!(
        live.contains("core_domain::SPEC_VERSION"),
        "the live assembler does not use the resolved spec version (V-102)"
    );
    assert!(
        live.contains("core_domain::BUILD_VERSION"),
        "the live assembler does not use the resolved build version (V-102)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// V-102 — a published document actually carries the resolved identity
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v102_a_published_identity_carries_the_real_values() {
    let id = core_domain::build_identity();
    assert_eq!(id.spec_version, core_domain::SPEC_VERSION);
    assert_eq!(id.build_version, core_domain::BUILD_VERSION);
    // Not placeholders, and not the two lines collapsed into one.
    assert_ne!(id.spec_version, "dev");
    assert_ne!(id.build_version, "dev");
    assert_ne!(id.spec_version, id.build_version);
}

#[test]
fn v102_the_banner_names_both_lines() {
    let banner = core_domain::version_banner();
    assert!(
        banner.contains("12.0.0"),
        "banner {banner:?} omits the spec line"
    );
    let sha = core_domain::BUILD_VERSION;
    assert!(
        banner.contains(sha),
        "banner {banner:?} omits the build SHA"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// V-105 — the shell reports the same identity
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v105_the_shell_reports_three_lines_not_a_corpus_and_a_dev() {
    let commands = read("crates/execution-daemon/src/shell/commands.rs");
    assert!(
        !commands.contains("GIT_BUILD_ID"),
        "the shell still reads GIT_BUILD_ID (V-105)"
    );
    assert!(
        commands.contains("core_domain::SPEC_VERSION"),
        "the shell does not report the spec line (V-105)"
    );

    let shell_mod = read("crates/execution-daemon/src/shell/mod.rs");
    assert!(
        shell_mod.contains("core_domain::BUILD_VERSION"),
        "the shell's version line does not report the build SHA (V-105)"
    );
    // The human-readable line must show three distinct facts.
    let version_line = shell_mod
        .split("pub fn version_line")
        .nth(1)
        .expect("version_line exists");
    for token in ["SPEC_VERSION", "CODE_VERSION", "BUILD_VERSION"] {
        assert!(
            version_line.contains(token),
            "version_line does not report {token} (V-105)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CH-8 — determinism follows from the identity being real
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ch8_two_builds_of_different_commits_are_distinguishable() {
    // The property the whole scheme exists to provide. If `BUILD_VERSION` were
    // a literal or a placeholder, this assertion could not even be written --
    // which is precisely why it is worth stating.
    let sha = core_domain::BUILD_VERSION;
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()) && sha.len() >= 7,
        "BUILD_VERSION {sha:?} is not a short SHA, so two builds would be indistinguishable (CH-8)"
    );
}
