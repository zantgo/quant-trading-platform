# Matrix Field Ownership

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Canonical mapping of every matrix field to its producing layer. This document is the authoritative reference for which engine layer owns which JSON key.

This document was introduced as part of the institutional-grade architectural redesign (Option α). It enforces the principle that **L3 (Analysis) is pure state**, **L4 (Opportunity) is pure forecast**, **L5 (Risk) is pure danger**, and **L6 (Decision) is the only synthesis point**.

---

## 0. Wire-Casing Conventions (two families)

Serde casing on the wire is **per-enum**, split into two families. When documenting or matching wire JSON, check which family the enum belongs to:

| Family | Rule | Enums |
|--------|------|-------|
| **PascalCase** (no `#[serde(rename_all)]`) | JSON values like `"StrongBullish"`, `"Healthy"`, `"TrendingBull"` | `AlignState`, `MarketBias`, `MarketRegime`, `TrendAssessment`, `MomentumAssessment`, `StructureAssessment`, `VolatilityAssessment`, `VolumeAssessment`, `MarketPhase`, `QualityLevel`, `SetupQuality`, `OpportunityType`, `TimeHorizon`-family (see note), `RiskLevel`, `RiskState`, `SignalKind`, `SignalDirection`, `SignalStatus`, `IndicatorLifecycleState`, `DirectionalGuidance`, `MarketStance`, `OpportunityClass`, `StrategyEnvironment`, `EntryGuidance`, `ExitGuidance`, `ProtectionStrategy`, `TargetStrategy` |
| **SCREAMING_SNAKE_CASE** (has `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`) | JSON values like `"TREND_RIDING"`, `"STRONG_BULLISH"` | `DirectionFamily`, `TradeViability`, `LevelSource`, `CandlePipelineState`, `GlobalBias`, `MarketBreadth`, `SyncLevel`, `HealthLevel`, `SequenceIntegrity`, `ReconstructionMethod`, all liquidity enums (`LiquiditySignalKind`, `LiquidationSide`, `ClusterKind`, `CascadeState`, `ClusterRefreshStatus`), `TimeframeSlot` (snake_case custom) |
| **Plain string / Debug-format** | Not serde enums at all | `time_horizon` (String, SCREAMING values), `AssetRank.bias` / `regime` (Rust `Debug`-format PascalCase), `mtf_overall_label` / `mtf_label` (literal `STRONG_BULL_MTF` … strings), `regime_distribution` / `opportunity_distribution` keys (custom key sets) |

> **`TimeHorizon` note.** No `TimeHorizon` enum exists in the Rust code — `OpportunityMatrix.time_horizon` is a `String` carrying the SCREAMING values (`"SCALP"` / `"INTRADAY"` / `"SWING"` / `"POSITION"`).

---

## 1. Ownership Hierarchy

```
                  ┌─────────────────────────┐
                  │   Metrics Matrix (L1)   │
                  │   (foundation)           │
                  └────────────┬────────────┘
                               │
                               ▼
              ┌────────────────────────────────┐
              │     Alignment Matrix (L2)      │
              │  (cross-timeframe consensus)   │
              └────────────────┬───────────────┘
                               │
                               ▼
                  ┌─────────────────────────┐
                  │   Analysis Matrix (L3)   │  ← pure state interpretation
                  │   bias, regime,           │
                  │   quality, assessments    │
                  └────┬──────────────┬───────┘
                       │              │
                       │              └────► L6 (Decision Synthesis)
                       ▼
       ┌──────────────────────┐    ┌──────────────────────┐
       │  Opportunity (L4)   │    │     Risk (L5)        │  ← strictly orthogonal
       │  primary_opportunity │    │  8 unipolar danger   │
       │  opportunity_score   │    │  dimensions +        │
       │  profiles[]          │    │  overall_risk        │
       └──────────┬──────────┘    └──────────┬───────────┘
                  │                            │
                  └─────────────┬──────────────┘
                                ▼
                   ┌──────────────────────────┐
                   │   Decision Matrix (L6)   │  ← only synthesis point
                   │   directional_guidance   │
                   │   trade_readiness         │
                   │   entry_danger           │
                   │   expected_reward_risk_ratio │
                   │   stop_loss_distance_pct │
                   └──────────────────────────┘
                                 │
                                 ▼
                   ┌─────────────────────────┐
                   │   Overview Matrix (L7)  │  ← cross-symbol aggregation
                   └─────────────────────────┘
                                 │
                                 ▼
                   ┌──────────────────────────┐
                   │   Policy Matrix (TAE L1) │  ← validated execution directives
                   │   policy_id, direction   │     (transient, in-memory)
                   │   stance, risk_params    │
                   └────────────┬─────────────┘
                                │
                                ▼
                   ┌──────────────────────────┐
                   │  Execution Matrix (TAE L2)│ ← persistent order state log
                   │  order_id, status        │    (materialized as `open_orders`)
                   │  filled_size, slippage   │
                   └──────────────────────────┘
```

