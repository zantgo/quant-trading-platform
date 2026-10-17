# Keltner and Bollinger Width Convergence

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum  |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The relative gap between the two volatility envelopes - a direct measure of whether the slow and fast views of volatility agree.

## Formula

```
convergence = (keltner_width - bollinger_width) / keltner_width
```

## Range

-1 to +1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Convergence crosses zero. |

## Divergence

Not supported.

## Levels and other outputs

- Convergence value

## Notes

- |A large positive gap means the ATR envelope is wider than the standard-deviation envelope - recent movement has been directional rather than noisy.

