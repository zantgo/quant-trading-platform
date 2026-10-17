# Fisher Transform

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A bounded transform that amplifies the extremes of a Gaussian input, making subtle turning points visible without distorting the total range.

## Formula

```
x = 2 * ((Close - min) / (max - min)) - 1; HT = 0.66 * ln((1 + x) / (2 - x)) + 0.67 * HT[-1]; then inverse transform and shift
```

## Range

-1 to +1 after the inverse transform

## Signals

| Kind | Fires when |
|---|---|
| Threshold - HT reverses direction. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Transformed value and its prior value

## Notes

- The recursion makes this reading sensitive to how the window is seeded. Warm-up behaviour must be specified, not left to emerge.

