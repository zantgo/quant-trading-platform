# Displacement from Origin (Absolute)

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The raw price distance travelled from the movement's origin.

## Formula

```
displacement = Close - origin_price
```

## Range

Signed price distance

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Displacement reverses sign. |

## Divergence

Not supported.

## Levels and other outputs

- Signed distance

## Notes

- |The most basic Progression reading and the one most often wrong, because the origin is usually taken from the wrong pivot.

