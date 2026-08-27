# 02-12: LiquidityMatrix — Real Liquidation Flow (Phase 1)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Producer:** DIE L1 (NormalizedEvent::Liquidation via WS liquidation events; persisted by the telemetry logger to liquidation_events, 90-day retention) → MME L1.5 (per-candle aggregation)
**Consumer:** MME L5 (Risk) — `cascade_risk` dimension; MME L6 (Decision) — Advisory rationale; Overview — cross-symbol aggregate
**Per-bar:** yes (computed on every completed candle)
**Snapshot field:** `MarketSnapshot.liquidity: Option<LiquidityFlow>`

The LiquidityMatrix carries the **per-candle aggregate of real liquidation
events** observed on the exchange WebSocket during the current bar. It
is the ground-truth signal — every field is derived from published
exchange data, not estimated.

## 1. Why this exists

Liquidations are the **only major market microstructure event that
exchanges publish in near-real-time**. They are observable. They are
loud. They mark inflection points. The platform needs to track them
faithfully so cascade detection, risk scoring, and cluster estimation
have a real input rather than an estimated one.

## 2. Data sources

- **Hyperliquid**: subscribe to `userFills` channel. Each fill entry
  has a `liquidation` field; non-empty values are force-closed positions.
- **Bitget**: subscribe to `fill` channel. Entries with `execType == "L"`
  are liquidations.

The raw events are persisted to `liquidation_events` (90-day
retention, enforced by hourly cleanup in the telemetry logger).

## 3. Schema

```rust
pub struct LiquidityFlow {
    pub long_liquidations_usd: f64,         // sum since last completed candle
    pub short_liquidations_usd: f64,        // sum since last completed candle
    pub net_liquidation_usd: f64,           // long - short; +ve = longs dumped
    pub event_count: u32,
    pub largest_event_usd: f64,
    pub largest_event_price: Option<f64>,
    pub largest_event_side: Option<LiquidationSide>,
    pub cascade_state: CascadeState,
    pub cascade_intensity: f64,             // 0..100
    pub recent_real_buckets: BTreeMap<i64, RealLiquidationBucket>, // rolling 24h price-bucketed observed liquidations
}

pub enum CascadeState {
    None,
    Detected,    // 1+ significant events (z-score over last 50 event notionals)
    Sustained,   // >= cascade_sustained_events significant events
    Exhausted,   // bar intensity declining after elevated state
}

pub enum LiquidationSide { Long, Short }
```

`recent_real_buckets` is the **observed** liquidation heatmap: price-bucketed (`bucket_index = round((price / mid − 1) / bucket_size_pct)` with `bucket_size_pct` default `0.001`) notional aggregation across the rolling 24h retention window (`heatmap_retention_secs`, default 86 400), keyed by a packed `(bucket_index, side)` i64. Each bucket carries `price_low`, `price_high`, `peak_price`, `notional_usd`, `event_count`, `last_updated_ms`. It is display-only (frontend heatmap layering over the estimated cluster matrix); `cascade_risk` and all decision math consume the per-bar totals above. Omitted from the wire when empty.

## 4. Sign convention

`net_liquidation_usd = long_liquidations_usd - short_liquidations_usd`

- **Positive** = more longs got dumped = bearish pressure (longs were
  forced sellers, adding to the sell side).
- **Negative** = more shorts got dumped = bullish pressure (short
  squeeze; shorts were forced buyers).

## 5. Cascade state machine

The accumulator runs a rolling window of recent events for event-rate context. For each completed bar, `cascade_intensity` is computed by the log-scaled ratio formula above, then `cascade_state` is derived from a **z-score over the last 50 events' notionals** (`mean + cascade_detected_zscore × σ`, default z-score 2.5): `≥ cascade_sustained_events` (default 3) significant events → `Sustained`; one or more → `Detected`; elevated intensity with a declining window → `Exhausted` (see the "Relationship to `cascade_state`" block above for the exact thresholds).

