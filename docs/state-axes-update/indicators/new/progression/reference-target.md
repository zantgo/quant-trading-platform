# Reference Target

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The explicitly stated estimate of where the current movement will finish.

## Formula

```
target = origin + estimated_extent; basis is recorded alongside the value
```

## Range

Price-relative

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Target is revised. |

## Divergence

Not supported.

## Levels and other outputs

- Target level
-  revision count
-  basis

## Notes

- **The reference must always be published with the reading.** A stage or velocity reported without its reference is not interpretable - the same price can be 40% or 67% of the same move depending on this number.

