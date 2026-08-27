# TAE — Simulation Backend & Persistence (v7)

**Version:** 11.0 (2026-08-26) — v7 redesign: the paper trading engine becomes the `PaperSimulation` backend of the unified ExecutionEngine, and the persistence contract now includes restart recovery.
**Status:** Specified — v7 implementation in progress.
**Engine:** Trade Automation Engine (TAE)
**Purpose:** This document specifies the simulated execution backend (`PaperSimulation`), the shared cost model, the canonical persistence contract (trades, telemetry, equity, activity log), and the restart-recovery contract that keeps the trader's account intact across daemon restarts.

---

## 1. Role

`PaperSimulation` is the default `ExecutionBackend` of the [unified ExecutionEngine](03-03-03-tae-layer2-execution.md). It intercepts order packets and processes them against the live mid-price (from the DIE) to produce synthetic fills, rejections, and cancellations — with the same fee/slippage/funding accounting the live path will use.

```
[Setup Executor] ──► [ExecutionEngine] ──► [PaperSimulation] ──► ledgers + persistence
```

---

## 2. Fill Simulation

| Order | Rule |
|-------|------|
| Market | Fill immediately at `mid ± spread/2`, slippage bps applied. |
| Limit Buy | Fill when `mid ≤ limit`; fills at mid (price or better). Already marketable → **instant fill** at mid. |
| Limit Sell | Fill when `mid ≥ limit`; instant when already marketable. |
| Stop Sell | Trigger when `mid ≤ stop`, then market fill. |
| Stop Buy | Trigger when `mid ≥ stop`, then market fill. |
| Gap | Stop triggers with mid already beyond → market fill at current mid. |

---

## 3. Simulated Costs (shared model)

| Cost | Source | Applied |
|------|--------|---------|
| Maker fee | `[workspace.fees] maker_fee_pct` (0.02%) | Maker-side fills |
| Taker fee | `[workspace.fees] taker_fee_pct` (0.06%) | Taker-side fills |
| Funding | `[workspace.fees] funding_rate_8h` (0.01%) | Every 8h on open positions |
| Slippage | per-fill bps | Market fills and stop triggers |

Fees are deducted from realized PnL on each fill. **The same model runs in live mode** — live fills carry the exchange's real prices, but the accounting is identical.

---

## 4. Persistence (canonical write path)

One canonical write path per event, all through `database_storage` query functions:

| Table | Written On | Content |
|-------|-----------|---------|
| `trade_telemetry_history` | Position close | Entry/exit, fees, realized PnL, `trigger_source` = setup type (e.g. `TrendContinuation`), exit reason |
| `paper_trades` | Position close | Closed trade record with PnL |
| `portfolio_equity_history` | Periodic + close | Equity time-series for PAE drawdown/Sharpe |
| `paper_balances` | Close + session | Peak equity, initial, session equity (via SafetyManager) |
| `automation_activity` | Every executor event | Audit trail: setup accepted, order placed, filled, invalidated (level/signal/replaced), re-priced, adopted, ratcheted, setup-gone, confidence drop, closed, blocked |

**v10 activity events (added to the vocabulary):** `reprice_pending` (pending
entry moved to a fresh same-direction geometry), `replaced_adopted`
(replacement adopted same tick), `setup_gone_cancel` / `setup_gone_close`
(strict posture), `bracket_refresh` (asymmetric ratchet — SL tighten-only /
TP RR-improvement), `confidence_drop` (confidence-fall exit), `chase_entry`
(chase-mode market order), `expired` (pending-entry bar expiry).
`cancelled_replaced` remains for `replace_policy = "cancel"`.

`trigger_source` carrying the setup type is what powers PAE's strategy stats regrouped by setup type/direction/timeframe.

---

## 5. Restart Recovery Contract

The trader's account and open positions must survive a daemon restart:

- **Equity:** persisted (paper_balances + periodic equity snapshots); restored at boot — **never** silently re-seeded to `initial_capital_usd` when a persisted balance exists.
- **Open state:** on graceful shutdown, the engine persists the open position + pending/bracket orders + setup fingerprint + timestamp. On boot, the executor restores them: position re-armed with live mark, bracket orders re-armed, pending entry restored (subject to re-validation against the current top setup).
- **Stale state:** if the persisted state is older than a configurable staleness window (or a crash interrupted persistence), recovery **flattens** at the last known mark — the trader is never left with phantom positions.
- **Activity log:** persisted to `automation_activity`; the in-memory ring buffer is a cache of the same events.

---

## 6. Deterministic Replay (backtest support)

Feeding a historical sequence of `MarketSnapshot`s through `extract_top_setup` + the executor + the simulation backend reproduces identical trades. This is the engine the Backtesting Engine (BTE) replays. The executor logic is pure over (snapshot, state) → effects, so replay has no wall-clock dependencies.

**v8.2 parity guarantees (paper = backtest by construction).** The historical runner drives the identical `run_tick` session body with these additions so the replay behaves exactly like a paper session:

| Guarantee | Paper behavior | Backtest behavior (v8.2) |
|---|---|---|
| Safety ladder | `SafetyManager` per instance fed by live equity; soft gate blocks new entries in `DRAWDOWN_STOP`/`SUSPENDED` | Same: a simulated `SafetyManager` per instance fed by replayed equity |
| Funding | Settled every 8h of wall-clock on open positions | Settled at simulated 8h boundaries of replay time |
| End-of-run | N/A (session keeps running) | Open positions force-closed at the final replayed candle close, `exit_reason = "end_of_backtest"` |
| Sizing | `equity × allocation_pct / 100` | Identical (shared engine ledger) |
| Tick granularity | 1s executor ticks reading the latest snapshot per TF | One tick per completed candle of each symbol's smallest ladder TF (documented boundary) |
| Strategy dials (v10) | The instance's bound strategy drives the executor tick | The run's bound strategy drives the identical tick — historical and recorded replays resolve the run's strategy JSON and pass it into the executor (recorded previously used defaults; v10 parity fix) |
| Strategy gates (v10) | The daemon evaluates the breadth/systemic intake gates + PME portfolio gates and feeds `market_filter_allows_entry` | Historical replay evaluates the same `strategy_gates` functions on the simulated portfolio (breadth = cross-symbol bias share; systemic veto inert — no L7 in replay); recorded replay keeps gates off (replay parity) |

---

## 7. Cross-References

- [TAE Overview](03-03-01-tae-overview-spec.md) — layers, lifecycle, invalidation.
- [TAE Layer ④ — Execution](03-03-03-tae-layer2-execution.md) — unified engine, mode split.
- [TAE Instance Lifecycle](03-03-06-tae-instance-lifecycle-spec.md) — pause/stop behavior.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — table contracts.
