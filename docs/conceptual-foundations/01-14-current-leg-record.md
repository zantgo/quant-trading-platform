# The Current-Leg Record

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Market Monitoring Engine (MME)

The entire Progression axis is computed from a **small fixed set of records** — one per
instrument per duration per structural level. This document specifies those records: their
fields, version discipline, the arithmetic that reads them, and their lifecycle.

It is the first thing to build, because it needs no new indicator — only the pivots already
computed for Structure and the candles already retained.

---

## 1. Why one record, versioned

A leg is the *current* movement: an origin, a direction, and everything that has happened to it
since. It is replaced wholesale when a new leg begins.

Four properties make it interpretable, and all four are easy to lose:

| Property | Requirement |
|---|---|
| **Versioned** | The record carries a `schema_version`. Field meanings change only across a version bump. |
| **Scaled** | A leg belongs to a named structural scale. "Current leg" is ambiguous without one (§2.5). |
| **Causal** | Every field states *when it became knowable*, not only when the thing happened. |
| **Revision-tracked** | When the reference target moves, the change is recorded as a revision — never silently overwritten. |

---

## 2. Field contract

Thirteen scalars plus a small revision log. This is Tier D.

### 2.1 Identity

| Field | Type | Meaning |
|---|---|---|
| `leg_id` | id | Stable identity. **Survives target revisions.** A retarget is not a new leg. |
| `instrument` | symbol | What this leg belongs to. |
| `timeframe_secs` | u64 | The duration. A leg never spans durations. |
| `structure_level` | enum | Which structural scale this leg belongs to (§2.5). |
| `swing_policy_version` | u32 | The swing-selection **policy** in force when this leg was admitted. System-wide, not per-leg; changes only on a policy change. |
| `structure_version` | u32 | **Per-leg monotonic counter of structural revisions.** Increments when the origin is revised or the level is reclassified. |
| `schema_version` | u32 | Record layout version. |

> **`leg_id` identifies a record. It does not identify which leg a reading is about.** Identity
> is `(instrument, timeframe_secs, structure_level)`. A consumer that omits `structure_level`
> cannot say which movement it is being told about.

#### What `structure_version` versions — and what it does not

The earlier name `leg_definition_version` was ambiguous: it could be read as versioning the whole
record, which would imply stage changes bump it. They do not. The counter is split so the two
things cannot be conflated.

| Field | Versioned by | Counter |
|---|---|---|
| `origin_price`, `origin_*_at`, `structure_level`, `extreme_price` — the **structural definition** | Structural revision | **`structure_version`** |
| `target_price` and its basis | Retargeting | **`target_version`** |
| The **swing-selection policy** itself | A policy change, system-wide | **`swing_policy_version`** |
| `stage`, `progress`, `target_stable`, `lifecycle` | **Nothing — they are derived** | **no counter** |

> **`stage` carries no version counter, deliberately.** It is a pure function of `progress`,
> `target_stable`, `lifecycle` and classification state. A counter on it would permit the state
> "stage changed and nothing else did" — which, under a correct implementation, cannot happen.
> Versioning a derived value invites it to be treated as an independent input.

The normative rules for all three counters are **L23, L25, L26 and L27 in §7** — stated once, so
there is no second copy to drift.

### 2.2 Origin

| Field | Type | Meaning |
|---|---|---|
| `origin_price` | f64 | Where the movement began. |
| `origin_occurred_at` | ts | **When the pivot price printed.** Historical. |
| `origin_available_at` | ts | **When the system could first know it was a pivot.** Usually later. |
| `origin_confirmed` | bool | Whether the confirmation bars have closed. |
| `direction` | enum | `long` / `short` — which way this leg runs. |

> **`origin_occurred_at` and `origin_available_at` are different facts and must both be
> stored.** A swing high on bar *n* is only knowable at bar *n + k*, once *k* bars have failed
> to exceed it. Collapsing them produces backtest results that no live run can reproduce.

### 2.3 Reference target

| Field | Type | Meaning |
|---|---|---|
| `target_price` | f64 | The current stated estimate of extent. |
| `target_version` | u32 | Increments **every time** `target_price` changes. |
| `target_basis` | enum | Which criterion produced it (see §5). |
| `target_revision_count` | u32 | How many times it has moved. |
| `target_stable` | bool | Whether it is currently still moving. |

The revision log retains, per revision: version, new price, old price, bar index, basis, and the
reading that triggered it.

### 2.4 Geometry and status

