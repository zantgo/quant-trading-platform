# Snapshot Export — On-Disk JSON Schema

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved

<!-- pascal-display-strings -->

This document specifies the on-disk JSON shape produced by the snapshot-export scheduler. Each file
under `<output_path>/<YYYY-MM-DD>/<HHhMMmSSs>/` is one `(pairKey, slot, tab)` triple at one wall-clock
instant. See [`../operations-and-compliance/08-09-snapshot-export.md`](../operations-and-compliance/08-09-snapshot-export.md)
for the operator manual and the file-layout diagram.

---

## 1. Top-level envelope

Every snapshot file is a single JSON document with this top-level shape:

```ts
interface SnapshotFile {
  snapshot_metadata: SnapshotMetadata;
  payload: unknown;       // per-tab; see §3 below
}

interface SnapshotMetadata {
  /// UTC timestamp at which the tick fired (RFC 3339).
  datetime_utc: string;
  /// Epoch millis — convenient for time-series joins.
  timestamp_ms: number;
  /// Tab id — see §3 for the canonical list.
  tab: string;
  /// Pair-key, e.g. `"BTC-USDT"`.
  pair_key: string;
  /// TF slot — from the snapshot's own `TimeframeSlot` (`micro` / `fast` / `slow` / `macro`).
  timeframe_slot: string;
  /// TF in seconds — the snapshot's ACTUAL configured duration (never a
  /// hardcoded 60/300/900/3600; a non-default `micro=1s` or `macro=1800s`
  /// config produces its real value here).
  timeframe_secs: number;
}
```

The `payload` field is the per-tab matrix as it appears on the wire. **It is NOT the same JSON the
per-tab export buttons in the GUI emit.** The GUI builders (`metricsTab.ts`, `mtfTab.ts`,
`analysisTab.ts`, …) reconstruct dashboard-chrome-specific payloads from live store state — they
wrap the underlying matrix in `meta`/`header` chrome, reshape fields for panel rendering, and
normalize display strings. The scheduler instead serializes the **raw DTO snapshot** captured at
tick time: the `metrics` tab is the entire `MarketSnapshot`, and every other tab is the
serde-serialized matrix (or the small synthetic wrappers of §3) with no GUI chrome attached.
Consumers must treat the two surfaces as sibling but distinct encodings of the same underlying
state.

---

## 2. Filename conventions

| Component | Convention | Example |
|---|---|---|
| `pair_key` | Same as the live `MarketSnapshot.symbol` field | `BTC-USDT` |
| `timeframe_slot` | `micro` / `fast` / `slow` / `macro` | `slow` |
| `tab` | Canonical id (§3) | `alignment` |
| Filename | `{sanitized_pair_key}.{slot}.{tab}.json` | `BTC_USDT.slow.alignment.json` |

`sanitize` replaces every non-`[A-Za-z0-9]` character with `_`. Pair-keys always contain `-`, so
the rename is the only sanitisation applied.

---

## 3. Tab catalogue

The 9 tabs are exhaustive — every per-TF matrix the engine publishes is covered. Each tab maps to a
canonical id used in (a) the on-disk filename, (b) the `tab` field of `snapshot_metadata`, and (c)
the `tabs` array of `SnapshotExportConfig`.

| Tab id | Source matrix | Payload shape |
|---|---|---|
| `metrics` | `MarketSnapshot` itself (the full per-TF record) | The entire `MarketSnapshot` object — every per-TF field. Largest of the 9 payloads. |
| `mtf` | Multi-timeframe wrapper | Small synthetic object joining the per-TF records: `{ slot, timeframe_secs, indicators, alignment, analysis, advisory, decision_context }`. `slot` is a string taken from the snapshot's `TimeframeSlot` (e.g. `"micro"`); `timeframe_secs` is the snapshot's actual configured duration; `indicators` is the indicator **count** for that TF (`snap.indicators.len()` — the field is named `indicators`, not `indicators_count`). |
| `alignment` | `AlignmentMatrix` | The 10-dimension × score/state/confidence matrix + per-TF rows + consensus score. See [`../matrices/02-01-alignment-matrix.md`](../matrices/02-01-alignment-matrix.md). |
| `opportunity` | `OpportunityMatrix` | The setup-quality + ranked opportunities + confluent levels. See [`../matrices/02-08-opportunity-matrix.md`](../matrices/02-08-opportunity-matrix.md). |
| `risk` | `RiskMatrix` | The 8-dimension risk matrix + per-dimension confidences. See [`../matrices/02-11-risk-matrix.md`](../matrices/02-11-risk-matrix.md). |
| `analysis` | `AnalysisMatrix` | Bias / regime / phase / quality + supporting & contradicting signals. See [`../matrices/02-02-analysis-matrix.md`](../matrices/02-02-analysis-matrix.md). |
| `advisory` | `AdvisoryMatrix` | Directional guidance + strategy environment + entry/exit + protection/target + final recommendation. Schema documented in [`../matrices/02-04-decision-matrix.md §2.1`](../matrices/02-04-decision-matrix.md). |
| `decision` | `DecisionContext` | Score / bias / `score_confidence` / expected R:R / trade readiness / contributing indicators. See [`../matrices/02-04-decision-matrix.md`](../matrices/02-04-decision-matrix.md). |
| `recommendation` | Synthetic | Small wrapper combining `advisory` + `decision_context` — the same view the GUI's Recommendation panel renders. |

