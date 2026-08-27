# Export Data Payload Schema

<!-- pascal-display-strings -->

<!-- This document's JSON examples carry UI display strings (e.g.
     "Momentum", "Bullish", "Trend Continuation") which are intentionally
     PascalCase — they document the screen-facing *display* fields, not
     wire enums. Exempted from the G6 enum-casing lint via the marker. -->

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document specifies the JSON payload produced by every panel's `Export Data` button. Each panel's export mirrors **1:1 the data the panel renders** — the same numbers, the same prices, the same dynamic strings, the same words; only the presentation changes (the screen formats `63390` as `$63390`, the JSON carries the raw value). Consumers (AI agents, downstream services, debugging tools) can rely on the field shapes documented here.

> **Filter pills (v6.10.19d B).** The filter pills (`Active only`,
> `Confirmed+`, `Hide gates`, `Hide overlays`, search) were removed from
> both Metrics surfaces — the views always run on the platform default
> filters, so every row is visible. Exports carry the full dataset with
> per-row `visible` flags (always `true` under the defaults); the
> top-level `filter_state` block is gone.

---

## 1. Architecture Overview

Each panel has a **scoped builder** that emits a JSON payload matching the panel's rendered surface. The legacy "kitchen-sink" (`buildMetricsExportJson` in `lib/metricsExport.ts`) is preserved for backward compatibility but is **not** called by any panel.

| Tab (UI label) | Panel source | Builder file | Source button |
|---|---|---|---|
| Charts → Positions | `BottomConsole.svelte` | `lib/exportBuilders/chartsTab.ts` | `buildPositionsTabExport` |
| Charts → Open Orders | `BottomConsole.svelte` | `lib/exportBuilders/chartsTab.ts` | `buildOrdersTabExport` |
| Charts → History | `BottomConsole.svelte` | `lib/exportBuilders/chartsTab.ts` | `buildHistoryTabExport` |
| Charts → Plan | `BottomConsole.svelte` | `lib/exportBuilders/chartsTab.ts` | `buildPlanTabExport` |
| Metrics → single-TF | `TerminalMonitor.svelte` | `lib/exportBuilders/metricsTab.ts` | `buildMetricsTabExport` |
| Metrics → MTF | `TerminalMonitor.svelte` | `lib/exportBuilders/mtfTab.ts` | `buildMtfExportJson` |
| Alignment | `AlignmentPanel.svelte` | `lib/exportBuilders/alignmentTab.ts` | `buildAlignmentTabExport` |
| Opportunities | `OpportunitiesPanel.svelte` | `lib/exportBuilders/opportunityTab.ts` | `buildOpportunityTabExport` |
| Risks | `RiskPanel.svelte` | `lib/exportBuilders/riskTab.ts` | `buildRiskTabExport` |
| Analysis | `AnalysisPanel.svelte` | `lib/exportBuilders/analysisTab.ts` | `buildAnalysisTabExport` |
| Recommendation | `RecommendationPanel.svelte` | `lib/exportBuilders/recommendationTab.ts` | `buildRecommendationTabExport` |

The shared envelope (`meta`, `header`, R:R helper) is in `lib/exportBuilders/shared.ts`. Canonical human-readable sentences shared between panels, facets and exports (Fibonacci GP status, VP position, …) live in `lib/structuralStrings.ts`; the phase prettifier is `lib/prettifyPhase.ts`.

---

## 2. Shared Envelope

### 2.1 MetaEnvelope — every payload

```json
{
  "source_tab": "metrics",
  "meta": {
    "datetime_utc": "2026-08-12T12:34:56.789Z",
    "exchange": "Hyperliquid",
    "pair": "BTC-USDT",
    "timeframe_secs": 60,
    "current_price": 63390.0,
    "prev_day_price": 62000.0,
    "price_change": 2.24,
    "price_change_direction": "up",
    "timestamp": 1753950000,
    "is_completed": true
  },
  "header": {
    "layer_name": "Metrics",
    "badge": { "label": "STRONG BULLISH", "sublabel": "TRENDING", "tone": "bull" },
    "chips": [ { "label": "Score", "value": 0.62 }, { "label": "Signals", "value": 7 } ],
    "status": "live"
  }
}
```

Rules:
- Numbers are raw (no `%`, `$`, `1 :` diminutives).
- Strings mixed with numbers are split into structured fields plus `*_display` verbatim copies of the screen sentence where the screen renders one (e.g. `rr_display`, `entry_danger_display`, `current_position_label`, `badge_text`).
- `header` mirrors the LayerHeader chrome (badge label/sublabel + meta chips + status). Chip values that are numeric strings are parsed to numbers.
- **No `filter_state` anywhere (v6.10.19d B).** The filter pill bars were removed from the Metrics and MTF views — the grid always runs on the default filters (everything visible). Neither `meta` nor the top level carries a `filter_state` block anymore (it was never inside `meta`).

### 2.2 Canonical value sources (cross-tab consistency)

Every tab exports from the **same active instance**, so the meta block is
identical across all seven tabs for a given click-epoch. The rules below
are enforced by the shared `buildPriceBlock` in `lib/exportBuilders/shared.ts`
and by the panels' builder wiring:

| Field | Canonical source | Notes |
|---|---|---|
| `meta.pair` | The full `instancesMap` key (`BTC-USDT` / `BTC-USDC`) | NEVER the bare base (`BTC`). All panels pass the full `pairKey` prop; the Analysis panel resolves `app.activeTab` (not `activeSymbol`). |
| `meta.current_price` | `pickInstanceLivePrice(activeInstance terms)` — the freshest live price within 30 s, else last known good | **Live tick value.** Two exports clicked at the same instant carry the same price; sequential exports naturally drift as the market moves — that is expected and is not an inconsistency. |
| `meta.prev_day_price` / `price_change` / `price_change_direction` | `pickLatestCompletedSnapshot(terms)` — the newest **completed-candle** snapshot (shadow/live-tick frames drop `prev_day_px`) | Consistent across tabs: one canonical completed snapshot feeds all seven. |
| `meta.timestamp` | The snapshot's Unix-seconds timestamp (`null` for MTF — no single TF) | Single-TF exports carry their active TF's snapshot ts; the L3–L6 tabs carry the newest snapshot's ts. |
| `meta.timeframe_secs` | `0` for MTF (multi-TF sentinel); the active TF's `barDurationSec` otherwise | MTF also emits `meta.timesframes: ["Micro","Fast","Slow","Macro"]`. |
| `meta.datetime_utc` | `now` at click time | Each export is a fresh click-epoch; timestamps legitimately differ across sequential clicks. |
| `meta.is_completed` | The snapshot's `is_completed` flag | Consistent per snapshot. |

---

## 3. Per-Tab Payload Schemas

### 3.1 Metrics Tab (single-TF) — `source_tab: "metrics"`

Mirrors `TerminalMonitor.svelte` single-TF mode (Market Context strip, Group Confluence grid, Structural Anchors strip, Indicators/Signals/Divergences/Levels facets).

