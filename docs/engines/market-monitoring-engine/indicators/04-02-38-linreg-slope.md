# Linear Regression Slope (20)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Group:** Regime

## 1. Introduction — Trading Function
The Linear Regression Slope measures the per-bar rate of price change using a least-squares fit over a lookback window. A positive slope indicates an uptrend; a negative slope indicates a downtrend; a flat slope indicates consolidation. It is a purely statistical directional indicator — it captures the linear trend component without smoothing lag. Traders use it to confirm trend direction and detect acceleration/deceleration.

## 2. Mathematical Formula
```
n = period
slope = (n × Σ(i × Price[i]) - Σi × ΣPrice[i]) / (n × Σi² - (Σi)²)
pct_per_bar = slope / price × 100
```

## 3. Normalization
```
norm = clamp(tanh(pct_per_bar × 3.0))
```
Labels: `LINREG_RISING_TREND` (norm > 0.1), `LINREG_FALLING_TREND` (norm < -0.1), `LINREG_FLAT` (-0.1 ≤ norm ≤ 0.1).

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| ZeroLineCross | LinReg zero cross | Slope crosses 0 (sign flip from positive to negative or vice versa). Transition-only via prev-bar slope comparison in engine. | Bullish / Bearish |

> Registry manifest (`signal_types`): ZeroLineCross. The legacy `Threshold` rows (`LINREG_RISING_TREND` / `LINREG_FALLING_TREND`) were removed — neither the registry nor the runtime deriver emits them (LinReg is a regime classifier, not a directional signal emitter).

## 5. Configuration
```json
{
  "indicators": {
    "linreg_period": 20
  }
}
```
