# Retention Policy

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

Every indicator needs history before any rate, percentile, or second-order reading can be
computed from it. This document defines how much, and — more importantly — the bound on each
tier.

**All retention is bounded.** No tier grows without limit.

A bound alone is not sufficient. It answers *how much do we keep* but not *when does a stored
event stop being true*. That second question is a **lifecycle** and is specified separately in
[`event-lifecycle.md`](01-15-event-lifecycle.md). The two are independent: a journal can sit inside
its count and age bounds and still contain nothing but expired events.

---

## Retention tiers

| Tier | Window | Storage | Sufficient for |
|---|---|---|---|
| **A** | **300 bars** | Rolling series | Percentile rank, slope, rate of change, derivative |
| **B** | 500 bars | Rolling series | Legs, distribution, cross-duration comparison |
| **C** | Last 200 events **and** not older than the Tier B window | Bounded event journal | Event counts, event age, event sequences |
| **D** | Current state only | **O(1) — one record** | Movement origin, leg geometry, reference target |

Tiers A and B are bounded by **storage** only. Tiers C and D are bounded by storage **and** carry
a **validity lifecycle** — [`event-lifecycle.md`](01-15-event-lifecycle.md) and [`leg.md`](01-14-current-leg-record.md)
respectively.

### The tier chain

These quantities are **different tiers of one resource**, not competing names for it. Their
relationship is an invariant, and it is checked at startup:

```
INDICATORS_MAX_BARS_REQUIRED (300) <= TIER_A (300) <= BUDGET.retention (500)
                                                    <= candle_buffer.size (500)
                                                    <= HIST_BUFFER_MAX (1000)
```

| Quantity | Question it answers | Value |
|---|---|---|
| `bars_required` (max) | How many bars must an indicator **receive** before it may emit? | 300 |
| **Tier A** | How many bars must be **retained** for percentile, slope, derivative? | **300** |
| `Budget.retention` | How deep is the **retained snapshot window**? | 500 |
| `candle_buffer.size` | How deep is the **warmup fetch**? | 500 |
| `HIST_BUFFER_MAX` | What is the absolute in-memory **cap**? | 1000 |

> **Tier A was 250 bars and was corrected to 300.** See
> [`change-requests.md`](01-17-change-requests.md) CR-1. Tier A at 250 sat **below** the indicator
> calculation floor: an indicator consuming 300 bars could emit, but only 250 survived retention,
> so its percentile was ranked against a sample shorter than its own input window. The registry
> requirement was **not** lowered — an existing assertion already locks
> `INDICATORS_MAX_BARS_REQUIRED` to the actual maximum `bars_required`, so lowering it would
> break a test rather than resolve the conflict.

---

## Why each bound

**Tier A — 300 bars.** A percentile rank needs a sample large enough that the rank is stable and
the window is long enough to contain a meaningful cycle. 300 is not chosen for convention — it is
**the maximum `bars_required` of any registered indicator**, and Tier A may never fall below it
(invariant **T1**). Retaining fewer bars than an indicator consumes would rank its percentile
against a sample shorter than its own input window.

**Tier B — 500 bars.** Exists to support two things the shorter window cannot: multi-leg
comparison ("is this leg longer than the last one") and cross-duration comparison against the
slowest duration in the ladder. 

**Tier C — bounded on two axes at once.** A journal bounded only by event count is a leak: a
quiet instrument emits few events and a wild one emits thousands, and both would grow until the
count limit. Bounding by count *and* by age fixes both ends. An event older than the Tier B
window is not part of the current structure.

**Tier D — O(1).** This is not a history. It is a handful of scalars describing the *current*
leg, replaced wholesale when a new leg begins. The old record is discarded, not archived.

---

## Tier D in detail

Tier D is what the entire Progression axis is built on. Its full field contract, version
discipline, arithmetic and state machine are specified in [`leg.md`](01-14-current-leg-record.md).

| Field | Purpose |
|---|---|
| Origin price | Where the movement began |
| Origin direction | Which way the leg runs |
| Origin bar index | Age of the movement in bars |
| Origin timestamp | Age in wall-clock time |
| Origin confirmation state | Whether the origin is confirmed or still provisional |
| Prior leg origin | The comparison baseline |
| Prior leg extent | The comparison baseline |
| Reference target value | The stated estimate of extent |
| Reference target basis | Which completion criterion informed it |
| Reference revision count | How often it has moved |
| Reference stability flag | Whether it is moving now |
| Stage classification | The reported ordinal stage |
| Completion criterion met | Which criterion, if any |
| Leg lifecycle state | forming / active / completing / completed / invalidated / superseded |
| Structural level | major / minor |
| Structural classification state | provisional / settled |

Roughly **thirteen scalar fields per instrument per duration per structural level**, plus a
bounded revision log for the reference target.

> **Tier D holds one record per structural level.** With the two levels defined in
> [`leg.md`](01-14-current-leg-record.md) §2.5 that is **two records per instrument per duration** — stated here
> explicitly so the multiplier is never discovered later. The level set is fixed and small, so
> the O(1) guarantee holds. An open-ended level hierarchy would make Tier D unbounded and break
> invariant **I10**.

> **Tier D is bounded but not archived.** The replaced record is discarded. Retention of past
> legs would be Tier B material and belongs there — mixing them would make the O(1) guarantee
> false and silently reintroduce unbounded growth.

