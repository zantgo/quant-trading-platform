# Accumulation / Distribution

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A cumulative line adding or subtracting volume according to where the close sat within each candle's range, separating accumulation from distribution over time.

## Formula

```
AD = AD[-1] + ((Close - Low) - (High - Close)) / (High - Low) * Volume
```

## Range

Unbounded signed

## Signals

| Kind | Fires when |
|---|---|
| Crossover - AD crosses its prior value. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Cumulative line

## Notes

- Diverging AD from price is one of the oldest accumulation signals. Its weakness is that a single large candle dominates a long window.

