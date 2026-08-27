# DIE Layer 2 — Market Data Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Layer:** 2 of 4
**Output Contract:** Market Data Matrix (multi-timeframe `NormalizedCandle`s)
**Purpose:** This document specifies the Market Data Layer — the tier that transforms the raw event stream into structured, uniform temporal boundaries (OHLCV candles) across multiple timeframes, and audits time-sync latency.

---

## 1. Purpose

The Market Data Layer converts irregular, event-based ticks into regular, time-boxed OHLCV bars. It answers *"what did the market do in each fixed interval?"* and produces the multi-timeframe candle set that is the foundation of all downstream analysis.

```
[NormalizedEvent: Trade] ──► MARKET DATA LAYER (L2) ──► [NormalizedCandle × N timeframes]
                               CandleGenerator (per-tick)
                               CandleAggregator (rollup)
```

---

## 2. Output Contract: NormalizedCandle

```rust
struct NormalizedCandle {
    exchange: String,          // originating venue (e.g. "Hyperliquid", "Bitget")
    symbol: String,            // unified internal symbol (e.g. "BTC-USDT")
    start_time_ms: u64,        // candle open time, Unix epoch milliseconds
    duration_ms: u64,          // candle duration, milliseconds
    open: Decimal, high: Decimal, low: Decimal, close: Decimal,
    volume: Decimal,
    trades_count: u64,
    reconstructed: Option<ReconstructionMethod>,  // provenance flag (see 08-04-candle-reconstruction.md)
}
```

> **Wire/struct mapping.** The canonical `MarketSnapshot` JSON uses `timestamp` (close time, epoch ms) and `timeframe_secs` (duration, seconds). The struct field `start_time_ms` is the **open** time and the struct carries the duration as `duration_ms` (milliseconds). The equivalence is: `wire.timestamp = start_time_ms + duration_ms` (close time) and `wire.timeframe_secs = duration_ms / 1000` — see the field-name registry in [02-06-market-data-matrix.md §2](../../matrices/02-06-market-data-matrix.md). All consumers derive close time from `(start_time_ms, duration_ms)` rather than storing it redundantly.

**`average_volume` is NOT a field of `NormalizedCandle`.** `average_volume` is the MME-side rolling average volume baseline (see [02-07-metrics-matrix.md §2.1](../../matrices/02-07-metrics-matrix.md)). L2 never emits it. The distinct per-candle quantity `volume / trades_count` is named `avg_trade_size` and is not part of the candle contract.

Every candle carries `assert_validity()` invariants (enforced downstream): `high ≥ low`, `open`/`close ∈ [low, high]`, `volume ≥ 0`.

> **Target Architecture.** See [01-07 §1](../../conceptual-foundations/01-07-target-architecture-roadmap.md) — "AoS → SoA candle history". The current candle history is an **Array of Structures (AoS)** — a collection of `NormalizedCandle` structs with `Decimal` OHLCV fields. The target hot-path model replaces this with a **Structure of Arrays (SoA)** so that all historical values of a field reside contiguously in the CPU cache for downstream indicator loops:
>
> ```rust
> pub struct ContiguousCandleHistory {
>     pub opens:   [f64; 1000],
>     pub highs:   [f64; 1000],
>     pub lows:    [f64; 1000],
>     pub closes:  [f64; 1000],
>     pub volumes: [f64; 1000],
>     pub write_ptr: usize,
> }
> ```
>
> The AoS `NormalizedCandle` contract above remains authoritative for the current implementation.

---

## 3. Real-Time Candle Generation

The `CandleGenerator` (`crates/market-analyzer/src/candle_generator.rs`) builds candles tick-by-tick from a single ordered `Trade` stream. It assumes ordered input from the L1 channel (the L1 mpsc preserves the order in which the adapter emitted events) and does **not** perform sequence auditing: no chronological reorder, no late-tick detection, no dedup. The single-stream generator emits one candle at a time, but if a trade arrives out-of-order it produces a candle whose `open/high/low/close` may not match the global truth.

