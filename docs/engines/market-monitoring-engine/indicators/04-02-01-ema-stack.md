# 📈 Exponential Moving Averages (EMA 10, 50, 100, 200) Protocol

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
Exponential Moving Averages (EMAs) are trend-following, lagging indicators that apply more weight to the most recent price data. Unlike Simple Moving Averages (SMAs), EMAs react more quickly to price changes, making them useful for identifying immediate momentum shifts while smoothing out short-term market noise.

This strategy uses a structured, four-timeframe EMA stacking model to classify market structure, filter out low-probability entries, locate dynamic support and resistance, and trigger structural position invalidations.

---

## 2. Technical Structure and Calculations
By adjusting the lookback period, each of the four EMAs is assigned a specific structural role in the execution pipeline:

### 2.1 The Stacking Components
1.  **EMA 10 (Instant - Tactical Momentum):** Reacts rapidly to immediate price fluctuations. It serves as your short-term momentum tracker and tactical pullback reference.
2.  **EMA 50 (Fast - Swing Trend):** Establishes the primary trend direction over a medium-term horizon. It acts as the dynamic support/resistance baseline during active trends.
3.  **EMA 100 (Medium - Structural Support):** Represents the secondary structural boundary. It serves as a deeper value-area pullback zone.
4.  **EMA 200 (Slow - Macro Trend Filter):** Represents the ultimate structural trend filter. The angle and location of the EMA 200 define the macro market regime.

### 2.2 Mathematical Smoothing Factor
Unlike SMAs, which weight all bars equally, the EMA calculation utilizes a multiplier ($\alpha$) based on the specified period ($P$):
$$\alpha = \frac{2}{P + 1}$$

The current EMA is then calculated by applying this multiplier to the current price ($Price_t$) and adding it to the weighted value of the previous period's EMA ($EMA_{t-1}$):
$$EMA_t = (Price_t \times \alpha) + (EMA_{t-1} \times (1 - \alpha))$$

---

## 3. Structural Regime Classification

Before evaluating any indicator crossover, the four EMAs must be analyzed to determine if the market structure supports a trending or ranging regime.

### 3.1 Full Trend Alignment (The Stack)
To enter trend-following positions, the four EMAs must be stacked in sequential order with positive or negative slopes:

*   **Bullish Stack (Long Only):**
    $$Price_t > EMA(10)_t > EMA(50)_t > EMA(100)_t > EMA(200)_t$$
    *   *Action:* Trend momentum is strong. Only long trades are permitted. Look to buy dynamic support pullbacks.
*   **Bearish Stack (Short Only):**
    $$Price_t < EMA(10)_t < EMA(50)_t < EMA(100)_t < EMA(200)_t$$
    *   *Action:* Downward momentum is strong. Only short trades are permitted. Look to short dynamic resistance rallies.

### 3.2 Tangled Regime (Consolidation)
*   **Condition:** The EMAs are crossing one another frequently, flattening out, and wrapping closely around the current close price.
*   **Action:** Price is range-bound. Trend-following signals are ignored. All trend-based entries are paused until a clean breakout occurs and EMAs begin to fan out.

---

## 4. Entry and Management Rules

### 4.1 The Macro Trend Filter (The Slow EMA Rule)
The EMA 200 is the ultimate trend filter across all execution timeframes:
*   **Long Prohibition:** If price is trading below the EMA 200 on your primary execution or macro chart, all Long entry signals are rejected.
*   **Short Prohibition:** If price is trading above the EMA 200 on your primary execution or macro chart, all Short entry signals are rejected.

### 4.2 Dynamic Support and Resistance Entries (Value Areas)
During a fully aligned trending stack, the area between the EMA 10 and EMA 50 represents the institutional "value area":
*   **Bullish Pullback Entry:** If a bullish stack is confirmed, wait for price to pull back into the zone between the EMA 10 and EMA 50. If price tests the EMA 50, wicks, and closes back above the EMA 10, enter a long position.
*   **Bearish Rally Entry:** If a bearish stack is confirmed, wait for price to rally into the zone between the EMA 10 and EMA 50. If price wicks off the EMA 50 and closes back below the EMA 10, enter a short position.

