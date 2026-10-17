# Relative Volume

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Current volume relative to its own recent average, normalising for the fact that volume is not comparable across instruments or times of day.


## Formula

```
RVOL = Volume / SMA(Volume, n)
```


## Range

Typically 0 to 5+|VolumeClimax


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| VolumeClimax | Neutral | RVOL exceeds 3.0 |

## Divergence

Not supported.

## Levels and other outputs

- Default window 20. Bands: 1.5 participation, 3.0 climax.


## Notes

- This is the platform's only participation gate and the pivot of the volume-climax lifecycle that replaces the forced-close cascade under the candle-only constraint.