---

## 2. Per-Field Ownership Table

### 2.1 Metrics Matrix (L1) — `02-07-metrics-matrix.md`

Owns: foundational indicator telemetry for a single Market Instance (Symbol × Timeframe).

| Field | Producer | Notes |
|---|---|---|
| `exchange`, `symbol`, `timeframe_secs`, `timestamp` | L1 (candle ingestion) | |
| `mid_price`, `bid_price`, `ask_price`, `bid_size`, `ask_size` | L1 | Top-of-book quotes |
| `funding_rate` | L1 | From DIE FundingRate event |
| OHLC + `volume`, `average_volume` | L1 (candle aggregation) | |
| `open_interest`, `oi_delta_1h`, `prev_day_px` | L1 | From DIE OpenInterest / AssetContext |
| `indicators` (map of `IndicatorEvaluation`) | L1 (indicator calculators) | 52 indicators with normalized scores, state_labels, signals |
| `context` (`MarketContext`) | L1 (`MarketContext::synthesize()`) | Per-TF context dimensions |
| `metrics_config` | L1 (config-driven Active Set) | **Configurable Data Activation** (added v6.2 per [03-02-12-mme-configurable-activation.md](../engines/market-monitoring-engine/03-02-12-mme-configurable-activation.md)). Records the active indicator/signal set actually present in this snapshot. Omitted entirely when the active set equals the registry default. Carries `config_version` for PAE attribution. |
| `alignment`, `analysis`, `opportunity`, `risk`, `advisory`, `decision_context` | **Attached matrices** | Composite envelope — L1 owns the envelope; the attached fields are *sourced* from L2–L6 for WebSocket delivery convenience (single frame carries the full cascade). The canonical sources of these fields are their respective layer matrices, NOT the Metrics Matrix. |
| `statistical_context` (`StatisticalContext`) | L1 (statistics module) | Native Metrics Matrix field — **not** an attached matrix. Statistical-intelligence envelope (Monte Carlo + z-scores) supporting the L4 Monte Carlo components and L5 z-score gates; schema in [02-07-metrics-matrix.md §3.4](02-07-metrics-matrix.md). |

> **Composite envelope convention.** The Metrics Matrix is the **WebSocket delivery unit** — a single `MarketSnapshot` frame contains the per-TF Metrics data plus the attached higher-order matrices. This is a *delivery* pattern, not a *production* pattern. The canonical owners of `alignment`, `analysis`, `opportunity`, `risk`, `advisory`, `decision_context` remain L2, L3, L4, L5, L6, L6 respectively; `statistical_context` is **L1-native** — produced by the Metrics layer as part of the Metrics Matrix itself (schema in §3.4 there), not attached from a later layer (see [02-07-metrics-matrix.md §2.1](02-07-metrics-matrix.md)).

### 2.2 Alignment Matrix (L2) — `02-01-alignment-matrix.md`

Owns: cross-timeframe consensus scores for one symbol.

| Field | Producer | Notes |
|---|---|---|
| `symbol`, `timeframes_present` | L2 | |
| `dimensions` (`AlignmentDimension[10]`) | L2 | Ordered: Trend, Momentum, Volume, Volatility, Structure, Signal, Regime, Confidence, Liquidity, **Tradability** (renamed from "Opportunity" in the institutional redesign — the L4 Opportunity Matrix is the canonical owner of opportunity concepts; this dimension measures TFs agreeing on tradability, see [Alignment Matrix §3](../matrices/02-01-alignment-matrix.md)) |
| `mtf_trend_alignment`, `mtf_momentum_alignment`, `mtf_volume_alignment`, `mtf_volatility_alignment` | L2 | Signed consensus in [-1, 1] |
| `mtf_overall_score`, `mtf_overall_label` | L2 | |
| `timeframe_alignments` (`TfAlignmentInfo[]`) | L2 | Per-TF breakdown |
| `signal_cross_tf_count`, `trend_agreement_pct` | L2 | |

