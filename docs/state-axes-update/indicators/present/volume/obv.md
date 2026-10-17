# On-Balance Volume

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Cumulative volume signed by the direction of each close, accumulating buying versus selling pressure over time.


## Formula

```
OBV = OBV[-1] + Volume if Close > Close[-1]; - Volume if Close < Close[-1]; unchanged if equal
```


## Range

Unbounded signed


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| TrendFlip | Direction | OBV slope changes sign |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- **Supported** - part of the series-divergence set.


## Notes

- Cumulative line
