# Level Test Count

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many times a level has been tested since it was created - the difference between a fresh level and a worn one.

## Formula

```
count of touches since level creation
```

## Range

0 to level lifetime

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count increments on each touch. |

## Divergence

Not supported.

## Levels and other outputs

- Touch count per level
-  last touch timestamp

## Notes

- Requires a level lifetime to be tracked, so the journal must retain creation time.|**Level test count is already tracked internally in the present set and discarded.** This reading is the reason to publish it.

