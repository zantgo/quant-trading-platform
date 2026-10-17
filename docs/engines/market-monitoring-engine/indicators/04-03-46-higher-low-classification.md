# Higher-High and Lower-Low Classification

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether a market is making higher highs and higher lows, lower lows and lower highs, or neither - the structural definition of trend direction.

## Formula

```
HH / HL for bullish structure; LH / LL for bearish; mixed otherwise
```

## Range

Categorical

## Signals

| Kind | Fires when |
|---|---|
| TrendFlip - Classification changes. |

## Divergence

Not supported.

## Levels and other outputs

- Classification per swing pair

## Notes

- |Structurally distinct from the Direction axis: it reads the shape of the pivots, not the direction of the last bar.

