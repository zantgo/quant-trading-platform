# Analysis Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Producing Layer:** Layer 3 — Analysis Layer
**Purpose:** This document defines the physical schema, classification vocabulary, and derivation contract of the **Analysis Matrix** — the market interpretation object. It transforms multi-timeframe agreement into a complete diagnosis: categorical bias, a continuous `market_bias_score`, regime classification, and **six qualitative assessments**.

---

## 1. Conceptual Definition

The Analysis Matrix represents the transition **from observation to understanding**. It consumes the [Alignment Matrix](02-01-alignment-matrix.md) and produces a structured interpretation of the asset's technical environment.

Its two headline outputs are:

1. **Categorical bias** — a five-state directional classification (`MarketBias`).
2. **Continuous market bias score** — a normalized value in `[-1.0, +1.0]` (surfaced from the alignment overall score scaled to `[-100, 100]`), where `-1.0` is absolute bearish and `+1.0` is absolute bullish.

```
[Alignment Matrix] ──► ANALYSIS LAYER (L3) ──► [Analysis Matrix]
                          derive_analysis()        (bias + regime + 6 assessments)
```

Implemented as `AnalysisMatrix` (`crates/core-domain/src/analysis.rs`), produced by `derive_analysis()`.

---

## 2. Physical Schema

### 2.1 AnalysisMatrix Fields

| Field | Type | Description |
|-------|------|-------------|
| `symbol` | `string` | Entity under analysis. |
| `bias` | `MarketBias` | Categorical directional bias (§3.1). |
| `state_confidence` | `f64` | Interpretation confidence in `[0.0, 1.0]`. *(Renamed from `confidence` in the institutional redesign; see [02-00b-confidence-hierarchy.md](02-00b-confidence-hierarchy.md).)* |
| `market_regime` | `MarketRegime` | Structural regime (§3.2). |
| `trend_assessment` | `TrendAssessment` | Trend-quality classification (§3.3). |
| `momentum_assessment` | `MomentumAssessment` | Momentum-state classification (§3.4). |
| `structure_assessment` | `StructureAssessment` | Structural-integrity classification (§3.5). |
| `volatility_assessment` | `VolatilityAssessment` | Volatility-state classification (§3.6). |
| `volume_assessment` | `VolumeAssessment` | Participation classification (§3.7). |
| `market_quality` | `QualityLevel` | Aggregate environment quality. Categorical enum (`POOR / WEAK / AVERAGE / GOOD / EXCELLENT` — wire PascalCase: `Poor` / `Weak` / `Average` / `Good` / `Excellent`) used by Decision Matrix `MarketStance` derivation and the GUI. |
| `market_quality_score` | `f64` | Raw numeric mean of the per-dimension scores (trend, momentum, structure, volume) in `[0, 100]`. The numeric companion to `market_quality`, consumed by the Layer 6 `confluence_score` formula and other downstream numeric aggregations. When unavailable at the L3 boundary, callers must map `QualityLevel → f64` via the §3.8 numeric bands. |
| `trend_score` / `momentum_score` / `structure_score` / `volatility_score` / `volume_score` | `f64?` | **v6.12 numeric companions.** The exact 0-100 alignment dimension scores each qualitative assessment is bucketed from — the disaggregated siblings of `market_quality_score`, rendered as badges on the Analysis panel (see §3.4.1–3.7.1). L3-owned, derived from L2 during `derive_analysis`; `Some` whenever `timeframes_present ≥ 1`, `None` on the empty sentinel (omitted from the wire, §6). The label can never disagree with its score — the label IS the band the score falls into (§4.2). |
| `representative_bbwp` / `representative_adx` | `f64?` | **v6.10.21 traceability.** The exact L3 regime-input raw values (representative first-TF-wins `bbwp` / `adx`) that the `rationale` quotes. The pair-level matrix mirror is per-slot last-writer-wins, so the exporting slot's own indicator map can differ from the matrix's provenance — these fields pin the exact inputs used. |
| `market_phase` | `MarketPhase` | Wyckoff-style market-cycle phase: 4 phases (`ACCUMULATION` / `MARKUP` / `DISTRIBUTION` / `MARKDOWN`) + `UNKNOWN` empty-state sentinel (§3.9). |
| `market_interpretation` | `string` | Human-readable natural-language summary. |
| `rationale` | `string` | Explainability trace of the derivation. |
| `supporting_signals` | `string[]` | Per-TF observations agreeing with `bias`. |
| `contradicting_signals` | `string[]` | Per-TF observations opposing `bias`. |
| `timeframes_considered` | `u8` | Timeframe count inherited from alignment. |

