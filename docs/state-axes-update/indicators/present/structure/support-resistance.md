# Support and Resistance

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Levels identified from swing pivots, classified as support or resistance by position relative to current price.


## Formula

```
pivots from fractal detection over the window; levels merged when within the minimum gap; role = Support when below price, Resistance when above; role flips on a decisive close through the level
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Structure | Close approaches a level |
| Breakout | Direction | Close breaks a level decisively |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Level prices and roles
- **level test count** and **last flip timestamp**
