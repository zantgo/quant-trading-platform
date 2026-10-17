# Average True Range

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Mean of the true range, the standard measure of how far price travels per bar regardless of direction.


## Formula

```
TR = max(High - Low, abs(High - Close[-1]), abs(Low - Close[-1])); ATR = WilderRMA(TR, n)
```


## Range

Price-relative, always positive


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Neutral | ATR crosses a magnitude band |
| CompressionRelease | Neutral | ATR expansion follows contraction |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- ATR and its rate of change
