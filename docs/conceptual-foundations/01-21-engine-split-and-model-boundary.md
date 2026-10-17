# Engine Split and the Model Boundary

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

Where does the system stop being a **model** and start being a **strategy**? This document
answers that question once, and names the layer and the crate where the line falls, so no
later phase has to re-derive it.

---

## 1. The line in one sentence

> The system stops being a model at the first output that only makes sense **if you are going
> to do something**. Before that it *describes*; after that it *advises*.

```
candles ──► MODEL ──► read-only state ──► STRATEGY ──► action ──► results
             ▲
             └─ the model never reads this way
```

---

## 2. The four-question test

Apply to any proposed output. **Any "yes" places it in the strategy.**

| # | Question |
|---|---|
| 1 | Is it still meaningful to someone with **no position and no mandate**? |
| 2 | Does it exist only to **decide, size, or bound** an action? |
| 3 | Does it consume anything the market did not produce — position, capital, an entry price, a mandate? |
| 4 | Is it a **counterfactual** — *"if you entered here, risk/reward would be 2.1"*? |

Four "no" ⇒ **model** (state). Any "yes" ⇒ **strategy** (decision).

---

## 3. Where the line falls, per layer

| Layer | Verdict | Why |
|---|---|---|
| L1 — the four axes per duration | **Model** | A camera works with no position. "Bearish", never "sell". |
| L2 — alignment per axis | **Model** | Still a measurement *over* state. Agreement across durations is a fact, not an act. |
| L3 — the four axes collapsed into one structure | **Model** | "Expanding, bearish, structure contested, late-mid" is a description. |
| Level geometry — POC/VAH/VAL, swing extremes, ATR envelope, Fib extensions, **state bands** | **Model** | Pure candle measurements. |
| L4 — the **entry / invalidation / target triple** + risk/reward | **STRATEGY** ← the line | Question 4: it is a counterfactual about *entering*. |
| L5 — risk dimensions and the composite | **Strategy** | It bounds action. |
| L6 — `Long \| Short \| Hold` + `strength_pct` | **Strategy** | The decision. |

**The subtlety that makes the line precise.** A price level *by itself* is a model output.
What makes it strategy is **labelling three of them "entry, invalidation, target" and dividing
them into a risk/reward**. The geometry is observation; the triple is intent.

---

## 4. Engine ownership

| Engine | Sole job | Emits | Observe mode |
|---|---|---|---|
| **DIE** — `network-adapters` | ingest, reconstruct, quality-gate, clock-monitor | `CandleSeries` — OHLCV per (pair, duration) | **active** |
| **MME** — `market-analyzer` | observe | **`MarketState`** — the model's only type | **active** |
| **TAE** — `portfolio-supervisor` | decide | `StrategyView` — setups, entry/invalidation/target, risk, verdict | **not run, not surfaced** |
| **PME** — `portfolio-supervisor` | portfolio configuration (max drawdown, max % of account, exposure caps) | `PortfolioView` | not run |
| **PAE** — `performance-analytics` | analytics | — | out of scope |
| **BTE** — `backtesting-engine` | historical replay | — | out of scope |

`candles → model → strategy → action → results`. Observe mode stops after `model`.

**Crate graph.** `portfolio-supervisor` already depends on `market-analyzer`, and
`market-analyzer` does not depend on `portfolio-supervisor`. Moving strategy *computation*
into TAE therefore requires no crate inversion: types stay in `core-domain` (a true leaf),
logic moves down the existing arrow. See
[01-06](01-06-crate-layout-and-cycles.md).

---

## 5. How the boundary is enforced

A line drawn in prose erodes. Four mechanical checks hold it.

| Gate | What it proves | Failure mode it prevents |
|---|---|---|
| **Field-set test** (`market_boundary.rs`) | The serialised key set of `MarketState` equals the declared list | Adding an entry/stop/target/score field to the model |
| **Call-site test** | `compute_opportunity`, `compute_risk`, `compute_advisory` have **zero** call sites in `market-analyzer` | Strategy quietly re-entering MME |
| **Surface greps** (`V-17`, `V-18`, `V-75`) | No UI component, no CLI command, no export builder names an action field | A strategy claim reaching a rendered pixel or a printed line |
| **Wire parity** (`V-54`, `V-55`) | The WS payload is exactly `market_state` + `ladder` | A legacy block leaking back onto the wire |

The compile-time half is already structural: `market-analyzer` cannot see
`portfolio-supervisor`. The tests close the other direction.

---

## 6. Naming

| Concept | Owner | Rule |
|---|---|---|
| **Confidence** | reserved | A calibrated meta-measure the layer does not produce. Never used as a field name inside the model. |
| **Readiness** | model | *"Are our inputs complete?"* — a mechanical fact about our own state machine. This is what replaces per-indicator `confidence`. |
| **State Consensus / State Stability** | model | The two named derived readings. Deliberately **not** called confidence. |
| **`strength_pct`** | strategy | *"How strong is this recommendation?"* — rendered on the decision layer, never inside the model. |

---

## 7. What each surface may and may not show

Identical on both surfaces. The CLI cannot leak a claim the UI hides, nor the reverse
(`V-75`, `V-62`).

| Allowed | Forbidden |
|---|---|
| Direction · Momentum · Structure · Progression, with position, size, readiness | entry, stop, target, risk/reward |
| the leg record, its stage, `progress`, `target_version`, `structure_level` | any setup name, setup quality, viability, trade plan |
| active / retained / emitted event counts with canonical identities | verdict, "trade / wait / stand aside" |
| level geometry: bands, POC/VAH/VAL, swings, Fib, ATR envelope | a probability of any outcome |
| the four-state diagnostic, consensus, stability | confidence, certainty, likelihood |

---

## 8. Consequences recorded here so they are not re-litigated

1. **`AnalysisMatrix` is dissolved.** Its content is redistributed — per-duration character
   becomes the axes, `market_regime` + `market_phase` become the four-state diagnostic, the
   overall bias becomes the Direction ladder aggregate, and the five `*_score` fields (which
   were duplicates of `AlignmentMatrix` dimension scores, as their own doc comment states) are
   deleted. `market_quality`, `state_confidence` and the free-prose `interpretation` /
   `rationale` are deleted: a score-of-scores and an unfalsifiable sentence are both
   unverifiable by any gate. See **CR-14**.
2. **`AlignmentMatrix` survives** as the only cross-duration view, with **one row per axis**
   replacing the ten dimensions, so the ladder speaks the same vocabulary as a single duration.
3. **The Analysis tab is deleted** from the frontend, and `AnalysisMatrix` appears in no CLI
   document. The States tab is a *new* tab, not a rename.
4. **The legacy terminal monitor is dropped.** Two CLI presentations of one data set is a
   parity violation; parity must hold by construction, not by discipline.
5. **The recommendation layer, including `strength_pct`, is deferred** to a future TAE phase.
   It is strategy, and strategy is not selectable in this release.

---

## 9. Related documents

- [08-10 Candles-Only Charter](../operations-and-compliance/08-10-candles-only-charter.md)
- [01-13 State Axes Framework](01-13-state-axes-framework.md) §1.1 — model vs strategy
- [02-16 MarketState Matrix](../matrices/02-16-market-state-matrix.md) — the model's single output
- [06-05 CLI Shell Contract](../integration-and-api/06-05-cli-shell-contract.md) — parity rules
- [01-17 Change Requests](01-17-change-requests.md) — CR-14, CR-15