# Aroon

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Progression · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

How recently the highest high and lowest low occurred, as a percentage of the lookback window.


## Formula

```
AroonUp = (n - barsSinceHighestHigh) / n * 100; AroonDn = (n - barsSinceLowestLow) / n * 100; oscillator = Up - Down
```


## Range

0 to 100 each


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Crossover | Direction | Aroon Up crosses Aroon Down |
| Threshold | Direction | Aroon reaches 100 or 0 |
| TrendFlip | Direction | Aroon oscillator changes sign |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
