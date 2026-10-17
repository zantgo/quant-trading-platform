# Efficiency Ratio regime

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Bucketing the efficiency ratio into trending, mixed, and ranging, giving a structural label independent of any oscillator.

## Formula

```
ER bucketed into three bands from the efficiency ratio
```

## Range

Categorical

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Bucket changes. |

## Divergence

Not supported.

## Levels and other outputs

- Bucket

## Notes

- |-|A structural state. Unlike ADX and Choppiness, it says how clean the movement is rather than how much movement there is.

