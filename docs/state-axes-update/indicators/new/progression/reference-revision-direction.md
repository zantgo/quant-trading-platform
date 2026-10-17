# Reference Revision Direction

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Which way the reference was last revised - upward revisions indicate the movement is stronger than modelled.

## Formula

```
direction = sign(target - prior_target)
```

## Range

Signed

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Direction of revision reverses. |

## Divergence

Not supported.

## Levels and other outputs

- Revision direction

## Notes

- |-|Repeated upward revisions are themselves a statement about the movement, independent of its stage.

