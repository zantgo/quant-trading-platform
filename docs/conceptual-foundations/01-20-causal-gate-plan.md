# State Axes — Agentic Execution Plan

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)
**Spec:** revision 1.0 · **Status:** ready to execute · **Date:** 2026-10-15
**Scope:** migrate the platform's metrics layer to the four-axis descriptive state layer defined
in [`framework.md`](01-13-state-axes-framework.md), behind a causal-replay gate.

---

## 1. What this plan is for

The specification is frozen. This plan is the **construction and verification** sequence that
takes the current system to that specification - and, critically, the sequence that produces
result at runtime rather than asserting it on paper. running

> **The gate verifies runtime behaviour.** A passing schema sweep, a matching type signature, or
> an implementation that mirrors the documentation proves **nothing** about causality. Every gate
> below executes the real components against real clocks and compares observed outputs against
> expected transitions.

**The 84 new readings stay frozen until Phase 6 passes.** They are the cheapest thing to add and
the most expensive to add twice. Nothing is built on top of an unverified foundation.

---

## 2. Frozen state — what may not move

Locked at plan time. A phase that requires changing any of these must stop and raise a change
request; it may not quietly adjust.

| # | Frozen item | Where |
|---|---|---|
| F1 | Four axes: Direction, Momentum, Structure, Progression | [`framework.md`](01-13-state-axes-framework.md) §3 |
| F2 | Candle-only OHLCV contract | [`framework.md`](01-13-state-axes-framework.md) §7 |
| F3 | Retention tiers A/B/C/D, all bounded | [`retention.md`](01-16-retention-tiers.md) |
| F4 | Leg record: fields, `structure_level`, `progress` arithmetic, stability transitions | [`leg.md`](01-14-current-leg-record.md) |
| F5 | Event lifecycle: five transitions, six states | [`event-lifecycle.md`](01-15-event-lifecycle.md) |
| F6 | Terminality vocabulary, and its separation from completion | [`framework.md`](01-13-state-axes-framework.md) §5.9 |
| F7 | Reading roles: ACCELERATION / FORCE / DIAGNOSTIC | [`framework.md`](01-13-state-axes-framework.md) §5.2 |
| F8 | Ownership / consumption / emission | [`framework.md`](01-13-state-axes-framework.md) §5.7 |
| F9 | Signal vocabulary, incl. `CenterlineCross` | [`vocabulary.md`](../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) |
| F10 | Invariants I1–I26, L1–L18, E1–E10 | framework / leg / event-lifecycle |

**Integrity check.** A phase-exit commit touches spec files only under an approved change request.
The spec gate `./manage.sh test-doc` re-runs on every phase exit; an unapproved spec diff fails
the phase.

---

## 3. Agent roster and separation of duties

Six roles. The separations are load-bearing — collapsing any of them invalidates the gate.

| Role | Code | Responsibility | May write code? | May write tests? | May approve own work? |
|---|---|---|---|---|---|
| **Orchestrator** | ORCH | Decompose phase → work items; assign; enforce gates; **never implements** | No | No | No |
| **Worker** | W | Implement exactly one bounded work item | Yes | No | No |
| **Tester** | T | Write and execute the phase's gate tests from the spec, not the code | No | **Yes** | **No** |
| **Reviewer** | RV | Adversarial spec-conformance and adversarial-causality review | No | No | No |
| **Verifier** | VC | Run the mechanical gate suite; produce evidence artifacts; fail closed | No | No | No |
| **Committer** | C | Commit only when ORCH + RV + VC all green; writes the gate report into the commit message | No | No | No |

### 3.1 The independence rules

| # | Rule | Why |
|---|---|---|
| **S1** | The agent that implements a work item may not author the test that judges it. | A test written after the code tends to assert what the code does. |
| **S2** | Tests are written from [`framework.md`], [`leg.md`], [`event-lifecycle.md`] — **not** from the implementation. | Testing against the implementation tests the implementation. |
| **S3** | ORCH cannot approve a phase. Only VC produces the pass/fail, and only C commits. | Removes the incentive to self-certify. |
| **S4** | RV's default posture is to find the defect. A review with no findings must state what was probed. | A rubber-stamp review is worse than none. |
| **S5** | Any agent that edits a frozen item (F1–F10) halts the pipeline and files a change request. | The freeze is the whole point. |

---

## 4. Execution contract for the replay harness

The harness every gate test runs on. Five steps, and they are the definition of "causal" in this
system.

| Step | Action | Rule |
|---|---|---|
| **1** | Advance a **simulated clock** through historical observations in causal availability order | Order is by `available_at`, never by `occurred_at`. |
| **2** | Allow each component to consume **only** data available at that clock time | A component that reaches past the clock fails the run, not the test. |
| **3** | Capture an **immutable snapshot**: leg record, event lifecycle, axis outputs, metadata | Snapshot includes `available_at`, `target_version`, `target_stable`, `structure_level`. |
| **4** | Compare snapshots against expected results; **replay the same prefix with altered future data** | Alteration must touch only bars strictly after *t*. |
| **5** | **Fail closed** when any required timestamp, reference or typed value is missing or invalid | Absent ≠ default. `UNDEFINED` ≠ `0`. |

### 4.1 The critical test — future perturbation

> Change every bar after *t*, leaving everything through *t* untouched. **No snapshot available at
> or before *t* may differ.**

This is the load-bearing property. Comparing a recomputed final indicator series is **not**
sufficient — a series can agree at the endpoint and disagree in the middle, which is exactly what
a look-ahead bug produces. **Historical snapshots are compared element-wise**, not endpoints.

### 4.2 CG2 — future perturbation, strengthened

Prefix invariance ("a longer dataset yields the same prefix") is necessary but weaker than the
property that actually matters: *changing the future must not alter what the system claimed it
knew about the past*.

For an observation time *t*, **CG2 runs as follows**:

1. Replay with the original data through *t*. Save **all outputs available at or before *t***.
2. Hold every input through *t* **byte-identical**. Construct **several distinct futures** by
   altering subsequent bars — highs, lows, closes, volumes, and specifically
   **swing-confirming bars** (the bars that resolve a pivot) and **new extremes** (the bars that
   would move a rank, an ATR floor, or a self-percentile).
3. Replay each extended dataset.
4. Compare the outputs available **through *t*** against the saved originals.

**The required property:**

```
D1[ <= t ]  ==  D2[ <= t ]      ==>      O1[ <= t ]  ==  O2[ <= t ]
```

where `D` is the causally available input and `O` is the corresponding historical output.

**Minimum 4 distinct futures**, each targeting a different mechanism — volume-only, a
swing-confirming bar only, a new extreme only, and one altering all four.

> **Multiple futures, because a single one can be accidentally inert.** Perturbing only closes
> will not expose a look-ahead bug in the swing detector. Perturbing only swing-confirming bars
> will not expose one in the ATR floor. The gate must hit each mechanism.

**Comparison is not limited to indicator values.** All of these must match element-wise:

- event identities and `supersedes` links
- lifecycle transitions, and the bar each occurred on
- `available_at` timestamps
- `target_version` and `structure_version` — compared **disjointly**, per S-N5
- axis values and stage publications

