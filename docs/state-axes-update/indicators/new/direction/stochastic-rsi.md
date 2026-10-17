# Stochastic RSI

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The stochastic construction applied to RSI, compressing an oscillator into its own range and exposing overbought conditions earlier than RSI alone.

## Formula

```
%K = (RSI - min(RSI, n)) / (max(RSI, n) - min(RSI, n)) * 100; %D = SMA(%K, smooth)
```

## Range

0 to 100

## Signals

| Kind | Fires when |
|---|---|
| Crossover - %K crosses %D. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- %K and %D values

## Notes

- More volatile than RSI because it inherits the stochastic's range compression. Do not read its overbought state as a stronger version of RSI's.

