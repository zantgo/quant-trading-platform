# Supertrend (10, 3.0)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

Supertrend is a trend-following overlay that plots a line above (bearish trend) or below (bullish trend) price based on the Average True Range. When price closes on the opposite side of the line, the trend is considered flipped and the line jumps to the other side. It is widely used for:

- **Trend identification:** price above the line = bullish; below = bearish.
- **Trailing stop:** the line acts as a dynamic stop-loss level that moves with the trend.
- **Entry signals:** a close crossing the Supertrend line generates a Crossover signal; a line-side flip (direction change) generates a TrendFlip signal.

## 2. Mathematical Formula

```
Upper Band = (High + Low) / 2 + multiplier × ATR
Lower Band = (High + Low) / 2 - multiplier × ATR

Supertrend = Upper Band  (if previous close ≤ previous Upper Band)
           = Lower Band  (if previous close ≥ previous Lower Band)
```

The `flipped` boolean is set to `true` on the bar where the line side changes.

## 3. Normalization

The normalized score in [-1, 1] is computed from the distance between price and the Supertrend line, scaled by the line level:

```
dist = |price - line| / |line|
mag = 0.6 + 0.4 × tanh(dist × 12)
norm = direction × mag   [direction = +1 bullish, -1 bearish]
```

The magnitude floor is ±0.6 (Supertrend never produces a near-zero reading). Label: `SUPERTREND_BULLISH` or `SUPERTREND_BEARISH`. The `values` sub-map carries `line` (raw Supertrend level) and `direction` (±1).

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| TrendFlip | SUPERTREND_FLIP | Supertrend direction changed this bar (`flipped == true`). Structured push from engine. | Bullish (flip to up) / Bearish (flip to down) |
| Crossover | SUPERTREND_PRICE_CROSS_BULLISH | Price crossed from below the Supertrend line to above. Detected via previous-bar price and line comparison (transition-only). | Bullish |
| Crossover | SUPERTREND_PRICE_CROSS_BEARISH | Price crossed from above the Supertrend line to below. | Bearish |
| LevelTest | SUPERTREND_RESISTANCE_TEST | Price tests the Supertrend line from below (acting as dynamic resistance) without crossing above. Confirms trend resistance. | Bearish |
| LevelTest | SUPERTREND_SUPPORT_TEST | Price tests the Supertrend line from above (acting as dynamic support) without crossing below. Confirms trend support. | Bullish |

> **Renamed in v2.1 (consolidation audit fix).** A previous version of this table classified the two proximity signals as `BandTouch` (`SUPERTREND_LINE_TOUCH_BULLISH` / `SUPERTREND_LINE_TOUCH_BEARISH`). These were reclassified to `LevelTest` because Supertrend does not emit `SignalKind::BandTouch` in the runtime — only `TrendFlip`, `Crossover`, and `LevelTest` are produced by `crates/market-analyzer/src/indicators/normalized/all.rs`. The label patterns `SUPERTREND_RESISTANCE_TEST` / `SUPERTREND_SUPPORT_TEST` reflect the actual runtime emission.

## 5. Scoring

`supertrend` is a `directional: true` indicator in the Trend group. It contributes `weight × normalized` to the confluence score.

## 6. Configuration

```json
{
  "indicators": {
    "supertrend_period": 10,
    "supertrend_multiplier": 3.0
  }
}
```