### 2.2 The Continuous Market Bias Score

The `market_bias_score ∈ [-1.0, 1.0]` referenced throughout the platform is the Alignment Matrix's `mtf_overall_score` (range `[-100, 100]`) divided by 100. The categorical `bias` is a bucketing of this same underlying score.

---

## 3. Classification Vocabularies

### 3.1 MarketBias

| Variant | `mtf_overall_score ∈ [-100, 100]` band | Meaning |
|---------|--------------------------|---------|
| `STRONG_BULLISH` | `> 40` | Dominant bullish conviction. |
| `BULLISH` | `> 20 AND ≤ 40` | Moderate bullish lean. |
| `NEUTRAL` | `≥ -20 AND ≤ 20` | No directional edge. |
| `BEARISH` | `≥ -40 AND < -20` | Moderate bearish lean. |
| `STRONG_BEARISH` | `< -40` | Dominant bearish conviction. |

**Wire values (PascalCase):** `StrongBullish` / `Bullish` / `Neutral` / `Bearish` / `StrongBearish` (the `MarketBias` enum derives serde without `rename_all`; the SCREAMING form above is the human-facing `Display` vocabulary).

> **Half-open intervals.** The bands are pinned to half-open intervals to keep `score = 20.0`, `40.0`, etc. from double-mapping: `STRONG_BULLISH = (40, 100]`, `BULLISH = (20, 40]`, `NEUTRAL = [-20, 20]`, `BEARISH = [-40, -20)`, `STRONG_BEARISH = [-100, -40)`. The same score never maps to two bands.

> **v6.10.16 grace band (sensitivity lever).** A composite inside `(15, 20]` (or `[-20, -15)`) is upgraded from `NEUTRAL` to `BULLISH`/`BEARISH` — **never** `STRONG` — when the per-timeframe vote is directionally coherent: ≥ 3 of 4 `timeframe_alignments` decisive on the dominant side (`|overall_score| > 10`, `COMPRESSION` windows excluded), `trend_agreement_pct ≥ 75`, and `signal_cross_tf_count ≥ 3`. The vote requirement is pinned to ≥3/4 of `timeframes_present` (minimum 3) so a 2-TF warmup window can never grace. The graced read carries a `×0.9` confidence haircut because the raw math did not confirm the direction — the haircut flows into `confidence_assessment` (L6) and the L5 dimension confidences, not into the probability split (which runs off the signed confluence score). Rationale (professional-trading view): a 4:0 TF vote with 100% agreement is a market telling you to lean, not a rounding artifact to HOLD — the readiness gate (WATCH/STAND_ASIDE) still governs execution. Constants: `BIAS_GRACE_*` in `crates/core-domain/src/analysis.rs`.

> **v6.10.17 LEAN tier (sensitivity — minimal confirmation).** A composite inside `(0, 15]` (or `[-15, 0)`) with a **decisive per-timeframe vote** (≥3:1, `trend_agreement_pct ≥ 75`, `signal_cross_tf_count ≥ 3`; COMPRESSION windows excluded) is rescued to `BULLISH`/`BEARISH` — capped at the plain directional tier, never `STRONG` — with the heavier `×0.8` confidence haircut. The composite may oppose the vote only within `BIAS_LEAN_COMPOSITE_TOLERANCE` (±10): the canonical case is the user's 03:40 capture — composite 2.6 with per-TF scores −58 / −51 / −11 / +42 (a 3:1 bearish vote) — which now reads **LEAN BEAR** instead of a flat NEUTRAL + 96% HOLD. A 2:2 vote at the same composite stays genuinely flat (HOLD). The bias machinery is sign-symmetric (a mirrored bullish capture reads LEAN BULL), so longs and shorts are generated with equal possibility. A directional bias with `|market_bias_score| ≤ 0.2` (the wire FRACTION of a composite ≤ 20 — `bias_lifted()`, v6.10.18 unit fix) can only have come from the margin paths (grace / hold / LEAN) — `bias_lifted()` in `crates/core-domain/src/analysis.rs` exposes this to DecisionContext and the advisory so the risk gate never silences a lifted read.