### 4.3 Fail-closed rules

| Condition | Required behaviour |
|---|---|
| `available_at` missing on a required field | Run **fails**. No default, no fallback. |
| `T == O` or inside noise floor | `progress = UNDEFINED` typed value. Never `0`, `NaN`, `inf`. |
| Required typed value missing | Run **fails** with the field name. |
| Event in non-`active` state referenced as evidence | Run **fails** with the event identity. |
| Snapshot differs under future perturbation | Run **fails** with the first differing bar. |

---

## 5. Normative decisions the gate must encode

These are resolved now so every implementation conforms. They are **spec-level**, and the gate
tests them directly.

### 5.1 Stability is measured in observations — never described as temporal stability

`STABILITY_WINDOW` is an **observation count**. It is *not* equivalent temporal stability across
durations, and no artifact may describe it as such.

| Duration | `STABILITY_WINDOW` bars | Actual clock time |
|---|---|---|
| 1 s | *N* | *N* seconds |
| 5 min | *N* | *N* × 300 s |
| 4 h | *N* | *N* × 14 400 s |

The rule is intentional: it counts **bars of unbroken reference**, which is what the claim
"the estimate has stopped moving" actually means. Elapsed time would let a single quiet hour
certify a target on the daily chart while twenty volatile minutes on the 1-second chart could
not. **Documentation, UI labels and gate names must say "observations" or "bars", never "time".**

### 5.2 The boundary bar — the target-change bar does not count

**On any target change, the stability counter resets to zero. Only subsequent eligible bars with
no further revision increment it.**

```
bar n    target revised          → counter = 0   eligible: NO
bar n+1  no revision             → counter = 1   eligible: YES
...
bar n+W  no revision             → counter = W   eligible: YES  → stable = true
```

`target_stable` becomes `true` when `counter >= STABILITY_WINDOW`, evaluated **after** the bar's
increment. The revision bar itself is never counted. Gate **CG4** asserts the boundary exactly:
`W-1` bars → still false; `W` bars → true.

### 5.3 The noise floor is fixed at leg creation

The ATR-based noise floor is **computed once, at `origin_available_at`, and frozen for the life
of the leg.** It does not evolve.

| Policy | Consequence | Verdict |
|---|---|---|
| **Fixed at creation** ✅ | The floor is a property of the leg's own origin context. Stable, explainable, and `progress` changes only for two reasons: price moved, or the target moved. | **Adopted.** |
| Evolving per bar | `progress` gains a **third** revision source unrelated to price or target. A leg can report a different value for the same `(P, O, T)` depending on when you looked. | Rejected — it reintroduces the ambiguity §4.3 of `leg.md` exists to remove. |

The floor is computed **only from candles available at `origin_available_at`**. Gate **CG6**
asserts both the freeze and the causality of the floor itself.

---

## 6. The gate matrix

Thirteen gates. Every one executes runtime behaviour.

| Gate | What it executes | Required result | Phase |
|---|---|---|---|
| **CG1** | Availability enforcement | Pivot at *t*, `available_at` > *t*; no output before it is influenced | P2 |
| **CG2** | **Future perturbation** | ≥4 altered futures; equal causally-available input through *t* ⟹ equal historical output through *t*, element-wise | P1, P5 |
| **CG3** | Target revision ≠ movement | Price rises, target moves away; `progress` may fall, no movement fabricated, unstable stage not promoted | P2 |
| **CG4** | Stability window boundary | `W-1` bars → false; `W` bars → true; the revision bar is never counted | P2 |
| **CG5** | Revision budget | `unviable`; stability unrecoverable for that leg; no stage; `progress` provisional when defined | P2 |
| **CG6** | Degenerate reference | `UNDEFINED` — never `0`, `inf`, `NaN`; floor fixed at creation and causal | P2 |
| **CG7** | Leg identity and nesting | Major/minor records distinct; scale policy selects; identity ≠ relevance | P2 |
| **CG8** | Event lifecycle | Activate → expire → re-trigger evidence; old stays expired, new identity with `supersedes` | P3 |
| **CG9** | Retention vs validity | Expired events inside storage bounds are stored but **never** counted as active | P3 |
| **CG10** | Terminality vs completion | Termination evidence with new extremes; reassessed, leg **not** closed | P4 |
| **CG11** | Cross-axis consumption | One canonical divergence consumed by many axes, never multiplied | P4 |
| **CG12** | Deterministic replay | Same immutable input twice; IDs, identities, transitions, values, timestamps match exactly | P1, P5 |
| **CG13** | **Scale-classifier causality** | Rank distribution is causal; warmup defined; reclassification ≠ retargeting | P2 |

**Total: 13 gates. Phase exit requires every gate in that phase to be green.**

### 6.1 CG13 — structural-scale classifier causality

The major/minor classifier ranks displacement against Tier A history. A percentile is only as
honest as the sample behind it, so the classifier inherits every temporal rule the leg obeys.

| # | Assertion |
|---|---|
| **13a** | The rank distribution contains **only observations eligible at classification time**. A future bar never enters it (S-N1). |
| **13b** | Every distribution member carries its own `available_at`; none predates the bar at which it becomes usable (S-N2). |
| **13c** | A distribution change revises a past classification **only from the bar at which it becomes available**, never retroactively (S-N3). |
| **13d** | **Warmup**: while the distribution is incomplete, classification is `provisional` and publishes **no stage** (S-N4, L22). |
| **13e** | The rank threshold is **frozen at leg creation** and never evolves (S-N4, L21). |
| **13f** | A reclassification increments `structure_version` and **never** `target_version`; a retarget does the reverse (S-N5, S-N6, L23). |

> **13f is the one that catches the subtle bug.** If reclassification bumped `target_version`,
> then `progress` would change with no price movement and no target movement - a **third**
> revision source, precisely the ambiguity `leg.md` 4.3 exists to eliminate. The counters must
> be separately observable or 13f is untestable.

**13c is tested by perturbation**: extend the dataset past the classification bar and confirm the
classification is unchanged until the bar that made the revision available.


---

## 7. Phase breakdown

Seven phases. P0 is complete. **P6 and P7 are blocked until P5 reports green.**

### 7.0 The two phase series, and which one commits cite

This document's `P0`–`P7` are the **gate plan's own** series: they order the *gate* work only. The
delivery plan runs a wider series (`P0`–`P14` plus `R1`–`R9`) that also carries the CLI shell, the
config split, the crate split and the release. Both are called `P`, and reading a commit message
against the wrong table is how a phase silently lands its work in the wrong place.

**Commit messages cite the delivery-plan series. This table is the mapping.**

