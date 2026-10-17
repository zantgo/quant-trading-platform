# Consecutive Same-Direction Closes

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

An unsmoothed persistence count - how many bars in a row have closed in the same direction.

## Formula

```
count of consecutive bars where Close[i] > Close[i-1] (or the inverse)
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count resets to zero. |

## Divergence

Not supported.

## Levels and other outputs

- Run length

## Notes

- |Deliberately unsmoothed. It is included precisely because every other momentum reading in the system is smoothed, and smoothing hides exhaustion.

