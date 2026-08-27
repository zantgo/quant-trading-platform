# MME Layer 3 — Analysis Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Layer:** 3 of 7
**Output Contract:** [Analysis Matrix](../../matrices/02-02-analysis-matrix.md)
**Purpose:** This document specifies the Analysis Layer — the process that transforms multi-timeframe agreement into a complete market interpretation: categorical bias, the continuous `market_bias_score`, regime classification, and real-time regime detection.

---

## 1. Purpose

The Analysis Layer is the transition from *observation* to *understanding*. It consumes the [Alignment Matrix](../../matrices/02-01-alignment-matrix.md) and produces the [Analysis Matrix](../../matrices/02-02-analysis-matrix.md).

```
              [Alignment Matrix] ──► ANALYSIS LAYER (L3) ──► [Analysis Matrix]
                                          derive_analysis()        bias + regime + assessments
                                                              │
                                                ┌─────────────┼─────────────┐
                                                ▼             ▼             ▼
                                          L4 (Opportunity)  L5 (Risk)   L6 (Decision)
```

The L3 output fans out to three downstream consumers. **L4 and L5 are strictly orthogonal** — they do not read each other's matrices. L6 is the only synthesis point. See [02-00-matrix-field-ownership.md](../../matrices/02-00-matrix-field-ownership.md).

Implementation: `crates/core-domain/src/analysis.rs::derive_analysis()`.

---

## 2. Continuous Bias Calculation

The headline `market_bias_score` is the alignment `mtf_overall_score` (range `[-100, 100]`), interpreted as `[-1.0, +1.0]` after scaling. The categorical `MarketBias` buckets it using **half-open intervals** (deterministic at every integer endpoint — no double-mapping):

| `mtf_overall_score ∈ [-100, 100]` | MarketBias |
|-----------------------------------|------------|
| `> 40` | `STRONG_BULLISH` |
| `> 20 AND ≤ 40` | `BULLISH` |
| `≥ -20 AND ≤ 20` | `NEUTRAL` |
| `≥ -40 AND < -20` | `BEARISH` |
| `< -40` | `STRONG_BEARISH` |

> **Boundary precision.** The bands use strict half-open intervals so integer endpoints (`score = 20.0`, `40.0`, etc.) map to exactly one band; the canonical derivation rule is in [02-02-analysis-matrix.md §3.1](../../matrices/02-02-analysis-matrix.md).

### 2.1 Confidence Model

```
base = |mtf_overall_score| / 100
state_confidence = base
+0.15 if trend_agreement_pct ≥ 75
cap 0.5 if trend_agreement_pct < 50
+0.10 if signal_cross_tf_count ≥ 3
cap 0.5 if timeframes_present ≤ 1
state_confidence = clamp(state_confidence, 0, 1)
```

---

## 3. Real-Time Regime Detection

The layer classifies the structural regime from the alignment score and per-timeframe context using the canonical decision tree in [Analysis Matrix §3.2](../../matrices/02-02-analysis-matrix.md):

| Regime | Trigger |
|--------|---------|
| `EXPANSION` | `bbwp ≥ 85` (priority 1) |
| `CONTRACTION` | `bbwp ≤ 10` (priority 1) |
| `TRENDING_BULL` | `adx ≥ 25` AND `score > +20` (priority 2) |
| `TRENDING_BEAR` | `adx ≥ 25` AND `score < -20` (priority 2) |
| `ACCUMULATION` | 1-bar score delta (`score − previous_score`) `> 0` AND `score ≥ 0` AND no expansion (priority 3) |
| `DISTRIBUTION` | 1-bar score delta `< 0` AND `score ≤ 0` AND no expansion (priority 4) |
| `TRANSITION` | `adx < 25` AND `bbwp` in `(10, 85)` AND `previous_regime != RANGE` on the previous **1** bar (priority 5) |
| `RANGE` | default — none of the above (priority 6) |

