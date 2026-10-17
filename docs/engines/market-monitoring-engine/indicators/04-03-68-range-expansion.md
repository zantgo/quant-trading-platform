# Range Expansion versus Prior Leg

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether the current leg is larger than the one before it - context that a percentage of this leg cannot give.

## Formula

```
expansion = current leg range / prior leg range
```

## Range

Non-negative ratio

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Expansion crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Expansion ratio
-  both leg ranges

## Notes

- |A leg that is 80% retraced is unremarkable if the prior leg was twice as large.

