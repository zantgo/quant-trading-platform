# Ultimate Oscillator

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A weighted blend of three buying-pressure timeframes, so short, intermediate and long momentum must agree before it reads extreme.

## Formula

```
BP = Close - min(Low, Close[-1]); TR = max(High, Close[-1]) - min(Low, Close[-1]); UO = 100 * (4*avg(BP/TR, n1) + 2*avg(BP/HC, n2) + avg(BP/LC, n3)) / 7
```

## Range

0 to 100

## Signals

| Kind | Fires when |
|---|---|
| Threshold - UO crosses its overbought or oversold band. |

## Divergence

Not supported.

## Levels and other outputs

- UO value
-  three component pressures

## Notes

- The three components use different denominators on purpose. Simplifying them to one destroys the multi-timeframe structure that makes the reading distinct.