```json
{
  "source_tab": "metrics",
  "meta": { },
  "header": { },
  "market_context": {
    "regime": "TRENDING",
    "overall_score": 0.62,
    "overall_label": "STRONG_BULLISH",
    "trend":      { "score": 0.7, "confidence": 80, "label": "BULLISH" },
    "momentum":   { "score": 0.5, "confidence": 70, "label": "BULLISH" },
    "volatility": { "score": -0.2, "confidence": 60, "label": "EXPANDING" },
    "volume":     { "score": 0.3, "confidence": 65, "label": "STRONG" },
    "liquidity":  { "score": 0.4, "confidence": 60, "label": "HEALTHY" },
    "signal_count": 7,
    "age_bars_display": "5b"
  },
  "group_confluence": [
    {
      "group": "Momentum", "label": "Momentum",
      "total": 2, "gates": 0, "bullish": 2, "bearish": 0, "neutral": 0,
      "active": 2, "active_signals": 3, "dominant": "bull"
    }
  ],
  "structural_anchors": {
    "fibonacci": {
      "present": true,
      "gp_top": 64050, "gp_bottom": 62600,
      "swing_direction": "BULL SWING",
      "status": "INSIDE GP (-8.97% from center)",
      "price_vs_gp_pct": 0.10,
      "ext_1618": 65500, "ext_2618": 67200,
      "retracement_coefficients": {
        "fib_0236": 63100, "fib_0382": 63250, "fib_0500": 63330,
        "fib_0618": 63400, "fib_0660": 63440, "fib_0786": 63500
      }
    },
    "volume_profile": {
      "poc_price": 63300, "value_area_high": 63700, "value_area_low": 63000,
      "total_volume": 12500, "range_low": 62800, "range_high": 63900,
      "num_bins": 60, "timestamp_ms": 1753950000000,
      "top_hvn": [ { "price_low": 63280, "price_high": 63320, "volume": 400, "buy_volume": 250, "sell_volume": 150, "strength_x_mean": 1.6 } ],
      "buy_total": 450, "sell_total": 370,
      "buy_sell_bias": 0.0976,
      "current_position_label": "INSIDE VALUE AREA",
      "range_pos_pct": 48.18
    },
    "liquidity": {
      "oi_long_pct": 57, "oi_short_pct": 43,
      "cascade_state": "SUSTAINED", "cascade_intensity": 72.5,
      "cascade_intensity_display": "73",
      "cascade_state_label": "SUSTAINED",
      "cascade_asymmetry": 0.35, "cascade_asymmetry_sign": "+",
      "cascade_asymmetry_magnitude_pct": 35.0,
      "cascade_asymmetry_description": "short_squeeze_risk",
      "estimation_confidence": 0.85, "estimation_confidence_pct": 85,
      "total_short_clusters": 4, "total_long_clusters": 4,
      "top_short": [
        { "peak_price": 63900, "price_low": 63800, "price_high": 63950, "notional_usd": 3200000,
          "dominant_leverage": 10, "distance_from_mid_pct": 0.8, "magnet_strength": 82, "cluster_kind": "ABOVE_CURRENT_PRICE" }
      ],
      "top_long": [ ]
    },
    "cascade_alert": { "state": "SUSTAINED", "intensity": 72.5 },
    "micro_volume_profile": {
      "poc_price": 63300, "value_area_high": 63700, "value_area_low": 63000,
      "total_volume": 12500, "range_low": 62800, "range_high": 63900,
      "num_bins": 60, "timestamp_ms": 1753950000000,
      "top_hvn": [ ],
      "buy_total": 450, "sell_total": 370,
      "buy_sell_bias": 0.0976,
      "current_position_label": "INSIDE VALUE AREA",
      "range_pos_pct": 48.18
    },
    "micro_cascade_alert": { "state": "SUSTAINED", "intensity": 72.5 }
  },
  "indicators": [
    {
      "key": "rsi", "period": 14, "fast_period": null, "slow_period": null, "signal_period": null,
      "display_name": "RSI (14)",
      "group": "Momentum", "class": "Leading",
      "raw": 63.5, "raw_display": "63.50",
      "normalized_available": true, "normalized_value": 0.31, "normalized_reason": null,
      "state": "LIVE", "state_display": "LIVE",
      "confidence_pct": 70,
      "signals": [
        { "key": "rsi_14", "display_name": "RSI (14)", "kind": "CRO", "direction": "Bullish",
          "status": "Confirmed", "label": "RSI crossed above 60", "strength": 0.8,
          "age_bars": 2, "display_label": "CRO·2" }
      ],
      "sub_values": null,
      "indicator_lifecycle": {
        "state": "Live", "state_display": "LIVE", "bars_seen": 14, "bars_required": 14,
        "last_updated_at": 1753950000, "last_error": null, "feed_state": null, "not_active": false
      }
    },
    {
      "key": "price_trend_sharpe", "period": null, "fast_period": null, "slow_period": null, "signal_period": null,
      "display_name": "Price Trend Sharpe",
      "group": "Regime", "class": "Lagging",
      "raw": 2.4, "raw_display": "2.40",
      "normalized_available": true, "normalized_value": 0.8, "normalized_reason": null,
      "state": "STRONG_POSITIVE_SHARPE", "state_display": "STRONG_POSITIVE_SHARPE",
      "confidence_pct": 80,
      "signals": [],
      "sub_values": null,
      "indicator_lifecycle": {
        "state": "Live", "state_display": "LIVE", "bars_seen": 300, "bars_required": 300,
        "last_updated_at": 1753950000, "last_error": null, "feed_state": null, "not_active": false
      }
    }
  ],
  "signals_by_kind": {
    "Divergence": [], "Crossover": [ ],
    "Threshold": [], "Breakout": [], "BandTouch": [], "ZeroLineCross": [],
    "CompressionRelease": [], "LevelTest": [], "TrendFlip": [],
    "VolumeClimax": [], "StackChange": [], "PatternForming": []
  },
  "divergences": [
    {
      "key": "rsi", "period": 14, "display_name": "RSI (14)",
      "sub_kind": "Regular Bull", "direction": "Bullish", "status": "Active",
      "strength": 0.75, "confidence_pct": 70, "age_bars": 3,
      "label": "BULLISH_DIVERGENCE",
      "pivots": [ { "time": 1753950000, "value": 34.2 } ]
    }
  ],
  "levels": [
    {
      "key": "pivot_points", "coefficient": null, "display_name": "Pivot Points",
      "level_name": "R2", "kind": "Pivot", "role": "resistance",
      "value_key": "r2", "is_range": false,
      "price_text": "$64800",
      "direction": "Bearish", "status": "Potential", "strength": 0.7,
      "confidence_pct": 40, "age_bars": 2
    }
  ],
  "liquidity_panel": {
    "flow": {
      "available": true,
      "long_liquidations_usd": 50000, "short_liquidations_usd": 15000,
      "net_liquidation_usd": 35000, "event_count": 4,
      "largest_event_usd": 30000, "largest_event_price": 63400, "largest_event_side": "LONG",
      "cascade_state": "SUSTAINED", "cascade_intensity": 72.5
    },
    "cluster": {
      "available": true, "mid_price": 63390, "cascade_asymmetry": 0.35,
      "estimation_confidence": 0.85,
      "total_long_oi_usd": 40000000, "total_short_oi_usd": 30000000,
      "total_short_clusters": 4, "total_long_clusters": 4,
      "leverage_assumptions": {
        "source": "FUNDING_ADAPTIVE", "buckets": [1,3,5,10,20,50,100],
        "weights": [0.05,0.1,0.2,0.3,0.2,0.1,0.05],
        "funding_modulation_active": true, "funding_extreme_pct": 0.0005
      },
      "short_clusters": [ ], "long_clusters": [ ]
    },
    "context": {
      "available": true, "long_oi_usd": 40000000, "short_oi_usd": 30000000,
      "estimation_confidence_pct": 85, "signals": []
    }
  }
}
```

Notes:
- `display_name` is the registry display name exactly as rendered (`RSI (14)`).
- Signal `display_label` mirrors the on-screen badge (`CRO·2`, `DIV·3` — `·` separator only when age > 0).
- WARMING entries render `raw_display: "--"` with `raw: null` — never a fabricated `0.00`. The one exception is the onoff family (squeeze): the screen's onoff branch runs before the warming check, so a warming squeeze renders `"OFF"` in both places. Normalized cells for WARMING rows and for non-Directional modes (`normalization_mode` ≠ `Directional`, e.g. the EventOnly Hull MA) emit `normalized_available: false` + a `normalized_reason` (`warming` / `context_only`) — the screen renders `--` / `N/A`.
- `state_display` mirrors the State column incl. `Warming (5/14)`, `SILENT`, `WAITING FEED ⏳`, `AWAITING DATA` and the pending-candle `⦿` suffix. When no lifecycle map is present the legacy heuristic mirrors the screen exactly (`AWAITING DATA` for WARMING, `SILENT` for Conditional/DataOnly rows without signals, `—` for empty labels).
- Liquidity ladder clusters (`top_short` / `top_long`) are the **top-4 by magnet strength** — the same selection as the anchors-strip ladder and the Levels facet "Liquidation Magnets".
- `fibonacci.status` and `volume_profile.current_position_label` are the canonical sentences also rendered by the strip and the Levels facet (`lib/structuralStrings.ts`).
- **`micro_volume_profile` / `micro_cascade_alert`** mirror the Structural Anchors strip's Volume Profile tile and the Tier-1 cascade banner, both of which are anchored to the **micro** timeframe regardless of the active TF. `volume_profile` / `cascade_alert` mirror the active-TF values (Levels facet / strip liquidity tile). Both the banner and `micro_cascade_alert` read the **snapshot-path** micro liquidity (v6.10.11) — the tf-level field retains stale values across shadow ticks.
- **L1 five-dimension synthesis (v6.10.11).** The `market_context` block carries the five dimensions (trend / momentum / volatility / volume / liquidity) with the same score + confidence + label values; the per-TF overall score and regime label surface in the Metrics LayerHeader headline.
- **Group-confluence counts (v6.10.13, M-7).** The `group_confluence` cards report the TF's RAW signal breakdown (unfiltered bull/bear/inactive counts — they are an explicit summary). The interactive facet badges and the strip's "N signals" badge are filter-aware; the group cards are intentionally not.
- Derived strings (`fibonacci.status`, `price_vs_gp_pct`, VP position label, `age_bars_display`) are computed from the **active TF's `priceText`** — the same price the screen uses. `meta.current_price` remains the freshest price across all four slots.

**EMA Ribbon block (`ema`, per-TF Metrics tab only).** Added in v6.11 — the four EMA lines (`fast` / `medium` / `slow` / `long`) read the SAME canonical record as the chart overlay (`MarketSnapshot.indicators["ema_stack"].values.*`), so every surface shows byte-identical numbers. `distance_from_price = (close − ema[role]) / close`; `spread_pct = (values.fast − values.long) / close` (positive = bull ribbon). Periods flow from the configured `ema_fast/medium/slow/long` — the same config the dashboard settings UI edits.

```json
{
  "source_tab": "metrics",
  "ema": {
    "fast":   { "value": 63420.5, "period": 10, "distance_from_price": 0.0005 },
    "medium": { "value": 63300.0, "period": 50, "distance_from_price": 0.0014 },
    "slow":   { "value": 63100.0, "period": 100, "distance_from_price": 0.0045 },
    "long":   { "value": 62800.0, "period": 200, "distance_from_price": 0.0093 },
    "spread_pct": 0.0098
  }
}
```

Notes:
- Each line's `value` is null until the EMA has warmed up (`bars_seen < bars_required`); `distance_from_price` is null when the line or close is unavailable.
- The `ema` block is additive — it does NOT replace `indicators[ema_stack].sub_values.*` and does not change the registry row. Other 6 MME tab exports do not carry it.

### 3.2 MTF Tab — `source_tab: "mtf"`

Mirrors `MtfView.svelte` (4 × N grid with per-row agreement). The meta block
carries `timeframes: ["Micro","Fast","Slow","Macro"]` (renamed from the
historical `timesframes` typo, 2026-08-17) and `timeframe_secs: 0`
(multi-TF sentinel).

