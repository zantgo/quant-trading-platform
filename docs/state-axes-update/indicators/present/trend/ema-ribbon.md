# EMA Ribbon

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Distance of price from each moving average in a stack, expressed as a fraction of price so the stack scales across instruments.


## Formula

```
EMA(n) = 2/(n+1) * Close + (1 - 2/(n+1)) * EMA(n-1); role_dist[role] = (Close - EMA[role]) / Close
```


## Range

role_dist in [-1, +1] per role


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| StackChange | Direction | EMA role ordering changes |
| Crossover | Direction | Price crosses an EMA role level |

## Divergence

Not supported.

## Levels and other outputs

- Four EMA levels (fast / medium / slow / long)
- role distance per level


## Notes

- The normalised score is a distance from price, not a slope. A flat EMA below price reads the same as a falling one. Read the StackChange signal for ordering, not the level.
