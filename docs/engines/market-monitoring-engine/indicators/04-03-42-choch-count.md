# CHoCH Count Since Leg Start

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many structural character changes have occurred within the current leg.

## Formula

```
count(CHoCH) since the current leg began
```

## Range

0 to journal capacity

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count increments. |

## Divergence

Not supported.

## Levels and other outputs

- Event counter

## Notes

- -|Repeated character changes in one leg is a materially different reading from a single one.

