# Metrics Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Producing Layer:** Layer 1 — Metrics Layer
**Purpose:** This document defines the physical schema, JSON serialization contract, and state-transition semantics of the **Metrics Matrix** — the unified single-timeframe observation object. The Metrics Matrix is the foundational analytical output of the platform: every downstream matrix (Alignment, Analysis, Opportunity, Risk, Decision, Overview) is a transformation of one or more Metrics Matrices.

---

## 1. Conceptual Definition

The Metrics Matrix is the structured output of the **Metrics Layer** for a single **Market Instance** (`Symbol × Timeframe`). It transforms a completed OHLCV candle plus its indicator buffers into a fully contextualized, multi-axis telemetry object.

Per the [Ontology](../conceptual-foundations/01-01-ontology.md), the Metrics Matrix contains two categories of first-class analytical entities:

1. **Indicators** — continuous quantitative measurements, each projected across the 8 **Indicator Evaluation Axes**.
2. **Signals** — discrete technical events, each projected across the 10 **Signal Evaluation Axes**.

These two categories, together with the attached telemetry sub-objects (`liquidity`, `cluster`, `liquidity_signals`, derivatives/orderbook data), form the platform's **Analytical Input Universe** — the collective term for everything the `MarketSnapshot` envelope carries into MME Layers 2–7. Canonical definition and membership rules: [Ontology §3.9.1](../conceptual-foundations/01-01-ontology.md). The Metrics Matrix itself is the **delivery vehicle**; the universe is the vocabulary.

The Metrics Matrix is **strategy-agnostic**: it describes what the market *is*, not what a strategy *should do*. It does not compare timeframes (that is the Alignment Matrix) and does not interpret bias (that is the Analysis Matrix).

```
[Market Data Matrix]
        │
        ▼
┌─────────────────────────────────────────┐
│            METRICS LAYER (L1)            │
│                                          │
│  candle ──► indicator calculators ──►    │
│  raw values ──► NormalizationEngine ──►  │
│  normalized scores ──► signal detectors  │
│  ──► SignalKind projection ──► axes      │
└─────────────────────────────────────────┘
        │
        ▼
[Metrics Matrix]  (one per Symbol × Timeframe)
```

---

## 2. Physical Schema

The Metrics Matrix is materialized as the `MarketSnapshot` structure (`crates/core-domain/src/models.rs`). It is the single object streamed over the WebSocket bus and persisted to the telemetry store.

> **Target Architecture (Not Yet Implemented).** The Metrics Matrix is intended to have a **dual representation**:
>
> - **Hot-path representation (`FastTelemetryFrame`):** a contiguous, binary, `#[repr(C)]` C-struct layout (enum-indexed `[IndicatorEvaluation; 52]`, `f64` fields) optimized for CPU caches and SIMD, used internally across MME Layers 1–5.
> - **Egress representation:** the serialized JSON-RPC 2.0 payload matching the schema below, used for API distribution and frontend rendering.
>
> *Current implementation:* a single representation — the `MarketSnapshot` struct with `Decimal` OHLCV and `indicators: HashMap<String, NormalizedIndicatorValue>` — serves both the internal broadcast and the JSON egress.

### 2.1 Top-Level Fields