| Field | Type | Meaning |
|---|---|---|
| `invalidation_price` | f64 | The level that declares this leg dead. |
| `extreme_price` | f64 | Furthest point reached in the leg's direction. |
| `progress` | f64 | The arithmetic in §4. **Not** a percentage and **not clamped**. |
| `lifecycle` | enum | `forming` / `active` / `completing` / `completed` / `invalidated` / `superseded`. |
| `close_reason` | enum | Which criterion fired, when one did. |

---

## 2.5 Structural scale — which leg is "current"

The same instrument on the same duration contains **nested** legs: a minor upswing inside a
larger downswing both have valid origins, valid targets and valid progress values. Nothing in
the arithmetic distinguishes them.

Without a scale convention, `current leg` resolves to a different movement depending on which
structural interpretation a consumer happens to hold. A progression reading would then be
unfalsifiable — it could not be wrong, because it was describing a different object than the
one being checked.

### The convention

**Legs are nested, and the scale is explicit.** Identity is
`(instrument, timeframe_secs, structure_level)`, and there is **exactly one current leg per
scale**. Nesting is supported; ambiguity is not.

| `structure_level` | Meaning |
|---|---|
| `major` | The dominant leg on the duration. The one Progression reports by default. |
| `minor` | A counter-movement inside the major leg. Reported separately, never mixed. |

Progression therefore renders **one track per level**, and the visual grammar marks which is
which. A `minor` stage of `late` beside a `major` stage of `early` is the ordinary condition of
a pullback in an intact trend, and is exactly the information the nesting exists to carry.

### Classification is self-referential

A pivot's level is assigned by the **rank of its own displacement** against the instrument's own
Tier A history — not by an absolute price distance, and not by a bar count.

| Level | Test |
|---|---|
| `major` | The origin-to-extreme displacement ranks at or above the level's self-percentile. |
| `minor` | Otherwise. |

This satisfies **P2** (no absolute, no per-duration constant) and **P3** (the same rule at every
duration). A threshold expressed in points would classify a quiet instrument as all-`minor` and a
volatile one as all-`major`; a self-percentile cannot, because each ranks itself against its own
history.

**Classification is revision-prone in the same way origins are.** A leg's displacement grows as
it extends, so a `minor` leg can be reclassified `major`. That is a `structure_version`
bump, recorded like any other revision (`structure_version` bump, per S-N5), and it is subject to the same causal rules — the
reclassification is not readable before the bar that produced it.

### The rank distribution is itself causal data

A percentile is only as honest as the sample behind it. The classification rule inherits every
temporal rule that governs a leg, and adds these of its own.

| Rule | |
|---|---|
| **S-N1** | The reference distribution contains **only observations eligible at classification time**. A bar after `now` never enters it. |
| **S-N2** | Every distribution member carries its own `available_at`; none may predate the bar at which it becomes usable. |
| **S-N3** | A change in the distribution may revise a past classification **only from the bar at which that change becomes available** — never retroactively. |
| **S-N4** | The rank threshold is computed once at leg creation and **frozen for the life of the leg**, exactly as the ATR noise floor is frozen. |

**S-N3 is the one that matters.** A distribution is a rolling window, so a future bar can change
today's rank. That is legitimate — but the change must arrive with a timestamp. A classification
that silently differs between a short run and a long run is a look-ahead violation wearing a
different hat.

### Warmup behaviour

During warmup the reference distribution is incomplete, and an incomplete sample produces a rank
that is not yet comparable to a complete one.

| Regime | Classification |
|---|---|
| Distribution **not yet full** | `structure_level` is **`provisional`** |
| Distribution full, classification issued | `structure_level` is **`settled`** |

A `provisional` classification publishes **no stage**, on the same rule as a `forming` leg (L6) —
an assessment built on a partial sample is not a reading. This is stated because the alternative
is silently publishing a rank that a later bar will contradict.

### Reclassification is not retargeting

These are **different revision sources** and are versioned **separately**. Conflating them would
make one of them unobservable.

| | Scale reclassification | Leg retargeting |
|---|---|---|
| What moved | The `structure_level` of the origin | The `target_price` |
| Trigger | Displacement rank crossing the frozen threshold | The target basis recomputing |
| Version field | **`structure_version`** | **`target_version`** |
| Effect on `progress` | **None** | Yes — one of the two documented causes |

| Rule | |
|---|---|
| **S-N5** | A reclassification increments `structure_version` and **never** `target_version`. |
| **S-N6** | A retarget increments `target_version` and **never** `structure_version`. |
| **S-N7** | A reclassification is itself a revision and obeys the same availability rules as any other. |

Keeping the counters disjoint is what makes CG3 (retargeting is not movement) and the scale tests
separately falsifiable. If one bumped both, a reclassification would masquerade as movement —
the exact confusion §4.3 exists to prevent.

