# Z-Score

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

How many standard deviations price sits from its own recent mean.


## Formula

```
Z = (Close - SMA(Close, n)) / sd(Close, n)
```


## Range

Unbounded signed


## Signals

| Kind | Fires when |
|---|---|
| |Threshold |


## Divergence

ZeroLineCross


## Levels and other outputs

- Not supported.


## Notes

- Z-score and its deviation band
