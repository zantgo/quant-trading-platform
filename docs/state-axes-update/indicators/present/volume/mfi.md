# Money Flow Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A volume-weighted relative strength measure, applying the RSI construction to money flow rather than price.


## Formula

```
TP = (High + Low + Close) / 3; rawMF = TP * Volume; positive = sum(rawMF where TP rising); negative = sum(rawMF where TP falling); MFI = 100 - 100/(1 + positive/negative)
```


## Range

0 to 100


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| Threshold | Contrarian | MFI crosses 80 or 20 |
| CenterlineCross | Direction | MFI crosses its centreline at 50 |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- **Supported** - part of the series-divergence set.
