# Bollinger Bandwidth Rate of Change

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum  |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The rate of change of band width, giving a continuous expansion signal where BBWP gives a distributional one.

## Formula

```
rate = delta((upper - lower) / centre)
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

- Bandwidth rate

## Notes

- BBWP and this are complementary: BBWP says where width sits in its history, this says which way it is moving. Neither replaces the other.