### 2.3 Analysis Matrix (L3) — `02-02-analysis-matrix.md`

Owns: pure state interpretation. **No forecast, no reward, no danger.**

| Field | Producer | Notes |
|---|---|---|
| `symbol` | L3 | |
| `bias` (`MarketBias` 5-state) | L3 | `STRONG_BULLISH / BULLISH / NEUTRAL / BEARISH / STRONG_BEARISH` (wire PascalCase: `StrongBullish` / `Bullish` / `Neutral` / `Bearish` / `StrongBearish`). |
| `state_confidence` (`f64`, [0,1]) | L3 | Renamed from `confidence` for clarity in the institutional redesign |
| `market_regime` (`MarketRegime` 8-state) | L3 | Wire PascalCase: `TrendingBull` / `TrendingBear` / `Range` / `Accumulation` / `Distribution` / `Expansion` / `Contraction` / `Transition` |
| `trend_assessment`, `momentum_assessment`, `structure_assessment`, `volatility_assessment`, `volume_assessment` | L3 | Five qualitative assessments |
| `trend_score`, `momentum_score`, `structure_score`, `volatility_score`, `volume_score` | L3 | **v6.12 numeric companions.** The exact 0-100 alignment dimension scores each assessment is bucketed from — the disaggregated siblings of `market_quality_score` (allowed `L3 ← L2` derivation, same model; see [02-02-analysis-matrix.md §3.4.1–3.7.1](02-02-analysis-matrix.md)) |
| `market_quality` (`QualityLevel` 5-state) | L3 | |
| `market_quality_score` (`f64`, [0,100]) | L3 | Numeric companion to `market_quality`; consumed by L6 `confluence_score` |
| `representative_bbwp` / `representative_adx` (`f64?`) | **L1 → L3 traceability stamp** | **v6.10.21.** The exact L3 regime-input raw values the `rationale` quotes (representative first-TF-wins map), pinned so exports can trace the derivation |
| `market_phase` (`MarketPhase` 5-state) | L3 | Wyckoff-style market-cycle phase (§3.9) |
| `market_interpretation` (string) | L3 | Natural-language summary |
| `rationale` (string) | L3 | Explainability trace |
| `supporting_signals`, `contradicting_signals` | L3 | Per-TF evidence |
| `timeframes_considered` | L3 | |

**Removed (architectural redesign):**
- ~~`opportunity_analysis`~~ — the field is **retained on the wire** for backward compatibility: `AnalysisMatrix.opportunity_analysis` (`OpportunityType`) still exists in `crates/core-domain/src/analysis.rs` and is emitted on every snapshot, mirroring the L4 selection with a coarser derivation (see [02-08-opportunity-matrix.md §8](../matrices/02-08-opportunity-matrix.md)). It is **not** the canonical source — `primary_opportunity` (L4) is. The UI reads `primary_opportunity`, never the L3 label.

### 2.4 Opportunity Matrix (L4) — `02-08-opportunity-matrix.md`

Owns: forecast / setup identification. The **canonical source** of the `OpportunityType` enum.

