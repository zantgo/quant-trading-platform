# Ease of Movement

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Price movement divided by volume - how cheaply price moved, which separates genuine effort from expensive churn.

## Formula

```
EM = ((High + Low)/2 - (High[-1] + Low[-1])/2) / Volume; EM_line = MA(EM, n)
```

## Range

Signed, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Threshold - EM line crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- EM and its moving average

## Notes

- |None. A high positive EM means price rose on little volume - a low-conviction advance.

