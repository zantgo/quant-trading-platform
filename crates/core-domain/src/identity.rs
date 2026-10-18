//! Compile-time identity of the running code (V-101, V-102, V-103; invariant **N4**).
//!
//! Three constants, three different questions. They are deliberately **not**
//! interchangeable, and the distinction is what makes a reproducibility check
//! possible at all.
//!
//! | constant        | question it answers                              | moves when |
//! |-----------------|--------------------------------------------------|------------|
//! | `SPEC_VERSION`  | *which specification does this code implement?*   | the spec changes |
//! | `CODE_VERSION`  | *which release line is this?*                     | a release is cut |
//! | `BUILD_VERSION` | *which exact commit produced these bytes?*        | anything commits |
//!
//! # The two defects this replaced
//!
//! **1. `spec_version` was `env!("CARGO_PKG_VERSION")`.** Every crate declared
//! `version = "0.1.0"`, so every published model document claimed
//! `spec_version: "0.1.0"` — the *cargo* version, in a field whose name promises
//! the *specification* version. Since all fifteen crates carried the same string,
//! two builds of two different specifications were indistinguishable.
//!
//! **2. `build_version` was `option_env!("GIT_BUILD_ID").unwrap_or("dev")`.**
//! There was no `build.rs` anywhere in the workspace and no script set that
//! variable, so `"dev"` was not a degraded fallback — it was the *only*
//! reachable value. A stale tree and a freshly built tree published byte-identical
//! identity fields, which is exactly the condition **CH-8** (determinism) and the
//! **V-46** reproducibility banner are supposed to exclude. `build.rs` now reads
//! the SHA and *fails the build* if it cannot, because a binary that cannot name
//! itself cannot be checked.
//!
//! # Invariant N4
//!
//! `spec_version` and `build_version` are resolved from the specification line
//! and from git respectively — **never** from `CARGO_PKG_VERSION`, and never from
//! a literal. [`tests/version_identity.rs`](../../tests/version_identity.rs)
//! enforces this by reading the sources back, and mutation **M9** (replacing
//! `BUILD_VERSION` with a hardcoded string) is the named demonstration that the
//! check would fail.

/// The version of the **specification** this build implements.
///
/// Distinct from [`CODE_VERSION`] on purpose: the docs line and the code line
/// advance independently, and a reader comparing two documents must be able to
/// tell "the specification moved" from "the binary moved".
pub const SPEC_VERSION: &str = env!("PLATFORM_SPEC_VERSION");

/// The version of the **code** — the release line, the one a git tag names.
pub const CODE_VERSION: &str = env!("PLATFORM_CODE_VERSION");

/// The exact commit that produced these bytes (`git rev-parse --short HEAD`).
///
/// Never `"dev"`, never `"unknown"`: `build.rs` refuses to emit a build whose
/// identity it cannot establish.
pub const BUILD_VERSION: &str = env!("PLATFORM_BUILD_VERSION");

/// Convenience for a document header that wants both lines.
///
/// Format is `spec/build` — e.g. `12.0.0/b3e2e4a5` — which is unambiguous to
/// read and sortable when two documents are diffed.
pub fn version_banner() -> String {
    format!("{SPEC_VERSION}/{BUILD_VERSION}")
}

/// The identity pair a published document carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildIdentity {
    pub spec_version: &'static str,
    pub build_version: &'static str,
}

/// The current process's identity, as stamped into a published document.
pub fn build_identity() -> BuildIdentity {
    BuildIdentity {
        spec_version: SPEC_VERSION,
        build_version: BUILD_VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constant_is_a_placeholder() {
        // The two defects this module exists to prevent. A regression to either
        // is silent -- the code still compiles and the document still serialises
        // -- so it is pinned here as well as by the source-reading test.
        assert_ne!(
            SPEC_VERSION, "dev",
            "spec_version regressed to a placeholder"
        );
        assert_ne!(
            SPEC_VERSION, "unknown",
            "spec_version regressed to a placeholder"
        );
        assert_ne!(
            SPEC_VERSION, "0.1.0",
            "spec_version regressed to the old cargo version"
        );
        assert_ne!(
            BUILD_VERSION, "dev",
            "build_version regressed to the unwinnable GIT_BUILD_ID fallback"
        );
        assert_ne!(
            BUILD_VERSION, "unknown",
            "build_version could not be resolved -- build.rs must fail instead"
        );
    }

    #[test]
    fn build_version_looks_like_a_sha() {
        // git's `--short` output: 7+ lowercase hex characters.
        assert!(
            BUILD_VERSION.len() >= 7,
            "build_version {BUILD_VERSION:?} is not a git short SHA"
        );
        assert!(
            BUILD_VERSION.chars().all(|c| c.is_ascii_hexdigit()),
            "build_version {BUILD_VERSION:?} contains non-hex characters"
        );
    }

    #[test]
    fn spec_and_code_lines_are_distinct() {
        // If these ever collapse into one value, the distinction the whole
        // two-line scheme exists to express has been lost.
        assert_ne!(
            SPEC_VERSION, CODE_VERSION,
            "the spec and code version lines collapsed into one"
        );
    }

    #[test]
    fn banner_names_both_lines() {
        let banner = version_banner();
        assert!(banner.contains(SPEC_VERSION));
        assert!(banner.contains(BUILD_VERSION));
    }
}