| Field | Producer | Notes |
|---|---|---|
| `symbol` | L4 | |
| `primary_opportunity` (`OpportunityType`) | L4 | Canonical — the only producer |
| `opportunity_score` (`f64`, [0,100]) | L4 | Weighted blend of Q_ctx + S_sig + A_mtf + F_fresh |
| `setup_quality` (`SetupQuality`) | L4 | `Prime / Strong / Moderate / Marginal / None` |
| `profiles` (`OpportunityProfile[]`) | L4 | Per-setup-type scored profiles |
| `forecast_confidence` (`f64`, [0,1]) | L4 | Renamed from `confidence` |
| `contributing_signals` | L4 | Signal labels supporting primary opportunity |
| `invalidation_note` | L4 | Condition that nullifies the opportunity |
| `entry_zone` (`PriceRange`) | L4 | Recommended entry band *(institutional redesign)* |
| `target_zone` (`PriceRange`) | L4 | Expected target band *(institutional redesign)* |
| `invalidation_level` (`Decimal`) | L4 | Structural level whose breach nullifies the thesis. Canonical across L4, Decision Matrix, and Position Matrix. *(Prior per-matrix spellings (L4/Decision and Position Matrix) unified to `invalidation_level` in v2.1; retired names recorded in `docs/CHANGELOG.md`.)* |
| `long_expected_rr_internal` (`f64`) | L4 | Per-direction R:R for a long setup. The active side is resolved by `analysis.bias`; the legacy matrix-level `expected_rr_internal` was removed in v6.9. |
| `short_expected_rr_internal` (`f64`) | L4 | Per-direction R:R for a short setup. |
| `display_score` (`f64?`) | L4 | **v6.14.** Precondition-scaled operator-facing score (`round(score × min(1, met/total))`) — additive; the raw `score` stays untouched. Single source of truth for every dashboard surface rendering a setup score. |
| `time_horizon` (`TimeHorizon`) | L4 | `SCALP` / `INTRADAY` / `SWING` / `POSITION` — all four variants are reachable from at least one `OpportunityType` (see [02-08-opportunity-matrix.md §3](../matrices/02-08-opportunity-matrix.md)). |

**Ownership rules for L4:**
- The setup-selection decision tree (formerly in `02-02-analysis-matrix.md §4.3`) is **moved** to the Opportunity Matrix and is the canonical source for `OpportunityType`.
- L4 reads from L3 (Analysis) for `bias`, `state_confidence`, `market_quality`, and the qualitative assessments.

> **Serialization convention.** `primary_opportunity` (`OpportunityType`) and `setup_quality` (`SetupQuality`) serialize **PascalCase strings on the wire** (`"TrendContinuation"`, `"LiquiditySqueeze"`, `"Prime"`, …). `time_horizon` is a plain `String` carrying the SCREAMING values (`"SWING"`, `"INTRADAY"`, …). `direction_family` serializes SCREAMING (`TREND_RIDING` / `COUNTER_TREND` / `NEUTRAL`). See the canonical note in [02-08-opportunity-matrix.md §7](../matrices/02-08-opportunity-matrix.md).

### 2.5 Risk Matrix (L5) — `02-11-risk-matrix.md`

Owns: pure environmental danger. **No reward, no opportunity, no state.**

| Field | Producer | Notes |
|---|---|---|
| `symbol` | L5 | |
| `market_risk`, `volatility_risk`, `execution_liquidity_risk`, `structure_risk`, `momentum_risk`, `signal_risk`, `execution_risk`, `cascade_risk` | L5 | **Eight** unipolar risk sub-dimensions in [0, 100]. The legacy `liquidity_risk` was renamed to `execution_liquidity_risk` (with serde alias). `cascade_risk` is the 8th sub-dimension (added in the Phase 0-4 Liquidity Intelligence extension, replacing the retired `expected_rr`/`sync_risk`). |
| `overall_risk` | L5 | Weighted aggregate of the **eight** sub-dimensions: `overall = 0.14·M + 0.14·V + 0.14·L_ex + 0.10·S + 0.14·Mo + 0.10·Sig + 0.10·E + 0.14·C` (total = 1.0; where `M`=market_risk, `V`=volatility_risk, `L_ex`=execution_liquidity_risk, `S`=structure_risk, `Mo`=momentum_risk, `Sig`=signal_risk, `E`=execution_risk, `C`=cascade_risk; see [Risk Matrix §4.9](02-11-risk-matrix.md) and [MME Layer 5 §3](../engines/market-monitoring-engine/03-02-06-mme-layer5-risk.md)). **Nine fields total** (eight sub-dimensions + `overall_risk`). |

**Removed (architectural redesign):**
- ~~`expected_rr`~~ — moved to Decision Matrix (L6) as `entry_danger`. The reward dimension is a **synthesis** concept and belongs in L6, not L5.

