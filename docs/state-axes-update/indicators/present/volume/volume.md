# Volume

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Raw traded volume, the denominator for every relative-volume reading.


## Formula

```
V
```


## Range

Non-negative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| VolumeClimax | Neutral | Volume exceeds 3x its average |

## Divergence

Not supported.

## Levels and other outputs

- Raw volume


## Notes

- On its own carries no direction. It exists in this framework as the base for RVOL, the climax signal, and volume-profile construction.
