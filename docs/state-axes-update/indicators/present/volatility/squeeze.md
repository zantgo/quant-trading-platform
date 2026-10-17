# TTM Squeeze

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Whether Bollinger bands are contained inside Keltner bands - a compression state that resolves into a directional expansion.


## Formula

```
bb_upper = SMA + 2 * sd; bb_lower = SMA - 2 * sd; kc_upper = SMA + 1.5 * ATR; kc_lower = SMA - 1.5 * ATR; squeeze_on = (bb_lower > kc_lower) and (bb_upper < kc_upper); momentum = OLS endpoint over the value history
```


## Range

momentum unbounded signed; duration in bars


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Divergence | Structure | Price and this oscillator form a regular or hidden divergence |
| CompressionRelease | Direction | Bollinger bands leave containment inside Keltner bands |
| Threshold | Direction | Momentum direction changes |

## Divergence

**Supported.** See [`../../vocabulary.md`](../../../../engines/market-monitoring-engine/03-02-18-mme-state-vocabulary.md) for the four divergence types and the confirmation rule.

## Levels and other outputs

- None published.


## Notes

- **Supported** - part of the series-divergence set.
