# Bars Since Origin Swing

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many bars the current movement has been running - the age of the leg.

## Formula

```
bars = current bar - origin bar index
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Age exceeds a band. |

## Divergence

Not supported.

## Levels and other outputs

- Bar count
-  origin bar index and timestamp

## Notes

- Requires a confirmed origin pivot.|**Directly answers how far along the movement is in time terms.** One of only two Progression inputs available without new construction.

