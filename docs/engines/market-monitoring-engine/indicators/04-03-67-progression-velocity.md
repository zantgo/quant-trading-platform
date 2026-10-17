# Progression Velocity

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How fast the movement is extending - extent accumulated per bar.

## Formula

```
velocity = extent from origin / bars elapsed
```

## Range

Signed, per bar

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Velocity crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Velocity value

## Notes

- Velocity is the first derivative of Progression. Where it is falling while displacement still grows, the movement is decelerating - which is the plainest statement of a late move.

