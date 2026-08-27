# Funding Rate

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

The **Funding Rate** is the periodic payment exchanged between long and short perpetual swap traders to tether the perpetual price to the underlying spot price. When the perpetual trades at a premium to spot, longs pay shorts (positive funding); when at a discount, shorts pay longs (negative funding).

**Implementation (live-state writer, not a candle-based calculator).** The reading is injected by the analyzer's live-state writers from the latest WS funding push (`latest_funding`), shared across timeframes; a bounded rolling history (`funding_history`, last 8 pushes) provides the previous-rate prior for the funding-flip detection. The `FundingRate` tracker (`crates/market-analyzer/src/indicators/funding.rs`) exposes:

- **Raw funding rate** — the per-8-hour rate as a decimal (e.g. `0.0001` = 0.01%).
- **Annualized percentage** — raw rate × 1095 (3 periods/day × 365 days) × 100.

```
annualized_pct = raw_rate × 1095 × 100
```

Funding rate is a **non-directional gate** — `normalized = 0.0` by contract (it never contributes a scored direction):

| Funding | Sentiment | Risk |
|---------|-----------|------|
| High positive (> `funding_extreme_pct`, default `0.0005`) | Crowded longs — everyone bullish. | Elevated reversal risk. |
| High negative (< `−funding_extreme_pct`) | Crowded shorts — everyone bearish. | Elevated squeeze risk. |
| Neutral | Balanced positioning. | Normal. |

## Standard Thresholds

| Zone | Raw Rate | Meaning |
|------|----------|---------|
| Extreme positive | `> funding_extreme_pct` (config `[liquidity] funding_extreme_pct`, default `0.0005` = 0.05%/8h) | Overcrowded longs — bearish contrarian signal. |
| Extreme negative | `< −funding_extreme_pct` | Overcrowded shorts — bullish contrarian signal. |
| Label bands | `> +0.005` → `FUNDING_HIGH_LONG_PAY`; `< −0.005` → `FUNDING_HIGH_SHORT_PAY` | Fixed ±0.005 state-label bands (independent of the extreme threshold). |

The extreme threshold is the configured `funding_extreme_pct` (threaded from `LiquidityConfig`); the `±0.005` label bands are hardcoded.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | FUNDING_EXTREME | `|raw_rate| > funding_extreme_pct` (default `0.0005`) — funding is at a sentiment extreme | Bearish (positive extreme — overcrowding longs) / Bullish (negative extreme — overcrowding shorts) |

The direction is contrarian: extreme positive funding is `Bearish` (longs may get flushed), extreme negative is `Bullish` (shorts may get squeezed).

## Normalization

```
raw_value = raw funding rate (per-8h decimal)
normalized = 0.0 (non-directional gate — does not contribute scored direction)
values.annualized_pct = raw × 1095 × 100  — emitted ONLY when |f| > funding_extreme_pct, omitted otherwise
state_label = FUNDING_HIGH_LONG_PAY  (f > +0.005)
            | FUNDING_HIGH_SHORT_PAY (f < −0.005)
            | FUNDING_NEUTRAL        (|f| < 1e-6)
            | FUNDING_NORMAL         (otherwise)
```

---

## Cross-References

- [Open Interest](04-02-44-open-interest.md) · [OI Delta](04-02-45-oi-delta.md) — Companion derivatives indicators.
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md)
