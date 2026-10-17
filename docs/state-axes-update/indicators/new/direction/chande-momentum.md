# Chande Momentum Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Net change divided by total change, isolating directional conviction from plain volatility.

## Formula

```
CMO = sum(positive delta) / sum(abs(delta)) * 100
```

## Range

-100 to +100

## Signals

| Kind | Fires when |
|---|---|
| ZeroLineCross - CMO crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Signed momentum value
- oversold and overbought states

## Notes

- CMO can exceed its nominal bounds on gap moves. Clamp before normalising.

