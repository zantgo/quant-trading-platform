# Event Lifecycle Policy

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Market Monitoring Engine (MME)

A bound is necessary but not sufficient. Truncating a journal to the last 200 events answers
*"how much do we keep?"* — it does not answer *"when does a stored event stop being true?"*

An event that remains in the journal after its meaning has lapsed is worse than no event: it is
counted by Structure, counted again by a Progression counter, and reported as current structure.
This document makes retention a **lifecycle** rather than a window.

---

## 1. The problem a window cannot solve

Consider a bearish divergence. It is emitted at bar 40 and remains in the journal because:

- it has not been **confirmed**,
- no contradicting event has arrived,
- and the journal has not yet reached 200 entries.

Bar 300 arrives. The price has made a substantially higher high and the market has been in a
clean uptrend for two hundred bars. The divergence is still in the journal, still "active", and
still counted by every consumer.

Nothing was truncated. The bound held perfectly. The event is simply **no longer true**, and no
window policy detects that on its own.

> **Absence of a contradicting event is not evidence that an event is still valid.** Retention
> must expire events on their own terms, not only when contradicted.

---

## 2. The five transitions

Every retained event carries a lifecycle. These five transitions are the whole policy.

| Transition | Trigger | Effect |
|---|---|---|
| **Activation** | The event is emitted. | Created with `activated_at`. It is **not yet citable**. |
| **Persistence** | The event's own confirming condition holds on subsequent bars. | Extends `last_validated_at`. |
| **Expiration** | A **time bound** elapses with no persistence. | Retained in the journal for history; **removed from every count**. |
| **Invalidation** | A named contradicting event occurs. | Moves to `invalidated`, with the contradicting event's identity recorded. |
| **Reactivation** | The confirming condition returns after expiry or invalidation. | A **new canonical event** is created. The old one is never revived. |

### 2.1 Why activation is not citation

An event emitted on bar *n* was *produced* on bar *n*, but it is rarely *conclusive* on bar *n*.
A divergence needs its confirmation; a break needs its close; a sweep needs the reclaim.

> **An activated-but-unconfirmed event is retained and visible, but contributes zero to every
> axis count.** It is pending, not evidence.

Without this separation, every unconfirmed break on the bar would inflate Structure's event
density, and Structure's output would measure how often signals fire rather than how often they
resolve.

### 2.2 Why reactivation creates a new identity

Reactivating a record would let one divergence be counted twice — once before it lapsed, once
after — and would make the count meaningless.

> **An event's canonical identity is `(source, kind, subject, anchor, timeframe)`. It is
> immutable. A reactivation is a new event with a new identity and a `supersedes` pointer to the
> one it replaced.**

This is the same identity rule as [`vocabulary.md`](../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md). It is what keeps one
divergence from becoming three pieces of evidence across Structure, Progression and a
consolidated reading.

---

## 3. Lifecycle states

```
  [emitted] ──> [pending] ──confirm──> [active] ──> [resolved]
      |             |                    |    |
      |             |                    |    └─ invalidate ─> [invalidated]
      |             |                    |
      |             └──timeout───────────┴──> [expired]
      |
      └──superseded_by_new_origin──> [superseded]

  [active] ──expire──> [expired]        (states 3-6 are all non-citable except [active])
```

| State | In journal | Counts toward axes | Visible |
|---|---|---|---|
| `pending` | Yes | **No** | Yes, marked pending |
| `active` | Yes | **Yes** | Yes |
| `expired` | Yes (history only) | **No** | Yes, marked expired |
| `invalidated` | Yes (history only) | **No** | Yes, with the contradicting event |
| `resolved` | Yes (history only) | **No** | Yes, as history |
| `superseded` | Yes (history only) | **No** | Yes |

**Only `active` counts.** Everything else is retained so that Structure can report history and
Progression can count events *relative to the current leg* — but none of it is current evidence.

---

## 4. Expiry bounds, by event kind

These are **age bounds**, not thresholds. They express how long a claim stays meaningful without
fresh support, and they are duration-relative — never tuned per duration (principle **P2**).

