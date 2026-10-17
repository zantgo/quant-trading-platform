# Export Data Payload Schema

<!-- pascal-display-strings -->

<!-- This document's JSON examples carry UI display strings (e.g.
     "Momentum", "Bullish", "Trend Continuation") which are intentionally
     PascalCase — they document the screen-facing *display* fields, not
     wire enums. Exempted from the G6 enum-casing lint via the marker. -->

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
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
    "summary_label": "SUMMARY",
    "badge": { "label": "STRONG BULLISH", "sublabel": "TRENDING", "tone": "bull" },
    "chips": [ { "label": "Score", "value": 0.62 }, { "label": "Signals", "value": 7 } ],
    "status": "live"
  }
}
```

Rules:
- Numbers are raw (no `%`, `$`, `1 :` diminutives).
- Strings mixed with numbers are split into structured fields plus `*_display` verbatim copies of the screen sentence where the screen renders one (e.g. `rr_display`, `entry_danger_display`, `current_position_label`, `badge_text`).
- `header` mirrors the LayerHeader chrome (badge label/sublabel + meta chips + status). Chip values that are numeric strings are parsed to numbers. The Alignment and Analysis tabs additionally carry `header.summary_label` (`"SUMMARY"`), the label over the panel's summary row.
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
| `duration_profiles` | v11.11: `GET /api/config` carries the 14 per-duration indicator profiles the registry runs (`{ "<secs>": IndicatorsConfig }`, `duration_profile::overlay(workspace, secs)`) — the settings editors seed each duration's draft from its row. |
| `meta.timeframe_secs` | `0` for MTF (multi-TF sentinel); the active TF's `barDurationSec` otherwise | MTF also emits `meta.timeframes` — the **ACTIVE** duration display labels in canonical order (v11.9: any subset of the 14-duration pool, `[workspace].timeframes` — default 1s→4h (12 of the 14 pool durations, excluding 12h and 1d) → `["1s","3s","5s","15s","30s","1m","3m","5m","15m","30m","1h","4h"]`, derived via `tfLabel`; renamed from the historical `timesframes` typo, 2026-08-17). |
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
          "age_bars": 2, "display_label": "CRO·2",
          "evidence": ["close above the 60 band", "volume confirming"] }
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
- Derived strings (`fibonacci.status`, `price_vs_gp_pct`, VP position label, `age_bars_display`) are computed from the **active TF's `priceText`** — the same price the screen uses. `meta.current_price` remains the freshest price across the ACTIVE slots (v11.2).

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

Mirrors `MtfView.svelte` (N × N grid with per-row agreement — one column per
ACTIVE ladder slot, v11.2). The meta block carries `timeframes` — the ACTIVE
duration display labels in canonical order (the ACTIVE set of the 14-duration pool;
e.g. N=5 → `["1s","3s","5s","15s","30s"]`; renamed from the
historical `timesframes` typo, 2026-08-17; v11.1 fixed the label set to the
10-slot ladder, v11.2 slices it to the active set) and `timeframe_secs: 0`
(multi-TF sentinel).

```json
{
  "source_tab": "mtf",
  "meta": { "timeframes": ["1s", "3s", "5s", "15s", "30s"] },
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
        { "timeframe": "1s", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "3s", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "5s",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "15s",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "30s",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "1m",  "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "3m", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "5m", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "15m", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false },
        { "timeframe": "1h", "normalized": 0.31, "normalized_display": "+0.31", "active": true, "warming": false, "gated": false }
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
          { "timeframe": "1s", "bull": 1, "bear": 0, "neutral": 0,
            "entries": [ { "display_name": "RSI (14)", "label": "OVERSOLD", "direction": "Bullish", "status": "Active", "strength": 0.5, "age_bars": 2 } ] },
          { "timeframe": "3s", "bull": 1, "bear": 0, "neutral": 0, "entries": [] },
          { "timeframe": "5s", "bull": 1, "bear": 0, "neutral": 0, "entries": [] },
          { "timeframe": "15s", "bull": 0, "bear": 1, "neutral": 0, "entries": [] },
          { "timeframe": "30s", "bull": 0, "bear": 1, "neutral": 0, "entries": [] },
          { "timeframe": "1m", "bull": 0, "bear": 1, "neutral": 0, "entries": [] },
          { "timeframe": "3m", "bull": 0, "bear": 0, "neutral": 1, "entries": [] },
          { "timeframe": "5m", "bull": 0, "bear": 0, "neutral": 1, "entries": [] },
          { "timeframe": "15m", "bull": 0, "bear": 0, "neutral": 1, "entries": [] },
          { "timeframe": "1h", "bull": 0, "bear": 0, "neutral": 1, "entries": [] }
        ],
        "totals": { "bull": 3, "bear": 3, "neutral": 4 } }
    ],
    "divergences": [
      { "key": "rsi", "display_name": "RSI (14)",
        "per_timeframe": [ { "timeframe": "1s", "sub": "RegularBull", "strength": 0.7 }, { "timeframe": "3s", "sub": null, "strength": 0 } ],
        "bull_count": 1, "bear_count": 0, "unknown_count": 0, "row_count": 1, "direction_label": "BULL" }
    ],
    "levels": [
      { "kind": "SupportResistance",
        "per_timeframe": [ { "timeframe": "1s", "chips": [ { "name": "Support", "role": "support", "count": 1, "price_text": "$63,200" } ], "bull": 1, "bear": 0, "neutral": 0, "support": 1, "resistance": 0 } ] }
    ],
    "totals": { "signal_count": 4, "divergence_count": 1,
      "global_signal_lean": { "bull": 2, "bear": 2, "neutral": 0 },
      "global_divergence_lean": "BULL" }
  },
  "timeframes": [
    {
      "label": "1s", "duration_seconds": 1, "mark_price": 63390,
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
- The `timeframes` array carries exactly **N entries** in ladder order — the
  ACTIVE durations of the 14-duration pool (`[workspace].timeframes`,
  any subset 1..=14, default 1s→4h (12 of the 14 pool durations, excluding 12h and 1d); v11.9) — one
  per ACTIVE ladder slot. Each
  entry is projected from the store's per-slot `TimeframeTelemetry` record,
  which carries the authoritative `slot` identity the entries derive from:

  ```json
  { "slot": "1s", "barDurationSec": 1, "indicators": { } }
  ```

- Per-TF indicator rows carry the **same triple** as the single-TF Metrics
  export: `raw` / `raw_display` / `state_display` (state is humanized the
  same way — `OVERBOUGHT_DISTRIBUTION` → `OVERBOUGHT DISTRIBUTION`).
- The top-level `signals_by_kind` / `divergences` / `levels` /
  `group_confluence` / `liquidity_panel` blocks aggregate across all ACTIVE TFs (v11.2)
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

Carries `timeframe_status` (v11.4): the TfStatusTable rows — one entry per
ACTIVE slot, each with `slot`, `secs`, `badge_label`, `badge_sublabel`
(optional), and `pipeline_status` (`live` / `stale` / `loading` / `error`).

```json
{
  "source_tab": "alignment",
  "meta": { },
  "header": { },
  "timeframe_status": [
    { "slot": "1s", "secs": 1, "badge_label": "NEUTRAL", "badge_sublabel": "COMPRESSION", "pipeline_status": "live" }
  ],
  "hero": {
    "mtf_overall_score": 62.0, "mtf_overall_label": "STRONG_BULL_MTF",
    "mtf_overall_label_display": "STRONG BULL",
    "timeframes_present": 5, "signal_cross_tf_count": 2, "trend_agreement_pct": 82
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
    { "timeframe": "1S", "trend_score": 0.70, "trend_score_display": "0.70",
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
  "interpretation": "Multi-timeframe alignment shows <strong>strong directional consensus</strong> (82% agreement across 5/5 timeframes). The composite score of +62 is classified as <strong>STRONG BULL</strong>. 2 cross-timeframe signal votes reinforce the current bias.",
  "composition_note": "Composition weights: Trend (fifty percent), Momentum (thirty percent), Volatility (ten percent), and Volume (ten percent).",
  "consensus_conflict_banner": ""
}
```

Notes:
- **Consensus hero (v7.0.1 B).** The alignment header carries no meta chips (the TFs chip was erased in v11.12.3; Agreement and Score were removed earlier) — the badge alone. The header-container hero row is now two side-by-side circular dial cards — an **AGREEMENT dial** (percentage + tier-colored verdict header + grey sub-label) and a **SCORE dial** (composite blend: ring filled `|score|%`, sign-colored, signed integer centered, prettified `mtf_overall_label` + tone explanation). The CONSENSUS 2×2 axis grid is erased from the screen (the four axis values still surface in the `score_calculation.weights` chips); the `consensus.axes` field stays in the JSON. The "Polarization" term is retired (the `consensus.polarization` field is renamed `consensus.axes`); the `label_display` mirrors the agreement dial verdict header verbatim (`"Strong Consensus"`), while the sub-label (`"Timeframes are aligned."`) stays DOM-only. The `consensus_conflict_banner` renders as a full-width strip under the dial row.
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

### 3.4 Analysis Tab — `source_tab: "analysis"`

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
      { "key": "rsi", "period": 14, "display_name": "RSI 14", "timeframe": "1S",
        "score": 62, "score_display": "+62", "regime": "TRENDING",
        "signals_count": 3, "signals_count_display": "3",
        "raw": "1S (bullish): rsi_14 score +62, TRENDING regime, 3 signals" }
    ],
    "contradicting": [ ],
    "list": [
      { "key": "rsi", "period": 14, "display_name": "RSI 14", "timeframe": "1S",
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
    { "name": "1S", "active": true, "trend": 0.45, "trend_display": "+0.45",
      "momentum": 0.3, "momentum_display": "+0.30",
      "overall": 1.0, "overall_display": "+1.0", "regime": "TRENDING" },
    { "name": "1H", "active": true, "trend": -0.1, "trend_display": "-0.10",
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
    "timeframes_present": 10,
    "signal_cross_tf_count": 3
  }
}
```

Notes:
- `signals.list` is the exact merged, timeframe-sorted list the screen grid squares render (bucket-annotated).
- **L3 header chip (v6.10.13, M-3).** The L3 header chip is labelled **`State Confidence`** (it reads `state_confidence`), distinct from the L6 risk-discounted `Confidence` chip.
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
- **`evidence` on the per-indicator `signals[]` rows.** Each entry carries the `evidence` array verbatim from the wire's `IndicatorSignal.evidence` — the reasons that fired, as a list of strings. It is the same array the signal chips' tooltip renders.
- **Trend Stability Sharpe removed (v6.14).** The v6.11 `qualitative_assessment.trend_stability_sharpe` pair was **removed** with the L1→L3 traceability-evidence stamp (the Trend card badge and the `AnalysisMatrix` field are gone). The L1 `price_trend_sharpe` indicator remains the sole Sharpe family member (Metrics tab, per-TF — see [04-02-52](../engines/market-monitoring-engine/indicators/04-02-52-price-trend-sharpe.md)).
- **Representative traceability (v6.10.21).** `representative_bbwp` / `representative_adx` now come from the AnalysisMatrix's own pinned `representative_bbwp`/`representative_adx` fields (the exact inputs the rationale quotes — the matrix mirror is per-slot last-writer-wins, so the exporting slot's indicator map can differ); the micro-map fallback applies only to older frames that lack the pins.
- **KEY METRICS parity (2026-08-17, audit C2).** `key_metrics` mirrors the panel's KEY METRICS row — Overall Score (`alignment.mtf_overall_score`), Timeframe Agreement (`trend_agreement_pct`), Timeframes present (`alignment.timeframes_present`, falling back to `analysis.timeframes_considered`), Total Signals (`signal_cross_tf_count`). Any field is `null` when the alignment is absent.
- The Interpretation's opportunity sentence follows the L3 `opportunity_analysis` chain, which is synced with the L4 §4 tree (v6.10.8) — the prose can no longer claim "Favors trend continuation" under an L4 NO CLEAR SETUP verdict.

### 3.5 Charts Sub-Tabs — `source_tab: "positions" | "orders" | "history" | "plan"`

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

