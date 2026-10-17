# Historical Volatility

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Momentum · Regime |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Annualised standard deviation of log returns, the volatility measure that depends on returns rather than on range.


## Formula

```
log_return = ln(Close / Close[-1]); HV = sd(log_returns, n) * sqrt(periods_per_year) * 100
```


## Range

Non-negative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| Threshold | Neutral | Annualised volatility crosses a magnitude band |

## Divergence

Not supported.

## Levels and other outputs

- Annualised volatility percentage


## Notes

- **Annualisation assumes a fixed period count per year and is therefore only valid for a fixed duration.** Comparisons across durations require re-scaling.- HV has no self-percentile of its own in this framework
- see the new Historical Volatility Percentile reading.
