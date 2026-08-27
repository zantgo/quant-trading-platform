# DIE Layer 1 — Raw Data Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Layer:** 1 of 4
**Output Contract:** Raw Data Matrix (`NormalizedEvent` stream)
**Purpose:** This document specifies the Raw Data Layer — the lowest-level connection tier responsible for establishing and maintaining WebSocket and REST connections to external venues, managing heartbeats and rate limits, and standardizing raw network frames into a uniform event envelope.

---

## 1. Purpose

The Raw Data Layer converts venue-specific network chaos into a single, uniform, venue-agnostic event stream. It is the only tier that touches raw sockets, exchange-specific JSON schemas, and rate-limit headers.

```
[Exchange WS/REST]
       │  raw frames
       ▼
┌───────────────────────────┐
│   RAW DATA LAYER (L1)      │
│  connection pool           │
│  heartbeat / ping-pong     │
│  frame parse & normalize   │
│  rate-limit accounting     │
└───────────────────────────┘
       │  NormalizedEvent
       ▼
[Market Data Layer (L2)]
```

---

## 2. Output Contract: NormalizedEvent

The Raw Data Matrix is the `NormalizedEvent` enum (`crates/core-domain/src/normalized/mod.rs`):

| Variant | Fields | Description |
|---------|--------|-------------|
| `Trade(NormalizedTrade)` | exchange, symbol, price, size, side, timestamp_ms, trade_id | A single executed trade. |
| `OrderBook(NormalizedOrderBook)` | exchange, symbol, `bids: Vec<[Decimal; 2]>`, `asks: Vec<[Decimal; 2]>`, timestamp_ms | Level-2 order book depth snapshot. Each ladder entry is a `[price, size]` tuple. |
| `AssetContext(AssetContext)` | symbol, prev_day_px | Reference context. |
| `OpenInterest(OpenInterestEvent)` | symbol, oi | Derivatives open interest. |
| `FundingRate(FundingRateEvent)` | symbol, rate | Perpetual funding rate. |
| `MarkPrice(MarkPriceEvent)` | symbol, mark_px, index_px | Exchange-computed mark price and index price. |
| `Liquidation(LiquidationEvent)` | symbol, side, price, size, timestamp_ms | Real forced position close published by exchange. |
| `Status { .. }` | exchange, status, message | Connection lifecycle. |

All numeric fields use `Decimal` to preserve exchange precision. `timestamp_ms` is Unix epoch in milliseconds.

> **Target Architecture.** See [01-07 §1 Target architecture inventory](../../conceptual-foundations/01-07-target-architecture-roadmap.md) — "DOD hot-path (≥ 50,000 events/sec)" and "Zero-copy MME distribution". The current implementation parses raw frames into the `NormalizedEvent` enum and delivers over a bounded `mpsc` channel.

---

## 3. Connection Pool

Each registered adapter runs in an independent Tokio task spawned by the `MarketDataOrchestrator`. The pool has these properties:

| Property | Value |
|----------|-------|
| Isolation | One adapter task + one WebSocket connection per `TimeframePipeline` (symbol × timeframe); a crash in one pipeline never affects another. |
| Channel | Bounded `mpsc` channel, capacity 10,000 `NormalizedEvent`s. |
| Backpressure | When the channel is full, the adapter awaits; no events are dropped silently. |
| Symbol scoping | Each adapter subscribes only to its assigned normalized symbols (via `SymbolMapper`). |

> **Per-pipeline tasks.** `run_for_symbol` spawns the symbol's per-timeframe adapter tasks — one task plus one WebSocket connection per `TimeframePipeline` — so each (symbol, timeframe) pipeline fails and reconnects independently, and connection quality is tracked per (pair, timeframe) (see [08-05-connection-quality.md](../../operations-and-compliance/08-05-connection-quality.md)).

---

## 4. Heartbeats & Keep-Alive

Each venue adapter maintains liveness according to its protocol:

| Venue | Keep-Alive Mechanism |
|-------|----------------------|
| Hyperliquid | Periodic ping frames; subscription acknowledgements monitored. |
| Bitget | Client `ping` → server `pong` on a fixed interval; missed pongs trigger reconnect. |

A stalled stream (no frames within the venue's expected cadence) is treated as a disconnect and surfaced as a `Status { status: Disconnected }` event, triggering the supervisor's backoff loop.

---

## 5. REST Fallback

When live streaming cannot supply required history (cold start, gap recovery), the Raw Data Layer falls back to REST:

| Venue | REST Module | Primary Function |
|-------|-------------|------------------|
| Hyperliquid | `adapters/hyperliquid_rest.rs` | `fetch_historical_candles()` |
| Bitget | `adapters/bitget_rest.rs` | `fetch_historical_candles()`, `symbol_exists()` |

REST is used for **bootstrap warm-up** and **gap-filling** (see [Layer 3](03-01-04-die-layer3-data-quality.md)), never as the primary real-time feed.

---

## 6. Rate-Limiting

The Raw Data Layer respects venue rate limits through:

- **Request pacing:** REST historical fetches are chunked and spaced to stay within venue quotas.
- **Subscription batching:** WebSocket subscriptions for multiple symbols are batched into minimal frames.
- **Backoff coupling:** The supervisor's exponential backoff (1 s → 30 s, ±20 % jitter — see [08-03-connection-resilience.md §3](../../operations-and-compliance/08-03-connection-resilience.md)) doubles as a rate-limit relief valve after `429`/rejection responses.

---

## 7. Reconnection Protocol

Reconnection is owned by the `MarketDataOrchestrator` supervisor loop (see [Overview §4](03-01-01-die-overview-spec.md)):

1. Adapter `start()` returns (clean exit or error).
2. Supervisor emits a `Status` event describing the outcome.
3. On error: increment the consecutive-failed-cycle counter (a cycle = one full backoff sequence; a failure = one attempt); the counter resets if > 300 s have elapsed since the last failed cycle.
4. If consecutive failed cycles ≥ 5 → permanent disable.
5. Otherwise sleep `backoff`, then loop; `backoff = min(backoff × 2, 30)` — **canonical cap is `30 s`** (matches [08-03-connection-resilience.md §3](../../operations-and-compliance/08-03-connection-resilience.md), the canonical resilience contract, and the runtime `ReconnectPolicy::default` in `crates/network-adapters/src/adapters/resilience.rs`).

---

## 8. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Venue-agnostic output** | Downstream layers never see exchange-specific schemas. |
| **Precision preservation** | All prices/sizes carried as `Decimal`. |
| **No silent loss** | Bounded channel applies backpressure rather than dropping. |
| **Self-healing** | Automatic reconnect with capped exponential backoff. |

---

## 9. Cross-References

- [DIE Overview](03-01-01-die-overview-spec.md)
- [DIE Layer 2 — Market Data](03-01-03-die-layer2-market-data.md) — Direct consumer.
- [Systemic Data Flow — Sequence A](../../conceptual-foundations/01-03-systemic-data-flow.md)
