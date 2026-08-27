# Market Monitoring Engine — Overview Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Purpose:** This document specifies the boundaries, module pipeline, concurrency strategy, and instance-management model of the Market Monitoring Engine — the analytical heart of the platform. The MME transforms clean market data into multi-timeframe technical intelligence across seven core analytical layers (L1–L7) with two fractional extension layers (L1.5: Derivatives Telemetry, L2.5: Liquidity Synthesis) — see `01-01-ontology.md` Ch. 6 and `03-02-11-mme-liquidity-extension.md`. L1–L3 sequential, L4 ∥ L5 parallel from L3 (with additional feeds from L1.5 and L2.5), L6–L7 sequential after convergence.

---

## 1. Mission & Boundaries

The MME **interprets** the market. It consumes the DIE's Market Data Matrix and produces the full analytical cascade: Metrics → Alignment → Analysis → (Opportunity ∥ Risk) → Decision → Overview. It executes **no trades** and holds **no capital** — it is a pure observation-and-interpretation engine.

> **Target Architecture (Not Yet Implemented).** MME **Layers 1–5 (including L1.5 and L2.5)** are intended to be **strict Data-Oriented Design (DOD)** pipelines: their concurrent `TimeframePipeline` tasks process contiguous `f64` arrays with **zero runtime allocations** and no heap-allocated collection searches (no per-tick `HashMap` lookups). *Current implementation:* the pipeline computes indicators in `rust_decimal::Decimal` and carries results in a `HashMap<String, NormalizedIndicatorValue>`-keyed `MarketSnapshot`.

```
[Market Data Matrix] ──► MME (7 core + 2 fractional layers) ──► [Decision Matrix] ──► [TAE]
                                                         └──► [Overview Matrix] ──► [PME veto]
```

### 1.1 Layer Structure

| Layer | Name | Output Matrix |
|-------|------|---------------|
| L1 | [Metrics](03-02-02-mme-layer1-metrics.md) | [Metrics Matrix](../../matrices/02-07-metrics-matrix.md) |
| L1.5 | [Derivatives Telemetry](03-02-11-mme-liquidity-extension.md) | [LiquidityFlow](../../matrices/02-12-liquidity-matrix.md) |
| L2 | [Alignment](03-02-03-mme-layer2-alignment.md) | [Alignment Matrix](../../matrices/02-01-alignment-matrix.md) |
| L2.5 | [Liquidity Synthesis](03-02-11-mme-liquidity-extension.md) | [LiquidationClusterMatrix](../../matrices/02-13-liquidation-cluster-matrix.md) |
| L3 | [Analysis](03-02-04-mme-layer3-analysis.md) | [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) |
| L4 | [Opportunity](03-02-05-mme-layer4-opportunity.md) | [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md) |
| L5 | [Risk](03-02-06-mme-layer5-risk.md) | [Risk Matrix](../../matrices/02-11-risk-matrix.md) |
| L6 | [Decision Support](03-02-07-mme-layer6-decision-support.md) | [Decision Matrix](../../matrices/02-04-decision-matrix.md) |
| L7 | [Overview](03-02-08-mme-layer7-overview.md) | [Overview Matrix](../../matrices/02-09-overview-matrix.md) |

---

## 2. Module Pipeline

The per-timeframe pipeline (`crates/market-analyzer/src/analyzer/mod.rs`) executes on every completed candle:

```
completed candle
   │
   ▼
[indicator calculators]  → raw values (52 indicators)
   │
   ▼
[NormalizationEngine]    → normalized [-1,1] scores + state labels
   │
   ▼
[build_indicator_map]    → unified IndicatorEvaluation map
   │
   ▼
[signal detectors]       → SignalKind projection (divergence, crossover, ...)
   │
   ▼
[MarketContext::synthesize] → per-TF context (L1 output)
   │
   ▼
[cross-TF synthesis]     → Alignment → Analysis → (Opportunity ∥ Risk) → Decision (L2–L6)
   │
   ▼
[Overview aggregation]   → cross-symbol synthesis (L7)
   │
   ▼
broadcast MarketSnapshot
```

For open (still-flickering) candles, a lighter **shadow path** runs on every tick:

```
open candle tick
   │
   ▼
[fast indicator recompute] → raw values update (no signal detection)
   │
   ▼
[build_indicator_map]     → unified IndicatorEvaluation map (shadow-only)
   │
   ▼
broadcast MarketSnapshot (is_completed = false)
```

Shadow snapshots carry updated indicator readings for real-time display, but signals, cross-TF synthesis, and higher-layer matrices are not re-evaluated until the candle closes.

