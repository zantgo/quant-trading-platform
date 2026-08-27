# Execution Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Trade Automation Engine (TAE)
**Producing Layer:** Layer 2 — Execution Layer
**Purpose:** This document defines the physical schema of the **Execution Matrix** — the persistent log of all order states produced by the TAE Execution Layer. It tracks every order from construction through exchange acknowledgement to final fill/cancel/rejection, guaranteeing full auditability.

---

## 1. Conceptual Definition

Per the [Ontology](../conceptual-foundations/01-01-ontology.md) §3.15, the **Execution Layer** is the platform's order routing authority. It translates validated Policy Layer directives into exchange orders, manages the order state machine, executes the Position Sizing Protocol, and maintains the Execution Matrix as its output contract.

Unlike MME matrices (which are JSON DTOs broadcast over WebSocket), the Execution Matrix is **materialized as the `open_orders` SQLite table** (see [Database Schema §3.2](../integration-and-api/06-02-database-schema-spec.md)). It is the single canonical log of every order lifecycle transition from `PENDING` onwards.

```
[Policy Matrix] ──► EXECUTION LAYER (L2) ──► [Exchange / Paper Engine]
                           │
                           └──(reads Capital Matrix)──► [PME]
                           │
                           ▼
                   [Execution Matrix] → `open_orders` (SQLite)
```

---

## 2. Schema

| Field | Type | Description |
|-------|------|-------------|
| `order_id` | `string` | Exchange-assigned order ID. |
| `client_order_id` | `string` | Idempotency key — prevents duplicate submission on retry. |
| `symbol` | `string` | Instrument. |
| `order_type` | `string` | `MARKET` / `LIMIT` / `STOP`. |
| `direction` | `string` | `BUY` / `SELL`. |
| `price` | `Decimal` | Order price (limit/stop). |
| `trigger_price` | `Decimal` | Trigger price (stop orders). |
| `size` | `Decimal` | Base-asset quantity. |
| `filled_size` | `Decimal` | Cumulative filled quantity. |
| `status` | `string` | `PENDING` / `SUBMITTED` / `OPEN` / `PARTIALLY_FILLED` / `CLOSED` / `REJECTED` / `CANCELLED`. |
| `is_reduce_only` | `bool` | Reduce-only flag. Forces exposure reduction only; non-configurable under `CLOSE_ONLY` stance (see [TAE Layer 2 §3.3](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md)). |
| `is_emergency_liquidation` | `bool` | Whether this order is part of the Hard Exit path. `true` ⇒ dispatched by PME Veto, bypasses pre-trade gates. |
| `associated_position_id` | `u64` | Linked position. |
| `created_at` | `u64` | Unix epoch timestamp of order creation. |
| `updated_at` | `u64` | Unix epoch timestamp of last state transition. |
| `slippage_bps` | `f64` | Fill slippage in basis points. |

---

## 3. Order State Machine

Every order transitions through the logged lifecycle defined in [TAE Layer 2 §4](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md):

```
PENDING → SUBMITTED → OPEN → (PARTIALLY_FILLED) → CLOSED / CANCELLED / REJECTED
```

The `PRE_DISPATCH` state (Gate 5 manual-review hold) is in-memory only — it is **never written** to the Execution Matrix or the `open_orders` table. Every persistent transition from `PENDING` onwards is recorded with a high-resolution timestamp.

---

## 4. Production Rules

The Execution Matrix is produced by the TAE Execution Layer on every order state transition:

- **New entries:** sized via the Position Sizing Protocol (`S = E·R / (D_sl / 100)`) from available margin and stop distance.
- **Exits / reduce-only:** size copied directly from the Position Matrix (bypasses sizing formula — see [TAE Layer 2 §3.5](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md)).
- **Hard Exit path:** orders tagged `is_emergency_liquidation = true`, bypass pre-trade gates, size copied from Position Matrix.
- **State transitions:** every ack, fill, cancel, or rejection updates `status`, `filled_size`, `updated_at`, and `slippage_bps`.

---

## 5. Consumption

| Consumer | Module | Usage |
|----------|--------|-------|
| **PME Position Layer** | `03-04-02-pme-layer1-position.md` | Fill events update `active_positions` and `position_slots`. |
| **PME Capital Layer** | `03-04-04-pme-layer3-capital.md` | Fills debit/credit the capital ledger. |
| **PAE Trade Analytics** | `03-05-02-pae-layer1-trade-analytics.md` | Closed fills feed trade reconstruction. |
| **Audit / Replay** | `06-02-database-schema-spec.md §3.2` | `open_orders` table is the canonical replay source. |

---

## 6. Materialization

The Execution Matrix is the **`open_orders`** SQLite table. The DB column names mirror the schema fields above, with the addition of persistence-specific columns (`id INTEGER PRIMARY KEY`, `exchange_order_id TEXT`). The canonical DDL is at [Database Schema §3.2](../integration-and-api/06-02-database-schema-spec.md).

The vocabulary is unified: all `status` values match the Execution Matrix lifecycle (verified by `DOCS-CONSISTENCY-MANIFEST.md` §G10).

---

## 7. Cross-References

- [TAE Layer 2 — Execution Layer](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md) — Producing-layer specification (position sizing, order construction, state machine, reduce-only handoff).
- [TAE Overview](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md) — Upstream setup source.
- [Database Schema §3.2 `open_orders`](../integration-and-api/06-02-database-schema-spec.md) — Persistence contract.
- [Matrix Field Ownership](02-00-matrix-field-ownership.md) — Canonical per-field ownership mapping.
- [Systemic Data Flow — Sequence B](../conceptual-foundations/01-03-systemic-data-flow.md) — Entry loop.
