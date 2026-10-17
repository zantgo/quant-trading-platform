# Swing Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A measure of how much a price extreme was confirmed by volume movement, weighting the two together.

## Formula

```
SI = delta(High) * delta(Volume) / max(abs(delta High), abs(delta Volume))
```

## Range

Signed, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Threshold - SI crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- SI series

## Notes

- |The first published use of the idea was an actual market index. Here it is a strength-of-extreme measure, and the denominator guard against a zero input is required.

