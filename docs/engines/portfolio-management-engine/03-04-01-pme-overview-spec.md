# Portfolio Management Engine — Overview Specification (v7)

**Version:** 11.0 (2026-08-26) — the v7 redesign makes PME **purely informational**: the veto/stance authority was erased; PME reports the portfolio's current state and the TAE setup executor consumes `safety_state` as its single soft entry gate.
**Status:** Specified — v7 implementation in progress.
**Engine:** Portfolio Management Engine (PME)
**Purpose:** This document specifies the boundaries, ledger model, layer structure, and safety reporting of the Portfolio Management Engine — the engine that answers **"what is my account doing right now?"** (equity, positions, exposure, capital, risk state) without executing anything.

---

## 1. Mission & Boundaries

The PME is the platform's **informational portfolio mirror**. It tracks active positions, aggregates exposure, computes capital/margin state, and maintains the account **safety state** — a read-only status ladder that the TAE setup executor consults before opening new entries.

**What PME does:**
- Maintains the per-instance equity ledger (via the unified execution engine), peak equity, session equity, daily PnL, and consecutive-loss counters.
- Computes Position / Exposure / Capital / Portfolio matrices from live state (pure math, `Decimal`).
- Maintains the `SafetyState` ladder (`NORMAL / WARN / CAUTIOUS / SUSPENDED / DRAWDOWN_STOP`) on every executor tick.
- Serves all of it through read-only API endpoints and the `PortfolioDashboard`.

**What PME does NOT do (v7):**
- **No enforcement.** The veto, hard-exit, stance machinery, and Gate-7 pre-trade checks are **erased**. PME never blocks, cancels, or forces anything.
- **No order construction.** Orders are built exclusively by the TAE setup executor.
- **No market interpretation.** The MME owns analysis; PME only mirrors the resulting state.

**The single enforcement point (TAE side, unchanged):** the setup executor refuses *new entries* when the instance safety state is `DRAWDOWN_STOP` or `SUSPENDED` (see [TAE Overview §7](../trade-automation-engine/03-03-01-tae-overview-spec.md)). PME computes the state; TAE applies the one rule.

```
[ExecutionEngine ledgers] ──► PME (pure computation)
        │                            │
        └── fills/equity             └──► [PortfolioOverviewMatrix] ──► API ──► PortfolioDashboard
                                                      │
                                                      └── safety_state ──► [TAE soft gate]
```

### 1.1 Layer Structure

| Layer | Name | Output |
|-------|------|--------|
| L1 | [Position Layer](03-04-02-pme-layer1-position.md) | Position Matrix (incl. mark-to-market) |
| L2 | [Exposure Layer](03-04-03-pme-layer2-exposure.md) | Exposure Matrix |
| L3 | [Capital Layer](03-04-04-pme-layer3-capital.md) | Capital Matrix |
| L4 | [Overview Layer](03-04-05-pme-layer4-overview.md) | PortfolioOverviewMatrix (risk *reporting*) |


**Dashboard tabs ↔ layers (v7.3).** Overview (landing) · Positions (L1) · Exposure (L2) · Capital (L3) · Portfolio (L4) · Safety (cross-cutting ladder last). Observe mode collapses to Overview + Safety (Readiness Board). Exposure limits are config-driven via `[workspace.risk_limits]` (see [07-07 §2](../../ui-ux/07-07-engine-dashboard-vocabulary.md)).

---

## 2. Ledger Model

The authoritative financial state lives in the **unified execution engine** (`ExecutionEngine` in `crates/portfolio-supervisor/src/execution/engine.rs`); PME reads it and mirrors it:

| Concept | Storage |
|---------|---------|
| Equity ledger (cash + realized PnL) | `ExecutionEngine.equity` (Decimal), persisted via `paper_balances` + equity snapshots |
| Active positions | `ExecutionEngine.positions` (per-symbol `PaperPosition`, marked to market each tick) |
| Peak / session equity, daily PnL, consecutive losses | `SafetyManager` (`paper_balances` persistence) |
| Equity history | `portfolio_equity_history` (60 s logger, 30-day purge) |
| Closed trades | `paper_trades`, `trade_telemetry_history` |

---

## 3. Margin & Leverage (informational)

| Metric | Default | Source |
|-------------|---------|--------|
| Cross leverage | 20× | `config.toml` `leverage.cross_leverage` |
| Max single-pair exposure | 20% of equity | `exposure_layer::ConcentrationLimits` (display + info) |
| Max portfolio exposure | 50% of equity | `exposure_layer::ConcentrationLimits` (display + info) |
| Margin usage alerts | WARN 80% / CLOSE-ONLY 95% / EMERGENCY 100% | `capital_layer::check_margin_alerts` (display only) |
| `max_daily_drawdown_pct` | 5% (= 0.05) of session equity | `config.toml` `safety` (drives WARN state) |
| `drawdown_limit_pct` | 30% (= 0.30) peak-to-trough | `config.toml` `safety` (drives DRAWDOWN_STOP state) |

The two drawdown metrics are distinct and are not synonyms (see §4).

---

## 4. Safety State Ladder (informational, maintained every tick)

`SafetyManager` (`crates/portfolio-supervisor/src/safety.rs`) tracks **five** escalating states, computed by `SafetyManager::update(equity)` on every executor tick (1 s):

```
NORMAL ──(daily_drawdown_pct ≥ max_daily_drawdown_pct)──► WARN
       ──(consecutive_losses[sym] ≥ caution_threshold)──► CAUTIOUS
       ──(consecutive_losses[sym] ≥ dropout_threshold)──► SUSPENDED (timed 8 h)
       ──(current_equity / peak_equity < 1 − drawdown_limit_pct)──► DRAWDOWN_STOP
```

- Losses are recorded by the setup executor when a position closes (`record_trade_outcome`); a win resets the per-symbol counter.
- Peak equity trails the high-water mark; DRAWDOWN_STOP is evaluated against it every tick.
- **Informational contract:** the state is *reported* — on the Safety panel, in `/api/instances/:id/safety`, and in the automation dashboard's safety chip. The only behavioral consequence lives in the TAE executor's soft gate (§1).

---

## 5. Read-only contract

Every PME endpoint is `GET` except the two informational resets:

| Endpoint | Purpose |
|----------|---------|
| `GET /api/instances/:id/portfolio` | Rich portfolio state (equity, PnL, drawdown, exposure, capital, positions, safety, systemic risk) |
| `GET /api/instances/:id/exposure` | Exposure Matrix |
| `GET /api/instances/:id/capital` | Capital Matrix + margin alert |
| `GET /api/instances/:id/safety` | Safety state, losses, peak/session equity, context |
| `POST /api/instances/:id/safety/session-reset` | Informational: rebaseline peak equity + daily PnL |
| `POST /api/instances/:id/safety/reset` | Informational: clear consecutive-loss counters |
| `POST /api/instances/:id/safety/release-veto` | Informational: return state to NORMAL + optional peak reset |

None of these can place, cancel, or modify orders.

---

## 6. Cross-References

- [PME Layer 1 — Position](03-04-02-pme-layer1-position.md)
- [PME Layer 2 — Exposure](03-04-03-pme-layer2-exposure.md)
- [PME Layer 3 — Capital](03-04-04-pme-layer3-capital.md)
- [PME Layer 4 — Overview](03-04-05-pme-layer4-overview.md)
- [TAE Overview — soft gate](../trade-automation-engine/03-03-01-tae-overview-spec.md)
- [Systemic Data Flow](../../conceptual-foundations/01-03-systemic-data-flow.md)