> **v6.10.16 hysteresis (FIX-H1).** The grace state **holds** across frames: once graced, the bias stays directional while `|score|` remains above `BIAS_GRACE_HOLD_BAND_MIN` (12) and the vote survives at 2:1+ — a 19.5 → 13.8 composite drift with an intact vote no longer flips Bullish→Neutral mid-consensus. The hold is guarded by the previous frame's score being inside the grace band (a plain-threshold Bullish at score 25 is never "held"), and a vote collapse (2:2) or a drop below 12 exits immediately. This eliminates discontinuous Bullish↔Neutral classification at the margin (mandatory before TAE wiring).

### 3.2 MarketRegime

`TRENDING_BULL`, `TRENDING_BEAR`, `RANGE`, `ACCUMULATION`, `DISTRIBUTION`, `EXPANSION`, `CONTRACTION`, `TRANSITION`.

> **Enum disambiguation.** `MarketRegime.ACCUMULATION/DISTRIBUTION` and `MarketPhase.ACCUMULATION/DISTRIBUTION` (§3.9) are different enums with different derivations; context determines which is meant. `MarketRegime` is a structural-regime classifier derived from ADX, BBWP, and score direction; `MarketPhase` is a Wyckoff-style market-cycle phase derived from volume trend, price trend, and structure slope.

**Canonical decision tree** (priority 1 → 6; first match wins). Uses `score = mtf_overall_score ∈ [-100, 100]`, `adx` = the ADX indicator value on the instance's **macro timeframe** (L1 Metrics, `[0, 100]`), `bbwp` = the BBWP indicator's raw percentile output on the macro timeframe (L1 Metrics, `[0, 100]`), and `regime_one_bar_ago = prior Assessment Layer regime`.

| Priority | Condition | Regime |
|----------|-----------|--------|
| 1 | `bbwp ≥ 85` | `EXPANSION` |
| 1 | `bbwp ≤ 10` | `CONTRACTION` |
| 2 | `adx ≥ 25` AND `score > +20` | `TRENDING_BULL` |
| 2 | `adx ≥ 25` AND `score < -20` | `TRENDING_BEAR` |
| 3 | 1-bar score delta `score − previous_score > 0` AND `score ≥ 0` AND not in priority 1 | `ACCUMULATION` |
| 4 | 1-bar score delta `score − previous_score < 0` AND `score ≤ 0` AND not in priority 1 | `DISTRIBUTION` |
| 5 | `adx < 25` AND `bbwp ∈ (10, 85)` AND `previous_regime != RANGE` (previous **1** bar) | `TRANSITION` |
| 6 | default (none of the above) | `RANGE` |

The decision tree deterministically produces all 8 variants. Empty/initial state defaults to `TRANSITION` (§6).

