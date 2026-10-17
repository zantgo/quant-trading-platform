# ADX Peak Decay

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Momentum |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

How far trend strength has fallen from its own maximum since the last directional crossover - the framework's cleanest peak-decay ratio.

## Formula

```
decay = 1 - ADX / max(ADX since last +DI/-DI crossover)
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - ADX crosses below a decay fraction of its peak. |

## Divergence

Not supported.

## Levels and other outputs

- Decay ratio and the peak it is measured against

## Notes

- A ratio, not a level. 0 means at the peak
-  1 means fully decayed.|**Requires ADX peak to be published.** The peak exists internally in the present set and is discarded - this reading is the reason to publish it.

