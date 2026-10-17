# Chaikin Money Flow

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Volume-weighted measure of where closes sit within each candle's range, accumulated over a window.


## Formula

```
MFM = ((Close - Low) - (High - Close)) / (High - Low); MFV = MFM * Volume; CMF = sum(MFV, n) / sum(Volume, n)
```


## Range

-1 to +1


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| ZeroLineCross | Direction | CMF crosses zero |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- **Supported** - part of the series-divergence set.


## Notes

- Money flow multiplier per bar
