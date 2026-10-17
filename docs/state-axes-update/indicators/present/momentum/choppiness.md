# Choppiness Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Whether the market is trending or chopping, by comparing summed true range to the total range spanned.


## Formula

```
CHOP = 100 * log10(sum(TrueRange, n) / (HighestHigh - LowestLow)) / log10(n)
```


## Range

0 to 100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Neutral | Choppiness crosses 61.8 or 38.2 |
| CompressionRelease | Neutral | Choppiness falls from the chop band |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Choppiness value
