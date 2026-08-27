# 03-02-13: MME Volume Profile Layer (L2.6 — Volume Profile Distribution)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**New layer:** L2.6 (Volume Profile Distribution)
**Existing layers:** unchanged — extends the existing L1 indicator pipeline with a chart-overlayable volume distribution.

This document specifies the per-timeframe **Volume Profile** snapshot that
the analyzer computes on every completed candle and the chart primitive that
renders it on the price chart as a stacked buy/sell histogram on the right
edge.

## Purpose

A volume profile answers the question "where did the volume happen?" in a
way that the bottom-of-chart volume bars cannot. By aggregating volume by
price level across a rolling window of completed bars, the trader can see
where the market has accepted prices (high-volume nodes / POC) and where it
has rejected them (low-volume nodes).

Unlike the liquidation heatmap (L2.5), which is a forward-looking
*magnet* indicator, the volume profile is a historical *acceptance*
indicator. They are complementary: the heatmap shows where forced selling
*could* happen, the volume profile shows where participants *already*
transacted.

## Inputs

- Loaded OHLCV candle history (default 500 bars, configurable via
  `volume_profile_window` in `[indicators]`).
- The same `completed` candle the L1 indicator pipeline already feeds.

## Outputs

`MarketSnapshot.volume_profile: Option<VolumeProfileSnapshot>` (per-TF,
populated only when the indicator has accumulated at least half its window).

### `VolumeProfileSnapshot`

```rust
pub struct VolumeProfileSnapshot {
    pub symbol: String,
    pub timeframe_slot: String,        // "micro" | "fast" | "slow" | "macro"
    pub timeframe_secs: u64,
    pub bins: Vec<VolumeProfileBin>,    // sorted ascending by price_low
    pub poc_price: f64,                 // midpoint of the highest-volume bin
    pub value_area_high: f64,           // upper edge of the 70% VA
    pub value_area_low: f64,            // lower edge of the 70% VA
    pub total_volume: f64,
    pub range_low: f64,
    pub range_high: f64,
    pub num_bins: usize,                // count of non-empty bins AFTER the zero-volume filter
    pub timestamp_ms: u64,
}

pub struct VolumeProfileBin {
    pub price_low: f64,
    pub price_high: f64,
    pub volume: f64,                    // total = buy_volume + sell_volume
    pub buy_volume: f64,
    pub sell_volume: f64,
    pub is_poc: bool,
    pub is_value_area: bool,
}
```

## Computation

### Static bin count

The bin count is **static**, taken from the `volume_profile_bins` config key (default `100`, floored at `1` inside `VolumeProfile::new` so a misconfigured `0` cannot panic the `range / num_bins` division). The snapshot builder does **not** re-derive the count from price range, tick size, or bar duration — `num_bins` on the wire is the count of **non-empty bins after filtering** (`out_bins.len()` after the `volume <= 0.0` bins are dropped in `build_volume_profile_snapshot`), which may be less than the configured count.

> **`dynamic_bin_count()` is dead code.** `VolumeProfileSnapshot::dynamic_bin_count(price_range, tick_size, bar_duration_secs)` (the tick-size / bar-duration adaptive formula, clamped `[30, 120]`) still exists in `crates/core-domain/src/volume_profile.rs` — **with no production callers**; it survives only for its unit tests. The legacy "30–120 adaptive bin count" behavior claims are retired.

### Per-candle distribution

For each completed candle, the analyzer distributes its `volume` across
the bins its `high..low` range spans, weighted by overlap fraction. The
per-bin total is the sum of all overlapping candles.

### Buy/sell split

Without taker-side data, the buy/sell split is approximated from candle
direction:

| Candle close vs open | Attribution |
|---|---|
| `close > open` | 100% buy_volume |
| `close < open` | 100% sell_volume |
| `close == open` (doji) | 50/50 |

This is the standard approximation used by TradingView and other platforms
when only OHLCV is available. When tick-level trade data becomes
available, the buy/sell attribution should be replaced with taker-buy /
taker-sell aggregates.

### POC and value area

Same algorithm as the existing `VolumeProfile` indicator:

1. POC = bin with maximum total volume.
2. Value Area = bins centered on POC that contain ≥ 70% of total volume.
3. The walk expands outward from POC, picking the adjacent bin with the
   larger volume first (continuing until the 70% threshold is met).