```json
{
  "source_tab": "mtf",
  "meta": { "timeframes": ["Micro", "Fast", "Slow", "Macro"] },
  "header": { },
  "groups": [
    { "key": "Momentum", "label": "Momentum", "accent": "#a78bfa", "indicator_count": 2, "total_indicator_count": 2 }
  ],
  "indicators": [
    {
      "key": "rsi", "period": 14, "display_name": "RSI (14)",
      "group": "Momentum", "label": "Momentum", "class": "Leading",
      "directional": true, "visible": true,
      "normalized_available": true, "confidence_pct": 70,
      "values": [
        { "timeframe": "Micro", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "Fast",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "Slow",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "Macro", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false }
      ],
      "agreement": 0.31, "agreement_display": "+0.31", "agreement_label": "BULL"
    }
  ],
  "group_confluence": [ ],
  "signals_by_kind": { "Divergence": [], "Crossover": [ ] },
  "divergences": [ ],
  "levels": [ ],
  "liquidity_panel": { "flow": null, "cluster": null, "context": { "available": true, "signals": [] } },
  "cross_tf_tables": {
    "signals": [
      { "kind": "Threshold",
        "per_timeframe": [
          { "timeframe": "Micro", "bull": 1, "bear": 0, "neutral": 0,
            "entries": [ { "display_name": "RSI (14)", "label": "OVERSOLD", "direction": "Bullish", "status": "Active", "strength": 0.5, "age_bars": 2 } ] },
          { "timeframe": "Fast", "bull": 1, "bear": 0, "neutral": 0, "entries": [] },
          { "timeframe": "Slow", "bull": 0, "bear": 1, "neutral": 0, "entries": [] },
          { "timeframe": "Macro", "bull": 0, "bear": 1, "neutral": 0, "entries": [] }
        ],
        "totals": { "bull": 2, "bear": 2, "neutral": 0 } }
    ],
    "divergences": [
      { "key": "rsi", "display_name": "RSI (14)",
        "per_timeframe": [ { "timeframe": "Micro", "sub": "RegularBull", "strength": 0.7 }, { "timeframe": "Fast", "sub": null, "strength": 0 } ],
        "bull_count": 1, "bear_count": 0, "unknown_count": 0, "row_count": 1, "direction_label": "BULL" }
    ],
    "levels": [
      { "kind": "SupportResistance",
        "per_timeframe": [ { "timeframe": "Micro", "chips": [ { "name": "Support", "role": "support", "count": 1, "price_text": "$63,200" } ], "bull": 1, "bear": 0, "neutral": 0, "support": 1, "resistance": 0 } ] }
    ],
    "totals": { "signal_count": 4, "divergence_count": 1,
      "global_signal_lean": { "bull": 2, "bear": 2, "neutral": 0 },
      "global_divergence_lean": "BULL" }
  },
  "timeframes": [
    {
      "label": "Micro", "duration_seconds": 60, "mark_price": 63390,
      "timestamp": 1753950000, "pipeline_state": "LIVE", "is_completed": true,
      "context": null,
      "fibonacci_summary": { "present": true, "gp_top": 64050, "gp_bottom": 62600, "swing_direction": "BULL SWING", "status": "INSIDE GP (-8.97% from center)", "ext_1618": 65500, "ext_2618": 67200, "retracement_coefficients": { } },
      "volume_profile": null, "liquidity_cluster": null, "liquidity_flow": null,
      "indicators": [
        {
          "key": "rsi", "period": 14, "display_name": "RSI (14)",
          "group": "Momentum", "class": "Leading",
          "raw": 63.5, "raw_display": "63.50",
          "normalized_available": true, "normalized_value": 0.31,
          "state": "LIVE", "state_display": "LIVE",
          "confidence_pct": 70, "signals": [], "sub_values": null
        }
      ]
    }
  ]
}
```

Notes:
- Per-TF indicator rows carry the **same triple** as the single-TF Metrics
  export: `raw` / `raw_display` / `state_display` (state is humanized the
  same way — `OVERBOUGHT_DISTRIBUTION` → `OVERBOUGHT DISTRIBUTION`).
- The top-level `signals_by_kind` / `divergences` / `levels` /
  `group_confluence` / `liquidity_panel` blocks aggregate across all 4 TFs
  with the exact shapes the Metrics single-TF export uses:
  `signals_by_kind` uses the **canonical kind keys** (`LevelTest`,
  `Divergence`, `Threshold`, … — never the abbreviated `LV`/`DIV`/… tokens;
  the per-entry `kind` field stays abbreviated like the Metrics entries),
  and `divergences` / `levels` surface every Divergence / LevelTest signal
  present on any TF.
- **`visible` flags (v7.0-verify, v6.10.19d B).** The filter pills were
  removed — the grid always runs on the platform defaults, so every row
  is visible and `groups[].indicator_count` equals
  `total_indicator_count`. The per-row `visible` flag (always `true`
  under the defaults) is retained for schema stability;
  `filter_state` is gone.
- **Cell semantics (2026-08-17, audit G-4).** `values[].active` mirrors
  `MtfView` — a WARMING placeholder (`state_label === "WARMING"`) or a
  non-Directional (gated) indicator is inactive: `normalized` is zeroed,
  `normalized_display` is `"—"` (never a fabricated `+0.00`), and the
  per-cell `warming` / `gated` flags identify the cause. The agreement
  average excludes inactive cells.
- **Cross-TF tables (2026-08-17, audit G-3).** `cross_tf_tables` mirrors
  the three screen tables 1:1: `signals[].per_timeframe[].{bull,bear,
  neutral,entries}` are the kind × TF direction tallies (entries carry the
  badge tooltip data — display name, label, direction, status, strength,
  age); `divergences[]` rows are the divergence-capable indicators with
  the per-TF strongest sub-type (`RegularBull`/`RegularBear`/
  `HiddenBull`/`HiddenBear`/`null`) and bull/bear/unknown counts;
  `levels[].per_timeframe[].chips[]` are the actual level chips
  (name/role/count/price text) with direction and S/R splits. The
  flattened `signals_by_kind` / `divergences` / `levels` aggregates above
  remain for single-TF-style consumers.

### 3.3 Alignment Tab — `source_tab: "alignment"`

```json
{
  "source_tab": "alignment",
  "meta": { },
  "header": { },
  "hero": {
    "mtf_overall_score": 62.0, "mtf_overall_label": "STRONG_BULL_MTF",
    "mtf_overall_label_display": "STRONG BULL",
    "timeframes_present": 4, "signal_cross_tf_count": 2, "trend_agreement_pct": 82
  },
  "dimensions": [
    { "name": "Trend", "score": 75, "state": "STRONG", "confidence": 78 }
  ],
  "consensus": {
    "trend_agreement_pct": 82,
    "label": "strong_consensus",
    "label_display": "Strong Consensus",
    "axes": [
      { "key": "Trend",      "label": "Trend",      "value": 0.70, "value_display": "+0.70" },
      { "key": "Momentum",   "label": "Momentum",   "value": 0.60, "value_display": "+0.60" },
      { "key": "Volume",     "label": "Volume",     "value": 0.50, "value_display": "+0.50" },
      { "key": "Volatility", "label": "Volatility", "value": 0.40, "value_display": "+0.40" }
    ]
  },
  "per_timeframe": [
    { "timeframe": "MICRO", "trend_score": 0.70, "trend_score_display": "0.70",
      "momentum_score": 0.60, "momentum_score_display": "0.60",
      "overall_score": 1.0, "overall_score_display": "1.0",
      "regime": "TRENDING", "active_signals": 5 }
  ],
  "score_calculation": {
    "weights": [
      { "key": "Trend", "label": "Trend", "pct": 50, "color": "#22c55e",
        "value": 0.70, "value_display": "+0.70", "contribution": 0.35, "contribution_display": "+0.35" }
    ]
  },
  "interpretation": "Multi-timeframe alignment shows <strong>strong directional consensus</strong> (82% agreement across 4/4 timeframes). The composite score of +62 is classified as <strong>STRONG BULL</strong>. 2 cross-timeframe signal votes reinforce the current bias.",
  "composition_note": "Composition weights: Trend (fifty percent), Momentum (thirty percent), Volatility (ten percent), and Volume (ten percent).",
  "consensus_conflict_banner": ""
}
```

