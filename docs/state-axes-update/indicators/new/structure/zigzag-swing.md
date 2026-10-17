# ZigZag Swing Detection

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Swing points confirmed only after price has reversed by a volatility-scaled amount, which is the standard definition of a market pivot.

## Formula

```
swing confirmed when |Close - extreme| > k * ATR(n); alternate high and low,
```

## Range

Price-relative

## Signals

| Kind | Fires when |
|---|---|
| Breakout - New swing confirmed. |

## Divergence

Not supported.

## Levels and other outputs

- Confirmed pivot list with bar index and timestamp
-  current leg direction

## Notes

- The reversal threshold is what distinguishes a swing from noise. A fixed percentage threshold does not survive a change in volatility
-  the ATR-scaled one does.|**This is the foundational reading for Progression.** Every anchor-dependent reading in that axis resolves to a pivot from this list.

