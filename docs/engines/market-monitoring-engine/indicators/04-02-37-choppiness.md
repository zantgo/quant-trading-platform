# Choppiness Index (14)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function
The Choppiness Index measures whether the market is trending or ranging by comparing the sum of true ranges over a period against the period's total price range. Values near 100 indicate a choppy, sideways, range-bound market; values near 0 indicate a strong directional trend. It is a **non-directional gate** — it never contributes a bullish or bearish score, but acts as a regime multiplier in the confluence engine: trending markets amplify directional conviction, choppy markets dampen it.

## 2. Mathematical Formula
```
TR_sum = Σ TrueRange(i), i ∈ [1, period]
HH = max(High, period)
LL = min(Low, period)
Choppiness = 100 × log10(TR_sum / (HH - LL)) / log10(period)
```

## 3. Normalization
Choppiness is a non-directional gate (`directional: false`). Its normalized value is always 0.0, but the state label classifies the current regime:
- `CHOP_CONSOLIDATION_RANGE` (≥ 61.8): range-bound/choppy — dampen confidence
- `CHOP_STRONG_TREND` (≤ 38.2): trending — amplify confidence
- `CHOP_TRANSITIONAL`: between extremes

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | CHOP_STRONG_TREND | Choppiness ≤ 38.2 — clean trending market. Trend-following favored. Amplify directional confidence. | Neutral (gate) |
| Threshold | CHOP_CONSOLIDATION_RANGE | Choppiness ≥ 61.8 — range-bound / choppy. Avoid trend entries. Dampen directional confidence. Expect mean-reversion. | Neutral (gate) |
| CompressionRelease | CHOP_SQUEEZE_COILING | Prolonged period of high Choppiness (>61.8) transitioning toward low Choppiness (<38.2). Indicates compressed energy coiling before a trending expansion. Distinct from the threshold-only classification signal. | Neutral (gate) |

## 5. Configuration
```json
{
  "indicators": {
    "chop_period": 14
  }
}
```