| This document | Delivery-plan phase | Gates it produces | State |
|---|---|---|---|
| **P0** Specification freeze | P0–P1 | — | ✅ |
| **MS-1** First milestone | **P7** (first half) | CG1 + M1 | ✅ `4b2bf330` |
| **P1** Replay harness | **P7** (second half) | harness, CG2, CG12 | ✅ `4b2bf330` |
| **MR** Measurement-quality review | **P12** entry | CR-16 applied before any new reading | ⬜ |
| **P2** Leg, progression, stability | **P8** | CG3 CG4 CG5 CG6 CG7 CG13 · M3 M4 M5 M7 | ⬜ |
| **P3** Event lifecycle | **P9** | CG8 CG9 · M2 | ⬜ |
| **P4** Ownership, terminality, cross-axis | **P10** | CG10 CG11 · M6 | ⬜ |
| **P5** Full gate execution and report | **P11** | all 13 · M1–M7 · SV · parity | ⬜ |
| **P6** Inventory expansion ⛔ | **P12** | — (entry is hard-gated on P11) | ⬜ |
| **P7** Surface | **P13 + P14** | — | ⬜ |

---

### P0 — Specification freeze ✅ COMPLETE

| | |
|---|---|
| **Entry** | — |
| **Work** | Framework, leg contract, event lifecycle, retention, vocabulary, visual grammar, 122 indicator specs |
| **VC evidence** | 131 files · 160 readings (76 present / 84 new) · 0 broken links · 0 contradictions |
| **Exit** | Done. 26 data + 18 leg + 10 event + 10 visual invariants defined |

---

### MS-1 — First milestone: CG1 + M1 (the only gate authorised to run alone)

**Deliberately narrow.** This milestone proves exactly one thing: *the harness detects a
precisely specified availability defect.* It does **not** prove the other twelve gates work, and
it does **not** prove mutation detection is reliable across classes.

> **MS-1 is not a smaller P1.** It is a precondition for trusting P1. Nothing else runs until the
> baseline → inject → catch → repair cycle has worked once and produced trustworthy evidence.

#### The five steps, and what each must produce

| Step | Action | Required evidence |
|---|---|---|
| **1. Baseline** | Run CG1 against **unmodified production logic** | CG1 passes. Command + exit status recorded. |
| **2. Inject M1** | Apply the availability mutation to an **isolated copy** of production logic | The mutation site is named. Production is untouched. |
| **3. Catch** | Run CG1 against the mutant | CG1 **fails at the expected availability assertion** — quoted verbatim. |
| **4. Repair** | Restore the unmutated logic | CG1 passes again. Command + exit status recorded. |
| **5. Preserve** | Write the evidence | Report identifies commit, spec, harness, fixture checksums, the mutation, and the assertion. |

#### Failure specificity — the requirement that matters

> **A suite that goes red because the mutant *crashed* has not demonstrated that CG1 detects the
> intended semantic defect.** A compile error, a panic, a `None` unwrap, or a failing *unrelated*
> assertion all produce red. None of them is evidence.

| The failure is NOT acceptable because | |
|---|---|
| The mutant fails to compile | That tests the compiler, not CG1 |
| The mutant panics on setup | That tests robustness, not availability semantics |
| A different assertion fails first | That tests ordering, not detection |
| The run errors before reaching any assertion | That tests the harness plumbing, not the gate |

The report must show the **specific assertion identifier** that fired, and that identifier must be
the availability assertion named in step 2. Anything else is a **failed milestone**, not a partial
pass.

#### Scope guard

| Rule | |
|---|---|
| **MS-1-S1** | MS-1 runs **CG1 only**. No other gate is implemented during MS-1. |
| **MS-1-S2** | MS-1 injects **M1 only**. M2–M7 are not created. |
| **MS-1-S3** | If MS-1 fails, it is fixed. It is **not** worked around by adding gates. |
| **MS-1-S4** | MS-1 produces a report of the **same format** as P5 — so the format itself is exercised once before it is depended upon. |
| **MS-1-S5** | MS-1 does **not** unlock the inventory. Only P5 does. |

---

---

### P1 — Replay harness (no domain logic)

The harness is built **before** the components it judges, so it cannot be shaped by them.

| Item | Content |
|---|---|
| **ORCH** | Decompose; assign |
| **W1.1** | Simulated clock. Advances by `available_at` order. Emits `now` to every component. |
| **W1.2** | Snapshot recorder. Immutable capture: leg record, event journal state, axis outputs, `available_at`, `target_version`, `target_stable`, `structure_level`. |
| **W1.3** | Perturbation engine. Rewrites bars strictly after *t*; asserts bars ≤ *t* are untouched. Supports **≥4 targeted future generators** — volume-only, swing-confirming-bar-only, new-extreme-only, all-four (§4.2). |
| **W1.4** | Element-wise snapshot differ. Reports the **first** differing bar, not just the endpoint. Compares event identities, transitions, `available_at`, both version counters, axis values and stages — **not indicator values alone**. |
| **W1.5** | Fail-closed assertions. Missing timestamps, references, typed values → hard fail with field name. |
| **W1.6** | Typed `UNDEFINED` plumbing end-to-end. Must survive serialisation and aggregation without becoming `0` or `NaN`. |
| **W1.7** | Reading-identity loader consuming the **CR-4** mapping. Must land before CG11. |
| **W1.8** | **Generated-artifact generator + `test-doc` drift check** (CR-5). Regenerates `inventory.md` and the README totals from the registry; fails on byte difference. Closes the one V-1 gap. |
| **T1.1** | Write **CG2** and **CG12** against the harness using a trivial stub aggregator — the harness must be capable of failing before it is capable of passing. |
| **T1.2** | Execute mutation **M1** (availability bypass) against the stub; the corresponding assertion must fail. |
| **RV1** | Adversarial: can the harness be fooled? Can a component read past the clock? Is the differ really element-wise? Does a single perturbation pass while a targeted one would fail? |
| **VC gate** | **CG2 + CG12 green.** Harness detects a seeded look-ahead violation. M1 detected and M1-repaired passes. Generator drift check green (**I35** enforceable). |
| **Exit** | A violation injected into a stub is *caught*. A clean stub is not. A single inert future does not produce a false pass. No generated artifact drifts from the registry. |
| **Commit** | `feat(harness): causal replay harness, multi-future perturbation, element-wise differ (CG2,CG12)` |

> **The seeded-violation check is mandatory.** A harness that has never failed has not been shown
> to work. Phase 1 cannot exit until a deliberately broken stub is caught by it — and until that
> stub is shown caught **by each distinct future generator**, not merely by one.

### MR — Measurement-quality review (parallel, never blocking the gate)

Runs alongside the phases. It **cannot** block MS-1 … P5, because nothing it decides affects
whether the harness is correct.

| Item | Content |
|---|---|
| **MR-P** | **Progression** — ✅ **DONE**. 22 specs audited; 1 duplicate, 7 leg fields, 1 cross-axis misfiling, 1 self-contradiction. 22 → 11–13. See [`measurement-quality.md`](01-19-measurement-quality.md) §6. |
| **MR-D** | **Direction** — 50 readings, highest density. Audit for smoothing variants of one quantity. |
| **MR-M** | **Momentum** — enforce the `role` rule: diagnostics must not enter the axis value. |
| **MR-S** | **Structure** — event *quality* over event *count*; also the next axis at risk of `field`-category error. |
| **MR-Q4** | Ablation harness — chronological, one reading/family at a time, measured on the **axis output**. **Runs only after P5.** Never on profitability (**P10**). |