**Ownership rules for L5:**
- L5 reads from L3 (Analysis) for `bias`, `market_regime`, `market_quality`, and the qualitative assessments.
- L5 does **not** consume the L4 Opportunity Matrix.
- L5 does **not** compute any reward or opportunity score.

### 2.6 Decision Matrix (L6) — `02-04-decision-matrix.md`

Owns: the **only synthesis point** in the pipeline. Combines L3 (state) + L4 (opportunity) + L5 (risk) into actionable guidance.

**`AdvisoryMatrix` (user-facing guidance):**

| Field | Producer | Notes |
|---|---|---|
| `symbol` | L6 | |
| `directional_guidance` (`DirectionalGuidance` 6-state) | L6 | Derived from L3 `bias` × L5 `overall_risk` × L6 `market_stance` (priority order — see Decision Matrix §3.1) |
| `market_stance` (`MarketStance` 5-state) | L6 | Derived from L3 `market_quality` × L5 `overall_risk` (sticky AVOID/CAUTIOUS guards — see Decision Matrix §3.2) |
| `strategy_environment` (`StrategyEnvironment` 6-state) | L6 | Derived from L3 `market_regime` |
| `entry_guidance` (`EntryGuidance` 5-state) | L6 | |
| `exit_guidance` (`ExitGuidance` 5-state) | L6 | |
| `protection_strategy` (`ProtectionStrategy` 5-state) | L6 | |
| `target_strategy` (`TargetStrategy` 5-state) | L6 | |
| `confidence_assessment` (`f64`, [0,100]) | L6 | Risk-attenuated: `clamp(L3.state_confidence × (1 − L5.overall_risk/100) × 100, 0, 100)` |
| `final_recommendation` (string) | L6 | Natural-language summary |
| `stop_loss_distance_pct` (`f64`) | L6 | **Type-boundary handoff.** Raw percent float carried into TAE Position Sizing Protocol; cast to `Decimal` at the MME L6 → TAE L2 boundary. Computed by the §3.6 volatility/structure-scaled formula (base `(1.0 | 1.5) × 2.0%` + `volatility_risk.score / 10` bump, clamped `[0.5, 15]` percent — **not** ATR-derived). See `03-03-03-tae-layer2-execution.md §2` and `01-02-global-architecture.md §6.3`. |

**`DecisionContext` (quantitative metadata):**

| Field | Producer | Notes |
|---|---|---|
| `score` (`f64`) | L6 | Quantitative confluence score |
| `bias` (`MarketBias` 5-state) | L6 | Mirror of L3 `bias` (5-state per platform convention; no 3-state collapse). Wire PascalCase (`"StrongBullish"`, …) |
| `score_confidence` (`f64`, [0,1]) | L6 | Renamed from `confidence` |
| `contributing_indicators` | L6 | |
| `trade_readiness` (String) | L6 | **Populated on `DecisionContext`** — `READY / FORMING / WATCH / STAND_ASIDE` (plain wire `String` with SCREAMING values, not a serde enum); not an `AdvisoryMatrix` field |
| `entry_danger` (`RiskDimension`) | L6 | **Populated on `DecisionContext`** — renamed from `risk_favorability` in v2.1 (semantic successor of `Risk.expected_rr`). The RiskDimension convention is **high score = danger, low score = safe**; `evidence` is left empty and omitted from the wire |
| `expected_reward_risk_ratio` (`f64`) | L6 | **Populated on `DecisionContext`** — risk-discounted synthesis: `active-side R:R × (1 − L5.overall_risk / 100.0)` (canonical: [Decision Matrix §2.2](../matrices/02-04-decision-matrix.md)). Note the `/100.0` divisor — `overall_risk` is on the canonical `[0, 100]` scale |
| `long_probability` / `short_probability` / `hold_probability` / `net_bias_pct` | L6 | Normalized probability split (0–100) + net bias; canonical server-side source of truth (see [Decision Matrix §2.4](../matrices/02-04-decision-matrix.md)) |
| `lean_floor_applied` (`bool`) | L6 | **v6.10.19 (P6).** `true` when the graded-lean floors adjusted the split (HOLD cap 60% / directional floor 15%) |

### 2.7 Overview Matrix (L7) — `02-09-overview-matrix.md`

