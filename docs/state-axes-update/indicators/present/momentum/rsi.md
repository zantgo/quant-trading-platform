# Relative Strength Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Proportion of upward movement relative to total movement, as a bounded oscillator between 0 and 100.


## Formula

```
AvgGain = (prevAvgGain * (n-1) + max(Close - prevClose, 0)) / n; AvgLoss = (prevAvgLoss * (n-1) + max(prevClose - Close, 0)) / n; RS = AvgGain / AvgLoss; RSI = 100 - 100/(1 + RS)
```


## Range

0 to 100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| Threshold | Contrarian | RSI crosses 70 or 30 |
| CenterlineCross | Direction | RSI crosses its centreline at 50 |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- **Supported** - regular and hidden. Bullish when price makes a lower low and RSI a higher low
- bearish when price makes a higher high and RSI a lower high. Detected by comparing the extremes of the two halves of a lookback window. Confirmed when a candle close breaks the nearest support or resistance by more than 0.2%.
