# PAE Layer 1 — Trade Analytics Layer

**Version:** 11.0 (2026-08-26) — v7: implemented; grouping keyed by setup type.
**Status:** Specified — implemented.
**Engine:** Performance Analytics Engine (PAE)
**Layer:** 1 of 4
**Input Contract:** Closed trade logs from PME
**Output Contract:** Trade Analytics Matrix (normalized ledger of reconstructed closed trades)
**Purpose:** This document specifies the Trade Analytics Layer — the trade reconstruction system that parses execution logs into complete, single-trade records with execution efficiency metrics.

---

## 1. Purpose

The Trade Analytics Layer is the PAE's **trade reconstruction engine**. It consumes raw execution logs from the PME, parses each closed trade from initial entry to final exit, and normalizes every transaction into a standardized trade ledger — computing hold times, execution slippage, and peak-trade deviations (MFE/MAE).

```
[Closed Trade Ledgers] ──► TRADE ANALYTICS (L1) ──► [Trade Analytics Matrix] ──► [Strategy Analytics (L2)]
```

---

## 2. Trade Analytics Matrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `trade_id` | `string` | Unique system-wide transaction identifier. |
| `symbol` | `string` | The financial instrument traded. |
| `direction` | `Direction` | `Long` / `Short`. |
| `entry_timestamp` | `u64` | Unix epoch of first entry fill. |
| `exit_timestamp` | `u64` | Unix epoch of final exit fill. |
| `hold_time_seconds` | `u64` | Exact duration between first entry and final exit. |
| `entry_price` | `Decimal` | Volume-weighted average entry price. |
| `exit_price` | `Decimal` | Volume-weighted average exit price. |
| `size` | `Decimal` | Base-asset quantity traded. |
| `gross_pnl` | `Decimal` | Closed financial result before fees. |
| `net_pnl` | `Decimal` | Realized profit or loss after trading fees, funding costs, and slippage. |
| `roi_pct` | `Decimal` | Net return as percentage of allocated capital. |
| `execution_slippage` | `Decimal` | Mathematical difference between target policy price and actual exchange fill price. |
| `mfe` | `Decimal` | Maximum Favorable Excursion — peak unrealized profit during the trade. |
| `mae` | `Decimal` | Maximum Adverse Excursion — peak unrealized loss during the trade. |
| `trigger_source` | `string` | The execution policy or manual action that initiated the trade. |
| `exit_reason` | `string` | `STOP_LOSS` / `TAKE_PROFIT` / `SIGNAL_EXIT` / `MANUAL` / `VETO_LIQUIDATION` / `EMERGENCY_LIQUIDATION` / `THESIS_INVALIDATION`. Legacy rows may carry `VETO`; map to `VETO_LIQUIDATION` on read. |
| `flat_trade` | `bool` | `true` if the trade's gross PnL was zero before fees (avoids division-by-zero in `fee_efficiency`); see §4 guard. |

---

## 3. Trade Reconstruction Process

```
1. Query closed trades from PME (paper_trades, trade_telemetry_history).
2. Group fill events by trade_id (entry fills → exit fills).
3. Compute volume-weighted average entry and exit prices.
4. Calculate hold_time = exit_timestamp − entry_timestamp.
5. Traverse position_equity_snapshots within the hold window to find MFE and MAE.
6. Compute slippage = |target_price − fill_price| for each fill.
7. Deduct fees (maker/taker) and funding payments.
8. Write reconstructed trade to Trade Analytics Matrix.
```

---

## 4. Execution Efficiency Metrics

| Metric | Formula | Interpretation |
|--------|---------|---------------|
| **Slippage bps** | `(|fill_price − target_price| / target_price) × 10000` | Execution quality. |
| **MAE ratio** | `|MAE| / |gross_pnl|` | How much the trade moved against before succeeding. |
| **MFE capture** | `gross_pnl / MFE` | How much of the available profit was captured. |
| **Fee efficiency** | `(gross_pnl − net_pnl) / |gross_pnl|` if `|gross_pnl| > 0`, else `0.0` with `flat_trade: true` flag set | Fee drag as a non-negative percentage of `|gross_pnl|`. Using `|gross_pnl|` as the denominator (rather than `gross_pnl`) keeps the metric non-negative for both winning and losing trades — a fee-increased loss yields a positive fee_efficiency, not a negative one. |

> **Division-by-zero guard.** When `gross_pnl = 0` (a flat-then-fee round-trip — entry fills cancel exit fills exactly before fees, so the trade closes at zero before fees are deducted), the bare formula `(gross_pnl − net_pnl) / gross_pnl` evaluates `0 / 0 = NaN`. The guard returns `0.0` and sets the `flat_trade: true` flag on the trade record so downstream consumers can detect and exclude the trade from aggregate ratio calculations (e.g. expectancy, profit factor). A `flat_trade=true` record indicates that the trade fully round-tripped price-wise but lost the fee component — the strategy had zero directional edge on that bar.

---

## 5. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Deterministic reconstruction** | Identical execution logs produce an identical Trade Analytics Matrix. |
| **Complete attribution** | Every fill, fee, and funding payment is attributed to its trade. |
| **Read-only** | The PAE never modifies trade records or portfolio state. |

---

## 6. Cross-References

- [PAE Overview](../performance-analytics-engine/03-05-01-pae-overview-spec.md) — Engine boundaries and database model.
- [PAE Layer 2 — Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md) — Next-stage consumer.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `paper_trades`, `trade_telemetry_history`.
- [Ontology — Performance Analytics](../../conceptual-foundations/01-01-ontology.md) — Conceptual definitions.
