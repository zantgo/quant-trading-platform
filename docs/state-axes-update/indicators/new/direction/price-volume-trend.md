# Price Volume Trend

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A cumulative line weighting each bar's volume by that bar's percentage price change.

## Formula

```
PVT = PVT[-1] + Volume * (Close - Close[-1]) / Close[-1]
```

## Range

Unbounded signed

## Signals

| Kind | Fires when |
|---|---|
| Crossover - PVT reverses direction. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Cumulative line

## Notes

- |Close relative to OBV: OBV weights only the sign of the change, PVT weights its magnitude.

