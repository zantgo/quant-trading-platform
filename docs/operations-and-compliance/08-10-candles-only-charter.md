# Candles-Only Charter

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

The market-monitoring layer computes state from **candles**. Nothing else. This document
states that boundary as eight normative rules, because the temptation to cross it is
constant and each crossing is individually reasonable.

---

## 1. Why a charter

Every rule below exists because a reasonable engineer, working on a reasonable feature,
would cross it and produce something that looks correct and cannot be reproduced.

The rationale for candle-only is not aesthetic. It is that **a reading that cannot be
recomputed from a candle archive can never be validated by replaying history**, and
replay is the only instrument this layer accepts (invariant **P10** of
[01-13](../conceptual-foundations/01-13-state-axes-framework.md)).

---

## 2. The eight rules

| # | Rule | What it forbids in practice |
|---|---|---|
| **CH-1** | **Candles only.** Every reading is computed from OHLCV candles. | No order book, open interest, funding, mark/index, liquidation or forced-close stream may reach a reading, an axis or a layer. A candle *constructed* from a live price feed is still a candle: the construction method is ingestion, not a reading. |
| **CH-2** | **CPU only.** | No GPU or accelerator code. No CUDA, OpenCL, wgpu, or ML runtime. |
| **CH-3** | **Rule-based and deterministic.** Published formulas over available candles. | Nothing trained, fitted, sampled at random, or simulated stochastically. |
| **CH-4** | **No statistics for now.** | No probability, no inference, no significance testing, no Monte Carlo **in the model**. A percentile rank against a declared reference set is arithmetic and is permitted; a probabilistic claim is not. |
| **CH-5** | **Descriptive, never predictive.** | The model may read −1 and say *bearish*. It may not say *sell* (**I48**). |
| **CH-6** | **One-way interface.** | The model never reads the strategy's output (**I47**). Enforced by a field-set test, not by convention — see [01-21](../conceptual-foundations/01-21-engine-split-and-model-boundary.md) §5. |
| **CH-7** | **Typed absence.** | `UNDEFINED` is a distinct value. Never `0`, never `NaN`, never a substituted default (**L14**). |
| **CH-8** | **Determinism.** | Same `(candles, config, spec version, build version)` → bit-identical output (**I44**, `D1`). |

---

## 3. What CH-4 does and does not permit

The line is easy to state and easy to blur, so it is stated with examples.

| Permitted (arithmetic on a declared reference set) | Forbidden (a claim about outcomes) |
|---|---|
| "ATR is at the 92nd percentile of its own last 300 bars" | "There is a 92 % chance price reaches the target" |
| "Direction persistence is 71.5 % of its own history" | "This setup is 71.5 % likely to work" |
| "Bollinger %B sits above its own 70th-percentile band" | "The band will hold" |
| "Events per bar ranks at the 60th percentile of its own window" | "Event density predicts a reversal" |

A rank is a **measurement relative to a declared reference set**. It asserts nothing about
the future, and it is not independent of how that set was defined (**I51**). Every rank in
this layer is published with the reference definition that produced it.

---

## 4. What CH-1 removed, and what replaced it

The derivatives and order-book telemetry that previously fed readings is deleted in phase
**P5**. The corpus names candle substitutes for the capabilities it gave up
([01-13](../conceptual-foundations/01-13-state-axes-framework.md) §7.3):

| Removed capability | Candle-only substitute | Status |
|---|---|---|
| Forced-close cascade lifecycle | Volume climax → exhaustion climax: an extreme-volume, extreme-range bar that fails to extend | established technique |
| Cascade intensity | Relative-volume percentile + range expansion | established technique |
| Leverage-cluster levels | Volume-profile POC / VAH / VAL, swing extremes — published as **state bands** (`tf.stateBands`) | established technique |
| Order-book pressure | Volume-weighted directional pressure: CMF, Force Index, Ease of Movement | established technique |
| Spread | ATR — the candle's own realised range | established technique |

**These are not equivalents in precision.** Volume and range are the footprint of the
activity; they do not identify who was aggressive. Every such reading is labelled a
price-derived estimate (**I12**, **I13**).

---

## 5. Enforcement

| Rule | Enforced by |
|---|---|
| CH-1 | `market-analyzer/tests/candle_purity.rs` — walks every reading's `derived_from` graph and proves the roots are OHLCV fields |
| CH-2 | source scan for accelerator/ML runtime identifiers |
| CH-3, CH-8 | `gate_cg12_determinism` + the reproducibility banner whose commit must equal `HEAD` |
| CH-4 | source scan + `states_no_strategy.rs` (no probability field on any axis) |
| CH-5 | `MarketState` field-set test (no action vocabulary) |
| CH-6 | field-set test + `V-12`/`V-13` |
| CH-7 | `gate_cg6_degenerate` + serialisation round-trip |

A rule with no mechanical check is a preference. Each rule above names the check that
makes it a rule; a rule that cannot name one is not yet in force, and says so.

---

## 6. Related documents

- [01-13 State Axes Framework](../conceptual-foundations/01-13-state-axes-framework.md) — principles P1–P10, invariants I1–I51
- [01-21 Engine Split and Model Boundary](../conceptual-foundations/01-21-engine-split-and-model-boundary.md) — where the model stops and the strategy begins
- [02-16 MarketState Matrix](../matrices/02-16-market-state-matrix.md) — the model's single output schema
- [08-08 Candle Buffer Spec](08-08-candle-buffer-spec.md) — the single 500-candle retention cap