# BOS Count Since Leg Start

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many structural continuation breaks have occurred within the current leg - accumulated structure rather than single-event detection.

## Formula

```
count(BOS) since the current leg began
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

- |Requires the bounded Tier C journal. Without an event log this cannot be computed.

