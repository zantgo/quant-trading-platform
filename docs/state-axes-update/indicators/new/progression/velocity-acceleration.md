# Velocity Acceleration

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The rate of change of velocity - whether the movement is speeding up or slowing down.

## Formula

```
acceleration = velocity - velocity[-1]
```

## Range

Signed, per bar squared

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Acceleration reverses sign. |

## Divergence

Not supported.

## Levels and other outputs

- Acceleration value

## Notes

- Requires a short series of velocity values.|- The distinction between velocity and acceleration is the whole of early versus late. Neither is available from a level.

