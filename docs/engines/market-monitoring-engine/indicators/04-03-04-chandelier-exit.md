# Chandelier Exit

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction  |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

An ATR-anchored trailing stop placed beyond the extreme of a lookback window, giving a volatility-scaled directional reference rather than a fixed percentage one.

## Formula

```
long_stop = max(High, n) - ATR(n) * mult; short_stop = min(Low, n) + ATR(n) * mult
```

## Range

Price-relative

## Signals

| Kind | Fires when |
|---|---|
| TrendFlip - Price closes through the stop in the opposite direction. |

## Divergence

Not supported.

## Levels and other outputs

- Long stop and short stop levels

## Notes

- The multiplier is the tunable, not the percentage distance. This is what keeps the reading duration-independent.

