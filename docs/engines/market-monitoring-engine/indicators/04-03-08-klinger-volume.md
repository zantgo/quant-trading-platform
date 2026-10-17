# Klinger Volume Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Long-term volume trend separated from short-term noise by applying a sign derived from the price-volume relationship.

## Formula

```
KVO = EMA(Volume * trend, 34); KVD = EMA(KVO, 55) where trend is +1 above the last price close and -1 below
```

## Range

Unbounded signed

## Signals

| Kind | Fires when |
|---|---|
| Crossover - KVO crosses KVD. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- KVO and KVD

## Notes

- The trend sign is binary by design. Using a continuous trend changes the indicator into something else.

