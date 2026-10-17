# Breakout versus Breakdown Classification

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether a break is upward or downward, distinguishing continuation from a reversal break at the same event.

## Formula

```
direction = sign(Close - broken level)
```

## Range

Signed

## Signals

| Kind | Fires when |
|---|---|
| Breakout - A break is confirmed by close. |

## Divergence

Not supported.

## Levels and other outputs

- Break direction

## Notes

- |A close above resistance is bullish by the break and bearish by the structure that preceded it. Both readings exist here.