| Rule | |
|---|---|
| **MR-1** | Verdicts are recorded in the registry (`verdict`, `owner_class`, `q1_unique_information`). |
| **MR-2** | `field` and `move` verdicts are resolved **before** implementation, never after. |
| **MR-3** | Q4 runs **after** the causal gate. A reading cannot be judged on whether it helps until it is known the system is not cheating while computing it. |
| **MR-4** | No verdict changes an axis count for balance. |

> **MR-P found the dominant failure mode on a record-based axis: a leg field promoted into a
> reading.** That is a category error, not a weak reading — the information exists, but it is not an
> independent observation and must not carry an aggregation weight.

---

### P2 — Leg record, progression, stability

| Item | Content |
|---|---|
| **ORCH** | Decompose; assign. **Confirm CR-1–CR-4 have landed before exit.** |
| **W2.1** | `LegRecord` struct — all fields; `schema_version`, `structure_level`, `structure_version`, `swing_policy_version`; **derived** fields carry no counter |
| **W2.2** | Pivot classification — `major` / `minor` by **self-percentile rank of displacement** (never a points threshold) |
| **W2.2a** | Rank distribution is **causal data** — eligible observations only; own `available_at`; threshold frozen at creation |
| **W2.2b** | Warmup → `provisional` classification, which publishes no stage |
| **W2.3** | Availability stamping — every field carries `available_at`; written at computation, not at inference |
| **W2.4** | `progress = (P - O) / (T - O)` + the **degenerate guard** (`T == O`, noise floor → `UNDEFINED`) |
| **W2.5** | Noise floor — computed once at `origin_available_at`, frozen. **Must use only candles available then.** |
| **W2.6** | Stability machine — §5.2 boundary rule, §5.3 freeze, `REVISION_BUDGET` → `unviable` |
| **W2.7** | Revision log — bounded; `target_version` and `structure_version` **monotonic and disjoint**; `stage`/`progress`/`target_stable` **carry no counter** (L26) |
| **W2.8** | Stage classification — ordinal, from `progress`; blocked while `target_stable = false`; withheld while `unviable` or `provisional` |
| **W2.9** | Tier D exclusion from the byte budget — no charge, no eviction path (**T2**, **L24**) |
| **T2.x** | **CG1, CG3, CG4, CG5, CG6, CG7, CG13** written from [`leg.md`](01-14-current-leg-record.md), before W2 runs |
| **RV2** | Adversarial: any read of an unconfirmed origin? Any fabricated movement? Any forced stability? Is the floor frozen? Is the rank threshold frozen? Does anything publish a stage for an `unviable` or `provisional` leg? Does reclassification leak into `target_version`? Can budget pressure reach a Tier D record? |
| **VC gate** | **CG1, CG3, CG4, CG5, CG6, CG7, CG13 green** + M3 M4 M5 M7 detected |
| **Exit** | Every L1–L24 invariant observable at runtime; leg behaviour matches all three worked examples (`leg.md` §8.1–8.3); CR-1–CR-4 landed |
| **Commit** | `feat(leg): versioned leg record, progression, stability, causal scale classifier (CG1,CG3-CG7,CG13)` |

---

### P3 — Event lifecycle

| Item | Content |
|---|---|
| **W3.1** | Canonical event identity `(source, kind, subject, anchor, timeframe)` — immutable |
| **W3.2** | Six states: `pending` · `active` · `expired` · `invalidated` · `resolved` · `superseded` |
| **W3.3** | Five transitions: activation, persistence, expiration, invalidation, reactivation |
| **W3.4** | Per-kind expiry bounds — duration-relative; instantaneous events expire on the crossing bar |
| **W3.5** | Reactivation — **new** identity + `supersedes`. No record is ever revived. |
| **W3.6** | Three-count export — active / retained / emit, never conflated |
| **W3.7** | Tier C journal bound — count **and** age, independent of validity |
| **T3.x** | **CG8, CG9** written from [`event-lifecycle.md`](01-15-event-lifecycle.md) |
| **RV3** | Adversarial: can an expired event be counted? Is reactivation reviving anything? Is the bound count-only anywhere? |
| **VC gate** | **CG8 + CG9 green** |
| **Exit** | Only `active` counts. A bounded journal holding 200 expired events reports active count 0 |
| **Commit** | `feat(events): event lifecycle with expiry, invalidation and non-reviving reactivation (CG8,CG9)` |

---

### P4 — Ownership, terminality, cross-axis consumption

| Item | Content |
|---|---|
| **W4.1** | Ownership / consumption / emission — declared per reading; counted exactly once |
| **W4.2** | Signal families — Williams %R and Stochastic %K are **one** family, not two |
| **W4.3** | Staged aggregation — normalise → family → axis outputs → test |
| **W4.4** | Terminality — `continuation_supported` / `indeterminate` / `termination_evidence`, from completion criteria only |
| **W4.5** | Terminality ⟂ completion — separate outputs; `termination_evidence` never closes a leg |
| **W4.6** | Regime reserved · Confidence reserved — named, scoped, non-axis, non-smuggled |
| **T4.x** | **CG10, CG11** written from [`framework.md`](01-13-state-axes-framework.md) §5.7–5.9 |
| **RV4** | Adversarial: is any indicator double-counted? Does terminality leak into stage? Is Regime leaking onto an axis? |
| **VC gate** | **CG10 + CG11 green** |
| **Exit** | One divergence consumed by three axes counts **once** |
| **Commit** | `feat(axes): ownership/consumption/emission, staged families, terminality (CG10,CG11)` |

---

### P5 — Full gate execution and report

**The phase that decides whether the 84 readings open.**

| Item | Content |
|---|---|
| **W5.1** | Synthetic edge-case corpus — degenerate refs, revision storms, expiry boundaries, scale collisions, instant crosses |
| **W5.2** | Representative historical replay — multiple durations, multi-scale, including a chop regime and a clean trend |
| **T5.x** | Run **all thirteen gates**, synthetic then historical |
| **RV5** | Full adversarial pass; confirm no gate is trivially satisfiable |
| **VC gate** | **12 / 12 green**, and the three-category report emitted (§9) |
| **Exit** | Report published; zero look-ahead violations |
| **Commit** | `test(gate): causal replay gate 13/13 — causality, semantics, determinism (unblocks inventory)` |

---

### P6 — Inventory expansion — ⛔ BLOCKED until P5 green

| Item | Content |
|---|---|
| **Entry** | **Hard gate.** P5 report green on all three categories. No exceptions, no partial entry. |
| **W6.1–W6.6** | Admit the 84 readings **by target axis**, in batches. Each batch: spec already exists in [`indicators/`](../engines/market-monitoring-engine/indicators) |
| **Rule** | New readings enter through **normalisation → family aggregation → axis testing**. They may not alter any contract in F1–F10 to accommodate themselves |
| **RV6** | Reject any batch that requires a contract change. That is a change request, not a feature |
| **VC gate** | Per batch: existing 13 gates stay green **plus** incremental-information test (§5.8 stage 4) |
| **Exit** | 160 readings implemented; architecture untouched |
| **Commit** | Per batch: `feat(inventory): <axis> readings batch N (gates 13/13 hold)` |

