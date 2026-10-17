# Bars Since Trend Slope Flip

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many bars have passed since the sign of the trend slope last changed - a direct clock on how long **this indicator's** own trend slope has held.

## Formula

```
bars since sign(slope) last changed
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count resets to zero on a sign change. |

## Divergence

Not supported.

## Levels and other outputs

- Bar count

## Notes

- |A companion reading to the ADX exhaustion hook, which counts consecutive decelerations. Together they describe how long the move has run and whether it is still building.

