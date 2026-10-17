# Chaikin Volatility

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum  |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The rate of change of the high-low range envelope, isolating whether volatility is expanding or contracting regardless of direction.

## Formula

```
CV = 100 * (EMA(High - Low, t) - EMA(High - Low, t - t)) / EMA(High - Low, t - t)
```

## Range

Signed percentage change

## Signals

| Kind | Fires when |
|---|---|
| Threshold - CV crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- CV value
-  its sign is the expansion or contraction state

## Notes

- The EMA span, not the indicator period, is the meaningful parameter - it sets the rate being measured.

