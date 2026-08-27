# DIE Layer 3 — Data Quality Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Layer:** 3 of 4
**Output Contract:** CandleQualityEnvelope + PipelineReliabilityMetrics (validated, gap-filled candle sets + reliability metrics)
**Purpose:** This document specifies the Data Quality Layer — the tier that enforces data integrity through REST gap-filling, sequence auditing, and median-based outlier filtering. It transforms raw candle streams into sanitized, trustworthy datasets.

---

## 1. Purpose

The Data Quality Layer guarantees that downstream analysis operates on **complete, ordered, clean** data. It reconciles the local telemetry store against exchange REST history, fills gaps, and rejects anomalous ticks.

```
[NormalizedCandle stream] ─┐
[Local telemetry store   ] ─┼──► DATA QUALITY LAYER (L3) ──► [validated candle set]
[Exchange REST history   ] ─┘        gap-fill · audit · filter
```

---

## 2. DB-First / REST-Gap-Fill Algorithm

Warm-up and gap recovery use the local-DB-first strategy implemented in `crates/portfolio-supervisor/src/registry/bootstrap.rs::collect_candles()`. In v6.5 the bootstrap is refactored behind the `HistoricalFetchPolicy` trait (see [03-01-07-die-historical-fetch-policy.md](./03-01-07-die-historical-fetch-policy.md)) and obeys the sub-minute / ≥ 1 minute behavior split from [08-08-candle-buffer-spec.md](../../operations-and-compliance/08-08-candle-buffer-spec.md):

```
target_count = candle_buffer.size    # CB-01, default 500

db_candles = query_recent_candles(symbol, secs, target_count)   # local warm base (SECONDARY)

IF secs < 60:
    rest_candles = []                          # HFP-03 — sub-minute bypasses REST
ELSE:
    rest_candles = await HistoricalFetchPolicy.fetch({           # HFP-04..HFP-06
        target_count, end_ts = now_ms,
        ...
    })                                          # paginated REST until target reached

merged = sort_desc(dedup(rest_candles ++ db_candles, start_time_ms))    # HFP-09
        .take(target_count)                                            # CB-03 cap
```

The behavior split is **uniform across all exchanges** — both Hyperliquid and Bitget implement the same `HistoricalFetchPolicy` trait; the sub-minute short-circuit lives in the trait caller (HFP-03), so there is no per-exchange divergence. The DB-precedence rule (newer DB wins on overlap) is preserved from the v6.4 behavior.

### 2.1 Strategy Properties

| Property | Rationale |
|----------|-----------|
| **Exchange-independent contract** | The same algorithm runs against every exchange via the `HistoricalFetchPolicy` trait ([03-01-07](03-01-07-die-historical-fetch-policy.md)). Per-adapter pagination rules (HFP-05 Hyperliquid, HFP-06 Bitget) are hidden behind the trait surface. |
| **Sub-minute bypass** | `timeframe_secs < 60` short-circuits to empty Vec (HFP-03); the platform does not fetch exchange history for sub-minute timeframes because both exchanges coerce sub-minute to 1m, producing duration-mismatched candles. |
| **Paginated ≥ 1 minute** | `timeframe_secs ≥ 60` paginates the exchange REST endpoint until `target_count` is reached or the exchange reports no more history. Hyperliquid uses backward `startTime` cursors; Bitget uses forward cursors with `limit=200` per page. |
| **DB-precedence on overlap** | When DB rows and REST rows overlap by `start_time_ms`, the DB row wins — the local store is authoritative for already-persisted candles. |
| **30 s fetch timeout** | REST pagination is bounded by `[candle_buffer] fetch_timeout_ms` (HFP-10, default 30 000 ms). Partial results are accepted; the pipeline enters `LOADING` until the buffer fills from live candles. |
| **Single source of truth** | `target_count = [candle_buffer] size` is the canonical buffer length; the previous `analysis_limit` is removed (v6.5 migration; AUDIT-V7-300). |

#### 2.1.1 Startup bootstrap (sub-minute path)

The startup cascade (invoked from `crates/portfolio-supervisor/src/registry/bootstrap.rs::fetch_and_warm_bootstrap`) for sub-minute timeframes is:

