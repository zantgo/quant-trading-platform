# SMC Structure

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Classification of market structure into trending or ranging, and the structural events that break or change it.


## Formula

```
swings from a rolling-window fractal; higher high after lower highs is a bullish break of structure; lower low after higher lows is a bearish break; a break against the prevailing sequence is a change of character
```


## Range

Categorical; event flags are boolean


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Structure | Break of Structure fires |
| TrendFlip | Structure | Change of Character fires |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Structure state (bullish / bearish / ranging)
- BOS and CHoCH event flags