`ACCUMULATION` / `DISTRIBUTION` fire on the **single-bar score delta** (the prior bar's `mtf_overall_score` — not a 3-bar slope), and `TRANSITION` fires when the **previous bar's** regime was not `RANGE` (a 1-bar shift, not a 3-bar window). Regime enum values serialize PascalCase on the wire (`TrendingBull` / `TrendingBear` / `Range` / `Accumulation` / `Distribution` / `Expansion` / `Contraction` / `Transition`); the SCREAMING forms above are the `Display` vocabulary.

The full decision tree with detailed conditions lives in the canonical Analysis Matrix spec; this layer is a thin executor of that tree. Regime detection is continuous — it re-evaluates on every completed candle, enabling downstream layers to adapt (e.g. the Decision Layer's strategy environment).

> **Direct L3 → L6 edge.** The Analysis Matrix is consumed by [Layer 6 (Decision Support)](03-02-07-mme-layer6-decision-support.md) directly — in addition to being an input to Layers 4 and 5. Specifically, the Decision Layer reads `bias`, `state_confidence`, `market_quality`, `market_regime`, and the five `*_assessment` fields (Trend, Momentum, Volume, Volatility, Structure — plus `market_quality` as a separate categorical field, for a total of six qualitative outputs from L3) directly from L3. See [Sequence A](../../conceptual-foundations/01-03-systemic-data-flow.md#sequence-a-market-telemetry--analysis-cascade-the-observation-loop).

---

## 4. Six Qualitative Assessments

Each is derived from a specific alignment dimension score (see [Analysis Matrix §4.2](../../matrices/02-02-analysis-matrix.md)):

| Assessment | Source dim | Vocabulary |
|-----------|-----------|-----------|
| Trend | 0 | `WEAK` / `DEVELOPING` / `HEALTHY` / `STRONG` / `EXHAUSTED` |
| Momentum | 1 | `INCREASING` / `STABLE` / `WEAKENING` / `EXHAUSTED` / `REVERSING` |
| Volume | 2 | `WEAK` / `NORMAL` / `STRONG` / `EXCEPTIONAL` |
| Volatility | 3 | `COMPRESSED` / `NORMAL` / `EXPANDING` / `EXTREME` / `UNSTABLE` |
| Structure | 4 | `STRONG` / `HEALTHY` / `WEAK` / `BROKEN` / `UNKNOWN` |
| Quality | mean(0,1,2,4) | `POOR` / `WEAK` / `AVERAGE` / `GOOD` / `EXCELLENT` |

*Note: the `Opportunity` assessment was removed in the institutional redesign — `OpportunityType` is now produced by L4 (the [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md)) as a forecast field, not a state interpretation.*

*Note: only `StructureAssessment` admits `UNKNOWN`; Trend / Momentum / Volatility / Volume have **no** `UNKNOWN` variant (their fall-through bands are `EXHAUSTED` / `REVERSING` / `UNSTABLE` / `WEAK`). Enum values serialize **PascalCase** on the wire (`Weak` / `Developing` / `Healthy` / `Strong` / `Exhausted`; `Compressed` / `Normal` / `Expanding` / `Extreme` / `Unstable`; `Poor` / `Weak` / `Average` / `Good` / `Excellent`); the SCREAMING forms above are the `Display` vocabulary.*

### 4.1 Numeric companions (v6.12)

Each qualitative enum is bucketed from a specific 0-100 alignment dimension score (§4.2 table above, implemented in `derive_analysis`). Since v6.12 those exact inputs travel on the matrix as `AnalysisMatrix.trend_score` / `momentum_score` / `structure_score` / `volatility_score` / `volume_score` — L3-owned derivations from L2 (the disaggregated siblings of `market_quality_score`), stamped inside `derive_analysis` itself; no L1 involvement.

The Analysis panel renders each as a monospace badge on its qualitative card (no tooltip), tinted by coarse band heat and carrying a ▲/▼ delta against the prior frame (UI-side over the WS stream). Each card carries exactly one numeric badge — the v6.11 `trend_stability_sharpe` badge on the Trend card was **removed in v6.14** along with the matrix field and its export pair (the L1→L3 traceability-evidence exception now covers only `representative_bbwp`/`representative_adx` — see [02-00-matrix-field-ownership.md §5](../../matrices/02-00-matrix-field-ownership.md)); the L1 `price_trend_sharpe` indicator remains the sole Sharpe family member on the Metrics tab ([04-02-52](indicators/04-02-52-price-trend-sharpe.md)).

---

## 5. Explainability Trace

The layer emits:

- `market_interpretation` — natural-language summary of regime + assessments.
- `rationale` — the numeric derivation (score, agreement %, cross-TF signals).
- `supporting_signals` / `contradicting_signals` — per-timeframe evidence split by whether it agrees with the derived bias.

This satisfies the ontology's Explainability principle: the categorical bias is always traceable to its numeric and per-timeframe evidence.

---

## 6. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Single interpretation** | Exactly one bias, regime, and set of assessments per symbol. |
| **Risk-free** | The Analysis Layer never evaluates danger (that is Layer 5). |
| **Deterministic** | A given Alignment Matrix always yields the same Analysis Matrix. |
| **Empty safety** | Zero timeframes → neutral empty analysis. |

---

## 7. Cross-References

- [Alignment Matrix](../../matrices/02-01-alignment-matrix.md) — Input.
- [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) — Output contract.
- [MME Layer 4 — Opportunity](03-02-05-mme-layer4-opportunity.md) · [MME Layer 5 — Risk](03-02-06-mme-layer5-risk.md) — Consumers.