L3 (Data Quality Layer, [03-01-04-die-layer3-data-quality.md](./03-01-04-die-layer3-data-quality.md)) owns sequence auditing across the candle stream and runtime gap detection. L2 owns single-stream candle generation; L3 owns cross-stream integrity. The boundary is:

- L2 receives a single ordered `Trade` stream and produces a single ordered `NormalizedCandle` stream. Any per-candle invariant (OHLCV validity, shadow/completed distinction, UTC alignment) is L2's responsibility.
- L3 receives the L2 candle stream plus the local DB and exchange REST history. Any cross-stream operation (dedup against REST, late-tick drop, missing-bar detection, source-mix classification) is L3's responsibility.

On each trade the L2 generator returns a tuple `(Option<completed>, live)`:

| Situation | Behaviour |
|-----------|-----------|
| First trade | Initializes the candle; returns `(None, live)`. |
| Trade within current interval | Updates high/low/close/volume/count; returns `(None, live)`. |
| Trade crosses interval boundary | Emits the completed candle; opens a new one; returns `(Some(completed), live)`. |
| Trade whose `timestamp_ms` is earlier than the current candle's open | L2 emits a candle update based on the out-of-order tick. L3 drops it on the audit pass (§3 of L3). |

### 3.1 Interval Alignment

Candles align to epoch buckets:

$$\text{interval\_start} = \left\lfloor \frac{\text{timestamp\_ms}}{\text{duration\_ms}} \right\rfloor \times \text{duration\_ms}$$

For example, a 60 s candle for a trade at `123456 ms` aligns to `120000 ms`.

