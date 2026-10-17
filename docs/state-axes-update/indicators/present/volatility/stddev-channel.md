# Standard Deviation Channel

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

A linear regression line with bands at a multiple of the residual scatter around it.


## Formula

```
slope = (n*sum(i*Price) - sum(i)*sum(Price)) / (n*sum(i^2) - sum(i)^2); centre = intercept + slope*(n-1); upper = centre + k * sd(residual); lower = centre - k * sd(residual)
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Breakout | Direction | Close breaks the +/-2 sigma band |
| BandTouch | Structure | Close touches a band boundary |
| LevelTest | Structure | Close approaches a band boundary |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
