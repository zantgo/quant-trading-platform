# MACD

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Difference between a fast and slow exponential average, with a signal line and a histogram that isolates momentum change.


## Formula

```
MACD = EMA(Close, fast) - EMA(Close, slow); signal = EMA(MACD, signal_period); histogram = MACD - signal
```


## Range

Unbounded signed


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| Crossover | Direction | MACD crosses its signal line |
| ZeroLineCross | Direction | MACD crosses zero |
| TrendFlip | Direction | MACD crosses signal in the opposite direction to the prior state |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- **Supported** - regular and hidden, the second primary divergence source.
