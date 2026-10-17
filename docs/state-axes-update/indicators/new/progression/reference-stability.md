# Reference Stability Flag

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether the reference target is currently being revised, which is the honest signal that the reading is provisional.

## Formula

```
stable if no revision within the revision window; revising otherwise
```

## Range

Boolean

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Stability state changes. |

## Divergence

Not supported.

## Levels and other outputs

- Stability flag

## Notes

- **Mandatory.** This is what makes the axis honest rather than falsely precise. A stage shown without it implies a confidence the estimate cannot deliver.