### Cost and its bound

Two levels means **two Tier D records per instrument per duration**. This is stated so it is
never discovered later: Tier D's O(1) guarantee is per *(instrument, duration, level)*, and the
level set is **fixed and small**. Adding a third level is a schema change with a stated cost,
not an open-ended hierarchy — an unbounded level set would make Tier D unbounded and break **I10**.

> **Not configurable at runtime.** The level set is part of the contract, precisely so that
> "current leg" cannot become a per-consumer choice.

---

> **A field may only be read at a bar at or after its `available_at`.** Not `occurred_at`.

Pivot-derived readings are the classic failure mode of this kind of system: they can be
*displayed* retrospectively, which makes a historical replay look correct while any consumer
acting on the reading at the time would have had no way to know it.

Three rules, and they are absolute:

| # | Rule |
|---|---|
| **A1** | A backtest or replay that feeds bar *n* to the aggregator may read only fields with `available_at <= n`. |
| **A2** | A field that has no `available_at` is not readable. Defaults are not silent substitutes. |
| **A3** | The lag between `occurred_at` and `available_at` is reported per duration. It is not a constant — a fractal pivot on a 1-second candle lags by one bar, and on a 1-day candle by as many as five. |

**What this costs.** Replays will show fewer structure events than a naive look-ahead run, and
progression stages will be revised more often. That is the correct result, not a defect. A
backtest that assumes it knew a pivot before it could be known is measuring itself, not the
market.

---

## 4. Progress arithmetic

Given origin price `O`, current price `P`, and the **current** reference target `T`:

```
progress = (P - O) / (T - O)
```

The same expression serves both directions. For a short leg `T < O` and the denominator is
negative, so a falling `P` yields a positive progress — the sign works out without a branch.

| Value | Meaning |
|---|---|
| `0` | At the origin. |
| `1` | At the reference target. |
| `< 0` | Price has moved **against** the leg's direction. |
| `> 1` | The target has been **exceeded**; the leg is running past its stated extent. |

### 4.0 The denominator is not guaranteed

`T - O` can be zero, and can be *negligibly* small. Both must produce an **explicitly undefined**
reading, never a number.

| Condition | Result |
|---|---|
| `T == O` exactly | `progress = UNDEFINED` |
| `abs(T - O)` below the leg's own noise floor | `progress = UNDEFINED` |

The noise floor is the instrument's own realised range over the leg's own duration — an ATR-scaled
measure, so it satisfies **P2** and **P3**. It is not a fixed price distance.

> **The denominator is never guarded by substituting a default.** Returning `0`, returning
> `progress = 0`, or falling back to a previous target all produce a number that *looks* like a
> measurement and is not one. A leg whose reference is degenerate reports `UNDEFINED` and
> publishes no stage. A missing reading is honest; a fabricated one is not.

`UNDEFINED` is a distinct value in the type, not a sentinel in a float. Where progress is
rendered, `UNDEFINED` renders as an absent value with a reason — not as `0`, and not as `NaN`
propagating silently through an aggregate.

### 4.1 Never clamp

**`progress` is not clamped to `[0, 1]`.** Clamping destroys the two readings that matter most:
that price has gone backwards, and that the target has been overrun. A clamped value cannot
distinguish "at the target" from "well past it".

### 4.2 What it does and does not measure

> **Progress is the fraction of the distance from origin to the *current* reference target that
> price has covered. It is not the percentage of the final move that has elapsed.**

The final move is unknowable, and the target is an estimate that moves. So a progress reading
is only ever meaningful alongside `target_version` and `target_stable`.

### 4.3 Revision is not movement

When `target_version` increments, `progress` changes for two independent reasons: the target
moved, and price moved. These must be separable:

| Rule | |
|---|---|
| **P1** | A change in `progress` accompanied by a change in `target_version` is **not** evidence that price travelled. |
| **P2** | Movement is `P(n) - P(n-1)`. It is read from price, never inferred from `progress`. |
| **P3** | `target_stable = false` suppresses stage promotion. A stage may not advance while its own reference is moving. |

A naive consumer that watches `progress` alone will read target expansion as a fresh advance
into new territory. That is the specific error this section exists to prevent.

### 4.4 Stage is the output; progress is the input

`progress` is **continuous** and feeds the stage classification. The stage itself is
**ordinal** and is what the Progression axis reports (principle **P5**).

| Stage | Typical band | Meaning |
|---|---|---|
| `early` | `progress` well below 1 | Barely developed. |
| `early_mid` | | |
| `mid` | around 1 | At the stated extent. |
| `late` | `progress > 1` | Target exceeded, no structural evidence of turn. |
| `terminal` | `progress > 1` **and** a completing criterion | Completing. |

