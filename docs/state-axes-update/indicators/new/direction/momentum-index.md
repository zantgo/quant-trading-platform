# Momentum Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Ratio of summed forward change to summed backward change - the directional-convention version of Rate of Change.

## Formula

```
MI = sum(Close - Close[-n]) / sum(Close[-n]) * 100
```

## Range

-100 to +100

## Signals

| Kind | Fires when |
|---|---|
| Threshold - MI crosses zero. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- Signed momentum value

## Notes

- Normalised to [-1, +1] naturally, which makes it one of the cleaner inputs to a confluence score.

