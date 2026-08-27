# Keltner Channels (20, 10, 2.0)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

Keltner Channels use an EMA midline surrounded by ATR-based bands. They are more responsive than Bollinger Bands (ATR-based vs standard-deviation-based) and are used for:

- **Breakout detection:** price exiting the bands signals a volatility-driven breakout.
- **Trend direction:** price consistently above the midline confirms an uptrend; below confirms a downtrend.
- **Band-touch reversals:** in range markets, price touching a band edge without breaking out is a mean-reversion signal.

## 2. Mathematical Formula

```
Middle = EMA(close, ema_period)
Range = ATR(atr_period) × multiplier
Upper = Middle + Range
Lower = Middle - Range
```

## 3. Normalization

The normalized score in [-1, 1] is identical in structure to Donchian:

```
If price ≥ Upper:     norm = 1.0  (KELTNER_UPPER_BREAKOUT)
If price ≤ Lower:     norm = -1.0 (KELTNER_LOWER_BREAKOUT)
Else:
  half = (Upper - Middle)
  n = (price - Middle) / half
  norm = clamp(n × 0.8)   // [-0.8, 0.8] range, gap at ±0.8
```

Labels: `KELTNER_UPPER_BREAKOUT`, `KELTNER_LOWER_BREAKOUT`, `KELTNER_UPPER_HALF`, `KELTNER_LOWER_HALF`. The `values` sub-map carries `upper`, `middle`, `lower`.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Breakout | KELTNER_UPPER_BREAKOUT | Price ≥ upper band | Bullish |
| Breakout | KELTNER_LOWER_BREAKOUT | Price ≤ lower band | Bearish |
| BandTouch | KELTNER_UPPER_BAND_TOUCH | Price inside channel, position > 0.85 (near upper edge). Structured push from engine. | Bearish |
| BandTouch | KELTNER_LOWER_BAND_TOUCH | Price inside channel, position < 0.15 (near lower edge). Structured push from engine. | Bullish |
| LevelTest | KELTNER_UPPER_LEVEL_TEST | Price within proximity of the upper band (generic band-proximity LevelTest). | Bearish |
| LevelTest | KELTNER_LOWER_LEVEL_TEST | Price within proximity of the lower band (generic band-proximity LevelTest). | Bullish |

> **Doc synced to runtime (AUDIT-AIU-090).** The previous table declared `KELTNER_MIDDLE_BAND_TEST` / `KELTNER_MIDDLE_BAND_SUPPORT`, which no code emits — the engine emits the generic `KELTNER_UPPER/LOWER_LEVEL_TEST` band-proximity labels instead.

## 5. Scoring

`keltner` is `directional: true`. Breakouts and band-touches both contribute to directional confluence.

## 6. Configuration

```json
{
  "indicators": {
    "keltner_ema_period": 20,
    "keltner_atr_period": 10,
    "keltner_multiplier": 2.0
  }
}
```
