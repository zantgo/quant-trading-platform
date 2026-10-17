# SMC Order Blocks

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

The last opposing candle before a structural break, marking a zone of resting interest that price may revisit.


## Formula

```
bullish: the last bearish candle before a bullish break, zone = [open, low]; bearish: the last bullish candle before a bearish break, zone = [open, high]
```


## Range

Price zone


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Structure | Price enters an order block zone |
| TrendFlip | Structure | Price tests an order block and flips |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Four zone bounds (bullish and bearish high and low)
- zone creation bar and mitigation state
