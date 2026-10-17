# Velocity versus Prior Leg Velocity

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Whether the current movement is extending faster than the last one did.

## Formula

```
ratio = velocity / prior_leg_velocity
```

## Range

Non-negative ratio

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Ratio crosses a band. |

## Divergence

Not supported.

## Levels and other outputs

- Velocity ratio

## Notes

- |Requires Tier B retention to hold the prior leg's velocity.

