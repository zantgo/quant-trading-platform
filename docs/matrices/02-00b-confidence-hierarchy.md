# Confidence Field Hierarchy

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Canonical reference for the platform's confidence-field pipeline. Documents the rename of `confidence` → `state_confidence` / `forecast_confidence` / `score_confidence` and the hierarchical flow from indicator-level up to the user-facing risk-attenuated assessment.

---

## 1. The Five Confidence Fields

| Field | Matrix | Producer | Range | Meaning |
|---|---|---|---|---|
| **Indicator confidence** | (per-indicator, inside `IndicatorEvaluation`) | L1 (per-indicator calculator) | `[0.0, 1.0]` | Conviction in the indicator's current normalized reading. Base = `\|normalized\|`, boosted by confirmed signals. |
| **`state_confidence`** | Analysis Matrix | L3 (Analysis Layer) | `[0.0, 1.0]` | Confidence in the **state interpretation** — bias, regime, market_quality. Driven by MTF agreement and cross-TF signal confluence. |
| **`forecast_confidence`** | Opportunity Matrix | L4 (Opportunity Layer) | `[0.0, 1.0]` | Confidence in the **forecast / setup identification**. Derived from the opportunity score. |
| **`score_confidence`** | Decision Matrix (`DecisionContext`) | L6 (Decision Layer) | `[0.0, 1.0]` | Confidence in the **quantitative confluence score**. `\|score\| / 100`. |
| **`confidence_assessment`** | Decision Matrix (`AdvisoryMatrix`) | L6 (Decision Layer) | `[0, 100]` | **Risk-attenuated** user-facing confidence. The terminal synthesis: `clamp(state_confidence × (1 − overall_risk/100) × 100, 0, 100)`. |

---

## 2. The Flow

```
indicator confidence (per-indicator, in IndicatorEvaluation)
        │
        │ aggregated by Analysis Layer (mean × MTF weights × agreement boosts)
        ▼
Analysis.state_confidence           ∈ [0, 1]
        │
        ├──► consumed by Opportunity Layer → forecast_confidence
        │                                  (per-setup-type score → forecast_confidence)
        ▼                                   ▼
   DecisionContext.score_confidence  Opportunity.forecast_confidence
        │                                   │
        │                                   │
        └──────────────► L6 synthesis ◄────┘
                                │
                                ▼
            AdvisoryMatrix.confidence_assessment
            = clamp(state_confidence × (1 − overall_risk / 100) × 100, 0, 100)
```

---

## 3. Renames Applied (Institutional Redesign — Option α)

For unambiguous layer identification, three of the four `confidence` fields have been renamed:

| Old name | New name |
|---|---|
| `Analysis.confidence` | **`Analysis.state_confidence`** |
| `Opportunity.confidence` | **`Opportunity.forecast_confidence`** |
| `DecisionContext.confidence` | **`DecisionContext.score_confidence`** |
| `AdvisoryMatrix.confidence_assessment` | (unchanged — already named) |

**No backwards-compat aliases** — the JSON keys change outright. Any consumer must update to the new key.

> **Code-truth note (`Analysis.confidence`).** The `AnalysisMatrix` struct still serializes a `confidence` field alongside `state_confidence` with the identical value (`analysis.rs` — both fields assigned at synthesis; neither is `skip_serializing_if`). This is a UI-facing duplicate retained from the pre-redesign wire so older dashboard revisions do not break; it is **not** a pipeline-level confidence field and carries no independent semantics. New consumers must read `state_confidence`; removal of the duplicate is a wire-breaking cleanup tracked in `docs/ROADMAP.md`.

---

## 4. Why Five Confidence Fields?

In a real institutional quant system, "confidence" means different things at different layers:

1. **Indicator confidence** (per-indicator, in L1) — *"how reliable is this single reading?"*
2. **State confidence** (L3) — *"how confident am I in my interpretation of what the market is doing?"*
3. **Forecast confidence** (L4) — *"how confident am I that this setup will work?"*
4. **Score confidence** (L6 context) — *"how confident am I in the quantitative confluence?"*
5. **Risk-attenuated confidence** (L6 advisory) — *"how confident am I that an operator should act?"*

Conflating these (e.g. the old `Analysis.confidence` vs `DecisionContext.confidence` vs `AdvisoryMatrix.confidence_assessment`) made policy evaluation ambiguous. The renames disambiguate.

> **Title-history note.** The file is titled *"The Four Confidence Fields"* in older revisions; the canonical count is **five** (indicator + state + forecast + score + risk-attenuated). The other four columns above (1–4) flow forward as the **pipeline-level** hierarchy; indicator confidence is the per-component origin that gets aggregated into state_confidence (see §5).

---

## 5. Per-Component vs Pipeline-Level Confidence

The four-level hierarchy above is the **pipeline-level** confidence flow — the values that flow forward through the layers. There are also several **per-component** confidence fields that are not part of this hierarchy but use the same word "confidence" because they measure the reliability of a specific data point:

| Field | Layer | Per-Component Meaning | NOT in Pipeline Hierarchy |
|---|---|---|---|
| `IndicatorEvaluation.confidence` | L1 | Per-indicator conviction in [0, 1]. Base = `\|normalized\|`, boosted by confirmed signals. | Aggregated into L3 `state_confidence` via MTF weighting. |
| `ContextDimension.confidence` | L1 | Mean confidence of contributing indicators in a context dimension. | Aggregated into L3 `state_confidence`. |
| `AlignmentDimension.confidence` | L2 | Per-dimension confidence in [0, 100] for the 10 dimensions. | Used to weight dimension contributions; not passed to L3. |
| `RiskDimension.confidence` | L5 | Per-dimension confidence in [0, 100] for the **8 risk sub-dimensions** (including `cascade_risk`). | Used to weight dimension contributions; not passed to L6. |
| `AssetRank.confidence` | L7 | Mirror of `Decision.confidence_assessment` for the asset ranking. | Not a new computation; a mirror. |

These per-component confidence values are **local reliability measures** for individual data points. They do not flow forward; they modulate the contribution of their parent field. The four pipeline-level `state_confidence` / `forecast_confidence` / `score_confidence` / `confidence_assessment` fields are the only ones that flow forward and are used by the TAE Policy Layer for condition evaluation.

> **Two `[0,100]` confidence scales.** `AlignmentDimension.confidence` (per-component, `[0,100]`) and `AdvisoryMatrix.confidence_assessment` (pipeline-level terminal synthesis, `[0,100]`) share the same numeric range but measure distinct concepts: the former is a local per-dimension reliability weight (not passed to L3); the latter is the risk-attenuated user-facing terminal output. They are never interchangeable.

**Duplicate-signal deduplication (v2.1).** When the same `(kind, direction, label, parent_indicator)` triple is emitted multiple times in a single snapshot (e.g. MACD structured-push + label-based trigger, Bollinger Bands similar), the confidence aggregator counts it **once**, not per emission. This applies to all indicators that intentionally emit duplicates. The `age_bars` of the first emission is preserved. This prevents double-counting duplicate signals in the confidence aggregation pipeline.

---

## 6. Cross-References

- [02-00-matrix-field-ownership.md](02-00-matrix-field-ownership.md) — Per-field producer mapping.
- [02-02-analysis-matrix.md](02-02-analysis-matrix.md) · [02-04-decision-matrix.md](02-04-decision-matrix.md) · [02-08-opportunity-matrix.md](02-08-opportunity-matrix.md) — Per-matrix specs.
- [01-01-ontology.md](../conceptual-foundations/01-01-ontology.md) — Conceptual layer definitions.
