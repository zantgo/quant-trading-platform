# Completion Criterion Met

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Which of the four completion criteria has fired, if any - structural, extension, character change, or volume climax.

## Formula

```
criterion = first of {opposing swing confirms, price reaches an extension reference, a character-change event fires against the leg, a volume climax fails to extend}
```

## Range

Categorical

## Signals

| Kind | Fires when |
|---|---|
| TrendFlip - A criterion is met. |

## Divergence

Not supported.

## Levels and other outputs

- Criterion name
-  all four flags

## Notes

- |-|Publishing which criterion fired is more informative than a single boolean, because each implies a different kind of completion.

