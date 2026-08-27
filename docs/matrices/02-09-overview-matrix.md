# Overview Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Producing Layer:** Layer 7 — Overview Layer
**Purpose:** This document defines the physical schema of the **Overview Matrix** — the global market-synthesis object. It aggregates every symbol's Decision Matrix plus per-symbol Alignment Matrices plus instance metadata into cross-market breadth indices, cross-timeframe alignment aggregates, asset rankings, synchronization measures, and a single Systemic Risk Score.

---

## 1. Conceptual Definition

Per the [Ontology](../conceptual-foundations/01-01-ontology.md) §3.17, **Market Overview** represents the aggregated state of the entire monitored universe. Where all prior matrices describe a *single asset*, the Overview Matrix describes the *collective market*.

```
[Decision Matrix: BTC-USDT]─┐
[Decision Matrix: ETH-USDT]─┼──► OVERVIEW LAYER (L7) ──► [Overview Matrix]
[Decision Matrix: SOL-USDT]─┘   compute_overview()      (global synthesis)
        + [Instance Metadata]
        + [Alignment Matrix per symbol]    (v6.10.3+ — cross-TF aggregate)
```

L7 aggregates **all four timeframe windows** per symbol (micro/fast/slow/macro — see the I-2 note below); per-window advisories feed the breadth/bias/opportunity/regime tallies, per-symbol scalars are the mean over the windows, and categorical per-asset fields are the mode (ties resolve to the fastest window). The Alignment Matrix inputs (v6.10.3+) are likewise sourced from each instance's `MarketSnapshot.alignment` and aggregated across all symbols (see §3.5 below). The legacy slow-tier-300s-only basis is retired.

Implemented as `OverviewMatrix` (`crates/core-domain/src/overview.rs`), produced by `compute_overview()`.

---

## 2. Physical Schema

### 2.1 OverviewMatrix Fields

| Field | Type | Description |
|-------|------|-------------|
| `global_market_bias` | `GlobalBias` | Universe-wide directional bias (§3.1). |
| `market_breadth` | `MarketBreadth` | Breadth classification (§3.2). |
| `low_coverage` | `bool` | `true` when `active_symbols.len() < 3` (symbol-count based — the breadth/sync/health aggregates are statistically unreliable on 1–2 symbols); default `false` (§3.2). |
| `breadth_pct` | `f64 ∈ [-100, 100]` | Continuous numeric breadth: signed percentage of bullish-asset count. Source of the UI's −100 % to +100 % breadth gauge and the input to `market_breadth` and `market_synchronization`. |
| `regime_distribution` | `map<string, f64>` | Fraction of assets per regime. (`f64` because the regime-classification partition is exhaustive — entries sum to `1.0`.) |
| `opportunity_distribution` | `map<string, u32>` | Count of assets per opportunity type (incl. `LiquiditySqueeze` and `Scalp` since the v2.1 completeness sweep — see [02-08-opportunity-matrix.md §3](../matrices/02-08-opportunity-matrix.md)). (`u32` because opportunity types are not mutually exclusive — a single asset can simultaneously satisfy the preconditions of multiple setups, so the map is a per-type count rather than a partition.) |
| `risk_distribution` | `RiskDistribution` | Low/moderate/high risk share + environment label (§4). |
| `cascade_risk_index` | `RiskDimension` | Cross-symbol aggregate of L5 `cascade_risk` (Phase 3). |
| `systemic_risk_score` | `f64` | `0.6 × high_pct + 0.4 × sync_penalty` — the `high_pct` term uses the **TF-decayed** high-share (micro 0.1 / fast 0.2 / slow 0.3 / macro 0.4, P7 §3 note below), and `sync_penalty` is nonzero only under a bearish `global_market_bias` (graded table §3.4). The market-wide danger index the PME veto loop consumes (`≥` the operator-configured `systemic_risk_threshold`, default `80`, triggers the systemic-risk veto path per [03-04-05-pme-layer4-overview.md §4.1](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md)). |
| `asset_ranking` | `AssetRank[]` | Assets ranked by composite score (§5). |
| `market_synchronization` | `SyncLevel` | Cross-asset correlation of direction (§3.3). |
| `market_health` | `HealthLevel` | Overall market health (§3.4). |
| `alignment_distribution` | `map<string, u32>` (v6.10.3+) | Count of assets per `AlignmentMatrix.mtf_overall_label` — `STRONG_BULL_MTF`, `WEAK_BULL_MTF`, `NEUTRAL_MTF`, `WEAK_BEAR_MTF`, `STRONG_BEAR_MTF`, `NO_DATA`. `u32` because an asset can satisfy at most one label (§3.5). |
| `alignment_consensus_index` | `f64 ∈ [-100, 100]` (v6.10.3+) | Mean of all per-symbol `AlignmentMatrix.mtf_overall_score` — the cross-timeframe counterpart to `breadth_pct` (§3.5). |
| `multi_tf_agreement_pct` | `f64 ∈ [0, 100]` (v6.10.3+) | Mean of all per-symbol `AlignmentMatrix.trend_agreement_pct` — "how well do timeframes within each symbol agree?" (§3.5). Distinct from `market_synchronization`, which is cross-symbol and derived from `breadth_pct`. |
| `global_summary` | `string` | Natural-language synthesis. |
| `instance_count` | `u32` | Active monitoring instances. |
| `active_symbols` | `string[]` | Sorted list of active symbols. |

