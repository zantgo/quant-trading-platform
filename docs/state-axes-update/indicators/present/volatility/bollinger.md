# Bollinger Bands

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A moving average with a standard-deviation envelope, giving both a volatility boundary and a normalised position within it.


## Formula

```
centre = SMA(Close, n); upper = centre + k * sd(Close, n); lower = centre - k * sd(Close, n); %B = (Close - lower) / (upper - lower)
```


## Range

%B from 0 to 1 between bands, unbounded outside


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Direction | Close breaks an outer band |
| BandTouch | Structure | Close touches a band boundary |
| LevelTest | Structure | Close approaches a band boundary |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
