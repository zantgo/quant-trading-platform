# MME Layer 4 — Opportunity Layer

**Version:** 11.9 (2026-09-17) — v11: quantity-first precondition defaults (trend 55, breakout 55/45, pullback 45, mean_rev 40, scalp struct 55, squeeze 0.25) + quality bands [75,60,45,25]. See docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Layer:** 4 of 7
**Output Contract:** [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md)
**Purpose:** This document specifies the Opportunity Layer — the process that sets parameters for and scores strategy-agnostic opportunities (breakout, continuation, pullback, mean-reversion, reversal) on a 0–100 scale.

---

## 1. Purpose

The Opportunity Layer identifies **positive** market configurations and scores their viability, independent of direction of exposure or execution parameters. It consumes the [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) and the underlying Metrics Matrix signals, producing the [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md).

```
[Analysis Matrix (L3)]      ─┐
[Metrics signals (L1)]       ─┤
                              ├──► OPPORTUNITY LAYER (L4) ──► [Opportunity Matrix]
[LiquidityFlow (L1.5)]       ─┤      profile + score
[LiquidationClusterMatrix    ─┘              │
  (L2.5)]                                    ▼
                                       L6 (Decision)
```

> **L1.5/L2.5 feeds only the `LiquiditySqueeze` precondition path; all other opportunity types read only L3 + L1.**

**Dependency edges:** L4 reads L3, L1 metrics signals, and L1.5/L2.5 liquidity products (see 02-00-matrix-field-ownership.md §5). L4 does **not** read L5. L4 outputs to L6 only. See [02-00-matrix-field-ownership.md](../../matrices/02-00-matrix-field-ownership.md).

---

## 2. Candidate Setup Types

The layer profiles each candidate `OpportunityType`. The canonical enum is **eight-valued**: the original five directional setups (`TrendContinuation`, `Breakout`, `Pullback`, `MeanReversion`, `Reversal`) plus the sentinel `NoClearOpportunity`, extended by `LiquiditySqueeze` (Phase 0-4 Liquidity Intelligence) and `Scalp` (v2.1 institutional completeness sweep). See [02-08-opportunity-matrix.md §3](../../matrices/02-08-opportunity-matrix.md) for the canonical precondition table and §4 for the decision tree:

| Setup | Precondition Signature |
|-------|------------------------|
| `TrendContinuation` | Strong/healthy trend + directional bias + non-exhausted momentum. |
| `Breakout` | Volatility expansion + healthy structure + compression release / level breach. |
| `Pullback` | Established trend + weakening momentum + retrace to dynamic level. |
| `MeanReversion` | Volatility compression + range regime + oscillator extreme. |
| `Reversal` | Confirmed divergence + structure break + reversing momentum. |
| `LiquiditySqueeze` | `LiquidityFlow.cascade_state ∈ {Detected, Sustained}` AND `|LiquidationClusterMatrix.cascade_asymmetry| > 0.3` AND `regime ∈ {EXPANSION, TRANSITION}`. Surface as a defensive opportunity — drives `CLOSE_ONLY` stance policy and tightens stops. |
| `Scalp` | High per-candle volatility (BBWP ∈ [70, 95)) + tight structural context (alignment dim 4 `Structure` ≥ 70) + directional bias + regime ∈ {TRENDING_BULL, TRENDING_BEAR}. Sub-minute-to-seconds holding period; maps to `time_horizon = SCALP`. Designed for HFT-adjacent, complementary to `Breakout` (multi-bar continuation) and `TrendContinuation` (multi-day). |
| `NoClearOpportunity` | No candidate met its preconditions (and no `LiquiditySqueeze` is active). |

---

## 3. Scoring Model

Each candidate's `score ∈ [0, 100]` blends four factors:

$$\text{score} = 0.35\,Q_{ctx} + 0.30\,S_{sig} + 0.20\,A_{mtf} + 0.15\,F_{fresh}$$

| Factor | Source |
|--------|--------|
| `Q_ctx` — context quality | Analysis `market_quality` + relevant assessment dimension. |
| `S_sig` — signal support | Strength + confirmation status of contributing signals. |
| `A_mtf` — MTF agreement | Alignment `trend_agreement_pct` for directional setups. |
| `F_fresh` — freshness | Inverse of youngest contributing signal `age_bars`. |

> **v6.10.1 activation vs viability note.** The score above is the raw viability blend; it is **not** gated by the precondition completion ratio. The previous implementation multiplied `score` by `preconditions_met / preconditions_total`, which collapsed every inactive setup (e.g. `preconditions 0/3 met`) to `score = 0` — the operator lost the view of "how close" each setup was to firing. The v6.10.1 fix returns the raw blend so every non-`NoClear` profile surfaces its true viability. Activation is communicated via the per-profile `preconditions_met` / `preconditions_total` fields (rendered as a dedicated progress bar in the UI) and via the Rust-only `scoring_factors.precondition_ratio` for telemetry. `OpportunityType::NoClearOpportunity` keeps the unconditional-zero sentinel because it is the explicit "no setup detected" placeholder. See [02-08-opportunity-matrix.md §6](../../matrices/02-08-opportunity-matrix.md) and `docs/CHANGELOG.md v6.10.1`.

The **primary opportunity** is determined by the priority-ordered decision tree in [02-08-opportunity-matrix.md §4](../../matrices/02-08-opportunity-matrix.md) (first match wins). The `opportunity_score` and `profiles[]` array expose the full scoring breakdown for downstream consumers but do **not** override the tree selection. In a tie, the profile with the higher `preconditions_met / preconditions_total` ratio wins.

---

## 4. Setup-Quality Classification

The canonical SetupQuality score→label bands are defined in [Opportunity Matrix §5](../../matrices/02-08-opportunity-matrix.md) (see also [01-01-ontology.md §A.4](../../conceptual-foundations/01-01-ontology.md)); this layer does not re-define them. The bands are lower-inclusive half-open intervals `[a, b)` — `Prime` ≥ 85, `Strong` [70, 85), `Moderate` [50, 70), `Marginal` [30, 50), `None` < 30 — so each `opportunity_score ∈ [0, 100]` maps to exactly one band, no endpoint ambiguity.

---

## 5. Parameter Setting

For each profiled opportunity the layer records the preconditions evaluated and satisfied (`preconditions_met / preconditions_total`), the contributing signal labels, and an invalidation note describing what would nullify the setup. These parameters flow to the Decision Layer to shape entry/target/protection strategy.

---

## 6. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Direction-neutral scoring** | Score reflects viability, not profit expectation. |
| **Strategy-agnostic** | No assumption of a specific trading methodology. |
| **Explainable** | Score decomposes into four weighted factors + precondition fractions. |
| **Bounded** | All scores clamp to `[0, 100]`. |

---

## 7. Cross-References

- [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) — Input.
- [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md) — Output contract.
- [MME Layer 6 — Decision Support](03-02-07-mme-layer6-decision-support.md) — Consumer.