Owns: cross-symbol aggregation.

| Field | Producer | Notes |
|---|---|---|
| `global_market_bias` (`GlobalBias` 6-state) | L7 | |
| `market_breadth` (`MarketBreadth` 7-state) | L7 | |
| `breadth_pct` (`f64`, [-100, 100]) | L7 | Continuous numeric derived from the `MarketBreadth` 7-state enum. UI renders as −100 % to +100 % gauge. |
| `regime_distribution` (`map<string, f64>`) | L7 | |
| `opportunity_distribution` (`map<string, u32>`) | L7 | Aggregated from L4 `primary_opportunity` |
| `risk_distribution` (`RiskDistribution`) | L7 | |
| `cascade_risk_index` (`RiskDimension`) | L7 | **Fully computed/aggregated** — the mean of the per-symbol L5 `cascade_risk.score` values across active instances (`cascade_risk_index = RiskDimension { score: mean, level, state, confidence: coverage × 100 }` in `overview.rs`). Stable contract; consumed by the dashboard as the cascade-only aggregate (distinct from `systemic_risk_score`). |
| `asset_ranking` (`AssetRank[]`) | L7 | score = `0.5 × confidence_assessment + 50` |
| `market_synchronization` (`SyncLevel` 5-state) | L7 | |
| `market_health` (`HealthLevel` 5-state) | L7 | |
| `global_summary` (string) | L7 | |
| `instance_count`, `active_symbols` | L7 | **Coincide in single-instance-per-symbol deployments** (`instance_count == active_symbols.length`); `active_symbols` is the union of instance + advisory symbols, so equality is not code-enforced (see [02-09 §2.1](../../docs/matrices/02-09-overview-matrix.md)). |
| `systemic_risk_score` (derived) | L7 | `0.6 × high_pct + 0.4 × sync_penalty` |
| `alignment_distribution` (`map<string, u32>`) (v6.10.3+) | L7 | Count of assets per `AlignmentMatrix.mtf_overall_label`. Aggregated from L2. |
| `alignment_consensus_index` (`f64`, [-100, 100]) (v6.10.3+) | L7 | Mean of per-symbol `AlignmentMatrix.mtf_overall_score`. Cross-timeframe counterpart to `breadth_pct`. Aggregated from L2. |
| `multi_tf_agreement_pct` (`f64`, [0, 100]) (v6.10.3+) | L7 | Mean of per-symbol `AlignmentMatrix.trend_agreement_pct`. Distinct from `market_synchronization` (which is cross-symbol, L6-derived). Aggregated from L2. |

**AssetRank enrichment (v6.10.3+).** Each `AssetRank` entry additionally carries `mtf_score` (`f64`, [-100, 100]) and `mtf_label` (`string`), mirrors of `AlignmentMatrix.mtf_overall_score` / `mtf_overall_label` keyed by `symbol`. Defaults to `(0.0, "NO_DATA")` when no alignment is available for the symbol.

---

### 2.8 SetupPlan (TAE v7) — replaces the erased Policy Matrix (`02-14` deleted)

Owns: the accepted top setup produced by the v7 Setup Executor. In-memory executor state surfaced via `GET /api/instances/:id/automation`; not independently persisted (the policy matrix, `policy_id`, `stance`, and `risk_parameters` fields were erased with the policy engine).

| Field | Producer | Notes |
|---|---|---|
| `setup_type` | MME L4 (via executor) | e.g. `TrendContinuation` |
| `symbol` | MME L4 | |
| `direction` | MME L6 | `LONG` / `SHORT` |
| `entry_mid` / `sl` / `tp` | MME L4 zones | zone midpoints + invalidation level |
| `net_rr` | MME L6 | risk-discounted R:R |
| `score` / `source_tf` | MME L4 / snapshot | display score + timeframe |

### 2.9 Execution Matrix (TAE L2) — `02-15-execution-matrix.md`

Owns: persistent log of all order state transitions. Materialized as the `open_orders` SQLite table (not a JSON DTO — unlike MME matrices). See [06-02-database-schema-spec.md §3.2](../integration-and-api/06-02-database-schema-spec.md) for the DDL.

