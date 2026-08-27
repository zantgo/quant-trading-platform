# SignalKind: ZeroLineCross

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Momentum
**Purpose:** Specification for the `ZeroLineCross` SignalKind — the event where an oscillator crosses its zero or mid-line, marking a momentum-regime shift.

---

## 1. Definition

A **ZeroLineCross** fires when an oscillator transitions across its neutral reference (zero for bipolar oscillators, the mid-line for bounded ones such as RSI at 50).

| Example | Bullish | Bearish |
|---------|---------|---------|
| MACD histogram | crosses above 0 | crosses below 0 |
| RSI | crosses above 50 | crosses below 50 |
| CMF | crosses above 0 | crosses below 0 |
| LinReg Slope | crosses above 0 | crosses below 0 |

---

## 2. Producing Indicators

Declared by **11** registry entries: `rsi`, `chandemo`, `williams_r`, `awesome_oscillator`, `cci`, `macd`, `cmf`, `force_index`, `linreg_slope`, `zscore`, `oi_delta`.

> **Editorial note.** A previous revision of this section listed 13 producers including `stochastic` and `mfi`. The canonical 11 above are authoritative — the per-signal sections of [04-02-12-stochastic.md](../indicators/04-02-12-stochastic.md) and [04-02-23-mfi.md](../indicators/04-02-23-mfi.md) do not declare ZeroLineCross signals. See [04-02-00-indicator-index.md](../indicators/04-02-00-indicator-index.md) and [01-01-ontology.md Appendix B.3](../../../conceptual-foundations/01-01-ontology.md) for the registry-verified producer list.

---

## 3. Detection Semantics

```
prev = PreviousBarState value
IF prev ≤ midline AND curr > midline → Bullish zero-line cross
IF prev ≥ midline AND curr < midline → Bearish zero-line cross
```

Emitted on the **transition bar only** (`age_bars = 0`). The `momentum-shift` named event is precisely a ZeroLineCross confirming a change in momentum polarity.

### 3.1 The `momentum-shift` pattern

`momentum-shift` denotes an oscillator's zero/mid-line cross that flips the momentum sign. When corroborated by a matching [Crossover](05-02-02-crossover.md) (e.g. MACD line×signal + histogram zero-cross), the shift is high-conviction.

---

## 4. Confirmation

ZeroLineCross is emitted `Confirmed` on candle close. Sustained readings beyond the line become `Threshold`-zone signals.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `ZeroLineCross` (via `label`, e.g. `RSI_ZERO_CROSS_BULLISH`). |
| Direction | Bullish / Bearish. |
| Freshness | Fresh on the transition bar. |
| Priority | Medium — momentum regime marker. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [rsi.md](../indicators/04-02-11-rsi.md) · [macd.md](../indicators/04-02-17-macd.md) · [linreg_slope.md](../indicators/04-02-38-linreg-slope.md)
- [SignalKind: Crossover](05-02-02-crossover.md) · [SignalKind: Threshold](05-02-03-threshold.md)