The bands are defined by the instrument's own history and its own reference — never by an
absolute price distance. Two legs of different instruments may have "late" at completely
different progress values.

### 4.5 Target stability — exact transitions

`target_stable` is a **stated claim about the reference**, not a display preference. Its
transitions are exact.

| Transition | Condition |
|---|---|
| **→ false** | **Immediately** on any `target_price` change. Every revision revokes stability, without exception. |
| **false → true** | The target has gone **unrevised for `STABILITY_WINDOW` consecutive bars**. |
| **true → false** | Any revision. Revocation is symmetric and unconditional. |
| **→ permanently false** | `target_revision_count` reaches `REVISION_BUDGET` (§4.6). The basis is declared unviable for this leg. |

**May stability be revoked?** Yes, always, at any time, at a single bar's notice. There is no
grace period and no hysteresis. A reference that has just moved cannot be called stable, whatever
its history.

**Is a minimum observation count required?** Yes — `STABILITY_WINDOW`. Without it, a target set
on one bar and untouched on the next would report `stable` immediately, which asserts a
confidence no one-bar observation supports.

**On the window's units.** `STABILITY_WINDOW` is an **observation count, not a price or time
threshold**. This is stated explicitly because it reads like a violation of **P2** and is not:
P2 forbids absolute numeric *reading* thresholds tuned per duration, and a bar count is the same
count on every duration by construction. It is a bookkeeping window over revisions, not a level.

### 4.6 Repeated revision — and the refusal to force stability

A target revised every few bars is not an estimate that is settling. It is an estimate with no
convergence, and calling it stable would manufacture certainty the system does not have.

**When `target_revision_count >= REVISION_BUDGET` for a leg:**

1. `target_stable` is `false` and **can no longer become true for this leg**.
2. `target_basis` is set to `unviable`.
3. **No stage is published**, and `progress` is published but explicitly marked provisional.
4. The leg continues to be tracked. It is the *reference* that failed, not the movement.

> **Stability is never forced to make progression available.** There is no fallback target, no
> widened window, and no relaxation of the budget. An explicitly provisional estimate is a
> truthful reading; a falsely definitive one is a defect that will not surface until it is
> believed.

This is why the `mid` stage's band cannot be tightened to compensate. When the reference is
unviable the answer is *we do not know how far along this is* — and that is a legitimate,
reportable state.

---

## 5. Target basis and completion criteria

These are distinct enumerations and are not interchangeable.

| `target_basis` | How the target is set |
|---|---|
| `extension` | A Fibonacci extension of the origin leg (1.272 / 1.618 / 2.0 / 2.618). |
| `measured` | The prior leg's extent projected from the origin. |
| `volatility` | A multiple of ATR from the origin. |
| `swing` | The origin of the next opposing swing, once it exists. |

| `close_reason` | What ended the leg |
|---|---|
| `opposing_swing_confirmed` | The opposing swing confirmed. |
| `extension_reached` | Price reached an extension reference. |
| `character_change` | A change-of-character event fired against the leg. |
| `volume_climax_failure` | An extreme-volume, extreme-range bar failed to extend. |
| `invalidation_touched` | Price reached `invalidation_price`. |
| `superseded` | A new origin replaced this leg. |

`basis` is an input; `close_reason` is a terminal fact. A leg may reach its extension target and
still be *active* — the target is an estimate, not an exit.

---

## 6. Lifecycle

```
        origin confirmed
              |
              v
          [forming] ---- invalidation touched ----> [invalidated]
              |                                        ^
              | leg confirmed as current                 |
              v                                          |
          [active] ----- completion criterion --------> [completing]
              |                                          |
              |                                          v
              +-----------------------------------> [completed]
                          new origin confirmed
              (either transition ends the leg; reason recorded)
```

| State | Meaning | Progression reports? |
|---|---|---|
| `forming` | Origin exists but is not yet confirmed. | Stage withheld. |
| `active` | The current leg. | Yes. |
| `completing` | A criterion has fired; the leg is resolving. | Yes, with the reason. |
| `completed` | Finished. | Final stage retained. |
| `invalidated` | Price reached the invalidation level. | Final stage retained. |
| `superseded` | A newer origin took over. | Final stage retained. |

**A `forming` leg reports no stage.** Reporting a stage from an unconfirmed origin is exactly
the revision problem P5 exists to prevent, and it would make every early leg look further along
than it is.

---

## 7. Invariants

