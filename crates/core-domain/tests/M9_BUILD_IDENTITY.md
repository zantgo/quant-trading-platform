# M9 — identity pinned to a literal

**Status:** injected → detected → repaired, this cycle.
**Invariant:** N4 (identity is compile-time resolved, never a literal).
**Gates:** `crates/core-domain/tests/version_identity.rs`,
`core_domain::identity` unit tests.

---

## 1. Named

| | |
|---|---|
| **Defect** | `BUILD_VERSION` is a hardcoded string instead of the git SHA the build script resolved |
| **Site** | `crates/core-domain/src/identity.rs:53` |
| **Injected form** | `pub const BUILD_VERSION: &str = "dev";` |
| **Correct form** | `pub const BUILD_VERSION: &str = env!("PLATFORM_BUILD_VERSION");` |

This is not a synthetic shape. It is **byte-for-byte the shipped defect this
phase set out to remove**: `build_version` was
`option_env!("GIT_BUILD_ID").unwrap_or("dev")`, `GIT_BUILD_ID` was never set by
anything, and `"dev"` was therefore the only value the platform ever published.
A stale tree and a freshly built tree serialised identical identity fields,
which is precisely the condition **CH-8** (determinism) and the **V-46**
reproducibility banner exist to exclude.

## 2. Isolated

Applied to a single constant in a scratch copy of `identity.rs`, never to
production at rest:

```
cp crates/core-domain/src/identity.rs /tmp/opencode/identity.rs.orig
# mutate the one line, run the gate, then restore from the saved copy
```

## 3. Demonstrated failing

**Four independent assertions fired** — not one, which is the property worth
recording. Each quotes the failing assertion verbatim.

```
---- ch8_two_builds_of_different_commits_are_distinguishable stdout ----
thread '...' panicked at crates/core-domain/tests/version_identity.rs:439:5:
  BUILD_VERSION "dev" is not a short SHA, so two builds would be
  indistinguishable (CH-8)
```

```
---- v102_a_published_identity_carries_the_real_values stdout ----
thread '...' panicked at crates/core-domain/tests/version_identity.rs:383:5:
assertion `left != right` failed
  left: "dev"
 right: "dev"
```

```
---- identity::tests::no_constant_is_a_placeholder stdout ----
thread '...' panicked at crates/core-domain/src/identity.rs:90:9:
assertion `left != right` failed: build_version regressed to the unwinnable
GIT_BUILD_ID fallback
  left: "dev"
 right: "dev"
```

```
---- identity::tests::build_version_looks_like_a_sha stdout ----
thread '...' panicked at crates/core-domain/src/identity.rs:103:9:
```

Suite totals under mutation: `version_identity` **11 passed, 2 failed**;
`identity::` **2 passed, 2 failed**.

## 4. Demonstrated repaired

```
$ cp /tmp/opencode/identity.rs.orig crates/core-domain/src/identity.rs
$ cargo test -p core-domain --test version_identity
test result: ok. 13 passed; 0 failed

$ cargo test -p core-domain --lib identity::
test result: ok. 4 passed; 0 failed
```

---

## Why four assertions and not one

A single assertion on `BUILD_VERSION != "dev"` would have caught this mutant,
but it would have caught **only** this mutant. The four that fired cover three
different failure shapes, so the suite survives a future variant that pins the
constant to some *other* wrong literal:

| assertion | shape it covers |
|---|---|
| `no_constant_is_a_placeholder` | the exact original string |
| `build_version_looks_like_a_sha` | *any* non-SHA literal (hex/length check) |
| `ch8_two_builds_of_different_commits_are_distinguishable` | the CH-8 property, stated directly — not its proxy |
| `v102_a_published_identity_carries_the_real_values` | the value as it reaches a published document |

The third is the important one. The other three assert facts *about the
constant*; the third asserts the *consequence the constant exists to prevent*.