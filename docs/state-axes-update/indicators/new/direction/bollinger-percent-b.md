# Bollinger %B

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction |
| **Retention tier** | C |
| **Data source** | OHLCV candles |

## Purpose

Where price sits inside the Bollinger envelope as a fraction of its width, normalised so it is comparable across instruments and volatility conditions.

## Formula

```
%B = (Close - lower) / (upper - lower)
```

## Range

0 to 1 between the bands; unbounded outside

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Price holds above the upper band (0.236-style extension reading). |

## Divergence

Not supported.

## Levels and other outputs

- Pressure band identity
-  position 0-1

## Notes

- Band width is handled by BBWP, not by this reading. A reading above 1 is a breakout state, not an error.

