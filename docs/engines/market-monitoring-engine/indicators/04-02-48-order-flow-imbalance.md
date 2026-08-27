# Order Flow Imbalance (OFI)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

Order Flow Imbalance measures the net pressure at the top of the limit order book: is there more resting volume on the bid side (buyers) or ask side (sellers)? It is a leading directional signal — short-term imbalances often precede price moves.

Computed from the `OrderBookAnalysis` struct (`crates/market-analyzer/src/indicators/order_book.rs`):

```
bid_vol = Σ bid_size for top N depth levels
ask_vol = Σ ask_size for top N depth levels
total  = bid_vol + ask_vol
OFI    = (bid_vol - ask_vol) / total      ∈ [-1, 1]
```

- **OFI > 0** → more resting bids than asks → buy pressure, bullish.
- **OFI ≈ 0** → balanced book.
- **OFI < 0** → more resting asks than bids → sell pressure, bearish.

## Interpretation

| OFI Range | State Label | Interpretation |
|-----------|-------------|----------------|
| > 0.7 | `BULLISH_IMBALANCE` | Strong bid-side dominance — aggressive buying interest. |
| 0.0 … 0.7 | `BUY_PRESSURE` | Moderate buy pressure. |
| −0.7 … 0.0 | `SELL_PRESSURE` | Moderate sell pressure. |
| < −0.7 | `BEARISH_IMBALANCE` | Strong ask-side dominance — aggressive selling interest. |
| ≈ 0 | `BALANCED` | No directional pressure from the order book. |

Wall detection augments OFI: a `BID_WALL` or `ASK_WALL` label is pushed as a signal on the OFI entry when a single level's size exceeds `wall_threshold × total_side_volume`.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | BULLISH_IMBALANCE | OFI > 0.7 — aggressive bid-side pressure | Bullish |
| Threshold | BEARISH_IMBALANCE | OFI < −0.7 — aggressive ask-side pressure | Bearish |
| Threshold | BID_WALL / ASK_WALL | Large single-level size dominates the side (from wall detection) | Bullish / Bearish |

## Normalization

```
raw_value = OFI ∈ [-1, 1]
normalized = OFI (direct — the raw is already in the normalized range)
state_label = BULLISH_IMBALANCE | BEARISH_IMBALANCE | BUY_PRESSURE | SELL_PRESSURE | BALANCED
confidence = |OFI|
```

---

## Cross-References

- [Depth Bias](04-02-50-depth-bias.md) — Full depth ratio (not just top N).
- [Spread](04-02-49-spread.md) · `order_book.rs` — Order book analysis.
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md)
