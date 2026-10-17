# Volatility Rate of Change

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The rate of change of realised volatility itself, giving the expansion or contraction state as a continuous quantity.

## Formula

```
rate = delta(sd(log returns, n))
```

## Range

Signed percentage change

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Rate crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Rate value

## Notes

- |-|The expanding and contracting labels are derived from this rate, not from a level crossing a threshold.

