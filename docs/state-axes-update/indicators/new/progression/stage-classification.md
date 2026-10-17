# Stage Classification

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The ordinal position of the movement in its own lifecycle - the single reading this axis exists to produce.

## Formula

```
stage derived from extent, velocity, efficiency and reference state; 5 buckets: early, early-mid, mid, late, terminal
```

## Range

Ordinal, 5 buckets

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Stage advances or regresses. |

## Divergence

Not supported.

## Levels and other outputs

- Stage
-  contributing sub-readings

## Notes

- **Never publish a continuous percentage.** The estimate is revision-prone, so a number would imply precision that does not exist.

