# Slope Consistency

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How uniform the regression slope has been across the window - whether the move was steady or lurching.

## Formula

```
consistency = 1 - sd(slope) / abs(mean(slope))
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Consistency crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Consistency ratio

## Notes

- |Low consistency with high efficiency is a different state from low consistency with low efficiency.

