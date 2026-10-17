# Hull MA

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A moving average using a weighted moving average of a differenced double-smoothed price, reducing lag while preserving smoothness.


## Formula

```
WMA(x) = sum(value[i] * i) / sum(i); HMA = WMA(2 * WMA(Close, sqrt(n)) - WMA(Close, n), sqrt(n))
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Crossover | Direction | Close crosses the Hull line |

## Divergence

Not supported.

## Levels and other outputs

- Single line


## Notes

- Smoother than an EMA at the same period and turns faster
- the standard use is slope, not level.
