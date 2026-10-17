# Trend Quality State

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

A composite label combining regression fit, efficiency and choppiness into a single cleanliness judgment.

## Formula

```
state derived from R2, efficiency ratio, and choppiness bands
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

- Clean, mixed, or choppy

## Notes

- A composite, not a score. Its value is in the label
-  the components remain separately published.

