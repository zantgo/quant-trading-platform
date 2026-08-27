# SignalKind: Breakout

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Structure
**Purpose:** Specification for the `Breakout` SignalKind — the event where price decisively breaches a structural boundary such as a channel edge, band, or horizontal level.

---

## 1. Definition

A **Breakout** fires when price closes beyond a defined structural boundary, signalling the potential start of a directional expansion.

| Example | Bullish | Bearish |
|---------|---------|---------|
| Donchian channel | close above upper band | close below lower band |
| Bollinger band | close above upper | close below lower |
| Support/Resistance | close above resistance | close below support |
| Ichimoku cloud | close above cloud | close below cloud |

---

## 2. Producing Indicators

Declared by 9 registry entries: `donchian`, `keltner`, `ichimoku`, `stddev_channel`, `volume_profile`, `bollinger`, `support_resistance`, `pivot_points`, `smc_structure`.

---

## 3. Detection Semantics

```
IF close > boundary_high + buffer → Bullish breakout
IF close < boundary_low  − buffer → Bearish breakout
```

A wick beyond the boundary is **not** a breakout — confirmation requires a candle **close** beyond the level plus a tolerance buffer (default 0.2% of price) to reject noise. This distinguishes a Breakout from a [BandTouch](05-02-05-band-touch.md).

### 3.1 The `breakout-confirmation` pattern

The named `breakout-confirmation` event is a Breakout whose `status` has upgraded to `Confirmed` after the closing candle clears the boundary by the buffer. Volume confirmation (RVOL ≥ institutional threshold) raises its strength.

---

## 4. Confirmation Lifecycle

```
Potential (wick/intrabar) ──(decisive close beyond boundary)──► Confirmed ──► Active (expansion)
```

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `Breakout` (via `label`, e.g. `DONCHIAN_BREAKOUT_UP`). |
| Direction | Bullish / Bearish. |
| Confirmation | Requires decisive close. |
| Strength | Break distance × volume confirmation. |
| Priority | High — breakouts often initiate opportunities. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [donchian.md](../indicators/04-02-03-donchian.md) · [bollinger.md](../indicators/04-02-26-bollinger.md) · [support_resistance.md](../indicators/04-02-32-support-resistance.md)
- [SignalKind: BandTouch](05-02-05-band-touch.md) — Contact without breach.
- [Opportunity Matrix](../../../matrices/02-08-opportunity-matrix.md) — Breakout setup consumer.
