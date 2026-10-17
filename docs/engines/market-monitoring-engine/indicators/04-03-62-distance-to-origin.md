# Distance to Origin

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How far the movement has travelled from where it began, normalised by volatility.

## Formula

```
distance = (Close - origin) / ATR(n)
```

## Range

Signed, unitless

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Distance crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Distance to origin

## Notes

- |-|The direct measure of extent that satisfies the rank-based principle.