---

## What the byte budget governs

A process-wide byte budget exists to shed memory under pressure. It governs a **storage**
resource, and the four resources below differ in kind, not merely in size.

| Resource | Kind | Evictable? | Governed by |
|---|---|---|---|
| Raw candles | Input | Yes — the rolling buffer | `candle_buffer.size` |
| Retained snapshots | Derived output | Yes — **Tier B** | the byte budget |
| Event journal | Derived output | Yes, within count **and** age | byte budget + [`event-lifecycle.md`](01-15-event-lifecycle.md) |
| **Current leg records** | **Live state** | **NEVER** | [`leg.md`](01-14-current-leg-record.md) only |

### The 50-entry floor is an eviction floor

It is the point below which eviction stops so a chart can still render something — the
difference between *shorter history* and *no history*. It is **not** a capacity and **not** a
minimum retained count in the semantic sense. It governs **Tier B** only.

### Tier D is never evicted

Memory pressure shortens chart history. It must never delete the current leg record. A leg record
is **not charged to the byte budget at all**, so there is no accounting path by which it could be
refunded away.

### Eviction is lifecycle-neutral

> **A storage event may never produce a semantic claim.** If it could, memory pressure would
> manufacture statements about the market that no reading ever made.

| Rule | |
|---|---|
| **E-N1** | Eviction never changes an event's lifecycle state. An evicted `active` event does **not** become `expired`. |
| **E-N2** | Expiry never causes eviction. A semantic transition never frees storage as a side effect. |
| **E-N3** | No eviction path touches a Tier D record, and Tier D holds no budget charge. |
| **E-N4** | An evicted event is reported as `evicted` — never as `active`, and never as evidence. |

Without **E-N1**, shedding memory would silently assert "this divergence expired", which is a
false statement about the market produced by a resource decision. That is precisely the
*retained record confused with a valid event* failure the gate exists to catch.

---

## Independent settings

`candle_buffer.size` and the snapshot retention window are **separate settings**. Both default to
500; that is a coincidence of defaults, not a shared knob.

| Setting | Measures | Must not change because |
|---|---|---|
| `candle_buffer.size` | **Raw candle capacity** and warmup fetch depth | the retention policy moved |
| snapshot `Budget.retention` | **Retained snapshot window** (Tier B) | the fetch depth moved |

The coupling is directional, and both directions bite:

- Shrinking `candle_buffer.size` does **not** shrink snapshot retention.
- Raising `Budget.retention` above `candle_buffer.size` **cannot be warmed** — the pipeline would
  retain a window it never received candles for.

A configuration violating the chain below is **rejected at load**, not silently clamped.

---

## Retention invariants

| # | Rule |
|---|---|
| **T1** | `TIER_A >= max(bars_required)` across the whole registry. Always. |
| **T2** | A Tier D record is never evicted, charged, or refunded by the byte budget. |
| **T3** | Eviction never changes an event's lifecycle state, and expiry never causes eviction. |
| **T4** | `Budget.retention <= candle_buffer.size <= HIST_BUFFER_MAX`. Violations reject the config. |
| **T5** | No single number is used to configure two tiers. Coincident defaults are permitted; shared knobs are not. |
| **T6** | Every tier bound is a hard maximum, never a target that may be exceeded under load. |

---

## Default policy

> **Retain Tier A minimum for every indicator on every duration.**

Uniform retention is cheaper to reason about than per-indicator tuning, and the cost of
retaining a series an axis does not yet use is far lower than the cost of a later decision to
retrofit history that was never kept.

Tiers B, C and D are required where an indicator's spec names them.

---

## What each axis needs

| Axis | Dominant tier | Why |
|---|---|---|
| **Direction** | A | Percentile context and slope of the confluence |
| **Momentum** | **A (100%)** | Every reading is a rate of change — the axis is *defined* by history |
| **Structure** | **C** | It is a sequence of dated events; events need timestamps and a bounded journal |
| **Progression** | **D** (+ A for velocity) | Current-leg record plus a short scalar window |

---

## Worked examples

**Why 300 bars for a percentile.** BBWP computes the fraction of prior bandwidths below the
current one. On a 100-bar window a single extreme bar moves the rank by roughly 1%. On 20 bars
one bar moves it 5% — the reading is dominated by its own current value and stops being
informative. 300 is the floor set by the longest indicator window, and it is comfortably above
the range where one bar dominates.

**Why Tier D is not Tier A.** The displacement from the origin is `C − origin`. It needs
`origin`, not a series. Keeping 300 bars of origin estimates would be 300 answers to a question
with one answer. Velocity acceleration is the exception — it needs a short window of prior
velocity values, which is Tier A.

**Why a journal needs two bounds.** A trend leg lasting 400 bars might emit 3 events. A
chop-bound leg lasting 40 bars might emit 60. A count-only limit would keep the first and
discard the second, which is exactly backwards — the long leg is the one whose history matters.
The age bound keeps both.

**Why a storage bound is not a validity rule.** A journal may hold 200 events, all of them
expired, and still satisfy its count **and** age bounds perfectly. Retention and validity are
independent axes — see [`event-lifecycle.md`](01-15-event-lifecycle.md), and **T3** for why eviction
must never be allowed to alter an event's state.
