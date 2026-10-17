# Path Efficiency

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Progression |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

An explicitly windowed form of the efficiency idea, with the window stated rather than implied.

## Formula

```
efficiency = (Close - Close[-n]) / sum(abs(Close[i] - Close[i-1]))
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - Efficiency crosses a low band. |

## Divergence

Not supported.

## Levels and other outputs

- Efficiency ratio

## Notes


- `D = sum(abs(Close[i] - Close[i-1])) = 0` (a perfectly flat window) → **`UNDEFINED`**. Never `0`, `1`, or `NaN`. A flat window has no path efficiency, and reporting `1` would assert the most efficient path on no evidence. See `leg.md` §4.0 and `change-requests.md` CR-6.
- |-|Preferred over the formless version because the window is a declared parameter rather than a default.