## Refresh cadence

The snapshot is recomputed on every completed candle close. This matches
the existing indicator cadence — no separate timer or polling.

## Frontend rendering

### Toggle

`ChartToggles.svelte` exposes a "VOL PROFILE" pill in the same group as
"LIQ HEATMAP". Default state is `false` (opt-in). The flag lives on
`TimeframeTelemetry.showVolumeProfile` (per-TF; the pill syncs all four
TFs in the same way as the existing VWAP / Bollinger pills).

### Primitive

`ui/src/lib/volumeProfile.ts` exports `VolumeProfilePrimitive` (Lightweight
Charts `ISeriesPrimitiveBase`) and `attachVolumeProfile(chart, candleSeries)`
factory. The primitive:

- Anchors to the candle price scale via `priceToCoordinate()`.
- Renders on the right ~12% of the chart width.
- Per bin (sorted by `price_low`): background fill tinted by
  `is_poc` / `is_value_area` / neither; overlaid stacked buy/sell bar
  with top half green (`BUY_COLOR = rgba(38,166,154,0.85)`) and bottom
  half red (`SELL_COLOR = rgba(239,83,80,0.85)`); POC bins get a bright
  yellow border. Buy/sell colors follow the canonical semantic conventions at [07-06-ui-color-conventions.md](../../ui-ux/07-06-ui-color-conventions.md): Green = bullish (buying activity), Red = bearish (selling activity).
- Labels: "POC" (yellow, right of POC), "VAH" / "VAL" (cyan, right of
  value-area bounds).

### Visibility

`PriceChart.svelte` has a `$effect` that feeds `tf.volumeProfile` into the
primitive when `tf.showVolumeProfile === true`, and clears it (passes
`null`) when the toggle is off. Clearing the data leaves the chart
unchanged.

## Edge cases

| Case | Behavior |
|---|---|
| Insufficient history (< half-window bars) | `compute_bins()` returns `None` → `volume_profile` field omitted from the snapshot. |
| Zero-volume window | `compute_bins()` returns `None`. |
| Empty `bins` array after bin-level filtering | `volume_profile` field omitted from the snapshot. |
| Single bin (very tight range) | POC / VAH / VAL collapse to the same midpoint. |
| Bin thickness < 4 px on screen | Stacked buy/sell collapsed to single background tint (avoids unreadable thin bars). |

## Producer/consumer contract

- **Writes:** `market-analyzer` (per-TF pipeline, on candle close).
- **Reads:** `PriceChart.svelte` (chart primitive), `LiquidityPanel`
  optional (can display POC / VAH / VAL alongside cluster info).

## Configuration

| Key | Default | Range | Description |
|---|---|---|---|
| `volume_profile_window` | 500 | 50–2000 | Number of completed bars in the rolling profile. |
| `volume_profile_bins` | 100 | ≥ 1 (floored at 1) | Static bin count for the profile histogram. |
| `volume_profile_value_area` | 0.70 | 0.50–0.90 | Fraction of volume the value area must contain. |

All three keys already exist in `config-models` `IndicatorsConfig`.

## Wire format

The snapshot serializes via `serde` with all fields in snake_case. The
matching TypeScript shape lives in `ui/src/types.ts`:

```ts
export interface VolumeProfileSnapshot {
    symbol: string;
    timeframe_slot: string;
    timeframe_secs: number;
    bins: VolumeProfileBin[];
    poc_price: number;
    value_area_high: number;
    value_area_low: number;
    total_volume: number;
    range_low: number;
    range_high: number;
    num_bins: number;
    timestamp_ms: number;
}
```

## Tests

- `crates/core-domain/src/volume_profile.rs` — `dynamic_bin_count()`
  clamp behavior, empty-snapshot round-trip.
- `crates/market-analyzer/src/indicators/volume_profile.rs` —
  `compute_bins()` POC identification, doji split math, value-area walk.
- `ui/src/lib/volumeProfile.test.ts` — wire-format round-trip, bin math,
  POC/VA identification, edge cases (empty, single bin, 100 bins).
- `ui/src/components/ChartToggles.test.ts` — toggle flag propagation
  across all 4 timeframes, toggle-off behavior, independence of the two
  toggle flags.