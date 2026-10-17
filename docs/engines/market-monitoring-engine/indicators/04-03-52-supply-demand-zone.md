# Supply and Demand Zone

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The last opposing candle before a displacement move, marking the zone from which the move originated.

## Formula

```
demand: last bullish candle before an up impulse; supply: the mirror
```

## Range

Price zone

## Signals

| Kind | Fires when |
|---|---|
| LevelTest - Price returns to the zone. |

## Divergence

Not supported.

## Levels and other outputs

- Zone bounds
-  creation bar

## Notes

- A zone without a creation bar has no maturity, and an old zone behaves differently from a new one.

