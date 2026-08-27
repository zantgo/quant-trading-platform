# Donchian Channels (20)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

Donchian Channels display the highest high and lowest low over a rolling window, forming an upper and lower envelope around the midpoint. They are used for:

- **Breakout detection:** a close above the upper band signals a bullish breakout; below the lower band signals a bearish breakout.
- **Mean-reversion proximity:** when price approaches a band from inside without breaking out, a BandTouch signal fires (reversal expectation in range markets).
- **Trend confirmation:** price consistently near the upper band confirms an uptrend; near the lower band confirms a downtrend.

## 2. Mathematical Formula

```
Upper = max(high, period)
Lower = min(low, period)
Middle = (Upper + Lower) / 2
```

## 3. Normalization

The normalized score in [-1, 1] is computed from price position within the channel:

```
If price ≥ Upper Band:     norm = 1.0  (DONCHIAN_UPPER_BREAKOUT)
If price ≤ Lower Band:     norm = -1.0 (DONCHIAN_LOWER_BREAKOUT)
Else (inside channel):
  half = (Upper - Middle)
  n = (price - Middle) / half
  norm = clamp(n × 0.7)   // [-0.7, 0.7] range with gap at ±0.7
```

Labels: `DONCHIAN_UPPER_BREAKOUT`, `DONCHIAN_LOWER_BREAKOUT`, `DONCHIAN_UPPER_RANGE`, `DONCHIAN_LOWER_RANGE`. The `values` sub-map carries `upper`, `middle`, `lower`.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Breakout | DONCHIAN_UPPER_BREAKOUT | Price ≥ upper band | Bullish |
| Breakout | DONCHIAN_LOWER_BREAKOUT | Price ≤ lower band | Bearish |
| BandTouch | DONCHIAN_UPPER_BAND_TOUCH | Price inside channel, position > 0.85 (near upper edge — mean-reversion proximity). Structured push from engine. | Bearish |
| BandTouch | DONCHIAN_LOWER_BAND_TOUCH | Price inside channel, position < 0.15 (near lower edge). Structured push from engine. | Bullish |
| LevelTest | DONCHIAN_UPPER_LEVEL_TEST | Price within proximity of the upper band (generic band-proximity LevelTest). | Bearish |
| LevelTest | DONCHIAN_LOWER_LEVEL_TEST | Price within proximity of the lower band (generic band-proximity LevelTest). | Bullish |

> **Doc synced to runtime (AUDIT-AIU-090).** The previous table declared `DONCHIAN_MIDDLE_BAND_TEST` / `DONCHIAN_MIDDLE_BAND_SUPPORT`, which no code emits — the engine emits the generic `DONCHIAN_UPPER/LOWER_LEVEL_TEST` band-proximity labels instead.

## 5. Scoring

`donchian` is a `directional: true` indicator. Contributes to confluence scoring with both breakout strength and band-touch reversal signals.

## 6. Configuration

```json
{
  "indicators": {
    "donchian_period": 20
  }
}
```
