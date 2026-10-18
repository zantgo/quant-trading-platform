//! Compile-time identity resolution (V-101, V-103; invariant **N4**).
//!
//! Three values, three different questions. They are deliberately **not**
//! interchangeable, and the distinction is what makes a reproducibility check
//! possible at all.
//!
//! | value         | question it answers                             | moves when        |
//! |---------------|-------------------------------------------------|-------------------|
//! | `SPEC_VERSION`  | which **specification** does this code implement? | the spec changes  |
//! | `CODE_VERSION`  | which release line is this?                       | a release is cut  |
//! | `BUILD_VERSION` | which exact commit produced these bytes?          | anything commits  |
//!
//! # Why this is a build script and not two `const`s
//!
//! * `spec_version` MUST NOT be `CARGO_PKG_VERSION`. The two lines move
//!   independently (docs 12.0.0 vs code 4.0.0) and a reader must be able to
//!   distinguish "the specification changed" from "the binary changed".
//!   Before this existed the field was `env!("CARGO_PKG_VERSION")`, which was
//!   `0.1.0` for all fifteen crates -- so every build of every commit claimed
//!   the same specification and CH-8 could not be checked at all.
//!
//! * `build_version` MUST be the SHA, not a literal. It used to be
//!   `option_env!("GIT_BUILD_ID").unwrap_or("dev")` and `GIT_BUILD_ID` was
//!   never set by anything, so the fallback `"dev"` was not a degraded mode --
//!   it was the only reachable value, forever. That made the reproducibility
//!   banner (banner == `git rev-parse HEAD`) unsatisfiable by construction: a
//!   stale tree and a fresh tree published identical bytes.
//!
//! # N4
//!
//! Neither constant may be sourced from `CARGO_PKG_VERSION`, and `BUILD_VERSION`
//! may never be a literal. `crates/core-domain/tests/version_identity.rs`
//! enforces both by reading the sources back, and mutation **M9** (pinning
//! `BUILD_VERSION` to a literal) is the named demonstration that the check
//! would fail.
//!
//! # Reproducibility
//!
//! When the tree is not a git checkout (vendored source, a tarball) there is no
//! SHA to read. We **fail the build** rather than silently publishing a
//! placeholder: a binary that cannot name itself cannot satisfy CH-8, and a
//! build failure is a visible, fixable problem while a wrong `build_version` is
//! an invisible one. `PLATFORM_BUILD_ID` overrides the lookup and is how a
//! release tarball is stamped.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `SPEC_VERSION` and `CODE_VERSION` as authored in the workspace table.
const SPEC: &str = "12.0.0";
const CODE: &str = "4.0.0";

fn main() {
    println!("cargo:rerun-if-env-changed=PLATFORM_BUILD_ID");
    // The version lines are read from the root manifest, so a change there must
    // invalidate this build script.
    println!("cargo:rerun-if-changed=../../Cargo.toml");

    let workspace = read_workspace_versions();
    let build_version = resolve_build_version();

    assert!(
        !matches!(build_version.as_str(), "" | "dev" | "unknown"),
        "build_version resolved to {build_version:?}. Identity must be a real value: a binary \
         that cannot name itself cannot satisfy CH-8. Build inside the git checkout, or set \
         PLATFORM_BUILD_ID to the intended SHA."
    );
    assert_eq!(
        workspace.code, CODE,
        "the code version line moved off {CODE} without the release phase updating this \
         build script (V-104)."
    );
    assert_eq!(
        workspace.spec, SPEC,
        "the docs/spec version line moved off {SPEC} without the release phase updating this \
         build script (V-104)."
    );
    assert_ne!(
        workspace.spec, workspace.code,
        "the spec and code version lines collapsed into one value -- the distinction they exist \
         to express has been lost (V-101)."
    );

    println!("cargo:rustc-env=PLATFORM_SPEC_VERSION={}", workspace.spec);
    println!("cargo:rustc-env=PLATFORM_BUILD_VERSION={build_version}");
    println!("cargo:rustc-env=PLATFORM_CODE_VERSION={}", workspace.code);
}

struct WorkspaceVersions {
    spec: String,
    code: String,
}

/// Reads `[workspace.package] spec-version` and `version` out of the root
/// manifest.
///
/// Cargo only forwards a fixed set of `CARGO_PKG_*` variables to build scripts,
/// so an arbitrary key like `spec-version` cannot be inherited the way
/// `rust-version` is. Parsing the table directly is the alternative that keeps
/// both lines in ONE greppable file instead of scattering them across fifteen
/// manifests.
///
/// This is a deliberately narrow parse: it scans for the two keys inside the
/// `[workspace.package]` table and ignores everything else. The root manifest is
/// the one file in this workspace whose formatting is stable enough for that,
/// and a parse failure is a loud build error rather than a silent wrong value.
fn read_workspace_versions() -> WorkspaceVersions {
    let manifest = workspace_root().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap_or_else(|e| {
        panic!("cannot read {}: {e}", manifest.display());
    });

    let mut in_metadata = false;
    let mut spec: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // The spec line lives under `[workspace.metadata.platform]`
            // because cargo forwards only a fixed set of CARGO_PKG_* variables
            // to build scripts, and reports an arbitrary `[workspace.package]`
            // key as an "unused manifest key".
            in_metadata = line == "[workspace.metadata.platform]";
            continue;
        }
        if !in_metadata || line.starts_with('#') {
            continue;
        }
        // `key = "value"`, tolerating spaces around `=`.
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let value = rest
            .trim()
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or_default()
            .to_string();
        if key.trim() == "spec-version" {
            spec = Some(value);
        }
    }

    // The code line is forwarded by cargo, so it is taken from the environment
    // rather than parsed a second time. main() cross-checks it against `CODE`
    // so the two copies in this file cannot drift apart unnoticed.
    WorkspaceVersions {
        spec: spec.expect("root Cargo.toml [workspace.metadata.platform] has no `spec-version`"),
        code: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// The workspace root, derived from this crate's manifest directory rather
/// than from CWD (cargo runs build scripts with CWD set to the package root).
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/core-domain is always two levels below the workspace root")
        .to_path_buf()
}

fn resolve_build_version() -> String {
    // An explicit override wins: this is how a reproducible release build is
    // stamped when the tree is not a checkout.
    if let Ok(id) = std::env::var("PLATFORM_BUILD_ID") {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    // `--git-dir` is not used; instead the workspace root is passed explicitly
    // so a nested member does not have to guess where `.git` lives.
    let root = workspace_root();
    let head = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    match head {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        // No git, or git failed. Return the value the assert in main() rejects
        // with a diagnosable message, rather than a silent "dev".
        _ => "unknown".to_string(),
    }
}
