# Participation State

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A composite label for how much the market is participating, from relative volume percentile and volume trend.

## Formula

```
state derived from RVOL percentile and volume trend bands
```

## Range

Categorical

## Signals

| Kind | Fires when |
|---|---|
| Threshold - State changes. |

## Divergence

Not supported.

## Levels and other outputs

- Thin, healthy, or heavy

## Notes

- |-|Thin participation invalidates directional readings far more often than a low score does.

