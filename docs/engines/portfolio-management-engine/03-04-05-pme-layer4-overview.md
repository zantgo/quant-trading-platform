# PME Layer 4 — Overview Layer (v7)

**Version:** 11.0 (2026-08-26) — v7: the veto/stance machinery is erased; the Overview Layer is now **risk reporting** only. v8.2: renamed from "Portfolio Layer"; the output matrix is renamed `PortfolioOverviewMatrix`.
**Status:** Specified — v7 implementation in progress.
**Engine:** Portfolio Management Engine (PME)
**Layer:** 4 of 4
**Input Contract:** Position Matrix (L1), Exposure Matrix (L2), Capital Matrix (L3), [Overview Matrix](../../matrices/02-09-overview-matrix.md) (MME L7)
**Output Contract:** PortfolioOverviewMatrix (unified account-health report)
**Purpose:** This document specifies the Overview Layer — the consolidation layer that synthesizes Position, Exposure, and Capital matrices into a single account-health vector and **reports** systemic safety conditions. It enforces nothing.

---

## 1. Purpose

The Overview Layer consolidates Position, Exposure, and Capital matrices into one portfolio report and maintains the account **safety state** (a read-only status ladder). In v7 it is a **reporting layer**: there is no veto, no stance override, no order cancellation. The only behavioral consumer of its output is the TAE setup executor's soft entry gate, which reads `safety_state` before opening new positions (see [TAE Overview §7](../trade-automation-engine/03-03-01-tae-overview-spec.md)).

```
[Position Matrix ] ─┐
[Exposure Matrix ] ─┼──► OVERVIEW LAYER (L4) ──► [PortfolioOverviewMatrix] ──► API / Dashboard
[Capital Matrix  ] ─┘                                  │
[Overview Matrix ] ─┘                                  │
                                                       └── safety_state ──► [TAE soft gate]
```

---

## 2. PortfolioOverviewMatrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `current_equity` | `Decimal` | Total account equity (`initial_balance + realized_pnl + unrealized_pnl`). |
| `realized_pnl` | `Decimal` | Cumulative realized profit/loss (net of fees). |
| `unrealized_pnl` | `Decimal` | Aggregate unrealized PnL (mark-to-market each tick). |
| `gross_exposure` | `Decimal` | Total notional exposure. |
| `net_exposure` | `Decimal` | Directional net exposure. |
| `margin_usage_ratio` | `Decimal` | Fraction of equity committed to margin, in `[0, 1]`. |
| `leverage_ratio` | `Decimal` | Effective leverage (`gross_exposure / equity`). |
| `daily_pnl` | `Decimal` | PnL in current session. |
| `max_daily_drawdown_pct` | `Decimal` | Configuration limit (default 0.05 = 5 %); drives the `WARN` state. |
| `drawdown_limit_pct` | `Decimal` | Peak-to-trough decline threshold (default 0.30); drives `DRAWDOWN_STOP`. |
| `peak_equity` | `Decimal` | Trailing high-water mark of `current_equity`. |
| `safety_state` | `SafetyState` | `NORMAL` / `WARN` / `CAUTIOUS` / `SUSPENDED` / `DRAWDOWN_STOP`. |
| `systemic_risk_score` | `f64` | MME Overview Matrix Systemic Risk Score (display only). |
| `consecutive_losses` | `map<string, u32>` | Per-symbol consecutive-loss counter. |
| `position_count` | `u32` | Number of active positions. |

---

## 3. Safety State Ladder (informational)

`SafetyManager::update(equity)` runs on every executor tick and maintains the five states:

```
NORMAL ──(daily_drawdown_pct ≥ max_daily_drawdown_pct)──► WARN
       ──(consecutive_losses[sym] ≥ caution_threshold)──► CAUTIOUS
       ──(consecutive_losses[sym] ≥ dropout_threshold)──► SUSPENDED (timed cooldown)
       ──(current_equity / peak_equity < 1 − drawdown_limit_pct)──► DRAWDOWN_STOP
```

- Losses are recorded by the setup executor on position close (`record_trade_outcome`); wins reset the per-symbol counter.
- Peak equity trails the high-water mark; `DRAWDOWN_STOP` persists until `release_veto` (informational reset) or session reset.
- **No state change ever cancels orders, closes positions, or blocks exits.** Exits (TP/SL/flip/manual/stop-flatten) are always permitted.
- The executor's soft gate blocks *new entries* in `DRAWDOWN_STOP` / `SUSPENDED` only.

## 4. Systemic Risk (display)

The MME L7 `OverviewMatrix.systemic_risk_score` is surfaced in the portfolio report and the dashboard's Overview panel. It is **display-only** in v7 — the systemic-risk veto was erased with the policy engine.

## 5. Resets (informational)

| Action | Effect |
|--------|--------|
| `session_reset` | Rebaseline peak equity + daily PnL to current equity. |
| `reset_consecutive_losses` | Clear the per-symbol consecutive-loss counters. |
| `release_veto` | Return the safety state to `NORMAL` (only when the underlying drawdown condition has cleared) + optional peak reset. |

---

## 6. Cross-References

- [PME Overview](03-04-01-pme-overview-spec.md) — read-only contract.
- [PME Layer 1 — Position](03-04-02-pme-layer1-position.md)
- [PME Layer 2 — Exposure](03-04-03-pme-layer2-exposure.md)
- [PME Layer 3 — Capital](03-04-04-pme-layer3-capital.md)
- [TAE Overview §7 — soft gate](../trade-automation-engine/03-03-01-tae-overview-spec.md)