1. **HistoricalFetchPolicy.fetch returns empty Vec** (HFP-03). The platform does not request historical candles from the SQLite cache or from any exchange REST endpoint.
2. The pipeline is constructed with an **empty buffer** and enters `CandlePipelineState::LOADING` (see [03-01-06-die-candle-pipeline-states.md §DCP-04](03-01-06-die-candle-pipeline-states.md) and [08-08 §CB-05](../../operations-and-compliance/08-08-candle-buffer-spec.md)).
3. Live tick ingestion begins immediately; candles accumulate one-by-one as their buckets close.
4. Indicators report `IndicatorLifecycleState::Loading` until each one reaches its `bars_required` (see [03-02-15-mme-indicator-lifecycle-states.md](../market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)). The pipeline transitions `LOADING → LIVE` after the buffer reaches `candle_buffer.size` entries (DCP-04).
5. **Cold-start duration:** `candle_buffer.size × timeframe_secs` of wall-clock time from cold start (e.g. 300 × 15 s = 75 minutes for a 15-second micro TF). This is **expected behavior** and is visible to operators via the `tf.pipeline_state` field on every emitted `MarketSnapshot`.

The behavior is intentionally distinct from the v6.4 implementation, which silently requested 1m exchange candles and warmed sub-minute pipelines with mismatched `duration_ms` values — a structural correctness bug documented in [08-08 §1](../../operations-and-compliance/08-08-candle-buffer-spec.md) and fixed by HFP-03.

#### 2.1.2 Live gap-fill (sub-minute path)

After startup, the `GapDetector` (see [08-04-candle-reconstruction.md §Gap Detection](../../operations-and-compliance/08-04-candle-reconstruction.md)) decides whether reconstruction is required for a runtime gap. Sub-minute runtime gaps follow the same EMA-preferred / linear-fallback ladder as startup, but:

- The trigger is `GapDetector.detect_gap(last_persisted_ts_ms, now_ms, gap_threshold_secs)` rather than a DB-empty check.
- The reconstructed candles are tagged with `ReconstructionMethod` and forwarded through the aggregator (per [08-04 §Forwarding](../../operations-and-compliance/08-04-candle-reconstruction.md)).
- New ticks arriving during reconstruction are buffered; indicator state is not updated until reconstruction completes (see [08-04 §Reconnect sequencing](../../operations-and-compliance/08-04-candle-reconstruction.md)).

The two paths share the EMA/linear thresholds (≥50 / ≥2) but differ in entry condition and ownership: §2.1.1 is invoked once at instance creation; §2.1.2 is invoked per detected runtime gap.

---

## 3. Sequence Auditing

The layer verifies candle-set ordering and continuity:

| Check | Action on Violation |
|-------|---------------------|
| **Chronological order** | Candles sorted oldest-first before use. |
| **Duplicate detection** | Candles with identical `start_time_ms` deduplicated (local preferred over REST). |
| **Missing bar** | A hole between consecutive `start_time_ms` values ≠ `duration_ms` flags a gap for REST recovery. |
| **Late-trade arrival** | A trade whose `timestamp_ms` falls inside a previously-closed interval is **dropped** at L3 and counted in the `out_of_order_dropped` reliability metric. The completed candle is immutable per the L4 Distribution Layer invariant (see [03-01-05-die-layer4-data-distribution.md §5](./03-01-05-die-layer4-data-distribution.md)). Retroactive reordering is forbidden: a broadcast `MarketSnapshot` may never be mutated after the **MME L1 producer** emits it (see [03-02-02-mme-layer1-metrics.md §8](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md)). |

---

## 4. Outlier Filtering

Anomalous ticks (fat-finger prints, venue glitches) are rejected before they contaminate candles:

### 4.1 Median Price Filter

For an incoming tick price `p` against a rolling window of recent prices:

```
median = median(recent_window)
IF |p − median| / median  >  outlier_tolerance:
    reject tick   (do not update candle)
```

This suppresses single-print spikes while preserving genuine fast moves (which persist across multiple ticks and shift the median).

