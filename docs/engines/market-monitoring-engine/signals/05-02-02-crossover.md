# SignalKind: Crossover

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Momentum / Trend transition
**Purpose:** Specification for the `Crossover` SignalKind — the event where two related series cross each other, marking a momentum or trend transition.

---

## 1. Definition

A **Crossover** fires on the bar where one series moves from one side of another to the opposite side. It is a state-transition signal: it requires the `PreviousBarState` to detect the crossing.

| Example | Bullish | Bearish |
|---------|---------|---------|
| MACD line × signal | line crosses above signal | line crosses below signal |
| Stochastic %K × %D | %K crosses above %D | %K crosses below %D |
| Price × Supertrend | close crosses above band | close crosses below band |

---

## 2. Producing Indicators

Declared by **9** registry entries: `ema_stack`, `supertrend`, `anchored_vwap`, `ichimoku`, `stochastic`, `hull_ma`, `macd`, `pivot_points`, `psar`.

> **Note.** `aroon` was previously listed here but has been reclassified — Aroon's Up/Down crossing is emitted as a `TrendFlip` (see [05-02-09-trend-flip.md](05-02-09-trend-flip.md)), not a `Crossover`, because it represents a directional regime change rather than a generic two-series cross. Likewise the DI+ × DI− crossover is emitted by `adx` as a `TrendFlip` (see [04-02-05-adx.md](../indicators/04-02-05-adx.md)).

---

## 3. Detection Semantics

```
prev_a, prev_b = PreviousBarState
curr_a, curr_b = current bar
IF prev_a ≤ prev_b AND curr_a > curr_b → Bullish crossover
IF prev_a ≥ prev_b AND curr_a < curr_b → Bearish crossover
```

Crossovers are emitted on the **transition bar only** — the signal is fresh (`age_bars = 0`) exactly once, then the resulting state persists as the indicator's `state_label`.

---

## 4. Confirmation

Crossovers are typically emitted as `Confirmed` on candle close (no intrabar flicker enters completed snapshots). Strength scales with the magnitude of separation after the cross.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `Crossover` (via `label`, e.g. `MACD_BULLISH_CROSSOVER`). |
| Direction | Bullish / Bearish. |
| Confirmation | Confirmed on close. |
| Freshness | `age_bars = 0` on the transition bar. |
| Priority | Medium–High depending on the indicator's weight. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [macd.md](../indicators/04-02-17-macd.md) · [stochastic.md](../indicators/04-02-12-stochastic.md) · [ema_stack.md](../indicators/04-02-01-ema-stack.md)
- [SignalKind: StackChange](05-02-11-stack-change.md) — Multi-line EMA reordering.
