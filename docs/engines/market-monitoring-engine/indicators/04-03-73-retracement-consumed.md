# Retracement Consumed Since Last Extreme

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How much of the move from the last extreme has already been given back.

## Formula

```
consumed = (extreme - Close) / (extreme - origin)
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Retracement crosses a fib band. |

## Divergence

Not supported.

## Levels and other outputs

- Consumed fraction

## Notes

- |-|Distinct from pullback depth, which measures against the preceding impulse rather than the current extreme.