| # | Rule |
|---|---|
| L1 | `leg_id` is stable across target revisions. A retarget is not a new leg. |
| L2 | `origin_available_at >= origin_occurred_at`, always. |
| L3 | No field is readable before its `available_at`. |
| L4 | `progress` is never clamped, truncated, or rendered as a percentage. |
| L5 | Every progress reading is published with `target_version`, `target_stable` and `structure_level`. |
| L6 | A `forming` leg publishes no stage. |
| L7 | Every terminal leg records a `close_reason`. |
| L8 | One leg per `(instrument, timeframe_secs, structure_level)`. A new leg replaces the record; the old one is discarded, not archived (Tier D, O(1)). |
| L9 | `invalidation_price` is set before the leg reaches `active`. |
| L10 | Stage promotion is blocked while `target_stable = false`. |
| L11 | Every progression reading carries its `structure_level`. A reading without one is not publishable. |
| L12 | The `structure_level` set is fixed and small. It is not runtime-configurable. |
| L13 | `T == O`, or `abs(T - O)` below the leg's own noise floor, yields `progress = UNDEFINED` — never a number. |
| L14 | `UNDEFINED` is a distinct typed value. It never renders as `0` and never propagates silently as `NaN` through an aggregate. |
| L15 | Any target revision sets `target_stable = false` immediately, including a revision made while it was `true`. |
| L16 | `target_stable` becomes `true` only after `STABILITY_WINDOW` consecutive unrevised bars. |
| L17 | Reaching `REVISION_BUDGET` sets `target_basis = unviable` and permanently blocks both stage publication and stability for that leg. |
| L18 | Stability is never forced, widened, or bypassed to make a progression reading available. |
| L19 | The scale-classification rank distribution contains only observations eligible at classification time. |
| L20 | A distribution change may revise a past classification only from the bar at which it becomes available. Never retroactively. |
| L21 | The rank threshold and the noise floor are each frozen at leg creation. Neither evolves. |
| L22 | A `provisional` classification publishes no stage. |
| L23 | `structure_version` and `target_version` are **disjoint** — a revision of one never increments the other. |
| L24 | A Tier D record is never evicted by the byte budget, at any memory pressure. [`retention.md`](01-16-retention-tiers.md) **T2**. |
| L25 | `structure_version` increments **only** on an origin revision or a level reclassification — never on anything derived. |
| L26 | `stage`, `progress`, `target_stable` and `lifecycle` are **derived** and carry no version counter. |
| L27 | `swing_policy_version` is shared by every leg under the same policy. Legs with different values are not comparable. |

---

## 8. Worked examples

### 8.1 A converging reference

A long leg on a 5-minute candle, `structure_level = major`.

```
bar 40   pivot low prints at 100.00          origin_occurred_at = 40
bar 44   three bars fail to breach it         origin_available_at = 44
         → leg confirmed, leg_id = L-0091

bar 44   origin 100.00, target 108.00         (measured basis, prior leg 8.00)
         progress = (100 - 100) / (108 - 100) = 0.00   stage: forming → withheld
bar 52   close 105.00                         progress = 0.63   stage: early_mid
bar 57   target revised to 110.00             target_version 1 → 2
         progress = 0.545                     ← DOWN, and price did not move
         target_stable = false                → no stage promotion   [L10]
bar 63   close 111.20, unrevised 6 bars       6 ≥ STABILITY_WINDOW
         progress = 1.067                     target_stable = true
         stage: late
bar 66   opposing swing confirmed             completing → completed
         close_reason: opposing_swing_confirmed
```

At bar 57, progress **fell** while price rose — the target moved faster than price, and per **P1**
that is explicitly not movement. The stage did not promote, per **L10**.

### 8.2 A reference that never converges

```
bar 70   leg opens, target 108.00            revision_count 0
bar 74   target 110.50                        count 1   stable=false
bar 79   target 107.20                        count 2   stable=false
bar 84   target 112.00                        count 3   stable=false
bar 88   target 109.40                        count 4 = REVISION_BUDGET
                                            → target_basis = unviable      [L17]
                                            → stable is false and CANNOT become true
                                            → no stage published for this leg, ever
                                            → progress continues, marked provisional
```

Four revisions in eighteen bars. The price may be trending perfectly cleanly; the **reference**
has failed. The honest reading is "how far along this is cannot be said" — and the system says
it, rather than widening the window until it can.

### 8.3 A degenerate reference

```
bar 95   target collapses to 100.02, origin 100.00
         abs(T - O) = 0.02, below the leg's own noise floor
         → progress = UNDEFINED                [L13]
         → no stage, no fallback target, no 0
```

Every tempting repair here is wrong. Defaulting the denominator, substituting `0`, or falling
back to the previous target version each produce a number that looks like a measurement and is
not one.