# Chart Patterns

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Multi-pivot shapes classified by the geometry of their highs and lows - triangles, wedges and channels.


## Formula

```
pivots detected over the window; regression fitted separately to pivot highs and pivot lows; shape classified from the convergence, divergence or parallelism of those fits
```


## Range

Confidence 0 to 1


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| PatternForming | Direction | Chart pattern geometry completes |

## Divergence

Not supported.

## Levels and other outputs

- Pattern identity, bullish/bearish classification, confidence
- fitted upper and lower slopes


## Notes

- A pure function with no retained state: the same pivot geometry yields the same pattern every bar, with no record of when it started.