---

### P7 — Surface

| Item | Content |
|---|---|
| **W7.1** | Dot-on-track primitive — identical track geometry on all four axes |
| **W7.2** | Three channels: position · size · readiness |
| **W7.3** | `UNDEFINED` renders as an **absent value with a reason** — never `0`, never a dot at origin |
| **W7.4** | `structure_level` rendered per Progression track; major vs minor never mixed |
| **RV7** | Is readiness ever labelled "confidence"? Does any non-direction axis take direction colour? |
| **VC gate** | `./manage.sh test-ui` green; V1–V10 visual invariants observable |
| **Exit** | Four axes rendered; invariants V1–V10 hold at the surface |
| **Commit** | `feat(ui): dot-on-track surface with readiness channel and structural scale` |

---

## 8. Verification table — the gate summary

One row per gate. This is the table that decides the verdict.

> **Result is filled only by VC, after the gate has actually executed against production logic.**
> Every row below is currently unchecked because **no implementation exists**. An unchecked row
> is an unrun test, not a passing one. See [`verification-status.md`](01-18-verification-status.md).

| Gate | Verifies | Phase | Test artifact | Command | Expected | Result |
|---|---|---|---|---|---|---|
| **CG1** | Availability enforcement | P2 | `market-analyzer/tests/gate_g1_availability.rs` | `cargo test -p market-analyzer --test gate_g1_availability` | No pre-`available_at` influence | ⬜ not run |
| **CG2** | **Future perturbation** | P1, P5 | `.../gate_g2_future_perturbation.rs` | `cargo test -p market-analyzer --test gate_g2_future_perturbation` | ≥4 independent fixtures; outputs through *t* element-wise identical | ⬜ not run |
| **CG3** | Target revision ≠ movement | P2 | `.../gate_g3_target_revision.rs` | `cargo test -p market-analyzer --test gate_g3_target_revision` | `progress` may fall; no fabricated movement; no promotion | ⬜ not run |
| **CG4** | Stability window boundary | P2 | `.../gate_g4_stability_window.rs` | `cargo test -p market-analyzer --test gate_g4_stability_window` | `W-1` false · `W` true · revision bar uncounted | ⬜ not run |
| **CG5** | Revision budget | P2 | `.../gate_g5_revision_budget.rs` | `cargo test -p market-analyzer --test gate_g5_revision_budget` | `unviable`; stability unrecoverable; no stage | ⬜ not run |
| **CG6** | Degenerate reference | P2 | `.../gate_g6_degenerate.rs` | `cargo test -p market-analyzer --test gate_g6_degenerate` | `UNDEFINED`; never `0`/`inf`/`NaN`; floor fixed + causal | ⬜ not run |
| **CG7** | Leg identity and nesting | P2 | `.../gate_g7_leg_identity.rs` | `cargo test -p market-analyzer --test gate_g7_leg_identity` | Major/minor distinct; policy selects; identity ≠ relevance | ⬜ not run |
| **CG8** | Event lifecycle | P3 | `.../gate_g8_event_lifecycle.rs` | `cargo test -p market-analyzer --test gate_g8_event_lifecycle` | Old stays expired; new identity + `supersedes` | ⬜ not run |
| **CG9** | Retention vs validity | P3 | `.../gate_g9_retention_validity.rs` | `cargo test -p market-analyzer --test gate_g9_retention_validity` | Stored but not counted as active | ⬜ not run |
| **CG10** | Terminality vs completion | P4 | `.../gate_g10_terminality.rs` | `cargo test -p market-analyzer --test gate_g10_terminality` | Reassessed; leg not closed | ⬜ not run |
| **CG11** | Cross-axis consumption | P4 | `.../gate_g11_cross_axis.rs` | `cargo test -p market-analyzer --test gate_g11_cross_axis` | Consumable, never multiplied | ⬜ not run |
| **CG12** | Deterministic replay | P1, P5 | `.../gate_g12_determinism.rs` | `cargo test -p market-analyzer --test gate_g12_determinism` | Exact match across two runs | ⬜ not run |
| **CG13** | Scale-classifier causality | P2 | `.../gate_g13_scale_causality.rs` | `cargo test -p market-analyzer --test gate_g13_scale_causality` | Distribution causal; warmup defined; reclass ≠ retarget | ⬜ not run |

### 8.2 Semantic validation — SV, the layer the gates do not cover

CG1–CG13 establish **specification conformance** and **causal replay**. They do **not** establish
that the readings *behave as intended*. A deterministic, causal system can compute the wrong
value consistently.

SV is organised into **three kinds of case**, because each catches a different class of error.

#### SV-A — Reference cases

Independently specified expected outputs, including boundary conditions.

| # | Property |
|---|---|
| **SV-A1** | Boundary behaviour — correct at the edges of the defined domain |
| **SV-A2** | Unit sanity — magnitude is plausible against the instrument's own scale |
| **SV-A3** | Reference examples — match an **independently defined** worked case |

#### SV-B — Adversarial cases

Scenarios constructed to expose a **plausible but incorrect** interpretation. These are the cases
that catch a system which is internally consistent and wrong.

| # | Property |
|---|---|
| **SV-B1** | Interpretations that differ by a sign, an inverse, or a scope slip |
| **SV-B2** | Inputs that satisfy the formula but violate its stated intent |
| **SV-B3** | Cases where a reasonable reader could implement it two different ways |

#### SV-C — Metamorphic properties

Relationships that must hold when inputs are changed in a controlled way. These need **no
external reference** — they are consequences of the definition.

| # | Property | Must hold |
|---|---|---|
| **SV-C1** | Progress vs price | `progress` moves according to the defined formula when price moves |
| **SV-C2** | Revision ≠ movement | **A target revision must not masquerade as price movement** |
| **SV-C3** | Availability monotonicity | A confirmed structural event **must not** become available before its confirmation time |
| **SV-C4** | Duration invariance | The same rule produces the same result at every active duration (P3) |
| **SV-C5** | Scale separation | The `progress` **formula** is scale-independent: `(P-O)/(T-O)` does not reference `structure_level`. **Qualification:** *definedness* is **not** scale-independent — see below |
| **SV-C6** | Eviction neutrality | Removing an event from storage never changes any event's lifecycle state (**T3**) |

> **SV-C2, C3 and C5 are the properties that would otherwise pass silently.** Each asserts that
> two quantities which *look* similar are in fact independent. A system that confuses them
> produces output that looks entirely reasonable.

#### SV-C5 qualification — definedness is not scale-independent

An earlier draft asserted that changing `structure_level` never alters `progress` for an
unchanged `(P, O, T)`. **That is false, and the counterexample is structural.**

