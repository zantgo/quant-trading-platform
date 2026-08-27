# MME Layer 7 — Overview Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved — feature status is tracked in [README §Feature Status](../../README.md).
**Engine:** Market Monitoring Engine (MME)
**Layer:** 7 of 7
**Output Contract:** [Overview Matrix](../../matrices/02-09-overview-matrix.md)
**Purpose:** This document specifies the Overview Layer — the process that aggregates every symbol's Decision Matrix and Alignment Matrix (v6.10.3+) into global breadth indices, cross-timeframe alignment aggregates, asset rankings, market-health summaries, and the Systemic Risk Score.

---

## 1. Purpose

The Overview Layer synthesizes cross-symbol intelligence. Where Layers 1–6 describe one asset, the Overview Layer describes the **whole monitored universe**, producing the [Overview Matrix](../../matrices/02-09-overview-matrix.md).

```
[Decision Matrix × all symbols]    ─┐
                                    │
[Alignment Matrix × all symbols]    ─┼──► OVERVIEW LAYER (L7) ──► [Overview Matrix]
                                    │       compute_overview()
[Instance metadata              ]  ─┘
```

Implementation: `crates/core-domain/src/overview.rs::compute_overview()`. The `alignments` slice (third argument) is required by signature but may be empty; see §5 below for the v6.10.3 alignment aggregation pipeline.

---

## 2. Global Breadth Aggregation

The layer tallies directional guidance across all decision matrices:

$$\text{breadth\_pct} = \frac{\text{long\_count} - \text{short\_count}}{\text{total}} \times 100$$

This drives `global_market_bias` (STRONG_BULLISH … MIXED), `market_breadth` (STRONG_POSITIVE … STRONG_NEGATIVE), and `market_synchronization` (HIGHLY_SYNCHRONIZED … HIGHLY_FRAGMENTED). Bands in [Overview Matrix §3](../../matrices/02-09-overview-matrix.md).

L7 aggregates **all four timeframe windows** per instance (micro / fast / slow / macro — I-2, v6.10.18); the legacy slow-tier-300s-only basis is retired. Per-window advisories feed the breadth / bias / opportunity / regime tallies; per-symbol scalars (confidence, overall risk) are the mean over the windows; categorical per-asset fields are the mode (ties resolve to the fastest window).

---

## 3. Asset Rankings

Each active Decision Matrix produces an `AssetRank`, scored to favour high-confidence, actionable assets using the canonical formula in [Overview Matrix §5](../../matrices/02-09-overview-matrix.md):

$$\text{score} = 0.5 \cdot \text{confidence\_assessment} + 50$$

Range `[50, 100]`; monotonic in `confidence_assessment`. Rankings sort descending — a leaderboard of relative strength/weakness for portfolio-level allocation.

---

## 4. Systemic Risk Score

The Overview Layer publishes the single market-wide danger index consumed by the PME safety veto:

$$\text{SystemicRisk} = 0.6 \cdot \text{high\_pct} + 0.4 \cdot \text{sync\_penalty}$$

Correlated downside elevates `sync_penalty` (0–100), because synchronized declines are systemically dangerous. It is `0` unless the global bias is in the bearish family (`BEARISH` or `STRONG_BEARISH`), then it scales with the synchronization level:

| Condition | `sync_penalty` |
|-----------|----------------|
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `HIGHLY_SYNCHRONIZED` | 100 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `SYNCHRONIZED` | 60 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `MIXED` | 30 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `FRAGMENTED` | 10 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `HIGHLY_FRAGMENTED` | 0 |
| `global_market_bias ∉ {BEARISH, STRONG_BEARISH}` | 0 |

The resulting `risk_environment` label (`LOW_RISK` / `MODERATE` / `HIGH_RISK` / `NO_DATA` — canonical derivation rule table in [Overview Matrix §2.3](../../matrices/02-09-overview-matrix.md)) gates the [Ontological Priority Veto](../portfolio-management-engine/03-04-05-pme-layer4-overview.md).

> **STRONG_BEARISH coverage (correction).** A previous version of this section used the informal phrase "unless the global bias is bearish" — this excluded `STRONG_BEARISH`. The corrected condition is member-set inclusion over `GlobalBias`'s bearish family (`BEARISH` ∪ `STRONG_BEARISH`), matching the canonical table in [Overview Matrix §4](../../matrices/02-09-overview-matrix.md).

