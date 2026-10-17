# Leg Duration in Bars

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The duration of the current leg, distinct from total movement age - it resets when a new leg begins even if the origin is shared.

## Formula

```
bars since the current leg began
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Duration exceeds a band. |

## Divergence

Not supported.

## Levels and other outputs

- Bar count

## Notes

- |-|Shorter than bars-since-origin by construction. Useful when a leg is a sub-structure of a larger move.