| Event kind | Confirms on | Expires after | Invalidated by |
|---|---|---|---|
| `Divergence` | A close breaks the nearest level by a defined margin | 2 × its own detection lookback | Price makes a new extreme in the divergence's direction |
| `Breakout` | Close outside the level | 1 × the level's defining window | Close back inside the broken level |
| `TrendFlip` | The flip condition holds one further bar | Immediately on the next opposite flip | — (the next flip supersedes it) |
| `LevelTest` | Touch registered | Same bar | — |
| `PatternForming` | Geometry completes | The pattern's own maximum bar count | Geometry invalidated before completion |
| `VolumeClimax` | The bar prints | Next bar — this is a **single-bar** event | — |
| `CompressionRelease` | Bands separate | The expansion's defining window | Bands re-contain |
| `Threshold` | The level is crossed | The crossing bar — this is an **instantaneous** event | — |
| `Crossover` | The crossing is printed | The crossing bar | — |
| `CenterlineCross` | The crossing is printed | The crossing bar | — |
| `ZeroLineCross` | The crossing is printed | The crossing bar | — |

The bottom four are **instantaneous**: they describe one bar and are expired by construction.
Reporting them as standing conditions is a category error — a crossover happened, it is not
happening.

---

## 5. Retention tier interaction

| Tier | Holds | Lifecycle applied? |
|---|---|---|
| **A** — 300 bars | Numeric series | No. A series has no validity state; its bound *is* its retention. |
| **B** — 500 bars | Numeric series for cross-duration comparison | No. |
| **C** — 200 events **and** ≤ Tier B age | Event journal | **Yes — the whole policy applies here.** |
| **D** — current leg record | One leg | **Yes — leg lifecycle, see [`leg.md`](01-14-current-leg-record.md) §6.** |

> **The lifecycle policy does not extend the Tier C bound.** A journal can be within its count
> and age bounds and still contain only expired events, because expiry is a *semantic* decay and
> the bounds are a *storage* bound. Both must be reported separately.

---

## 6. What consumers must do

Every consumer of an event journal must distinguish three quantities, and must never conflate
them:

| Quantity | Definition |
|---|---|
| **Active count** | Events in state `active`. This is what an axis consumes. |
| **Retained count** | Events in any state within the bound. This is what the journal holds. |
| **Emit count** | Events ever produced this session. This is a rate, not a state. |

A consumer that reads retained count where it needed active count is reading history as
structure. A consumer that reads emit count is measuring signal frequency.

The three must be separately exported, and a journal that reports only a total is under-specified.

---

## 7. Invariants

| # | Rule |
|---|---|
| E1 | Only events in state `active` contribute to any axis count. |
| E2 | An activated but unconfirmed event contributes zero. |
| E3 | Absence of a contradicting event never extends an event's life. Every event has an independent age bound. |
| E4 | A reactivation creates a new canonical event with a `supersedes` pointer. A record is never revived. |
| E5 | Canonical event identity is immutable and counted exactly once across all axes. |
| E6 | Instantaneous events (`Crossover`, `ZeroLineCross`, `CenterlineCross`, `Threshold`) expire on the crossing bar. |
| E7 | Active count, retained count and emit count are separately exported. |
| E8 | The lifecycle policy never causes the Tier C bound to be exceeded. |
| E9 | Expiry bounds are duration-relative. No per-duration constants. |
| E10 | An invalidated event records the identity of the event that invalidated it. |

---

## 8. Worked example

A bearish divergence on a 1-hour candle.

```
bar 40   RSI divergence emitted, unconfirmed
         → state: pending        active count 0

bar 43   close breaks the nearest support by 0.4%
         → state: active         active count 1
         canonical id D-4471 recorded

bar 67   24 bars later. No contradicting event has arrived.
         expiry bound: 2 x detection lookback = 24 bars → reached
         → state: expired        active count 0
         journal still holds D-4471 for history

bar 90   price makes a new high; the divergence's own premise reappears
         → NEW event D-4472, supersedes D-4471
         → state: pending        active count 0

bar 93   confirmed → active     active count 1
```

At bar 67 nothing was truncated and no threshold was crossed. The event simply stopped being
true. A count-only retention policy would have carried it for another 200 events and reported a
bearish divergence on a market that had been trending up for half that span.