---

## 4. Worked example

A tick at `2026-08-13T14:30:05.123Z` on a 3-instance workspace (BTC, ETH, SOL × 4 TF slots × all 9
tabs) writes 108 files. Two of them are:

### 4.1 `2026-08-13/14h30m05s/BTC_USDT.slow.alignment.json`

```json
{
  "snapshot_metadata": {
    "datetime_utc": "2026-08-13T14:30:05.123456+00:00",
    "timestamp_ms": 1755090605123,
    "tab": "alignment",
    "pair_key": "BTC-USDT",
    "timeframe_slot": "slow",
    "timeframe_secs": 900
  },
  "payload": {
    "symbol": "BTC-USDT",
    "timeframes_present": 4,
    "dimensions": [
      { "score": 65.0, "state": "Bullish", "confidence": 0.65 },
      { "score": 50.0, "state": "Neutral", "confidence": 0.50 },
      ...
    ],
    "mtf_trend_alignment": 0.45,
    "mtf_momentum_alignment": 0.30,
    "mtf_volume_alignment": 0.10,
    "mtf_volatility_alignment": -0.20,
    "mtf_overall_score": 35.0,
    "mtf_overall_label": "WEAK_BULL_MTF",
    "timeframe_alignments": [ ... ],
    "signal_cross_tf_count": 2,
    "trend_agreement_pct": 72.0
  }
}
```

### 4.2 `2026-08-13/14h30m05s/ETH_USDT.fast.recommendation.json`

```json
{
  "snapshot_metadata": {
    "datetime_utc": "2026-08-13T14:30:05.123456+00:00",
    "timestamp_ms": 1755090605123,
    "tab": "recommendation",
    "pair_key": "ETH-USDT",
    "timeframe_slot": "fast",
    "timeframe_secs": 300
  },
  "payload": {
    "advisory": {
      "symbol": "ETH-USDT",
      "directional_guidance": "Long",
      "strategy_environment": "TrendFollowing",
      "confidence_assessment": 68.0,
      ...
    },
    "decision_context": {
      "symbol": "ETH-USDT",
      "score": 75.0,
      "bias": "Bullish",
      "confidence": 0.7,
      "trade_readiness": "READY",
      ...
    }
  }
}
```

---

## 5. Data-science ingestion patterns

### 5.1 Polars / Pandas

```python
import polars as pl
import json, glob

rows = []
for path in glob.iglob("snapshots/**/*.alignment.json", recursive=True):
    with open(path) as f:
        d = json.load(f)
    rows.append({
        "ts": d["snapshot_metadata"]["timestamp_ms"],
        "pair": d["snapshot_metadata"]["pair_key"],
        "slot": d["snapshot_metadata"]["timeframe_slot"],
        "score": d["payload"]["mtf_overall_score"],
        "label": d["payload"]["mtf_overall_label"],
        "agreement_pct": d["payload"]["trend_agreement_pct"],
    })

df = pl.DataFrame(rows).sort("ts")
```

### 5.2 DuckDB

```sql
-- Single-table view over all tabs (json + per-tab fields).
SELECT
    snapshot_metadata.timestamp_ms   AS ts,
    snapshot_metadata.pair_key      AS pair,
    snapshot_metadata.timeframe_slot AS slot,
    snapshot_metadata.tab           AS tab,
    payload                         AS payload_json
FROM read_json_auto(
    'snapshots/*/*/*.json',
    format = 'array'
);
```

---

## 6. Forward-compatibility

The snapshot-export task is additive — new fields may appear on the `payload` (e.g. future matrix
versions) without breaking existing consumers. The 9 canonical `tab` ids are stable; if a tab is
removed in a future release, the scheduler emits no file for it (rather than a placeholder).

Consumers should treat unknown top-level fields as future-proofing and ignore them.
