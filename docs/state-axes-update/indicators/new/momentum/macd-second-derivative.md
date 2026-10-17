# MACD Histogram Second Derivative

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The rate of change of the rate of change of MACD - acceleration of momentum, separating a fading trend from a accelerating one.

## Formula

```
acceleration = delta(histogram) - delta(histogram[-1])
```

## Range

Signed, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Acceleration reverses sign. |

## Divergence

Not supported.

## Levels and other outputs

- Acceleration value

## Notes

- |Zero means the histogram's rate of change is constant. Read together with the histogram's sign to distinguish acceleration from deceleration.

