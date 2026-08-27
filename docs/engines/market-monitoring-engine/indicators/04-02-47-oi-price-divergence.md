# OI-Price Divergence

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

OI-Price Divergence detects disagreement between **price trend direction** (via the EMA ribbon bias) and **open interest flow** (via OI Delta). When price moves one way but capital (OI) moves the other, the trend lacks confirmation and is vulnerable to reversal.

Computed in the analyzer pipeline (`analyzer/mod.rs`) on every snapshot:

```
ema_bias = normalized EMA ribbon score ∈ [-1, 1]
oi_delta_1h = current OI - OI from 1 hour ago

IF oi_delta > 0 AND ema_bias < -0.3:
    divergence = -0.7 → Bearish OI-Price divergence (price weak, OI rising)
ELIF oi_delta < 0 AND ema_bias > 0.3:
    divergence = +0.7 → Bullish OI-Price divergence (price strong, OI falling)
ELSE:
    divergence = 0.0 → aligned
```

## Interpretation

| Condition | State | Meaning |
|-----------|-------|---------|
| Price ⬆ + OI ⬆ | `OI_PRICE_ALIGNED` | Clean trend — capital confirming price. |
| Price ⬇ + OI ⬇ | `OI_PRICE_ALIGNED` | Clean decline — positions liquidating. |
| Price ⬆ + OI ⬇ | `OI_BULLISH_DIV` | Bullish divergence — price strong despite capital outflow. May signal short covering rather than organic buying. |
| Price ⬇ + OI ⬆ | `OI_BEARISH_DIV` | Bearish divergence — price weak despite capital inflow. May signal distribution into strength. |

OI-Price Divergence is the derivatives equivalent of price/oscillator divergence — it measures the gap between what *price* says and what *capital commitment* says.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Divergence | OI_PRICE_DIVERGENCE | `|divergence| > 0.3` — OI and EMA ribbon bias disagree directionally | Bullish (OI falling + price bullish) / Bearish (OI rising + price bearish) |

The divergence fires as `Active` when the threshold is crossed — it functions as a structural warning, not a momentary signal.

## Normalization

```
normalized = divergence value (−0.7 / 0.0 / +0.7)
state_label = OI_BULLISH_DIV | OI_BEARISH_DIV | OI_PRICE_ALIGNED
```

---

## Cross-References

- [Open Interest](04-02-44-open-interest.md) — Raw OI tracker.
- [OI Delta](04-02-45-oi-delta.md) — The capital-flow signal.
- [EMA Stack](04-02-01-ema-stack.md) — EMA ribbon providing price-bias reference.
- [Signals Guide — Divergence](../signals/05-02-01-divergence.md)
