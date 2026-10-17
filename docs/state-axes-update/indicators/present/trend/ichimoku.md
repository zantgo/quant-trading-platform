# Ichimoku Cloud

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A multi-line system that projects support and resistance forward in time using median balances.


## Formula

```
Tenkan = (max(High,9) + min(Low,9)) / 2; Kijun = (26H + 26L) / 2; SpanA = (Tenkan + Kijun) / 2 shifted +26; SpanB = (52H + 52L) / 2 shifted +26; Chikou = Close shifted -26
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Crossover | Direction | Close crosses Tenkan or Kijun |
| Breakout | Direction | Close leaves the cloud |
| TrendFlip | Direction | Cloud cross flips |
| LevelTest | Structure | Close approaches a cloud edge |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- LevelTest
