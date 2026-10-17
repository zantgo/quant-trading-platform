# Pivot Points

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

| | |
|---|---|
| **Status** | **PRESENT** |
| **Axis coverage** | Direction · Structure |
| **Retention tier** | A |
| **Data source** | OHLCV candles |


## Purpose

Levels derived from a prior period's high, low and close, giving objective reference points for the next period.


## Formula

```
P = (High + Low + Close) / 3; R1 = 2P - L; S1 = 2P - H; R2 = P + (H - L); S2 = P - (H - L); R3 = High + 2(P - L); S3 = Low - 2(H - P)
```


## Range

Price-relative


## Signals

| Kind | Direction | Fires when |
|---|---|---|
| LevelTest | Structure | Close approaches a pivot level |
| Breakout | Direction | Close breaks a pivot level |
| Crossover | Direction | Close crosses the central pivot |

## Divergence

Not supported.

## Levels and other outputs

- None published.


## Notes

- Not supported.
