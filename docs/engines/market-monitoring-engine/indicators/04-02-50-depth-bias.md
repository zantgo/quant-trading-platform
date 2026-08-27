# Depth Bias

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

Depth Bias measures the bid-side vs ask-side depth ratio across the **full sampled order book** (all depth levels, not just the top N). While Order Flow Imbalance (OFI) captures near-term pressure at the top of the book, Depth Bias captures the broader structural support/resistance in the limit order book.

Computed from the `OrderBookAnalysis` struct (`crates/market-analyzer/src/indicators/order_book.rs`):

```
cum_bid = Σ bid_size across all sampled depth levels
cum_ask = Σ ask_size across all sampled depth levels
ratio   = cum_bid / cum_ask
```

Normalization uses the ratio:

$$\text{normalized} = \text{clamp}\left(\frac{\text{ratio} - 1}{\text{ratio} + 1},\ -1,\ 1\right)$$

## Interpretation

| Ratio | Normalized | Label | Interpretation |
|-------|-----------|-------|----------------|
| > 2.0 | > 0.33 | `DEEP_BIDS` | Strong cumulative bid depth — structural buying support. |
| 1.5 … 2.0 | 0.20 … 0.33 | `DEEP_BIDS` | Moderate bid-side depth advantage. |
| 0.67 … 1.5 | −0.20 … 0.20 | `BALANCED_DEPTH` | Roughly symmetric depth. |
| 0.5 … 0.67 | −0.33 … −0.20 | `DEEP_ASKS` | Moderate ask-side depth advantage. |
| < 0.5 | < −0.33 | `DEEP_ASKS` | Strong cumulative ask depth — structural selling pressure. |

## Relationship with OFI

| OFI | Depth Bias | Interpretation |
|-----|-----------|----------------|
| Bullish + Deep bids | Alignment — near and far book agree bullish. |
| Bullish + Deep asks | Conflict — near-term buying but structural sell wall ahead. |
| Bearish + Deep bids | Conflict — near-term selling but structural buy wall beneath. |
| Bearish + Deep asks | Alignment — near and far book agree bearish. |

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | BID_DEPTH_SURGE | Ratio > 2.0 — bid depth ≥ 2× ask depth | Bullish |
| Threshold | ASK_DEPTH_SURGE | Ratio < 0.5 — ask depth ≥ 2× bid depth | Bearish |

Strength scales with the normalized value: `strength = |normalized|`.

## Normalization

```
raw_value = bid_depth / ask_depth ratio
normalized = clamp((ratio - 1) / (ratio + 1), -1, 1)
state_label = DEEP_BIDS | DEEP_ASKS | BALANCED_DEPTH
confidence = |normalized|
```

## Risk Integration (depth-bias-specific)

The `RiskMatrix.execution_liquidity_risk` dimension incorporates Depth Bias in addition to RVOL and spread:

| Condition | Effect on `execution_liquidity_risk` |
|-----------|--------------------------------------|
| Depth Bias ratio < 0.5 (`DEEP_ASKS`, strong ask wall) | +15 (large resting sell orders → harder to lift for long entries) |
| Depth Bias ratio > 2.0 (`DEEP_BIDS`, strong bid wall) | -10 (large resting buy orders → easier to fill long entries, structural support beneath) |
| Ratio ∈ [0.5, 2.0] (`BALANCED_DEPTH`) | 0 (no adjustment) |

The Depth Bias adjustment is **distinct from** the spread-based adjustment in `execution_risk` (which captures per-fill cost) and the RVOL-based adjustment in `execution_liquidity_risk` (which captures participation regime). Depth Bias captures **structural book support/resistance** — large resting orders that may act as price barriers.

---

## Cross-References

- [Order Flow Imbalance](04-02-48-order-flow-imbalance.md) — Top-N book imbalance.
- [Spread](04-02-49-spread.md) — Liquidity signal.
- [Risk Matrix — Liquidity Risk](../../../matrices/02-11-risk-matrix.md)
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md)
