# Data Infrastructure Engine — Overview Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Purpose:** This document specifies the boundaries, responsibilities, layer structure, exchange adapters, performance targets, and connection-monitoring model of the Data Infrastructure Engine — the first engine in the platform's unidirectional cascade. The DIE ingests, normalizes, validates, and distributes exchange telemetry.

---

## 1. Mission & Boundaries

The DIE is the **sole ingress point** for external market data. It owns everything from raw network frames to the clean, uniform Market Data Matrix that the Market Monitoring Engine consumes. It performs **no market interpretation** — it does not compute indicators, bias, or risk.

```
[Exchange APIs] ──► DIE ──► [Market Data Matrix] ──► [MME]
```

### 1.0 Canonical glossary (DIE terminology)

| Term | Definition | Source |
|------|------------|--------|
| **Micro** | The tier name for the smallest timeframe (default 60s). One of four tiers in the ladder. | [01-04-timeframe-model.md §1](../../conceptual-foundations/01-04-timeframe-model.md) |
| **Sub-minute** | The duration class for any timeframe shorter than 60s, including user-configured micro<60s. | [08-04-candle-reconstruction.md](../../operations-and-compliance/08-04-candle-reconstruction.md) |
| **<1m** | Shorthand for the sub-minute class. Identical meaning. | [08-04-candle-reconstruction.md](../../operations-and-compliance/08-04-candle-reconstruction.md) |

The three terms refer to the same reconstruction ladder in different contexts: "micro" identifies the tier; "sub-minute" / "<1m" describes the duration class for triggering `ExponentialMovingAverage` or `LinearExtrapolation` reconstruction (see [08-04 §Two Strategies](../../operations-and-compliance/08-04-candle-reconstruction.md)). The micro tier is one minute by default; configurable below 60 s for sub-minute operation but can be configured ≥ 60s (e.g. micro300); in that case, the micro tier is **not** sub-minute and reconstruction is unnecessary.

### 1.1 Responsibilities

| In Scope | Out of Scope |
|----------|--------------|
| WebSocket connection management | Indicator computation |
| REST historical fetching / gap-filling | Bias / regime interpretation |
| Symbol normalization across venues | Order execution |
| OHLCV candle aggregation | Portfolio state |
| Data quality validation | Strategy logic |
| Broadcast distribution (NormalizedCandle channel; MarketSnapshot broadcast is MME L1) | Persistence beyond the telemetry store |

### 1.3 Operational Acceptance Criteria

The DIE meets these acceptance criteria when run with default configuration under nominal load (1 active symbol, 4-tier ladder, 1 venue):

| ID | Criterion | Verification |
|----|-----------|--------------|
| `AC-DIE-1` | Raw WS frame → `NormalizedEvent` p95 < 1 ms | `crates/network-adapters/tests/perf_ingest.rs` (Phase 1) |
| `AC-DIE-2` | Trade tick → live candle update p95 < 2 ms | `crates/market-analyzer/tests/perf_candle.rs` (Phase 1) |
| `AC-DIE-3` | End-to-end observation loop (raw frame → completed snapshot broadcast) p95 < 25 ms | `crates/api-gateway/tests/observation_loop.rs` (Phase 1) |
| `AC-DIE-4` | Sustained ingestion: ≥ 1,000 trades/sec without event-channel saturation (channel capacity 10,000) | `crates/network-adapters/tests/load_ingest.rs` (Phase 1) |
| `AC-DIE-5` | Reconnect after forced disconnect completes within 1–30 s ± 20 % jitter (3 trial average) | `crates/network-adapters/tests/orchestrator_reconnect.rs` (Phase 1) |
| `AC-DIE-6` | Permanent disable after 5 consecutive failed cycles | `crates/network-adapters/tests/orchestrator_reconnect.rs` (Phase 1) |
| `AC-DIE-7` | Drift breach detected within 3 NTP polls (≤ 90 s default) | `crates/network-adapters/tests/clock_monitor_breach.rs` (Phase 1) |
| `AC-DIE-8` | L2 candle close instant aligns to integer UTC epoch multiple to within the ≤ 100 µs drift budget | `crates/network-adapters/tests/candle_alignment.rs` (Phase 1) |
| `AC-DIE-9` | EMA reconstruction (sub-minute, ≥ 50 history) converges within `ema_window` ticks of first synthesis | `crates/network-adapters/tests/reconstruction_ema.rs` (existing) |
| `AC-DIE-10` | Composite score formula returns 100 for a perfect session and 0 for a worst-case session (no uptime, 10+ disconnects, 5 s+ reconnect, 600 s+ data loss, 100+ reconstructed candles) | `crates/network-adapters/tests/connection_quality_score.rs` (Phase 1) |

### 1.2 Layer Structure

