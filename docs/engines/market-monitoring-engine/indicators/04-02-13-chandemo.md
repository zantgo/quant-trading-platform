# Chande Momentum Oscillator (12)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

The Chande Momentum Oscillator (CMO) is a refined momentum indicator that measures the net momentum of price changes over a lookback period. Unlike RSI, which compresses gain/loss averages, CMO uses the raw sum of gains and losses to produce an unbounded oscillator scaled to [-100, 100]. It is used for:
- Overbought/oversold extremes (climactic exhaustion)
- Zero-line crossings (trend changes)
- Divergence detection against price

## 2. Mathematical Formula

```
Gain = sum(Close[i] - Close[i-1]) for all periods where Close[i] > Close[i-1]
Loss = sum(Close[i-1] - Close[i]) for all periods where Close[i-1] > Close[i]
CMO = (Gain - Loss) / (Gain + Loss) × 100
```

## 3. Normalization

The normalized score in [-1, 1] is a direct linear mapping:

```
norm = CMO / 100   clamped to [-1, 1]
```

Labels include: `CMO_CLIMACTIC_BULL_EXHAUSTION` (CMO extreme positive), `CMO_CLIMACTIC_BEAR_EXHAUSTION` (CMO extreme negative), `CMO_BULLISH_BIAS`, `CMO_BEARISH_BIAS`, `CMO_NEUTRAL`.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| ZeroLineCross | CMO zero cross | CMO crosses 0 (positive to negative or vice versa). Transition-only via prev-bar CMO comparison in engine. | Bullish / Bearish |
| Threshold | CLIMACTIC_BULL_EXHAUSTION | CMO at extreme bullish overextension | Bearish (exhaustion) |
| Threshold | CLIMACTIC_BEAR_EXHAUSTION | CMO at extreme bearish overextension | Bullish (exhaustion) |
| Threshold | CMO_BULLISH_BIAS / BEARISH_BIAS | CMO between extremes with directional bias | Bullish / Bearish |
| Divergence | BULLISH/BEARISH_DIVERGENCE | Price-vs-CMO divergence via SeriesDivergence (20-bar lookback). | Bullish / Bearish |

## 5. Scoring

`chandemo` is `directional: true`. Its divergence is emitted as a nested `Divergence` signal on the `chandemo` key — there is no separate `chandemo_divergence` registry entry or JSON key.

## 6. Configuration

```json
{
  "indicators": {
    "chandemo_period": 12
  }
}
```