> **Bootstrap behaviour (v2.1 — clarification).** The rolling window is initialised lazily. For the first `N = median_window_size` ticks (default `20`, configurable via `config.toml` `[quality.median_window_size]`), every tick is accepted unfiltered — the warm-up mode allows the window to fill before the filter evaluates normally. From tick `N + 1` onward the median filter evaluates against the prior `N` ticks. The current tick is appended to the window **after** the filter check (not before), so the filter cannot reject its own input. When the median is exactly zero (rare but possible on a venue reset), the filter is bypassed for that tick and a debug-level log entry is emitted. The window is monotonically expanded during warm-up; ticks observed during warm-up are still written to the candle and propagated downstream — only their filter rejection is deferred.
>
> **Filter parameters (canonical defaults).**
>
> | Parameter | Default | Config key | Type |
> |-----------|---------|------------|------|
> | `median_window_size` | 20 | `[quality.median_window_size]` | `usize` |
> | `outlier_tolerance` | 0.05 (5% deviation from rolling median) | `[quality.outlier_tolerance]` | `f64` (raw decimal fraction; 0.05 = 5%) |
> | `bypass_on_zero_median` | true | `[quality.bypass_on_zero_median]` | `bool` |
>
> A tick is rejected when `|p − median| / median > outlier_tolerance`. The bypass-on-zero-median prevents division-by-zero on a venue reset (rare but observed); bypassed ticks are logged at debug level and counted under `outliers_bypassed` in `PipelineReliabilityMetrics`.
>
> **Target Architecture.** See [01-07 §1](../../conceptual-foundations/01-07-target-architecture-roadmap.md) — "DOD hot-path" row. The rolling median price filter and standard-deviation outlier calculations would execute over the contiguous `f64` price arrays resident in the CPU cache. *Current implementation:* these checks run over `Decimal`/`VecDeque`-style windows.

### 4.2 Structural Validity

Every candle passes `NormalizedCandle::assert_validity()`:

```
high ≥ low
open  ∈ [low, high]
close ∈ [low, high]
volume ≥ 0
```

Candles failing validity are quarantined and refetched from REST.

---

## 5. Pipeline Reliability Metrics

These are **per-instance** operational metrics (not the per-candle `CandleQualityEnvelope` envelope defined in [02-03-data-quality-matrix.md](../../matrices/02-03-data-quality-matrix.md)). They are served via `GET /api/data-quality` (`crates/api-gateway/src/handlers/data_quality.rs`; see [06-01-api-gateway-contract.md §2.11](../../integration-and-api/06-01-api-gateway-contract.md)) and roll up to the dashboard's Data Quality panel.

| Metric | Meaning |
|--------|---------|
| Coverage | Fraction of expected bars present after gap-fill. |
| Gap count | Number of holes detected and repaired. |
| Outliers rejected | Count of ticks filtered this session. |
| Source mix | Ratio of DB-warm vs REST-gap vs live candles. |
| `out_of_order_dropped` | Trades dropped because their `timestamp_ms` fell inside a previously-completed interval (§3). Persisted in-memory; surfaced through `GET /api/data-quality`; lost on restart. |

**Naming disambiguation.** The per-candle envelope documented in [02-03-data-quality-matrix.md](../../matrices/02-03-data-quality-matrix.md) is `CandleQualityEnvelope` (formerly called the Data Quality Matrix). It rides the `MarketSnapshot` and contains `quality_score: f64 ∈ [0, 100]`. The metrics in this section are `PipelineReliabilityMetrics` — a per-instance roll-up, not a per-candle annotation. The two are complementary: `CandleQualityEnvelope` evaluates one candle's validity; `PipelineReliabilityMetrics` measures the sanitization pipeline's health.

---

## 6. Bootstrap Warm-Up

At instance startup, `fetch_and_warm_bootstrap()` (`crates/portfolio-supervisor/src/registry/bootstrap.rs`) feeds the sanitized historical candle set into the MME warm-up pipeline (`analyzer/warm.rs`), producing a `WarmedPipelineState` so the pipeline emits fully-formed Metrics Matrices from the first live candle rather than after a cold warm-up. The DIE does not compute indicators itself (see [03-01-01-die-overview-spec.md §1](./03-01-01-die-overview-spec.md) "Mission & Boundaries"); it produces validated candle histories and hands them to the MME for indicator warm-up.

### 6.1 `WarmedPipelineState`

`WarmedPipelineState` is the per-`(symbol, timeframe)` handoff record produced at the end of a successful bootstrap and consumed by the live pipeline on the first tick. One instance is built **per configured tier** by `warm_indicators_for_timeframe(candles, tf_config, fib_config, symbol, timeframe_secs)` — there is no single cross-timeframe struct; the per-tier fan-out lives in the caller (`fetch_and_warm_bootstrap()` invokes the warm-up once per configured timeframe).

The authoritative Rust definition lives in `crates/market-analyzer/src/analyzer/warm.rs`. Its canonical shape (abridged — the full struct holds ~40 warmed indicator state machines):

