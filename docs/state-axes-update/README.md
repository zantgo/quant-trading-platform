# State Axes — Independent Specification

A self-contained specification of a **descriptive market-state layer** organised as four
orthogonal axes, with a candle-only data contract, a bounded retention policy, an explicit
current-leg record, and an event-lifecycle policy.

This folder is complete on its own. It does not depend on any other documentation, and it
describes no other part of any host program. Copy the whole folder into a new project and it
stands up as written.

---

## What this system is

It answers **what the market is doing right now**. It does **not** answer what will happen
next. Everything here is descriptive — observed state, published as levels, ordinals, and
percentile ranks.

### The three standing rules

| Rule | Statement |
|---|---|
| **Rule-based and deterministic** | Every reading is a published formula over available candles. Nothing is trained, fitted, sampled at random, or simulated stochastically. Determinism is defined over the **complete input tuple** — candles, availability timestamps, configuration, reference-set definition, spec version and build version — and equality is **bit-for-bit** unless a report explicitly declares per-quantity tolerances. [`framework.md`](framework.md) **§8.1**, **P9**, **I43**, **I44** |
| **Descriptive, not probabilistic** | The layer reports deterministic measurements and comparisons against explicitly defined historical reference sets. A percentile rank describes the observation **relative to that reference set**; it does not claim a probability of a future outcome, and it is not independent of how the set was defined. **P1**, **I1**, **I9** |
| **Backtesting validates strategy, never the model** | A backtest may exercise a strategy built on these readings. It may **not** be used to accept, reject, or tune a reading, an axis, or the model. The model's only permitted test is the **causal replay gate** — by re-executing it, never by trading it. **P10**, **I45**, [`framework.md`](framework.md) §8.2 |

> **The word "stochastic" appears in this package only as the Stochastic Oscillator.** There is no
> randomness, sampling or simulation anywhere in the framework.

> **A passing result is evidence for the cases tested — never a proof.** The gates and the
> semantic validation suite establish what was actually exercised. Untested inputs and
> unexercised case classes remain unverified, and a gap in coverage is a gap in knowledge rather
> than proof of absence. [`framework.md`](framework.md) **§8.4**.

---

### The model and the strategy

This package is **the model**. It is not a trading system.

| | **The model** — this package | **The strategy** — not this package |
|---|---|---|
| Asks | *What is the market doing right now?* | *What should be done about it?* |
| Produces | Observed state — ordinal stages, percentile ranks, dated events | Actions — entries, exits, sizing, allocation |
| Predicts? | **No.** No horizon, no likelihood | Often, and that is legitimate |
| Verified by | **Causal replay** — re-execution | **Backtest / paper / live** |
| Owns risk? | No | Entirely |

```
candles ──► MODEL (this package) ──► read-only state ──► STRATEGY ──► action
                           ◄── the model never reads the strategy ──
```

> **Direction is reported, never advised.** The Direction axis may read −1 and say *bearish*. It
> may not say *sell*. That single difference is the whole boundary.

Full definitions, the forbidden-contents list and the naming discipline:
[`framework.md`](framework.md) **§1.1**.

The single organising idea: **one indicator usually serves more than one axis, and which
axis it feeds depends on which property of it is read.**

| Property read | Axis |
|---|---|
| The **level** of a directional oscillator | Direction |
| The **rate of change** of that oscillator | Momentum |
| Its **divergence** against price, or the events it emits | Structure |
| Its **displacement from an anchor** | Progression |

This is why axes are not populated by assigning indicators to boxes. RSI contributes to three
axes from a single calculation.

---

## The four axes

| # | Axis | Question | Output | Character |
|---|---|---|---|---|
| 1 | **Direction** | Which way, and with what magnitude? | Signed value `[-1, +1]` + magnitude | Instantaneous, level-based |
| 2 | **Momentum** | How forcefully, accelerating or decelerating? | Signed force + acceleration state | Rate-based |
| 3 | **Structure** | How is the move unfolding? | Discrete dated events + relationships | Event-based |
| 4 | **Progression** | How far along is the current movement? | Ordinal stage + reference stability | Revision-prone |

Plus two derived readings — **State Consensus** (% of axes agreeing) and **State Stability**
(holding vs oscillating).

Three further concepts are **named and scoped but are not axes** — Regime (contextual modulator,
reserved), Terminality (`continuation_supported` / `indeterminate` / `termination_evidence` —
definable now, and explicitly *not* the completion event), and Confidence (separate meta-measure,
reserved). See [`framework.md`](framework.md) §5.9.

---

## Folder map

