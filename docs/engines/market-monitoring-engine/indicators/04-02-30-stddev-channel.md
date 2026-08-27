# Standard Deviation Channel (20)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function
The Standard Deviation Channel is a volatility-based envelope that uses a linear regression centerline (least-squares fit) surrounded by ±2 standard deviation bands. Unlike Bollinger Bands (which use a simple moving average centerline), the StdDev Channel's linear regression centerline captures the directional trend component, making it more responsive in trending markets while still providing volatility-based band width. It is used for:
- Breakout detection (price outside the 2σ bands)
- Band-touch reversals in range markets
- Trend direction confirmation (price consistently above/below the centerline)
- Volatility expansion/contraction assessment

## 2. Mathematical Formula
```
n = period (default 20)

Linear regression:
  slope = (n × Σ(i × Price[i]) - Σi × ΣPrice[i]) / (n × Σi² - (Σi)²)
  intercept = (ΣPrice[i] - slope × Σi) / n
  center = intercept + slope × (n - 1)

Standard deviation:
  fitted[i] = intercept + slope × i
  σ = sqrt(Σ(Price[i] - fitted[i])² / n)

Upper = center + 2 × σ
Lower = center - 2 × σ
```

## 3. Normalization
The normalized score follows the same pattern as Bollinger and Keltner:
```
If price ≥ Upper:     norm = 1.0  (STDDEV_UPPER_BREAKOUT)
If price ≤ Lower:     norm = -1.0 (STDDEV_LOWER_BREAKOUT)
Else:
  half = (Upper - center)
  n = (price - center) / half
  norm = clamp(n × 0.8)  // [-0.8, 0.8] range with gap at ±0.8
```
Labels: `STDDEV_UPPER_BREAKOUT`, `STDDEV_LOWER_BREAKOUT`, `STDDEV_UPPER_HALF`, `STDDEV_LOWER_HALF`. The `values` sub-map carries `upper`, `center`, `lower`. The slope is stored as `raw_value` and available for frontend rendering.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Breakout | STDDEV_UPPER_BREAKOUT | Price ≥ upper band (2σ above centerline) | Bullish |
| Breakout | STDDEV_LOWER_BREAKOUT | Price ≤ lower band (2σ below centerline) | Bearish |
| BandTouch | STDDEV_UPPER_BAND_TOUCH | Price inside channel, position > 0.85 (near upper edge — mean-reversion). Structured push from engine. | Bearish |
| BandTouch | STDDEV_LOWER_BAND_TOUCH | Price inside channel, position < 0.15 (near lower edge). Structured push from engine. | Bullish |
| LevelTest | STDDEV_UPPER/LOWER_LEVEL_TEST | Price in 60-85% or 15-40% of band range — proximity awareness without a full touch | Neutral |
## 5. Scoring

`stddev_channel` is `directional: true`. Contributes to confluence scoring alongside Bollinger, Keltner, and Donchian for comprehensive volatility-channel analysis. The centerline slope serves as a trend-strength indicator.

## 6. Configuration
```json
{
  "indicators": {
    "stddev_channel_period": 20
  }
}
```
