# Volume Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Percentage difference between a fast and slow average of volume - participation accelerating or fading.

## Formula

```
VO = 100 * (EMA(Volume, slow) - EMA(Volume, fast)) / EMA(Volume, slow)
```

## Range

-100 to +100, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Crossover - VO crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- VO value

## Notes

- |Participants often
-  20s and 10s is a common pairing. Whatever the pair, it must be fixed across durations by role.

