# TRIX

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A triple-smoothed rate of change, giving a momentum reading with almost no lag against a single exponential average.

## Formula

```
TRIX = EMA(EMA(EMA(ROC(Close, n), n), n), n)
```

## Range

Signed, near zero unless trending

## Signals

| Kind | Fires when |
|---|---|
| ZeroLineCross - ROC crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Triple-smoothed ROC

## Notes

- The triple smoothing makes ROC cross zero later and stay there longer than a single smoothing. That is the intent
-  do not "correct" it.

