# True Strength Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A double-smoothed ratio of directional change to absolute change, giving a noise-resistant momentum reading with a natural zero line.

## Formula

```
TSI = 100 * EMA(EMA(delta, n1), n2) / EMA(EMA(abs(delta), n1), n2) where delta = Close - Close[-1]
```

## Range

-100 to +100

## Signals

| Kind | Fires when |
|---|---|
| ZeroLineCross - TSI crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Signed momentum value

## Notes

- The denominator is absolute change, so TSI answers how much of the movement was directional. A high value with low movement still reads high
-  read it with volatility context.

