# Pullback Volume Ratio

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether the pullback was ordered or opportunistic, measured by the volume it took relative to the impulse.

## Formula

```
ratio = sum(pullback volume) / sum(impulse volume)
```

## Range

Non-negative ratio

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Ratio crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Volume ratio

## Notes

- |A deep pullback on light volume is a different structure from a shallow pullback on heavy volume.