> **Delta & shift semantics.** `ACCUMULATION` / `DISTRIBUTION` fire on the **single-bar score delta** (`previous_score.map(|prev| score - prev)` — the previous bar's `mtf_overall_score`, not a 3-bar slope), and `TRANSITION` fires when the **previous bar's** regime was not `RANGE` (a 1-bar shift, not a 3-bar window). `score` here is `mtf_overall_score ∈ [-100, 100]`, `adx` = the ADX indicator value on the instance's **macro timeframe** (L1 Metrics, `[0, 100]`), `bbwp` = the BBWP indicator's raw percentile output on the macro timeframe (L1 Metrics, `[0, 100]`), and `previous_score` / `previous_regime` = the prior Assessment Layer outputs (see [03-02-04-mme-layer3-analysis.md §3](../engines/market-monitoring-engine/03-02-04-mme-layer3-analysis.md)).

**Wire values (PascalCase):** `TrendingBull` / `TrendingBear` / `Range` / `Accumulation` / `Distribution` / `Expansion` / `Contraction` / `Transition` (the SCREAMING form above is the `Display` vocabulary).

> **Layer-specific BBWP thresholds (intentional divergence).** This L3 decision tree uses `bbwp ≤ 10` for `CONTRACTION` and `bbwp ≥ 85` for `EXPANSION`. The Layer 1 local 4-state regime in [03-02-02-mme-layer1-metrics.md §6](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) uses looser thresholds (`bbwp ≤ 15` for `COMPRESSION`, `bbwp ≥ 85` for `EXPANSION`). A value in `[10, 15]` therefore classifies as `COMPRESSION` at Layer 1 but as `RANGE` (or `TRANSITION`) at Layer 3 — both states are valid for their respective layers. This document is authoritative for the L3 classifier; Layer 1's looser threshold is documented in the layer 1 spec.

### 3.3 TrendAssessment
`WEAK`, `DEVELOPING`, `HEALTHY`, `STRONG`, `EXHAUSTED` — derived from alignment dimension 0 (trend). **No `UNKNOWN` variant.** Wire values (PascalCase): `Weak` / `Developing` / `Healthy` / `Strong` / `Exhausted`.

#### 3.3.1 Removed numeric field: `trend_stability_sharpe` (v6.11 → v6.14)

The v6.11 **Trend Stability Sharpe** (annualized EMA-50 log-return Sharpe over the trailing 300-bar window) was carried on this matrix as the Trend assessment's **statistical proof** and rendered as a badge inside the Trend qualitative card. **v6.14:** the field, the card badge, and the `qualitative_assessment` export pair were **removed** — the L1→L3 traceability-evidence exception (see [02-00 §5](02-00-matrix-field-ownership.md)) no longer includes it, keeping L3's derived state strictly `L3 ← L2`. The L1 `price_trend_sharpe` indicator remains the sole Sharpe family member on the wire (Metrics tab, per-TF — [04-02-52](../engines/market-monitoring-engine/indicators/04-02-52-price-trend-sharpe.md)). The formula (kept for history):

$$\text{Trend Stability Sharpe} = \frac{\text{mean}\left(\ln\frac{\text{EMA}_{50,t}}{\text{EMA}_{50,t-1}}\right)}{\sigma\left(\ln\frac{\text{EMA}_{50,t}}{\text{EMA}_{50,t-1}}\right)} \times \sqrt{\frac{86\,400}{\text{timeframe\_secs}} \times 365}$$

### 3.4 MomentumAssessment
`INCREASING`, `STABLE`, `WEAKENING`, `EXHAUSTED`, `REVERSING` — derived from alignment dimension 1 (momentum). **No `UNKNOWN` variant.** Wire values (PascalCase): `Increasing` / `Stable` / `Weakening` / `Exhausted` / `Reversing`. (`Exhausted` is a declared variant but is not produced by the current §4.2 banding, which emits only `Increasing` / `Stable` / `Weakening` / `Reversing`.)

### 3.5 StructureAssessment
`STRONG`, `HEALTHY`, `WEAK`, `BROKEN`, `UNKNOWN` — derived from alignment dimension 4 (structure). *(Renamed from `UNCLEAR` — the canonical empty-state sentinel for the structure/phase enums is `UNKNOWN`.)* Wire values (PascalCase): `Strong` / `Healthy` / `Weak` / `Broken` / `Unknown`.

### 3.6 VolatilityAssessment
`COMPRESSED`, `NORMAL`, `EXPANDING`, `EXTREME`, `UNSTABLE` — derived from alignment dimension 3 (volatility). **No `UNKNOWN` variant.** Wire values (PascalCase): `Compressed` / `Normal` / `Expanding` / `Extreme` / `Unstable`.

### 3.7 VolumeAssessment
`WEAK`, `NORMAL`, `STRONG`, `EXCEPTIONAL` — derived from alignment dimension 2 (volume). **No `UNKNOWN` variant.** Wire values (PascalCase): `Weak` / `Normal` / `Strong` / `Exceptional`.

#### 3.4.1–3.7.1 Supporting numeric fields: the per-assessment dimension scores (v6.12)

Each of the four assessments above (plus Trend, §3.3) carries its exact derivation input on the wire: `AnalysisMatrix.momentum_score` / `structure_score` / `volatility_score` / `volume_score` (and `trend_score`), the 0-100 alignment dimension scores the §4.2 bands bucket. They are the **disaggregated siblings of `market_quality_score`** (which is `mean(0,1,2,4)` of the same inputs) and follow its derivation model exactly — L3-owned values derived from L2, never L1 products.

The Analysis panel renders each as a high-contrast monospace badge on the qualitative card face, **v6.13:** in rounded-integer + `%` form (e.g. `77%` — the `%` makes the cross-timeframe agreement semantics explicit, unlike a bare `76.50`), tinted by coarse band heat (≥70 strong / ≥40 mid / <40 weak) and carrying a ▲/▼ delta arrow against the previous frame's score (UI-side computation over the WS stream — no backend state). Each badge carries a hover tooltip (v6.13) qualifying its meaning — "agreement across timeframes", the % share of weighted TF readings sharing the dominant direction (Structure: % of TFs sharing the same S/R label). **v6.14:** the Trend card's second `trend_stability_sharpe` badge (v6.11) was removed with the field — the cards now carry exactly one numeric badge each. The export's `qualitative_assessment` block carries raw value + verbatim display string per field (see [07-05-export-data-payload-schema.md](../ui-ux/07-05-export-data-payload-schema.md)).

> **Layer note.** The dimension scores live on the **Alignment Matrix** (L2, §2.2). Stamping them onto the Analysis Matrix is the same allowed `L3 ← L2` derivation as `market_quality_score` (see [02-00-matrix-field-ownership.md](02-00-matrix-field-ownership.md) §2.3).

> **`UNKNOWN` sentinel.** Only `StructureAssessment` and `MarketPhase` admit `UNKNOWN`; the other four assessment enums have **no** `UNKNOWN` variant (their fall-through bands are `Exhausted` / `Reversing` / `Unstable` / `Weak` respectively — see §4.2). Enum values serialize as **PascalCase** on the wire (e.g. `"TrendingBull"`, `"Healthy"`, `"Compressed"`) — the SCREAMING_SNAKE forms above are the `Display` vocabulary.

### 3.8 QualityLevel
`POOR`, `WEAK`, `AVERAGE`, `GOOD`, `EXCELLENT` — computed as the mean of the trend, momentum, structure, and volume dimension scores. Wire values (PascalCase): `Poor` / `Weak` / `Average` / `Good` / `Excellent`. Numeric bands:

| QualityLevel | `mean(0,1,2,4)` Score |
|--------------|------------------------|
| `POOR` | **< 30** |
| `WEAK` | **≥ 30 AND < 50** |
| `AVERAGE` | **≥ 50 AND < 70** |
| `GOOD` | **≥ 70 AND < 85** |
| `EXCELLENT` | **≥ 85** |

### 3.9 MarketPhase
Four phases — `ACCUMULATION`, `MARKUP`, `DISTRIBUTION`, `MARKDOWN` — plus the `UNKNOWN` empty-state sentinel (§6). Wire values (PascalCase): `Accumulation` / `Markup` / `Distribution` / `Markdown` / `Unknown`. Wyckoff-style market-cycle phase. Derived from volume trend + price trend + structure slope:

> **Enum disambiguation.** `MarketPhase.ACCUMULATION/DISTRIBUTION` and `MarketRegime.ACCUMULATION/DISTRIBUTION` (§3.2) are different enums with different derivations; context determines which is meant. `MarketPhase` is a Wyckoff-style market-cycle phase derived from volume trend, price trend, and structure slope; `MarketRegime` is a structural-regime classifier derived from ADX, BBWP, and score direction.

| Phase | Condition |
|-------|-----------|
| `ACCUMULATION` | price ranging (low volatility) + rising volume_assessment (WEAK → STRONG) + structure healthy |
| `MARKUP` | price trending up + volume STRONG/EXCEPTIONAL + bias BULLISH/STRONG_BULLISH |
| `DISTRIBUTION` | price ranging (low volatility) + falling volume_assessment (STRONG → WEAK) + structure weakening |
| `MARKDOWN` | price trending down + volume STRONG/EXCEPTIONAL + bias BEARISH/STRONG_BEARISH |

---

## 4. Derivation Contract

### 4.1 Confidence Model

```
base = |mtf_overall_score| / 100
state_confidence = base
IF trend_agreement_pct ≥ 75  → state_confidence += 0.15
IF trend_agreement_pct < 50  → state_confidence  = min(state_confidence, 0.5)
IF signal_cross_tf_count ≥ 3 → state_confidence += 0.10
IF timeframes_present ≤ 1    → state_confidence  = min(state_confidence, 0.5)
state_confidence = clamp(state_confidence, 0, 1)
```

> ⚠️ `signal_cross_tf_count` is the honest distinct cross-TF agreement count
> (`02-01-alignment-matrix.md` §4.4 — not a `0.3 × total` heuristic since
> AUDIT-H1). This `+0.10` branch fires when ≥3 distinct signal identities are
> genuinely shared across timeframes — a discriminative rule that only fires
> on real multi-TF signal agreement.

### 4.2 Assessment Thresholds

| Assessment | Source dim | Bands |
|-----------|-----------|-------|
| Trend | dim 0 | `≥90` `STRONG` · `≥75` `HEALTHY` · `≥50` `DEVELOPING` · `≥25` `WEAK` · else `EXHAUSTED` |
| Momentum | dim 1 | `≥80` `INCREASING` · `≥60` `STABLE` · `≥40` `WEAKENING` · else `REVERSING` |
| Structure | dim 4 | `≥80` `STRONG` · `≥60` `HEALTHY` · `≥40` `WEAK` · `≥20` `BROKEN` · else `UNKNOWN` |
| Volatility | dim 3 | `≥90` `EXTREME` · `≥70` `EXPANDING` · `≥40` `NORMAL` · `≥20` `COMPRESSED` · else `UNSTABLE` |
| Volume | dim 2 | `≥90` `EXCEPTIONAL` · `≥70` `STRONG` · `≥40` `NORMAL` · else `WEAK` |

> **v6.12 invariant.** Each assessment's numeric companion (§3.4.1–3.7.1) is the exact dimension score this table buckets: the emitted label and its score are always consistent by construction.

### 4.3 *(Removed in the institutional redesign)*

The opportunity-selection decision tree has been **moved to the Opportunity Matrix** ([02-08-opportunity-matrix.md §4](02-08-opportunity-matrix.md#4-setup-selection-rule)) because the `OpportunityType` field is a forecast (L4-owned) and not a state interpretation (L3-owned). See [02-00-matrix-field-ownership.md](02-00-matrix-field-ownership.md) for the canonical mapping.

### 4.4 Explainability

The `rationale` and `market_interpretation` strings are generated deterministically from the numeric derivation, satisfying the ontology's **Explainability** principle: every categorical output traces back to the alignment scores and per-timeframe evidence recorded in `supporting_signals` / `contradicting_signals`.

---

## 5. JSON Serialization Contract

```json
{
  "symbol": "BTC-USDT",
  "bias": "Bullish",
  "state_confidence": 0.65,
  "market_regime": "TrendingBull",
  "trend_assessment": "Healthy",
  "momentum_assessment": "Stable",
  "structure_assessment": "Healthy",
  "volatility_assessment": "Expanding",
  "volume_assessment": "Strong",
  "market_quality": "Good",
  "market_quality_score": 72.0,
  "trend_score": 76.5,
  "momentum_score": 83.2,
  "structure_score": 81.4,
  "volatility_score": 55.0,
  "volume_score": 78.8,
  "market_interpretation": "Bullish trending market with healthy trend, stable momentum, healthy structure, expanding volatility, and strong volume participation. Favors trend continuation.",
  "rationale": "state_confidence = |40|/100 + 0.15 (agreement 75%) + 0.10 (3 cross-TF signals) = 0.65",
  "supporting_signals": ["fast180 (bullish): score +42, TRENDING regime, 3 signals"],
  "contradicting_signals": [],
  "timeframes_considered": 4
}
```

Enum values serialize as **PascalCase** on the wire (as shown above); the SCREAMING_SNAKE form is the human-facing `Display` vocabulary.

---

## 6. Empty State

When `timeframes_present == 0`, `derive_analysis` returns `AnalysisMatrix::empty()`. All defaults (empty-state values serialize as their PascalCase wire forms — `Weak`, `Stable`, `Unknown`, `Normal`):

| Field | Empty-state value |
|-------|-------------------|
| `bias` | `NEUTRAL` |
| `state_confidence` | `0.0` |
| `market_regime` | `TRANSITION` |
| `market_quality` | `POOR` |
| `market_quality_score` | `0.0` |
| `trend_score` / `momentum_score` / `structure_score` / `volatility_score` / `volume_score` | absent (`Option::None` omitted from the wire) |
| `market_phase` | `UNKNOWN` |
| `trend_assessment` | `WEAK` |
| `momentum_assessment` | `STABLE` |
| `structure_assessment` | `UNKNOWN` |
| `volatility_assessment` | `NORMAL` |
| `volume_assessment` | `NORMAL` |
| `market_interpretation` | `"No data available — no candles have been completed."` |
| `supporting_signals` | `[]` |
| `contradicting_signals` | `[]` |
| `timeframes_considered` | `0` |

---

## 7. Cross-References

- [Alignment Matrix](02-01-alignment-matrix.md) — Sole input.
- [Opportunity Matrix](02-08-opportunity-matrix.md) — Consumes Analysis Matrix state (`bias`, `market_quality`, `state_confidence`, qualitative assessments) and is the canonical producer of `OpportunityType` (formerly `opportunity_analysis`).
- [Risk Matrix](02-11-risk-matrix.md) — Consumes the full Analysis Matrix.
- [Decision Matrix](02-04-decision-matrix.md) — Downstream synthesis.
- [MME Layer 3 — Analysis](../engines/market-monitoring-engine/03-02-04-mme-layer3-analysis.md) — Producing-layer specification.