| Step | Consequence |
|---|---|
| The noise floor is the instrument's realised range over **the leg's own** window, **frozen at leg creation** from candles available at that leg's `origin_available_at` (**L13**, **L21**) | Each leg has its **own** floor |
| A `major` and a `minor` leg may have **different origins** | → different `origin_available_at` → **different frozen floors** |
| For a target very close to the origin, `abs(T - O)` may fall **below one floor and above the other** | → one level returns a number, the other returns **`UNDEFINED`** |

| Resolution | |
|---|---|
| **Chosen** | **SV-C5 is constrained to cases where both legs' validity conditions are unchanged** — same origin, same frozen floor, same target. |
| **Rejected** | Forcing one noise floor across scales. That would violate **L21**, and the per-leg floor is correct: a leg that began in a quiet period genuinely has a tighter floor than one that began in a violent one. |

> **This asymmetry is deliberate and must not be "fixed."** The *formula* is scale-independent; the
> *guard* is not, because the guard is anchored to the leg. SV-C5 asserts the former and asserts
> nothing about the latter.

| Rule | |
|---|---|
| **SV-R6** | A test asserting scale-independence must fix `(P, O, T)` **and** the originating leg. Varying one while holding the other fixed is a different test. |
| **SV-R7** | Any apparent contradiction between SV-C5 and **L13**/**L21** is resolved in favour of the leg contract. The floors are per-leg by design. |

| Rule | |
|---|---|
| **SV-R1** | Expected values come from the specification or an external reference — **never from a current implementation run.** Otherwise the test asserts only self-consistency. |
| **SV-R2** | A reading may not be marked verified on causal evidence alone. |
| **SV-R3** | SV failures are **semantic** findings. They are not resolved by adjusting the test to match the output. |

**SV is not prediction.** These are assertions about *intended behaviour on described
situations*, not claims about future prices or trade outcomes. **SV establishes what a reading
means, not whether it improves a trading strategy** — that remains a separate downstream question.

**Scope.** SV is scoped to the **implemented** inventory. It does not gate MS-1, P1 or P2, which
build the machinery. It becomes an exit condition for **P5**, alongside CG1–CG13.

**13 gates. `Result` is filled only by VC, never by the implementing agent.**

### 8.0 Four verification layers, reported separately

| Layer | Establishes | Status now |
|---|---|---|
| **V-1** Documentation consistency | Normative statements agree with each other | 🟡 PASS (I35 not yet enforceable) |
| **V-2** Gate execution | CG1–CG13 pass against production logic | ⬜ **not run** |
| **V-3** Mutation testing | M1–M7 fail the relevant assertion, pass after repair | ⬜ **not run** |
| **V-4** Reproducible evidence | Report bound to a commit, spec, harness, fixtures, mutations | ⬜ not applicable |

**A V-1 result never implies V-2.** Full status and the one declared gap:
[`verification-status.md`](01-18-verification-status.md).

### 8.1 CG2's four futures are independent fixtures

CG2's value is **not** that one look-ahead test passes. It is that **four distinct futures each
fail to move the past**, and that each exercises a different mechanism.

| Fixture | Alters | Mechanism it probes |
|---|---|---|
| **FX1** `volume_only` | volume only | participation and volume-derived readings |
| **FX2** `swing_confirm_only` | adds/removes a swing-confirming bar only | pivot `available_at`, leg origin timing |
| **FX3** `new_extreme_only` | creates a new extreme only | rank distributions, ATR noise floor, scale classification |
| **FX4** `all_four` | highs, lows, closes, volumes together | composite interaction |

Each is a **separate fixture file** with its own checksum in the report banner, and each is
compared with the **element-wise prefix comparison** — never an endpoint comparison, and never a
single shared future reused across assertions.

> **One shared future across all assertions would be a silent weakening.** If that future happened
> not to move the mechanism under test, CG2 would pass while proving nothing about it.

### 8.1 Mutation programme — the gate must be shown sensitive

A gate that has never failed has not been shown to work. Each mutation is a **deliberate,
isolated defect** injected into a copy of production logic — never into production itself.

| # | Injected defect | Must be caught by | Status |
|---|---|---|---|
| **M1** | Availability enforcement bypassed — a field read before its `available_at` | CG1, CG2 | ✅ detected, repaired (P7 — record in §8.1.1) |
| **M2** | Event expiry suppressed — absence of contradiction treated as persistence | CG8, CG9 | ⬜ not run |
| **M3** | Stability boundary off-by-one — the revision bar counted | CG4, CG5 | ⬜ not run |
| **M4** | Progression clamped to `[0, 1]` | CG3, CG6 | ⬜ not run |
| **M5** | Reclassification bumps `target_version` — scale change masquerades as movement | CG3, CG13 | ⬜ not run |
| **M6** | Cross-axis double-count — one event weighted once per consuming axis | CG11 | ⬜ not run |
| **M7** | Byte-budget eviction applied to a Tier D leg record | CG7, CG9 | ⬜ not run |

**Every mutation must produce all four of these, recorded in the report:**

1. **Named** — which defect was injected and where.
2. **Isolated** — a copy of production logic, not production itself. A mutation that ships is a
   process failure, not a test result.
3. **Demonstrated failing** — the specific assertion that caught it, quoted.
4. **Demonstrated repaired** — the same test passes against the unmutated implementation.

### 8.1.1 M1 — the executed record (P7)

The four records this section requires. Quoted output is verbatim from the run; nothing is
paraphrased.

| # | Record | Evidence |
|---|---|---|
| **1** | **Named** | Availability enforcement bypassed. `crates/market-analyzer/src/indicators/fibonacci.rs` → `PivotPoint::confirming_bar()` returned `self.index` instead of `self.index + self.strength` — a pivot claimed it was knowable on the bar it *occurred*, three bars before the far side supplied the evidence. |
| **2** | **Isolated** | The defect was injected, run, and repaired inside one working session; `git checkout crates/market-analyzer/src/indicators/fibonacci.rs` restored the file and `git diff` on it returned empty before the re-run. Nothing mutated was committed. |
| **3** | **Demonstrated failing** | `cargo test --no-fail-fast -p market-analyzer --test gate_cg1_availability --test gate_cg2_future_perturbation` → **CG1: 1 failed / 9 passed, CG2: 2 failed / 10 passed.** Quoted: `a_pivot_declares_the_bar_it_was_confirmed_not_the_bar_it_happened` — `assertion left == right failed / left: 100 / right: 103`. Quoted: `c2_availability_matches_the_window_the_scan_actually_consults` — `pivot at bar 6 claims availability at 6, but the scan consults through bar 9 — availability earlier than that is a look-ahead / left: 6 / right: 9`. Quoted: `c2_a_pivot_is_not_knowable_before_its_far_side_bar_closed` — `a pivot at bar 6 was still hidden at bar 6, its confirming bar`. Each assertion failed on a **value mismatch**, not on a compile error or a setup panic. |
| **4** | **Demonstrated repaired** | After the restore: CG1 `10 passed; 0 failed` · CG2 `13 passed; 0 failed` · CG12 `7 passed; 0 failed`. The mutant is also retained permanently as an **isolated copy** in `gate_cg2_future_perturbation.rs::c2_the_gate_catches_m1_bypassed_availability`, so the evidence does not require anyone to re-inject the defect. |

**M2–M7 remain unrun** and are gated behind the phases that own their subject matter (M2 behind the
event lifecycle, M3–M5 M7 behind the leg record, M6 behind the axis assembly).

> **M1 alone is not sufficient.** One caught mutation demonstrates detection of one specific
> injected defect, under the tested conditions. M1 alone does not establish that the other
> detection paths work. M1-M7 span
> availability, expiry, stability, progression, scale versioning, cross-axis counting and
> eviction - the seven ways this system could silently confuse future with past, a changed
> reference with movement, or a retained record with a valid event.

**Regression floor — every phase exit re-runs:**

| Suite | Command |
|---|---|
| Core + golden + indicators | `./manage.sh test-core` · `./manage.sh test-golden` · `./manage.sh test-indicators` |
| Engine | `./manage.sh test-engine` |
| Spec corpus | `./manage.sh test-doc` |
| Full | `./manage.sh test` |

---

## 9. Gate report format

Three categories. Reported separately — they are different properties and a combined pass/fail
conceals which one broke.

```
════════════════════════════════════════════════════════════════
CAUSAL REPLAY GATE — <phase> — <date>
════════════════════════════════════════════════════════════════

REPRODUCIBILITY BANNER
   Source commit ......... <sha>
   Spec version ........... <spec sha / CR set>
   Harness version ........ <sha>
   Fixtures ............... <names + checksums>
   Mutations .............. M1..Mn exercised this run

1. CAUSALITY
   Gates ......... 4/4  (CG1 CG2 CG4 CG12)
   Look-ahead violations .................. 0
   Pre-availability influence ............. 0
   Perturbation futures exercised ......... 4+
   Perturbation diffs at or before t ....... 0
   Mutation M1 detected ................... YES
   VERDICT: PASS / FAIL

2. SEMANTIC CORRECTNESS
   Gates ......... 6/6  (CG3 CG5 CG6 CG7 CG10 CG13)
   Progression transitions ................ expected
   Stage publication gating ................ correct
   Leg identity / nesting ................. distinct
   Terminality vs completion .............. separated
   Reclassification vs retargeting ........ disjoint counters
   Mutations M3 M4 M5 detected ............ YES
   VERDICT: PASS / FAIL

3. DETERMINISM AND INTEGRITY
   Gates ......... 3/3  (CG8 CG9 CG11)
   Repeat-replay agreement ................ exact
   UNDEFINED typing preserved ............. yes
   Expired-as-active count ................. 0
   Multi-axis double-count ................. 0
   Tier D evicted under budget ............. 0
   Mutations M2 M6 M7 detected ............ YES
   VERDICT: PASS / FAIL

COMMANDS AND EXIT STATUS
   cargo test -p market-analyzer --test 'gate_*'   -> exit <n>
   ./manage.sh test-core                            -> exit <n>
   ./manage.sh test-engine                          -> exit <n>
   ./manage.sh test-doc                             -> exit <n>

════════════════════════════════════════════════════════════════
OVERALL: PASS / FAIL     (13/13 gates)
INVENTORY (84 readings): LOCKED / UNLOCKED
════════════════════════════════════════════════════════════════
```

### 9.1 Minimum evidence for a green verdict

A verdict is **reproducible or it is not a verdict.** All of the following must be present.

| Category | Minimum evidence |
|---|---|
| **Causality** | Zero look-ahead violations. CG2 future-perturbation passes across ≥4 futures. Replay consumes **only** eligible historical inputs — verified, not assumed. M1 detected. |
| **Semantic correctness** | Every specified boundary case passes: target revision, permanent reference invalidation, nested legs, typed `UNDEFINED`, event expiry, reclassification-vs-retargeting. M3 M4 M5 detected. |
| **Determinism & integrity** | Repeated replay agrees exactly. Canonical identities and timestamps stable. Retention limits respected and Tier D never evicted. M2 M6 M7 detected. The report corresponds to **the exact tested revision**. |

### 9.2 The report is a claim about a specific revision

A green report with no commit hash describes nothing. The banner carries the source commit, spec
version, harness version, fixture checksums, and the mutations exercised.

> **A report whose commit does not match the merged tree is void.** VC fails the run rather than
> re-running to obtain a matching one.

**The report ships in the commit message.** An uncommitted green run is not evidence.

---

## 10. Failure handling

| Failure | Response |
|---|---|
| Gate fails | Phase halts. No partial commit. RV files a finding with the first differing bar. |
| Look-ahead violation found **after** P6 opens | Inventory re-freezes. Inventory phases revert. This is the one genuinely expensive failure and the reason the gate comes first. |
| Spec and code disagree | **The spec wins.** Either the spec was misread (fix the code) or the spec is wrong (change request, ORCH approves, `test-doc` re-stamped). Never edit the spec to match the code without a change request. |
| Harness cannot detect a seeded violation | P1 does not exit. A harness that cannot fail is not a harness. |
| Gate passes but RV finds a defect the gates do not cover | RV's finding outranks VC's green. The gate matrix grows. |
| Test authored by the implementer | Test is discarded and rewritten by T from the spec. |
| A reading justified by backtest profitability | **Rejected.** That settles a strategy question, not a reading's (**P10**, `framework.md` §8.2). The reading is judged on the causal gate and on its effect on the axis output. |
| A stochastic or fitted step introduced anywhere | **Rejected.** Nothing is trained, fitted, sampled at random or simulated stochastically (**P9**, **I43**). |
| A reading approved because it "generalises" | **Rejected.** There is no fitted model here to generalise, and nothing in the layer predicts (**P1**). |

**Rollback.** Every phase is one commit. `git revert <phase-commit>` is the rollback; no phase
mutates another phase's files except by explicit declaration in its work items.

---

## 11. Anti-goals

Things this plan deliberately does **not** do, so they are not smuggled in later.

| Anti-goal | Why |
|---|---|
| Open the inventory before P5 | 84 readings on an unverified foundation means 84 × N unknown behaviours |
| Test against the implementation instead of the spec | Tests the code, not the correctness of the code |
| Accept a schema-consistency result as the gate | Explicitly rejected — schema and causality are different properties |
| Compare only final indicator values | A series can agree at the endpoint and disagree mid-history; compare snapshots |
| Let a phase approve itself | S1–S5 exist for this |
| Add a fifth axis, a probability, or a confidence score | F1, I1, I9, I21 |
| Widen `STABILITY_WINDOW` to unblock stage publication | I26, L18 — stability exists to keep the reference honest |
| Force a reference to make progression available | I26 — a provisional estimate beats a false definitive one |
| Call `STABILITY_WINDOW` "temporal stability" | §5.1 — it is an observation count, full stop |
| Clamp `progress` to `[0, 1]` | L4 — clamping destroys the two most informative values |
| Revive an expired event record | E4 — reactivation is a new identity |
| Change any of F1–F10 to admit an indicator | That is a change request, not a feature |

---

## 12. Appendix — file map and commands

### 12.1 New code (all under `crates/market-analyzer`)

```
src/legs/mod.rs               LegRecord, structure_level, identity
src/legs/progress.rs          progress arithmetic + degenerate guard + noise floor
src/legs/stability.rs         stability machine, STABILITY_WINDOW, REVISION_BUDGET
src/legs/classification.rs    major/minor by self-percentile rank
src/events/mod.rs             canonical identity, six states
src/events/lifecycle.rs       five transitions
src/replay/mod.rs             simulated clock, fail-closed assertions
src/replay/snapshot.rs        immutable snapshot recorder
src/replay/perturb.rs         future-bar perturbation
src/replay/differ.rs          element-wise snapshot differ
tests/gate_g{1..12}_*.rs      one file per gate
```

`core-domain` carries the public DTOs so `api-gateway` can serialise them; the logic stays in
`market-analyzer`, matching the existing unidirectional dependency graph.

### 12.2 Commands

```bash
# Gate (per gate)
cargo test -p market-analyzer --test gate_g1_availability

# Gate (all twelve)
cargo test -p market-analyzer --test 'gate_*'

# Regression floor
./manage.sh test-core
./manage.sh test-golden
./manage.sh test-indicators
./manage.sh test-engine
./manage.sh test-doc
./manage.sh test
```

### 12.3 Reconciliation items — all decided

Surfaced during planning and resolved by formal change request. **No implementation exception is
permitted in place of these decisions.**

| # | Item | Decision | Recorded in | Lands before |
|---|---|---|---|---|
| **R1** | Spec Tier A = 250; registry declares `INDICATORS_MAX_BARS_REQUIRED = 300`. Do both govern the same resource? | **Same resource, different tiers.** Tier A → **300 bars**. The registry is **not** lowered — an existing assertion already locks it to the real maximum. | [CR-1](01-17-change-requests.md#cr-1--r1-tier-a-vs-the-indicators-calculation-floor) → `retention.md` **T1** | P2 exit |
| **R2** | `history_budget.rs` evicts to a 50-entry floor. Which resource, and does it touch Tier D? | The floor is an **eviction floor on Tier B**. **Tier D is never evicted.** Eviction is **lifecycle-neutral** — a storage event may never produce a semantic claim. | [CR-2](01-17-change-requests.md#cr-2--r2-history_budget-vs-tier-b-and-tier-d) → `retention.md` **T2 T3**, `leg.md` **L24** | P2 exit |
| **R3** | `candle_buffer.size` = 500 and Tier B = 500. Same knob? | **Separate settings.** Coincident defaults are permitted; a shared knob is not. A violating config is rejected at load, not clamped. | [CR-3](01-17-change-requests.md#cr-3--r3-candle_buffersize-vs-tier-b) → `retention.md` **T4 T5** | P2 exit |
| **R4** | 76 readings map to 52 shipped indicators. Is the mapping machine-checkable? | **No** — `IndicatorMeta` has no readings field today. The **registry becomes authoritative**; every reading declares a producer or explicit `derived_from`. | [CR-4](01-17-change-requests.md#cr-4--r4-76-readings-mapped-to-52-shipped-indicators) → `framework.md` **I27–I32** | **P1** — CG11 cannot test double-counting without it |

> **CR-4 is not a P6 item.** It was scheduled for P6 and is now scheduled ahead of the gates that
> consume reading identity. Ordering note, not a scope change.

---

## 13. Execution order at a glance

```
P0 SPEC FREEZE + CR-1..CR-5 ... ✅ COMPLETE
        │
MS-1 FIRST MILESTONE ......... CG1 + M1 only - baseline/inject/catch/repair
        │                      MUST fail at the availability assertion
                      (demonstrates one specific defect; establishes nothing else)
P1 REPLAY HARNESS ............. generation + drift check (CR-5) closes V-1 gap
        │                      gates: CG2 CG12   mutations: M1
        ▼
P2 LEG + PROGRESSION ......... gates: CG1 CG3 CG4 CG5 CG6 CG7 CG13
        │                      mutations: M3 M4 M5 M7
        │                      CR-1..CR-5 must have landed
        ▼
P3 EVENT LIFECYCLE ............ gates: CG8 CG9   mutations: M2
        │
P4 OWNERSHIP + TERMINALITY ... gates: CG10 CG11  mutations: M6
        │
P5 FULL GATE + REPORT ......... gates: 13/13 → three categories
        │                      mutations: M1..M7 all detected
        ▼  ┌─────────────────────────────────────────┐
   ⛔ HARD GATE — INVENTORY UNLOCKS ONLY HERE      │
        ▼  └─────────────────────────────────────────┘
P6 INVENTORY (84) ............. per batch, 13/13 must hold
        │
P7 SURFACE ................... ./manage.sh test-ui
```

---

### 13.1 Identifier namespaces

Every identifier belongs to exactly one namespace. Collisions are forbidden - a milestone was
once called `M0` while `M1`-`M7` denoted mutations, and the two namespaces had to be separated.

| Prefix | Namespace | Members |
|---|---|---|
| `G` | **Gates** | CG1–CG13 |
| `M` | **Mutations** | M1–M7 |
| `P` | **Phases** | P0–P7 |
| `MS` | **Milestones** | MS-1 … MS-4 |
| `CR` | **Change requests** | CR-1 … CR-5 |
| `F` | **Frozen items** | F1–F10 |
| `S` | **Separation-of-duty rules** | S1–S5 |
| `I` / `L` / `T` / `E` / `V` | **Invariants** | framework / leg / retention / event / visual |
| `A` / `P1`–`P3` (§4 of `leg.md`) / `S-N` / `E-N` | **Leg-local rules** | availability / progression / scale / eviction |
| `FX` | **Perturbation fixtures** | FX1–FX4 |

> **No prefix may name more than one kind of thing.** A milestone is `MS-n`, never `Mn`,
> because `Mn` already means a mutant. If a new identifier would collide, the *new* one renames —
> not the established series.

---

### 13.2 Milestone sequence

| # | Milestone | Provides evidence for | Does **not** establish |
|---|---|---|---|
| **MS-1** | CG1 + M1 | The harness detects **one** precisely specified availability defect | That CG2–CG13 work; that mutation detection is reliable across classes |
| **MS-2** | CG2 + FX1–FX4 | Causal invariance across **four distinct perturbation mechanisms** | That the gates are complete |
| **MS-3** | CG3–CG13 + M2–M7 | Behavioural coverage and mutation detection across classes | That the evidence is reproducible |
| **MS-4** | V-4 | The evidence is bound to the exact merged commit and spec | — |

> **MS-1 must not be expanded into a larger gate suite before the baseline / mutation / repair cycle
> works and produces trustworthy evidence.** A wider suite that is not yet trusted produces more
> red without producing more knowledge.

---

**Verdict rule.** The 84 readings unlock on **P5 green across all three report categories**, with
**M1–M7 all detected**, and a reproducibility banner whose commit matches the merged tree.

Not on schema consistency. Not on code review. Not on a passing unit suite. On thirteen gates
that each executed runtime behaviour, caught an injected violation when one existed, and named the
exact revision they ran against.
