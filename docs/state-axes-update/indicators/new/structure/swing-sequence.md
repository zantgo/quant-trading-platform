# Swing Sequence and Leg Alternation

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The ordered classification of consecutive pivots into higher-high, lower-high, higher-low and lower-low, which is the definition of trend versus range.

## Formula

```
HH if swing high exceeds prior swing high; LH if lower; LL / HL for lows
```

## Range

Categorical per pivot

## Signals

| Kind | Fires when |
|---|---|
| Breakout - A pivot changes the sequence classification. |

## Divergence

Not supported.

## Levels and other outputs

- Pivot sequence
-  per-leg direction and size

## Notes

- The four-way classification is what distinguishes a trending market from a ranging one structurally, independent of any oscillator.

