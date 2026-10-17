# Kaufman Efficiency Ratio

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **NEW** - to be built |
| **Axis coverage** | Structure - **consumed** by Progression stage classification |
| **Retention tier** | A |
| **Data source** | OHLCV candles |

## Purpose

Net displacement divided by total path travelled - how much of the movement was progress rather than churn.

## Formula

```
ER = abs(Close - Close[-n]) / sum(abs(Close[i] - Close[i-1]))
```

## Range

0 to 1

## Signals

| Kind | Fires when |
|---|---|
| Threshold - ER crosses a low band. |

## Divergence

Not supported.

## Levels and other outputs

- Efficiency ratio

## Notes


- `D = sum(abs(Close[i] - Close[i-1])) = 0` (a perfectly flat window) → **`UNDEFINED`**. Never `0`, `1`, or `NaN`. A flat window has no path efficiency, and reporting `1` would assert the most efficient path on no evidence. See `leg.md` §4.0 and `change-requests.md` CR-6.
- |0.9 is a clean impulse
-  0.3 is a grind.|**Unsigned** and therefore direction-neutral: it answers *how direct was the path, regardless of which way*. That makes it a **Structure** reading. Do not confuse it with **Path Efficiency** (`new/progression/path-efficiency.md`), which keeps the sign and answers *how direct, and which way* - they differ in sign on any downward net move. Owner: **Structure**; consumed by Progression stage classification. See `change-requests.md` CR-6.|

