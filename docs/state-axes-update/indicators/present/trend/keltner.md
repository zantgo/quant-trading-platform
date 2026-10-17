# Keltner Channel

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

An EMA centre line with an ATR-scaled envelope, defining a trend-following band that widens with volatility.


## Formula

```
centre = EMA(Close, p); range = ATR(a) * mult; upper = centre + range; lower = centre - range
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Direction | Close breaks the Keltner envelope |
| BandTouch | Structure | Close touches the envelope boundary |
| LevelTest | Structure | Close approaches the envelope boundary |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