**Invariant (code truth).** `instance_count` counts active `InstanceMeta` records (`is_active`), while `active_symbols` is the sorted union of symbols from active instances **and** all advisory windows (v6.10.18 I-2: each symbol contributes 0–4 TF-window advisories). In the current single-instance-per-symbol deployment the two coincide (`instance_count == active_symbols.length`), but the code does not enforce equality — `global_summary` phrases it as "N active instances across M symbols". Multi-instance mode (multiple `MarketSnapshot` per symbol) is not currently supported.

### 2.2 AssetRank

| Field | Type | Description |
|-------|------|-------------|
| `symbol` | `string` | Asset. |
| `score` | `f64` | Composite ranking score. |
| `bias` | `string` | Directional guidance label — Rust `Debug`-format value of `DirectionalGuidance` (`"StrongLong"`, `"Neutral"`, `"AvoidDirectionalExposure"`, …). |
| `confidence` | `f64` | Decision Matrix `confidence_assessment` value (mirror, in `[0, 100]`). |
| `regime` | `string` | Strategy environment label — Rust `Debug`-format value of `StrategyEnvironment` (`"TrendFollowing"`, `"MeanReversion"`, …). |
| `risk_level` | `string` | Risk band — per-asset L5 `overall_risk.score` (v6.10.16 FIX-O3): `≤ 30` LOW, `≥ 70` HIGH, else MODERATE (Display vocabulary, SCREAMING). |
| `mtf_score` | `f64 ∈ [-100, 100]` (v6.10.3+) | `AlignmentMatrix.mtf_overall_score` for this symbol. `0.0` when no alignment is available for the symbol. |
| `mtf_label` | `string` (v6.10.3+) | `AlignmentMatrix.mtf_overall_label` for this symbol — `STRONG_BULL_MTF` / `WEAK_BULL_MTF` / `NEUTRAL_MTF` / `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`. |

### 2.3 RiskDistribution

| Field | Type | Description |
|-------|------|-------------|
| `low_pct` | `f64` | % of assets at low risk. |
| `moderate_pct` | `f64` | % at moderate risk. |
| `high_pct` | `f64` | % at high risk. |
| `risk_environment` | `string` | `LOW_RISK` / `MODERATE` / `HIGH_RISK` / `NO_DATA`. |

`risk_environment` is derived from the **mean** per-asset L5 overall risk (ordered, first match wins):

| Priority | Condition | `risk_environment` |
|----------|-----------|--------------------|
| 1 | `instance_count = 0` | `NO_DATA` |
| 2 | mean overall risk `≥ 50` | `HIGH_RISK` |
| 3 | mean overall risk `≥ 25` | `MODERATE` |
| 4 | otherwise | `LOW_RISK` |

> **v6.10.16 (FIX-O3).** `risk_environment` previously binned on `high_pct` alone — an environment where 100% of assets sat in "moderate" (mean ≈ 37) was labelled `LOW_RISK`, contradicting the L5 panels and the dashboard card. Binning the mean makes "100% moderate" read `MODERATE`. The `AssetRank.risk_level` field (§2.2) likewise bins per-asset L5 `overall_risk.score` (`≤ 30` LOW / `≥ 70` HIGH / else MODERATE) — it previously binned on `confidence_assessment`, a confidence value mislabeled as risk (75% confidence with 50 overall risk read `LOW`).

---

## 3. Classification Vocabularies

