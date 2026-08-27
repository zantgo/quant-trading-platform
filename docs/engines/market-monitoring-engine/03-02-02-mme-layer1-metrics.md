# MME Layer 1 — Metrics Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Layer:** 1 of 7
**Output Contract:** [Metrics Matrix](../../matrices/02-07-metrics-matrix.md)
**Purpose:** This document specifies the Metrics Layer — the process by which the MME computes technical indicators and signals for a single timeframe and projects each onto its standardized Evaluation Axes.

---

## 1. Purpose

The Metrics Layer is the first analytical transformation: it converts a completed OHLCV candle plus indicator buffers into a fully contextualized [Metrics Matrix](../../matrices/02-07-metrics-matrix.md). It projects each indicator across its **8 Indicator Evaluation Axes** and each detected signal across its **10 Signal Evaluation Axes**.

```
[completed candle + buffers]
   │
   ▼
indicator calculators ──► NormalizationEngine ──► signal detectors ──► MarketContext
   │                                                                        │
   └──────────────────► [Metrics Matrix] ◄──────────────────────────────────┘
```

Implementation: `analyzer/mod.rs::run_single()`, `analyzer/normalize.rs::build_indicator_map()`.

---

## 2. Stage 1 — Indicator Computation

Every registry-enabled indicator calculator runs against the current candle buffers, producing a native `raw_value` (and auxiliary component lines for multi-line indicators such as MACD, Bollinger, ADX). The 52 indicators span eight functional groups (10 Trend + 7 Momentum + 7 Volume + 6 Volatility + 5 Structure + **5 Regime** + 4 Institutional + 8 Derivatives — see [`01-01-ontology.md` Appendix B §B.2](../../conceptual-foundations/01-01-ontology.md)):

| Group | Examples |
|-------|----------|
| Trend | EMA ribbon, Supertrend, Donchian, Keltner, ADX, VWAP, Ichimoku, PSAR, Hull MA |
| Momentum | RSI, Stochastic, ChandeMO, Williams %R, Awesome Oscillator, CCI, MACD |
| Volume | Volume, RVOL, Volume Profile, OBV, CMF, MFI, Force Index |
| Volatility | ATR, Bollinger, BBWP, TTM Squeeze, HV, StdDev Channel |
| Structure | Fibonacci, Support/Resistance, Pivot Points, Chart Patterns, Candlestick |
| Regime | Aroon, Choppiness, LinReg Slope, Z-Score, Price-Trend Sharpe |
| Institutional | SMC Structure, Liquidity, FVG, Order Blocks |
| DerivativesData | Open Interest, OI Delta, Funding Rate, OI-Price Divergence, Order Flow Imbalance, Spread, Depth Bias, Mark-Index Spread |

See [indicators/index.md](indicators/04-02-00-indicator-index.md) for the authoritative manifest.

> **EMA Ribbon canonical record (v6.11+).** The four EMAs (fast / medium / slow / long) share a single canonical record: `MarketSnapshot.indicators["ema_stack"].values.{fast, medium, slow, long}`. The Metrics Layer writes it via `inject_ema_values` (`crates/market-analyzer/src/analyzer/normalize.rs:521-546`); the chart overlay reads it per bar (`PriceChart.svelte:336-340`); the on-screen Indicators facet micro-grid reads it (`IndicatorsView.svelte`); and the per-TF Metrics export body's `body.ema` block reads it via `buildEmaBlock` (`shared.ts`). Every consumer reads the same record — there is no second computation. See [04-02-01-ema-stack.md §Unified Ribbon Export](indicators/04-02-01-ema-stack.md) for the full contract.

---

## 3. Stage 2 — Normalization

The `NormalizationEngine` (`crates/market-analyzer/src/indicators/normalized/`) maps each raw value to a continuous `normalized ∈ [-1.0, 1.0]` score and a context-aware `state_label`. Normalization is **regime-aware**: thresholds shift with market context (e.g. RSI overbought tightens to 80 in a strong trend).

Each indicator becomes an `IndicatorEvaluation` (`NormalizedIndicatorValue`) carrying `raw_value`, `normalized`, `state_label`, optional `values`, `signals`, and `confidence`. See [Metrics Matrix §3](../../matrices/02-07-metrics-matrix.md).