## Cascade Intensity Computation (`LiquidityFlow.cascade_intensity`)

The `cascade_intensity` field is the **canonical risk-feed value** consumed by `RiskMatrix.cascade_risk` (see [02-11-risk-matrix.md §4.8](../matrices/02-11-risk-matrix.md)) and surfaced on the Frontend's `LiquidityPanel` (§[07-04-ui-liquidity-panel-spec.md Flow tab](../ui-ux/07-04-ui-liquidity-panel-spec.md)). This section is the **single canonical specification** of how the value is computed. The implementation lives in `crates/core-domain/src/liquidity/mod.rs::LiquidityEventAccumulator::compute_intensity`; the equations below mirror that implementation 1:1.

### Formula (log-scaled ratio)

```
total    = long_liquidations_usd(current_bar) + short_liquidations_usd(current_bar)
baseline = $1,000                                                       // no window history yet
         | mean(rolling_intensity) × 1,000 + 1                          // otherwise (USD scale)

intensity = clamp(ln(total / baseline) × 20, 0, 100)
```

- The **rolling window** is the last `cascade_window_candles` completed bars' intensities (`rolling_intensity` deque; default `5` via `LiquidityEventAccumulator::new`, `max(2)` floor). When the deque is empty (no prior bars), the baseline is the fixed `$1,000` — a single event is then significant.
- The baseline is mapped **back to USD** (`mean(rolling_intensity) × 1,000 + 1`) because the rolling window stores intensities, not notionals.
- `ln(ratio) × 20` maps a 1× ratio to `0`, ~`e² ≈ 7.4×` to `40`, and `e⁵ ≈ 148×` to `100`; the `clamp` guarantees the field stays in `[0, 100]` per the schema.
- No z-score, no warm-up gate, and no `50 + z × 12.5` mapping — the legacy W = 200 baseline-window / K = 20 recent-event window spec is retired (there is no `cascade_baseline_window_bars` or `cascade_min_warmup_bars` config).

### Warm-up reset behavior

- **Initial cold start** (no completed micro candles): `cascade_intensity = 0.0` and `cascade_state = None`.
- **Empty bar** (no liquidation events in the bar): `cascade_intensity = 0.0`.
- **First event bar** (no window history): baseline `$1,000` — any notional at or above `1,000 × e = $2,718` reads `intensity ≥ 20`.

### Relationship to `cascade_state`

`cascade_state` is derived from a **genuine z-score over the last 50 liquidation events' notionals** (event-level, not per-bar): a significant event is one whose notional exceeds `mean(notionals) + cascade_event_zscore × σ(notionals)` (config `cascade_detected_zscore`, default `2.5`; a flat window with `σ ≈ 0` falls back to `mean × cascade_event_zscore`, minimum `$500`). `≥ cascade_sustained_events` (default `3`) significant events → `Sustained`; `1 … sustained−1` → `Detected`; no significant events with `cascade_intensity > 30` and a non-empty window → `Exhausted` (bar was hot, window shows decline); else `None`. `cascade_intensity` (continuous 0..100) is published on every candle; `cascade_state` advances only when these discrete thresholds are crossed.

### Consumer contract

- **`RiskMatrix.cascade_risk` (L5)** consumes `cascade_intensity` directly as `score = max(score, flow.cascade_intensity)` (see [02-11-risk-matrix.md §4.8](../matrices/02-11-risk-matrix.md)). The discrete `cascade_state` adds a risk premium on top of the intensity (`+15` for `Detected`, `+30` for `Sustained`, `+0` for `Exhausted`).
- **`LiquidityPanel`** displays `cascade_intensity` numerically (0..100) and color-codes it relative to the per-bar thresholds (blue ≤ 30, amber ≤ 60, red > 60) for the operator's situational awareness (see [07-04-ui-liquidity-panel-spec.md Flow tab](../ui-ux/07-04-ui-liquidity-panel-spec.md)). Red = bearish cascade pressure, amber = moderate risk, blue = calm/safe (see canonical conventions at [07-06-ui-color-conventions.md](../ui-ux/07-06-ui-color-conventions.md)).

