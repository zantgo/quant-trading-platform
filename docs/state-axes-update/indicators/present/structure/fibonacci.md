# Fibonacci Retracement and Extension

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Proportional levels derived from the last completed swing leg, expressing where price sits within a measured move.


## Formula

```
swing_high, swing_low = the most recent opposing pivot pair; retracement = fib * (swing_high - swing_low) + swing_low for fib in {0.236, 0.382, 0.5, 0.618, 0.786}; extension = fib * (swing_high - swing_low) + swing_low for fib in {1.272, 1.618, 2.0, 2.618}
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Structure | Close approaches a retracement or extension level |

## Divergence

Not supported.

## Levels and other outputs

- Twelve level prices, golden pocket band
- **swing anchor bar index and timestamp**


## Notes

- **The anchor must be published.** Without it, "70% retraced" is uninterpretable, because the reader cannot tell how old the leg is.- Extension levels are the closest thing this system has to a measured-completion target, and are a named Progression completion criterion.
