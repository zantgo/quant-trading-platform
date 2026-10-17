# SMC Liquidity

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Whether price swept a prior swing level and closed back inside, which removes resting liquidity at that level.


## Formula

```
sweep_buy: high exceeds the most recent swing high and the close returns below it; sweep_sell: the mirror condition on the low
```


## Range

Boolean


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Structure | A sweep occurs |
| PatternForming | Structure | Liquidity sweep detected |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Sweep side, swept level
