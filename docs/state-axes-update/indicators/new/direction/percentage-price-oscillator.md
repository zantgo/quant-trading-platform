# Percentage Price Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

MACD expressed as a percentage of the slow average, so the reading is scale-free and comparable across instruments.

## Formula

```
PPO = (EMA(Close, fast) - EMA(Close, slow)) / EMA(Close, slow) * 100
```

## Range

-100 to +100, unbounded

## Signals

| Kind | Fires when |
|---|---|
| Crossover - PPO crosses its signal line. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- PPO value and signal line

## Notes

- Scale-free where MACD is not. Prefer PPO to MACD wherever readings are compared across instruments.

