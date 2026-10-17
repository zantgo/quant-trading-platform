# Displacement from Origin (Percent)

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The displacement expressed as a fraction of the origin price.

## Formula

```
displacement = (Close - origin) / origin * 100
```

## Range

Signed percentage

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Displacement crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Signed percentage

## Notes

- |-|Convenient to read and violates the scale-independence principle. Publish alongside the ATR-normalised form, never instead of it.

