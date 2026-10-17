# Linear Regression R-Squared

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The proportion of price variance explained by the linear fit - the missing quality half of trend measurement.

## Formula

```
R2 = SSR / SST over the window
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - R2 crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- R2 value

## Notes

- |**The single most valuable addition to the present set.** A strong trend and a grinding one produce the same slope
-  only R2 separates them. Every quality reading in Momentum depends on it.

