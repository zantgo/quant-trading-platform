# Charter Close and Axis Filing

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

This document closes the model contract. It answers, once and in normative prose, four questions
that every earlier draft left open or answered differently:

1. What are the model's output types, and what are they called?
2. How does a reading get assigned to an axis, and what stops two readings counting twice?
3. What may a cross-duration reduction legally do, and what may it never do?
4. What is the final deliverable, and what is deliberately *not* in it?

It implements [CR-18](01-17-change-requests.md#cr-18--charter-close-four-names-three-reductions-one-envelope)
and refines [01-13](01-13-state-axes-framework.md). Where the two differ on vocabulary, this
document wins.

---

## The model is three reductions deep

```
 candles              500 OHLCV per (pair, duration)
     ↓
  indicators           157 · each: a 500-point series + typed signals
     ↓                            one observable → one axis → one signal type
  AXES                 4 per duration · the subdivision happens HERE
     ↓
  timeframe_state      THE per-duration model output
     ↓
  cross_tf_state       reduction of axis VALUES ONLY · never reads an indicator
     ↓
  cross_pair_state     reduction of pairs
     ↓
  model_state          THE ENVELOPE
```

**Each level collapses over exactly one axis.** No level may produce a price, a target, a verdict,
or a probability. No level may read a field private to the level above it.

### The four names

| Level | Type | Wire key | Collapses over | Count |
|---|---|---|---|---|
| 1 | `TimeframeState` | `timeframe_state` | — (one pair, one duration) | per (pair, duration) |
| 2 | `CrossTfState` | `cross_tf_state` | the durations of one pair | per pair |
| 3 | `CrossPairState` | `cross_pair_state` | the watched pairs | per workspace |
| — | `ModelState` | `model_state` | the envelope | the deliverable |

Every name is `<collapsed-dimension>_state`. The four are **distinct and never reused** (V-106).

### Names that were considered and rejected

| Rejected | Because |
|---|---|
| `market_state` | meant two things — the per-duration object *and* "the model's output" in speech |
| `ladder` | already names the **duration set** everywhere else; describes shape, not content |
| `multi_timeframe` | asymmetric with the cross-pair level |
| `universe_state` | "universe" already means `meta.universe` and the dex-resolution candidate set |
| `market_overview` | the corpus already uses "overview" for four different things |
| `portfolio_view` | **`portfolio` is PME's domain** — equity, positions, exposure, margin, P&L |

---

## The deliverable

`model_state` contains **all three** states. A point-in-time document must be self-consistent, so
it is one payload; a consumer reads nothing else to render the model.

```jsonc
{
  "version": "12.0.0", "build_version": "4.0.0",
  "generated_at_ms": 1718000000000,
  "ladder_secs": [1,3,5,15,30,60,180,300,900,1800,3600,14400],

  "instances": {
    "BTC-USDT": {
      "cross_tf_state": { … },
      "durations": {
        "1": { "timeframe_state": { … } },
        "3": { "timeframe_state": { … } }
        // 12 entries
      }
    }
  },

  "cross_pair_state": { … }
}
```

**The socket splits it into three frame types.** They invalidate at different rates: a 1s tick
changes `timeframe_state["1"]` and `cross_tf_state`, but not `cross_pair_state` nor the eleven slow
durations. One payload on the wire re-broadcasts 12× for no new information (V-112).

**All three are stored, not recomputed.** The reductions are **not invertible**:

- `mean_bias` over twelve durations cannot be recovered from a count of five bearish pairs
- an ordinal `modal_stage` cannot be recovered at all

A consumer holding only the per-duration states could re-derive the direction mean but would have
to **invent** the momentum breadth, the progression mode and the level-convergence verdict — three
of four claims, recomputed differently by every consumer. Storing all three is what makes the UI
and the CLI byte-identical (V-113).

### What is deliberately not in it

| Data | Served by | Why not in `model_state` |
|---|---|---|
| 500 candles | `/api/history`, `die candles` | raw input, trivially recomputable |
| one indicator's series | `mme indicator <key>` | recomputable per indicator |
| 50 bars of typed signals | `timeframe_state.axes.*.readings[]` | derived, and both surfaces must agree on it |
| 4 axis aggregates | `timeframe_state.axes` | the point of level 1 |
| cross-duration reduction | `cross_tf_state` | non-invertible — stored |
| cross-pair reduction | `cross_pair_state` | non-invertible — stored |

The test: **if a consumer could compute it from cheaper data, it does not belong in the state.**

---

## Indicator identity

**One trader-recognizable observable = one indicator = exactly one owner axis.**

| Axis | Count | Character |
|---|---|---|
| Direction | 50 | Signed, domain `DirectionalBias`. The only axis averaged across durations |
| Momentum | 48 | Signed, domain `Acceleration`. Counted, never averaged |
| Structure | 34 | Signed, domain `NetEventBias`. Counted — an unweighted mean would not weight by event count |
| Progression | 25 | **Ordinal.** No arithmetic exists |
| **Total** | **157** | |

### The registry holds observables, not calculators

The registry is **157 observable keys**. Where several observables come from one calculation, they
share code through a `calculator` field:

| Observable key | Calculator | Owner axis |
|---|---|---|
| `rsi` | `rsi` | Direction |
| `rsi_slope` | `rsi` | Momentum |
| `rsi_divergence` | `rsi` | Structure |

Three keys, one calculator, three axes. The identity rule is about *what a trader recognises*, not
about how many passes over the candles it costs.

### Cluster vs family — the distinction that matters

| | Definition | Weight effect |
|---|---|---|
| **Cluster** | indicators reading the **same source series** | **none — display only** |
| **Family** | indicators that **cannot disagree** | one shared weight |

A cluster is a display convenience: grouping RSI-family visuals together must never change a score.
A family is a genuine redundancy: `williams_r ≡ stochastic_k − 100`, so if both vote they vote
identically and must count once.

**Families are rare.** The confirmed case today is one. A second candidate must survive the
incremental-information test before being admitted.

### Double-count prevention

Three layers, in order:

1. **Ownership is single.** Every reading declares exactly one `owner_axis`. Consumption by other
   axes is allowed and does not create ownership.
2. **Family members share one weight.** They cannot disagree, so they count once.
3. **The incremental-information test.** Within an axis, adding a reading must change the axis
   output on a fixed corpus. A reading that never changes it carries no information and is
   rejected — the per-axis form of SV-C/Q4.

Clusters get none of these, because they have no weight.

---

## Typed values

```rust
pub enum AxisValue {
    Signed    { value: f64, domain: Domain },   // [-1,1] WITH its meaning attached
    Rank      { pct: f64 },                     // [0,100] percentile vs own Tier A
    Ordinal   { stage: Stage },                 // 5 anchors, NO arithmetic ops
    Undefined { reason: UndefinedReason },      // never 0, never NaN (CH-7)
}
```

`Domain` is not decoration. A `Signed` value without its domain is a bare float, and a bare float
cannot be aggregated — which is exactly why `NetEventBias` and `Acceleration` may not be averaged
together.

**These are compile errors**, proven by `trybuild` compile-fail tests (V-37, V-38):

```rust
stage + 1        // E0369 — no Add impl
mean(stage)      // E0277 — Ord is not Add
stage * 0.5      // E0369
```

---

## Typed signals

```rust
pub enum Signal {
    Level      { price_low: f64, price_high: f64, basis: String },
    Event      { kind: EventKind, at_bar: usize, anchor: f64, subtype: Option<String> },
    Transition { kind: String, from: String, to: String },
}
```

| Variant | Is it priced? | Does it persist? | Example |
|---|---|---|---|
| `Level` | yes | yes | a Fibonacci retracement zone, an ATR envelope edge |
| `Event` | anchored to one | no | a confirmed divergence, a liquidity void |
| `Transition` | no | no | Stage 2 → Stage 3 |

**Divergence is an `Event` subtype, not a fourth variant.** A divergence is something that
happened at a bar to an anchor — the same shape as every other event.

`readings[]` holds **50 bars** of typed signal history per indicator. One cap, no override (V-95q).
A bare number is never a reading; if it cannot be named as one of the three variants, it is not in
the model (V-94q).

---

## What a reduction may do

```rust
pub enum LadderAggregation {
    MeanAgreement     { mean: f64, agreement_pct: f64, gradient: f64 }, // Direction only
    StageDistribution { rows: Vec<AxisState>, histogram: StageHistogram, gradient: f64 },
}
```

**The rule: a mean is legal only where the quantity shares a domain *and* means the same thing for
every member.** Everything else is a count, a mode, an interval, or a comparison.

| Reduction | Legal | Illegal |
|---|---|---|
| durations → `cross_tf_state` | mean over Direction · counts · ordinal mode/interval · gradients · convergence | averaging ordinals · **merging events or legs across durations** |
| pairs → `cross_pair_state` | `mean_bias` across pairs · counts · ordinal histograms | **any unweighted mean over event counts or ordinal stages** |

### `cross_tf_state`

Each axis is reduced by what its type allows:

| Axis | Reduction | Why not something else |
|---|---|---|
| Direction | `mean_bias` + `agreement_pct` + `gradient` | the only axis where a mean is meaningful |
| Momentum | breadth **count** + fast-vs-slow comparison | a mean would not weight by acceleration magnitude — +0.40 and +0.01 would count equally |
| Structure | **counts** + convergence + readiness count | a mean over `net_event_bias` would not weight by event count — 0 events and 8 events would count equally |
| Progression | **ordinal mode + interval + leg count** | ordinal — a mean will not compile |

**No `rows[]` in the reduction.** Every per-duration value already ships in the twelve
`timeframe_state` objects and is fetchable with `mme state --tf`. The reduction headlines only what
no single duration can say.

**Cross-duration agreement counts and compares; it never merges.** Two durations' events are two
events — `(kind, subject, anchor, timeframe)` is the identity, and the timeframe is part of it. The
reduction reports *how many* durations hold an active divergence, never one merged divergence.

**Level convergence** is a candle-derived tolerance: two durations' levels converge when they fall
within **1× ATR(n) of the slower duration** (V-99q). A tuned constant would not be reproducible from
published candles.

### `cross_pair_state`

| Field | Reduction | Why |
|---|---|---|
| `breadth.mean_bias` | mean of per-pair `direction.mean_bias` | shared domain `[-1,1]`, same meaning per pair — legal |
| `breadth.bias_range` | max − min of the same values | dispersion; the spread of a shared-domain quantity |
| `concordance` | **count** of pairs whose sign matches `mean_bias` | measured, not re-bucketed |
| `coverage` | **counts** of complete / stale / converging pairs | always legal |
| `progression.mode` | argmax of the stage histogram | ordinal mode, never a mean |

**Mean and range together are load-bearing.** Two markets can carry an identical breadth *count* and
be completely different:

| | 4 bullish | 5 bearish | 1 unclear | count breadth |
|---|---|---|---|---|
| **X** | +0.05 (barely) | −0.35 | — | −10% |
| **Y** | +0.90 (strongly) | −0.02 | — | −10% |

`mean_bias` and `bias_range` separate them in one line each. A single bucketed number never will
(V-110).

### The redundancy rule

**No cross-pair field is a monotone re-bucketing of another cross-pair field** (V-108).

The previous layer expressed one number four times — as a breadth enum, a synchronisation enum, a
global-bias enum and a risk distribution — so three of its four headline fields carried no
information the first did not. V-108 kills that class permanently, and
`systemic_risk_score` is deleted outright: its only consumer was PME's safety veto, which observe
mode never reaches.

---

## Freshness

Every reduction declares its slowest member. `cross_tf_state` names the slowest duration;
`cross_pair_state` names the slowest pair.

**A cross-duration view is only as fresh as its slowest member.** Without `member_freshness`, a
consensus figure silently mixes a one-second reading with a twenty-minute one and presents the
result as a single number.

---

## The domain boundary

`portfolio` · `position` · `exposure` · `capital` · `equity` · `pnl` · `margin` · `leverage` ·
`safety_state` are **PME identifiers**. None may appear in any model payload or field name (V-107).

PME's `PortfolioOverviewMatrix` is a different engine's output. It is not renamed, not borrowed and
not read.

---

## One CLI command per scope

| Command | Scope |
|---|---|
| `mme state <instance> --tf <secs>` | **one** duration — the four axes and their readings |
| `mme alignment <instance>` | **all** durations — the reduction |

No command means two different things depending on a flag. An earlier draft had `mme states [--tf]`
returning the per-duration view *with* the flag and the cross-duration view *without* it — the same
ambiguity as the cross-timeframe grid living inside the Metrics tab.

---

## The surface split

| Surface | Scope | Shows |
|---|---|---|
| **Metrics** | one duration | the 157 readings flat and filterable — candles, series, typed signals |
| **States** | one duration | the same 157 **grouped by axis** into 4 tracks + the axis aggregate |
| **Alignment** | all durations | the reduction — gradients, agreement, consensus, convergence |
| **Overview** | all pairs | `cross_pair_state` |

The progression is *what did each indicator say* → *what do the four axes say* → *what do the
durations say to each other* → *what do the pairs say to each other*.

**The cross-timeframe grid belongs in Alignment**, not Metrics. It is a cross-duration object and
Metrics is per-duration. That misplacement was the same class of defect as the payload duplication:
a cross-duration object living where a per-duration object belongs.

---

## Invariants this document produces

| # | Check |
|---|---|
| V-54 | payload is the three states inside `model_state` |
| V-106 | the four names are distinct and never reused |
| V-107 | no PME domain identifier in the model |
| V-111 | `model_state` contains all three; none derived on demand |
| V-112 | socket emits three frame types; document is one payload |
| V-113 | all three reductions stored; none re-derived by a consumer |
| V-93q | the axis view carries all 157 readings, none orphaned |
| V-94q | every reading is `Level`, `Event` or `Transition` |
| V-95q | `readings[]` capped at 50, no override |
| V-96q | `cross_tf_state` contains no indicator reference |
| V-97q | `cross_tf_state` computed from axis values only |
| V-99q | convergence uses the slower duration's ATR |
| V-100 | breadth counts `cross_tf_state`, never strategy advice |
| V-101 | cross-pair input is `&[CrossTfView]`, never `&[AdvisoryMatrix]` |
| V-102 | no `regime` identifier |
| V-103a | a mean only where domain shared **and** meaning per member |
| V-108 | no cross-pair field is a re-bucketing of another |
| V-110 | equal breadth count + different bias range stay distinguishable |

---

## Refines

- [`01-13`](01-13-state-axes-framework.md) — the axis framework; this document supplies the final
  vocabulary and the reduction-legality rule
- [`01-17`](01-17-change-requests.md) — CR-18, which produces this document
- [`02-16`](../matrices/02-16-market-state-matrix.md) — the schema, which this document renames to
  `timeframe_state`