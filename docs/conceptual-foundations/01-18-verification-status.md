# Verification Status

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

The current, honest state of verification for this specification.

> **A green documentation check is not a passing implementation.** The two are reported
> separately because they establish different things. Conflating them is how an unimplemented
> system comes to look verified.

---

## 1. The four layers

| # | Layer | What it establishes | Depends on an implementation? |
|---|---|---|---|
| **V-1** | **Documentation consistency** | Links, references, normative statements and cross-document invariants agree | **No** |
| **V-2** | **Gate execution** | Each of CG1–CG13 actually passes against production logic | **Yes** |
| **V-3** | **Mutation testing** | Each of M1–M7 demonstrably fails the relevant assertion, and passes after repair | **Yes** |
| **V-4** | **Reproducible evidence** | The reports identify the exact commit, spec, harness, fixtures and mutations tested | **Yes** |

**Dependency.** V-2 requires V-1. V-3 requires V-2. V-4 requires V-3. They cannot be
substituted for one another, and a lower layer never implies a higher one.

## 2. Status labels — a controlled vocabulary

Status labels are claims, and an undefined label is an unbounded claim. Every status reported for
this package uses one of these six, and **no other word may be used as a verdict**.

| Label | Evidence required | What it asserts |
|---|---|---|
| **CONSISTENT** | V-1 executed | The documentation agrees with itself. Nothing about behaviour. |
| **REVIEWED — SCOPE RECORDED** | V-1 **plus** an explicit written scope | A property has been examined and the limits of what was examined are stated. |
| **GATE-GREEN** | V-2 | *Named* gates passed against production logic. |
| **MUTATION-GREEN** | V-3 | *Named* mutants were caught, and repaired logic re-passes. |
| **SEMANTIC-GREEN** | SV | *Named* semantic case families passed. |
| **VERIFIED** | V-1 + V-2 + V-3 + SV **all**, with evidence bound to a commit | ⛔ **RESERVED.** Unavailable until every layer holds. |

> **`VERIFIED` is a reserved word.** It may not be used for a property, a subsystem, or a
> document while any of V-2, V-3 or SV is unrun. Using it is a process failure regardless of how
> well-supported the property feels.

### Applied to this package, right now

| Subject | Label | Why not higher |
|---|---|---|
| The four-axis architecture | **CONSISTENT** | It is a specification. No behavioural evidence exists. |
| Model / strategy distinction | **REVIEWED — SCOPE RECORDED** | The definition is unambiguous, internally consistent and cross-referenced. **No runtime behaviour has been observed** — V-2, V-3 and SV are unrun. |
| The three standing rules | **CONSISTENT** | Stated without contradiction across 135 files. Enforcement is untested. |
| Retention, leg, event contracts | **CONSISTENT** | Defined and cross-referenced. Unimplemented. |
| Inventory counts (160 / 76 / 84) | **REVIEWED — SCOPE RECORDED** | Reconciled by hand across documents. **I35 drift check lands in P1**, so they are not yet machine-verified against the registry. |

**The label is the claim. If the evidence does not reach the label, the label is wrong — not the
evidence.**

---

## 3. What a passing result establishes — and what it never establishes

> **Passing tests establish evidence for the cases tested. They never establish universal
> correctness, and a green suite is not a proof.**

### What V-2 and V-3 do *not* establish

Both are about **how** the system computes. Neither asks whether it computes the **right thing**.

| Kind of validation | Status |
|---|---|
| **Specification conformance** | Partly covered by V-2 |
| **Causal replay** | Covered by V-2 (CG1–CG13) |
| **Semantic validation** — does it *behave as intended*? | ⬜ **not established.** SV-A / SV-B / SV-C defined; no implemented inventory yet |
| **Strategy backtesting** | Out of scope — validates the strategy, not the model |

> **A fully green CG1–CG13 result would establish that the system uses only what it knew, does so
> deterministically, and matches its written definitions on the cases the suite covers.** It would
> **not** establish that a reading captures the market characteristic it was meant to capture.
>
> **Being a well-defined observer and being a useful observer are different properties.**

---

## 4. Current status

Measured 2026-10-15, against specification revision **pre-implementation**.

| Layer | Status | Evidence | Established |
|---|---|---|---|
| **V-1** Documentation consistency | 🟡 **PASS** (with one declared gap) | 135 files · 122 indicator specs · 0 broken links · 0 control characters · 0 corruption · 13 gates defined · 7 mutations defined | Normative statements agree with each other |
| **V-2** Gate execution | ⬜ **NOT RUN** | — | **Nothing.** No production logic exists to execute |
| **V-3** Mutation testing | ⬜ **NOT RUN** | — | **Nothing.** No mutants have been injected |
| **V-4** Reproducible evidence | ⬜ **NOT APPLICABLE** | — | **Nothing.** No commit, spec hash or harness to bind a report to |
| **SV** Semantic validation | ⬜ **NOT RUN** | SV-A reference · SV-B adversarial · SV-C metamorphic | **Nothing.** No implemented inventory to validate |

### What the V-1 pass does and does not mean

| It establishes | It does **not** establish |
|---|---|
| Every link resolves | That any code implements the linked contract |
| Counts reconcile across documents | That the counts describe a running system |
| Invariants are stated without contradiction | That any invariant is *enforced* |
| Gates and mutations are specified | That any gate has ever been *run* |

**The phrase "the specification is consistent" means V-1 and nothing else.** Any report claiming
more than V-1 on the basis of a documentation sweep is overstating what was measured.

### Measurement-quality review — a separate axis, not a V-layer

| Axis | Status |
|---|---|
| **Progression (22 specs)** | ✅ audited — 1 duplicate, 7 leg fields, 1 cross-axis misfiling, 1 contradiction. **Not yet applied.** |
| **Direction / Momentum / Structure** | ⬜ not audited |
| **Q4 ablation (OOS)** | ⬜ blocked until the causal gate passes |

> The review is measured evidence about the **inventory**. It is not a verification layer and does
> not gate anything until P5.

### The one declared gap in V-1

| Gap | Detail |
|---|---|
| **I35 not enforceable** | Generated artifacts cannot be verified byte-identical because the generator lands in **P1**. `inventory.md` and the README totals remain hand-maintained. Recorded in [`change-requests.md`](01-17-change-requests.md) CR-5 as declared-not-enforced, with its landing phase named. |

This is recorded rather than hidden. A drift check that does not yet exist cannot be counted as
passing.

---

## 5. What will change this

| Phase | Layer advanced | How |
|---|---|---|
| **P1** | V-1 gap closed | Generator + `test-doc` drift check → I35 enforceable |
| **P2** | V-2 partial | CG1 CG3 CG4 CG5 CG6 CG7 CG13 executed; M3 M4 M5 M7 injected |
| **P3** | V-2 partial | CG8 CG9 executed; M2 injected |
| **P4** | V-2 partial | CG10 CG11 executed; M6 injected |
| **P5** | **V-2 V-3 V-4 complete** | 13/13 gates · M1–M7 all detected · report bound to a commit |

---

## 6. The honest bottom line

**Today this package is a specification.** It is internally consistent (V-1) and it is the
contract a future implementation would be tested against. It is **not** a working system, and no
statement in this folder should be read as evidence that one exists.

The 84 readings remain frozen. They unlock on **V-2 ∧ V-3 ∧ V-4**, which requires running code.

> The objective is not to produce a green test run. It is to demonstrate that the system cannot
> silently confuse **future information with past knowledge**, **a changed reference with price
> movement**, or **a retained record with a valid event** — and to show that demonstration with
> mutants that were caught, not merely with tests that passed.
