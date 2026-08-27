# PME Layer 3 — Capital Layer

**Version:** 11.0 (2026-08-26) — v7: PME is informational; this layer's math is unchanged.
**Status:** Specified — implemented (pure math); v7 surface wiring in progress.
**Engine:** Portfolio Management Engine (PME)
**Layer:** 3 of 4
**Input Contract:** Position Matrix (L1), Exposure Matrix (L2), exchange balance events
**Output Contract:** Capital Matrix (high-frequency balance sheet of active and available capital)
**Purpose:** This document specifies the Capital Layer — the account solvency monitor that tracks balances, margin usage, leverage ratios, and equity curves.

---

## 1. Purpose

The Capital Layer is the PME's **balance-sheet authority**. It holds the definitive ledger of the trading account's financial state: available balances, committed margin, realized PnL, and equity curves. It is the sole source queried by the TAE [Execution Layer](../trade-automation-engine/03-03-03-tae-layer2-execution.md) for the Position Sizing Protocol.

```
[Position Matrix] ─┐
                   ├──► CAPITAL LAYER (L3) ──► [Capital Matrix] ──► [TAE sizing]
[Exposure Matrix] ─┘                                               └──► [Overview Layer (L4)]
```

---

## 2. Capital Matrix Schema

> **Target Architecture (Not Yet Implemented) / Cold-Path Invariant.** All capital, margin, balance, fee, and equity fields are maintained strictly as 128-bit arbitrary-precision decimals (`rust_decimal::Decimal`) to prevent rounding errors and guarantee penny-perfect cross-asset accounting. This is the OOP/Domain-Driven **cold path**; no `f64` intermediary is permitted in the ledger. The base capital supplied to the TAE Position Sizing Protocol is **`available_margin`** (free buying power), never `current_equity` (which includes unrealized PnL). *Note:* the schema fields below already use `Decimal`; the outstanding gap is that the downstream TAE sizing calculator still consumes these values as `f64` (see [TAE Layer 2 §2.1](../trade-automation-engine/03-03-03-tae-layer2-execution.md)).

### 2.1 Core Balance Fields

| Field | Type | Description |
|-------|------|-------------|
| `initial_balance` | `Decimal` | Starting capital at session initiation. |
| `current_equity` | `Decimal` | `initial_balance + realized_pnl + unrealized_pnl` (**canonical**, single definition). |
| `available_margin` | `Decimal` | Liquid capital available for new position initiation (formula in §4.2). |
| `committed_margin` | `Decimal` | Total margin locked by active positions. |
| `realized_pnl` | `Decimal` | Cumulative PnL from closed trades. **Net of fees and funding** — fees and funding costs are deducted at the fill level, never separately. |
| `unrealized_pnl` | `Decimal` | Aggregate unrealized PnL from all active positions. |

> **Persistence mapping (v2.1).** The in-memory Capital Matrix fields above map to the persistent `paper_balances` table ([06-02-database-schema-spec.md §3.2](../../integration-and-api/06-02-database-schema-spec.md)) as follows:
>
> | Capital Matrix field | `paper_balances` column |
> |----------------------|-------------------------|
> | `initial_balance` | `initial_balance` |
> | `balance` | `balance` (current; the liquid cash balance tracked by the capital ledger) |
> | `committed_margin`, `unrealized_pnl`, `available_margin` | **derived metrics — not persisted**. Computed on demand from `active_positions` and `open_orders` (see §4.2 and the database spec §3.4 preamble). The startup recovery process recomputes them from the persisted `active_positions` and `open_orders` rows. |

### 2.2 Risk Metrics

| Field | Type | Description |
|-------|------|-------------|
| `margin_usage_ratio` | `Decimal` | Fraction of total equity committed to maintenance/initial margin, in `[0, 1]`. Multiply by 100 for human-readable display. |
| `leverage_ratio` | `Decimal` | `gross_exposure / current_equity` (fraction, `[0, ∞)`). |
| `max_daily_drawdown_pct` | `Decimal` | **Configuration limit** — the operator-set early-warning threshold (default 0.05 i.e. 5%). Distinguished from the live metric `daily_drawdown_pct` computed at runtime. Triggers `safety_state = WARN` (no stance change) when the live metric crosses the configured limit; see [03-04-05-pme-layer4-overview.md §3](../portfolio-management-engine/03-04-05-pme-layer4-overview.md). |
| `daily_pnl` | `Decimal` | Equity change since session start (the **live metric**; corresponds to the older name `current_daily_pnl`). Used for WARN evaluation as `daily_drawdown_pct = -daily_pnl / starting_session_equity`. |
| `starting_session_equity` | `Decimal` | Equity recorded at the most recent session-reset boundary (operator-defined `session_reset_cron`, default `00:00 UTC`). On session reset, set to the current `current_equity` value. Persisted across restarts. |

---

## 3. Equity Tracking

The Capital Layer maintains a continuous equity time-series:

$$\text{current\_equity} = \text{initial\_balance} + \text{realized\_pnl} + \text{unrealized\_pnl}$$

> **Canonical convention.** `realized_pnl` is **net of fees and funding** (fees are deducted at the fill level, never separately). This is the **single canonical** equity formula used everywhere in the corpus; do **not** introduce alternate forms with explicit `-fees` terms, which would double-count.

Equity snapshots are persisted every 60 seconds to `portfolio_equity_history` for drawdown analysis and performance metrics (see [PAE Layer 3 — Risk Analytics](../performance-analytics-engine/03-05-04-pae-layer3-risk-analytics.md)).

---

## 4. Margin Model

### 4.1 Cross Margin (Default)

