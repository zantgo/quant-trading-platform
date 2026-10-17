# Bollinger Band Width Percentile

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Where current band width sits within its own recent distribution - the framework's reference self-referential percentile.


## Formula

```
width = (upper - lower) / centre; BBWP = 100 * count(prior widths < current width) / (n - 1)
```


## Range

0 to 100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| CompressionRelease | Neutral | Bandwidth percentile leaves the compression band |

## Divergence

Not supported.

## Levels and other outputs

- Percentile rank
- current raw bandwidth


## Notes

- **The most important reading in the system after Direction, because it is the only true self-percentile.** BBWP below 10 is coiled and unmoved
- above 90 is bandwidth at the top of its own recent distribution.- Requires a full warm window before it emits anything meaningful. A partial window produces a confident and wrong rank.
