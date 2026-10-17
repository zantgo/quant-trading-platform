# Donchian Channel

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Rolling highest high and lowest low, forming a channel whose break is the definition of a breakout.


## Formula

```
upper = max(High, n); lower = min(Low, n); middle = (upper + lower) / 2
```


## Range

upper >= lower; price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Direction | Close breaks the upper or lower band |
| BandTouch | Structure | Close touches a channel boundary |
| LevelTest | Structure | Close approaches a channel boundary |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
