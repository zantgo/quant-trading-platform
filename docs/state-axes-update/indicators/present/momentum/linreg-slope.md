# Linear Regression Slope

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

The slope of a least-squares fit through recent closes, giving the rate of price change per bar.


## Formula

```
slope = (n*sum(i*Price) - sum(i)*sum(Price)) / (n*sum(i^2) - sum(i)^2); pct_per_bar = slope / Price * 100
```


## Range

Signed, unbounded


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| ZeroLineCross | Direction | Regression slope crosses zero |

## Divergence

Not supported.

## Levels and other outputs

- Slope and percent change per bar


## Notes

- A bare scalar with no quality measure. A violent grind and a clean impulse produce the same slope. See the new Linear Regression R-squared reading for the missing half.