In v6.5 every `MarketSnapshot` additionally carries:

- **`tf.pipeline_state: CandlePipelineState`** — the per-timeframe pipeline lifecycle ([03-01-06](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md) DCP-01 … DCP-15). One of `INITIALIZING | LOADING | LIVE | STALE | FAILED`. Published on every snapshot.
- **`tf.indicator_lifecycle: HashMap<String, IndicatorLifecycleStatus>`** — the per-indicator lifecycle ([03-02-15](03-02-15-mme-indicator-lifecycle-states.md) ILS-01 … ILS-15), keyed by registry key. Each value carries `state | bars_seen | bars_required | last_updated_at | last_error | stale_threshold_secs`. The map is populated alongside `indicators` on every snapshot and updates on every completed candle. Confidence overrides apply (ILS-14) when the lifecycle state is non-`Live`.

Both fields are always populated (no `skip_serializing_if`); the dashboard never has to distinguish "absent" from "empty map". The active-set rule ([03-02-12](03-02-12-mme-configurable-activation.md)) applies asymmetrically: disabled indicators are **removed from `indicators` only** — they **remain** in `indicator_lifecycle` (reporting `Loading` with `present = false`; see ILS-12 in [03-02-15](03-02-15-mme-indicator-lifecycle-states.md)), so the lifecycle map covers the full registry.

> **Target Architecture (Not Yet Implemented).** To eliminate slow string-hashing lookups (e.g. `map.get("rsi")`), the hot-path Metrics Matrix is intended to be a flat, cache-aligned struct indexed by a compiled `Enum` offset rather than a `HashMap<String, …>`:
>
> ```rust
> #[repr(C)]
> pub struct MetricsMatrix {
>     pub indicators: [IndicatorEvaluation; 51], // indexable by an Enum offset
>     pub timestamp: u64,
>     pub close: f64,
> }
> ```
>
> All 52 technical calculators (RSI, ATR, MACD, …) would execute their smoothing and crossovers using raw `f64` primitives, enabling CPU SIMD auto-vectorization. *Current implementation:* indicators are stored in `MarketSnapshot.indicators: HashMap<String, NormalizedIndicatorValue>` and most calculators compute in `rust_decimal::Decimal`.

---

## 4. Stage 3 — Signal Detection

Signal detectors project discrete events onto the 12 `SignalKind`s. Detection is stateful — crossovers and zero-line crosses require the `PreviousBarState` for reference.

| Detector family | SignalKinds produced |
|-----------------|----------------------|
| Divergence engine (8 indicators) | `Divergence` |
| Crossover detectors | `Crossover`, `ZeroLineCross`, `StackChange` |
| Threshold detectors | `Threshold` |
| Structural detectors | `Breakout`, `BandTouch`, `LevelTest`, `TrendFlip` |
| Volatility / volume | `CompressionRelease`, `VolumeClimax` |
| Pattern detectors | `PatternForming` |

Each `IndicatorSignal` carries `kind`, `direction`, `status`, `label`, `strength`, and `age_bars` (stamped by the stateful ager). See [Signals Guide](03-02-10-mme-signals-guide.md).

---

## 5. Stage 4 — Evaluation-Axis Projection

The layer projects flat telemetry onto the standardized axes defined in the [Ontology](../../conceptual-foundations/01-01-ontology.md):

### 5.1 Indicator Axes (8)
Value · State · Direction · Strength · Market Regime · Confidence · Freshness · Quality.

### 5.2 Signal Axes (10)
Signal Type · Direction · Strength · Confidence · Freshness · Confirmation · Market Regime · Multi-Timeframe Agreement · Risk · Priority.

The mapping from struct fields to axes is defined in [Metrics Matrix §3.2 / §4](../../matrices/02-07-metrics-matrix.md).

---

## 6. Stage 5 — Local Confluence (MarketContext)

`MarketContext::synthesize()` aggregates the indicator map into per-timeframe dimensions (trend, momentum, volatility, volume, liquidity), classifies the regime, and computes an `overall_score ∈ [-100, 100]`. This is **local confluence** — single-timeframe consensus — distinct from cross-timeframe alignment (Layer 2).