Notes:
- **Consensus hero (v7.0.1 B).** The alignment header carries the `TFs` chip only — the `Agreement` chip (v6.10.19d) and the `Score` chip (v7.0.1) are both removed from the chrome. The header-container hero row is now two side-by-side circular dial cards — an **AGREEMENT dial** (percentage + tier-colored verdict header + grey sub-label) and a **SCORE dial** (composite blend: ring filled `|score|%`, sign-colored, signed integer centered, prettified `mtf_overall_label` + tone explanation). The CONSENSUS 2×2 axis grid is erased from the screen (the four axis values still surface in the `score_calculation.weights` chips); the `consensus.axes` field stays in the JSON. The "Polarization" term is retired (the `consensus.polarization` field is renamed `consensus.axes`); the `label_display` mirrors the agreement dial verdict header verbatim (`"Strong Consensus"`), while the sub-label (`"Timeframes are aligned."`) stays DOM-only. The `consensus_conflict_banner` renders as a full-width strip under the dial row.
- Dimension `confidence` is already 0..100 on the wire — it mirrors the screen's `confidence.toFixed(0)%` (no ×100 inflation).
- `interpretation` carries the real `mtf_overall_label` (never a hardcoded token) and matches the panel sentence verbatim (HTML `<strong>` markers included).
- **Interpretation fixes (v7.1).** The prose prints the **exact signed score string** the SCORE dial renders (e.g. `+62`) — the previous unsigned `toFixed(1)` form (`62.0`/`8.1`) beside the dial's signed integer was read as score drift; both surfaces now bind to the same live `mtf_overall_score`. When the composite classifies `NEUTRAL`, the ≥75% agreement branch reads **"moderate consensus"** (never "strong directional consensus" — neutral is directionless by definition) and closes with "the dimensions offset into a flat composite." The panel's Interpretation card additionally renders a **Consensus Composition Strip** (four directional segments, width = live `blend_weights`, color = axis sign, opacity = |axis| intensity) and a **whisper footnote** under a hairline rule; `composition_note` mirrors that footnote in full words (order Trend, Momentum, Volatility, Volume), `null` while data is still populating.
- `consensus.trend_agreement_pct` keeps the raw float (screen text `toFixed(0)`, bar width `toFixed(1)`).
- The `NO_DATA` dimension state renders `"NO DATA"` on both surfaces (panel and export).
- **Formula line removed (v6.10.19d A).** `score_calculation.formula` was removed with the on-screen formula line — the block carries the weight chips only. The backend math is unchanged: `mtf_overall_score = 100·(0.5·T + 0.3·M + 0.1·V_t + 0.1·V_m)` on signed axes `[−1, 1]` ([02-01 §4.2](../matrices/02-01-alignment-matrix.md)); the equation just no longer prints. `breakdown_meta` (the `Trend:0.70 …` caption) was removed with the duplicate breakdown text — the sign-prefixed axis values live in `consensus.axes[].value_display` and the weight chips.
- **Full-word weight keys (v6.10.18).** The `blend_weights` / `score_calculation.weights` / `consensus.axes` keys are the full dimension names (`"Trend"`, `"Momentum"`, `"Volume"`, `"Volatility"`) — never abbreviations. The legacy wire keys `"T"` / `"M"` / `"Vt"` / `"Vm"` bound Volume/Volatility **swapped** vs. the spec (`V_t` = volatility, `V_m` = volume), so the panel previously mislabeled the chips as `Vt Volume` / `Vm Volatility`; consumers still normalize legacy payloads (`"Vt"` → Volume, `"Vm"` → Volatility, matching the legacy wire's actual bindings).
- **Low-agreement wording (v6.10.10).** The sub-50 consensus label reads `"Mixed consensus — timeframes not aligned"` and the banner `"TIMEFRAME MISALIGNMENT — …"` — "conflict" overstated the case where low agreement comes from undecided (neutral) TFs.
- **Sentinel gate (v6.10.10).** The backend's warmup sentinel (`timeframes_present: 0`, label `NO_DATA`, agreement 0) renders the awaiting consensus (`trend_agreement_pct: null`, `label: null`, `label_display: "—"`) and the awaiting interpretation — never a fabricated "Conflict" verdict. The 10 `NO_DATA` dimension rows are kept.
- **Null state (`alignment: null`).** The consensus block mirrors the screen placeholders exactly: `trend_agreement_pct: null`, `label: null`, `label_display: "—"`, axes `value_display: "+0.00"`, and score-calculation `value_display` / `contribution_display` `"—"` — never fabricated zeros or a fabricated "Conflict" verdict.

### 3.4 Opportunity Tab — `source_tab: "opportunity"`

```json
{
  "source_tab": "opportunity",
  "meta": { },
  "header": { },
  "directional_bars": { "bullish_pct": 60, "bearish_pct": 10, "hold_pct": 30, "sort": "desc" },
  "trade_setups": [
    {
      "opportunity_type": "Trend Continuation", "viability": "Actionable",
      "badge_text": "TOP · ACTIONABLE", "side": "LONG", "rank_idx": 0, "is_top": true,
      "quality": "STRONG", "below_floor": false,
      "geometry_consistent": true,
      "entry_mid": 63300, "entry_zone": { "low": 63200, "high": 63400 },
      "tp1": 66000, "tp2": 66500, "invalidation": 62800,
      "rr_available": true, "rr_value": 2.5, "rr_reason": null,
      "score": 78, "preconditions_met": 3, "preconditions_total": 3,
      "notes": "Trend + bias + momentum aligned"
    },
    {
      "opportunity_type": "Mean Reversion", "viability": "DirectionalNeutral",
      "badge_text": "RANGE · NEUTRAL", "side": "NEUTRAL", "rank_idx": 1, "is_top": false,
      "quality": "MODERATE", "below_floor": false,
      "geometry_consistent": false, "entry_mid": null, "entry_zone": null,
      "tp1": 0, "tp2": 0, "invalidation": null,
      "rr_available": false, "rr_value": null, "rr_reason": "no_actionable_geometry",
      "score": 42, "preconditions_met": 2, "preconditions_total": 3, "notes": "Reversion candidate"
    }
  ],
  "rr_internal": {
    "expected_rr_available": true, "expected_rr_value": 2.5, "expected_rr_reason": null,
    "gross_rr_value": 2.52, "time_horizon": "SWING"
  },
  "confluent_rr": {
    "sides": [
      { "side": "LONG", "entry_avg": 63330, "target_avg": 66000, "invalidation_avg": 62800,
        "risk_basis": "invalidation", "rr": 2.5, "rr_display": "2.5R", "reason": null }
    ],
    "reason": null
  },
  "invalidation_note": "A close below 62800 on the completed candle invalidates the Trend Continuation thesis.",
  "evaluated_setups": [
    { "opportunity_type": "Trend Continuation", "viability": "Actionable", "score": 78,
      "preconditions_met": 3, "preconditions_total": 3, "trade_viability": "Actionable",
      "notes": "Trend + bias + momentum aligned" }
  ],
  "confluent_entry_levels": [
    { "price": 63330, "sources": ["FIB", "VP", "PP"], "strength": 78, "strength_label": "STRONG", "side": "SHORT" }
  ],
  "confluent_target_levels": [ ],
  "market_position": { "bias": "Bullish", "regime": "TRENDING", "trend": "Healthy", "quality": "Good" },
  "environment": { "timeframes_considered": 4, "timeframes_considered_display": "4/4 TFs considered", "confidence_pct": 72, "confidence_display": "72%" },
  "summary": "Trend continuation setup: entry 63.3K, targets 66K/66.5K, invalidation 62.8K, R:R 2.5",
  "summary_display": "Trend continuation setup: <strong>entry 63.3K</strong>, targets 66K/66.5K, invalidation 62.8K, R:R 2.5",
  "summary_label": "OPPORTUNITY SUMMARY"
}
```

Notes:
- `trade_setups` mirrors the panel's full leaderboard **one card per qualifying profile** — NEUTRAL-side cards (`side: "NEUTRAL"`, `RANGE · NEUTRAL`) and reference-bracket rows included. v6.10.19c: the dedicated empty-state fields were removed — the NEUTRAL/BULL/BEAR sections are the container for the empty state (they always render, with empty lists allowed).
- **`summary` / `summary_display` / `summary_label` (v7.0/v7.2)** — the shared OPPORTUNITY SUMMARY card parity block: `summary` is the plain-text paragraph the panel renders (shared generator with the card), `summary_display` is the keyword-highlighted HTML variant, `summary_label` is the card's heading.
- **`confluent_rr` (2026-08-17, audit C2; v7.3 bracket-geometry fallback)** — the per-side "Expected Reward-to-Risk Ratio" section the panel renders via `computeConfluentRr`: one row per side with a complete entry+target level set (`entry_avg`/`target_avg`/`invalidation_avg` means of the confluent levels, `risk_basis: "invalidation" | "market_distance"`, `rr` rounded to 2 decimals or `null` with a `reason` like `"degenerate geometry"`, `rr_display` the exact magnitude label — `"1.5R"`, `"10R+"`, or `"N/A"`). **v7.3:** a side whose confluent set is incomplete (no entry or no target levels — e.g. the LONG side of a NoClear state with only SHORT-tagged confluent levels) falls back to the matrix's per-side bracket zones (the reference-bracket geometry): `entry_avg`/`target_avg` become the zone midpoints, `invalidation_avg` the zone invalidation, and `risk_basis: "bracket_geometry"` flags the synthesized row. The fallback only fires while at least one side-tagged confluent level exists somewhere, so the `"no confluent levels"` / `"incomplete confluent levels"` empty-states never fabricate rows from zones alone. `reason` at block level reports the global no-row cause (`"no confluent levels"` / `"incomplete confluent levels"`). Distinct from `rr_internal` (the resolved active-side R:R from the decision chain).
- **`trade_setup_sections` (v6.10.19b C2, v6.10.19c naming; v7.1 ranked order)** — the nested view mirroring the sectioned panel 1:1 — always present (empty lists allowed), top-ranked first within each. **v7.1:** the three folders render in **RANKED order** — the folder with the most content (setups + reference bracket) first, then by its top setup's score — the same relevance ordering as the directional conviction bars (a lone BEARISH setup puts the BEARISH folder first). The fixed RANGE → BULLISH → BEARISH fallback applies only to empty ties:

```json
{
  "trade_setup_sections": [
  { "section": "NEUTRAL", "label": "RANGE SETUPS", "setups": [
    { "opportunity_type": "Mean Reversion", "viability": "DirectionalNeutral",
      "badge_text": "RANGE · NEUTRAL", "side": "NEUTRAL", "score": 42, "score_display": 28,
      "quality": "MODERATE", "below_floor": false,
      "preconditions_met": 2, "preconditions_total": 3,
      "entry_zone": null, "tp1": 0, "tp2": 0, "invalidation": null,
      "rr_available": false, "rr_value": null, "rr_reason": "no_actionable_geometry",
      "geometry_consistent": false, "notes": "Reversion candidate" }
  ]},
  { "section": "BULL", "label": "BULLISH", "setups": [
    { "opportunity_type": "Trend Continuation", "viability": "Actionable",
      "badge_text": "TOP · ACTIONABLE", "side": "LONG", "score": 78, "score_display": 78,
      "quality": "STRONG", "below_floor": false,
      "preconditions_met": 3, "preconditions_total": 3,
      "entry_zone": { "low": 63200, "high": 63400 }, "tp1": 66000, "tp2": 66500,
      "invalidation": 62800, "rr_available": true, "rr_value": 2.5, "rr_reason": null,
      "geometry_consistent": true, "notes": "Trend + bias + momentum aligned" }
  ]},
  { "section": "BEAR", "label": "BEARISH", "setups": [] }
  ]
}
```

Every row carries the FULL value set (entry zone low/high, TP1, TP2, SL/invalidation, R:R value/available/reason, score + score_display, preconditions, geometry_consistent, viability, badge_text, **quality**, **below_floor**, notes, section). **v6.10.23:** reference brackets ride inside their directional section as rows — a section hosting zero qualifying setups carries its aggregated bracket (LONG → BULL, SHORT → BEAR) and/or the backend neutral range frame (RANGE, `opportunity_type: "Neutral Reference Bracket"`), each only when it carries real zones (`zones != null`), with `below_floor: true` + `badge_text: "BELOW ACTIONABLE FLOOR"` when its R:R is sub-1.0 or its geometry is inconsistent.
- `viability` is the **PascalCase-normalized** token (`Actionable` / `DirectionalNeutral` / `GeometryInverted` / `NoClear`). The wire serializes `TradeViability` as SCREAMING_SNAKE_CASE (`ACTIONABLE`); the panel conditionals and the export's `badge_text` both compare on the normalized form, so `badge_text` (`TOP · ACTIONABLE`, `ACTIONABLE`, `QUALIFYING`, `RANGE · NEUTRAL`, `GEOMETRY INVERTED`, `INFORMATIONAL`, `BELOW ACTIONABLE FLOOR`) matches the screen exactly. **v6.10.23 badge policy:** EVERY Actionable card carries the actionable badge (the HOLD-verdict gate is removed — `TOP · ACTIONABLE` for the top-ranked Actionable card, plain `ACTIONABLE` for the rest); a card with inconsistent geometry always renders `GEOMETRY INVERTED`.
- `quality` (**v6.10.23**) is the quality band of the **displayed** (precondition-scaled) score — `PRIME` ≥85 / `STRONG` 70–84 / `MODERATE` 50–69 / `MARGINAL` 30–49 / `NONE` <30 — mirroring the per-card outlined pill; `null` on reference rows (the screen renders no pill there).
- `rr_value` prefers the wire per-side `expected_rr_internal` exactly like the screen card.
- Confluent levels are capped at the screen's first 4 per group; sources are the on-screen abbreviations (FIB/VP/PP/SR/LIQ, with the screen's `ATR` fallback for unknown tokens). **v6.15:** `strength_label` mirrors the panel's qualitative pill band (`WEAK` <30 / `MODERATE` 30–54 / `STRONG` 55–79 / `VERY STRONG` ≥80) computed from `strength` via the shared `confluenceStrength` helper — the screen no longer renders the raw additive weight as a percentage (it read like a probability).
- The evaluated list excludes the NoClearOpportunity profile (it has its own strip).
- `directional_bars` is **always** emitted — when the matrix is absent it mirrors the screen's always-rendered bars as `{0, 0, 100}`. The split is direction-aware (v6.10.6): the effective direction is the top qualifying profile's resolved side (`selectProfileSide`), else the macro bias, else the argmax per-side R:R; conviction weights **only the active side's** R:R (`exp(RR·3)` vs a hold floor, capped by `opportunity_score` and floored at 30% (v6.10.12, `MIN_ACTIVE_FLOOR`) whenever a valid active-side bracket exists — so a bearish panel can never emit bullish-dominant bars from a countertrend long bracket, and a `NO CLEAR SETUP` (score 0) matrix with a real bracket emits a ~30/70 directional split instead of collapsing to `{0, 0, 100}`. **v7.1 (L4-only):** the bars read **only** the L4 opportunity matrix — the L6 decision-context probabilities (`long_probability` / `short_probability` / `hold_probability`) never shape them, and the L6-only `lean_floor_applied` flag was removed from the block (the LEAN annotation is the Recommendation gauge's marker, not this panel's).
- `expected_rr` mirrors the screen cell exactly: `N/A` only when the verdict is HOLD **and** the active-side R:R is 0 (degenerate ratios below the 0.1 meaningfulness floor read as 0); any other state emits `available: true` with the raw value (including `0` → screen `"0.00"`).
- `evaluated_setups[].notes` / `trade_setups[].notes` are the **raw wire strings** the panel renders verbatim (never prettified).
- Empty states render the screen's `"—"` placeholder in `rr_internal.time_horizon` and all four `market_position` fields. (**v7.1:** `header_block` was removed with the panel's TOP SETUP hero — the top setup lives on the Recommendation tab; the Opportunity `header` block's badge + chip rail carry the matrix class/score/R:R/horizon.)

