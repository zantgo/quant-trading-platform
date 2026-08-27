# SignalKind: StackChange

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Trend
**Purpose:** Specification for the `StackChange` SignalKind — the event where the EMA ribbon reorders, marking a shift in the multi-period moving-average alignment.

---

## 1. Definition

A **StackChange** fires when the ordering of the EMA ribbon's constituent averages changes — for example, transitioning from a bearish stack (`fast < medium < slow < long`) to a bullish stack (`fast > medium > slow > long`) or into a tangled (crossing) state.

| Stack state | Meaning |
|-------------|---------|
| Bullish stack | `fast > medium > slow > long` — clean uptrend alignment. |
| Bearish stack | `fast < medium < slow < long` — clean downtrend alignment. |
| Tangled | Averages interleaved — trend transition / no clear alignment. |

---

## 2. Producing Indicators

Declared by 1 registry entry: `ema_stack` (the EMA Ribbon). Default periods: `ema_fast=10`, `ema_medium=50`, `ema_slow=100`, `ema_long=200`.

---

## 3. Detection Semantics

```
prev_stack = ordering of {fast, medium, slow, long} last bar
curr_stack = ordering this bar
IF prev_stack != curr_stack → StackChange
   label = BULLISH_STACK | BEARISH_STACK | TANGLED
```

A transition into a fully aligned stack is a strong trend-confirmation; a transition into tangled warns of a weakening or transitioning trend. The `ema_stack_state()` accessor maps the ribbon to `bullish` / `bearish` / `tangled`.

---

## 4. Confirmation

StackChange is a **momentary** signal: it is `Confirmed` on the transition bar the ordering changes (`age_bars = 0`) and does not persist as `Active` — the resulting alignment is carried forward by the ribbon's `state_label`, not by an ageing signal. Alignment across timeframes (all TFs bullish-stacked) is a powerful multi-timeframe consensus input.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `StackChange` (via `label`). |
| Direction | Bullish / Bearish / Neutral (tangled). |
| Strength | Ribbon separation width. |
| Market Regime | Confirms TRENDING alignment. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [ema_stack.md](../indicators/04-02-01-ema-stack.md)
- [SignalKind: Crossover](05-02-02-crossover.md) — Individual EMA crosses.
- [MME Layer 2 — Alignment](../03-02-03-mme-layer2-alignment.md) — Cross-TF stack consensus.
