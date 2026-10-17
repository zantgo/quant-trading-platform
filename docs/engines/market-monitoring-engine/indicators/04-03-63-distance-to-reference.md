# Distance to Reference

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How far the movement still has to run, normalised by volatility.

## Formula

```
distance = (reference - Close) / ATR(n)
```

## Range

Signed, unitless

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Distance crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Distance to reference

## Notes

- |-|Reading close to zero while the movement is still extending is the classic failed-completion state.