> **Systemic risk (P7, v6.10.19).** The `systemic_risk_score` and its
> `high_pct` term use the **TF-decayed** high-share (micro 0.1 / fast 0.2 /
> slow 0.3 / macro 0.4 weights) — a transient micro-tier risk spike
> contributes at most 10% to the PME safety veto; the safety math stays
> anchored to macro stability. The descriptive `risk_distribution` /
> `risk_environment` / health keep the plain TF-mean (screen-to-panel
> parity).

> **L7 aggregation basis (v6.10.18 I-2).** The Overview aggregates ALL FOUR
> timeframe windows per symbol (micro/fast/slow/macro; the 300s-slow-only
> basis made the headline contradict every panel — e.g. HIGH_RISK next to
> an avg-risk of 41). Per-window advisories feed the breadth/bias/
> opportunity/regime tallies; per-symbol scalars (confidence, overall
> risk) are the MEAN over the windows; categorical per-asset fields are
> the MODE (ties resolve to the fastest window). Under `low_coverage`
> (≤2 active symbols) the frontend demotes STRONG_* display tokens one
> tier and appends the pair count — "BULLISH (1 pair)", never
> "STRONG BULLISH 100% breadth" from a single symbol (I-10).

### 3.1 GlobalBias
`STRONG_BULLISH`, `BULLISH`, `NEUTRAL`, `BEARISH`, `STRONG_BEARISH`, `MIXED`.

```
priority 1: long_count/total ≥ 0.8  AND market_synchronization ∈ { HIGHLY_SYNCHRONIZED, SYNCHRONIZED } → STRONG_BULLISH
priority 1: short_count/total ≥ 0.8 AND market_synchronization ∈ { HIGHLY_SYNCHRONIZED, SYNCHRONIZED } → STRONG_BEARISH
priority 2: long_count/total  ≥ 0.6                                → BULLISH
priority 2: short_count/total ≥ 0.6                                → BEARISH
priority 3: long_count > short_count                              → BULLISH
priority 3: short_count > long_count                              → BEARISH
priority 4: else                                                 → MIXED
```

All six variants are reachable from this rule. **v6.10.17 (P2):** the legacy `neutral_count/total ≥ 0.6 → NEUTRAL` priority was stale — the code (`overview.rs::aggregate`) resolves tie-broken direction by count and falls through to `MIXED`; a split market is `MIXED`, never `NEUTRAL` (a global NEUTRAL would misrepresent a genuine directional disagreement as no information).

### 3.2 MarketBreadth
`VERY_WEAK`, `WEAK`, `BALANCED`, `POSITIVE`, `STRONG_POSITIVE`, `NEGATIVE`, `STRONG_NEGATIVE`.

Breadth percentage: $\text{breadth\_pct} = \frac{\text{long\_count} - \text{short\_count}}{\text{total}} \times 100$.

```
# Priority order (first match wins):
1. breadth_pct >  60        → STRONG_POSITIVE
2. breadth_pct >  20        → POSITIVE
3. breadth_pct < -60        → STRONG_NEGATIVE
4. breadth_pct < -20        → NEGATIVE
5. |breadth_pct| < 10       → BALANCED
6. breadth_pct > 0         → WEAK
7. otherwise                → VERY_WEAK
```

> **`VERY_WEAK` semantics.** `VERY_WEAK` denotes marginally negative breadth — weaker than `BALANCED`, not yet a confirmed negative (`NEGATIVE` requires `breadth_pct < −20`).

> **Low-coverage flag (`low_coverage`).** The Overview Matrix carries an additive `low_coverage: bool` field (default `false`) alongside `market_breadth`: it is `true` when **`active_symbols.len() < 3`** — a **symbol-count** threshold (`compute_overview` sets `low_coverage: active_symbols.len() < 3`), not a signal-kind count. Under low coverage the **frontend** demotes the `STRONG_*` display tokens one tier and appends the pair count (e.g. "BULLISH (1 pair)", never "STRONG BULLISH 100% breadth" from a single symbol — I-10). In the empty state (§7) `low_coverage` is `false`.

### 3.3 SyncLevel (Market Synchronization)
`HIGHLY_SYNCHRONIZED`, `SYNCHRONIZED`, `MIXED`, `FRAGMENTED`, `HIGHLY_FRAGMENTED` — from `|breadth_pct|`:
```
# Priority order (first match wins):
1. |breadth_pct| > 75  → HIGHLY_SYNCHRONIZED
2. |breadth_pct| > 50  → SYNCHRONIZED
3. |breadth_pct| > 25  → MIXED
4. |breadth_pct| > 10  → FRAGMENTED
5. otherwise           → HIGHLY_FRAGMENTED
```

