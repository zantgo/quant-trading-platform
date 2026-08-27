# SignalKind: Threshold

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Zone / level state
**Purpose:** Specification for the `Threshold` SignalKind — the event where an indicator value enters a named significance zone (overbought, oversold, trend-strength, extreme).

---

## 1. Definition

A **Threshold** fires when a value crosses into a named band. It is the most widely declared SignalKind (26 declarations) because most oscillators and gates define significance zones.

| Example | Threshold | Meaning |
|---------|-----------|---------|
| RSI ≥ 70 | Overbought | Distribution zone (bearish). |
| RSI ≤ 30 | Oversold | Accumulation zone (bullish). |
| ADX ≥ 25 | Trend | Trend-strength gate active. |
| ADX ≥ 40 | Exhaustion | Trend potentially over-extended. |
| CCI ≥ 100 / ≤ −100 | Extreme | Momentum extreme. |

---

## 2. Producing Indicators

Declared by 26 registry entries: `adx`, `rsi`, `stochastic`, `chandemo`, `williams_r`, `awesome_oscillator`, `cci`, `macd`, `obv`, `cmf`, `mfi`, `force_index`, `atr`, `bbwp`, `squeeze`, `hv`, `aroon`, `choppiness`, `linreg_slope`, `zscore`, `open_interest`, `oi_delta`, `funding_rate`, `order_flow_imbalance`, `spread`, `depth_bias`.

Non-directional gates (ADX, ATR, HV, Choppiness, Spread, etc.) emit thresholds that modulate confidence rather than direction.

---

## 3. Detection Semantics

```
IF value ≥ upper_threshold → enter upper zone (label = OVERBOUGHT / EXTREME / ...)
IF value ≤ lower_threshold → enter lower zone (label = OVERSOLD / ...)
ELSE                        → neutral zone
```

Thresholds are **regime-aware**: e.g. RSI overbought tightens from 70 to 80 when `ADX > 30` (strong trend), preventing premature reversal signals.

---

## 4. Confirmation

Threshold signals are `Active` while the value remains in the zone; `age_bars` tracks how long the zone has been occupied. Some thresholds (e.g. `RSI_NEUTRAL_RANGE`) require sustained occupancy over multiple bars.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `Threshold` (via `label`). |
| Direction | Bullish / Bearish / Neutral (gates = Neutral). |
| Strength | Distance beyond the threshold. |
| Freshness | Bars since zone entry. |
| Confirmation | Active while in-zone. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [rsi.md](../indicators/04-02-11-rsi.md) · [adx.md](../indicators/04-02-05-adx.md) · [cci.md](../indicators/04-02-16-cci.md)
- [Indicators Guide §3 Key Thresholds](../03-02-09-mme-indicators-guide.md)
