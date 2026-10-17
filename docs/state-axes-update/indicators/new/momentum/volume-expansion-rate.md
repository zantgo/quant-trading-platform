# Volume Expansion and Contraction Rate

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How quickly the relative volume measure is itself changing - participation accelerating or fading.

## Formula

```
rate = delta(V / SMA(V, n)) / SMA(V, n)
```

## Range

Signed, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Rate crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Expansion and contraction rate

## Notes

- |RVOL says how much volume
-  this says how fast that level is changing. The two together give participation momentum.

