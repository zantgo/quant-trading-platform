# Open Interest

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

Open Interest (OI) tracks the total number of outstanding derivative contracts (futures/perpetuals) that have not been settled. It represents the *flow of capital* into or out of the market — rising OI signals new money entering, falling OI signals exiting or liquidation.

The `OpenInterest` struct (`crates/market-analyzer/src/indicators/open_interest.rs`) maintains a rolling history of OI values (window: hardcoded `OI_DELTA_WINDOW_SECS = 3600` in `analyzer/mod.rs` — the `oi_lookback` config key is **not wired** and must not be relied on) and computes:

- **Raw current OI** — the latest observed open interest value.
- **Rolling average OI** — mean over the lookback window.
- **OI percentile** — where current OI ranks within its recent history (0–100).
- **OI delta (window)** — change from the oldest to newest value in the window.

OI is a **non-directional gate** by itself. Its meaning emerges when combined with price:

| Price + OI | Interpretation |
|------------|----------------|
| Price ⬆ + OI ⬆ | Strong uptrend — new capital confirming the move. |
| Price ⬆ + OI ⬇ | **Bullish** (AUDIT-AIU-007, canonical with 04-02-47): shorts closing into strength; the uptrend is intact (OI_PRIC-Divergence emits Bullish `OI_BULLISH_DIV`). |
| Price ⬇ + OI ⬆ | **Bearish** (AUDIT-AIU-007, canonical with 04-02-47): fresh shorts entering (OI-Price divergence emits Bearish `OI_BEARISH_DIV`). |
| Price ⬇ + OI ⬇ | Weakening downtrend — longs liquidating, shorts covering. |

> **Direction convention (AUDIT-AIU-007).** The OI-Price divergence direction is canonicalized on the MME indicator layer (`04-02-47`): price-up + OI-down = **Bullish**; price-down + OI-up = **Bearish**. This table previously implied the opposite; the liquidity-layer `OiPriceDivergence` signal now matches the indicator layer on the same snapshot.

## Standard Thresholds

| Zone | Condition | Interpretation |
|------|-----------|----------------|
| OI Elevated | OI > 1,000,000,000 (adjusted per asset) | High total open contracts — crowded trade, potential for cascading liquidations. |
| OI Percentile > 80 | Current OI in upper quintile of lookback | Elevated relative to recent history. |
| OI Percentile < 20 | Current OI in lower quintile | Depressed — market cooling. |

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | OI_ELEVATED | OI exceeds the elevated threshold (1B +) | Neutral |

## Normalization

OI is not directly normalized (non-directional gate). It provides context for the OI Delta and OI-Price Divergence indicators, which carry the actual directional signal.

---

## Cross-References

- [OI Delta](04-02-45-oi-delta.md) — The annualized change rate derived from OI.
- [OI-Price Divergence](04-02-47-oi-price-divergence.md) — Divergence between price trend and OI direction.
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md)
