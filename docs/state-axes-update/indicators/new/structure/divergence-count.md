# Divergence Count Since Leg Start

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many divergences have appeared within the current leg - accumulated evidence rather than a single occurrence.

## Formula

```
count(divergence) since the current leg began
```

## Range

0 to journal capacity

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count increments on a new divergence. |

## Divergence

Not supported.

## Levels and other outputs

- Event counter
-  per-type breakdown

## Notes

- A single bearish divergence is a hint. A third inside one leg is a different statement.

