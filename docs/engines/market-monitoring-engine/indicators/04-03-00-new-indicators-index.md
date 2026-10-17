//! The `04-03` series — specifications for readings that do not exist yet.
//!
//! **Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
//! **Status:** Approved
//! **Owner:** Market Monitoring Engine (MME)
//!
//! 78 specifications, one per new indicator. They are filed **by the axis they will serve**,
//! not by what they measure, because that is the question a reader has when looking for one:
//! *"what feeds Momentum?"*
//!
//! **Nothing here is built.** These are contracts for phase **P12**, which cannot open until the
//! 13 causal gates are green ([01-20 §6](../../../conceptual-foundations/01-20-causal-gate-plan.md)).
//! An indicator whose specification exists has earned nothing; it is admitted only when it passes
//! the gate and the measurement-quality review.

---

## Filing rule

| | |
|---|---|
| `04-02-NN` | the **44 shipped** candle-only indicators, filed by what they measure |
| `04-03-NN` | the **78 new** indicators, filed by the axis they will serve |
| `04-04-00` | the reading inventory across both trees |

The two trees are filed differently on purpose. *Present* is organised the way those indicators
are implemented; *new* is organised the way they will be consumed. A present indicator's folder is
therefore **not** its axis, and an indicator's axis coverage is always stated in its own header.

**CR-17** records why the 44 present specifications were not duplicated into this corpus: their
axis coverage, retention tier, role and family are exactly what **CR-4** requires the registry to
carry and **CR-5** requires the generated index to render. A second prose copy would be a second
place to drift.

---

## Direction — 19 specifications

Files `04-03-01` … `04-03-19`. Each states a signed level in `[-1, +1]` and, where relevant, a
persistence measure. Direction is the only axis that aggregates across durations
(mean bias + agreement percentage).

## Momentum — 18 specifications

Files `04-03-20` … `04-03-37`. Every reading here is a **rate of change**; the axis is defined by
history, so a Momentum reading with fewer than three level samples reports readiness rather than a
value.

## Structure — 19 specifications

Files `04-03-38` … `04-03-56`. Discrete, dated, with a lifecycle
([01-15](../../../conceptual-foundations/01-15-event-lifecycle.md)). Structure's net bias is derived
from structural events — never from a normalised price slope, which would make it answer
Direction's question a second time.

## Progression — 22 specifications

Files `04-03-57` … `04-03-78`. **CR-16 applies before any of these is built:** the measurement-quality
audit already found that 7 of the 22 restate a leg-record field, 1 duplicates another, 1 is filed on
the wrong axis and 1 contradicts itself. The audit reduces this set to 11-13 readings, and the
reduced set is what gets implemented.

---

## Each specification's contract

Every file in this series carries the same seven sections, and the same header table:

| Section | Contents |
|---|---|
| Header table | status · **axis coverage** · retention tier · data source |
| **Purpose** | What the reading is for, in one paragraph |
| **Formula** | The published mathematics, in a code block |
| **Range** | Output range and what saturates |
| **Signals** | Signal kinds and when each fires — `Threshold`, never `ZeroLineCross`, on a bounded series (CR-9) |
| **Divergence** | Whether divergence is supported, and its rule |
| **Levels and other outputs** | Every value the reading publishes |
| **Notes** | Caveats, defaults, and what must accompany the reading |

Three rules apply to every one of them:

1. **Published formula, candle-only** ([08-10 §3](../../../operations-and-compliance/08-10-candles-only-charter.md)).
   No indicator is justified because a backtest of it was profitable — that settles a strategy
   question, not a reading's (P10).
2. **Readiness, not confidence.** A reading that cannot yet be computed says so
   mechanically; it never reports a number that means "nothing yet".
3. **Ordinal positions are not numbers.** A Progression reading publishes a stage anchor and a
   `target_version`, never an interpolated percentage.

---

## Cross-references

- [04-02-00 Indicator Index](04-02-00-indicator-index.md) — the 44 shipped indicators (**generated** from the registry)
- [04-04-00 Reading Inventory](04-04-00-reading-inventory.md) — all readings across both trees
- [01-13 State Axes Framework](../../../conceptual-foundations/01-13-state-axes-framework.md) — the axes and their invariants
- [01-19 Measurement Quality](../../../conceptual-foundations/01-19-measurement-quality.md) — Q1–Q4 and the applied audit
- [02-16 MarketState Matrix](../../../matrices/02-16-market-state-matrix.md) — where these readings are published