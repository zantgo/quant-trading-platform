# Pullback Depth

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How much of the preceding impulse the pullback retraced, measured against the impulse rather than in absolute terms.

## Formula

```
depth = (impulse high - pullback low) / (impulse high - impulse low)
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| LevelTest - Pullback exceeds a fib band. |

## Divergence

Not supported.

## Levels and other outputs

- Depth ratio
-  shallow or deep classification

## Notes

- A 50% pullback in a large impulse and in a small one mean different things. Always express depth against the impulse.

