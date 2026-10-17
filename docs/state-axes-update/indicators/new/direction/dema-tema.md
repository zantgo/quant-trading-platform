# DEMA and TEMA

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Higher-order moving averages that reduce the lag of an exponential average without the overshoot a simple extrapolation produces.

## Formula

```
DEMA = 2*EMA(C,n) - EMA(EMA(C,n),n); TEMA = 3*EMA1 - 3*EMA2 + EMA3 where EMA1=EMA(C,n), EMA2=EMA(EMA1,n), EMA3=EMA(EMA2,n)
```

## Range

Price-relative

## Signals

| Kind | Fires when |
|---|---|
| Crossover - DEMA or TEMA crosses the price. |

## Divergence

Not supported.

## Levels and other outputs

- Smoothed line

## Notes

- DEMA and TEMA are near-identical in practice
-  pick one per axis role rather than adding both to the confluence.