## 5.1 Frontend exposure

`MarketSnapshot.liquidity` rides the WebSocket frame to the frontend
under `liquidity`. The LiquidityPanel (Phase 4) renders the Flow tab
from this field.

## 6. Configuration Surface

The Liquidity Intelligence configuration is set in `config.toml` under the `[liquidity]` table. The canonical configuration surface is defined by `crates/config-models/src/models.rs::LiquidityConfig`; the TOML below mirrors that struct exactly.

```toml
[liquidity]
enabled = true
liquidation_feed = true
cluster_estimation = true
signals = true
mark_price_poll_ms = 60000
event_retention_days = 90
bucket_retention_days = 7
cluster_refresh_secs = 300
maintenance_margin_rate = 0.005
cascade_detected_zscore = 2.5
cascade_sustained_events = 3
funding_extreme_pct = 0.0005
magnet_activation_distance_pct = 0.5
liquidity_vacuum_threshold = 0.3
oi_funding_divergence_pct = 2.0
min_cluster_notional_usd = 50000
```

| Field | Default | Description |
|------|---------|-------------|
| `enabled` | `true` | Master switch for the Liquidity Intelligence extension. |
| `liquidation_feed` | `true` | Liquidation-event feed ingestion switch. |
| `cluster_estimation` | `true` | Cluster estimator switch. |
| `signals` | `true` | Phase 3 liquidity-signal derivation switch. |
| `mark_price_poll_ms` | `60000` | Hyperliquid mark-price / OI / funding polling cadence. |
| `event_retention_days` | `90` | Raw `liquidation_events` retention. |
| `bucket_retention_days` | `7` | Aggregated bucket retention. |
| `cluster_refresh_secs` | `0` | Cluster matrix refresh interval. `0` = synchronize with the TF's own candle cadence (per-TF matrices since v6.4.2 — see [03-02-11 §refresh](../../docs/engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md)). A non-zero value overrides to a fixed interval. |
| `maintenance_margin_rate` | `0.005` | Industry-standard 0.5 % maintenance margin for perpetuals. |
| `cascade_detected_zscore` | `2.5` | Z-score (over the last 50 events' notionals) above which a single event triggers `Detected` state. |
| `cascade_sustained_events` | `3` | Min significant events to escalate to `Sustained`. |
| `funding_extreme_pct` | `0.0005` | Funding rate extreme threshold (0.05 % / 8 h). |
| `magnet_activation_distance_pct` | `0.5` | Distance from mid that activates a cluster magnet (0.5 %). |
| `liquidity_vacuum_threshold` | `0.3` | Liquidity-vacuum detection threshold. |
| `oi_funding_divergence_pct` | `2.0` | OI/funding divergence percentage. |
| `min_cluster_notional_usd` | `50000` | Minimum cluster notional (USD) below which a bin is noise and dropped. |

> **Configuration key alignment (v2.1 — canonical).** A previous version of this table used different keys (`cascade_z_score_threshold`, `cascade_sustained_min_events`, `cluster_refresh_interval_secs`, `funding_flip_threshold_pct`, `cascade_rolling_window_bars`, `oi_divergence_window_bars`) that did not match the runtime `LiquidityConfig` struct or the [01-05-liquidity-domain.md §Configuration](../conceptual-foundations/01-05-liquidity-domain.md) source-of-truth block. The corrected surface above is the single canonical configuration; the runtime enforces it via `serde` deserialization in `crates/config-models/src/models.rs`. **The phantom keys `funding_refresh_ms`, `cascade_baseline_window_bars`, and `cascade_min_warmup_bars` are retired** — they never existed on `LiquidityConfig` (the cascade-intensity window is the hardcoded `cascade_window_candles = 5` default, not a config key).