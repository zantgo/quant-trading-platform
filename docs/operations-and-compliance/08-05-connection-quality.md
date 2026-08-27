# Connection Quality

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Implemented

## Purpose

Aggregates raw WebSocket events (connect, disconnect, reconnect, heartbeat) into a composite quality score tracked over rolling time windows. Exposes the data via REST API for the dashboard and stores historical samples in SQLite for trend analysis.

## Three Rolling Windows

| Window | Duration | Use case |
|--------|----------|----------|
| `ONE_HOUR` | 3600 s | Real-time operational awareness |
| `SIX_HOUR` | 6 × 3600 s | Trading session quality review |
| `TWENTY_FOUR_HOUR` | 24 × 3600 s | Daily SLO / uptime reporting |

All three windows are computed **within a single 60-second tick** and persisted as three independent rows per tick (one per window, sharing the same `timestamp_ms`). The dashboard switches between them via tabs in the `ConnectionQualityPanel` component; each tab switch triggers a fresh REST request for the chosen window. The REST API returns a single `ConnectionQualityReport` per request, not an aggregate of all three.

## Data Model

```rust
pub struct ConnectionQualityReport {
    pub window: QualityWindow,         // ONE_HOUR | SIX_HOUR | TWENTY_FOUR_HOUR
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub uptime_pct: f64,                // 0..100
    pub disconnect_count: u32,
    pub avg_reconnect_ms: f64,
    pub total_data_loss_secs: u64,
    pub reconstructed_candles: u32,
    pub score: f64,                    // 0..100 composite
}
```

## Composite Score Formula

```
score = clamp(0..100,
  50 × (uptime_pct / 100)
  + 30 × (1 − min(disconnect_count / 10, 1))
  + 20 × (1 − min(avg_reconnect_ms / 5000, 1))
  − 5 × min(total_data_loss_s / 600, 1)
  − 5 × min(reconstructed_candles / 100, 1)
)
```

Interpretation:

| Term | Weight | Saturates at | Notes |
|------|--------|--------------|-------|
| `uptime_pct` | 0..50 points | 100% uptime | The dominant signal. |
| `disconnect_count` | 0..30 points | 10 disconnects | Penalised linearly up to 10, then floor. |
| `avg_reconnect_ms` | 0..20 points | 5000 ms | Saturates at 5s reconnect time. |
| `total_data_loss_secs` | 0..5 points subtracted | 600 s of data loss | Reflects sustained outage beyond what `uptime_pct` alone captures. |
| `reconstructed_candles` | 0..5 points subtracted | 100 reconstructed candles | Penalises reconstruction-heavy windows (a proxy for venue instability). |

**Worked example** (uptime=95, dc=8, rc_ms=2000, data_loss=300s, reconstructed=50 candles):
```
50 × (95 / 100)     = 47.50
30 × (1 − min(8 / 10, 1)) = 6.00
20 × (1 − min(2000 / 5000, 1)) = 12.00
− 5 × min(300 / 600, 1) = −5 × 0.5 = −2.50
− 5 × min(50 / 100, 1)  = −5 × 0.5 = −2.50

total = 47.5 + 6 + 12 − 2.5 − 2.5 = 60.5
```

A perfect session (100% uptime, 0 disconnects, 0 ms reconnect, 0 data loss, 0 reconstructed candles) scores 100.

**Saturation rationale.** The 5 s reconnect ceiling and 10-disconnect ceiling match the [08-03-connection-resilience.md §State Transitions](../operations-and-compliance/08-03-connection-resilience.md) "anything worse than this is the supervisor's problem, not the tracker's" boundary. The 600 s data-loss ceiling matches the 5-minute data-loss SLO defined here (300 s of 3600 s). The 100-reconstructed-candle ceiling reflects one full micro-tier recovery window.

## Event Sources

| Event | Source | Effect |
|-------|--------|--------|
| `Connected` | `Resilience::ReconnectState::Connected` | Reset connection-time accumulator |
| `Disconnected` | `Resilience::ReconnectState::Reconnecting` | Start data-loss timer |
| `ReconnectCompleted` | `Resilience::on_resume` callback | Stop data-loss timer, log duration |
| `Heartbeat` | WS adapter periodic tick | Detect silent drops |
| `ReconstructedCandle` | `reconstruction.rs` | Increment counter |

## Persistence

Samples are written to the `connection_quality_samples` SQLite table every 60 seconds by a background task. The table is **per-`(pair_key, timeframe_secs)`** (one Market Instance × timeframe pipeline owns one series). A cross-scope process-wide aggregate is computed on demand at query time when the API receives no scope filters.

The `pair_key` and `timeframe_secs` columns scope every sample to its owning `(instance, pipeline)`. This per-instance shape was merged at v6.0 (see `docs/CHANGELOG.md`) to back the per-instance dashboard panel; the earlier process-wide eight-column form is no longer used. See [`06-02-database-schema-spec.md §3.9`](../integration-and-api/06-02-database-schema-spec.md) for the authoritative DDL.

## REST API

```
GET /api/connection-quality?instance_id=…&timeframe_secs=…&window=one_hour|six_hour|twenty_four_hour
```

Both `instance_id` and `timeframe_secs` are **optional**. When both are supplied, the response is scoped to that `(instance_id, timeframe_secs)` pair. When either parameter is absent, the API returns a cross-scope process-wide aggregate (composite of all tracked scopes). See `06-01-api-gateway-contract.md §2.3`.

Default window: `one_hour`. Response: `ConnectionQualityReport` JSON.

Example:
```json
{
  "window": "ONE_HOUR",
  "window_start_ms": 1784131217285,
  "window_end_ms": 1784134817285,
  "uptime_pct": 100.0,
  "disconnect_count": 0,
  "avg_reconnect_ms": 0.0,
  "total_data_loss_secs": 0,
  "reconstructed_candles": 0,
  "score": 100.0
}
```

## Frontend Panel

`ConnectionQualityPanel.svelte` (with companion `.module.css` and `.test.ts`) is wired into the dashboard under the **Data Infrastructure → Overview → Connectivity** sub-tab (see [`07-02-ui-dashboard-layout.md §5.3`](../ui-ux/07-02-ui-dashboard-layout.md)). It:
- Polls `/api/connection-quality` every 30 seconds
- Switches between 1h / 6h / 24h tabs
- Color-codes the score (≥90 blue, ≥75 lighter-blue, ≥50 amber, <50 grey)
- Color-codes uptime (≥99 blue, ≥95 lighter-blue, <95 grey)

Threshold colors conform to the canonical semantic conventions at [07-06-ui-color-conventions.md](../ui-ux/07-06-ui-color-conventions.md): Blue = connected/safe, Amber = risky, Grey = error. Green is reserved for bullish market direction; red is reserved for bearish market direction.
- Shows: score, uptime, disconnect count, avg reconnect ms, total data loss, reconstructed candle count

## Testing

Backend (`connection_quality.rs`):
- 7 unit tests covering: connect/disconnect recording, uptime computation, disconnect counting, avg reconnect, score monotonicity with disconnects, reconstructed candle counting, window filtering of old events.

Frontend (`ConnectionQualityPanel.test.ts`):
- 5 tests covering: loading state, post-fetch render, tab switching, error state, score-class application.

## Cross-References

- [Connection Resilience](08-03-connection-resilience.md) — source of `ReconnectState` events
- [Candle Reconstruction](08-04-candle-reconstruction.md) — source of `reconstructed_candles` count
- [AGENTS.md §Runtime](../../AGENTS.md) — operational overview
