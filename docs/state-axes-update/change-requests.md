# Change Requests

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

The formal process for amending a frozen item. Nothing here is an implementation exception —
each CR carries a decision, the evidence it was decided on, and the exact spec text it produces.

| # | Title | Status | Amends | Verdict |
|---|---|---|---|---|
| [CR-1](#cr-1--r1-tier-a-vs-the-indicators-calculation-floor) | Tier A vs the indicator calculation floor | **Accepted** | F3, `retention.md` | **Tier A = 300 bars.** Raise, do not lower the registry. |
| [CR-2](#cr-2--r2-history_budget-vs-tier-b-and-tier-d) | `history_budget` vs Tier B and Tier D | **Accepted** | F3, F5, `retention.md` | Floor is an **eviction floor on Tier B**. Tier D is **never evicted**. Eviction is lifecycle-neutral. |
| [CR-3](#cr-3--r3-candle_buffersize-vs-tier-b) | `candle_buffer.size` vs Tier B | **Accepted** | F3, `retention.md` | **Separate settings**, retained as separate. Both default 500; coincidence is not identity. |
| [CR-4](#cr-4--r4-76-readings-mapped-to-52-shipped-indicators) | 76 readings mapped to 52 shipped indicators | **Accepted** | F1, F7, F8, `framework.md` | **Registry is authoritative.** Add a declared `readings` mapping with machine-checkable invariants. |
| [CR-5](#cr-5--the-registry-is-the-only-source-of-reading-totals) | The registry is the only source of reading totals | **Accepted** | F1, `inventory.md`, `README.md` | **Displayed totals are generated.** Hand-maintained copies are a defect; drift fails the build. |
| [CR-6](#cr-6--inventory-revision-162--160-record-of-a-removal-that-was-never-recorded) | Inventory revision 162 → 160 | **Accepted — retroactive record** | F1, `inventory.md` | **One duplicate reading removed from two rows.** 162 / 76 / 86 → **160 / 76 / 84**. |
| [CR-7](#cr-7--direction-persistence-had-no-producer-on-its-own-axis) | Direction's persistence had no producer | **Accepted** | F1, F7, `visual-vocabulary.md`, inventory | **Persistence is an axis-level derived output**, not a reading. The similarly-named Momentum reading is renamed to remove the collision. |

**Ordering constraint.** CR-4 and CR-5 are consumed by test infrastructure and must land before
the gates that depend on reading identity or its totals. CR-1–CR-3 must land before **P2** exits.
CR-6 is a record of an already-applied change and carries no ordering constraint.

---

## CR-1 — R1: Tier A vs the indicator calculation floor

### The question

Tier A is specified as **250 bars**. The registry declares
`INDICATORS_MAX_BARS_REQUIRED = 300`. Do both govern the same resource?

### Evidence

The existing code already answers this, in [`retention.md`](retention.md)'s own territory:

> "The three candle numbers are **independent tiers**: `INDICATORS_MAX_BARS_REQUIRED = 300`
> (indicator calculation floor), this `size` (default **500**, the internet warmup), and
> `HIST_BUFFER_MAX = 1000` (absolute in-memory cap)."
> — `crates/config-models/src/models.rs:152-153`

> "Canonical buffer size from `[candle_buffer] size` … **Independent of** the indicator floor
> (`INDICATORS_MAX_BARS_REQUIRED = 300`) and the absolute cap (`HIST_BUFFER_MAX = 1000`)."
> — `crates/market-analyzer/src/analyzer/mod.rs:86-89`

So: **same resource, different tiers, and the separation is already documented in the code.**

| Quantity | Value | Question it answers |
|---|---|---|
| `bars_required` (max) | 300 | How many bars must an indicator **receive** before it may emit a reading? |
| **Tier A** | 250 (spec) | How many bars must be **retained** for percentile, slope, rate of change, derivative? |
| `candle_buffer.size` | 500 | How deep is the **warmup fetch**? |
| `HIST_BUFFER_MAX` | 1000 | Absolute in-memory **cap**. |

### The conflict

Tier A at 250 is **below** the 300-bar calculation floor. An indicator requiring 300 bars can
emit a value, but only 250 bars of it survive retention — so a percentile computed over Tier A is
ranked against a sample shorter than the window the indicator itself consumes. That is not a
naming disagreement; it is a resource that cannot satisfy both requirements.

### The decision

**Tier A becomes 300 bars.** The binding constraint is the maximum `bars_required`, and the
registry requirement is **not lowered**.

Lowering `INDICATORS_MAX_BARS_REQUIRED` to 250 is additionally **not available as a fix** — an
existing assertion already locks the constant to reality:

```rust
// crates/market-analyzer/src/indicators/registry.rs:1690-1693
assert_eq!(
    max_bars, INDICATORS_MAX_BARS_REQUIRED,
    "INDICATORS_MAX_BARS_REQUIRED must match the actual maximum bars_required"
);
```

That test is derived from `price_trend_sharpe`'s window. Lowering the constant would not make the
numbers agree; it would **break an existing test and misdeclare what the indicator needs**.

### Resulting invariant

```
INDICATORS_MAX_BARS_REQUIRED (300) <= TIER_A (300) <= BUDGET.retention (500)
                                                         <= candle_buffer.size (500)
                                                         <= HIST_BUFFER_MAX (1000)
```

**`TIER_A >= max(bars_required)`, always.** Stated as an invariant so that adding an indicator
with a longer window cannot silently under-fill Tier A.

### Spec text produced

`retention.md` §Retention tiers, row **A**, and new invariant **T1**.

---

## CR-2 — R2: `history_budget` vs Tier B and Tier D

### The question

What resource does the existing byte budget actually govern, and what is the 50-entry policy?

### Evidence

```rust
// crates/market-analyzer/src/analyzer/history_budget.rs
pub const DEFAULT_SNAPSHOT_HISTORY_RETENTION: usize = 500;   // Tier B
pub const SNAPSHOT_HISTORY_MAX:             usize = 1000;    // absolute cap
pub const DEFAULT_MAX_SNAPSHOT_HISTORY_BYTES: usize = 512 * 1024 * 1024;
pub const SNAPSHOT_HISTORY_FLOOR:          usize = 50;
```

> "Floor on entries per pipeline. Eviction stops here so a chart always has *something* to render
> — the difference between *shorter history* and *no history*."
> — `history_budget.rs`, on `SNAPSHOT_HISTORY_FLOOR`

> "The floor is enforced by the caller … so this primitive stays a pure *make the window this
> size* operation."
> — `history_budget.rs`, on `evict_to_entries`

### The four resources are not interchangeable

This was the substance of the finding. They differ in kind, not merely in size:

| Resource | Kind | Evictable? | Governed by |
|---|---|---|---|
| Raw candles | Input | Yes, by the rolling buffer | `candle_buffer.size` |
| Retained snapshots | Derived output | Yes — Tier B | `history_budget` |
| Event journal | Derived output | Yes, within count **and** age | `history_budget` + [`event-lifecycle.md`](event-lifecycle.md) |
| **Current leg records** | **Live state** | **NEVER** | [`leg.md`](leg.md) only |

### The decision

**1. The 50-entry policy is an eviction floor on Tier B.** It is *not* a capacity, and *not* a
minimum retained count in the semantic sense — it is the point below which eviction stops so a
chart can still render something.

**2. Tier D is never evicted by the global byte budget.** Memory pressure shortens chart history.
It must never delete the current leg record. This is the single most important clause in this CR.

**3. Eviction is lifecycle-neutral.** The budget is a *storage* mechanism and may not touch
*validity*:

| Rule | |
|---|---|
| **E-N1** | Eviction never changes an event's lifecycle state. An evicted `active` event does **not** become `expired`. |
| **E-N2** | Expiry never causes eviction. A semantic transition never frees storage as a side effect. |
| **E-N3** | Eviction never releases a `Budget` charge on a Tier D record — Tier D records are not charged to the snapshot budget at all. |
| **E-N4** | An event evicted from the journal is reported as `evicted`, never as `active` or as evidence. |

Without **E-N1**, memory pressure would silently manufacture "this divergence expired" — a
*false semantic claim* produced by a *storage* event. That is precisely the
"retained record confused with a valid event" failure the gate exists to catch.

### Spec text produced

`retention.md` — new §"What the byte budget governs" and invariants **T2**, **T3**.

---

## CR-3 — R3: `candle_buffer.size` vs Tier B

### Evidence

They are **already separate settings** in the codebase — `candle_buffer.size` is a config field;
`Budget::retention` is configured independently (`DEFAULT_SNAPSHOT_HISTORY_RETENTION = 500`).

### The decision

**Keep them separate.** Both default to 500; that is a coincidence of defaults, not a shared
knob. Sharing the number never made them the same quantity.

| Setting | Measures | Must not be changed because |
|---|---|---|
| `candle_buffer.size` | **Raw candle capacity** and warmup fetch depth | the retention policy moved |
| `Budget.retention` | **Retained snapshot window** (Tier B) | the fetch depth moved |

The reason is directional, and both directions are real:

- Shrinking `candle_buffer.size` does **not** shrink snapshot retention.
- Raising `Budget.retention` above `candle_buffer.size` **cannot be warmed** — the pipeline would
  retain a window it never had candles for.

**Startup consistency check (fails closed):**

```
Budget.retention <= candle_buffer.size <= HIST_BUFFER_MAX
```

A configuration violating this is rejected at load, not silently clamped.

### Spec text produced

`retention.md` — new §"Independent settings" and invariants **T4**, **T5**.

---

## CR-4 — R4: 76 readings mapped to 52 shipped indicators

### Evidence

`IndicatorMeta` currently carries `key`, `display_name`, `group`, `class`, `render`,
`directional`, `supports_divergence`, `signal_types`, `default_weight`, `default_enabled`,
`config_params`, `value_format`, `value_source`, `color`, `guide_section`, `bars_required`.

**It has no `readings` field.** Verified: `axes`, `owner_axis`, `axis_codes` do not appear in
`registry.rs` at all; `readings` appears only inside prose comments. The 160-reading inventory in
[`inventory.md`](inventory.md) is therefore **maintained by hand and not machine-checkable**.

### The decision

**The registry is authoritative.** Each reading declares its producer.

**1. `IndicatorMeta` gains a `readings` field** — a list of stable reading IDs this indicator
produces. One indicator may produce several; that is normal.

**2. A derived-reading table declares readings with no single producer.** Consolidated
divergence, cross-axis composites and staged-aggregation outputs name their `derived_from` inputs
explicitly instead.

**3. Multi-reading producers are one family by default.** Declaring that RSI produces three
readings does **not** make those three independent votes — that is exactly the Williams %R /
Stochastic %K error this framework already documents. Family membership is declared, not inferred
from count.

### Invariants

| # | Rule |
|---|---|
| **I27** | Every reading in [`inventory.md`](inventory.md) resolves to a declared producer **or** an explicit `derived_from` list. No orphans. |
| **I28** | Reading IDs are **stable and never renumbered**. A retired reading is tombstoned, not reused. |
| **I29** | Every producer key referenced by a reading exists in `INDICATORS`, or is a declared derived-composite. Dangling references fail. |
| **I30** | A reading's owner axis is one of its declared axes. Ownership is single; consumption is many (F8). |
| **I31** | Inventory coverage reconciles exactly: Σ axis counts = 160, and present/new split = 76/84. |
| **I32** | A producer declaring N readings is asserted to **N-1 families or fewer**. Declaring many readings never multiplies weight by default. |

### Spec text produced

`framework.md` — §5.7 extension and invariants **I27–I32**.

### Ordering

CR-4 **must land before the gates that consume reading identity** — G11 in particular cannot test
cross-axis double-counting without knowing which readings a producer declares.

---

## CR-5 — The registry is the only source of reading totals

### The question

CR-4 makes the registry authoritative for **reading identity**. Does it also own the **numbers**
displayed across the documentation, or may a second inventory restate them?

### The finding

Today it does not. [`inventory.md`](inventory.md) holds 160 hand-maintained rows, and the README,
the framework and the plan each restate totals independently. Nothing ties them together — which
is precisely how the earlier hand-maintained inventory came to disagree with the registry in the
first place. Making the registry authoritative for *identity* while leaving the *totals*
hand-maintained fixes the smaller half of the problem.

### The decision

**One authoritative record. Everything else is generated.**

| Artifact | Status |
|---|---|
| The reading registry | **Authoritative** |
| [`inventory.md`](inventory.md) | **Generated** — never hand-edited |
| README axis counts, totals, present/new splits | **Generated** |
| Per-axis reading tables in `framework.md` | **Generated** |

**Drift is a failing check, not a merge conflict.** `./manage.sh test-doc` regenerates every
generated artifact and fails on any byte difference. There is no workflow in which a human
resolves inventory drift by editing the inventory.

| # | Rule |
|---|---|
| **I35** | Every generated artifact is byte-identical to freshly generated output. Drift fails the build. |
| **I36** | No hand-maintained document restates a registry total. Where a total appears, it is generated. |

### The honest cost

Until the generator exists, `inventory.md` **is** hand-maintained and I35 cannot be enforced. So
the current state is:

| | Status |
|---|---|
| Registry schema defined | ✅ `framework.md` §5.7 |
| Generator implemented | ⬜ lands in **P1** |
| Drift check in `test-doc` | ⬜ lands in **P1** |
| I35 enforceable | ⬜ **from P1 exit onward** |

> **I35 is declared but not yet enforceable.** It is recorded as a specification obligation with a
> named landing phase, rather than asserted as a property the system does not yet have. The gate
> report distinguishes declared invariants from enforced ones.

### Ordering

CR-5 lands in **P1**, with CR-4. G11 consumes reading identity and therefore the totals.

---

## CR-6 — Inventory revision 162 → 160

### The discrepancy

Two published totals existed for the same inventory:

| | Readings | Present | New | Arithmetic |
|---|---|---|---|---|
| Earlier | 162 | 76 | 86 | ✅ consistent |
| Current | **160** | 76 | **84** | ✅ consistent |

Both add up, so the change was **not** detectable by summing. The user of these documents noticed
that the current total implies **two readings removed with the present count unchanged**, and asked
for the record. There wasn't one.

### What was actually removed

**Not two readings — one reading, listed twice.**

| | |
|---|---|
| **Removed** | `Directional Persistence Ratio` |
| **Rows removed** | Momentum #29 **and** Progression #15 — the same reading filed under two axes |
| **Reason** | Its formula was **byte-identical** to `Path Efficiency`, which remains at Progression #13 |
| **Formula (both)** | `(C − C₋ₙ) / Σ│ΔC│` |
| **Status of both rows** | **NEW** — which is why the **present** count did not move |

| Axis | Before | After | Present | New |
|---|---|---|---|---|
| Direction | 50 | 50 | 31 | 19 |
| Momentum | 51 | **50** | 26 | 24 |
| Structure | 35 | 35 | 16 | 19 |
| Progression | 26 | **25** | 3 | 22 |
| **Total** | **162** | **160** | **76** | **84** |

`19 + 24 + 19 + 22 = 84` — the drop lands entirely in **new**, exactly as it must when both
removed rows were unbuilt.

### What survives, and why it is not a third duplicate

> **Resolution: RETAIN THE DISTINCTION.** The normative formulas differ, so these are **not**
> duplicates. Their relationship is exact and stated below.

Let `N = C − C₋ₙ` (net displacement) and `D = Σ│ΔC│` (gross path). For `D > 0`:

```
Kaufman Efficiency Ratio   = |N| / D        (UNSIGNED — absolute numerator)
Signed Path Efficiency     =  N  / D        (SIGNED   — numerator keeps its sign)

    PE = sign(N) · ER          whenever D > 0
```

**For any downward net movement these differ in sign.** They are therefore **not** identical under
their normative definitions, and no reading is removed here.

| | **Kaufman Efficiency Ratio** | **Signed Path Efficiency** |
|---|---|---|
| Owner axis | **Structure** | **Progression** |
| Question answered | *How direct was the path, **regardless of which way**?* | *How direct was the path, **and in which direction**?* |
| Sign behaviour | Always ≥ 0 | Follows the leg |
| On a down-leg | Reports the same positive value as the up-leg | Reports negative |
| Consumers | Stage classification (input) | The reading itself |

> **The distinction is load-bearing.** A direction-neutral Structure reading cannot substitute for
> a Progression reading that knows *which way* the leg ran — the sign is the entire difference in
> what each is for. Collapsing them would either destroy Progression's directional information or
> make Structure directional, violating **P4**.

#### The comparison that *was* a duplicate — do not conflate the two

| Comparison | Formulas | Outcome |
|---|---|---|
| `Directional Persistence Ratio` **vs** `Path Efficiency` | `(C−C₋ₙ)/Σ│ΔC│` **vs** `(C−C₋ₙ)/Σ│ΔC│` — **identical, both signed** | **Removed** (CR-6) |
| `Kaufman Efficiency Ratio` **vs** `Path Efficiency` | `│C−C₋ₙ│/Σ│ΔC│` **vs** `(C−C₋ₙ)/Σ│ΔC│` — **differ by sign** | **Retained** |

> **The exact normative formulas decide which applies — never a desire to reduce a count.** Only
> the first pair was identical. The second is a signed/unsigned pair answering different questions.

Recorded in [`framework.md`](framework.md) §5.2, *Known equivalences*.

### Why this needed a CR at all

The removal itself was correct and was reported — but **only conversationally.** It was applied by
pattern-matching edit to `inventory.md`, which means:

- no document recorded what was removed, or why;
- the counts changed in three places with no single authoritative source;
- **no check would have detected a wrong number.** The sums still balanced.

> **This is the exact failure CR-5 exists to eliminate, demonstrated on CR-5's own subject.** A
> hand-maintained count can be wrong while remaining arithmetically consistent — which is why
> consistency checking was not sufficient and why **I35** (byte-equality with generated output) is
> the check that would actually have caught it.

**Correct label until I35 is operational:** `REVIEWED — SCOPE RECORDED`. The totals are now
reconciled across every document *and* have an audit trail, but they are **reconciled by hand,
not machine-verified against the registry.**

### Strengthening the equivalence claim (CR-6 addendum)

The removal is justified on **complete calculation semantics**, not formula notation alone:

| Dimension | `Directional Persistence Ratio` | `Path Efficiency` | Identical? |
|---|---|---|---|
| Formula | `(C − C₋ₙ) / Σ│ΔC│` | `(C − C₋ₙ) / Σ│ΔC│` | ✅ |
| **Input** | `Close` only | `Close` only | ✅ |
| **Lookback** | *n* bars | *n* bars | ✅ |
| **Calculation semantics** | signed net over gross path | signed net over gross path | ✅ |
| **Ownership** | filed to two axes | filed to one | ❌ — the actual defect |

The ownership split was the only real difference, and it was the defect: one quantity carrying two
axis claims.

### Zero-denominator validity for the efficiency family (CR-6 addendum)

`PE = sign(N) · ER` holds only when `D > 0`. The degenerate case is specified in
[`leg.md`](leg.md) §4.0 for `progress`; the same rule is adopted for both efficiency readings:

| Condition | Result |
|---|---|
| `D = Σ│ΔC│ = 0` (a perfectly flat series) | **`UNDEFINED`** — never `0`, never `1`, never `NaN` |
| `D > 0`, `N = 0` | `0` — a legitimate value: net displacement zero over a non-zero path |

> `ER` and `PE` may **not** fall back to a default denominator. A flat window has no path
> efficiency, and reporting `1` would assert the most efficient path possible on the basis of no
> evidence.

---

## CR-7 — Direction's persistence had no producer on its own axis

### The defect

`Direction` declares **persistence** as its size variable. Three places referenced it, and **no
two agreed**:

| Source | What it said |
|---|---|
| `visual-vocabulary.md` | "Bars since the sign of the **confluence score** last changed" — an **aggregate, post-aggregation** quantity |
| `framework.md` §10.1 | "bars since the **direction** last flipped" — consistent prose, no formula |
| The only spec file | `Bars Since Direction Flip`, filed to **`new/momentum/`** with axis code **M**, computed as "bars since **sign(slope)** last changed" — a **per-indicator, pre-aggregation** quantity |

**So Direction's declared size variable had no producer on Direction's own axis**, and the
nearest-named reading measured a *different* quantity on a *different* axis. This is the same
naming-collision class as CR-6, one level up: `Directional Persistence Ratio` collided on
**formula**; `Bars Since Direction Flip` collides on **name** while measuring something else.

### The decision

**1. Direction's persistence is persistence *through time*.**

The question the user posed — *agreement across durations, or persistence through time?* — has an
unambiguous answer, and it is the second:

| Candidate | Verdict |
|---|---|
| **Persistence through time** — bars since the axis position last changed sign | ✅ **This is it.** |
| Directional agreement across durations | ❌ **A different quantity, and it is not merged here** |

Agreement across durations is an **aggregate over the ladder**; persistence through time is a
**per-duration duration measure**. §9 renders Direction aggregated but Momentum, Structure and
Progression per duration, so the two are not even computed at the same granularity — folding them
together would be a category error.

**2. It is an axis-level derived output, not a reading.**

Persistence is a function of the **aggregate confluence score**, so it is produced *after*
aggregation and cannot be an indicator. This mirrors `stage` on Progression.

#### Persistence has two representations, not one

> **Correction (CR-7 addendum).** An earlier draft gave this variable `Domain [0, 100]` and a cap
> of 100 bars. That conflated **two different quantities** and produced a value that does **not**
> satisfy **V3** — "every size is a percentile against its own history, never an absolute value."

| Representation | Definition | Domain | Role |
|---|---|---|---|
| **`direction_persistence_bars`** | Raw count of bars since the last sign change | `[0, ∞)` — **unbounded upward** | The **measurement**. Canonical. |
| **`direction_persistence_pct`** | Percentile rank of that raw count against its own Tier A history | `[0, 100]` | The **rendered size**. Satisfies **V3**. |

| | |
|---|---|
| **Producer** | The Direction aggregation stage — **not** any indicator |
| **Level** | **Axis output.** No reading ID; no aggregation weight. |
| **Raw cap** | ⛔ **Removed.** There is no 100-bar cap. A direction that holds for 400 bars must be able to say so. |
| **Radius source** | `direction_persistence_pct` **only** — never the raw count |

> **V3 is a hard constraint on the rendering, not on the measurement.** The raw count is the
> honest fact; the percentile is the honest *comparison*. Reporting the raw count as a radius would
> make "held 100 bars" mean the same thing on a quiet instrument and a violent one — which is
> exactly the absolute-value comparison P2 forbids.

#### `sign(direction_bias) = 0` is specified, not left implicit

The previous definition said "since the **sign** last changed" without stating what happens when
the bias is exactly zero.

| Rule | |
|---|---|
| **CR7-R4** | **Neutrality is a corridor, not a state.** `sign(0)` is **not** a state and entering it **does not** reset persistence. |
| **CR7-R5** | Persistence resets **only** on a genuine sign reversal: `+ → −` or `− → +`. |
| **CR7-R6** | A bias oscillating around zero without reversing sign is **one continuous state**, not a series of flips. |

**Why this rule.** If `sign(0)` counted as a state, a bias hovering at neutral would reset
persistence on every bar — the reading would flicker to zero continuously and would measure
*noise proximity*, not persistence. Treating neutrality as a corridor answers the intended question
("how long has the direction held?") instead of an accidental one.

**The consequence is stated so it is not mistaken for a bug:** a direction that decays into
neutrality and returns shows a **long** persistence, because it never reversed. If that is
undesirable for a given use, the axis needs an explicit *neutral-exit* event — it does not get it
by accident through sign arithmetic.

#### Initialization — before the first observed reversal

> **Correction (CR-7 addendum).** The reset rules above govern reversals. They say nothing about
> what happens **before the first one has ever been observed**, and that boundary was unspecified.

| # | State | `direction_persistence_bars` | Rendered as |
|---|---|---|---|
| **1** | **No non-neutral direction observed yet** — warm-up, or bias has not left neutral | **`UNDEFINED`** — **not `0`** | **Outline only**, per readiness **V4**. No radius. |
| **2** | **First non-neutral direction established** at bar *b* | `0` at bar *b*, then increments | Radius from percentile |
| **3** | **Genuine sign reversal** at bar *r* | `0` at bar *r*, then increments | Radius from percentile |
| **4** | **Neutral interval, no reversal** | **Continues counting** (CR7-R6) | Radius from percentile |

**Why `UNDEFINED` and not `0` at startup.** A `0` there would assert that a reversal *just
happened*. Nothing happened — a direction was *never yet established*. Reporting `0` converts an
initialization artifact into a directional claim about the market. This is the same rule as **L6**:
a `forming` leg publishes no stage, because an assessment built on nothing is not a reading.

**Why no seed count.** Starting from an arbitrary positive value would assert persistence the
system never observed — the mirror-image error, and the harder one to detect because the number
looks plausible.

#### The zero is ambiguous — so a companion field disambiguates it

`0` means two entirely different things in states 2 and 3, with opposite implications:

| `0` means | Implication |
|---|---|
| `first_establishment` | **Nothing has happened yet.** A direction now exists. |
| `reversal` | **Something just happened.** A prior direction just ended. |

| Field | Values |
|---|---|
| `persistence_origin_kind` | `none` · `first_establishment` · `reversal` |
| `persistence_origin_bar` | Bar index of that event |

| Rule | |
|---|---|
| **CR7-R7** | Before the first establishment, `direction_persistence_bars` is **`UNDEFINED`** and renders as outline. It is never `0` and never seeded. |
| **CR7-R8** | `persistence_origin_kind` is **mandatory** whenever persistence is defined. A bare `0` without it is not a valid record. |
| **CR7-R9** | `first_establishment` and `reversal` are reported as **different facts** and may never be conflated in storage, transport, or display. |

> **This is the same defect class as `UNDEFINED` on a degenerate denominator.** A `0` that means
> "nothing yet" and a `0` that means "something just changed" are indistinguishable to a consumer
> that reads only the number — so the number is not sufficient and the record carries the
> disambiguator alongside it.

| | |
|---|---|---|
| **Measurement** | `direction_persistence_bars` = bars since the last genuine sign reversal, or since first establishment |
| **Rendered size** | `direction_persistence_pct` = percentile of the raw count against its own Tier A history |
| **Origin kind** | `persistence_origin_kind` = `none` / `first_establishment` / `reversal` + `persistence_origin_bar` |
| **Pre-establishment** | **`UNDEFINED`** - renders as outline (readiness V4), never `0`, never seeded |
| **Producer** | The Direction aggregation stage - **not** any indicator |
| **Level** | **Axis output.** No reading ID; no aggregation weight. |

**3. The Momentum reading is renamed to remove the collision.**

`Bars Since Direction Flip` → **`Bars Since Trend Slope Flip`**, because that is what it measures:
the sign of **one indicator's slope**, pre-aggregation. It stays on **Momentum** where it belongs.

| | Before | After |
|---|---|---|
| Name | Bars Since Direction Flip | **Bars Since Trend Slope Flip** |
| Axis | M | M (unchanged) |
| Measures | Sign of one indicator's slope | Sign of one indicator's slope (unchanged) |
| Collides with | Direction's size variable | **Nothing** |

**The rename is applied now precisely because the reading is NEW and unbuilt.** After
implementation it would be a breaking change for no benefit.

### The generalisable rule

> **An axis-level variable is not a reading.** If a variable can only be computed after
> aggregation, it has no reading ID, carries no aggregation weight, and must never share a name
> with a per-indicator constituent that *looks* like it.

| Rule | |
|---|---|
| **CR7-R1** | Every declared axis variable names its **producer**. An axis variable with no producer is an incomplete contract. |
| **CR7-R2** | A variable computed **post-aggregation** is an axis output, not a reading. |
| **CR7-R3** | No axis variable may share a name with a constituent reading measuring a different quantity at a different granularity. |

| File | Change |
|---|---|
| [`retention.md`](retention.md) | Tier A 250 → **300**; tier chain invariant; §byte-budget-governs; §independent settings; invariants **T1–T5** |
| [`framework.md`](framework.md) | Invariants **I27–I32**; registry authority; family-by-default rule |
| [`leg.md`](leg.md) | Tier D is never evicted (**T2** cross-reference); scale classification causality; invariants **L19–L22** |
| [`execution-plan.md`](execution-plan.md) | CR-4 before identity-dependent gates; CR-1–3 before P2 exit |
