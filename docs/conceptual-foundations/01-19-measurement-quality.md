# Measurement-Quality Review

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

A per-reading review that decides whether a reading earns its place, with a stated verdict and a
stated basis. It is **additive to the causal gate, not a substitute for it.**

---

## 1. Why this exists, and what it is not

The causal replay gate provides evidence that the system **uses information correctly, on the cases tested**. It establishes nothing about
whether the information **helps**. A system can be perfectly causal and perfectly wrong.

| Gate answers | This review answers |
|---|---|
| Does it use only what it knew? | Does this reading carry information another does not? |
| Is it deterministic? | Does it change the axis output at all? |

**Neither substitutes for the other.** Passing the causal gate with a redundant inventory yields a
correctly-computed wrong answer.

### The one thing it must not become

> **This review does not balance axis counts.** Direction having 50 readings and Progression 25 is
> not a defect. Progression legitimately needs fewer readings *because it rests on an explicit leg
> record* rather than on an expansive pile of unrelated indicators. Equal-sized axes would be an
> arbitrary aesthetic constraint masquerading as semantic completeness.

---

## 2. The four questions

Every reading answers all four. A reading that cannot answer Q1 has no case.

| # | Question | Test |
|---|---|---|
| **Q1** | **Unique information** | What does this capture that no other reading captures? |
| **Q2** | **Ownership** | Direct measurement, derived feature, event, or diagnostic? |
| **Q3** | **Family redundancy** | Independent evidence, or another encoding of an existing signal? |
| **Q4** | **Incremental value** | Does removing it change the axis output, historically? |

### Q2 — the four ownership classes

| Class | Meaning | Counts as evidence? |
|---|---|---|
| **MEASUREMENT** | Directly observed from the candles | Yes |
| **DERIVED** | A function of other readings or leg fields | **Once**, via its parent |
| **EVENT** | A discrete occurrence with a timestamp | Yes, as an event |
| **DIAGNOSTIC** | Context or classification | **No** — §5.2 role rule |

> **A DERIVED reading is not an independent vote.** `efficiency-regime` is a bucketing of
> `path-efficiency`; it is the same information in a different container. Q3 catches this; Q4
> confirms it by ablation.

---

## 3. Verdicts

| Verdict | Meaning | Effect on the inventory |
|---|---|---|
| **KEEP** | Distinct, correctly owned, materially informative | Retained |
| **MERGE** | Same information, different name or normalisation | Folded into its family; one entry survives |
| **DEMOTE** | Real quantity, but diagnostic — must not vote | `role = diagnostic`; removed from aggregation |
| **FIELD** | Already a field of the leg record | **Removed as a reading.** It is a field, not an indicator |
| **MOVE** | Genuine quantity, wrong axis | Re-filed to its correct axis |
| **CUT** | No unique information | Removed |

> **FIELD is the verdict people skip.** A reading that restates a leg field is not a weak reading;
> it is **a category error** — an indicator-shaped wrapper around something the record already
> holds. It inflates the count, it adds an aggregation weight that carries no information, and it
> creates two sources of truth for the same quantity.

---

## 4. Q4 — incremental value, tested

Q4 is empirical and cannot be argued from a formula.

### 4.1 What Q4 measures, and what it must not

> **Q4 asks whether a reading changes the *descriptive output*.** It does not ask whether the
> system predicts anything — **nothing here predicts anything** (P1), and there is no fitted model
> to generalise.

The vocabulary matters here. Words like *out-of-sample*, *in-sample* and *overfitting* import
predictive-model assumptions into a layer that has none: **nothing is trained, nothing is fitted,
and there is no generalisation gap.** Using them would imply the layer is a model awaiting
validation, which is precisely what it is not.

| Requirement | Detail |
|---|---|
| **Chronological split** | Earlier history, then later history, **time-ordered**. Never a random split — candle data is serially dependent. |
| **Ablation** | Remove one reading, or one family, and re-measure the axis output. Repeat for every reading. |
| **Metric** | Change in the **axis output** and in the **distribution** of values that axis produces — nothing else. |
| **Whole-ladder test** | Repeat across the duration ladder. A reading that changes only one duration is not established. |
| **Temporal stability** | The effect must be present on the later period too, not only the earlier one. |

> **The object is the reading's effect on the description.** A family that changes nothing on
> either period carries no information and should be `MERGE` or `CUT`.

### 4.2 The backtesting boundary — P10

If a Q4 investigation is extended to ask *"does this reading lead to better trade outcomes?"*, it
has **left the layer**. That question belongs to the strategy sitting above it, and:

- the verdict is evidence about the **strategy**, never about the reading's correctness;
- it may **not** be used to admit a reading, set its weight, or tune its formula (**P10**, §8.2);
- **no backtest result may be cited in this document as justification for a reading.**

**Q4 is measured only after the causal gate passes.** A reading cannot be judged on what it
changes until it is known that the system is not cheating while computing it.

---

## 5. Review order

| Priority | Axis | Why |
|---|---|---|
| **1** | **Progression** | Smallest inventory, largest share of leg-derived candidates, and the axis most likely to restate the record |
| **2** | **Direction** | Highest density; many smoothing variants of one underlying quantity |
| **3** | **Momentum** | Role classification must be enforced, or diagnostics start voting |
| **4** | **Structure** | Least redundant by construction; focus on event *quality*, not event *count* |

