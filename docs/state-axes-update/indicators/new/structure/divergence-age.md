# Divergence Age

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many bars have passed since the pivot that produced the current divergence.

## Formula

```
age = bars since the divergence pivot bar
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Age exceeds a staleness band. |

## Divergence

Not supported.

## Levels and other outputs

- Bar count
-  pivot bar and timestamp

## Notes

- **The pivot bar index is already computed in the present set and discarded at the module boundary.** Publishing it makes divergence age available with no new detection logic.

