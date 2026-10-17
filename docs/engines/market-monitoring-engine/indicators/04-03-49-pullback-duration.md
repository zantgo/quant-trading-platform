# Pullback Duration

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How many bars the pullback lasted before the impulse resumed.

## Formula

```
bars from impulse end to impulse resume
```

## Range

0 to window length

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Pullback ends. |

## Divergence

Not supported.

## Levels and other outputs

- Bar count

## Notes

- -|Short pullbacks preserve trend
-  long ones indicate the impulse has failed.

