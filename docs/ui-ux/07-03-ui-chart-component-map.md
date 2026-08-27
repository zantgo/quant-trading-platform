# UI Chart Component Map

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Per-indicator mapping from registry key to frontend rendering location. Companion to [UI Overview](07-01-ui-overview-spec.md) and [Dashboard Layout](07-02-ui-dashboard-layout.md).

The platform has **50 registered indicators** and the rendering destinations fall into exactly three buckets:

- **PriceChart overlays** (§1) — drawn directly on the main `PriceChart` component.
- **Dedicated panes** (§2) — 18 indicators, one chart component each.
- **Reused / generic panes** (§3) — non-overlapping, share an existing chart surface; categorised by family (Oscillator / Derivatives).

The aggregate count is **22 + 18 + 10 = 50**, registry-verified `2026-07-16`.

---

## 1. Price-Chart Overlays (rendered on `PriceChart`)

The following indicators are drawn directly on the main price chart (OHLCV + overlays). All overlays share a single `Lightweight Charts` canvas; toggles are managed by `ChartToggles` (see §4).

| Registry Key | Display Name | Overlay Form | Toggleable via `ChartToggles`? |
|---|---|---|---|
| `ema_stack` | EMA Ribbon | 4 line series (fast / medium / slow / long) | Yes — 4 separate pill buttons (`INSTANT` / `FAST` / `MEDIUM` / `SLOW`). |
| `donchian` | Donchian | Upper & lower bands as line series | Yes — single `DONCHIAN` pill. |
| `keltner` | Keltner | Upper, middle, lower channels as line series | Yes — single `KELTNER` pill. |
| `vwap` | VWAP | Single line series | Yes — single `VWAP` pill. |
| `anchored_vwap` | Anchored VWAP | Single line series, anchor-point selectable | No — auto-rendered. |
| `supertrend` | Supertrend | Line + direction-flip markers | Yes — single `SUPERTREND` pill. |
| `ichimoku` | Ichimoku Cloud | Tenkan, Kijun, Senkou A, Senkou B, Chikou as 5 line series | No — auto-rendered. |
| `psar` | Parabolic SAR | Dotted markers overlaid on price | No — auto-rendered. |
| `hull_ma` | Hull MA | Single line series | No — auto-rendered. |
| `bollinger` | Bollinger Bands | Upper, middle, lower bands | Yes — single `BOLLINGER` pill. |
| `stddev_channel` | StdDev Channel | ±Nσ envelope | No — auto-rendered. |
| `support_resistance` | Support/Resistance | Horizontal level lines | No — auto-rendered. |
| `fibonacci` | Fibonacci | Retracement/extension level lines | No — auto-rendered (per-WS settings). |
| `pivot_points` | Pivot Points | Classic pivot level lines | No — auto-rendered. |
| `smc_structure` | SMC Structure | CHoCH / BOS markers on price | No — auto-rendered. |
| `smc_liquidity` | SMC Liquidity | Liquidity-pool markers | No — auto-rendered. |
| `smc_fvg` | SMC Fair Value Gap | FVG zone shading | No — auto-rendered. |
| `smc_order_blocks` | SMC Order Blocks | Order-block zone shading | No — auto-rendered. |
| `patterns` | Chart Patterns | Pattern detection markers (bullish / bearish breakout) | No — auto-rendered. |
| `candlestick` | Candlestick Patterns | Pattern detection markers (single-candle / multi-candle patterns) | No — auto-rendered. |
| `volume_profile` | Volume Profile | Price-overlay histogram on `PriceChart` | No — auto-rendered. |
| `oi_price_divergence` | OI-Price Divergence | Divergence marker on `PriceChart` (standalone registry entry with its own JSON key — see `04-02-00-indicator-index.md §Derivatives note`) | No — auto-rendered. |

### 1.1 Price Line Mode

`PriceChart` also supports a binary `priceLineMode` toggle (CANDLES vs LINE) on the top-left of the chart header, controlled via `pair.priceLineMode`. Both modes render the same overlay set above.

---

## 2. Dedicated Pane Components

The following 18 indicators each have their own dedicated chart component, mounted as a vertically-stacked canvas below the `PriceChart`. Each pane receives `pairKey` as a prop and binds its own canvas.

