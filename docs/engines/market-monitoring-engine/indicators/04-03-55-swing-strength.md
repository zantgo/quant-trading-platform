# Swing Strength

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Each swing's magnitude normalised by volatility, so swings from different periods are comparable.

## Formula

```
strength = abs(High[i] - High[i-1]) / ATR(n)
```

## Range

Non-negative ratio

## Signals

| Kind | Fires when |
|---|---|
| Threshold - A swing exceeds a strength band. |

## Divergence

Not supported.

## Levels and other outputs

- Per-swing strength

## Notes

- |Without this normalisation, a large swing during a quiet period and a small swing during a volatile one cannot be compared.