> **Single Source of Truth.** The `indicators` map produced at `build_indicator_map` (both completed and shadow paths) is the **single canonical source of truth** for all indicator data across the platform. Every downstream consumer — all 35 frontend chart components, all 6 Metrics-tab facets, GroupConfluenceGrid, StructuralAnchorsStrip, export JSON, DB logger, and L2–L7 synthesis layers — reads from the accumulated `tf.indicators` map and from no other data source for indicator values. On the frontend, `applySnapshotToTimeframe()` performs a per-key spread-merge on every snapshot arrival so close-dependent indicators (Fibonacci, patterns, S/R, Ichimoku, etc.) persist across shadow ticks. The `updates_on_shadow` registry metadata governs which entries carry tick-fresh values vs. last-completed-candle values. See the [Metrics Matrix §2.1.1](../../matrices/02-07-metrics-matrix.md) for the full contract.

---

## 3. Concurrency Strategy

| Concern | Strategy |
|---------|----------|
| **Per-instance isolation** | Each Market Instance owns an independent async pipeline; no shared mutable state between instances. |
| **Per-timeframe pipelines** | The four timeframes of an instance run as concurrent `TimeframePipeline`s. |
| **Warm-then-stream** | An instance bootstraps (warm-up from history) before subscribing to the live broadcast. |
| **Lock scope** | Shared registries (symbol mapper, instance registry) use `RwLock` with minimal critical sections. |
| **Backpressure** | Bounded channels between DIE and MME; broadcast lag is signalled, never silently dropped. |

> **Target Architecture (Not Yet Implemented).** In the DOD target, each `TimeframePipeline` operates on pre-allocated, cache-aligned `f64` buffers so the Layer 1–5 chain runs allocation-free per candle, letting the CPU prefetch and vectorize the indicator math. The isolation and backpressure guarantees above hold for both the current and target designs.

---

## 4. Symbol-Specific Instance Management

A **Market Instance** (`crates/portfolio-supervisor/src/instance.rs`) is the smallest operational unit: one symbol with its four timeframe pipelines, trading state, safety manager, and config.

### 4.1 Instance Lifecycle

| Operation | Effect |
|-----------|--------|
| `add_instance` | Validates symbol availability, bootstraps history, spawns pipelines. |
| `pause_instance` | Halts the event loop while preserving state. |
| `stop_instance` | Cancels the instance via its cancel token. |
| `delete_instance` | Removes the instance and releases resources. |
| `recharge_instance` | Reconfigures (e.g. new indicator periods) with preserved warm state. |

Instance CRUD is exposed via the `/api/instances` REST surface (see [API Gateway](../../integration-and-api/06-01-api-gateway-contract.md)).

### 4.2 Session-First Boot

The MME follows a Welcome-Gate pattern: no pipelines spawn until a **session** (exchange + currency) is initialized. This ensures every instance is created against a well-defined venue and settlement currency.

---

## 5. Indicator & Signal System

The MME computes **52 technical indicators** across 8 functional groups, with **101 signal-kind declarations** across 12 SignalKind types (post-v6.6; the historical 101 → 100 transition is documented in [`01-01-ontology.md` Appendix B §B.3 editor's note](../../conceptual-foundations/01-01-ontology.md), and the current 100 → 101 add-back reflects the v6.6 `mark_index_spread` registry entry). Every indicator is declared once in the authoritative registry (`crates/market-analyzer/src/indicators/registry.rs`).

- Per-indicator specifications: [indicators/](indicators/04-02-00-indicator-index.md)
- Indicator rulebook: [mme-indicators-guide.md](03-02-09-mme-indicators-guide.md)
- Signal rulebook (12 SignalKinds): [mme-signals-guide.md](03-02-10-mme-signals-guide.md)

---

## 6. Performance Targets

| Metric | Target |
|--------|--------|
| Full 7-layer cascade per candle | ≤ 15 ms (MME share of the 25 ms end-to-end observation loop: DIE Raw→Distribution ≤ 10 ms + MME cascade ≤ 15 ms; see [01-03 §4](../../conceptual-foundations/01-03-systemic-data-flow.md)) |
| Indicator computation (52) | < 10 ms |
| Cross-TF synthesis (L2–L6) | < 5 ms |
| Shadow / pipeline overhead (L3 quality stamp, L7 rollup) | ≤ 3 ms |
| Live shadow update | < 5 ms |

---

## 7. Cross-References

- [Global Architecture](../../conceptual-foundations/01-02-global-architecture.md)
- [Systemic Data Flow — Sequence A](../../conceptual-foundations/01-03-systemic-data-flow.md)
- [Ontology](../../conceptual-foundations/01-01-ontology.md)
- All seven layer specs and the matrix contracts linked in §1.

**Dashboard tabs ↔ layers (v7.3).** The MME workspace sub-tabs follow the layer order (see [07-07 §2](../../ui-ux/07-07-engine-dashboard-vocabulary.md)): Charts · Metrics (L1) · Alignment (L2) · Analysis (L3) · Opportunities (L4) · Risks (L5) · Recommendation (L6); the Overview tab is the L7 aggregate landing.