| Field | Type | Nullable | Description |
|-------|------|----------|-------------|
| `exchange` | `Exchange` enum | Yes | Originating venue (`Hyperliquid`, `Bitget`). |
| `symbol` | `string` | No | Unified instrument key, e.g. `BTC-USDT`. |
| `timeframe_secs` | `u64` | No | Candle duration in seconds (any positive integer; shipped config uses 60 / 180 / 300 / 900, sub-minute tiers 1 s–30 s are fully supported — see [03-02-16](../engines/market-monitoring-engine/03-02-16-mme-subminute-vs-aboveminute-parity.md)). |
| `timestamp` | `u64` | No | Candle close time (Unix epoch, **seconds** — `start_time_ms / 1000`). |
| `is_completed` | `bool` | Yes | `true` for a finalized candle; `false`/absent for a real-time "shadow" flicker snapshot. |
| `timeframe_slot` | `TimeframeSlot` | Yes | Stable slot identity (`Micro`/`Fast`/`Slow`/`Macro`/`Custom{id}`) stamped on every snapshot — the authoritative wire-side slot identifier (06-01 §3.1). |
| `pipeline_state` | `CandlePipelineState` | No | `Initializing`/`Loading`/`Live`/`Stale`/`Failed` (DCP-05: `Stale` = no completed candle for `candle_buffer.stale_threshold_secs`; `Failed` = 2× window). Documented in [03-01-06-die-candle-pipeline-states.md](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md). |
| `indicator_lifecycle` | `map<string, IndicatorLifecycleStatus>` | No | Per-indicator lifecycle states (ILS-01..ILS-16) — see [03-02-15-mme-indicator-lifecycle-states.md](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md). |
| `mid_price` | `Decimal` | No | Mid of best bid/ask at snapshot time — the **fresh order-book mid** `(best_bid + best_ask) / 2` when the book was updated within the grace window (AUDIT-V8-002; `grace_period_ms` = candle duration), else the candle close. No longer the candle volume/close blend. |
| `bid_price` / `ask_price` | `Decimal` | No | Top-of-book quotes. |
| `bid_size` / `ask_size` | `Decimal` | Yes | Top-of-book **level-1 resting sizes** (`best_bid_size` / `best_ask_size`); JSON `null` when the book is not fresh within the AUDIT-V8-002 grace window (no `skip_serializing_if` on these fields). No longer the candle volume. |
| `funding_rate` | `Decimal` | Yes | Current perpetual funding rate. |
| `open` / `high` / `low` / `close` | `Decimal` | Yes | OHLC of the candle. |
| `volume` | `Decimal` | Yes | Candle volume. |
| `volume_profile` | `VolumeProfileSnapshot` | Yes | Per-candle volume-profile histogram (POC/VAH/VAL, buy/sell bins) — see [03-02-13-mme-volume-profile-layer.md](../engines/market-monitoring-engine/03-02-13-mme-volume-profile-layer.md). `None` before the warmup gate. |
| `quality_envelope` | `CandleQualityEnvelope` | Yes | Per-candle data-quality envelope (score, validity, gap/reconstruction provenance, sequence integrity) — canonical source: [02-03-data-quality-matrix.md](02-03-data-quality-matrix.md). |
| `average_volume` | `Decimal` | Yes | Rolling average volume baseline. |
| `open_interest` | `Decimal` | Yes | Open interest at snapshot time. |
| `oi_delta_1h` | `Decimal` | Yes | 1-hour rolling open-interest change. |
| `prev_day_px` | `Decimal` | Yes | Prior-day reference price (from asset context). |
| `mark_price` | `Decimal` | Yes | Mark price at snapshot time. In-memory writer live (AUDIT-AIU-091); `null` until the first WS mark push. |
| `index_price` | `Decimal` | Yes | Index price at snapshot time. `null` until Phase 3. |
| `mark_index_spread_pct` | `f64` | Yes | Mark/index spread as percentage. Computed live from the in-memory mark/index writers (AUDIT-AIU-091); `null` until both are available. |
| `liquidity` | `Option<LiquidityFlow>` | Yes | Phase 1 LiquidityFlow (real liquidation events aggregated per candle). `None` when liquidity extension disabled. |
| `cluster` | `Option<LiquidationClusterMatrix>` | Yes | Phase 2 LiquidationClusterMatrix (estimated heatmap; refresh cadence = the TF's candle cadence, `[workspace.liquidity] cluster_refresh_secs = 0` default, operator-overridable; the matrix carries `valid_until_ms` (5-min TTL) and the frontend dims + badges it when stale — AUDIT-AIU-116). `None` when liquidity extension disabled or the refresh task cleared a stale matrix after consecutive skips. |
| `liquidity_signals` | `Vec<LiquiditySignal>` | Yes | Phase 3 derived signals (per-snapshot, computed from `liquidity` + `cluster`). **Omitted entirely when empty** via `skip_serializing_if = "Vec::is_empty"` (liquidity extension disabled or no signals fired this snapshot) — never serialized as a literal `[]`. |
| `indicators` | `map<string, IndicatorEvaluation>` | No | The unified dual-representation indicator map (see §3). |
| `context` | `MarketContext` | Yes | Synthesized per-timeframe context (see §5). |
| `alignment` | `AlignmentMatrix` | Yes | Attached Alignment Matrix (populated on completed snapshots). |
| `analysis` | `AnalysisMatrix` | Yes | Attached Analysis Matrix. |
| `risk` | `RiskMatrix` | Yes | Attached Risk Matrix. |
| `advisory` | `AdvisoryMatrix` | Yes | Attached Decision Matrix. |
| `decision_context` | `DecisionContext` | Yes | Quantitative decision metadata. |
| `opportunity` | `OpportunityMatrix` | Yes | Attached Opportunity Matrix — canonical source: 02-08-opportunity-matrix.md; null when no clear setup. |
| `statistical_context` | `StatisticalContext` | Yes | Statistical intelligence — see schema in §3.4 below. |
| `risk_profile` | `i32` | Yes | Associated risk-profile identifier (`Option<i32>` — the integer primary key of the `risk_profiles` table per [06-02-database-schema-spec.md §3.3](../integration-and-api/06-02-database-schema-spec.md)). Serialized as JSON `null`/omitted when no profile is bound. |
| `metrics_config` | `Option<MetricsConfig>` | Yes | **Configurable Data Activation** (added v6.2). Optional block recording the active indicator/signal set the cascade considered. **Omitted entirely** when the active set is the registry default (all enabled). Canonical form, semantics, and gating rules: [03-02-12-mme-configurable-activation.md §3](../engines/market-monitoring-engine/03-02-12-mme-configurable-activation.md). `metrics_config.config_version` joins PAE attribution. |

> #### 2.1.1 Single Source of Truth — v6.11 EMA Ribbon unification
> The `indicators` map is the **single canonical source of truth** for all indicator readings across the platform. The per-TF pipeline produces this map on every completed candle (`build_indicator_map()`) and refreshes it on shadow ticks via clone-based tick-safe recomputation. On the frontend, `TimeframeTelemetry.indicators` accumulates every incoming snapshot via per-key spread-merge (`{ ...prev, ...incoming }`) so close-dependent indicators (Fibonacci, patterns, S/R zones, Ichimoku, etc.) persist across shadow ticks. **Every downstream consumer — all 35 chart components, all 6 Metrics-tab facets, GroupConfluenceGrid, StructuralAnchorsStrip, ScoreChart, export JSON, and the L2–L7 synthesis layers — reads indicator values from this single accumulated map.** No consumer derives indicator values from raw OHLCV, `latestSnapshot` fields, or any secondary source. The companion `indicator_lifecycle` map (per-key `IndicatorLifecycleStatus`) is the operational sidecar that drives the Loading/Live/Stale/Failed lifecycle badges — it is always co-emitted with the indicators map as a pair, never accessed independently for valuation purposes.
>
> See the [Layer 1 Metrics spec §9.3](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) for the production-side accumulation contract, and the [Consumer Onboarding doc §3.1](../integration-and-api/06-00-consumer-onboarding.md) for consumer-side usage rules.
>
> **EMA Ribbon consumers (v6.11+).** The four surfaces that read the EMA values all share the SAME record — `MarketSnapshot.indicators["ema_stack"].values.{fast, medium, slow, long}`. There is no second computation, no second cache, no second configuration lookup. The surfaces are:
>
> | Surface | Read | Path |
> |---|---|---|
> | Metrics Layer (L1, Rust) | writes the record | `crates/market-analyzer/src/analyzer/{mod.rs:694-697, 1713-1716}` (via `inject_ema_values` in `normalize.rs:521-546`) |
> | Metrics Matrix | the record itself | `MarketSnapshot.indicators["ema_stack"].values.*` |
> | Charts tab overlay | reads per-bar series | `ui/src/components/PriceChart.svelte:336-340, 811-846` |
> | Metrics tab on-screen micro-grid | reads for collapsed `raw_value` cell on `ema_stack` row | `ui/src/components/facets/IndicatorsView.svelte` (via `buildEmaRibbonCellView` in `ui/src/lib/telemetry.ts:323-348`) |
> | Metrics tab export body `body.ema` | reads for top-level block in JSON | `ui/src/lib/exportBuilders/metricsTab.ts` → `buildEmaBlock` in `ui/src/lib/exportBuilders/shared.ts` |
>
> The `per-line distance_from_price = (close − ema) / close` and the cross-line `spread_pct = (fast − long) / close` are computed once in `ui/src/lib/telemetry.ts` (`distFromPrice`, `emaSpreadPct`) and consumed by both the on-screen cell and the export body. The 4 raw prices are shown inline on screen (collapsed `raw_value` cell — `F`/`M`/`S`/`L` rows) and in `body.ema.{fast,medium,slow,long}.value` in the export. See [04-02-01-ema-stack.md §Unified Ribbon Export](../engines/market-monitoring-engine/indicators/04-02-01-ema-stack.md) for the full contract.

> **Composite envelope.** Although the higher-order matrices (Alignment → Decision) are conceptually produced by later layers, they are attached to the completed Metrics Matrix envelope so that a single WebSocket frame carries the per-instance analytical cascade for one `(Symbol × Timeframe)` Market Instance. The composite envelope is **per-instance** (one `(symbol, timeframe_secs)` at a time); portfolio-wide aggregates (the Overview Matrix L7) are surfaced through a separate path and never ride a per-instance WS frame. The canonical sources of the attached fields are their respective layer matrices (Alignment, Analysis, Risk, Advisory, DecisionContext, StatisticalContext); the Metrics Matrix envelope is a delivery vehicle, not the canonical store.

---

## 3. Indicator Evaluation Schema

Each entry in the `indicators` map is an **`IndicatorEvaluation`** (implemented as `NormalizedIndicatorValue`). This is the dual-representation model: it carries the raw native value *and* its normalized interpretation simultaneously.

### 3.1 IndicatorEvaluation Fields

| Field | Type | Description | Evaluation Axis |
|-------|------|-------------|-----------------|
| `raw_value` | `f64` | Primary scalar in native indicator units (e.g. `RSI = 68.4`). | **Value** |
| `normalized` | `f64` | Continuous score in `[-1.0, 1.0]` (bullish positive, bearish negative). | **State / Direction / Strength** |
| `state_label` | `string` | Context-aware qualitative label (e.g. `OVERBOUGHT_DISTRIBUTION`). | **State** |
| `values` | `map<string, f64>` | Auxiliary component lines (e.g. MACD `line`/`signal`/`histogram`; Bollinger `upper`/`middle`/`lower`). Null for single-line indicators. | **Value** |
| `signals` | `IndicatorSignal[]` | Discrete signals fired on this snapshot (see §4). | (Signal projection) |
| `confidence` | `f64` | Conviction in `[0.0, 1.0]`. Base = \|`normalized`\|, boosted by confirmed signals. | **Confidence** |

### 3.2 Mapping to the 8 Indicator Evaluation Axes

The ontology defines 8 Indicator Evaluation Axes. They are derived from the `IndicatorEvaluation` fields plus registry metadata as follows:

| Axis | Source | Notes |
|------|--------|-------|
| **Value** | `raw_value`, `values` | The unaltered mathematical output. |
| **State** | `state_label` | Qualitative bucketing of the value. |
| **Direction** | sign of `normalized` | `+` bullish, `−` bearish, `≈0` flat. |
| **Strength** | \|`normalized`\| | Bucketed: Weak `<0.15`, Moderate `0.15–0.6`, Strong `0.6–0.85`, Extreme `>0.85`. |
| **Market Regime** | `MarketContext.regime` | Environmental context under which the value is interpreted. |
| **Confidence** | `confidence` | Reliability percentile `[0,1]`. |
| **Freshness** | signal `age_bars` / snapshot recency | Temporal decay of the reading. |
| **Quality** | registry `class` + signal-to-noise heuristics | Healthy / Noisy / Weak / Exceptional. |

### 3.3 Registry Binding

Every indicator key in the map corresponds to exactly one `IndicatorMeta` entry in the authoritative registry (`crates/market-analyzer/src/indicators/registry.rs`). Registry metadata carried per indicator:

| Registry Field | Purpose |
|----------------|---------|
| `key` | Map key (e.g. `rsi`, `ema_stack`). |
| `display_name` | Human label. |
| `group` | `Trend` / `Momentum` / `Volume` / `Volatility` / `Structure` / `Regime` / `Institutional` / `DerivativesData`. |
| `class` | `Leading` / `Hybrid` / `Lagging`. |
| `render` | `Pane` / `PriceOverlay` / `PriceLevels` / `Marker`. |
| `directional` | `true` = signed scoring contributor; `false` = non-directional gate. |
| `supports_divergence` | Whether this indicator can emit a nested `Divergence` signal (no separate `*_divergence` key exists). |
| `signal_types` | The `SignalKind`s this indicator may emit. |
| `default_weight` | Baseline scoring weight. |

See the [Indicator Index](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md) for the complete registry manifest (**52 entries**, **101 signal-kind declarations** — post-v6.6; the historical 101 → 100 transition is documented in [`01-01-ontology.md` Appendix B §B.3 editor's note](../conceptual-foundations/01-01-ontology.md), and the current 100 → 101 add-back reflects the v6.6 `mark_index_spread` registry entry).

#### 3.3.1 `price_trend_sharpe` — dual-representation wire format (v6.11)

The fifty-second registry entry measures single-timeframe asset-return smoothness: the **annualized Sharpe ratio of price log returns** over the trailing **300-bar window** (the indicator reaches `Live` at its 300 real-bar `bars_required`, which the `max(size/10, 50)` pipeline floor never exceeds — it can never lifecycle-lock; warm-state annualization uses the replayed closes' actual spacing, AUDIT-AIU-119).

| Wire field | Format | Meaning |
|------------|--------|---------|
| `raw_value` | `f64` | Annualized Sharpe: `mean(ln(c_t/c_{t-1})) ÷ σ(ln(c_t/c_{t-1})) × sqrt((86400/timeframe_secs) × 365)` over the trailing 300 completed closes. **v6.10.21:** clamped to ±20 and `None` when σ < 1e-9 (numerically-flat series — the Sharpe `σ → 0` pathology) so the wire can never carry an absurd value like −117. |
| `normalized` | `f64` | `(raw_value / 3.0).clamp(-1.0, 1.0)` — the ±3 Sharpe significance band maps to the unit interval. |
| `state_label` | `string` | Banded: `STRONG_POSITIVE_SHARPE` (≥ +2.0) · `POSITIVE_SHARPE` (> 0) · `NEGATIVE_SHARPE` (≤ 0) · `STRONG_NEGATIVE_SHARPE` (≤ −2.0). |
| `values` | — | Absent (single-line indicator). |
| `signals` | — | Never emitted (`signal_types = []`, `DataOnly` signal axis). |

Registry metadata: `group = Regime`, `class = Lagging`, `render = Pane`, `directional = true`, `value_format = ratio2`, `bars_required = 300`, `data_source = CandleBased`, `updates_on_shadow = false` (close-only — shadow ticks preserve the last completed-candle value via the frontend per-key merge). The value is computed by the pipeline from its rolling `close_history` window and injected into the indicator map in `crates/market-analyzer/src/analyzer/normalize.rs` after `normalize_all` (before the CA-06 retain, so a disabled `price_trend_sharpe` is filtered like every other indicator). Warm-started pipelines replay the buffer from historical closes so the reading is available at the first live close.

### 3.4 `StatisticalContext` Schema

The `StatisticalContext` sub-object is a **placeholder block** — populated on live completed frames by the SIL statistics engine (`statistical_context: Some(sil_ctx)` in `crates/market-analyzer/src/analyzer/mod.rs`, nulled pre-live and on shadow frames), but **consumed by no UI panel today** (placeholder semantics; the frontend carries the type defensively). Authoritative source: `crates/core-domain/src/models.rs::StatisticalContext`.

| Field | Type | Description |
|-------|------|-------------|
| `close_z` | `Option<f64>` | Rolling z-score of the close price against the trailing mean/σ. |
| `rsi_z` | `Option<f64>` | Rolling z-score of the RSI value. |
| `macd_z` | `Option<f64>` | Rolling z-score of the MACD histogram value. |
| `monte_carlo_expected` | `Option<f64>` | Monte Carlo expected value slot. |
| `monte_carlo_stdev` | `Option<f64>` | Monte Carlo standard-deviation slot. |

All five fields are `Option<f64>` and the block is documented as a **placeholder**: the legacy `close_zscore` / `rsi_zscore` / `macd_zscore` / `monte_carlo_expected_return` / `monte_carlo_std_dev` / `monte_carlo_sample_count` / `monte_carlo_p_value` / `window_bars` schema is retired. `Option::None` fields are omitted via `skip_serializing_if` per §6.1.

---

## 4. Signal Evaluation Schema

Each `IndicatorSignal` in an indicator's `signals` array is a discrete detected event, projected across the 10 Signal Evaluation Axes.

### 4.1 IndicatorSignal Fields

| Field | Type | Description | Axis |
|-------|------|-------------|------|
| `kind` | `SignalKind` enum | The event class (12 variants — see §4.2). | **Signal Type** |
| `direction` | `SignalDirection` | `Bullish` / `Bearish` / `Neutral`. | **Direction** |
| `status` | `SignalStatus` | `Potential` / `Confirmed` / `Active`. | **Confirmation** |
| `label` | `string` | Specific event label (e.g. `BULLISH_DIVERGENCE`). | **Signal Type** |
| `strength` | `f64` | Trigger intensity. | **Strength** |
| `age_bars` | `u32` | Completed bars since first appearance (`0` = fresh). | **Freshness** |
| `points` | `SignalPoint[]` | Pivot coordinates (used for divergence line drawing). | (rendering) |

### 4.2 The 12 SignalKind Variants

| SignalKind | Meaning | Detailed Spec |
|-----------|---------|---------------|
| `Divergence` | Price/indicator directional disagreement. | [divergence.md](../engines/market-monitoring-engine/signals/05-02-01-divergence.md) |
| `Crossover` | Two series cross (e.g. MACD line × signal). | [crossover.md](../engines/market-monitoring-engine/signals/05-02-02-crossover.md) |
| `Threshold` | Value enters a named zone (e.g. RSI ≥ 70). | [threshold.md](../engines/market-monitoring-engine/signals/05-02-03-threshold.md) |
| `Breakout` | Price breaks a structural boundary. | [breakout.md](../engines/market-monitoring-engine/signals/05-02-04-breakout.md) |
| `BandTouch` | Price contacts a channel/band edge. | [band-touch.md](../engines/market-monitoring-engine/signals/05-02-05-band-touch.md) |
| `ZeroLineCross` | Oscillator crosses its zero/mid line. | [zero-line-cross.md](../engines/market-monitoring-engine/signals/05-02-06-zero-line-cross.md) |
| `CompressionRelease` | Volatility cycle phase transition (coiling + release). | [compression-release.md](../engines/market-monitoring-engine/signals/05-02-07-compression-release.md) |
| `LevelTest` | Price tests a horizontal level (S/R, fib, pivot). | [level-test.md](../engines/market-monitoring-engine/signals/05-02-08-level-test.md) |
| `TrendFlip` | Directional regime reverses (Supertrend, PSAR). | [trend-flip.md](../engines/market-monitoring-engine/signals/05-02-09-trend-flip.md) |
| `VolumeClimax` | Abnormal volume surge. | [volume-climax.md](../engines/market-monitoring-engine/signals/05-02-10-volume-climax.md) |
| `StackChange` | EMA ribbon reorders. | [stack-change.md](../engines/market-monitoring-engine/signals/05-02-11-stack-change.md) |
| `PatternForming` | Chart/candlestick pattern detected. | [pattern-forming.md](../engines/market-monitoring-engine/signals/05-02-12-pattern-forming.md) |

### 4.3 Signal Status Semantics

Status is chosen **per-detector at emission time** — there is no server-side `POTENTIAL → CONFIRMED → ACTIVE` transition chain:

| Status | Emitted by |
|--------|------------|
| `Potential` | `Divergence` geometry detected but not yet confirmed. |
| `Confirmed` | `Divergence` (confirmed divergence state machine) and `StackChange` (EMA-stack reorder). |
| `Active` | All other detectors — emitted immediately on the triggering bar. |

`age_bars` is the **only persistence axis**: the analyzer's stateful ager re-emits surviving signals on subsequent bars with an incremented `age_bars` (or drops them when the condition lapses); a status never upgrades in place on a later bar.

---

## 5. Market Context Sub-Object

The `context` field carries the **`MarketContext`** synthesis (`crates/core-domain/src/market_context.rs`) — a per-timeframe aggregation of the indicator map into higher-level dimensions. It is meta-intelligence built on the indicators, not a standalone indicator.

### 5.0 Local-Regime vs Canonical-Regime Vocabulary

> **Vocabulary mapping.** The platform uses **two distinct regime vocabularies** at different layers — they are not interchangeable:

| Layer | Vocabulary | Cardinality | Use |
|-------|------------|-------------|------|
| L1 `MarketContext.regime` (per-timeframe coarse gating) | `COMPRESSION` / `EXPANSION` / `TRENDING` / `RANGE` | **4-state** | Local confluence / per-timeframe indicator normalization; used to gate confidence in `overall_score`. The `EXPANSION` state aligns with the L3 `MarketRegime.EXPANSION`; `COMPRESSION` here is the canonical-term counterpart to `MarketRegime.CONTRACTION` — they describe the same concept with slightly different naming conventions because `COMPRESSION` is the in-place volume/BBWP-labelled state for the per-timeframe indicator aggregator. |
| L3 `AnalysisMatrix.market_regime` (cross-TF canonical) | `TRENDING_BULL` / `TRENDING_BEAR` / `RANGE` / `ACCUMULATION` / `DISTRIBUTION` / `EXPANSION` / `CONTRACTION` / `TRANSITION` | **8-state** | Canonical regime for downstream layers (L4 opportunity, L5 risk, L6 decision, L7 overview); canonical source: [02-02-analysis-matrix.md §3.2](../matrices/02-02-analysis-matrix.md). |

The 4-state → 8-state mapping is:

| `MarketContext.regime` (L1) | Maps to (any of) `MarketRegime` (L3) |
|-----------------------------|-------------------------------------|
| `COMPRESSION` | `CONTRACTION` (volatile compression — BBWP low, choppiness high) |
| `EXPANSION` | `EXPANSION` (volatility release — BBWP high) |
| `TRENDING` | `TRENDING_BULL` / `TRENDING_BEAR` / `ACCUMULATION` / `DISTRIBUTION` (depending on directional bias) |
| `RANGE` | `RANGE` / `TRANSITION` (no directional commitment, ADX < 25, BBWP in mid-band) |

Implementations must not compare the two enum values directly across layers — always go through the L3 Analysis Matrix's `market_regime` for cross-TF / cross-layer logic.

### 5.1 MarketContext Fields

| Field | Type | Description |
|-------|------|-------------|
| `trend` | `ContextDimension` | Weighted mean of directional Trend-group indicators. |
| `momentum` | `ContextDimension` | Weighted mean of directional Momentum-group indicators. |
| `volatility` | `ContextDimension` | Magnitude from BBWP/HV (expansion vs compression). |
| `volume` | `ContextDimension` | RVOL-derived participation magnitude. |
| `liquidity` | `ContextDimension` | VWAP proximity + participation proxy. |
| `regime` | `string` (4-state) | `COMPRESSION` / `EXPANSION` / `TRENDING` / `RANGE` — local 4-state regime (see §5.0 for the cross-layer mapping). |
| `overall_score` | `i32` | Directional conviction in `[-100, 100]`. |
| `overall_label` | `string` | `STRONG_BULL` / `WEAK_BULL` / `NEUTRAL` / `WEAK_BEAR` / `STRONG_BEAR`. |

### 5.2 ContextDimension

| Field | Type | Range | Description |
|-------|------|-------|-------------|
| `score` | `f64` | `[-1.0, 1.0]` | Signed directional score (or magnitude for non-directional). |
| `confidence` | `f64` | `[0.0, 1.0]` | Mean confidence of contributing indicators. |
| `label` | `string` | — | Human-readable classification. |

### 5.3 Regime Classification Rule

```
IF   bbwp ≤ 15 OR choppiness ≥ 61.8  → COMPRESSION
ELIF bbwp ≥ 85                        → EXPANSION
ELIF adx ≥ 25 OR choppiness ≤ 38.2    → TRENDING
ELSE                                  → RANGE
```

The `overall_score` blends `trend·0.6 + momentum·0.4`, dampened by a regime gate (`TRENDING`/`EXPANSION` = 1.0, `RANGE` = 0.6, else 0.5).

---

## 6. JSON Serialization Contract

A representative completed Metrics Matrix frame (abridged). The example illustrates the JSON shape and field set; **the exact numeric values are illustrative** — the canonical per-indicator normalization formulas live in the individual indicator specifications under [indicators/](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md). Each indicator file documents the precise mapping from `raw_value` to `normalized` to `state_label` (e.g. see [04-02-11-rsi.md §Normalization](../engines/market-monitoring-engine/indicators/04-02-11-rsi.md#normalization)).

```json
{
  "exchange": "Hyperliquid",
  "symbol": "BTC-USDT",
  "timeframe_secs": 180,
  "timestamp": 1752192000,
  "is_completed": true,
  "mid_price": 64012.5,
  "bid_price": 64012.0,
  "ask_price": 64013.0,
  "open": 63890.0, "high": 64120.0, "low": 63850.0, "close": 64012.5,
  "volume": 182.4, "average_volume": 150.1,
  "indicators": {
    "rsi": {
      "raw_value": 68.4,
      "normalized": -0.42,
      "state_label": "BULLISH_MOMENTUM",
      "confidence": 0.42,
      "signals": [
        { "kind": "Threshold", "direction": "Bearish", "status": "Active",
          "label": "OVERBOUGHT_DISTRIBUTION", "strength": 0.6, "age_bars": 2 }
      ]
    },
    "macd": {
      "raw_value": 12.3,
      "normalized": 0.55,
      "state_label": "BULLISH_CROSSOVER",
      "values": { "line": 12.3, "signal": 9.8, "histogram": 2.5 },
      "confidence": 0.7,
      "signals": [
        { "kind": "Crossover", "direction": "Bullish", "status": "Confirmed",
          "label": "MACD_BULLISH_CROSSOVER", "strength": 0.8, "age_bars": 0 }
      ]
    }
  },
  "context": {
    "trend":   { "score": 0.62, "confidence": 0.71, "label": "STRONG_BULL" },
    "momentum":{ "score": 0.40, "confidence": 0.55, "label": "BULL" },
    "regime": "TRENDING",
    "overall_score": 54,
    "overall_label": "WEAK_BULL"
  }
}
```

### 6.1 Serialization Rules

- All `Decimal` price fields serialize as **plain JSON numbers** (`rust_decimal` `serde-float` feature, `crates/core-domain/Cargo.toml`) — never strings.
- `timestamp` is Unix epoch **seconds** (10-digit).
- Optional fields use `skip_serializing_if = "Option::is_none"` — absent means "not computed", never "zero".
- Empty `signals` arrays, null `values` maps, and empty `liquidity_signals` vectors are omitted to minimize frame size.
- Signal enums serialize **PascalCase** on the wire: `SignalKind` (`Threshold`, `Crossover`, …), `SignalDirection` (`Bullish` / `Bearish` / `Neutral`), `SignalStatus` (`Potential` / `Confirmed` / `Active`). `state_label` / `label` strings keep their SCREAMING display form (e.g. `OVERBOUGHT_DISTRIBUTION`, `MACD_BULLISH_CROSSOVER`).

---

## 7. Lifecycle & Guarantees

| Property | Guarantee |
|----------|-----------|
| **Immutability** | Once a completed snapshot (`is_completed = true`) is broadcast, its content is never mutated; corrections appear as a new timestamped snapshot. |
| **Determinism** | Given identical candle buffers and prior-bar state, the Metrics Matrix is byte-for-byte reproducible. |
| **Freshness** | Real-time "shadow" snapshots (`is_completed = false`) stream on every tick for live flicker; only completed snapshots feed downstream matrices. |
| **Completeness** | Every registry-enabled indicator is present in the map (as `neutral` if data is insufficient). |

---

## 8. Cross-References

- [Ontology — Evaluation Axes](../conceptual-foundations/01-01-ontology.md) — Axis definitions.
- [MME Layer 1 — Metrics](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) — Producing-layer specification.
- [Indicator Index](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md) — Full registry manifest.
- [Signals Guide](../engines/market-monitoring-engine/03-02-10-mme-signals-guide.md) — Signal detection rulebook.
- [Alignment Matrix](02-01-alignment-matrix.md) — Next-stage consumer of Metrics Matrices.
- [Database Schema](../integration-and-api/06-02-database-schema-spec.md) — Persistence of the Metrics Matrix.
