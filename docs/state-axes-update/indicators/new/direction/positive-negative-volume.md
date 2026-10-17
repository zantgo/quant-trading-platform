# Negative and Positive Volume Index

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Two cumulative lines that move only on one kind of volume bar, isolating whether moves happen on conviction or on noise.

## Formula

```
NVI = NVI[-1] + Volume * deltaC/C when Volume < Volume[-1], otherwise unchanged; PVI is the complement
```

## Range

Unbounded signed

## Signals

| Kind | Fires when |
|---|---|
| Crossover - One index diverges from the other. |

## Divergence

Supported - regular and hidden.

## Levels and other outputs

- NVI line and PVI line

## Notes

- |The two lines diverge during distribution - heavy volume on down bars, light volume on up bars.

