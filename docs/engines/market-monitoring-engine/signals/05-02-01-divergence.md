# SignalKind: Divergence

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Momentum / Structure exhaustion
**Purpose:** Specification for the `Divergence` SignalKind — the disagreement between price direction and an oscillator's direction, signalling momentum exhaustion or continuation.

---

## 1. Definition

A **Divergence** fires when the price chart and an oscillator disagree about direction. It is the platform's primary momentum-exhaustion signal.

| Type | Price | Oscillator | Implication |
|------|-------|-----------|-------------|
| **Regular Bullish** | Lower Low | Higher Low | Selling momentum weakening → potential reversal up. |
| **Regular Bearish** | Higher High | Lower High | Buying momentum weakening → potential reversal down. |
| **Hidden Bullish** | Higher Low | Lower Low | Trend continuation up. |
| **Hidden Bearish** | Lower High | Higher High | Trend continuation down. |

---

## 2. Producing Indicators

Declared by 9 registry entries: `rsi`, `stochastic`, `chandemo`, `macd`, `obv`, `cmf`, `mfi`, `squeeze`, `oi_price_divergence`.

The eight momentum/volume oscillators run through a shared divergence engine using 20-bar peak/trough comparison; `oi_price_divergence` compares open interest against price.

---

## 3. Detection Semantics

1. Identify two consecutive price pivots (swing highs or lows) over the lookback window.
2. Compare the oscillator values at the same pivot timestamps.
3. If price and oscillator slopes disagree per the table in §1, a `Potential` divergence is emitted.
4. `direction` = `Bullish` or `Bearish`; `label` = e.g. `BULLISH_DIVERGENCE`.

---

## 4. Confirmation Lifecycle

```
Potential ──(candle close breaks nearest S/R by >0.2%)──► Confirmed
```

- **Bullish confirm:** close breaks below support then reverses back above with conviction (or aggressive close above recent swing high).
- **Bearish confirm:** close breaks above resistance then reverses back below (or aggressive close below recent swing low).

Until confirmed, a divergence is secondary confluence only. A `ConfirmedBullish` divergence can override an oscillator's normalized score to +1.0 (bearish → −1.0).

See [rsi.md §Divergence](../indicators/04-02-11-rsi.md) for the canonical worked example.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `Divergence` (regular/hidden, bull/bear via `label`). |
| Direction | Bullish / Bearish. |
| Confirmation | Potential → Confirmed. |
| Strength | Pivot separation × oscillator delta. |
| Freshness | `age_bars` since detection. |
| Priority | High — exhaustion signals gate reversals. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md) · [divergence indicator spec](../indicators/04-02-11-rsi.md)
- [Metrics Matrix §4](../../../matrices/02-07-metrics-matrix.md)
- [SignalKind: ZeroLineCross](05-02-06-zero-line-cross.md) — Companion momentum signal.