| File | Contents |
|---|---|
| [`execution-plan.md`](execution-plan.md) | The agentic build plan — roles, phases, the 13-gate causal replay matrix, mutation programme M1–M7, report format, anti-goals |
| [`verification-status.md`](verification-status.md) | **What is and is not verified right now** — four layers V-1…V-4 with honest current status |
| [`measurement-quality.md`](measurement-quality.md) | The per-reading quality review — Q1–Q4, verdicts, and the **completed Progression audit** (22 specs → 11–13) |
| [`change-requests.md`](change-requests.md) | Formal amendments to frozen items — CR-1 Tier A, CR-2 byte budget, CR-3 independent settings, CR-4 registry authority, CR-5 generated totals |
| [`framework.md`](framework.md) | The specification proper: purpose, axes, principles, definitions, invariants, anti-patterns, build order |
| [`leg.md`](leg.md) | The versioned current-leg record — fields, structural scale and its causal classification, `occurred_at` vs `available_at`, the `progress` arithmetic and its undefined case, target revisions and stability transitions, lifecycle, invariants L1–L24 |
| [`event-lifecycle.md`](event-lifecycle.md) | Retention as a lifecycle — activation, persistence, expiration, invalidation, reactivation; invariants E1–E10 |
| [`retention.md`](retention.md) | Four bounded retention tiers, the tier chain, what the byte budget governs, independent settings, invariants T1–T6 |
| [`visual-vocabulary.md`](visual-vocabulary.md) | The dot-on-track primitive, the three channels, per-axis variables, readiness versus confidence, invariants V1–V10 |
| [`vocabulary.md`](vocabulary.md) | Signal kinds, divergence rules, level vocabulary — the shared language |
| [`inventory.md`](inventory.md) | All 160 readings in one table — **generated** from the registry, never hand-edited |
| [`indicators/`](indicators/) | One full specification file per reading — 122 files |

**Start with `framework.md`**, then `leg.md` and `event-lifecycle.md` — those three are what the
build order actually depends on.

---

## Scope — what is deliberately absent

Two components are **excluded on purpose**, named here so they are not reintroduced by
accident:

- **Continuation / termination risk** — "will this extend, stall, or reverse over a stated
  horizon." That is a probability claim. Evidence for it exists (divergence, exhaustion,
  character change) but none of it establishes the outcome.
- **Calibrated confidence** — a **scope** decision, not a mathematical prohibition. This layer
  reports defined measurements and does not claim a calibrated probability that its
  interpretation is correct. A confidence estimate built from the same features it informs would
  be circular, but confidence is not inherently invalid — only uncalibrated here. Adding it
  would require its own target definition, calibration methodology and independent validation.

Both may be added later, in a layer downstream of this one, once outcome data exists.

> **Being a well-defined observer and being a useful observer are different properties.** The
> causal replay gate establishes the first. The second requires **semantic validation** against
> independently defined reference examples — see [`framework.md`](framework.md) §8.4.

> Naming a concept is not the same as deleting the question. **Terminality** — the *ordinal*
> question "does the evidence support continuation, or a potential end?" — is answerable today
> from completion criteria and is specified in `framework.md` §5.9. What is excluded is
> terminality **as a probability over a horizon**. Likewise **Regime** is reserved as a
> contextual modulator and **Confidence** as a separate meta-measure, so that when either is
> added it is added deliberately rather than smuggled onto an existing axis.

State Consensus and State Stability are deliberately **not** called "confidence."

---

## Scale

| | Count |
|---|---|
| Axes | 4 |
| Named non-axis concepts | 3 — Regime, Terminality, Confidence |
| **Readings** (axis-serving properties) | **160** |
| — present today | 76 |
| — to be added | 84 |
| **Indicator specification files** | **122** |
| — present indicators | 44 |
| — new indicators | 78 |
| Retention tiers | 4, all bounded |
| Invariants | 42 data · 27 leg · 10 event · 10 visual · 6 retention |
| Gates | 13 causal replay gates (G1–G13), all runtime-executed |
| Mutations | 7 (M1–M7), each must be detected |
| Milestones | 4 (MS-1 … MS-4); **MS-1 = G1 + M1 only** |
| Validation kinds | 4 — spec conformance · causal replay · **semantic validation** · strategy backtesting |
| Quality review | 4 questions · 6 verdicts · **Progression audited (22 → 11–13)** · 3 axes pending |
| Inventory revision | **160 / 76 / 84** — was 162 / 76 / 86; one duplicate removed from two rows. Record: **CR-6** |
| Semantic validation | 3 case families — **reference · adversarial · metamorphic** |
| Verification | **CONSISTENT** (V-1 docs ✅) · V-2 gates ⬜ · V-3 mutations ⬜ · V-4 reproducible evidence ⬜ · SV semantic validation ⬜ |
| Status labels | 6-value controlled vocabulary — **VERIFIED is reserved** and unavailable until every layer holds |
| Data sources | **1** — OHLCV candles |

Readings exceed files because one indicator can serve several axes. RSI alone contributes
three readings from one calculation.
