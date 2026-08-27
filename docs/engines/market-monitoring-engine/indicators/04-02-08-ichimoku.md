# ☁️ Ichimoku Cloud (Ichimoku Kinko Hyo)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
Ichimoku is a complete trend-following and dynamic support/resistance system that uses five concurrent lines derived from OHLC data. It is treated as **one unified system** rather than five independent indicators — the scoring engine and frontend consume it as a single registry entry (`ichimoku`) with the 5 lines stored in its `values` sub-map.

This V1 implementation ships the five lines with full normalization, signals, and chart rendering. The shaded cloud fill between Senkou A/B is **deferred to a follow-up phase** (the indicator is fully functional without it).

---

## 2. Five Lines

| Line | Period | Formula | Rendering |
|------|--------|---------|-----------|
| **Tenkan-sen** (Conversion) | 9 | (highest-high + lowest-low) / 2 over last 9 candles | Solid magenta |
| **Kijun-sen** (Base) | 26 | (highest-high + lowest-low) / 2 over last 26 candles | Solid blue |
| **Senkou Span A** (Leading A) | — | (Tenkan + Kijun) / 2, plotted **+26 bars forward** | Dashed green |
| **Senkou Span B** (Leading B) | 52 | (highest-high + lowest-low) / 2 over last 52 candles, plotted **+26 bars forward** | Dashed red |
| **Chikou Span** (Lagging) | — | Current close, plotted **−26 bars back** | Dotted purple |

The cloud is defined by the area between Senkou A and Senkou B. A=green, B=red → bullish. A=red, B=green → bearish.

---

## 3. Normalization

`normalize_ichimoku` computes a directional score from price position relative to the **current-applicable** cloud (the projections made `displacement` bars ago, stored as `senkou_a_current` / `senkou_b_current`).

| Condition | Normalized value |
|-----------|-----------------|
| Price above cloud, all factors bullish | +1.0 |
| Price above cloud, mixed factors | +0.6 .. +1.0 |
| Price inside cloud | ±0.2 (slight TK lean) |
| Price below cloud, mixed factors | −0.6 .. −1.0 |
| Price below cloud, all factors bearish | −1.0 |

Conviction factors: Tenkan-vs-Kijun alignment (fast line leading), current cloud colour, future cloud colour. All three agreeing produces the strongest signal.

---

## 4. Signals

| Signal | Kind | Trigger |
|--------|------|---------|
| `ICHIMOKU_TK_CROSS_BULLISH/BEARISH` | Crossover | Tenkan crosses Kijun (transition bar only, via `PreviousBarState`) |
| `ICHIMOKU_CLOUD_BREAKOUT_UP/DOWN` | Breakout | Price exits the cloud (crosses above/below) |
| `ICHIMOKU_PRICE_ENTERING_CLOUD` | LevelTest (Neutral) | Price moves from outside to inside the cloud |
| `ICHIMOKU_FUTURE_CLOUD_TWIST_BULLISH/BEARISH` | TrendFlip | Forward Senkou A crosses Senkou B (future cloud colour flips) |
| `ICHIMOKU_CHIKOU_CONFIRMS_BULL/BEAR` | LevelTest | Chikou span clears the cloud in the trend direction |

---

## 5. Scoring
* `ichimoku` is a `directional` registry indicator → contributes to the registry-driven confluence score.
* The full DTO with all 5 lines + `cloud_thickness` + `future_bias` is exposed in the `values` sub-map; Ichimoku is treated as a complete trend system.

---

## 6. Configuration

```json
{
  "indicators": {
    "ichimoku_tenkan": 9,
    "ichimoku_kijun": 26,
    "ichimoku_senkou_b": 52,
    "ichimoku_displacement": 26
  }
}
```

---

## 7. Frontend Rendering
* 5 `LineSeries` overlays on the main price chart.
* Senkou A/B use time-shifted data (the backend stores forward-projected values on the current candle; the frontend applies the +displacement offset by shifting the time axis).
* Chikou uses a backwards time shift (−displacement).
* Toggled via the `ICHIMOKU` chart pill; Chikou has a separate `CHIKOU` pill (defaults off to reduce clutter).
* Shaded cloud fill deferred to Phase 4b.

---

## 8. Persistence
All 5 lines + `senkou_a_current`/`senkou_b_current` + `cloud_thickness` + `future_bias` flow through the JSON blob — no schema migration.
