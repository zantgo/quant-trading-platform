# SMC Fair Value Gap

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A three-candle imbalance where the wicks of the outer candles do not overlap, marking an inefficiency in the range.


## Formula

```
bullish: Low[n] > High[n-2], zone = [High[n-2], Low[n]]; bearish: High[n] < Low[n-2], zone = [High[n], Low[n-2]]
```


## Range

Price zone


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Structure | Price enters a fair value gap |

## Divergence

Not supported.

## Levels and other outputs

- Gap zone top and bottom, bullish or bearish


## Notes

- Single-slot by construction: only one gap is tracked at a time, with no gap list and no fill counter.