**UTC boundary map** (closing instant of each candle, by tier): candle boundaries are *exact epoch-duration multiples of UTC*. `micro60` closes at `:00.000` of every minute (the next minute's start); `fast180` closes at `:03:00.000`, `:06:00.000`, `:09:00.000`, … (top of every third minute); `slow300` closes at `:05:00.000`, `:10:00.000`, `:15:00.000`, … (top of every fifth minute); `macro900` closes at `:00:00.000`, `:15:00.000`, `:30:00.000`, `:45:00.000` of every hour. The aggregator formula `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms` deterministically produces these boundaries, so candles always close on the integer epoch multiple, never at a `.999` sub-second offset.

**Clock-drift budget:** Local server system clocks execute continuous NTP polling to keep local time drift under $\le 50 \text{ microseconds}$ of UTC, ensuring locally computed indicator values match exchange historical benchmarks to the millisecond. Implemented in `crates/network-adapters/src/clock_monitor.rs` (spawned from `main.rs`; configured via the `[clock_monitor]` section of `config.toml`). See [Global Architecture §2.1](../../conceptual-foundations/01-02-global-architecture.md) and [Timeframe Model §3.1](../../conceptual-foundations/01-04-timeframe-model.md).

### 3.2 Live "Shadow" Candles

The `live` candle returned on every tick is the **shadow** value — the real-time flickering state of the in-progress candle. It powers live dashboard updates but does **not** feed downstream matrices; only the `completed` candle (`is_completed = true`) triggers the analytical cascade.

---

## 4. Multi-Timeframe Aggregation

The platform monitors four timeframes per instance. The base (micro) timeframe is generated directly from ticks; higher timeframes are rolled up.

### 4.1 Standard Timeframe Ladder

| Tier | Default Duration | Source |
|------|------------------|--------|
| Micro | 60 s | Direct from ticks (`CandleGenerator`). |
| Fast | 180 s | Rollup / dedicated generator. |
| Slow | 300 s | Config `[slow_timeframe]`. |
| Macro | 900 s (configurable, e.g. 3600 s / 86400 s) | Config `[macro_timeframe]` + `CandleAggregator`. |

### 4.2 Higher-Timeframe Aggregation

The `CandleAggregator` (`crates/market-analyzer/src/candle_aggregator.rs`) rolls the base micro candle stream into the configured `fast`, `slow`, and `macro` timeframe buckets. The duration of each tier is configured in `config.toml` (`[fast_timeframe.duration_seconds]`, `[slow_timeframe.duration_seconds]`, `[macro_timeframe.duration_seconds]`); the default ladder is micro 60 s / fast 180 s / slow 300 s / macro 900 s, but other ladders (e.g. 4h, 1d) are produced the same way when configured.

```
1m close ──► process_1m_candle() ──► (Option<fast>, Option<slow>, Option<macro>)
             │
             ├─ update pending_<tf>: high=max, low=min, close=latest, volume+=, count+=
             └─ on interval rollover: emit completed <tf> candle, reset pending
```

Aggregation preserves OHLCV integrity: `high = max(highs)`, `low = min(lows)`, `close = last close`, `volume = Σ volumes`, `trades_count = Σ counts`.

---

## 5. Time-Sync Latency Audit

Because venues timestamp differently and network jitter varies, the Market Data Layer tracks:

| Audit | Surface name | Purpose |
|-------|--------------|---------|
| Ingest-vs-event skew | `ingest_skew_ms` | Difference between local receipt time and `timestamp_ms`. |
| Interval boundary drift | `observation_loop_latency_ms` (per-candle) | Ensures candles close on aligned epoch boundaries regardless of tick arrival jitter; reported per completed candle. |
| Cross-venue offset | (internal; not yet surfaced) | When a symbol can be sourced from multiple venues, timestamp offsets are reconciled to the unified clock. |

Latency measurements surface through the `/api/system/status` endpoint under three distinct fields: `observation_loop_latency_ms` (end-to-end raw-frame-to-broadcast, per [03-01-01 §3](./03-01-01-die-overview-spec.md)), `ingest_skew_ms` (per-trade receipt skew), and `system_heartbeat_latency_ms` (most recent WS control frame round-trip). The single ambiguous `latency_ms` field used in earlier versions is deprecated in v6.0; see [06-01 §2.8](../../integration-and-api/06-01-api-gateway-contract.md).

---

## 6. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Uniform intervals** | Every completed candle spans exactly `duration_ms` aligned to epoch buckets. |
| **OHLCV integrity** | Aggregation and generation preserve open/high/low/close/volume semantics. |
| **Shadow separation** | Live shadow candles never contaminate the completed-candle analytical path. |
| **Determinism** | Identical trade sequences yield identical candle sets. |

### 6.1 Operational Acceptance Criteria

The L2 (Market Data Layer) layer meets these criteria when run with default configuration under nominal load:

| ID | Criterion | Verification |
|----|-----------|--------------|
| `AC-L2-1` | Candle close instant is exactly `interval_start + duration_ms` for every completed candle. | `crates/market-analyzer/tests/candle_alignment.rs` (existing) |
| `AC-L2-2` | Multi-timeframe rollup preserves OHLCV invariants: `high = max(highs)`, `low = min(lows)`, `close = last close`, `volume = Σ volumes`, `trades_count = Σ counts`. | `crates/market-analyzer/tests/candle_aggregator.rs` (existing) |
| `AC-L2-3` | Shadow candles never appear in the MME pipeline (verified by absence in `MarketSnapshot.is_completed = true` filter). | `crates/market-analyzer/tests/shadow_separation.rs` (Phase 1) |
| `AC-L2-4` | `average_volume` is the MME-side rolling baseline ([02-07-metrics-matrix.md §2.1](../../matrices/02-07-metrics-matrix.md)); the per-candle `volume / trades_count` ratio is `avg_trade_size` and is not emitted. | `crates/market-analyzer/tests/average_volume_derivation.rs` (Phase 1) |

---

## 7. Cross-References

- [DIE Layer 1 — Raw Data](03-01-02-die-layer1-raw-data.md) — Input.
- [DIE Layer 3 — Data Quality](03-01-04-die-layer3-data-quality.md) — Validation & gap-fill.
- [Metrics Matrix](../../matrices/02-07-metrics-matrix.md) — Ultimate consumer of candles.
