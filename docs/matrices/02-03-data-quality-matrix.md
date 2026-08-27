# Candle Quality Envelope Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Produced by DIE Layer 3 validation logic (executed inline in market-analyzer) and attached to the MME MarketSnapshot envelope.
**Producing Layer:** Layer 3 — Data Quality Layer (DIE); layer name in code: `CandleQualityEnvelope`
**Purpose:** This document defines the physical schema of the **per-candle quality envelope** — the integrity-checked candle with attached validity metadata that rides the `MarketSnapshot` payload. (v6.0 renamed the document from "Data Quality Matrix Specification" to "Candle Quality Envelope Specification" to disambiguate it from the per-instance `PipelineReliabilityMetrics` documented in [03-01-04 §5](../engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md).)

---

## 1. Conceptual Definition

The Data Quality Layer audits the Market Data Matrix output for integrity before it reaches downstream consumers. It detects missing candles, stale ticks, out-of-order sequences, and price spikes, producing sanitized data paired with per-candle validity metadata (this document) and per-instance reliability metrics (see [03-01-04 §5](../engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md) for `PipelineReliabilityMetrics`).

```
[Market Data Matrix] ──► DATA QUALITY LAYER (L3) ──► [Candle Quality Envelope + Pipeline Reliability Metrics] ──► [Distribution Layer (L4)]
```

---

## 2. Physical Schema

| Field | Type | Description |
|-------|------|-------------|
| `candle` | `NormalizedCandle` | The validated candle (from Market Data Matrix) — rides alongside the envelope on the snapshot; **not** a nested field of `quality_envelope`. |
| `is_valid` | `bool` | Whether the candle passed all structural validity checks (`high ≥ low`, etc.). |
| `is_gap_filled` | `bool` | `true` if this candle was synthetically filled (no data for this interval). |
| `is_stale` | `bool` | `true` if the candle's last trade timestamp exceeds the staleness threshold. |
| `spike_detected` | `bool` | `true` if a price spike (outlier tick) was rejected during this candle. |
| `had_outliers_rejected` | `bool` | Deprecated alias for `spike_detected`; retained for backward compatibility. |
| `sequence_integrity` | `SequenceIntegrity` | `VALID` / `OUT_OF_ORDER` / `DUPLICATE`. |
| `quality_score` | `f64` | Overall reliability metric in `[0, 100]`. |
| `gap_since_last` | `u64` | Seconds since the last valid candle (≤ timeframe_secs = continuous). |
| `validated_at` | `u64` | Unix epoch of quality validation, in **milliseconds** (consistent with the canonical timestamp unit defined in [02-06-market-data-matrix.md §2](02-06-market-data-matrix.md)). |

---

## 3. JSON Serialization Contract

The quality envelope rides **inside** `MarketSnapshot.quality_envelope`
on every completed snapshot (it is not a standalone frame with a nested
`candle` — the candle travels alongside as the snapshot's OHLCV fields):

```json
{
  "quality_score": 100.0,
  "is_valid": true,
  "is_gap_filled": false,
  "had_outliers_rejected": false,
  "spike_detected": false,
  "is_stale": false,
  "sequence_integrity": "VALID",
  "gap_since_last": 60,
  "validated_at": 1752192001000
}
```

> `is_valid` mirrors the candle-validity gate (`false` when the validity
> assertion fails, which also drives `quality_score = 0`).
> `sequence_integrity` is currently always `VALID` at both production
> construction sites — `OUT_OF_ORDER` / `DUPLICATE` are reserved for the
> future sequence-audit path.

---

## 4. Quality Score Computation

The `quality_score` is derived from:

$$\text{quality\_score} = 100 - 20 \cdot (\text{is\_gap\_filled} ? 1 : 0) - 30 \cdot (\text{is\_stale} ? 1 : 0) - 10 \cdot (\text{spike\_detected} ? 1 : 0)$$

The score is clamped to $[0, 100]$. An invalid candle (failing `assert_validity()`) is scored at 0 regardless of other factors. A score below 50 triggers a warning. Scores below 30 may cause the candle to be suppressed from downstream consumers.

**Weight rationale:** Gap-filled (reconstructed/REST-backfilled) candles are less reliable than live data (-20). Staleness (no recent trades) is the strongest negative signal (-30) as it indicates the price may be outdated. Spike detection (outlier rejection) has the smallest penalty (-10) since it reflects successful quality filtering rather than data degradation.

---

## 5. Cross-References

- [DIE Overview](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md) — Engine boundaries and quality layer description.
- [DIE Layer 3 — Data Quality](../engines/data-infrastructure-engine/03-01-04-die-layer3-data-quality.md) — Producing-layer specification.
- [Market Data Matrix](02-06-market-data-matrix.md) — Upstream input.
- [Distribution Matrix](02-05-distribution-matrix.md) — Downstream consumer.
