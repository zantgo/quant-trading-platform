# Retrace Classification

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Bucketing a retracement against the standard Fibonacci bands, giving an ordinal depth label rather than a raw percentage.

## Formula

```
depth bucketed at 0.382, 0.618 and 1.0 of the prior leg
```

## Range

Categorical

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Depth crosses a band boundary. |

## Divergence

Not supported.

## Levels and other outputs

- Shallow, normal, deep, or beyond

## Notes

- |-|The buckets are the reading
-  the continuous percentage is an intermediate value.

