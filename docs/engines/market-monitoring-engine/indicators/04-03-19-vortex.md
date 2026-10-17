# Vortex Indicator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Two directional accumulation lines comparing favourable and adverse price movement, giving a directional reading that is not derived from a moving average.

## Formula

```
VM_plus = abs(High - Low[-1]); VM_minus = abs(Low - High[-1]); VI_plus = sum(VM_plus, n) / sum(VM_minus, n); VI_minus = inverse
```

## Range

Ratio, 1.0 is balanced

## Signals

| Kind | Fires when |
|---|---|
| Crossover - VI_plus crosses VI_minus. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- VI_plus, VI_minus

## Notes

- The natural neutral line is 1.0, not 0. Any normalisation must account for that or the sign convention will be wrong.