| Registry Key | Display Name | Component | Notes | CSS-module exemption? |
|---|---|---|---|---|
| `rsi` | RSI | `RsiChart` | OB/OS lines at 70 / 30 (or 80 / 20 in strong trend). | **Yes** (chart-only) |
| `macd` | MACD | `MacdChart` | Line / signal / histogram. | **Yes** (chart-only) |
| `adx` | ADX | `AdxChart` | ADX + DI+ / DI−. | **Yes** (chart-only) |
| `atr` | ATR | `AtrChart` | Single line series. | **Yes** (chart-only) |
| `squeeze` | TTM Squeeze | `SqueezeChart` | Momentum histogram with squeeze dots. | **Yes** (chart-only) |
| `bbwp` | BBWP | `BbwpChart` | BB Width Percentile. | No (companion `.module.css`) |
| `volume` | Volume | `VolumeChart` | Volume bars + rolling average line. | **Yes** (chart-only) |
| `rvol` | Relative Volume | `RvolChart` | RVOL line with institutional / climax bands. | No (companion `.module.css`) |
| `stochastic` | Stochastic | `StochasticChart` | %K / %D with OB/OS lines. | No (companion `.module.css`) |
| `chandemo` | Chande MO | `ChandeMoChart` | Line with zero line. | No (companion `.module.css`) |
| `obv` | On-Balance Volume | `ObvChart` | Cumulative line. | No (companion `.module.css`) |
| `cmf` | Chaikin MF | `CmfChart` | Line with zero line. | No (companion `.module.css`) |
| `mfi` | Money Flow Index | `MfiChart` | Line with OB/OS lines. | No (companion `.module.css`) |
| `hv` | Historical Volatility | `HvChart` | Line series. | No (companion `.module.css`) |
| `aroon` | Aroon | `AroonChart` | Aroon Up / Aroon Down. | No (companion `.module.css`) |
| `choppiness` | Choppiness Index | `ChoppinessChart` | Line with chop/trend bands. | No (companion `.module.css`) |
| `linreg_slope` | LinReg Slope | `LinRegSlopeChart` | Line with zero line. | No (companion `.module.css`) |
| `zscore` | Z-Score | `ZScoreChart` | Line with zero line. | No (companion `.module.css`) |

### 2.1 CSS-module exemption rule

The six chart-only components (`AtrChart`, `RsiChart`, `MacdChart`, `SqueezeChart`, `VolumeChart`, `AdxChart`) qualify for the CSS-module exemption described in [07-01 §8](07-01-ui-overview-spec.md) — they wrap a single Lightweight Charts canvas with a minimal `.chart-container { width: 100%; height: 100% }` style and need no companion `.module.css` file. The remaining 12 dedicated panes keep their companion modules because they add non-trivial layout chrome (headers, OB/OS line legends, dual-axis annotations).

---

## 3. Reused / Generic Panes (rendered in shared panes)

The following 10 indicators share existing chart surfaces rather than having dedicated ones. They prevent vertical screen crowding when all 21 chart surfaces (PriceChart + 18 dedicated + the generic "Oscillator" and "Derivatives" panes = 21) would otherwise dominate the viewport.

| Registry Key | Display Name | Rendered In |
|---|---|---|
| `awesome_oscillator` | AO | generic "Oscillator" pane |
| `williams_r` | Williams %R | generic "Oscillator" pane |
| `cci` | CCI | generic "Oscillator" pane |
| `force_index` | Force Index | generic "Oscillator" pane |
| `open_interest` | Open Interest | generic "Derivatives" pane |
| `oi_delta` | OI Delta | generic "Derivatives" pane |
| `funding_rate` | Funding Rate | generic "Derivatives" pane |
| `order_flow_imbalance` | Order Flow Imbalance | generic "Derivatives" pane |
| `spread` | Spread | generic "Derivatives" pane |
| `depth_bias` | Depth Bias | generic "Derivatives" pane |

> **Note:** `volume_profile` and `oi_price_divergence` are listed under §1 (PriceChart overlays). Any earlier listing under this section was a placement error — both indicators render on `PriceChart`, not in generic panes.

### 3.1 Generic Pane Behavior

A generic pane is a single canvas shared by 2-4 oscillators or derivatives. The pane header shows a tab strip of the family members; selecting a tab swaps the data series bound to that canvas (no full remount required, just a `setData()` call on the existing canvas). The `chartRegistry.svelte.ts` module keeps the registry of which indicators map to which generic pane.

---

## 4. `ChartToggles` Behavior

`ChartToggles` is mounted in the `LiveTerminal` toolbar (above the PriceChart) and provides a row of pill-style toggle buttons. Only the overlay indicators with a user-facing show/hide decision expose a pill — the rest are auto-rendered when their data is present.

| Toggle group | Pill label(s) | Bound field |
|--------------|---------------|-------------|
| Price mode | `CANDLES` / `LINE` | `pair.priceLineMode` |
| EMA stack | `FAST` / `MED` / `SLOW` / `LONG` | `pair.showEma{Fast,Medium,Slow,Long}` |
| Channel overlays | `VWAP` / `BOLLINGER` / `SUPERTREND` / `KELTNER` / `DONCHIAN` | `pair.microTerm.show{Vwap,Bb,Supertrend,Keltner,Donchian}` (synced across all 4 TFs via `syncAll`) |
| Chart overlays (opt-in) | `LIQ HEATMAP` / `VOL PROFILE` | `pair.microTerm.show{LiqHeatmap,VolumeProfile}` (synced across all 4 TFs via `syncAll`; both default to `false`) |

