# Mark-Index Spread

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

The **Mark-Index Spread** measures the percentage gap between the perpetual contract's mark price and the underlying index price:

```
mark_index_spread_pct = (mark_price - index_price) / index_price × 100
```

It is a cross-cuts indicator between derivatives telemetry and order-book microstructure. The mark price is the reference used for funding and liquidation calculations; the index price is the underlying spot reference. A divergence between them signals stress at the venue, can foreshadow funding flips, and is monitored by derivative risk systems.

## Data Source

Injected by `analyzer::inject_derivatives_indicators` from `latest_mark_px` / `latest_index_px` (both `Option<Decimal>` on the `MarketSnapshot`). Both fields are populated by the live derivatives WS layer (in-memory writers live — AUDIT-AIU-091) and may be `null` until the first WS mark push — the `mark_index_spread` row is then absent from the `indicators` map.

`mark_index_spread` is a **non-directional context gate** (AUDIT-AIU-024) — `normalized` is contractually `0.0` and its Threshold signal is Neutral; it provides venue-stress context but does not vote on bullish/bearish direction.

## Interpretation

| Spread % | Condition | Meaning |
|----------|-----------|---------|
| < 0.05% | Tight | Mark and index are aligned — venue is quoting firmly. |
| 0.05–0.15% | Normal | Routine noise; nothing actionable. |
| > 0.15% | Diverged | Mark is trading off peg — watch for funding flips and cascading liquidations. |
| > 0.30% | Stressed | Venue dislocation — avoid market orders; expect insurance-fund draws. |

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | MARK_INDEX_DIVERGED | |\(mark_index_spread_pct\)| exceeds the stressed threshold (configurable, default ~0.30%) | Neutral (gate only) |

Strength scales with the spread magnitude: `strength = min(|spread_pct| / 1.0, 1.0)`.

## Normalization

```
raw_value = mark_index_spread percentage (signed; positive = mark above index)
normalized = 0.0 (non-directional gate)
state_label = MARK_INDEX_DIVERGED | ALIGNED
```

## Registry

```
key: "mark_index_spread"
display_name: "Mark-Index Spread"
group: DerivativesData
class: Hybrid
render: Pane
data_source: DerivativesWs
normalization_mode: ContextOnly     # Norm column renders "N/A"
value_format: "percent1"
value_source: "raw"
bars_required: 1
```

---

## Cross-References

- [Open Interest](04-02-44-open-interest.md) · [OI Delta](04-02-45-oi-delta.md) · [Funding Rate](04-02-46-funding-rate.md) — Companion derivatives-data indicators.
- [Liquidity Domain](../../../conceptual-foundations/01-05-liquidity-domain.md) — Layer 1.5 derivatives telemetry.
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md)
