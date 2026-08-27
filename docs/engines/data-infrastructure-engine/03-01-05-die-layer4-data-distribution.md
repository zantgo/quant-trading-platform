# DIE Layer 4 — Data Distribution Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Layer:** 4 of 4
**Output Contract:** Distribution Matrix (high-throughput real-time channels)
**Purpose:** This document specifies the Data Distribution Layer — the tier that routes validated data to downstream consumers via asynchronous broadcast channels, with low-overhead serialization and a subscription registry.

---

## 1. Purpose

The Data Distribution Layer is the DIE's final egress. It publishes validated `NormalizedCandle` frames to the Candle Aggregator for higher-timeframe rollup. The `MarketSnapshot` channel (which feeds the MME L2–L7, the UI, and the telemetry logger) is an MME L1 artifact — see [03-02-02 §8](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md).

```
[validated candle / snapshot]
       │
       ▼
┌─────────────────────────────────┐
│   DATA DISTRIBUTION LAYER (L4)   │
│  per-timeframe broadcast channels│
│  subscription registry           │
│  low-overhead serialization      │
└─────────────────────────────────┘
       │           │            │
       ▼           ▼            ▼
   [MME]      [WS clients]  [telemetry logger]
```

---

## 2. Broadcast Channel Model

Distribution uses a Tokio `broadcast` channel per `(symbol, timeframe)` pipeline — a single producer, many independent consumers, each with its own cursor.

| Property | Value |
|----------|-------|
| Topology | One `NormalizedCandle` broadcast channel per `(symbol, timeframe)` pipeline. |
| Fan-out | Unlimited subscribers; each receives every frame. |
| Lag handling | Slow consumers receive `RecvError::Lagged(n)`; they resynchronize rather than blocking the producer. |
| Payload | Completed `NormalizedCandle` frames (with `ReconstructionMethod` provenance flag). |

### 2.1 Producer / Consumer Decoupling

The producer (candle generator) never awaits a specific consumer. A crashed or slow subscriber cannot stall ingestion — it simply misses frames and re-subscribes at the current head. This satisfies the platform's **Decoupled Producer/Consumer** principle.

**Shared-state caveat.** The "zero shared state" framing is aspirational; in practice, state shared across the engine boundary is held in `Arc<…>` containers owned by `RegistryContext` (see [01-06-crate-layout-and-cycles.md §3.2](../../conceptual-foundations/01-06-crate-layout-and-cycles.md)). The actual invariant is *no mutable shared state without synchronisation*: shared containers are read-only after construction, or guarded by Tokio primitives (`RwLock`, `Mutex`, atomic counters). The decoupled-API guarantee is that a slow consumer cannot block the producer; it is not that no state is shared.

> **Target Architecture.** See [01-07 §1](../../conceptual-foundations/01-07-target-architecture-roadmap.md) — "Zero-copy MME distribution". The target design splits distribution into two explicitly different formats: internal distribution (to MME) bypasses JSON serialization, routing raw zero-copy binary memory slices; external distribution (to Frontend / DB) uses the canonical JSON-RPC 2.0 schemas (§4). *Current implementation:* the internal DIE L4 path broadcasts cloned `NormalizedCandle` structs; the external `MarketSnapshot` path (MME L1, see [03-02-02 §8](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md)) uses JSON-RPC 2.0 as documented in [06-01-api-gateway-contract.md](../../integration-and-api/06-01-api-gateway-contract.md).

---

## 3. Subscription Registry

Consumers subscribe by `(symbol, timeframe_secs)`. The L4 layer maintains a single `NormalizedCandle` broadcast channel per `(symbol, timeframe)` pipeline, carrying raw OHLCV candles (and `ReconstructionMethod` provenance). This is the DIE L4 transport, consumed by the Candle Aggregator for higher-timeframe rollup.

| Consumer | Subscription | Transport |
|----------|-------------|-----------|
| Candle aggregator | Base timeframe (micro) closes. | Dedicated `NormalizedCandle` broadcast receiver. |

The `MarketSnapshot` broadcast channel (which carries indicators, matrices, and telemetry) is an MME L1 artifact produced by the MME analyzer pipeline. Its transport specification, subscriber table (MME L2-L7, telemetry logger, frontend WebSocket), and serialization contract live at [03-02-02-mme-layer1-metrics.md §8](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md).

---

## 4. Serialization

### 4.1 Wire Format

The `NormalizedCandle` is serialized as part of the broader `MarketSnapshot` payload by MME L1 (see [03-02-02 §8](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md)). DIE L4 transports `NormalizedCandle` as in-memory Rust structs via `Arc<NormalizedCandle>` broadcast; no independent JSON serialization is performed at this layer.

### 4.2 Low-Overhead Rules

| Rule | Effect |
|------|--------|
| In-memory only | `NormalizedCandle` frames are passed as `Arc<…>` references; no serialization overhead at L4. |
| `ReconstructionMethod` provenance | Attached to candle; `None` when not reconstructed — `skip_serializing_if` applied downstream by MME L1. |

---

## 5. Distribution Guarantees

| Property | Guarantee |
|----------|-----------|
| **Non-blocking** | A slow/failed subscriber never stalls the producer. |
| **At-most-once per cursor** | Each subscriber sees each frame at most once; lag is signalled explicitly. |
| **Ordering** | Frames within a channel are delivered in production order. |
| **Immutability** | Once broadcast, a completed `NormalizedCandle` is never mutated (see [L3 §3 Late-trade arrival rule](./03-01-04-die-layer3-data-quality.md)).

### 5.1 Operational Acceptance Criteria

The L4 (Data Distribution Layer) layer meets these criteria when run with default configuration under nominal load. L4 code executes physically in `market-analyzer` (per [01-06 §1](../../conceptual-foundations/01-06-crate-layout-and-cycles.md)); tests are co-located with the implementation.

| ID | Criterion | Verification |
|----|-----------|--------------|
| `AC-L4-1` | Broadcast fan-out is `O(subscribers)` and non-blocking; a 1 s sleep on one subscriber does not delay other subscribers or the producer. | `crates/market-analyzer/tests/broadcast_fanout.rs` (Phase 1) |
| `AC-L4-2` | Lagged consumer receives `RecvError::Lagged(n)` within 1 frame of falling behind; the consumer can resubscribe at the current head. | `crates/market-analyzer/tests/broadcast_lag.rs` (existing) |

## 6. Performance Targets

| Metric | Target |
|--------|--------|
| Broadcast dispatch | O(subscribers), non-blocking |

---

## 7. Cross-References

- [DIE Layer 3 — Data Quality](03-01-04-die-layer3-data-quality.md) — Input.
- [MME Layer 1 — Metrics §8 (MarketSnapshot broadcast channel)](../market-monitoring-engine/03-02-02-mme-layer1-metrics.md) — The analytical `MarketSnapshot` channel specification.
- [DIE End-to-End Flow](03-01-00-die-end-to-end-flow.md) — How the `NormalizedCandle` channel fits in the overall DIE cascade.
