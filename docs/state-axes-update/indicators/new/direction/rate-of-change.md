# Rate of Change

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Percentage change over a fixed lookback - the simplest possible statement of how fast price is moving.

## Formula

```
(Close - Close[-n]) / Close[-n] * 100
```

## Range

-100 to +100, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Threshold - ROC crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Signed rate value

## Notes

- Unbounded by construction. Normalise before it reaches a bounded score.

