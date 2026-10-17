# Anchored VWAP

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

VWAP accumulated from an explicitly chosen bar rather than a session boundary, giving a long-horizon fair-value reference.


## Formula

```
AVWAP = sum(typical_price * Volume) / sum(Volume) from the anchor bar onward
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Crossover | Direction | Close crosses the anchored VWAP |
| LevelTest | Direction | Close approaches the anchored VWAP |

## Divergence

Not supported.

## Levels and other outputs

- Not supported.


## Notes

- Anchor bar index and timestamp
- anchored VWAP level
