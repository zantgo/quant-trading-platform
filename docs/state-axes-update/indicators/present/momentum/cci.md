# Commodity Channel Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Deviation of typical price from its mean, scaled by mean absolute deviation so it is comparable across instruments.


## Formula

```
TP = (High + Low + Close) / 3; CCI = (TP - SMA(TP, n)) / (0.015 * meanAbsoluteDeviation(TP, n))
```


## Range

Typically -200 to +200


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Contrarian | CCI crosses +/-100 |
| ZeroLineCross | Direction | CCI crosses zero |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Standard threshold at +/-100