| Field | Producer | Notes |
|---|---|---|
| `order_id`, `client_order_id` | TAE L2 | |
| `symbol`, `order_type`, `direction` | TAE L2 | |
| `price`, `trigger_price` | TAE L2 | |
| `size`, `filled_size` | TAE L2 | |
| `status` | TAE L2 | 7-state lifecycle vocabulary |
| `is_reduce_only` | TAE L2 | `true` for all bracket/exit orders (v7) |
| `is_emergency_liquidation` | TAE L2 | Hard Exit path flag |
| `associated_position_id` | TAE L2 | |
| `created_at`, `updated_at` | TAE L2 | |
| `slippage_bps` | TAE L2 | |

> **Materialization note.** Unlike MME matrices (which are JSON DTOs broadcast over WebSocket), the Execution Matrix is a database artifact — its canonical schema is the `open_orders` DDL in `06-02-database-schema-spec.md §3.2`. The SetupPlan is transient executor state; the Execution Matrix is the persistent artifact of the TAE chain.

---

## 3. Removed Fields (Migration Map)

| Removed From | Old Field | New Location | Migration Note |
|---|---|---|---|
| Analysis Matrix (L3) | ~~`opportunity_analysis`~~ (canonical producer) | Opportunity Matrix (L4) `primary_opportunity` | The setup selector moved to where it belongs: L4 owns forecasts. **The L3 field is retained on the wire** for backward compatibility only (coarser derivation; see §2.3) — the UI reads `primary_opportunity`. |
| Risk Matrix (L5) | `expected_rr` | Decision Matrix (L6) `entry_danger` | The reward dimension is a synthesis, not pure danger; moved to L6. |

---

## 4. Confidence-Field Renames

Per the institutional redesign (no backwards-compat aliases):

| Old Name | New Name | Producer |
|---|---|---|
| `Analysis.confidence` | **`Analysis.state_confidence`** | L3 |
| `Opportunity.confidence` | **`Opportunity.forecast_confidence`** | L4 |
| `DecisionContext.confidence` | **`DecisionContext.score_confidence`** | L6 |
| `AdvisoryMatrix.confidence_assessment` | (unchanged) | L6 |

The four `confidence`-bearing fields follow a strict hierarchy: indicator → state → forecast → score → risk-attenuated assessment. See [02-00b-confidence-hierarchy.md](02-00b-confidence-hierarchy.md) for details.

---

## 5. Dependency Edges (Allowed and Forbidden)

**Allowed (forward-only, unidirectional):**
```
L2 ← L1 (per-TF)
L3 ← L2 (per-instance)
L4 ← {L3, L1 metrics signals, L1.5/L2.5 liquidity products}
L5 ← {L3, L1 indicator map, L1.5, L2.5}
L6 ← {L2 tradability, L3, L4, L5}
L7 ← {L6 of all symbols}
TAE (v7 executor) ← {L4, L6}  (SetupPlan reads Opportunity + Decision)
TAE_L2 ← {TAE_L1, PME Capital Matrix, MME L6 stop_loss_distance_pct}  (Execution Matrix reads Policy + sizing inputs)
```

> **L1→L3 traceability-evidence exception (v6.10.21).** The Analysis Matrix carries two L1-produced fields — `representative_bbwp` and `representative_adx` — as *evidence copies* stamped during cross-TF synthesis. These are provenance/traceability data (the exact L1 inputs behind the L3 regime interpretation), **not** L3 computation dependencies: the qualitative enums derive purely from L2 alignment scores. This mirrors the `L4 ← {L1, L1.5, L2.5}` multi-source exceptions already formalised for the Liquidity Intelligence extension and does not extend L3's computation boundary. L3's *derived state* remains `L3 ← L2`-only; the v6.12 numeric companions (`trend_score` … `volume_score`) follow that pure edge, unlike the traceability stamps. **v6.14:** the third stamp, `trend_stability_sharpe` (v6.11), was **removed** — the Trend card badge, matrix field, and export pair are gone; the L1 `price_trend_sharpe` indicator remains the sole Sharpe family member (see [04-02-52](../engines/market-monitoring-engine/indicators/04-02-52-price-trend-sharpe.md)).

