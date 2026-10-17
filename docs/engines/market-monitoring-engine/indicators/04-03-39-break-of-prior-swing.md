# Break of Prior Swing

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether the current close has broken the most recent confirmed swing in either direction.

## Formula

```
breakout when Close > prior swing high; breakdown when Close < prior swing low
```

## Range

Boolean

## Signals

| Kind | Fires when |
|---|---|
| Breakout - A prior swing is broken. |

## Divergence

Not supported.

## Levels and other outputs

- Break state and the level broken

## Notes

- Distinct from the SMC break of structure, which requires a sequence. This is the single-swing version.

