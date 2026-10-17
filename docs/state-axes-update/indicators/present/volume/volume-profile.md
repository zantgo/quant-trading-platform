# Volume Profile

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Distribution of volume across price, identifying where trading actually transacted rather than where price closed.


## Formula

```
bins over window, volume distributed by each candle's range-overlap proportion; POC = bin of greatest volume; value area expands from POC until the configured fraction is covered; HVN = bin >= 1.5x mean; LVN = bin <= 0.5x mean
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Direction | Close breaks POC or a value-area edge |
| LevelTest | Structure | Close approaches POC or a value-area edge |
| TrendFlip | Direction | Close crosses to the other side of POC |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