**Forbidden:**
- L4 ↔ L5 (no edge in either direction — they are strictly orthogonal; the only cross-coupling is via the shared L3 fan-out plus the L1.5/L2.5 multi-source exceptions above)
- Anything ← L6 (L6 is terminal within the MME cascade; downstream engines TAE/PME read from L6 but do not write back)
- Anything ← L7
- TAE → MME / DIE (forward-only from MME to TAE; TAE outputs do not feed back into market analysis)

> **L4↔L1.5 / L4↔L2.5 architecture clarification.** The Opportunity Matrix `LiquiditySqueeze` precondition reads `LiquidityFlow.cascade_state` (L1.5 derivatives telemetry) and `LiquidationClusterMatrix.cascade_asymmetry` (L2.5 cluster matrix). The rule above formalises L4's *forward-only* access to L1.5/L2.5 — the reverse edge (L1.5/L2.5 reading L4) remains forbidden. The earlier restriction (`L4 ↔ L1.5 / L2.5 forbidden` without exception) was incorrect: a forward-only exception for the Liquidity Intelligence extension is required to evaluate the `LiquiditySqueeze` setup preconditions.

> **Panel composition vs matrix production (v6.10.19b; v7.1 L4-only bars).** The ownership/dependency edges above bind **matrix production** — the Rust computation that writes each matrix. The dashboard **panels are composed views** over the wire state and may display L6-derived data on any tab; the canonical precedent is the L4 card direction staying coherent with the L6 market verdict (`02-08`, FIX-1 v6.10.15). **v7.1:** the second precedent (the L4 directional bars mirroring the L6 verdict split, v6.10.18 I-4) was **reversed** — the L4 bars are bracket-derived from the L4 matrix only and never read L6 probabilities. Two v6.10.19b compositions follow the same rule:
> - **Verdict-consistent Recommendation `top_setup`** — the L6 panel selects its headline from L4 profiles by the L6 verdict (L6 ← L4 is an allowed edge); counter-bias qualifying setups ride in `alternate_qualifying_setups`.
> - **G3 verdict lean** — the L6 verdict-resolution may be pulled to the side of a qualifying setup with a resolvable side when probabilities are hold-dominant (the backend `DecisionContext` probabilities are untouched; only the UI's `top` labelling changes).
> - **Opportunities reference bracket (v6.10.19b, per-folder v6.10.23)** — the L4 panel mounts per-direction aggregated reference brackets inside the BULLISH/BEARISH folders (mirroring the Recommendation's aggregated bracket for display parity: the invariant *whatever the Recommendation headlines is always present in the Opportunities panel*) plus the L4-produced `neutral_reference_bracket` range frame in RANGE SETUPS. The Opportunity **Matrix** is never computed from L6 — production stays `L4 ← {L3, L1, L1.5, L2.5}`; `neutral_reference_bracket` is pure L4 (NoClear + range), and the per-folder brackets are panel compositions over the matrix's own per-side zones.
>
> **L5↔L1.5 / L5↔L2.5 architecture clarification.** Per [Risk Matrix §4](../matrices/02-11-risk-matrix.md) and [MME Layer 5 §1](../engines/market-monitoring-engine/03-02-06-mme-layer5-risk.md), `cascade_risk` combines `LiquidityFlow.cascade_intensity` (L1.5) with `cascade_asymmetry` (L2.5). The dependency edges above are `L1.5 → L5` and `L2.5 → L5`. The 7-layer architecture is preserved by treating these as multi-source exceptions for the Liquidity Intelligence extension; the foundational 7-layer model remains the spine for non-liquidity layers.

---

## 6. Cross-References

- [02-00b-confidence-hierarchy.md](02-00b-confidence-hierarchy.md) — Confidence field rename & flow.
- [02-02-analysis-matrix.md](02-02-analysis-matrix.md) · [02-08-opportunity-matrix.md](02-08-opportunity-matrix.md) · [02-11-risk-matrix.md](02-11-risk-matrix.md) · [02-04-decision-matrix.md](02-04-decision-matrix.md) — Per-matrix specs.
- [01-01-ontology.md](../conceptual-foundations/01-01-ontology.md) — Conceptual layer definitions.
- [01-03-systemic-data-flow.md](../conceptual-foundations/01-03-systemic-data-flow.md) — Sequence diagrams.