### 3.5 Risk Tab — `source_tab: "risk"`

```json
{
  "source_tab": "risk",
  "meta": { },
  "header": { },
  "hero": {
    "overall_score": 48, "overall_level": "Moderate", "overall_state": "Elevated",
    "overall_confidence": 74, "top_severity": "High"
  },
  "summary_counts": {
    "very_low": { "label": "Very Low", "count": 0 },
    "low": { "label": "Low", "count": 3 },
    "moderate": { "label": "Moderate", "count": 2 },
    "high": { "label": "High", "count": 3 },
    "extreme": { "label": "Extreme", "count": 0 }
  },
  "dimensions": [
    {
      "name": "Cascade Risk", "key": "cascade_risk",
      "weight": 0.14, "weight_pct": 14,
      "score": 70, "level": "High", "state": "Critical",
      "state_display": "↑ ELEVATED", "confidence": 85,
      "evidence": ["SUSTAINED cascade above price"],
      "no_evidence_text": null, "not_active_text": null,
      "awaiting": false, "awaiting_badge": null,
      "bar_pct": 70, "weight_mark_pct": 14,
      "is_cascade_dim": true, "not_active": false,
      "cascade_extras": {
        "state_label": "SUSTAINED", "intensity_display": "72.5",
        "asymmetry_sign": "+", "asymmetry_magnitude_pct": 35.0,
        "asymmetry_description": "short squeeze",
        "asymmetry_display": "↑35.0% (short squeeze)"
      }
    },
    {
      "name": "Execution Risk", "key": "execution_risk",
      "weight": 0.10, "weight_pct": 10,
      "score": 35, "level": "Low", "state": "Stable",
      "state_display": "→ COMPOSED", "confidence": 50,
      "evidence": ["Favorable volatility-to-spread (9.2)"],
      "no_evidence_text": null, "not_active_text": null,
      "awaiting": false, "awaiting_badge": null,
      "bar_pct": 35, "weight_mark_pct": 10,
      "is_cascade_dim": false, "not_active": false,
      "cascade_extras": null,
      "execution_extras": { "volatility_to_spread_ratio": 9.2 }
    }
  ],
  "interpretation_full": "<strong>Elevated risk environment.</strong> 3 dimensions at high levels. Consider reduced position sizing and wider stops. Monitor the highest-severity dimensions for evidence of improvement before committing capital. Overall composite score is <strong>moderate</strong> at 74% confidence.",
  "disclosure": {
    "weights": [
      { "label": "Market", "pct": 14 }, { "label": "Volatility", "pct": 14 },
      { "label": "Execution Liquidity", "pct": 14 }, { "label": "Structure", "pct": 10 },
      { "label": "Momentum", "pct": 14 }, { "label": "Signal", "pct": 10 },
      { "label": "Execution", "pct": 10 }, { "label": "Cascade", "pct": 14 }
    ],
    "note": "Overall risk is a weighted sum of the eight dimension scores. Hover a segment for its full name and weight. The state chip describes the risk trend (elevating / improving / stable); it does not change the score."
  },
  "awaiting_dimensions_text": "Awaiting risk assessment — this dimension will populate once market data stabilizes."
}
```

Notes:
- `top_severity` is `null` when it equals the overall level (the screen hides the "Top risk:" label in that case — renamed from "peak:" in v6.10.19d C).
- `asymmetry_magnitude_pct` is the screen's percentage (`|asym| × 100`); `asymmetry_display` is the exact badge sentence.
- Zero-count sentences are omitted from `interpretation_full` exactly like the screen paragraph.
- Dimension names are **byte-identical to the screen cards** — including the full `"Execution Liquidity Risk"` (v6.16: the former `"Exec Liquidity Risk"` abbreviation was unified).
- **Risk state (v6.10.9) + Scheme-A badge (v6.10.19d C, display-only).** `state` is functional — the backend derives `CRITICAL` (score ≥ 80) / `ELEVATED` (score ≥ 60) / `INCREASING` / `IMPROVING` / `STABLE` (previous-synthesis delta ±10). The dashboard maps each dimension to ONE Scheme-A token badge: trend states win (`INCREASING → RISING ↗`, `IMPROVING → IMPROVING ↘`), otherwise the LEVEL maps to its token (`Extreme→CRITICAL ⚠`, `High→ELEVATED ↑`, `Moderate→STEADY →`, `Low→COMPOSED →`, `VeryLow→MINIMAL →`). The level name keeps its wording (Extreme/High/…) tinted by the token color. `state_display` carries `"{icon} {TOKEN}"` verbatim from the screen. Display-only: the mapping never feeds back into the backend.
- **Warmup sentinel gate (v6.10.9).** The backend's empty matrix (`RiskMatrix::empty` — every dimension AND the overall at exactly `50`/`Moderate` with no evidence) is treated as awaiting: `hero: null`, all 8 `awaiting` rows, and the initializing `interpretation_full` — never fabricated "Moderate risk" data. Consumers can reuse `isAwaitingRiskMatrix` (exported from `exportBuilders/riskTab.ts`).
- **Execution friction gauge (v6.11, v6.16 label).** The `execution_risk` dimension carries `execution_extras: { volatility_to_spread_ratio }` (`Average True Range(14) ÷ top-of-book spread` — the field label reads "Average True Range to Spread" and the value renders without the `×` suffix since v6.16; raw price units) when the backend publishes it; every other dimension (and the awaiting/null rows) carries `execution_extras: null`.
- **Disclosure/hint copy (v6.10.9, v6.10.19d C, v6.16).** The state chip is descriptive — it does not modify the weighted sum. The hero caption ("Lower is safer. …") was removed in v6.10.19d — the risk bar carries the guidance as a tooltip only, so the export has no `hero.hint` string. **v6.16:** the "How is overall risk computed?" accordion and its 8-chip weight grid were replaced by a horizontal segmented weight strip inside the Risk Summary card — segment widths are the weights, hover tooltips carry the full dimension name + weight, and the caption reads "Overall risk is a weighted sum of the eight dimension scores. Hover a segment for its full name and weight." The export keeps the structured `disclosure.weights` (labels now write the execution-liquidity dimension in full: `"Execution Liquidity"` — the `"ExecLiq"` contraction is gone) with `note` = the screen caption + the state-chip clarification. The old "State and confidence modify each dimension's contribution" claim was never implemented and has been removed.
- **Header trailing headline removed (v7.1).** The panel header no longer renders the "3 high · 2 moderate · overall moderate" trailing slot (title-only header, consistent with every other tab) — the export was stripped to match: there is no `headline_parts` / `interpretation_headline` field. Per-level distribution lives in `summary_counts` (the tiles).
- **Null state (`risk: null`).** `dimensions` carries the 8 placeholder rows the screen's "AWAITING" cards render (name + `weight_pct`, `awaiting: true`, `awaiting_badge: "AWAITING"`), and `interpretation_full` carries the "Risk synthesis is initializing — …" paragraph verbatim.

### 3.6 Analysis Tab — `source_tab: "analysis"`