```rust
pub struct WarmedPipelineState {
    // Fully-warmed indicator instances (EMA stack, RSI, MACD, ADX, Squeeze,
    // Bollinger, ATR, BBWP, Stochastic, ChandeMO, Supertrend, Keltner,
    // Donchian, OBV, CMF, MFI, HV, Aroon, Choppiness, LinReg, ZScore,
    // divergence detectors, PivotPoints, Candlestick, Ichimoku, CCI, PSAR,
    // Williams %R, HullMA, AO, ForceIndex, StdDevChannel, VolumeProfile,
    // SMC, AnchoredVwap, …), e.g.:
    pub ema_fast: Ema,
    pub rsi_14: Rsi,
    // …

    /// VWAP session accumulators.
    pub vwap_sum_tp_vol: Decimal,
    pub vwap_sum_vol: Decimal,
    /// Rolling volume baseline feeding the MME-side `average_volume`.
    pub volume_history: VecDeque<Decimal>,
    /// Indicator-ready candle history (capped at `[candle_buffer] size`, default 500).
    pub history: Vec<NormalizedCandle>,
    /// Support/resistance role-tracker state.
    pub sr_tracker: SrRoleTracker,
    /// Most recent snapshot + rolling snapshot history for warm replay.
    pub latest_snapshot: Option<MarketSnapshot>,
    pub snapshot_history: Vec<MarketSnapshot>,
    // …
}
```

The bootstrap path (physically hosted in `portfolio-supervisor`; see §2) collects the validated candle history and replays it through every indicator state machine in chronological order, so the first live candle lands on fully-warmed state. Warm-up sufficiency is governed by the cold-start minimums of [08-04-candle-reconstruction.md §Cold-start minimums](../../operations-and-compliance/08-04-candle-reconstruction.md): indicators emit `state_label = INSUFFICIENT_DATA` / `confidence = 0.0` until their per-indicator minimum buffers fill.

---

## 7. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Completeness** | Gaps are filled before candles reach the analyzer. |
| **Ordering** | Candle sets are strictly chronological and deduplicated. |
| **Cleanliness** | Outlier ticks and invalid candles are rejected. |
| **Warm start** | Indicators are pre-warmed from validated history. |
| **Buffer invariance (v6.5)** | The in-memory candle history is rolled at exactly `candle_buffer.size` entries (CB-03). On every completed candle the new candle is pushed back; if the deque would exceed `size` the oldest entry is popped front. There is no grow-then-trim mode — the deque never exceeds `size`. |
| **Exchange independence (v6.5)** | The bootstrap algorithm produces the same buffer shape on every exchange via `HistoricalFetchPolicy`. Sub-minute bypasses historical fetch (HFP-03); ≥ 1 minute paginates to exactly `size` (HFP-04 … HFP-06). |
| **Lifecycle visibility (v6.5)** | Every `MarketSnapshot` carries a `tf.pipeline_state` and a per-indicator `indicator_lifecycle` map so the dashboard can show the warm-up progress in real time ([03-01-06](03-01-06-die-candle-pipeline-states.md), [03-02-15](../market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)). |

### 7.1 Operational Acceptance Criteria

The L3 (Data Quality Layer) layer meets these criteria when run with default configuration under nominal load:

| ID | Criterion | Verification |
|----|-----------|--------------|
| `AC-L3-1` | Median warm-up accepts every tick for the first `median_window_size = 20` ticks; from tick 21 onward the filter evaluates against the prior 20. | `crates/network-adapters/tests/median_warmup.rs` (existing) |
| `AC-L3-2` | Tick rejection: a tick whose `|p − median| / median > 0.05` is dropped and counted in `outliers_rejected`. | `crates/network-adapters/tests/median_filter.rs` (Phase 1) |
| `AC-L3-3` | Late ticks (timestamp earlier than a previously-completed candle) are dropped at L3 and counted in `out_of_order_dropped`. | `crates/network-adapters/tests/late_tick_drop.rs` (Phase 1) |
| `AC-L3-4` | Median = 0 (venue reset) bypasses the filter for that tick and logs at debug level. | `crates/network-adapters/tests/median_zero_bypass.rs` (Phase 1) |
| `AC-L3-5` | Every completed candle passes `assert_validity()` before reaching L4; quarantine and re-fetch from REST on failure. | `crates/market-analyzer/tests/candle_validity.rs` (existing) |
| `AC-L3-6` | `CandleQualityEnvelope.quality_score` for a fully-valid candle (no gap, no spike, no staleness, valid integrity) equals 100. | `crates/market-analyzer/tests/quality_score.rs` (existing) |

---

## 8. Cross-References

- [DIE Layer 2 — Market Data](03-01-03-die-layer2-market-data.md) — Input.
- [DIE Layer 4 — Data Distribution](03-01-05-die-layer4-data-distribution.md) — Downstream.
- [MME Overview](../market-monitoring-engine/03-02-01-mme-overview-spec.md) — Warm-up consumer.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `market_snapshots` warm base.