Regime rule (local 4-state `MarketContext.regime` vocabulary; see [Metrics Matrix §5.0](../../matrices/02-07-metrics-matrix.md) for the cross-layer mapping to the canonical 8-state L3 `MarketRegime`):
```
bbwp ≤ 15 OR chop ≥ 61.8 → COMPRESSION
bbwp ≥ 85                → EXPANSION
adx ≥ 25 OR chop ≤ 38.2  → TRENDING
else                     → RANGE
```

> **Cross-layer vocabulary.** Note that `MarketContext.regime` is the **local 4-state** vocabulary (`COMPRESSION` / `EXPANSION` / `TRENDING` / `RANGE`). The cross-TF canonical `MarketRegime` at L3 uses the **8-state** vocabulary (`TRENDING_BULL` / `TRENDING_BEAR` / `RANGE` / `ACCUMULATION` / `DISTRIBUTION` / `EXPANSION` / `CONTRACTION` / `TRANSITION`). The two are linked by the mapping table in [02-07-metrics-matrix.md §5.0](../../matrices/02-07-metrics-matrix.md); comparisons across layers must go through the L3 Analysis Matrix.
>
> **Layer-specific BBWP thresholds (intentional divergence).** The Layer 1 local 4-state regime uses a slightly looser compression threshold (`bbwp ≤ 15`) and expansion threshold (`bbwp ≥ 85`) than the L3 Analysis Matrix (`bbwp ≤ 10` for `CONTRACTION`, `bbwp ≥ 85` for `EXPANSION` — see [02-02-analysis-matrix.md §3.2](../../matrices/02-02-analysis-matrix.md)). The two thresholds serve different purposes: Layer 1's local `MarketContext.regime` is a coarse 4-state approximation for the chart-side composite that should flag borderline conditions early, while the L3 regime is the cross-symbol classifier that feeds the Decision Layer. A value in `[10, 15]` is therefore classified as `COMPRESSION` at Layer 1 but as `RANGE` (not `CONTRACTION`) at Layer 3 — both states are valid for their respective layers. Operators who rely on the local 4-state should treat `[10, 15]` as 'borderline compression' rather than canonical `CONTRACTION`.

---

## 7. Output & Freshness

- **Completed candle** → full Metrics Matrix (feeds Layers 2–7).
- **Live tick** → shadow snapshot (`is_completed = false`) for real-time display only.
- Signal `age_bars` increments per completed bar, driving the Freshness axis.

---

## 8. MarketSnapshot Broadcast Channel

The Metrics Layer is the **sole producer** of `MarketSnapshot` frames. Once built (after Stage 5), the completed or shadow snapshot is published into a per-`(symbol, timeframe)` Tokio `broadcast` channel. This is the same broadcast transport referenced in the DIE L4 doc — the channel infrastructure is co-located in `market-analyzer`, but the **content ownership** belongs exclusively to MME L1. (DIE L4 owns the separate `NormalizedCandle` broadcast channel; see [03-01-05-die-layer4-data-distribution.md](../data-infrastructure-engine/03-01-05-die-layer4-data-distribution.md).)

### 8.1 Channel Model

| Property | Value |
|----------|-------|
| Topology | One `MarketSnapshot` broadcast channel per `(symbol, timeframe)` pipeline. |
| Fan-out | Unlimited subscribers; each receives every frame. |
| Lag handling | Slow consumers receive `RecvError::Lagged(n)`; they resynchronize rather than blocking the producer. |
| Payload | Completed (`is_completed = true`) and shadow (`is_completed = false`) `MarketSnapshot` frames. |

### 8.2 Subscriber Table

| Consumer | Subscription | Transport |
|----------|-------------|-----------|
| MME L2–L7 | All configured timeframes for an instance. | `MarketSnapshot` broadcast receiver → higher-layer cascade. |
| Frontend | 4 parallel connections (micro/fast/slow/macro). | WebSocket `/ws?symbol=&timeframe_secs=` → `MarketSnapshot` channel. |
| Telemetry logger | Completed snapshots only. | `MarketSnapshot` broadcast receiver → SQLite `market_snapshots`. |