### 4.3 Structural Invalidation (The Decisive Close)
The EMAs provide early invalidation triggers before your hard stop-losses are hit:
*   **Long Invalidation:** If you are holding a long position and a candle closes decisively below the EMA 100 or EMA 200, the trend structure is compromised. Close the trade via a market order.
*   **Short Invalidation:** If you are holding a short position and a candle closes decisively above the EMA 100 or EMA 200, the trend structure is compromised. Close the trade via a market order.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| StackChange | ESTABLISHED_BULLISH_STACK | Fast > Medium > Slow > Long, price > fast EMA | Bullish |
| StackChange | ESTABLISHED_BEARISH_STACK | Fast < Medium < Slow < Long, price < fast EMA | Bearish |
| StackChange | CONSOLIDATED_TANGLED_STACK | EMAs are interwoven, no clear ordering | Neutral |
| Crossover | EMA_PRICE_CROSS_FAST_BULLISH | Price crosses above the EMA 10 line — transition bar only | Bullish |
| Crossover | EMA_PRICE_CROSS_FAST_BEARISH | Price crosses below the EMA 10 line — transition bar only | Bearish |
| Crossover | EMA_PRICE_CROSS_MEDIUM_BULLISH | Price crosses above the EMA 50 line. Swing trend confirmation. | Bullish |
| Crossover | EMA_PRICE_CROSS_MEDIUM_BEARISH | Price crosses below the EMA 50 line. Swing trend breakdown. | Bearish |

The StackChange and Crossover signals are distinct. StackChange fires on regime-level ribbon alignment; Crossover fires on point-in-time price-vs-fast-EMA crossings.

## Normalization

The EMA Ribbon normalized score in [-1, 1] is binary/tertiary:
- **ESTABLISHED_BULLISH_STACK**: +1.0
- **DYNAMIC_BULLISH_RETEST**: +0.8 (bullish pullback to medium EMA)
- **CONSOLIDATED_TANGLED_STACK**: 0.0
- **DYNAMIC_BEARISH_RETEST**: -0.8 (bearish pullback to medium EMA)
- **ESTABLISHED_BEARISH_STACK**: -1.0

The `values` sub-map carries: `fast`, `medium`, `slow`, `long` (the 4 EMA values). Confidence = |normalized|.

> **Note.** The composite `ema_stack.normalized` score is a discrete stack-state classifier — it is **NOT** an equal-weighted average of the 4 EMAs. It considers all four lines always (never just the fast) and emits one of 5 buckets. If a future change wants a continuous equal-weighted aggregate, it must be added as a separate signal/value — not by replacing this classifier.

---

## Unified Ribbon Export (v6.11)

The four EMA surfaces in the platform now read from the **same record** —
`MarketSnapshot.indicators["ema_stack"].values.{fast,medium,slow,long}` —
and so carry byte-identical numbers in every consumer. Surfacing the full
four values plus a cross-line spread gives the trader the fast/medium/slow/long
view at a glance without expanding the indicator row.

### The four surfaces (all read the same record)

| Surface | Path | What it carries |
|---|---|---|
| **Metrics Layer (L1)** | `crates/market-analyzer/src/analyzer/{mod.rs:694-697, 1713-1716}` + `crates/market-analyzer/src/analyzer/warm.rs:156-159, 321-324, 533-536` | The `Ema::new(period)` calculator instances that produce `final_ema_*` Decimals per candle |
| **Metrics Matrix** | `MarketSnapshot.indicators["ema_stack"].values.*` (written by `inject_ema_values` in `crates/market-analyzer/src/analyzer/normalize.rs:521-546`) | Canonical record on the wire and DB |
| **Charts tab overlay** | `ui/src/components/PriceChart.svelte:336-340` + `:811-846` | 4 colored lines drawn as price overlays (per-bar series via `alignedSeriesFromHistory`) |
| **Metrics tab — on-screen micro-grid** | `ui/src/components/facets/IndicatorsView.svelte` (collapsed `raw_value` cell when `m.key === 'ema_stack'`) | 4-line / 8-cell micro-grid: `LABEL  value  distance%`, plus `spread ↔ 0.27%` sub-label |
| **Metrics tab — export body `body.ema`** | `ui/src/lib/exportBuilders/metricsTab.ts` → `buildEmaBlock()` (defined in `ui/src/lib/exportBuilders/shared.ts`) | Top-level `body.ema.{fast,medium,slow,long}.{value, period, distance_from_price}` plus `body.ema.spread_pct` |

