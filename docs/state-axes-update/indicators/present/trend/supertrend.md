# Supertrend

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A volatility-adjusted trailing level that flips sides when price closes through it, giving a continuous directional state.


## Formula

```
mid = (High + Low) / 2; upper = mid + mult * ATR; lower = mid - mult * ATR; state flips when Close crosses the active band
```


## Range

|Band level is a price|


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| TrendFlip | Direction | Close crosses the active band |
| Crossover | Direction | Close crosses the supertrend line |
| LevelTest | Direction | Close approaches the supertrend band |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
