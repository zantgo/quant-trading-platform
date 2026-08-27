# Liquidation Heatmap — Leverage Tier Configuration

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Scope:** Operator workflow for the per-timeframe Liquidation Heatmap tier controls introduced in v7.0-prod (D5 default = 10×, range = [1, 100] integers).

---

## 1. Where it lives

`MARKET MONITORING ▸ WORKSPACE ▸ SETTINGS ▸ {pick timeframe on left rail} ▸ LIQUIDATION HEATMAP · {SLOT}`

```
┌──────────────┬──────────────────────────────────────────┐
│ MTF          │  LIQUIDATION HEATMAP · MICRO TERM        │
│ ● MICRO      │  ┌────────────────────────────────────┐  │
│ FAST         │  │ Highlight clusters whose          │  │
│ SLOW         │  │ dominant_leverage falls within    │  │
│ MACRO        │  │ ±0.5 of any selected integer ×.   │  │
│              │  └────────────────────────────────────┘  │
│              │  [10×]  [25×]  [50×]  [×]   [ADD]        │
│              │  Integer leverage × in [1, 100].        │
└──────────────┴──────────────────────────────────────────┘
```

Picking a different rail item refreshes the card with the saved tier list for that slot. Tier lists are stored PER TIMEFRAME — micro can highlight `[10]` while macro highlights `[25, 50]`.

## 2. Visual semantics

Once the chart is open with the LIQUIDATION HEATMAP overlay enabled (`Pair.microTerm.showLiqHeatmap = true` via the chart-toggles row), every cluster's `dominant_leverage` is matched against the **selected** tier list with a ±0.5 epsilon:

- **Match (e.g. cluster `9.7`, chip `10×`)** — `globalAlpha` bumps from 0.45 to 0.85 and intensity is multiplied by 1.4 (capped at 1). The matching band reads as a bright, prominent strip on the candle pane.
- **No match** — intensity is multiplied by 0.6 (default 0.45 × 0.6 ⇒ 0.27). The band fades into the background while remaining readable.
- **Empty set** — every cluster renders at base intensity (no boost or dim). Equivalent to the pre-v7.0-prod overlay.

The matching helper is exported from `ui/src/lib/liquidationHeatmap.ts::clusterInHighlight` and is unit-tested for the epsilon (0.5), integer-only restriction, range validation ([1, 100]), and graceful fallbacks for null/malformed input. Tier set updates take effect on the next candle interval (or sooner if `bumpWsVersion` fires).

## 3. Input contract

| Operator action        | Result                                                       |
|------------------------|--------------------------------------------------------------|
| Click `ADD` with empty input | Button disabled.                                            |
| Type `25` + Enter / `ADD` | Add `[25]` (sorted ascending, deduped against existing list). |
| Type `12.5` + `ADD`    | Reject — fractional inputs are silently dropped (D6).       |
| Type `0`, `101`, `-1`, `500` + `ADD` | Reject — out-of-range inputs are silently dropped. |
| Type an existing tier  | Reject — duplicates are silently dropped.                   |
| Click the `×` on a chip | Remove that tier from the set.                              |

The tier chip's remove button is keyboard accessible (`aria-label="Remove {t}x tier"`). The integer input has `inputmode="numeric"` so mobile devices show a numeric keypad.

## 4. Persistence

- **UI side:** `tf.heatmapLeverageTiers: number[]` on `TimeframeTelemetry` (`ui/src/types.ts` ~line 681). Each TF slot has its own list.
- **Default seed:** `[10]` (single chip). New pairs / freshly created instances start with this default.
- **Save flow:** `WorkspaceSettings.svelte::applySettings()` posts a single body that includes `heatmap_leverage_tiers` inside each of `micro_term.indicators`, `fast_term.indicators`, `slow_term.indicators`, `macro_term.indicators` (see `body` builder at `ui/src/components/WorkspaceSettings.svelte::buildIndicators`).
- **Hydrate flow:** `ui/src/lib/api.svelte.ts::advancedIndicators()` reads `ind.heatmap_leverage_tiers` and defensively filters to integers in [1, 100] before writing the array onto the live TF. Malformed entries fall back to `[10]`.

## 5. Save / refresh contract

1. Open SETTINGS, pick a slot on the left rail, edit the tier chip rail, click `SAVE WORKSPACE CONFIGURATION`.
2. The single POST applies to all 4 TFs; the daemon echoes back the new `heatmap_leverage_tiers` for each slot.
3. Reload the page; the chips survive.
4. Toggle the LIQ HEATMAP overlay off and back on — the tier selection is decoupled from visibility, matching the existing `setVisible` + `updateData` decoupling (mirrors `VolumeProfilePrimitive`).
5. The `bumpWsVersion()` call after save forces each WS connection to reconnect with the freshly-published `timeframe_secs`. The heatmap effect re-runs on the new connection and re-applies the new tier list — no operator intervention required.

## 6. Operator workflow — recommended preset per liquidity regime

The preset is the operator's call. A few practical starting points observed during development:

| Regime (L3 analysis.market_regime) | Suggested tier set    | Rationale                                              |
|------------------------------------|-----------------------|--------------------------------------------------------|
| `TRENDING_BULL` / `TRENDING_BEAR` | `[5, 10, 25]`        | Mid-tier leveraged positions are typically the exit fuel. |
| `RANGE`                           | `[1, 3, 50, 100]`    | Wicks tend to hunt 1× and 3×; breakouts come from 50×+. |
| `EXPANSION`                       | `[10, 25, 50]`        | High-vol clusters cluster in the mid-to-high ×.       |
| Manual override                   | any integer [1, 100] | Operator picks their risk window.                      |

These are starter values — production traders should pick the tiers that match their book composition.

## 7. Cross-references

- `docs/engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md` — Block C wire format (`LiquidationClusterMatrix.dominant_leverage`).
- `ui/src/lib/liquidationHeatmap.ts::clusterInHighlight` — matcher implementation (1-line epsilon comment, integer filter).
- `ui/src/lib/liquidationHeatmap.test.ts` — unit tests.
- `ui/src/components/LiquidationHeatmapTierPicker.svelte` — picker UI (input + chips + remove).
- `ui/src/components/LiquidationHeatmapTierPicker.test.ts` — input validation tests.