```json
{
  "source_tab": "analysis",
  "meta": { },
  "header": { },
  "body": {
    "bias": "Bullish", "confidence_pct": 72, "state_confidence": 0.72,
    "market_regime": "TrendingBull", "market_quality": "Good", "cycle_phase": "Markup"
  },
  "signal_lean_hero": {
    "label_html": "Net bullish (2↑ vs 1↓)",
    "meta_html": "2.0:1 signal ratio",
    "bullish_pct": 67, "bearish_pct": 33, "tone": "bull"
  },
  "signals": {
    "supporting": [
      { "key": "rsi", "period": 14, "display_name": "RSI 14", "timeframe": "MICRO",
        "score": 62, "score_display": "+62", "regime": "TRENDING",
        "signals_count": 3, "signals_count_display": "3",
        "raw": "MICRO (bullish): rsi_14 score +62, TRENDING regime, 3 signals" }
    ],
    "contradicting": [ ],
    "list": [
      { "key": "rsi", "period": 14, "display_name": "RSI 14", "timeframe": "MICRO",
        "score": 62, "score_display": "+62", "regime": "TRENDING",
        "signals_count": 3, "signals_count_display": "3",
        "raw": "…", "bucket": "supporting" }
    ],
    "lean": { "label": "Net bullish · 2↑ vs 1↓", "bullish": 2, "bearish": 1, "tone": "bull" }
  },
  "qualitative_assessment": {
    "trend": "Healthy", "momentum": "Increasing", "structure": "Strong",
    "volatility": "Normal", "volume": "Strong", "cycle_phase": "Markup",
    "trend_score": 76.5, "trend_score_display": "76.50",
    "momentum_score": 83.2, "momentum_score_display": "83.20",
    "structure_score": 81.4, "structure_score_display": "81.40",
    "volatility_score": 55.0, "volatility_score_display": "55.00",
    "volume_score": 78.8, "volume_score_display": "78.80"
  },
  "per_timeframe_alignment": [
    { "name": "MICRO", "active": true, "trend": 0.45, "trend_display": "+0.45",
      "momentum": 0.3, "momentum_display": "+0.30",
      "overall": 1.0, "overall_display": "+1.0", "regime": "TRENDING" },
    { "name": "MACRO", "active": true, "trend": -0.1, "trend_display": "-0.10",
      "momentum": -0.05, "momentum_display": "-0.05",
      "overall": -0.2, "overall_display": "-0.2", "regime": "RANGE" }
  ],
  "interpretation": "Price is making higher highs and higher lows on strong volume. Momentum is increasing and structure remains intact.",
  "interpretation_display": "Price is making <strong>higher</strong> highs and higher lows on <strong>strong</strong> volume. Momentum is <strong>increasing</strong> and structure remains intact.",
  "rationale": "The market is in a healthy uptrend with broad participation across timeframes.",
  "representative_bbwp": 83.3,
  "representative_adx": 33.0,
  "key_metrics": {
    "mtf_overall_score": 75,
    "trend_agreement_pct": 75,
    "timeframes_present": 4,
    "signal_cross_tf_count": 3
  }
}
```

Notes:
- `signals.list` is the exact merged, timeframe-sorted list the screen grid squares render (bucket-annotated).
- **L3 header chip (v6.10.13, M-3).** The L3 header chip is labelled **`State Conf`** (it reads `state_confidence`), distinct from the L6 risk-discounted `Confidence` chip.
- **L3 warmup sentinel (v6.10.13, M-2).** An analysis with `timeframes_considered === 0` (the backend's empty-matrix sentinel) renders the null-state payload — never the fabricated Neutral/Poor/Transition values the sentinel carries.
- With zero directional signals the hero still carries the screen sentences (`label_html: "No signals"`, `meta_html: "Waiting for cross-TF consensus"`) — including when `analysis` is null; the hero block is never absent.
- **Signal lean hero (v6.10.16 FIX-O2).** Under a NEUTRAL market bias a directional TF vote renders amber with a "market bias neutral" qualifier instead of a green hero under the NEUTRAL badge — raw counts stay visible: `label_html: "TF votes: Net bullish (4↑ vs 0↓)"`, `meta_html: "4:0 signal ratio · market bias neutral"`, `tone: "split"`. Under a directional bias the hero is unchanged.
- **Neutral signals (v6.10.8).** The hero distinguishes *no data yet* (empty `supporting_signals`/`contradicting_signals` → `"No signals"` / `"Waiting for cross-TF consensus"`) from *all timeframes neutral* (non-empty lists but zero bullish/bearish → `label_html: "Neutral signals"`, `meta_html: "No directional lean across timeframes"`, tone `split`). The screen renders neutral signals with a gray square + flat dash icon (never the bearish red styling).
- **Zero-opposing ratio (v6.10.8).** `meta_html` renders `"3:0 signal ratio"` (or `"0:3"`) when the opposing count is zero — never a misleading `"3:1"` that implies opposing signals exist. The `2.0:1` format is unchanged when both sides have counts.
- The split-tone hero label is exactly the screen's `"Split signals"` (the bull/bear counts live in `meta_html` — no parenthetical in `label_html`).
- Each signal row carries `signals_count_display` (`"—"` when the count is absent) alongside the raw `signals_count`.
- Empty states render the screen's `"—"` placeholder in the five qualitative cards, `cycle_phase`, the inactive per-TF score displays and `rationale`.
- `interpretation` is the raw text; `interpretation_display` carries the keyword-`<strong>` markup the screen renders (shared `highlightKeywords` helper).
- `cycle_phase` uses the shared `prettifyPhase` helper — identical string on screen and in the JSON.
- **Per-card dimension scores (v6.12).** `qualitative_assessment.trend_score` / `momentum_score` / `structure_score` / `volatility_score` / `volume_score` are the exact 0-100 alignment dimension scores each qualitative label is bucketed from (the badges on the Analysis cards; see [02-02-analysis-matrix.md §3.4.1–3.7.1](../matrices/02-02-analysis-matrix.md)). Each `_display` is the verbatim screen string — **v6.13:** the rounded integer + `%` form (e.g. `"77%"`; the `%` makes the cross-timeframe agreement semantics explicit), `"\u2014"` when the field is absent (`null`, empty sentinel). Raw and display always agree with the emitted label — the label IS the band.
- **Trend Stability Sharpe removed (v6.14).** The v6.11 `qualitative_assessment.trend_stability_sharpe` pair was **removed** with the L1→L3 traceability-evidence stamp (the Trend card badge and the `AnalysisMatrix` field are gone). The L1 `price_trend_sharpe` indicator remains the sole Sharpe family member (Metrics tab, per-TF — see [04-02-52](../engines/market-monitoring-engine/indicators/04-02-52-price-trend-sharpe.md)).
- **Representative traceability (v6.10.21).** `representative_bbwp` / `representative_adx` now come from the AnalysisMatrix's own pinned `representative_bbwp`/`representative_adx` fields (the exact inputs the rationale quotes — the matrix mirror is per-slot last-writer-wins, so the exporting slot's indicator map can differ); the micro-map fallback applies only to older frames that lack the pins.
- **KEY METRICS parity (2026-08-17, audit C2).** `key_metrics` mirrors the panel's KEY METRICS row — Overall Score (`alignment.mtf_overall_score`), Timeframe Agreement (`trend_agreement_pct`), Timeframes present (`alignment.timeframes_present`, falling back to `analysis.timeframes_considered`), Total Signals (`signal_cross_tf_count`). Any field is `null` when the alignment is absent.
- The Interpretation's opportunity sentence follows the L3 `opportunity_analysis` chain, which is synced with the L4 §4 tree (v6.10.8) — the prose can no longer claim "Favors trend continuation" under an L4 NO CLEAR SETUP verdict.

### 3.7 Recommendation Tab — `source_tab: "recommendation"`

```json
{
  "source_tab": "recommendation",
  "meta": { },
  "header": { },
  "gauge": {
    "net_bias_pct": 45, "bias_direction": "LONG",
    "long_pct": 60, "short_pct": 15, "hold_pct": 25,
    "net_bias_display": "+45%",
    "lean_floor_applied": false
  },
  "environment": {
    "directional_guidance": "Long", "market_stance": "Constructive",
    "strategy_environment": "TrendFollowing", "opportunity_classification": "TrendContinuation",
    "confidence_pct": 72, "readiness": "READY",
    "entry_danger_score": 35, "entry_danger_level": "LOW",
    "quality_to_risk_ratio": 1.5, "quality_to_risk_ratio_display": "1.50"
  },
  "verdict": { "top": "LONG", "long_probability": 60, "short_probability": 15, "hold_probability": 25 },
  "top_setup_empty_text": null,
  "top_setup": {    "opportunity_type": "Trend Continuation", "viability": "Actionable",
    "badge_text": "ACTIONABLE", "score": 78, "score_display": 78,
    "preconditions_met": 3, "preconditions_total": 3, "direction_label": "LONG",
    "entry_zone": { "low": 63200, "high": 63400 }, "target_zone": { "low": 66000, "high": 66500 },
    "invalidation": 62800,
    "entry_zone_display": "$63200–$63400", "target_zone_display": "$66000–$66500",
    "invalidation_display": "$62800",
    "rr_display": "2.50",
    "rr_available": true, "rr_value": 2.5, "rr_reason": null,
    "rationale": "TrendContinuation: preconditions 3/3",
    "alternate_qualifying_setups": [
      { "opportunity_type": "Mean Reversion", "side": "NEUTRAL", "score": 42,
        "preconditions_met": 2, "preconditions_total": 3 }
    ]
  },
  "safety_flags": {
    "readiness": "READY",
    "rr_available": true, "rr_value": 2.5, "rr_reason": null,
    "risk_adj_rr_explanation": null,
    "stop_loss_pct": 0.01, "confidence_pct": 72,
    "entry_danger_score": 35, "entry_danger_level": "LOW",
    "rr_display": "2.50",
    "stop_loss_display": "1.00%", "confidence_display": "72%",
    "entry_danger_display": "35 (LOW)",
    "quality_to_risk_ratio": 1.5, "quality_to_risk_ratio_display": "1.50"
  },
  "why_note": null,
  "why": [
    "Bullish market bias: confluence score of 62 (derived from the Layer 2 Tradability Dimension, Layer 3 Quality Score, and Layer 4 Opportunity Score)",
    "Active setup: Trend Continuation (Layer 4 Opportunity Score of 78, classified as Strong quality)",
    "Trade readiness is Ready: Entry Danger of 35 (Low) is acceptable"
  ],
  "price_levels": {
    "side": "long",
    "entry_zone": { "low": 63200, "high": 63400 }, "target_zone": { "low": 66000, "high": 66500 },
    "invalidation": 62800, "horizon": "SWING", "hold_placeholder": null
  },
  "strategy": {
    "entry": "Pullback", "exit": "Trend Weakening",
    "protection": "ATR-Based", "target": "Resistance-Based"
  },
  "final_verdict": "LONG 60% — Ready (readiness: Ready).",
  "final_verdict_guidance": "Environment guidance: Bullish market bias with 60% confidence, Long on pullback toward the 63200-63400 entry zone with invalidation below 62800.",
  "projection": {
    "configured": true,
    "capital": 10000, "leverage": 20, "direction": "LONG",
    "entry_price": 63300, "stop_loss": 62800, "take_profit": 66000,
    "position_size_units": 1.58, "position_notional_usd": 10000,
    "entry_fee_usd": 6.0, "exit_fee_usd": 8.5, "total_fees_usd": 14.5,
    "liquidation_price": 62010, "net_profit_usd": 347.6, "roi_pct": 3.48
  }
}
```