The WebSocket handler (`server/ws.rs`) resolves the requested `(symbol, timeframe_secs)` to the correct `MarketSnapshot` channel and streams frames to the client.

### 8.3 Wire Format (JSON-RPC 2.0)

Frames are serialized as **JSON-RPC 2.0 notifications** for the WebSocket transport:

```json
{
  "jsonrpc": "2.0",
  "method": "broadcast.market_snapshot",
  "params": {
    "symbol": "BTC-USDT",
    "timeframe_secs": 60,
    "snapshot": { /* MarketSnapshot */ }
  }
}
```

Notifications carry no `id` (no response expected). See the [API Gateway Contract](../../integration-and-api/06-01-api-gateway-contract.md) for the full protocol.

### 8.4 Serialization Rules

| Rule | Effect |
|------|--------|
| `skip_serializing_if = "Option::is_none"` | Absent optional fields are omitted, shrinking frames. |
| Empty collections omitted | Empty `signals` / null `values` maps dropped. |
| Decimal-as-number | `Decimal` fields serialize as plain JSON numbers (`rust_decimal` `serde-float` feature, `crates/core-domain/Cargo.toml`). See [06-01 §4](../../integration-and-api/06-01-api-gateway-contract.md). |
| Shadow streaming | Live shadow frames (`is_completed = false`) stream at tick cadence; completed frames (`is_completed = true`) on candle close. |

### 8.5 Guarantees

| Property | Guarantee |
|----------|-----------|
| **Non-blocking** | A slow/failed subscriber never stalls the producer. |
| **At-most-once per cursor** | Each subscriber sees each frame at most once; lag is signalled explicitly. |
| **Ordering** | Frames within a channel are delivered in production order. |
| **Immutability** | Once broadcast, a completed snapshot is never mutated (see [Metrics Matrix §7](../../matrices/02-07-metrics-matrix.md)). |

---

## 9. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Completeness** | Every registry-enabled indicator is present (neutral if data-starved). |
| **Determinism** | Identical buffers + prior-bar state → identical Metrics Matrix. |
| **Regime awareness** | Normalization thresholds adapt to the active regime. |
| **Explainability** | Every signal carries its label, status, and strength. |

### 9.1 Configurable activation (Active Set)

The layer's indicator/signal computation is driven by a config-derived **Active Set**: registry defaults minus the union of the global `[activation]` denylist and the per-instance `[instances.*.activation]` denylist. Disabled indicators/signals are **absent** from the produced `MarketSnapshot` — never null, never tombstoned — and downstream layers reuse the existing NO_DATA/empty-state machinery with no new special cases. The active set is *recorded* on completed-candle snapshots via the optional `metrics_config` block (omitted when the active set equals the registry default, so default-path frames remain byte-identical to pre-feature frames). Shadow frames always carry `metrics_config: None` (`broadcast_live_snapshot` zeroes it for throughput); consumers reading the accumulated `tf.indicators` map see the completed-frame attribution. The 52-indicator / 12-SignalKind / 101-declaration **registry** describes capability and never changes with config; activation is a runtime config concern. Canonical spec, wire contract, downstream degradation rules, and the registry-invariance requirement are in [03-02-12-mme-configurable-activation.md](03-02-12-mme-configurable-activation.md).

### 9.2 Shadow-path freshness (v6.7)

Shadow ticks (`is_completed = false`) carry a clone-based recomputation of ~22 tick-safe indicators (EMA, RSI, MACD, ADX, Bollinger, ATR, BBWP, Stochastic, ChandeMO, Supertrend, Keltner, Donchian, OBV, CMF, MFI, HV, Aroon, Choppiness, LinReg, ZScore, VWAP) — the same calculators as the completed path, `.clone().update()` on the in-progress tick price, then discarded. These ~22 entries carry no wire-side marker (freshly computed this tick) — the earlier spec text referenced a `pending_candle` property, but no such field exists on the wire (`NormalizedIndicatorValue` has no `pending_candle`; it survived only as a legacy frontend concept and was dropped by the export builders). Freshness on the wire is expressed by `updates_on_shadow` in the registry and by absence from the shadow map. The remaining ~28 close-dependent indicators (Fibonacci, patterns, S/R zones, Ichimoku, CCI, PSAR, Hull MA, AO, Force Index, StdDev Channel, Volume Profile, SMC, Anchored VWAP, Derivatives/OrderBook group) are absent from the shadow indicator map. For consumers reading from the accumulated `tf.indicators` map (the canonical source — see §9.3), the frontend per-key merge preserves the last completed-candle value across shadow ticks; the `updates_on_shadow` registry metadata tells the UI whether to append a `◉` confirmed-on-close marker or display a tick-fresh reading.

