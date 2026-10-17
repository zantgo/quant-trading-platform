# ATR Percentile

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Where current range volatility sits within its own recent distribution, in the same form BBWP provides for band width.

## Formula

```
percentile = 100 * count(prior ATR < current ATR) / (n - 1)
```

## Range

0 to 100

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Percentile crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Percentile rank

## Notes

- Requires a full warm window. A partial window produces a confident and wrong rank, which is worse than no rank.

