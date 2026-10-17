# Reference Revision Count

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many times the reference target has been revised since the origin.

## Formula

```
count of target revisions since origin
```

## Range

0 and above

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Count increments on revision. |

## Divergence

Not supported.

## Levels and other outputs

- Revision count

## Notes

- |-|A movement that has forced four upward revisions is behaving differently from one that has held its first estimate.

