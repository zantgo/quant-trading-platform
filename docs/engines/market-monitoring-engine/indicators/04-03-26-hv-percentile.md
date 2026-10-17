# Historical Volatility Percentile

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The same self-referential treatment applied to return-based volatility.

## Formula

```
percentile = 100 * count(prior HV < current HV) / (n - 1)
```

## Range

0 to 100

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Percentile crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Percentile rank

## Notes

- |-|ATR percentile and HV percentile disagree meaningfully - range-based and return-based volatility measure different things. The disagreement is informative.