| Layer | Name | Output |
|-------|------|--------|
| L1 | [Raw Data Layer](03-01-02-die-layer1-raw-data.md) | Standardized `NormalizedEvent` stream |
| L2 | [Market Data Layer](03-01-03-die-layer2-market-data.md) | Uniform multi-timeframe `NormalizedCandle`s |
| L3 | [Data Quality Layer](03-01-04-die-layer3-data-quality.md) | Gap-filled, validated candle sets |
| L4 | [Data Distribution Layer](03-01-05-die-layer4-data-distribution.md) | Broadcast channels to consumers |

**Dashboard tabs ↔ layers (v7.3).** The DIE dashboard tabs follow the layer order with the aggregate landing first and cross-cutting concerns last (see [07-07 §2](../../ui-ux/07-07-engine-dashboard-vocabulary.md)):

| Tab | Layer / role | Data source |
|---|---|---|
| Overview | Aggregate landing | frontend aggregation of the DIE endpoints |
| Exchange Status | L1 (raw ingestion) | `GET /api/exchange-status` |
| Connectivity | L1 (connection quality) | `GET /api/connection-quality` |
| Market Data | L2 (candle pipelines) | `GET /api/system/pipelines` |
| NTP Clock Monitor | L2 time-alignment contract | `GET /api/system/clock` |
| Data Quality | L3 | `GET /api/data-quality` |
| Distribution | L4 (egress telemetry) | `GET /api/system/distribution` |
| Settings | cross-cutting | `GET /api/system/platform-config` + `GET /api/config` |

---

## 2. Exchange Adapters

The DIE supports pluggable venue adapters conforming to the `ExchangeAdapter` trait (`crates/core-domain/src/normalized/mod.rs`):

```rust
#[async_trait]
pub trait ExchangeAdapter: Send + Sync {
    fn exchange(&self) -> Exchange;
    async fn start(
        &self,
        symbols: Vec<String>,
        event_tx: Sender<NormalizedEvent>,
        mapper: Arc<SymbolMapper>,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}
```

### 2.1 Supported Venues

| Exchange | WebSocket Endpoint | REST Fallback | Adapter |
|----------|--------------------|--------------|---------|
| **Hyperliquid** | `wss://api.hyperliquid.xyz/ws` | `hyperliquid_rest.rs` | `HyperliquidAdapter` |
| **Bitget** | `wss://ws.bitget.com/v2/ws/public` | `bitget_rest.rs` | `bitget::run_for_symbol` |

Endpoints are configured in `config.toml` (`[hyperliquid.ws_url]`, `[bitget.ws_url]`).

### 2.2 Ingested Data Types

Each adapter emits a `NormalizedEvent` enum:

| Variant | Payload | Source |
|---------|---------|--------|
| `Trade` | price, size, side, timestamp, id | Trade stream |
| `OrderBook` | bids/asks depth ladders | Level-2 order book stream |
| `AssetContext` | previous-day price | Asset context feed |
| `OpenInterest` | current OI | Derivatives feed |
| `FundingRate` | current funding rate | Derivatives feed |
| `MarkPrice` | mark_px, index_px | Ticker/mark price stream |
| `Liquidation` | symbol, side, price, size, timestamp_ms | Liquidation event stream |
| `Status` | connection lifecycle message | Adapter supervisor |

`Liquidation` events follow a dedicated persistence path: network-adapters receives liquidation WS events → DIE L1 normalizes them into `NormalizedEvent::Liquidation` (full contract in [02-10-raw-data-matrix.md §2](../../matrices/02-10-raw-data-matrix.md)) → the telemetry logger persists them to `liquidation_events` (90-day retention per [02-12-liquidity-matrix.md](../../matrices/02-12-liquidity-matrix.md)).

---

## 3. Performance Targets

| Metric | Target |
|--------|--------|
| Raw frame → NormalizedEvent | < 1 ms |
| Trade → live candle update | < 2 ms |
| Observation loop (DIE share: Raw → Market Data Matrix) | < 10 ms |
| Event channel capacity | 10,000 buffered events |
| Reconnect backoff | 1 s → 30 s (exponential, ±20 % jitter) — **supervisor budget**; see [08-03 §Retry Budgets](../../operations-and-compliance/08-03-connection-resilience.md) for the three-layer retry model |
| Permanent disable threshold | 5 consecutive cycles (supervisor) — see 08-03 for the adapter-layer `max_attempts: None` semantics |

---

## 4. Connection Monitoring & Fault Tolerance

The `MarketDataOrchestrator` (`crates/network-adapters/src/orchestrator.rs`) supervises every adapter in an independent Tokio task per `TimeframePipeline` (symbol × timeframe), each with a self-healing loop:

```
       ┌──────────────────────────────────────────────┐
       │              Adapter Supervisor               │
       │                                               │
       │  [get active symbols] ──► empty? ──► dormant  │
       │        │ non-empty                            │
       │        ▼                                      │
       │  emit Connecting ──► adapter.start()          │
       │        │                                      │
       │   ┌────┴────┐                                 │
       │   ▼         ▼                                 │
       │  Ok(())   Err(e)                              │
       │   │         │ failures++                      │
       │   │         ▼                                 │
       │   │    failures ≥ 5 ──► permanently disable   │
       │   ▼                                           │
       │  emit Disconnected ──► backoff ──► retry      │
       └──────────────────────────────────────────────┘
```

### 4.1 Fault-Tolerance Rules

These are the **supervisor-level** retry rules. The adapter-level rules (governed by `ReconnectPolicy` and `run_with_reconnect`) are documented in [08-03-connection-resilience.md](../../operations-and-compliance/08-03-connection-resilience.md) §Retry Budgets.

- **Exponential backoff:** `backoff = min(backoff × 2, max_backoff)`, with `max_backoff = 30s`, starting at 1 s, with ±20 % jitter applied **before** the cap (so the effective delay range is `[delay × 0.8, min(delay × 1.2, max_backoff)]`). See [08-03-connection-resilience.md](../../operations-and-compliance/08-03-connection-resilience.md).
- **Failure window reset:** See [08-03-connection-resilience.md](../../operations-and-compliance/08-03-connection-resilience.md) — the canonical home of the supervisor retry rules, including the failure-window reset.
- **Permanent disable:** After 5 consecutive failed **cycles** (each cycle = one full `max_attempts` retry sequence in the adapter loop), the adapter emits a terminal `Disconnected` status and the supervisor loop breaks.
- **Dormant state:** With no configured symbols, the adapter idles (polling every 2 s) rather than failing.

### 4.2 ConnectionStatus Lifecycle

The `ConnectionStatus` enum (`crates/core-domain/src/normalized/mod.rs`) has five variants. State transitions:

```
              transport error
   ┌─────────────────────────────────────┐
   │                                     │
   ▼                                     │
Connecting ──► Connected ◄────────► Disconnected
                  │  resume                │
                  │                        │ backoff elapsed
                  ▼                        ▼
              (stream)                 Reconnecting ──► Failed (after max_attempts or cancel)
                                            │
                                            │ resume (on_resume callback)
                                            ▼
                                        Connected
```

- `Connecting` — adapter is establishing the WS handshake.
- `Connected` — handshake succeeded; frames are flowing.
- `Disconnected` — transport error detected; supervisor begins the backoff loop.
- `Reconnecting` — supervisor is sleeping before the next `adapter.start()` attempt.
- `Failed` — terminal; reached only on `max_attempts` exhaustion (08-03 §Retry Budgets) or cancellation.

---

## 5. Symbol Normalization

The `SymbolMapper` (`crates/core-domain/src/normalized/symbol_mapper.rs`) maps exchange-native symbols (e.g. Hyperliquid `BTC`, Bitget `BTCUSDT`) to unified internal symbols (e.g. `BTC-USDT`). The configured `symbols` list uses `Exchange:Symbol` syntax (e.g. `Hyperliquid:BTC`) to bind each internal symbol to exactly one preferred venue. **Aggregation of parallel streams from multiple venues for the same symbol is not supported; cross-venue failover is not implemented.**

---

## 6. Configuration Surface

| Config Key | Purpose |
|------------|---------|
| `symbols` | Target instruments (`Exchange:Symbol` form). |
| `candles.duration_seconds` | Base (micro) candle duration. |
| `candles.analysis_limit` | Warm-up lookback depth. |
| `fast_timeframe` | Fast tier object: `{ duration_seconds: 180, enabled: true }` (default; see [01-04-timeframe-model.md §1](../../conceptual-foundations/01-04-timeframe-model.md)). |
| `slow_timeframe` | Slow tier object: `{ duration_seconds: 300, enabled: true }` (default). |
| `macro_timeframe` | Macro tier object: `{ duration_seconds: 900, enabled: true }` (default). |
| `hyperliquid.ws_url` / `bitget.ws_url` | Venue WebSocket endpoints. |

---

## 7. Cross-References

- [DIE Layer 1 — Raw Data](03-01-02-die-layer1-raw-data.md)
- [DIE Layer 2 — Market Data](03-01-03-die-layer2-market-data.md)
- [DIE Layer 3 — Data Quality](03-01-04-die-layer3-data-quality.md)
- [DIE Layer 4 — Data Distribution](03-01-05-die-layer4-data-distribution.md)
- [Market Data Matrix](../../matrices/02-06-market-data-matrix.md) — DIE's primary inter-engine output contract.
- [Distribution Matrix](../../matrices/02-05-distribution-matrix.md) — L4 broadcast channel schema.
- [Global Architecture](../../conceptual-foundations/01-02-global-architecture.md) — Engine positioning.
- [Systemic Data Flow](../../conceptual-foundations/01-03-systemic-data-flow.md) — Observation loop.