### 9.3 Single Source of Truth

The `indicators` map emitted by this layer — paired with its sidecar `indicator_lifecycle` map — is the **single canonical source of truth** for all indicator data across the platform. Every downstream consumer (frontend charts, Metrics-tab facets, synthesis layers L2–L7, export JSON, DB telemetry logger) reads from this accumulated map and from no other source. On the frontend, the `applySnapshotToTimeframe()` handler performs a per-key spread-merge (`{ ...tf.indicators, ...incoming }`) on every snapshot arrival so the map is never sparse even when shadow ticks omit close-dependent indicators. The `updates_on_shadow` metadata in the indicator registry (`registry.rs`) governs which entries are fresh on shadow ticks vs. confirmed-on-close — see the [Metrics Matrix §2.1.1](../../matrices/02-07-metrics-matrix.md) and the [Consumer Onboarding doc §3.1](../../integration-and-api/06-00-consumer-onboarding.md) for consumer-side rules.

---

## 10. Phase 0-3 Extensions: Derivatives Telemetry

The Phase 0-3 Liquidity Intelligence extension adds a parallel
**derivatives telemetry** stream that runs alongside the price
indicators:

| New indicator group | Source | Field on `MarketSnapshot` |
|---|---|---|
| `mark_index_spread` | mark + index prices | `mark_index_spread_pct` |
| Real OI (Hyperliquid via activeAssetCtx) | `MetaAndAssetCtxs` polling | `open_interest`, `oi_delta_1h` |
| Real funding rate | WS push | `funding_rate` |
| Real liquidation events (Phase 1) | `userFills` / `fill` channel | `liquidity: LiquidityFlow` |
| Estimated heatmap (Phase 2) | Cluster estimator | `cluster: LiquidationClusterMatrix` |
| Liquidity signals (Phase 3) | Signal derivation | `liquidity_signals: Vec<LiquiditySignal>` |

> **Liquidity fields are part of the Metrics Layer pipeline, not an independent matrix.** The `liquidity`, `cluster`, and `liquidity_signals` fields on `MarketSnapshot` are computed by the same pipeline infrastructure as the indicator and signal evaluation. The Liquidity Intelligence extension (Phases 0-4) adds two fractional layers (L1.5: Derivatives Telemetry, L2.5: Liquidity Synthesis) between L1 and L2 — these are additive sub-layers of the MME, documented at `03-02-11-mme-liquidity-extension.md`. In the frontend, liquidation cluster data renders inline on the Charts tab alongside price candlesticks and indicator panes.

These are not "indicators" in the strict sense (they are not
normalised f64 signals in the indicator map) — they are
*telemetry matrices* that ride the MarketSnapshot as optional
fields and are consumed by extension layers L1.5/L2.5 (fractional layers of MME), L4 (Opportunity), L5, and the UI.

## 11. Cross-References

- [Metrics Matrix](../../matrices/02-07-metrics-matrix.md) — Output contract.
- [DIE Layer 4 — Data Distribution](../data-infrastructure-engine/03-01-05-die-layer4-data-distribution.md) — The `NormalizedCandle` broadcast channel.
- [Indicators Guide](03-02-09-mme-indicators-guide.md) · [Signals Guide](03-02-10-mme-signals-guide.md)
- [MME Layer 2 — Alignment](03-02-03-mme-layer2-alignment.md) — Direct consumer.
- [Liquidity Extension](03-02-11-mme-liquidity-extension.md) — Derivatives telemetry.
- [LiquidityMatrix](../../matrices/02-12-liquidity-matrix.md) · [ClusterMatrix](../../matrices/02-13-liquidation-cluster-matrix.md)