---

## 5. Market Health Summary

`market_health` (Poor … Strong) is derived from `global_market_bias`. The `global_summary` field renders a natural-language synthesis (instance count, symbol count, global bias, breadth) for the dashboard's market-overview surface.

---

## 6. Alignment Aggregation (v6.10.3+)

The Overview Layer consumes a `&[AlignmentMatrix]` slice (one per active symbol) and synthesizes three system-wide fields. The slice is sourced by the periodic aggregation task in `crates/execution-daemon/src/main.rs` (§ L7 task, ~1368–1422) — one alignment per symbol, pushed **once from the fastest present window** (not the slow tier). For full field semantics see [Overview Matrix §3.5](../../matrices/02-09-overview-matrix.md).

| Field | Formula | Range |
|-------|---------|-------|
| `alignment_distribution` | `tally(aln.mtf_overall_label for aln in alignments)` | `map<string, u32>` (counts sum to `alignments.len()`) |
| `alignment_consensus_index` | `mean(aln.mtf_overall_score for aln in alignments)` | `[-100, 100]` (signed) |
| `multi_tf_agreement_pct` | `mean(aln.trend_agreement_pct for aln in alignments)` | `[0, 100]` |

The per-asset `AssetRank` (see [Overview Matrix §2.2](../../matrices/02-09-overview-matrix.md)) is also enriched in the same pass — each rank carries a `mtf_score` and `mtf_label` mirror of `AlignmentMatrix.mtf_overall_score` / `mtf_overall_label`. When a symbol is present in `advisories` but absent from `alignments` (cold start / transient snapshot gap), the AssetRank defaults to `(mtf_score = 0.0, mtf_label = "NO_DATA")` so downstream consumers can detect the gap explicitly rather than reading a misleading zero.

**Empty-input semantics.** When `alignments.is_empty()` but L6 inputs are populated (e.g. the engine has just booted and no instance has produced an Alignment Matrix yet), the three aggregate fields default to neutral values:

| Field | Default |
|-------|---------|
| `alignment_distribution` | `HashMap::new()` |
| `alignment_consensus_index` | `0.0` |
| `multi_tf_agreement_pct` | `0.0` |

The remaining breadth / bias / sync / risk aggregates are unaffected. The dashboard's Market Alignment card detects this state and renders an "Awaiting alignment data…" placeholder rather than a misleading neutral gauge.

**Why this is independent of breadth / sync.** The existing `breadth_pct`, `market_breadth`, and `market_synchronization` are derived from L6 `directional_guidance` (a per-symbol, per-snapshot bias tally). The new alignment aggregates are derived from L2 `mtf_overall_score` and `trend_agreement_pct` (per-symbol, per-timeframe-axis aggregates). They measure different things — bias voting vs time-horizon agreement — and the Overview Matrix exposes both lenses deliberately so the operator can disambiguate "every symbol is bullish, but their timeframes disagree" (high breadth + low multi_tf_agreement_pct) from "every symbol's timeframes agree, but on opposite directions" (low breadth + high multi_tf_agreement_pct).

---

## 7. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Aggregation only** | Never recomputes per-asset analysis. |
| **Systemic authority** | Sole producer of the Systemic Risk Score. |
| **Deterministic ranking** | Ties resolve by stable sort. |
| **Empty safety** | No active instances → neutral empty overview. |
| **Alignment independence** | Alignment aggregates (`alignment_distribution`, `alignment_consensus_index`, `multi_tf_agreement_pct`) do not influence `breadth_pct`, `market_breadth`, `market_synchronization`, or `systemic_risk_score` — the L6 and L2 lenses remain independent so a single corrupt alignment source cannot poison the systemic-risk veto. |

---

## 8. Cross-References

- [Decision Matrix](../../matrices/02-04-decision-matrix.md) — Per-asset L6 input.
- [Alignment Matrix](../../matrices/02-01-alignment-matrix.md) — Per-asset L2 input (v6.10.3+).
- [Overview Matrix](../../matrices/02-09-overview-matrix.md) — Output contract.
- [PME Layer 4 — Overview](../portfolio-management-engine/03-04-05-pme-layer4-overview.md) — Systemic Risk consumer.
