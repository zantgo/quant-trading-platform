# Chande Momentum Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Net momentum divided by total movement, isolating directional conviction from volatility.


## Formula

```
CMO = sum(Close[i] - Close[i-1] where positive) / sum(abs(Close[i] - Close[i-1])) * 100
```


## Range

-100 to +100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| ZeroLineCross | Direction | CMO crosses zero |
| Threshold | Contrarian | CMO crosses +/-50 |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- **Supported** - part of the series-divergence set.
