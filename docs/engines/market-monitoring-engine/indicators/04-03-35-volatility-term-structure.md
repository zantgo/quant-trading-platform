# Volatility Term Structure

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Fast volatility divided by slow volatility - whether the near horizon is more or less agitated than the background.

## Formula

```
structure = HV(fast period) / HV(slow period)
```

## Range

Non-negative ratio

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Ratio crosses a sustained extreme. |

## Divergence

Not supported.

## Levels and other outputs

- Term structure ratio

## Notes

- A ratio above 1 means short-horizon movement is outpacing the background. This is the volatility form of the cross-duration gradient.