> **`projection` (v6.10.4+).** Mirrors the Project Risk and Return drawer grid — `configured: false` with all-null numerics until a calculation runs (the drawer's `margin_required` cell has no export counterpart).

Notes:
- `rr_display` in `top_setup` derives from the canonical wire `rr_value`
  (`long_/short_expected_rr_internal`, target-mid geometry) with the same
  single-decimal formatting as the Safety-Flags KPI (`2.50`, `9.99+`) —
  the payload never recomputes an independent geometry (a legacy
  `computeRiskReward` recompute here disagreed with the chip and the
  cards; see the export-consistency regression tests).
- `safety_flags.*_display` are verbatim KPI-chip strings (`2.50`, `1.00%`, `72%`, `35 (LOW)`).
- **Quality-to-Risk ratio (v6.11).** `environment.quality_to_risk_ratio` and `safety_flags.quality_to_risk_ratio` mirror the Quality/Risk KPI chip (`market_quality_score ÷ overall_risk.score`, 2-dp `_display`, `"\u2014"` when `null` — the dual placement mirrors `entry_danger`).
- **`risk_adj_rr_explanation` (v6.10.14 / RR-008)** — the first-class R:R discount sentence, e.g. `"Risk-adjusted: geometric R:R 2.00 × risk factor 0.30 = 0.60"` — the exact string the Safety-Flags KPI tooltip renders (v6.10.19d D: the L6 header chip was removed; the KPI is the single Risk-Adj R:R surface). `null` when there is no real risk-adjusted R:R (or the factor is trivial). Consumers no longer need to recompute the factor from `top_setup.rr_value` ÷ `safety_flags.rr_value`.
- **`risk_adj_rr_explanation` (v6.10.14 / RR-008).** First-class R:R discount sentence — the identical string the Safety-Flags KPI tooltip renders (v6.10.19d D: header chip removed), so consumers don't recompute the factor. `null` when there is no real risk-adjusted R:R (HOLD with 0, or only one of the two values present).
- Missing `entry_danger` reads 50 (MODERATE) exactly like the panel.
- `top_setup_empty_text` carries the section-meta caption the panel renders when no qualifying setup exists (`"no qualifying setup yet"`); it is `null` whenever a setup renders.
- The four `strategy` fields render the screen's `"—"` placeholder when the advisory guidance is absent.
- **Gauge needle (v6.10.7 / R1, v6.10.12 GAUGE-001, v6.10.15 FIX-3, v6.10.17 decoupling, v6.10.19c D2, v6.10.19e).** `gauge.net_bias_pct` / `bias_direction` are the raw long−short math. The panel's needle renders **neutral** (amber, no arc) only when `verdict.top === "HOLD"` — a directional lean gated by `STAND_ASIDE` (e.g. LONG 62/2/36) draws its REAL `+60%` needle next to the gate badge, because the directional read is decoupled from the execution gate (v6.10.17). The center-bottom dial label mirrors the needle: the verdict-consistent net % (`gauge.net_bias_display`), color-coded by sign — green `+N%` LONG, red `-N%` SHORT, amber `0%` under a HOLD verdict (the raw split stays in `gauge.long_pct` / `hold_pct` / `short_pct` for data consumers only; **no LONG/HOLD/SHORT percentage split row is rendered** under the dial).
- **Risk-Adjusted Reward-to-Risk KPI (v6.10.7 / R2, v6.10.12 RR-001, v6.10.19d D, v6.10.19e).** The Safety-Flags KPI row is the single **`Risk-Adjusted Reward-to-Risk`** surface (it reads `DecisionContext.expected_reward_risk_ratio` — geometric × (1 − overall_risk/100)) — distinct from the L4 geometric `R:R` the setup cards show. The L6 **header chip was removed** in v6.10.19d (the header keeps Confidence + Stance). `—` only when `verdict.top === "HOLD"` AND the risk-adjusted R:R is `0`; a HOLD verdict with a non-zero R:R still surfaces the single decimal (`X.Y`). When a value renders, the KPI's tooltip explains the discount (`geometric R:R × risk factor = value`).
- **Final verdict (v6.10.7 / R6, v6.10.15 FIX-4, v6.10.17, v6.17).** `final_verdict` is the verdict — one of four graded sentences, built by the shared `buildVerdictSentence` (panel and export can never disagree). v6.17 sentence-cases the readiness gate (`Watch`, `Ready`, `Forming`, `Stand aside`) and spells the danger level (`Entry Danger Moderate`):
  - `HOLD — no directional call (readiness: …)` — only under a genuine HOLD top.
  - `${TOP} lean ${pct}% — Stand Aside (readiness: Stand Aside, Entry Danger ${LEVEL}).` — directional lean gated by the execution gate.
  - `${TOP} ${pct}% — Ready (readiness: Ready).` — execution-cleared.
  - `${TOP} lean ${pct}% — awaiting confirmation (readiness: ${Watch|Forming}).`
  The advisory `final_recommendation` is carried separately as `final_verdict_guidance` (`Environment guidance: …`) whenever present.
- **Strategy block (v6.10.16 FIX-O5, v6.10.17, v6.10.19d D, v6.10.28).** `strategy.entry` / `exit` / `protection` / `target` render `"—"` ONLY under a genuine HOLD top — a directional lean gated by STAND ASIDE carries its REAL guidance values (the lean is directional; only the gate differs). The section is titled "Environment Guidance" on screen (renamed from "Environment Playbook" in v6.10.19d; the export key `strategy` is unchanged). **v6.10.28:** the four cards are labelled `Trigger Tactic` / `Exit Condition` / `Protection` / `Target` (tactics, distinct from the SETUP section's price-level labels — its `TARGET` zone reads `Take-Profit` on screen), the HOLD reference caption was erased, and the `strategy.hold_caption` export field was removed — the export mirrors the panel 1:1. The Final Verdict quote's left accent line is verdict-colored (green LONG, red SHORT, amber HOLD), and the L6 header `Confidence` chip was removed (the Safety Flags KPI row owns confidence; the Recommendation export's `header.chips` drops it too).
- **Gauge lean-floor flag (v6.10.19 P6).** `gauge.lean_floor_applied` is `true` when the graded-lean floors adjusted the split — the operator-facing LEAN annotation marks a structurally boosted low-confidence read, never a deep consensus. (**v7.1:** the flag was removed from the Opportunity `directional_bars` block — the L4 bars are bracket-derived and never floor-boosted; the LEAN marker is the Recommendation gauge's surface only.)
- **Net R:R (v6.10.19 P5).** The L4 `long_expected_rr_internal` / `short_expected_rr_internal` (and every card reading them) carry the NET R:R — the gross geometric ratio minus estimated fees/slippage/funding (`NetCostModel`, 6/5/0 bps defaults). `rr_internal.gross_rr_value` (from `long_gross_rr_internal` / `short_gross_rr_internal`) preserves the gross for offline analysis; the Risk-Adj R:R explanation sentence reads "Risk-adjusted: net R:R … × risk factor … = …".
- **BelowFloor (v6.10.19 T3, v6.10.23).** A sub-1.0 aggregated reference bracket exports `badge_text: "BELOW ACTIONABLE FLOOR"` (v6.10.23 wording) with `below_floor: true` and the levels intact — it is a reference card, never a trade.
- **Verdict-aware guidance (v6.10.19 T2/T5, v6.17).** `final_verdict_guidance` can never contradict the verdict headline. Under `verdict.top === "HOLD"` it carries no "Entry:/Stop:" clauses and leads with "…no actionable directional edge (HOLD)" — the server omits the entry/stop suffix under Neutral/Avoid guidance too. Under a **directional** verdict (v6.17) it leads with the verdict's OWN read — `Bullish` / `Bearish market bias with {verdict probability}% confidence, …` — using the same `rank.top_prob` the headline renders (a stale "Neutral — no directional edge" advisory sentence is rewritten, never passed through verbatim); the advisory's environment tail survives, and "Entry:/Stop:" clauses are stripped under every verdict. The Why bullets (v6.17) use the same polished prose vocabulary — `Layer 2 Tradability Dimension` / `Layer 3 Quality Score` / `Layer 4 Opportunity Score`, `Entry Danger`, sentence-cased readiness (`Watch` / `Ready`) and clean colons instead of `=`/`—` operators.
- **Evaluated-setup scores (v6.10.19 T1, wire-first v6.14).** `evaluated_setups[].score_display` and `trade_setups[].score_display` scale the raw wire `score` by the precondition ratio (0 when inactive) — the operator sees readiness at a glance; `score` stays raw. **v6.14:** the scaled value is now emitted by the backend as `OpportunityProfile.display_score` and the export is **wire-first** — `score_display = display_score` when the field is present, falling back to the local `displayScore` rule only on legacy payloads, so screen, export, and wire can never disagree.
- **Signal-lean hero (v6.10.19c C).** The Analysis hero counts ALL timeframe lines present (no `TF votes:` prefix; no COMPRESSION/flat exclusion) — a display choice over the raw supporting/contradicting lists. The bias engine's LEAN-tier vote definition (COMPRESSION/flat excluded, `analysis.rs`) is unchanged — the hero and the bias vote intentionally differ: the hero shows every TF that reported; the bias engine votes only on the decisive ones.
- **Analysis rationale traceability (v6.10.18 I-9).** The analysis payload carries `representative_bbwp` / `representative_adx` — the L3 regime-input values (first-TF-wins representative map) the rationale quotes, so a quant can trace the L3 regime derivation from the data itself. Intra-candle shadow drift means these can differ from any single per-TF row in the same export; the payload now makes the source explicit.
- **Opportunity directional bars (v6.10.18 I-4, superseded v7.1).** `directional_bars` mirror the L6 verdict split whenever a decision context is present — the L4 and L6 panels can never show two different conviction numbers for the same market. **v7.1 (L4-only):** this was reversed — the L4 bars (panel + export) read **only** the opportunity matrix's bracket conviction; the L6 verdict split is the Recommendation gauge's story and the two panels intentionally differ.
- **Asset-ranking rows (v6.10.16 FIX-O1).** `asset_rankings.rows[].signal` is `BUY`/`SELL` only when that instance holds an Actionable + READY setup — the same gate the hero's `actionable_count` / `valid_setups` use — otherwise `WAIT` (a directional verdict with WATCH/STAND_ASIDE readiness never renders BUY beside "no READY trade yet"). `rows[].rr` comes from the shared `resolveActiveRr` chain (N/A when the L4/L6 panels mark N/A; never the legacy scalar).
- `price_levels.hold_placeholder` (v6.10.7 / R5) describes the ACTUAL card state: the Top Setup card carries the aggregated bracket on the net-bias side (R:R `N/A` when geometry is inverted) — not the close-pinned sentinel the legacy copy claimed.
- **Top setup under No Clear (v6.10.17 A3, superseded v6.10.19c).** `top_setup` is now ALWAYS published when the opportunity matrix exists: a state with no qualifying profile yields the **aggregated bracket on the bias side** (`opportunity_type: "Aggregated Bracket"`, `viability: "NoClear"`, real ENTRY/TARGET/SL + R:R, marked informational) so the operator always has TPs/SLs to work with. The explanatory no-clear card that coexisted with it (it explained WHY no profile qualifies) was **removed in v6.10.19c** — the informational reference row is the only surface.

### v6.10.19b — verdict-consistent Recommendation + all-opportunities panel

- **Verdict-consistent `top_setup` (B1).** The Recommendation's `top_setup` is now the **verdict-consistent headline**: under a directional verdict it is the best qualifying profile ON that side, else the verdict-side aggregated reference bracket (`viability: "NoClear"`, informational); under a HOLD verdict it is the NEUTRAL no-bracket headline (`entry_zone`/`target_zone`/`invalidation` all `null`, `rr_reason: "no directional bias"`). A counter-bias qualifying setup is never the headline: it rides in **`top_setup.alternate_qualifying_setups`** (`[{opportunity_type, side, score, preconditions_met, preconditions_total}]`) and always appears on the Opportunities panel.
- **Verdict lean from valid setups (G3, v6.10.19c D3).** When `verdict.top` would be HOLD but a qualifying setup with a resolvable side exists, the verdict leans to that side with its REAL probability (e.g. `LONG lean 12% — STAND ASIDE`) — the badge/headline/gauge stay consistent, the readiness gate is unchanged. Under HOLD with a qualifying setup that has NO resolvable side (NEUTRAL, e.g. DirectionalNeutral Mean Reversion), the verdict stays HOLD but the setup still headlines the container with its zones and bracket R:R — never hidden behind a placeholder.
- **Unified SETUP block (B3 + v6.10.19c D3/D4).** `top_setup` is the single price-levels source: it also carries `horizon` and `hold_placeholder`. The `price_levels` block is an ALIAS of the same summary (identical zones/side — the two can never disagree); the panel renders ONE setup section at the top and the separate "Price Levels" section is removed. **v6.10.19c:** the container has two cases — (a) no qualifying setup → `top_setup.opportunity_type: "No Active Setup"` with null zones, empty `badge_text` and `hold_placeholder: "No active setup."` (v6.10.19d D: the "fields are placeholders" caveat was erased); (b) qualifying setup (any side incl. NEUTRAL) → the setup headlines the container with its real zones/R:R. The explanatory no-clear card was removed. The **Risk-Adj R:R** (`safety_flags` KPI only — the header chip was removed in v6.10.19d) is bracket-aware: backend `expected_reward_risk_ratio` when > 0, else `top_setup.rr_value × (1 − overall_risk/100)` (N/A only without a bracket or below the 0.10 floor). Section order (payload keys): gauge → top_setup → safety_flags → price_levels → strategy → final_verdict → final_verdict_guidance → why_note → why (Why at the bottom). **v6.14:** `top_setup` additionally carries `score_display` — the card's precondition-scaled score (backend `display_score` when present, else the raw `score`), keeping `score` itself raw for data-science consumers.
- **All opportunities always visible (C1, v6.10.19c; per-folder references v6.10.23; ranked order v7.1).** The Opportunities Trade Setups area renders three always-present folders in **RANKED order** — the folder with the most content first (same relevance ordering as the conviction bars; e.g. a lone BEARISH setup puts the BEARISH folder first), ties broken by the top setup's score, the fixed RANGE → BULL → BEARISH order applying only to empty ties (`trade_setups[].section` = `'NEUTRAL'|'BULL'|'BEAR'`, mirrored in `trade_setup_sections`) — top-ranked first within each. **v6.10.23:** reference brackets are fully integrated into their folders — a folder hosting zero qualifying setups mounts its own aggregated bracket (`opportunity_type: "Aggregated Bracket"`, `badge_text: "INFORMATIONAL"`, in zero-qualifying states AND counter-bias-qualifying states such as a SHORT verdict with only LONG setups qualifying) and the RANGE folder additionally mounts the backend neutral range frame (`opportunity_type: "Neutral Reference Bracket"`) — each only when it carries real zones. Sub-1.0 or geometry-broken references export `badge_text: "BELOW ACTIONABLE FLOOR"` with `below_floor: true` — reference cards, never trades. **Parity invariant:** whatever the Recommendation's `top_setup` shows is always present in the Opportunities export — provable zone-for-zone (test-locked).
- **Truthful geometry (A1/A2).** `compute_side_rr_v2`'s SlAtEntry epsilon is `1e-6 × price` (the old 0.01% rejected real 1.5×ATR stops in low-ATR states as "zero risk"); the UI fallbacks mirror the backend guards (SlAtEntry + close-aware TargetOnWrongSide) and **respect the server geometry flag** — a GeometryInverted bracket never leaks a locally-recomputed R:R.

### 3.8 Charts Sub-Tabs — `source_tab: "positions" | "orders" | "history" | "plan"`

These use the legacy envelope (`exported_at`, `symbol`, `mark_price`, `tf_secs`) plus `account` / `counts` blocks and per-tab arrays (`position`+`slots`+`brackets`, `open_orders`, `history`, `targets`+`stop`+`plan_visible`). Shapes unchanged since 6.x — see the previous revision of this document for the full examples.

Notes (v7.0-verify):
- Orders/brackets/counts read `AppStore.openOrders` — the same array the console table renders (`paper.openOrders` is legacy and never written).
- `position.liq_price` uses the shared `calcLiqPrice` from `lib/telemetry.ts` — identical to the console cell, including the `SHORT` + leverage-1 `2× entry` case.
- `history[].symbol` is the raw wire string and `null` when absent (the screen renders `"—"`; there is no activeTab fallback).
- `buildPlanTabExport(app, planOverride?)` accepts the console's currently-edited plan rows so the exported targets/stop mirror what the user sees in the inputs (edits are local state, not written back to `app.activePlan`).
- The console's `fmtTs` renders 24-hour time (`hour12: false`), byte-identical to the `*_display` strings in the payload.

---

## 4. Migration Notes

The legacy `buildMetricsExportJson` / `buildPanelExportJson` functions in `lib/metricsExport.ts` continue to produce the "kitchen-sink" payload (every matrix in one JSON). External consumers depending on that shape can keep using those functions. The new panels route to the per-tab builders via `buildXxxTabExport(args)` directly.

The v7.0 payload shapes are **not** backwards compatible with the legacy kitchen-sink:
- The **MTF** export added a top-level `filter_state` block + per-row `visible` flags (v7.0-verify); the single-TF **Metrics** export later gained the same top-level `filter_state` block (payload rows stay the unfiltered superset) so each tab's on-screen row set is reconstructible.
- A single `meta.current_price` replaced the multiple price mirrors.
- Screen sentences are exposed as `*_display` fields alongside raw numerics.
- Ladder clusters are canonicalized to top-4 by magnet strength everywhere (strip, Levels facet, export).
- Empty/null states mirror the screen placeholders exactly (`"—"`, `null`, `AWAITING` / `NO DATA` tokens) — the export never fabricates values the screen does not show.

---

## 5. Test Coverage

| Builder | Test file | Tests |
|---|---|---|
| `shared.ts` | `shared.test.ts` | 10 |
| `chartsTab.ts` | `chartsTab.test.ts` | 22 |
| `riskTab.ts` | `riskTab.test.ts` | 6 |
| `opportunityTab.ts` | `opportunityTab.test.ts` | 11 |
| `alignmentTab.ts` | `alignmentTab.test.ts` | 9 |
| `analysisTab.ts` | `analysisTab.test.ts` | 9 |
| `recommendationTab.ts` | `recommendationTab.test.ts` | 9 |
| `metricsTab.ts` | `metricsTab.test.ts` | 14 |
| `mtfTab.ts` | `mtfTab.test.ts` | 4 |
| `BottomConsole.test.ts` | (component) | 5 |
| **Export-consistency harness** | `tests/exportConsistency/exportConsistency.test.ts` (renders each MME panel, presses EXPORT DATA, cross-checks DOM vs JSON both directions) | 12 |
| **Total** | | **111** |

The harness (`ui/src/tests/exportConsistency/`) is the enforcement mechanism for the "export == screen" contract: it renders every Market Monitoring panel with a rich synthetic store state, captures the clipboard JSON, and asserts both directions — every displayed number/string/word is present in the JSON, and every exported display string maps back to the screen.

---

## 6. Engine dashboard exports (v7.3)

The four engine dashboards (DIE / TAE / PME / PAE) extend the same "export == screen" principle with a shared envelope defined in `ui/src/lib/engineExport.ts` and specified in [07-07 Engine Dashboard Vocabulary §4](07-07-engine-dashboard-vocabulary.md#4-export-data-contract-v73):

- DIE — every panel exports its fetched state (platform config, quality report, pipelines, distribution, clock runtime, data quality, exchange status).
- TAE / PME — one context-aware button in the dashboard header exports the current tab's visible state (mode-aware payloads: radar vs lab vs cockpit; readiness vs accounting).
- PAE — the shell exports Overview / Trades / Strategy / Risk / Performance / Methodology; the Backtesting and History tabs export their own local state (form + result; run list + selected run).

The envelope fields (`schema`, `engine`, `tab`, `mode`, `exported_at`, `data`) are not part of the MME per-tab contracts above — they are documented in 07-07 and enforced by `ui/src/lib/engineExport.test.ts`.

