# Elder's Force Index (13)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function
Elder's Force Index (FI) combines price change direction with volume to measure the strength of buying or selling pressure behind a move. A positive FI indicates bulls are in control (price rising on volume); a negative FI indicates bears are in control (price falling on volume). The raw FI is smoothed by an exponential moving average (default 13-period) to produce tradeable signals. FI detects whether money is flowing into or out of a move — a key institutional confirmation for trend strength and divergence analysis. Zero-line crosses are the primary signal: FI crossing above 0 confirms bullish momentum; crossing below 0 confirms bearish momentum.

## 2. Mathematical Formula
```
Raw FI = (Close - Previous Close) × Volume
FI = EMA(Raw FI, 13)
```

## 3. Normalization
```
norm = clamp(tanh(fi / 5000.0))
```
The divisor 5000.0 provides a reasonable saturation range. Labels: `FI_BULLISH` (FI > 0), `FI_BEARISH` (FI < 0).

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| ZeroLineCross | FI zero cross | FI crosses 0 (bullish/bearish flow flips). Transition-only via engine. | Bullish / Bearish |
| Threshold | FI extreme | Significant |FI| magnitude indicating strong directional flow | Bullish / Bearish |
## 5. Scoring

`force_index` is `directional: true`. Contributes to volume-weighted confluence scoring as a volume-momentum confirmation tool: positive FI with rising price confirms uptrend health; negative FI with falling price confirms downtrend; FI diverging from price warns of trend exhaustion.

## 6. Configuration
```json
{
  "indicators": {
    "force_index_smoothing": 13
  }
}
```