---

## 6. Progression audit — the 22 new specifications

Performed by extracting every formula from the 22 specs and comparing right-hand sides
mechanically. **Results below are measured, not estimated.**

### 6.1 Confirmed duplicate

| | |
|---|---|
| **Duplicate pair** | `displacement-atr` and `distance-to-origin` |
| **Formula** | `(Close - origin) / ATR(n)` — **identical** |
| **Verdict** | **MERGE** — one survives |

This is the same defect class as the earlier `Directional Persistence Ratio` / `Path Efficiency`
collision: two names, one formula. Their *purposes* even agree — both described as "the extent
measure that satisfies the rank-based principle."

### 6.2 Leg-record fields, not readings

These restate a quantity [`leg.md`](01-14-current-leg-record.md) already holds. Under Q2 they are **FIELD**.

| Spec | Already a leg field or derived from it |
|---|---|
| `reference-target` | `target_price` |
| `reference-stability` | `target_stable` — and L26 already declares it derived, carrying no version counter |
| `reference-revision-count` | `target_revision_count` |
| `stage-classification` | `stage` — **the axis's declared output**, derived under L26 |
| `completion-criterion` | `close_reason` + `lifecycle` |
| `time-since-origin` | `now - origin_occurred_at` |
| `displacement-absolute` | `extreme_price - origin_price` |

**7 of 22.** Each is a legitimate quantity and a redundant *reading* — the information exists,
but it is not an independent observation of the market, and giving it an aggregation weight would
double-count the leg record.

### 6.3 Derived from another reading

| Spec | Parent | Verdict |
|---|---|---|
| `efficiency-regime` | `path-efficiency` — it is a bucketing of it | **MERGE** into the family |

### 6.4 Wrong axis

| Spec | Issue | Verdict |
|---|---|---|
| `velocity-acceleration` | `velocity - velocity[-1]`. This is an **acceleration**, which is Momentum's declared position variable (§5.2). It is filed on Progression. | **MOVE** → Momentum |

### 6.5 Internal contradiction

`leg-duration` states it is *"distinct from total movement age — it resets when a new leg begins
even if the origin is shared"*, and its formula is *"bars since the current leg began"*.

But **L8** establishes that a new leg **replaces** the record — there is one leg per
`(instrument, duration, structure_level)`. Origins are shared **across structural levels**, never
across legs at the same level. So at any fixed level, `leg-duration` and `bars-since-origin`
compute the same number, and the stated distinction does not exist.

| Verdict | |
|---|---|
| **MERGE** into `bars-since-origin`, distinguished by `structure_level` rather than by a second duration measure | |

### 6.6 Progression verdict summary

| Verdict | Count | Specs |
|---|---|---|
| **KEEP** | 11 | `path-efficiency`, `displacement-absolute`*, `displacement-atr`*, `displacement-percent`, `distance-to-reference`, `retracement-consumed`, `progression-velocity`, `slope-consistency`, `range-expansion`, `velocity-vs-prior`, `bars-since-origin` |
| **MERGE** | 4 | `displacement-atr`/`distance-to-origin` (1), `efficiency-regime`, `leg-duration` |
| **FIELD** (remove as reading) | 7 | the seven in §6.2 |
| **MOVE** | 1 | `velocity-acceleration` → Momentum |
| **CUT** | 0 | — |

\* `displacement-absolute` and `displacement-atr` are kept pending Q4: they are the same quantity
under different normalisations, and **which normalisation earns a slot is an empirical question**,
not a definitional one.

**Net effect: 22 → 11–13 readings.** A ~50 % reduction.

> **This is not a coverage loss.** The removed eleven were never additional observations of the
> market — they were the leg record wearing an indicator's clothing, plus one duplicate, one
> cross-axis misfiling and one self-contradicting entry. What remains is what Progression actually
> measures.

### 6.7 The generalisable finding

> **On an axis defined by a record rather than by a pile of indicators, the dominant failure mode
> is not a missing reading. It is a field promoted into a reading.**

Every axis should be audited for it. Structure is the next most likely to show it, because it
also rests on an explicit structure object (the swing and leg records), not only on price.

---

## 7. Adding this to the acceptance process

| Requirement | |
|---|---|
| **MR-1** | Every reading carries `verdict`, `owner_class`, `family`, and the Q1 answer as registry fields. |
| **MR-2** | A reading may not enter an inventory at `KEEP` status without a recorded Q1 answer. |
| **MR-3** | `FIELD` and `MOVE` verdicts are resolved **before** the reading is implemented, not after. |
| **MR-4** | Q4 (ablation, chronological OOS) runs **after** the causal gate passes and **before** any family is admitted to aggregation. |
| **MR-5** | No review verdict changes an axis count for balance. Verdict follows evidence, never symmetry. |

---

## 8. Current status

| Item | Status |
|---|---|
| Review process defined | ✅ this document |
| Progression audit | ✅ 22 specs, measured — §6 |
| Direction redundancy audit | ⬜ not performed |
| Momentum role enforcement audit | ⬜ not performed |
| Structure event-quality audit | ⬜ not performed |
| Q4 ablation harness | ⬜ lands after the causal gate |

**The inventory freeze holds.** Nothing is cut or added until each axis has been audited and the
causal gate has passed.