### Per-line distance_from_price and spread

For each line:
```
distance_from_price[role] = (close − ema[role]) / close
```

For the cross-line spread:
```
spread_pct = (values.fast − values.long) / close
```

Positive spread = bull (fast above long). Negative = bear. Magnitude =
ribbon "spread", the canonical trend-conviction proxy on a 4-EMA system
(coiled breakout → spread → 0; trending maturity → spread → wider).
Implemented once in `distFromPrice()` / `emaSpreadPct()` in
`ui/src/lib/telemetry.ts` and reused by every consumer — single source
of truth, no second computation.

### Defaults and configuration

Default periods (`ui/src/stores/settings.svelte.ts:10`): `fast=10`,
`medium=50`, `slow=100`, `long=200`. These can be overridden per
timeframe via the dashboard settings; the same configured list drives
the `period` field on every `body.ema.*` line in the export (single
source: `app.settings.globalIndicatorsConfig.{ema_fast,ema_medium,ema_slow,ema_long}`,
read at `ui/src/state.svelte.ts:419-422`).

## Sub-minute warm-up (per-line availability, AUDIT-V8-001)

On sub-minute timeframes (CB-05 skips the historical bootstrap, so the
buffer starts at zero) the ribbon is emitted **per line**: each line
appears in `ema_stack.values.*` only once the pipeline has accumulated at
least its configured period of completed closes.

| Line | Appears at | Example at 3 s candles |
|------|-----------|------------------------|
| `fast` | ≥ 10 closes | 30 s after cold start |
| `medium` | ≥ 50 closes | 2.5 min |
| `slow` | ≥ 100 closes | 5 min |
| `long` | ≥ 200 closes | 10 min |

Implementation: the registry entry carries `bars_required = 1` (the entry
always survives the normalize gate); the per-line gate lives in
`inject_ema_values` (`crates/market-analyzer/src/analyzer/normalize.rs`),
which skips any line whose `bar_count < period`. Consumers treat a missing
sub-key as `None` (chart lines, the on-screen micro-grid, and the export
body all render `--`). Above-minute timeframes are unaffected: the
historical bootstrap preloads ≥ `[candle_buffer] size` candles, so all
four lines are present from first paint.

### Unification invariants (regression-tested)

1. `body.ema.{fast,medium,slow,long}.value` is byte-identical to
   `indicators[ema_stack].sub_values.{fast,medium,slow,long}` on the
   export body. Test:
   `ui/src/lib/exportBuilders/metricsTab.test.ts > body.ema — Metrics
   tab export body block > unification: body.ema.*.value ===
   indicators[ema_stack].sub_values.*`.
2. The on-screen micro-grid cell and the export body's `body.ema`
   block read the same record — `buildEmaRibbonCellView()` and
   `buildEmaBlock()` both funnel through `buildEmaRibbonView()` in
   `ui/src/lib/telemetry.ts`. Tests:
   `ui/src/components/facets/IndicatorsView.test.ts > IndicatorsView
   EMA Ribbon micro-grid` and
   `ui/src/lib/exportBuilders/shared.test.ts > buildEmaBlock`.
3. `meta` does NOT carry an `ema` field — the per-TF indicator snapshot
   lives in the body. Test:
   `ui/src/lib/exportBuilders/metricsTab.test.ts > meta envelope —
   does NOT carry ema`.
