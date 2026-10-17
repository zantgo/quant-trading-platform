# Williams %R

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

The mirror of the stochastic, expressed on a negative scale so it reads as an oversold oscillator.%R = (HighestHigh(n) - Close) / (HighestHigh(n) - LowestLow(n)) * -100


## Formula

```
-100 to 0
```


## Range

|Threshold


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Contrarian | %R crosses -20 or -80 |
| CenterlineCross | Direction | %R crosses its centreline at -50 |

## Divergence

Not supported.

## Levels and other outputs

- -100 to 0 scale


## Notes

- Identical information to stochastic, differently signed. Do not treat the two as independent confluence.
