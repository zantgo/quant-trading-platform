# Distribution Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Producing Layer:** Layer 4 — Data Distribution Layer
**Purpose:** This document defines the physical schema of the **Distribution Matrix** — the transport contract for validated `NormalizedCandle` frames published by DIE L4 to the Candle Aggregator for higher-timeframe rollup.

The analytical `MarketSnapshot` broadcast channel (which carries indicators, matrices, and telemetry to the MME, UI, and DB) is an **MME L1** artifact. Its channel specification, subscriber table, and wire format are documented at `03-02-02-mme-layer1-metrics.md §8`.

---

## 1. Conceptual Definition

The Distribution Layer is the DIE's **output boundary**. It receives validated candles from the Data Quality Layer and publishes them as `NormalizedCandle` frames over a per-`(symbol, timeframe)` broadcast channel consumed exclusively by the Candle Aggregator for higher-timeframe rollup.

```
[CandleQualityEnvelope] ──► DISTRIBUTION LAYER (L4) ──► [Candle Aggregator] ──► higher-TF rollup
```

---

## 2. Physical Schema

The Distribution Matrix itself is not a single data structure but a **multiplexed channel topology** for DIE's raw OHLCV transport:

| Component | Description |
|-----------|-------------|
| **Channel per `(symbol, timeframe)` pipeline** | Each `(symbol, timeframe_secs)` combination owns a dedicated `NormalizedCandle` broadcast channel. A 4-tier ladder with one symbol thus yields four channels. |
| **Candle Aggregator subscriber** | The Candle Aggregator subscribes to the micro (base) timeframe `NormalizedCandle` channel for higher-timeframe rollup. |
| **Channel capacity** | 10,000 buffered events per channel. |

The `MarketSnapshot` broadcast channel (consumed by MME L2–L7, the frontend WebSocket, and the telemetry logger) is produced by MME L1 and documented at `03-02-02-mme-layer1-metrics.md §8`.

---

## 3. Performance Targets

| Metric | Target |
|--------|--------|
| Raw frame → `NormalizedEvent` | < 1 ms |
| Trade → live candle update | < 2 ms |
| Observation loop (Raw → Market Data Matrix) | < 25 ms |
| Event channel capacity | 10,000 buffered events |
| Reconnect backoff | 1 s → 30 s (exponential, ±20 % jitter) |
| Permanent disable threshold | 5 consecutive failures |

The observation-loop latency budget decomposes as: **DIE contribution ≤ 10 ms; MME contribution ≤ 15 ms**.

---

## 4. Distribution Contract

Each DIE L4 `NormalizedCandle` frame carries the OHLCV candle with its `ReconstructionMethod` provenance flag (see [02-06-market-data-matrix.md](02-06-market-data-matrix.md)). Transport is by in-memory `Arc<NormalizedCandle>` broadcast; no independent JSON serialization is performed at this layer. The candle is serialized downstream by MME L1 as part of the `MarketSnapshot` envelope (see `03-02-02 §8.3`).

The analytical `MarketSnapshot` envelope carries the candle's quality metadata as the **full `quality_envelope`** object (`MarketSnapshot.quality_envelope: Option<CandleQualityEnvelope>`, `crates/core-domain/src/models.rs`), not as a `CandleDistributionFrame` / `CandleQualitySummary` subset — the phantom `CandleDistributionFrame` and its 3-field `quality` summary are retired:

| Envelope field | Source |
|----------------|--------|
| `quality_score` | `CandleQualityEnvelope` (0.0–100.0 composite; 100.0 = fully valid) |
| `is_valid` | Structural validity check result |
| `is_gap_filled` | Reconstructed / REST backfill marker |
| `had_outliers_rejected` | Outlier-tick rejection marker |
| `spike_detected` | Price-spike filter marker |
| `is_stale` | Last-trade staleness marker |
| `sequence_integrity` | `SequenceIntegrity` classification (`Valid` / `OutOfOrder` / `Duplicate`; wire SCREAMING: `VALID` / `OUT_OF_ORDER` / `DUPLICATE`). **Currently always `VALID` in production** — both construction sites hardcode `SequenceIntegrity::Valid`; `OutOfOrder` / `Duplicate` are reserved for future use. |
| `gap_since_last` | Seconds since the last valid candle (≤ `timeframe_secs` = continuous) |
| `validated_at` | Unix epoch of quality validation, in milliseconds |

`NormalizedCandle.trades_count` is intentionally excluded from the envelope.

```json
{
  "exchange": "Hyperliquid",
  "symbol": "BTC-USDT",
  "timeframe_secs": 60,
  "timestamp": 1752192000,
  "candle": {
    "open": 63890.0,
    "high": 64120.0,
    "low": 63850.0,
    "close": 64012.5,
    "volume": 182.4
  },
  "quality_envelope": {
    "quality_score": 100.0,
    "is_valid": true,
    "is_gap_filled": false,
    "had_outliers_rejected": false,
    "spike_detected": false,
    "is_stale": false,
    "sequence_integrity": "VALID",
    "gap_since_last": 60,
    "validated_at": 1752192000000
  }
}
```

---

## 5. Backpressure & Fault Tolerance

- **Bounded channels** prevent unbounded memory growth on consumer slowdown.
- **Broadcast lag signalling**: if a subscriber's channel approaches capacity, an internal backpressure warning is raised to the orchestrator (logged via `tracing::warn` and surfaced through `/api/system/observability`).
- **Dropped frame policy**: frames are never silently dropped; broadcast lag is logged and surfaced in observability.

---

## 6. Cross-References

- [DIE Overview](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md) — Engine boundaries and performance targets.
- [DIE Layer 4 — Data Distribution](../engines/data-infrastructure-engine/03-01-05-die-layer4-data-distribution.md) — Producing-layer specification.
- [Candle Quality Envelope](02-03-data-quality-matrix.md) — Upstream input.
- [MME Layer 1 §8 (MarketSnapshot channel)](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) — The analytical broadcast channel consumed by MME L2–L7, UI, and DB.
- [Market Data Matrix](02-06-market-data-matrix.md) — The `NormalizedCandle` schema contract.