All positions share a single margin pool. The engine uses **cross leverage** (default: 20×, configurable via `config.toml` `[leverage.cross_leverage]`). All quantities in the margin model are `rust_decimal::Decimal`.

$$\text{margin\_required} = \frac{\text{position\_notional}}{\text{cross\_leverage}}$$

$$\text{margin\_usage\_ratio} = \frac{\text{committed\_margin}}{\text{current\_equity}}$$

### 4.2 Available Margin

$$\text{available\_margin} = (\text{initial\_balance} + \text{realized\_pnl} + \min(0,\ \text{unrealized\_pnl})) - \text{committed\_margin}$$

This is the canonical definition: **spendable buying power** includes realized gains/losses (via `realized_pnl`, already net of fees) and unrealized **losses** (via `min(0, unrealized_pnl)`) but **excludes** unrealized gains (which are not yet realised cash). Three protections are baked into this formula:

1. **No phantom buying power from unrealized gains.** Adding only `min(0, unrealized_pnl)` (not the full `unrealized_pnl`) prevents the TAE from sizing new positions against floating gains that have not yet become cash.
2. **No over-commitment during floating drawdowns.** `min(0, unrealized_pnl)` is negative during losses and shrinks `available_margin`, so a drawdown reduces spendable buying power — preventing the system from opening new positions that compound the loss.
3. **Fees already netted.** Fees are deducted at the fill level (via `realized_pnl`); this formula does not subtract them again.

This `available_margin` value is the `E` (available margin) supplied to the TAE Position Sizing Protocol. Note that it is **not** `current_equity`, which includes the full `unrealized_pnl` (gains and losses) and is not spendable buying power in the gain case.

#### 4.2.1 Notation Summary (Equivalence Check)

For downstream readers, the sizing-protocol `E` field **must never** be substituted with `current_equity`:

| Symbol | Includes unrealized PnL? | Suitable as sizing `E`? |
|---|---|---|
| `current_equity` | **yes (full)** | **no** — floating gains are not spendable buying power |
| `initial_balance` | n/a | yes, but ignores realized PnL |
| `initial_balance + realized_pnl` | **no** | partial — ignores drawdowns |
| `initial_balance + realized_pnl + min(0, unrealized_pnl) − committed_margin` | **losses only** | **yes — this is `available_margin`** |

All definitions of `available_margin` across the corpus (this file §4.2, `03-03-03-tae-layer2-execution.md` §2, `01-02-global-architecture.md` §2.3, `01-03-systemic-data-flow.md` Sequence B, `08-02-pre-trade-risk-controls.md` Gate 3, `01-01-ontology.md` §3.21) refer to the same field computed via this formula.

---

## 5. Fee Tracking

| Fee Type | Tracking |
|----------|----------|
| **Trading fees** | Deducted from realized PnL on each fill (maker/taker). |
| **Funding payments** | Accrued on receipt of each `FundingRate` event from the DIE; for venues publishing every 8 hours (`funding_rate_8h`), each event is recorded as a discrete accrual. The 8-hour cadence is a **venue property**, not an internal cron. |
| **Spread cost** | Implicit cost captured by comparing fill price to mid-price; logged to `execution_slippage`. |

All fees are configurable via `config.toml` `[fees]`.

---

## 6. Liquidation Risk Monitoring

The Capital Layer continuously monitors `margin_usage_ratio` (fraction in `[0, 1]`):

| Threshold | Action |
|-----------|--------|
| `margin_usage_ratio ≥ 0.80` | Warning published to Overview Layer (L4). |
| `margin_usage_ratio ≥ 0.95` | Alert: automatic `CLOSE_ONLY` stance for all symbols (see PME Layer 4 §4.1 for trigger-to-stance mapping). |
| `margin_usage_ratio ≥ 1.00` | Potential liquidation — emergency position reduction (PME Veto triggers `AVOID` stance + Hard Exit path). |

---

## 7. Interaction with TAE

The Capital Layer is the **single source of truth** for the TAE Position Sizing Protocol. On every sizing event:

1. TAE Execution Layer sends a synchronous request: `query_available_margin(symbol)`.
2. Capital Layer responds with `available_margin`. The query is **read-only** — no reservation is taken at query time. Margin is committed only when the order passes Gate 3 at dispatch.
3. TAE computes $\text{notional} = \text{equity} \times \text{allocation\_pct} / 100$ and dispatches the order. *(Units: `equity` = engine ledger equity (Decimal, quote currency); `allocation_pct` = raw percent in `[1, 100]`; `size_units` = notional ÷ entry price.)*
4. On fill confirmation, Capital Layer updates `committed_margin` and `available_margin`.

---

## 8. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Single source of truth** | No other engine computes or stores equity or margin values. |
| **Atomic balance updates** | Equity changes are applied atomically; partial updates are impossible. |
| **Immutable history** | Equity snapshots are append-only and never mutated. |

---

## 9. Cross-References

- [PME Overview](../portfolio-management-engine/03-04-01-pme-overview-spec.md) — Engine boundaries and ledger model.
- [PME Layer 1 — Position](03-04-02-pme-layer1-position.md) — Upstream valuation source.
- [PME Layer 2 — Exposure](03-04-03-pme-layer2-exposure.md) — Leverage and exposure aggregation.
- [PME Layer 4 — Overview](03-04-05-pme-layer4-overview.md) — Safety reporting and drawdown ladder.
- [TAE Layer 2 — Execution](../trade-automation-engine/03-03-03-tae-layer2-execution.md) — Position Sizing Protocol consumer.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `paper_balances`, `portfolio_equity_history`.
