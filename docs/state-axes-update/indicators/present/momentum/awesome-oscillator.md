# Awesome Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Difference between a fast and slow average of median price, isolating the short-horizon impulse from the background.


## Formula

```
median = (High + Low) / 2; AO = SMA(median, fast) - SMA(median, slow)
```


## Range

Unbounded signed


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| ZeroLineCross | Direction | AO crosses zero |
| Threshold | Neutral | AO crosses a magnitude band |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- AO value and its rising/falling state
