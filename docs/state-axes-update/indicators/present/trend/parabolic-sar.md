# Parabolic SAR

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A trailing dot that accelerates toward price as a trend extends, flipping sides on reversal.


## Formula

```
SAR[next] = SAR + AF * (EP - SAR); AF starts at step and increases by step on each new extreme point, capped at max
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| TrendFlip | Direction | SAR flips side |
| Crossover | Direction | Close crosses the SAR dot |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- SAR dot price, extreme point, direction, flip event
- **AF (acceleration factor)**
