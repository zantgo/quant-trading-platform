# Volatility Expansion Across the Duration Ladder

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum  |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The ratio of realised volatility at a fast duration to realised volatility at a slow one - whether short-horizon movement is outpacing the background rate.

## Formula

```
ratio = sd(HV at duration d) / sd(HV at the slowest active duration)
```

## Range

Non-negative

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Ratio crosses a sustained extreme. |

## Divergence

Not supported.

## Levels and other outputs

- Cross-duration ratio

## Notes

- The only reading in the system that requires more than one duration to compute. It is the quantitative form of the cross-duration gradient.

