# PME Layer 1 — Position Layer

**Version:** 11.0 (2026-08-26) — v7: PME is informational; this layer's math is unchanged.
**Status:** Specified — implemented (pure math); v7 surface wiring in progress.
**Engine:** Portfolio Management Engine (PME)
**Layer:** 1 of 4
**Input Contract:** Exchange execution events and order fill confirmations (from TAE)
**Output Contract:** Position Matrix (active position directory with live valuation metrics)
**Purpose:** This document specifies the Position Layer — the active exposure tracking system that monitors individual positions, updates mark-to-market valuations, and coordinates dynamic stop adjustments.

---

## 1. Purpose

The Position Layer is the PME's **active position tracker**. It receives execution fill events from the TAE and exchange, initializes position records, continuously updates mark-to-market valuations from live DIE price feeds, computes unrealized PnL and ROI, and manages dynamic stop-loss and take-profit levels.

```
[Exchange Fills] ──► POSITION LAYER (L1) ──► [Position Matrix] ──► [Exposure Layer (L2)]
```

---

## 2. Position Lifecycle

```
        fill confirmed
             │
             ▼
       ┌─────────────┐   mark-price updates    ┌─────────────┐
       │  OPEN        │───────────────────────►│  MANAGING    │
       └─────────────┘   (continuous)          └─────────────┘
             │                                        │
             │ fill cancelled / rejected              │ exit trigger (stop / target / signal)
             ▼                                        ▼
       ┌─────────────┐                          ┌─────────────┐
       │  REJECTED    │                          │  CLOSED     │
       └─────────────┘                          └─────────────┘
```

---

## 3. Position Matrix Schema

### 3.1 Core Position Fields

| Field | Type | Description |
|-------|------|-------------|
| `position_id` | `u64` | Unique position identifier. |
| `symbol` | `string` | Instrument (e.g., `BTC-USDT`). |
| `direction` | `Direction` | `Long` / `Short`. |
| `entry_price` | `Decimal` | Initial fill price. |
| `average_entry_price` | `Decimal` | Volume-weighted average entry (adjusts on scaled entries). |
| `size` | `Decimal` | Current base-asset quantity. |
| `allocated_usd` | `Decimal` | Notional value at entry. |
| `entry_timestamp` | `u64` | Unix epoch of first fill. |

### 3.2 Live Valuation Fields

| Field | Type | Description |
|-------|------|-------------|
| `current_price` | `Decimal` | Latest DIE mid-price for this symbol. |
| `unrealized_pnl` | `Decimal` | Live dollar profit/loss (before fees). |
| `roi_pct` | `Decimal` | Live percentage return. |
| `unrealized_pnl_after_fees` | `Decimal` | Live PnL with estimated exit fees deducted. |

### 3.3 Protection Fields

| Field | Type | Description |
|-------|------|-------------|
| `stop_loss_price` | `Decimal` | Current stop-loss trigger level. |
| `take_profit_price` | `Decimal` | Current take-profit target level. |
| `invalidation_level` | `Decimal` | Structural level whose breach nullifies the thesis (see §4.3). Canonical across L4 Opportunity Matrix, Decision Matrix, and this Position Matrix. *(Prior per-matrix spellings (L4/Decision and Position Matrix) unified to `invalidation_level` in v2.1 — retired names recorded in `docs/CHANGELOG.md`. The migration map is in [`02-00-matrix-field-ownership.md §2.4`](../../matrices/02-00-matrix-field-ownership.md).)* |
| `target_profit_ratio` | `Decimal` | Desired reward-to-risk ratio for this position. |

### 3.4 Scaled Entry Fields

| Field | Type | Description |
|-------|------|-------------|
| `current_portions` | `u32` | Number of filled position slots (1–4). |
| `initial_allocated_margin` | `Decimal` | Margin committed at first entry. |
| `realized_pnl_accumulator` | `Decimal` | PnL from partially closed portions. |

---

## 4. Dynamic Stop Management

The Position Layer reads updated invalidation levels from the MME [Decision Matrix](../../matrices/02-04-decision-matrix.md) as market structure evolves:

1. MME Decision Layer publishes updated `stop_loss_distance_pct` and `invalidation_level`.
2. Position Layer recomputes `stop_loss_price` from the new distance.
3. **Tighten-only rule (price-based, not distance-based).** The new stop is adopted **only if it is more favourable than the current stop on the price axis** — i.e. the stop is ratcheted **up for longs**, **down for shorts**, never the reverse. For a long position, a more favourable stop is one with a higher `stop_loss_price` (closer to or above entry); for a short position, a more favourable stop is one with a lower `stop_loss_price`. For a profitable long trade the stop ratchets upward (trailing the price), locking in unrealized gains. For a losing trade the stop either stays put or tightens in the favourable direction; it never widens. This guarantees stops only ever reduce risk distance.
4. Updated stop is routed to the exchange via the TAE Execution Layer.

> **Direction inversion note.** A previous version of step 3 stated the ratchet moved "down for longs, up for shorts" — this is **inverted** and would cause a developer implementing the rule to widen the stop on profitable longs. The corrected rule is: **up for longs, down for shorts** (toward the favourable side).

### 4.3 Thesis Invalidation (invalidation_level breach)

A close at or beyond `invalidation_level` on the active timeframe is treated as a **thesis-failure event**. The PME Position Layer issues a high-priority `LiquidateCommand` to the TAE Policy Layer (Hard Exit path, see [PME Layer 4 §4.2](./03-04-05-pme-layer4-overview.md)). The liquidation:

- Bypasses the Position Sizing Protocol (size is copied verbatim from the Position Matrix).
- Forces `reduce_only = true` and `is_emergency_liquidation = true` (v7: dispatched by the TAE stop-flatten path; there is no stance check anymore).
- Dispatches as a `Market` order to the exchange.

The breach is detected on candle close at the active timeframe (i.e. intrabar wicks through the level do not trigger the liquidation).

> **v7 note:** the veto Hard Exit was erased. The Position Layer's `LiquidateCommand` (invalidation-level breach) remains as the *reporting* signal; the TAE bracket SL stop is what actually closes the position (see [TAE Overview §6](../trade-automation-engine/03-03-01-tae-overview-spec.md)).

---

## 5. Scaled Entry Model (Position Slots)

Positions support up to 4 scaling slots via `position_slots`:

| Slot | Behaviour |
|------|-----------|
| Slot 1 | Initial entry — always filled first. |
| Slot 2–4 | Conditional scaling — triggered when price moves favourably by a configurable percentage. |
| Allocation curve | `Stepped` (equal portions), `Linear` (linearly increasing), or `Exponential` (aggressive scaling). |

Each slot's entry updates `average_entry_price` (volume-weighted) and `allocated_usd`.

---

## 6. Interaction with Capital & Exposure

- The Position Layer reports `allocated_usd` and `unrealized_pnl` to the [Capital Layer](03-04-04-pme-layer3-capital.md) for margin tracking.
- The Position Layer reports `direction × size` to the [Exposure Layer](03-04-03-pme-layer2-exposure.md) for concentration checks.

---

## 7. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Deterministic valuation** | Unrealized PnL is computed from `direction_sign × (current_price − average_entry_price) × size` where `average_entry_price` is the volume-weighted average across all filled slots (see §5) and `direction_sign = +1 (Long) | −1 (Short)`. For single-slot positions, `average_entry_price == entry_price` and the formula reduces to `direction_sign × (current_price − entry_price) × size`. The `direction_sign` multiplier is mandatory — omitting it would compute a profit for a short position when price rises (and a loss when price falls), corrupting margin calculations for all short trades. The same `direction_sign` convention applies to mark-to-market valuation, equity contribution, and `roi_pct` calculation; implementations MUST branch on `position.direction` rather than relying on sign-of-size. The same form applies to multi-slot positions: a formula reading the initial `entry_price` rather than the volume-weighted average misreports PnL after Scaled Entry slots 2–4 fire. |
| **Stop improvement only** | Dynamic stops only tighten; they never widen automatically. |
| **Partial-close tracking** | Realized PnL from partially closed portions is accumulated separately. |

---

## 8. Cross-References

- [PME Overview](../portfolio-management-engine/03-04-01-pme-overview-spec.md) — Engine boundaries and ledger model.
- [PME Layer 2 — Exposure](03-04-03-pme-layer2-exposure.md) — Concentration and net exposure.
- [PME Layer 3 — Capital](03-04-04-pme-layer3-capital.md) — Margin and equity tracking.
- [Decision Matrix](../../matrices/02-04-decision-matrix.md) — Invalidation level source.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `active_positions`, `position_slots`, `position_equity_snapshots`.
- [Ontology — Position](../../conceptual-foundations/01-01-ontology.md) — Conceptual definition.
