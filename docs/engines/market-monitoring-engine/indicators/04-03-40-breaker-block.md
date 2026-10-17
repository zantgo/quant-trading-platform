# Breaker Block

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

An order block that failed and now acts in the opposite direction - the failed level becomes the new zone of interest.

## Formula

```
a bullish order block whose zone is closed through becomes a bearish breaker block and vice versa
```

## Range

Price zone

## Signals

| Kind | Fires when |
|---|---|
| LevelTest - Price enters the zone. |

## Divergence

Not supported.

## Levels and other outputs

- Zone bounds
-  creation bar

## Notes

- Requires an order-block history to exist. The present set tracks only a single slot.

