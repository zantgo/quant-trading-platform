# Price Trend Sharpe

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Risk-adjusted rate of change - mean return divided by its own variability, scaled to an annual figure.


## Formula

```
Sharpe = mean(returns, n) / sd(returns, n) * sqrt(periods_per_year)
```


## Range

Signed, unbounded


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| - | - | This reading emits no discrete events |

## Divergence

Not applicable - no discrete events.

## Levels and other outputs

- Annualised Sharpe ratio


## Notes

- The only quality-adjusted reading in the present set. It is the closest thing to a trend-quality measure and belongs to Momentum rather than Direction, because it rates the rate rather than the sign.