Toggles write directly to `TimeframeTelemetry` fields — they are runtime overlays, not config-level settings. The dedicated panes (§2) are NOT toggleable from this bar; each pane has its own visibility header on its own canvas.

**LIQ HEATMAP** enables the `LiquidationHeatmapPrimitive` overlay (colored horizontal bands at cluster price zones). **VOL PROFILE** enables the `VolumeProfilePrimitive` overlay (right-edge stacked buy/sell histogram). Both overlays share the same `candleSeries` price scale and cost no extra chart instances. See `crates/market-analyzer/src/indicators/volume_profile.rs` and `docs/engines/market-monitoring-engine/03-02-13-mme-volume-profile-layer.md`.

---

## 5. Chart Interaction Model (v6.5)

### 5.1 Independent Pan & Zoom

Every chart pane has its own `IChartApi` instance with `handleScale: true` and `handleScroll: true`. Each pane supports:

| Gesture | Effect |
|---------|--------|
| Scroll wheel (vertical) | Zoom vertical price/indicator axis |
| Drag (horizontal) | Pan timeline left/right |
| Drag (vertical) | Pan price/indicator axis |
| Crosshair tracking | Normal crosshair mode with styled vert/horz lines (`#4c525e`) |

Charts are **fully independent** — crosshair, scroll position, and zoom level are not synchronized between panes. Each chart has its own `timeScale()` and `priceScale()`.

### 5.2 Double-Click Fullscreen

All 20 chart components support **double-click → fullscreen**:

1. Double-clicking any chart pane toggles `isFullscreen = true`.
2. The chart wrapper receives CSS class `.fs-active`: `position: fixed; inset: 0; z-index: 990; background: #131722; padding: 44px 16px 16px 16px`.
3. The `ChartFullscreenOverlay` component renders a backdrop (`rgba(0,0,0,0.88)`, `z-index: 1000`) plus a header bar with:
   - **Title:** e.g. "Price Chart — BTC-USDT · 60s" or "RSI 14 — BTC-USDT · 60s"
   - **Screenshot button:** blue outlined pill, exports PNG
   - **Close button:** ✕ glyph
4. Chart resizes to fill the available space via `requestAnimationFrame(() => chart.resize(w, h))`.
5. **Dismiss:** Click backdrop, press `Escape` (`<svelte:window onkeydown>`), or click ✕.

### 5.3 Screenshot Export

When the **Screenshot** button is clicked in the fullscreen header:

1. `chart.takeScreenshot()` (Lightweight Charts API) → `HTMLCanvasElement`
2. `canvas.toBlob('image/png')` → PNG blob
3. `URL.createObjectURL(blob)` → temporary URL
4. Programmatic `<a download="chart-{indicator}-{pairKey}-{timeframe}s-{timestamp}.png">` click
5. `URL.revokeObjectURL()` cleanup

Implementation: shared `lib/chartScreenshot.ts` (`takeChartScreenshot(chart, filename)`) used by all 20 chart components.

### 5.4 Resizable Panes

Between each adjacent pane in `LiveTerminal` is a 6 px drag handle (`<button class="dragHandle">`):

- **Appearance:** `height: 6px; background: #1a1d26; cursor: ns-resize`. On hover: `background: #42a5f5`. A centered `::after` ridge (24×2 px, `#3a3f4e`) indicates the drag target.
- **Drag:** `mousedown` → track `mousemove` Y delta → redistribute height between adjacent panes. `mouseup` stops tracking. Total height conserved between the two panes.
- **Double-click handle:** resets both adjacent panes to defaults (Price: 420 px, indicators: 160 px each).
- **Constraints:** minimum 60 px, maximum 800 px per pane.
- **State:** `paneHeights = $state([420, 160, 160, 160, 160, 160])` array. Each pane renders with `style="height:{paneHeights[i]}px"`.

Internal canvas resizing is handled automatically by each chart's `ResizeObserver` (watching the pane's `parentElement` dimensions).

---

## 6. Aggregate Counts

| Render Bucket | Indicator Count |
|---|---|
| Price-chart overlays | 22 (18 structural-line overlays + `patterns` + `candlestick` marker overlays + `volume_profile` price-overlay histogram + `oi_price_divergence` divergence marker) |
| Dedicated panes | 18 |
| Reused / generic panes | 10 (4 oscillators + 6 derivatives, excluding the two entries counted under PriceChart overlays) |
| **Total** | **50** |

---

## 7. Cross-References

- [UI Overview](07-01-ui-overview-spec.md) — Chart architecture (§7), fullscreen overlay model, CSS module contract.
- [Dashboard Layout](07-02-ui-dashboard-layout.md) — Panel placement for the Charts tab, resizable pane handles, fullscreen and screenshot UX.
- [Indicator Index](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md) — Authoritative registry.
