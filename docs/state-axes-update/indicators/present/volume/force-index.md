# Force Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Price change multiplied by volume, smoothed, measuring the force behind a move rather than its extent.


## Formula

```
rawFI = (Close - Close[-1]) * Volume; FI = EMA(rawFI, period); mean_abs = EMA(abs(rawFI))
```


## Range

Signed, unbounded


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| ZeroLineCross | Direction | Force Index crosses zero |
| Threshold | Direction | abs(FI) exceeds k x its own mean absolute |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- FI value and its EMA
- mean absolute FI baseline
