# Candlestick Patterns

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Named single, two and three candle shapes, carried through a formation and confirmation lifecycle.


## Formula

```
29 patterns over 1 to 3 candles, evaluated on relative candle geometry; lifecycle runs Formed to Confirmed or Invalidated within the confirmation window
```


## Range

Quality 0 to 1


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| PatternForming | Direction | Candlestick pattern forms or confirms |

## Divergence

Not supported.

## Levels and other outputs

- Pattern identity, direction, quality, status, **age in bars**


## Notes

- The lifecycle state machine is genuinely useful: a formed pattern is a hypothesis, a confirmed one is evidence.- Age must be published
- without it an old pattern is indistinguishable from a fresh one.
