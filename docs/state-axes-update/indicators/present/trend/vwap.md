# VWAP

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Volume-weighted average price for the session, the standard institutional reference for where a day's trading was balanced.


## Formula

```
typical_price = (High + Low + Close) / 3; VWAP = sum(typical_price * Volume) / sum(Volume)
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Direction | Close approaches session VWAP |

## Divergence

Not supported.

## Levels and other outputs

- Session VWAP level


## Notes

- Reset convention is session-scoped and must be stated explicitly by the implementation.
