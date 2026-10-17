# ADX / DMI

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Trend strength regardless of direction, plus the directional movement that gives it sign.


## Formula

```
TR = max(High-Low, abs(High-Close[-1]), abs(Low-Close[-1])); +DM = max(High-High[-1], 0); -DM = max(Low[-1]-Low, 0); +DI = 100 * RMA(+DM)/RMA(TR); -DI = 100 * RMA(-DM)/RMA(TR); DX = 100 * abs(+DI - -DI) / (+DI + -DI); ADX = RMA(DX, n)
```


## Range

ADX [0,100]; DI [0,100]


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| TrendFlip | Direction | +DI crosses -DI |
| Threshold | Neutral | ADX crosses a strength band boundary |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- +DI, -DI, DX, ADX
- ADX slope over the lookback window
- **ADX peak** - running maximum since the last DI crossover
