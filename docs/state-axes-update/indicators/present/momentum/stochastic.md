# Stochastic Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Where price sits within its own recent high-low range, as a percentage.


## Formula

```
%K = (Close - LowestLow(n)) / (HighestHigh(n) - LowestLow(n)) * 100; %D = SMA(%K, smooth)
```


## Range

0 to 100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| Crossover | Direction | %K crosses %D |
| Threshold | Contrarian | %K crosses 80 or 20 |
| CenterlineCross | Direction | %K or %D crosses its centreline at 50 |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- Divergence
