# Time Since Origin Swing

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The age of the movement in wall-clock time, which is what the operator actually experiences.

## Formula

```
elapsed = now - origin timestamp
```

## Range

Non-negative duration

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Age exceeds a band. |

## Divergence

Not supported.

## Levels and other outputs

- Elapsed duration

## Notes

- |Bar count and elapsed time are not interchangeable for a multi-day instrument. Publish both.

