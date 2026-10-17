# Volume Volatility

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The variability of volume relative to its own longer-run variability - whether participation is steady or erratic.

## Formula

```
ratio = sd(Volume, short) / sd(Volume, long)
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

- Ratio

## Notes

- |-|Steady volume supports a trend reading
-  erratic volume undermines it even when the average is unchanged.

