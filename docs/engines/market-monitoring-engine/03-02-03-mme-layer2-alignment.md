# MME Layer 2 — Alignment Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Layer:** 2 of 7
**Output Contract:** [Alignment Matrix](../../matrices/02-01-alignment-matrix.md)
**Purpose:** This document specifies the Alignment Layer — the process that correlates multiple single-timeframe Metrics Matrices for one symbol to measure cross-timeframe agreement, detect timeframe conflict, and compute trend/momentum consensus scores.

---

## 1. Purpose

The Alignment Layer answers *"do the timeframes agree?"*. It consumes the set of Metrics Matrices for a symbol (micro/fast/slow/macro) and produces the 10-dimensional [Alignment Matrix](../../matrices/02-01-alignment-matrix.md).

```
[Metrics Matrix × 4 timeframes] ──► ALIGNMENT LAYER (L2) ──► [Alignment Matrix]
                                     compute_alignment()        (10 dimensions)
```

Implementation: `crates/core-domain/src/alignment.rs::compute_alignment()`.

---

## 2. Input Assembly

The layer collects one `MarketContext` per active timeframe (re-synthesized from each timeframe's indicator map) plus per-timeframe metadata:

| Input per timeframe | Used for |
|---------------------|----------|
| `MarketContext` (trend/momentum/volume/volatility/regime) | Signed dimension consensus. |
| Indicator map | Structure (S/R), liquidity (RVOL) dimensions. |
| Active signal count | Cross-TF signal confluence. |
| Reference price | Per-TF breakdown. |

---

## 3. Timeframe Weighting

Higher timeframes carry more weight in the consensus:

$$w_{tf} = \text{clamp}\left(\frac{\text{duration\_seconds}}{\text{divisor}},\ 0.2,\ 1.0\right)$$

The divisor is the session's **slowest enabled tier's duration** (see [Timeframe Model §4](../../conceptual-foundations/01-04-timeframe-model.md) and [Alignment Matrix §4.1](../../matrices/02-01-alignment-matrix.md)). The slowest active tier always weights `1.0`; shorter tiers scale down proportionally, regardless of which tier is active. This is dynamic rather than the fixed `900 s` constant so custom sessions (e.g. macro = 1 d, or `macro_timeframe.enabled = false`) retain a proper hierarchy.

**Divisor rule.** `divisor = max({duration_seconds for tier in enabled_tiers})` — the slowest active tier wins. When the macro tier is disabled, the divisor stays bound to whichever tier is slowest.

Signed consensus for trend/momentum/volume/volatility:

$$\text{mtf\_alignment} = \text{clamp}\!\left(\frac{\sum_{tf} \text{score}_{tf}\, w_{tf}}{\sum_{tf} w_{tf}},\ -1,\ 1\right)$$

> **Target Architecture (Not Yet Implemented).** The 10 Alignment Dimensions are intended to be computed as **parallelized vector operations**. Because the underlying per-timeframe scores are stored contiguously in the DOD hot path, the CPU can load them into vector registers (AVX/SSE) and compute the weighted consensus across all dimensions in `< 3 ms`. *Current implementation:* consensus is computed with scalar `f64` arithmetic over `HashMap`-keyed per-timeframe maps.

---

## 4. The 10 Alignment Dimensions

| # | Dimension | Basis |
|---|-----------|-------|
| 0 | Trend | Weighted mean per-TF trend score. |
| 1 | Momentum | Weighted mean per-TF momentum score. |
| 2 | Volume | Weighted mean per-TF volume score. |
| 3 | Volatility | Weighted mean per-TF volatility score. |
| 4 | Structure | % of TFs with agreeing S/R role. |
| 5 | Signal | % of signals appearing across ≥2 TFs. |
| 6 | Regime | % of TFs sharing the dominant regime. |
| 7 | Confidence | Consistency (`100 − stddev`) of per-TF confidence. |
| 8 | Liquidity | RVOL consistency (`1 − coefficient of variation`). |
| 9 | Tradability | % of TFs with non-neutral, non-compressed conditions. *(Renamed from "Opportunity" — L4 owns opportunity concepts.)* |

Full computation and `AlignState` derivation: [Alignment Matrix §3](../../matrices/02-01-alignment-matrix.md).

---

## 5. Overall Score & Trend Agreement

$$\text{mtf\_overall\_score} = \text{clamp}\big((0.5T + 0.3M + 0.1V_{t} + 0.1V_{m}) \times 100,\ -100,\ 100\big)$$

where `T` = trend alignment, `M` = momentum alignment, `V_t` = volatility alignment, `V_m` = volume alignment.

$$\text{trend\_agreement\_pct} = \frac{\max(\text{pos\_tf}, \text{neg\_tf})}{\text{total\_tf}} \times 100$$

---

## 6. Timeframe Conflict Detection

Conflict is surfaced through low `trend_agreement_pct` and `Mixed` dimension states:

| Condition | Interpretation |
|-----------|----------------|
| `trend_agreement_pct ≥ 75` | Strong multi-timeframe consensus. |
| `50 ≤ trend_agreement_pct < 75` | Partial agreement; caution. |
| `trend_agreement_pct < 50` | Conflict — timeframes disagree; downstream confidence is capped. |

Per-timeframe entries are split into supporting vs contradicting evidence by the Analysis Layer, providing an explainable conflict trace.

---

## 7. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Dimensional invariant** | Exactly 10 dimensions, always. |
| **Higher-TF dominance** | Weighting favours slower, more reliable timeframes. |
| **Read-only** | Never triggers indicator recomputation. |
| **Empty safety** | No timeframes → `NO_DATA` with 10 zero dimensions. |

---

## 8. Cross-References

- [Metrics Matrix](../../matrices/02-07-metrics-matrix.md) — Input.
- [Alignment Matrix](../../matrices/02-01-alignment-matrix.md) — Output contract.
- [MME Layer 3 — Analysis](03-02-04-mme-layer3-analysis.md) — Direct consumer.