### 3.4 HealthLevel
`POOR`, `WEAK`, `NEUTRAL`, `HEALTHY`, `STRONG` — derived from `global_market_bias` × `risk_environment` (ordered, first match wins):

| Priority | Condition | `market_health` |
|----------|-----------|-----------------|
| 1 | `risk_environment = HIGH_RISK` | `POOR` |
| 2 | `global_market_bias ∈ {STRONG_BULLISH, STRONG_BEARISH}` | `STRONG` |
| 3 | `global_market_bias ∈ {BULLISH, BEARISH}` | `HEALTHY` |
| 4 | `global_market_bias = NEUTRAL` | `NEUTRAL` |
| 5 | `global_market_bias = MIXED` | `WEAK` |

---

## 3.5 Alignment Aggregation (v6.10.3+)

Three fields synthesize cross-timeframe alignment data from every symbol's [Alignment Matrix](./02-01-alignment-matrix.md) into a single system-wide view. They are computed by `compute_overview()` from the per-symbol `AlignmentMatrix` slice and are **independent** of the L6-derived breadth / bias / sync fields above — alignment is a higher-order, per-TF lens that complements (rather than replaces) the per-symbol bias tally.

### 3.5.1 `alignment_distribution` — count per `mtf_overall_label`

```python
for aln in alignments:
    alignment_distribution[aln.mtf_overall_label] += 1
```

An asset satisfies exactly one of the 6-state vocabulary — see [Alignment Matrix §3.1](./02-01-alignment-matrix.md) — so the map is a count, not a fraction (entries do not sum to 1.0). When `alignments` is empty, the map is empty. UI consumers typically render this as a stacked horizontal bar where each segment's width is proportional to `count / total_symbols`.

### 3.5.2 `alignment_consensus_index` — mean of `mtf_overall_score`

```
alignment_consensus_index = mean(aln.mtf_overall_score for aln in alignments) ∈ [-100, 100]
```

This is the cross-timeframe counterpart to `breadth_pct` (which is cross-symbol). Whereas `breadth_pct` answers "what fraction of symbols are bullish vs bearish?", `alignment_consensus_index` answers "across the timeframes of every symbol, what is the net directional bias?". When `alignments` is empty, the value is `0.0`.

UI consumers typically render this as a signed horizontal gauge with a centerline at `0`. The same color convention as `breadth_pct` applies: green for `> +20`, red for `< -20`, amber in the `[-20, +20]` neutral band.

### 3.5.3 `multi_tf_agreement_pct` — mean of `trend_agreement_pct`

```
multi_tf_agreement_pct = mean(aln.trend_agreement_pct for aln in alignments) ∈ [0, 100]
```

Answers "how well do the timeframes within each symbol agree on direction?". This is distinct from `market_synchronization` (which is **cross-symbol** and derived from `breadth_pct`): two markets with 100% agreement within each symbol can have very different `market_synchronization` values, and vice versa.

UI consumers typically render this as a large numeric with a 3-bucket classifier:

| Range | Label |
|-------|-------|
| `≥ 75` | `STRONG` — "Strong consensus" |
| `[50, 75)` | `PARTIAL` — "Partial consensus" |
| `< 50` | `CONFLICTED` — "Conflicted" |

### 3.5.4 Empty / partial-input semantics

When no instance has yet produced an Alignment Matrix (`alignments.is_empty()`) but the L6 inputs (advisories + active instances) are populated:

- `alignment_distribution` → empty map.
- `alignment_consensus_index` → `0.0` (not NaN).
- `multi_tf_agreement_pct` → `0.0` (not NaN).

This is the dashboard's "Awaiting alignment data…" state — the Market Alignment card detects this and renders a single muted placeholder rather than a misleading neutral gauge. The remaining breadth / bias / sync / risk aggregates are unaffected; they continue to be computed from L6 advisories.

When an alignment is missing for a specific symbol but other symbols have alignments (e.g. cold-start with a single symbol lagging), that symbol's `AssetRank.mtf_score` defaults to `0.0` and `AssetRank.mtf_label` defaults to `"NO_DATA"` — see §2.2.

---

## 4. Risk Distribution & Systemic Risk Score

