# Displacement from Origin (ATR-Normalised)

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

The same displacement divided by volatility, which is what makes it comparable across instruments and durations.

## Formula

```
displacement = (Close - origin) / ATR(n)
```

## Range

Signed, unitless

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Displacement crosses zero or a band. |

## Divergence

Not supported.

## Levels and other outputs

- Normalised displacement

## Notes

- **This is the reading that satisfies the rank-based principle.** A two-standard-deviation move means the same thing on any instrument at any duration
-  a raw price distance does not.

