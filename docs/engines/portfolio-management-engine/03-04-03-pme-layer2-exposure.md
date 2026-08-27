# PME Layer 2 — Exposure Layer

**Version:** 11.0 (2026-08-26) — v7: PME is informational; this layer's math is unchanged.
**Status:** Specified — implemented (pure math); v7 surface wiring in progress.
**Engine:** Portfolio Management Engine (PME)
**Layer:** 2 of 4
**Input Contract:** Position Matrix (L1)
**Output Contract:** Exposure Matrix (aggregated risk allocation and concentration metrics)
**Purpose:** This document specifies the Exposure Layer — the concentration and directional-risk aggregation system that groups active positions across correlated boundaries to prevent overexposure to specific market vectors.

---

> **v7.3 — config-driven limits.** The concentration caps (`max_single_pair_exposure_pct = 20`, `max_portfolio_exposure_pct = 50`, `max_correlation = 0.8`) are no longer hardcoded constants: they come from `[workspace.risk_limits]` in config.toml (`exposure_layer::ConcentrationLimits::from_config`). The PME Exposure tab renders the same numbers the backend enforces, including a BREACH warning when a pair crosses its cap. The `GET /api/instances/:id/exposure` and `GET /api/instances/:id/portfolio` responses carry a `limits` block with the enforced values.

## 1. Purpose

The Exposure Layer prevents **concentration risk** — the danger of excessive capital allocation to a single asset, correlated group, or directional vector. It consumes the active Position Matrix from Layer 1 and aggregates positions into sector-level, asset-level, and directional exposure metrics.

```
[Position Matrix] ──► EXPOSURE LAYER (L2) ──► [Exposure Matrix] ──► [Capital Layer (L3)]
                                                                 └──► [Overview Layer (L4)]
```

---

## 2. Exposure Matrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `gross_exposure` | `Decimal` | Total absolute notional value of all positions. |
| `net_exposure` | `Decimal` | Long notional − Short notional. |
| `net_exposure_pct` | `Decimal` | Net exposure as percentage of total equity. |
| `long_exposure` | `Decimal` | Total long notional. |
| `short_exposure` | `Decimal` | Total short notional. |
| `symbol_concentration` | `map<string, Decimal>` | Per-symbol exposure as percentage of equity. |
| `sector_concentration` | `map<string, Decimal>` | Per-sector (correlated group) exposure. |
| `max_single_pair_pct` | `Decimal` | Highest single-symbol allocation percentage. |
| `correlation_matrix` | `CorrelationMap` | Cross-symbol correlation coefficients. |

---

## 3. Concentration Limits

Per [PME Overview](../portfolio-management-engine/03-04-01-pme-overview-spec.md) §3:

| Limit | Default | Enforced By |
|-------|---------|-------------|
| Max single-pair exposure | 20% of capital | Exposure Layer — blocks new entries exceeding this. |
| Max portfolio exposure | 50% of capital | Exposure Layer — total long + short capped. |
| Max correlation threshold | 0.8 | Exposure Layer — blocks correlated entries above this. |

When a limit is approached:
1. New position requests that would breach the limit are rejected pre-trade at Gate 6 (see [08-02-pre-trade-risk-controls.md](../../operations-and-compliance/08-02-pre-trade-risk-controls.md)).
2. The Overview Layer (L4) is notified.
3. v7: concentration limits are **informational** (displayed on the Exposure panel); nothing is flagged, reduced, or blocked on breach.

---

## 4. Directional Netting

The Exposure Layer computes net directional bias:

$$\text{net\_exposure} = \sum \text{long\_notional} - \sum \text{short\_notional}$$

$$\text{net\_exposure\_pct} = \frac{\text{net\_exposure}}{\text{total\_equity}} \times 100$$

This is used by the Overview Layer (L4) to detect when the portfolio becomes excessively directional, which may trigger a stance adjustment.

---

## 5. Sector / Correlation Grouping

Assets are grouped into correlation sectors based on historical price co-movement:

| Sector | Example Assets |
|--------|---------------|
| Base Chain | BTC, SOL, AVAX |
| L2 Protocols | ARB, OP, MATIC |
| DeFi | UNI, AAVE, MKR |
| Meme | DOGE, SHIB, PEPE |

The `CorrelationMap` is updated periodically from historical price data. New correlations above 0.8 trigger a warning.

---

## 6. Interaction with Capital Layer

The Exposure Layer reports:
- `gross_exposure` → feeds into leverage ratio calculation in Capital Layer.
- `net_exposure` → used for directional risk assessment in Overview Layer.

---

## 7. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Real-time limits** | Concentration checks run on every position update (no polling delay). |
| **Deterministic grouping** | Sector assignments are deterministic per symbol. |
| **Upstream isolation** | The Exposure Layer never modifies positions — it only aggregates and warns. |

---

## 8. Cross-References

- [PME Overview](../portfolio-management-engine/03-04-01-pme-overview-spec.md) — Engine boundaries and leverage restrictions.
- [PME Layer 1 — Position](03-04-02-pme-layer1-position.md) — Upstream data source.
- [PME Layer 3 — Capital](03-04-04-pme-layer3-capital.md) — Leverage calculation consumer.
- [PME Layer 4 — Overview](03-04-05-pme-layer4-overview.md) — Veto trigger consumer.
- [Ontology — Portfolio Management](../../conceptual-foundations/01-01-ontology.md) — Conceptual definitions.
