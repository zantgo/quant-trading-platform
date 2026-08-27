# Anchored VWAP (Multi-Session)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Group:** Trend

## 1. Introduction — Trading Function

Anchored VWAP extends the standard daily VWAP by computing the volume-weighted average price from multiple fixed and dynamic start points. Daily VWAP resets each UTC day. Weekly VWAP resets each 7-day boundary. Monthly VWAP resets each 30-day boundary. Swing-anchored VWAP resets automatically when the algorithm detects a new swing pivot (high or low), providing an institutional reference level that follows market structure. The closest anchor to the current price is selected as the "active" anchor for directional scoring. Traders use anchored VWAPs as multi-horizon fair-value references — price above all anchors signals strong bullish conviction; price below all anchors signals strong bearish conviction.

## 2. Mathematical Formula

Each anchor accumulates independently:

```
typical_price = (High + Low + Close) / 3
sum_TP_vol += typical_price × volume
sum_vol += volume
AVWAP = sum_TP_vol / sum_vol
```

- **Daily anchor:** Reset when `day_index = candle_close_sec / 86400` changes (UTC day).
- **Weekly anchor:** Reset when `week_index = day_index / 7` changes.
- **Monthly anchor:** Reset when `month_index = day_index / 30` changes.
- **Swing anchor:** Reset when `reset_swing()` is called by the engine on detection of a new swing pivot.

## 3. Normalization

The active anchor is the one closest to the current price (minimum distance). Directional score uses the ratio `price / active_avwap`:

| Condition | Normalized | Label |
|-----------|-----------|-------|
| Ratio > 1.01 (price > 1% above anchor) | −0.7 | `AVWAP_PREMIUM_ZONE` |
| Ratio < 0.99 (price > 1% below anchor) | +0.7 | `AVWAP_DISCOUNT_ZONE` |
| Ratio > 1.001 (slightly above) | −0.3 | `AVWAP_ABOVE_ACTIVE` |
| Ratio < 0.999 (slightly below) | +0.3 | `AVWAP_BELOW_ACTIVE` |
| Within 0.1% of anchor | 0.0 | `AVWAP_AT_ACTIVE` |

The `values` sub-map carries: `daily`, `weekly`, `monthly`, `swing` (each anchor's VWAP value).

## 4. Signals

| SignalKind | Label Pattern | Trigger | Direction |
|-----------|--------------|---------|-----------|
| Crossover | AVWAP_ABOVE_ACTIVE | Price crosses above the active anchor | Bearish |
| Crossover | AVWAP_BELOW_ACTIVE | Price crosses below the active anchor | Bullish |
| LevelTest | AVWAP_DISCOUNT_ZONE | Price at significant discount to active anchor | Bullish |
| LevelTest | AVWAP_PREMIUM_ZONE | Price at significant premium to active anchor | Bearish |

## 5. Scoring

`anchored_vwap` is `directional: true`. Contributes to confluence scoring. All four anchor levels are exposed in the `values` sub-map; the active anchor is the primary fair-value reference.

## 6. Configuration

AVWAP uses the same accumulation formula as daily VWAP and requires no additional config fields — the session boundaries are computed dynamically.