The `risk_distribution` bins assets by their L5 **`overall_risk.score`** (v6.10.13 — the canonical aggregate the dashboard's RiskDistributionCard uses):

> **Overall-risk source (v6.10.13, L7-A).** The distribution, `risk_environment`, and `systemic_risk_score`'s `high_pct` term bin per-asset L5 `overall_risk.score` carried on the active instances (`InstanceMeta.overall_risk`; missing symbols default to 50/moderate). A previous revision binned on `advisory.confidence_assessment` — a confidence value, not a risk measure (high confidence ⇒ low risk is the inverse relationship and broke on low-confidence quiet markets), and an interim implementation used `cascade_risk_score` alone (chosen only because the producer signature carried it), making the L7 export disagree with the dashboard card for the same labelled split. Both are superseded: confidence ≠ risk, and cascade risk is a single dimension of the L5 aggregate.

```
low_pct  = % of assets with overall_risk.score ≤ 30
high_pct = % of assets with overall_risk.score ≥ 70
moderate_pct = 100 − low_pct − high_pct
```

> **Sys Risk vs AVG RISK (v6.10.13, L7-C).** The dashboard's "Sys Risk" chip is `systemic_risk_score` (`0.6·high_pct + 0.4·sync_penalty`) — a market-wide danger index that adds the bearish-sync penalty on top of the overall-risk high share — while "AVG RISK" is the plain mean of the per-pair `overall_risk.score`. They are two different aggregates by design: Sys Risk is the correlated-downside-adjusted index for the PME safety veto; AVG RISK is the unadjusted mean. `cascade_risk_index` remains the explicit cascade-only aggregate (its `confidence` is the 0-100 coverage fraction — fixed ×100 in v6.10.13).

The **Systemic Risk Score** is the market-wide danger index published for the Portfolio Management Engine's safety veto. It is derived from the risk distribution and synchronization:

$$\text{SystemicRisk} = 0.6 \cdot \text{high\_pct} + 0.4 \cdot \text{sync\_penalty}$$

`sync_penalty` (0–100) captures the danger of **correlated downside** — synchronized declines are systemically dangerous. It is `0` unless the global bias is in the bearish family, then it scales with the synchronization level:

| Condition | `sync_penalty` |
|-----------|----------------|
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `HIGHLY_SYNCHRONIZED` | 100 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `SYNCHRONIZED` | 60 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `MIXED` | 30 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `FRAGMENTED` | 10 |
| `global_market_bias ∈ {BEARISH, STRONG_BEARISH}` + `HIGHLY_FRAGMENTED` | 0 |
| `global_market_bias ∉ {BEARISH, STRONG_BEARISH}` (any bullish / neutral / mixed state) | 0 |

> **STRONG_BEARISH coverage (correction).** `GlobalBias` is a 6-state enum that includes both `BEARISH` and `STRONG_BEARISH` as separate bearish-class members (see §3.1). A previous version of this table used `global_market_bias != BEARISH`, which silently excluded `STRONG_BEARISH` — the regime with the worst correlated downside would have bypassed the safety penalty entirely. The corrected condition is `∈ {BEARISH, STRONG_BEARISH}` (member-set inclusion).

The resulting `risk_environment` label gates the PME [Ontological Priority Veto](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md).

#### 4.0.1 Cross-Reference

The Systemic Risk Score is consumed by the PME in [03-04-05-pme-layer4-overview.md §5](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) and is read by the operator-configurable threshold gate in [08-02-pre-trade-risk-controls.md Gate 7](../operations-and-compliance/08-02-pre-trade-risk-controls.md) (`systemic_risk_threshold`, default `≥ 80`). Systemic risk does not alter PME `safety_state`: a breach is enforced by Gate 7 (blocks new entries) and the PME veto loop (AVOID + Hard Exit).

---

## 5. Asset Ranking

Each active Decision Matrix produces an `AssetRank`. The composite score is monotonically increasing with `confidence_assessment ∈ [0, 100]` and maps to a sortable `[50, 100]` range so even no-confidence assets remain orderable:

$$\text{score} = 0.5 \cdot \text{confidence\_assessment} + 50$$

Properties:
- `confidence_assessment = 0` ⇒ `score = 50` (worst, neutral baseline).
- `confidence_assessment = 100` ⇒ `score = 100` (best).
- `confidence_assessment = 75` ⇒ `score = 87.5` (matches §6).

Rankings sort descending, producing a leaderboard of relative strength/weakness for portfolio-level allocation. The formula input is `confidence_assessment` from the Decision Matrix (§4), normalized to `[0, 100]`.

---

## 6. JSON Serialization Contract

```json
{
  "global_market_bias": "BULLISH",
  "market_breadth": "POSITIVE",
  "low_coverage": false,
  "breadth_pct": 60.0,
  "regime_distribution": { "TRENDING": 0.6, "RANGE": 0.4 },
  "opportunity_distribution": { "Breakout": 2, "TrendContinuation": 1 },
  "risk_distribution": { "low_pct": 60.0, "moderate_pct": 40.0, "high_pct": 0.0, "risk_environment": "LOW_RISK" },
  "systemic_risk_score": 0.0,
  "asset_ranking": [
    { "symbol": "BTC-USDT", "score": 87.5, "bias": "StrongLong", "confidence": 75.0, "regime": "TrendFollowing", "risk_level": "MODERATE", "mtf_score": 65.0, "mtf_label": "STRONG_BULL_MTF" }
  ],
  "market_synchronization": "SYNCHRONIZED",
  "market_health": "HEALTHY",
  "alignment_distribution": { "STRONG_BULL_MTF": 2, "WEAK_BULL_MTF": 1, "NEUTRAL_MTF": 2 },
  "alignment_consensus_index": 35.5,
  "multi_tf_agreement_pct": 72.0,
  "global_summary": "5 active instances across 5 symbols. Global bias: BULLISH with positive market breadth. Risk environment: LOW_RISK.",
  "instance_count": 5,
  "active_symbols": ["BTC-USDT", "ETH-USDT", "SOL-USDT", "AVAX-USDT", "MATIC-USDT"]
}
```

> The `global_summary` sentence always appends the `Risk environment: {env}.` clause (see §2.1). `cascade_risk_index` and the per-symbol `asset_ranking` entries are emitted alongside the fields above.

> **Worked example for `systemic_risk_score`.** With `global_market_bias = BULLISH`, `sync_penalty = 0` regardless of synchronization level. The score reduces to `0.6 × high_pct + 0.4 × 0 = 0.6 × high_pct`. The example above (3 low-risk + 2 moderate-risk = 0 high-risk out of 5) yields `high_pct = 0` and `systemic_risk_score = 0.0`. A biased-bearish example with `high_pct = 60.0` and `sync_penalty = 0` (e.g. `HIGHLY_FRAGMENTED`) would yield `systemic_risk_score = 36.0`.

**Wire casing.** `GlobalBias` / `MarketBreadth` / `SyncLevel` / `HealthLevel` serialize SCREAMING_SNAKE_CASE (`"BULLISH"`, `"POSITIVE"`, `"SYNCHRONIZED"`, `"HEALTHY"`). The `AssetRank.bias` / `regime` strings are **Rust `Debug`-format values** of `DirectionalGuidance` / `StrategyEnvironment` — `"StrongLong"`, `"TrendFollowing"`, `"AvoidDirectionalExposure"`, … (PascalCase, NOT the enum SCREAMING form). `regime_distribution` keys are the L7 custom keys (`TRENDING`, `EXPANSION`, `RANGE`, `HIGH_VOLATILITY`, `LOW_ACTIVITY`, `UNFAVORABLE` — `TrendFollowing → TRENDING`, `Breakout → EXPANSION`, `MeanReversion → RANGE`); `opportunity_distribution` keys are the `Debug`-format PascalCase opportunity names (`"TrendContinuation"`, `"Breakout"`, …).

---

## 7. Empty State

When there are no Decision Matrices and no active instances, `compute_overview` returns `OverviewMatrix::empty()`: `global_market_bias = Neutral`, `market_breadth = Balanced`, `market_synchronization = HighlyFragmented`, `instance_count = 0`, summary `"No active instances — no market data available."`.

---

## 8. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Aggregation only** | The Overview Matrix never recomputes per-asset analysis; it aggregates published Decision matrices. |
| **Systemic focus** | Its Systemic Risk Score is the sole market-wide danger signal consumed by the PME veto loop. |
| **Deterministic ranking** | Ties in `AssetRank` resolve by stable sort. |

---

## 9. Cross-References

- [Decision Matrix](02-04-decision-matrix.md) — Per-asset input.
- [MME Layer 7 — Overview](../engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md) — Producing-layer specification.
- [PME Layer 4 — Portfolio](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) — Consumes the Systemic Risk Score for the veto.
- [Ontology — Market Overview](../conceptual-foundations/01-01-ontology.md) — Conceptual definition.
