# Trade Automation Engine — Overview Specification (v7)

**Version:** 11.0 (2026-08-26) — the v7 redesign replaces the policy-driven TAE with a **setup executor** that consumes MME top setups directly.
**Status:** Specified — v7 implementation in progress.
**Engine:** Trade Automation Engine (TAE)
**Purpose:** This document specifies the boundaries, architecture, trade lifecycle, invalidation semantics, and execution model of the v7 Trade Automation Engine — the engine that **executes what the Market Monitoring Engine recommends** through a single unified execution engine whose only mode-dependent part is the final broker dispatch.

---

## 1. Mission & Boundaries

The TAE is the platform's **execution authority**. It listens for **top setups** emitted by the MME recommendation layer (the L4 opportunity + L6 decision matrices broadcast on every completed candle), accepts the best eligible setup per symbol, and manages the resulting trade to completion (take-profit, stop-loss, or invalidation).

**What v7 removes.** Policies, stances, trigger modes, the pre-trade gate chain, the veto loop, and profile evaluation are **erased**. The executor runs the MME's finished product directly:

- **No market interpretation** — the MME decides *what* to trade (entry/SL/TP/RR). The TAE only executes.
- **No policy configuration** — there is nothing to author. A single `[workspace.minimal_tae]` config section tunes risk.
- **No PME enforcement** — PME is informational. The executor applies exactly one soft gate: no new entries when the instance safety state is `DRAWDOWN_STOP` or `SUSPENDED`.

**One engine for paper and live.** The TAE is built so that paper and live are the same program: same fees, same commissions, same slippage model, same position accounting, same equity ledger. Paper simply simulates fills; live will route orders to an exchange. Only the last millimeter differs (`ExecutionBackend` dispatch).

**Per-instance lifecycle.** Every instance carries a `LifecycleState` ∈ {`RUNNING`, `instance PAUSED`, `STOPPING`, `STOPPED`}. Entries are admitted only when `RUNNING`; `PAUSED` cancels pending entries and holds open positions; `STOP` flattens everything and transitions to `STOPPED`. See [TAE Instance Lifecycle](03-03-06-tae-instance-lifecycle-spec.md).

---

## 2. Architecture — the 7 layers

```
MME (unchanged) ──► ① Setup Intake          extract top setup (entry/SL/TP/RR)
                        │
                        ▼
                     ② Setup Executor        trade lifecycle state machine
                        │
                        ▼
                     ③ Sizing & Economics    allocation_pct → size + fees + projection
                        │
                        ▼
                     ④ UNIFIED ExecutionEngine ◄── ExecutionBackend trait
                        │   orders/fills/positions/equity/funding — SHARED
                        │        ├─ PaperSimulation (now)
                        │        └─ LiveBroker      (later, same trait)
                        ▼
                     ⑤ Risk & Invalidation    bracket, level/signal invalidation,
                                              safety soft gate, lifecycle gate
                        │
                        ▼
                     ⑥ Telemetry & Persistence  trades, equity history, activity log,
                                                restart-recovery state
                        │
                        ▼
                     ⑦ Surface                /api/instances/:id/automation + dashboard
```

| Layer | Name | Documented In |
|-------|------|---------------|
| ① | Setup Intake | §3 below |
| ② | Lifecycle & Adoption Layer (trade lifecycle) | §4 below |
| ③ | Sizing & Economics | §5 below |
| ④ | Unified ExecutionEngine | [Layer 2 — Execution](03-03-03-tae-layer2-execution.md) |
| ⑤ | Risk & Invalidation | §6 below |
| ⑥ | Telemetry & Persistence | [Paper Trading / Simulation Backend](03-03-05-tae-paper-trading-spec.md) |
| ⑦ | Surface (API + dashboard) | §8 below |


**Dashboard tabs ↔ layers (v7.3).** Overview (① + ② + ③ aggregate; observe = Setup Radar, paper = Paper Lab, live = Live Cockpit) · Orders (④ execution order board) · Activity (⑥ telemetry log) · Trade History (⑥ closed trades). Observe mode collapses to Overview + Activity (see [07-07 §3](../../ui-ux/07-07-engine-dashboard-vocabulary.md)).

---

## 3. Terminology (trader-perspective definitions)

| Term | Definition |
|---|---|
| **Setup** | A concrete trade plan produced by the MME recommendation layer: direction (LONG/SHORT), entry zone (price range), take-profit zone (price range), invalidation level (price), net reward-to-risk ratio, quality score, setup type (e.g. `TrendContinuation`). |
| **Entry zone** | The price range where the setup wants you to enter. **LONG:** the zone sits *below* the candle close (guaranteed by `derive_side_zones`: `high ≤ close`) — you wait for price to *pull back into it*. **SHORT:** the zone sits *above* the close (`low ≥ close`) — you wait for price to *rally into it*. The entry order is placed at the **zone midpoint**. |
| **Take-profit (TP)** | The price range where the setup wants you to exit with profit. Order placed at the **zone midpoint**. |
| **Stop-loss (SL)** | Same price as the **invalidation level** for the active side. This is the price beyond which the setup's logic is *broken* — the maximum you are willing to lose on this trade. **LONG:** SL below entry. **SHORT:** SL above entry. |
| **Invalidation** | The setup's thesis is broken. Exactly two flavors, both surfaced with distinct frontend labels: |
| | **① LEVEL invalidation** — price traded through the SL/invalidation level. Before entry: the setup is dead, the order is cancelled, nothing is traded. After entry: the SL stop fills (a loss, by design — this is the risk you sized for). |
| | **② SIGNAL invalidation** — the recommendation itself changed: the MME's direction flipped to the opposite side on a new completed candle. Before entry: the pending order is cancelled. After entry: the position is **closed at market** (the level was never hit; we exit because the market changed its mind, not because we reached our risk limit). |
| **REPLACED** | (Pending entries only) a different setup type now tops the ranking on a new completed candle — the tracked setup is superseded; the pending order is cancelled. |
| **Instant fill** | The entry limit order is placed **unconditionally** at the zone midpoint. Where price sits relative to the zone determines the fill behavior: |
| | - **Price on the approach side** (LONG: mid > zone.high) — resting limit; waits for the pullback into the zone. |
| | - **Price inside the zone** — fills immediately (paper: at mid with slippage; live: marketable limit crosses). |
| | - **Price already beyond the far side** (LONG: mid < zone.low) — the limit is marketable; fills immediately at the current mid, i.e. a price **better than the midpoint**. The plan isn't cancelled; the market simply came to us cheaper. |
| **Top setup** | Per symbol, the best eligible setup among the 4 latest completed candle snapshots (one per timeframe): highest `display_score` among profiles that are `Actionable` (net RR ≥ 1.0), geometry-consistent, `preconditions_met > 0`, with `trade_readiness == READY`. |
| **Tracked setup** | The setup the executor has accepted and is currently acting on (pending entry or open position). |
| **Setup fingerprint** | Idempotency key for a setup: `symbol + direction + setup_type + candle_timestamp`. The same setup can never be accepted twice. |

---

## 4. Trade lifecycle (the trader's view)

### 4.1 Layer ① — Setup Intake

**Input:** the 4 latest completed `MarketSnapshot`s per symbol (micro/fast/slow/macro), each carrying the full MTF-synthesized decision matrix. Read from the instance's `TimeframeBuffers` every executor tick (1s).

**Pure function:** `extract_top_setup(snapshots) -> Option<SetupPlan>`

1. `snapshot.is_completed == Some(true)` and `decision_context` present.
2. Candidates = `opportunity.profiles[]` where `preconditions_met > 0`, `trade_viability == Actionable`, geometry consistent for the active side.
3. Active side from `analysis.bias` (fallback `advisory.directional_guidance`); neutral → no setup.
4. `SetupPlan { symbol, direction, entry_mid, sl, tp, net_rr, setup_type, score, source_tf, time_horizon }`:
   - `entry_mid = (entry_zone.low + entry_zone.high) / 2`
   - `sl = long_invalidation_level | short_invalidation_level`
   - `tp = (target_zone.low + target_zone.high) / 2`
   - `net_rr` from `decision_context.expected_reward_risk_ratio` (risk-discounted), fallback profile `long/short_expected_rr_internal`
5. **RR filter:** `net_rr >= config.min_net_rr` (default 1.0). Rejected setups logged with reason.
6. **Aggregation:** among the 4 snapshots' eligible plans, pick highest `score` (ties → faster TF wins).

### 4.2 Layer ② — Lifecycle & Adoption Layer (state machine)

```
Idle ──accept──► PendingEntry ──fill──► PositionOpen ──TP/SL/invalidate──► Closed
  ▲                  │  ▲                                                   │
  └──────────────────┴──┴──────────── cancel (invalidate/abandon) ─────────┘
```

**Adoption rules (Idle → PendingEntry):**
- No existing pending entry or open position for the symbol (one position per symbol).
- Global position cap respected: open positions across all symbols `< max_open_positions` (1–100, default 10).
- Safety soft gate: instance safety state ∉ {`DRAWDOWN_STOP`, `SUSPENDED`}.
- Lifecycle gate: instance is `RUNNING`.
- Entry order = **limit** at `entry_mid` (LONG → `Buy`, SHORT → `Sell`), sized by Layer ③.
- The setup fingerprint is recorded; re-acceptance of the same setup is impossible.

**While PendingEntry:** fills evaluated each tick; cancelled on LEVEL breach, SIGNAL flip, or REPLACED (see §6).

**While PositionOpen:** bracket (TP limit + SL stop) armed on fill; fills evaluated each tick; funding settled every 8h.

**Exit outcomes (each writes `exit_reason` to telemetry):**

| Outcome | Trigger | Mechanics |
|---|---|---|
| TP hit | TP limit fills | Closed at TP, win, consecutive-loss counter reset |
| SL hit | SL stop fills | Closed at SL, loss → safety counters update |
| SIGNAL invalidation | Direction flipped on completed candle | Closed **at market**, `exit_reason = "invalidated_signal"` |
| Stop flatten | Instance stopped | All orders cancelled, position closed at market, lifecycle → STOPPED |
| Manual close | Operator `POST /automation/close` | Bracket cancelled, position closed at market, `exit_reason = "manual"` |

**No re-entry churn:** after any close, a new setup is accepted only from a **later completed candle** (never the candle that produced the close).

---

## 5. Layer ③ — Sizing & Economics (v8.2 allocation model)

**Canonical sizing (v8.2):** the position size is the instance's allocated share of current portfolio equity — the stop-loss no longer sizes the position:

```text
notional             = equity × allocation_pct / 100
position_size_units  = notional / entry_mid
```

- `allocation_pct` ∈ 1–100 % (global default 10 %; per-instance override; the **sum of all instance allocations is validated ≤ 100 %** — enforced at config-save, instance-create, and backtest-run).
- `equity` comes from the shared execution-engine ledger (the platform portfolio — one equity across all instances).
- `margin_required = notional / leverage`; `liquidation_price = entry_mid ∓ entry_mid / leverage` — leverage still applies to margin, not to the allocation itself.
- Optional notional clamp `max_position_size_usd` still applies.
- The old stop-distance sizing (`risk_per_trade_pct`) is **erased**; the `/api/risk/calculate` drawer retains `risk_calculator.rs::compute_risk` as a manual preview only — it no longer sizes execution.

Inputs (from config + live state):
- `entry = entry_mid`, `stop_loss = sl`, `take_profit = tp` (risk display + bracket levels)
- `equity` = engine ledger equity
- `allocation_pct` = instance's portfolio share (1–100 %)
- `leverage` = instance leverage config
- fees = instance fee config (maker/taker), slippage bps

Outputs → usage:

| Output | Used for |
|---|---|
| `position_size_units` | Entry order quantity (notional clamped to `max_position_size_usd`) |
| `position_notional` | Exposure display |
| `entry_fee_usd` / `exit_fee_usd` / `total_fees` | Fee accounting + display (identical in paper and live) |
| `liquidation_price` | Risk display |
| `net_pnl` (at TP), `roi_pct`, net R:R | Projected Risk and Return card; net R:R re-gates acceptance (`min_net_rr`) |

**Fee model (shared, both modes):** taker/maker % per side on notional, slippage bps applied on fills, 8h funding on open positions — all from `[workspace.fees]`. Paper simulates these costs; live uses them for accounting. The engine never changes behavior by mode — only the fill source does.

---

## 6. Layer ⑤ — Invalidation semantics (complete scenario table)

**LONG shown; SHORT mirrors.** LEVEL = price crossed the SL/invalidation level. SIGNAL = MME direction flipped opposite on a completed candle. REPLACED = different setup type tops the ranking (pending only). GONE = no actionable setup (neutral / STAND_ASIDE).

**v10 posture** (`tae.risk.setup_gone_policy`, tri-state — config + TAE Settings tab):

| Posture | Pending + setup gone | Open + setup gone |
|---|---|---|
| `balanced` (default) | keep pending until bar-expiry (default 12 bars) | hold — SL/TP/time-stop remain the only exits |
| `strict` | cancel pending immediately | close at market, `exit_reason = "setup_gone"` |
| `risky` | keep pending forever (no expiry) | hold — exit only on an opposite-direction flip |

| # | Scenario (LONG) | Executor behavior | Frontend label |
|---|---|---|---|
| 1 | Price above entry zone (normal pullback setup) | Limit BUY placed per `entry_mode` dial; wait | `WAITING ENTRY` |
| 2 | Price inside entry zone | Limit BUY placed; fills as soon as mid ≤ price | `WAITING ENTRY` → `FILLED` |
| 3 | Price already below entry zone low (ran through) | `take_better` (default): fills immediately at current mid (better than the resting price); `cancel`: entry refused | `INSTANT FILL` |
| 4 | Pending; price crosses **below SL** (LEVEL) | Entry order **cancelled**; setup dead | `INVALIDATED — level breached` |
| 5 | Pending; MME direction **flips to SHORT** (SIGNAL) | Entry order **cancelled** (posture-independent) | `INVALIDATED — recommendation flipped` |
| 6 | Pending; different setup type wins ranking (REPLACED) | `cancel_and_adopt` (default): cancel old + adopt the replacement **same tick**; `cancel`: v9 behavior | `CANCELLED — replaced → adopted` |
| 7 | Pending; setup **gone** (neutral) | Posture: balanced = keep until expiry (default 12 bars); strict = cancel (`setup_gone_cancel`); risky = keep forever | `HOLDING PENDING` / `CANCELLED — setup gone` |
| 8 | Pending; new candle, same direction + type (fresh zone) | **Re-price** the pending order to the fresh geometry when the move ≥ `min_reprice_delta_atr × ATR` (cancel-first, then place; projection recomputed) | `RE-PRICED` |
| 9 | Open; **TP hit** | Close at TP; win (always 100 % of size) | `TP HIT` |
| 10 | Open; **SL hit** (price crossed invalidation level — LEVEL) | Close at SL; loss | `SL HIT` |
| 11 | Open; price **gaps through SL** (both TP and SL marketable) | **SL fills first** (risk before profit), at market | `SL HIT (gap)` |
| 12 | Open; MME direction **flips to SHORT** (SIGNAL) | **Close at market now** (not at SL — level never hit) | `INVALIDATED — recommendation flipped — closed at market` |
| 13 | Open; setup **gone** (neutral) | Posture: balanced = **hold** (bracket + time stop bound the risk); strict = close at market (`setup_gone_close`); risky = hold until an opposite flip | `HOLDING` / `CLOSED — setup gone` |
| 14 | Open; new candle, same direction (fresh setup) | **Asymmetric ratchet:** SL only tightens in favor (never widens); TP refreshes only when the fresh setup improves net RR by ≥ `tp_refresh_min_rr_delta`; both gated by the min-reprice delta | `BRACKET REFRESHED` / `HOLDING` |
| 15 | Open; source-snapshot confidence fell ≥ `confidence_drop_pct` | Close at market, `exit_reason = "confidence_drop"` | `CLOSED — confidence drop` |
| 16 | Instance paused | Pending entries cancelled; position held; no new setups | `instance PAUSED` |
| 17 | Instance stopped | Everything flattened at market; lifecycle → STOPPED | `STOPPED — flattened` |
| 18 | Safety state `DRAWDOWN_STOP` / `SUSPENDED` | No new entries; open positions managed normally | `BLOCKED — safety state` |

**Why #13 holds under balanced:** invalidation-by-flip is defined strictly as an *opposite* direction. Neutral means "no opinion", not "wrong side" — the SL, the time stop and the pending-entry expiry already bound the risk; exiting on every transient neutral bleeds fees. Operators wanting the strict semantics select the `strict` posture; `invalidate_on` remains `direction_flip` for the flip semantics.

**Frontend definition of invalidation (user-facing copy, shown in an info banner + tooltips):**

> *Invalidation means the setup's thesis is broken. It happens two ways: (1) price trades through the stop-loss level — if we hadn't entered yet, the order is cancelled and nothing is traded; if we were in, the stop takes us out. (2) the recommendation flips to the opposite direction — before entry the order is cancelled; after entry the position is closed at market. A neutral signal does not invalidate an open position (balanced posture).*

---

## 7. Risk gates (the only gates — no veto system)

1. **Safety soft gate:** no new entries when instance safety state is `DRAWDOWN_STOP` or `SUSPENDED` (informational PME state; the only enforcement point). UI shows: `BLOCKED — safety state SUSPENDED`.
2. **Lifecycle gate:** entries only while `RUNNING`; pause cancels pending; stop flattens.
3. **Global position cap:** `max_open_positions` across all symbols (1–100, default 10).
4. **Fill priority:** if both TP and SL are marketable on the same tick (gap), **SL fills first**.
5. **Bracket cleanup:** any non-bracket close (SIGNAL flip, manual, stop flatten) cancels the remaining bracket orders first.

---

## 8. Layer ⑦ — Surface

### 8.1 API

`GET /api/instances/:id/automation` returns:

```json
{
  "mode": "paper", "enabled": true,
  "tracked_setup": { "symbol", "direction", "setup_type", "score", "source_tf",
                     "entry_mid", "entry_zone": { "low", "high" }, "sl", "tp",
                     "net_rr", "time_horizon" },
  "projection": { "position_size_units", "position_notional", "entry_fee_usd",
                  "exit_fee_usd", "total_fees", "liquidation_price",
                  "net_profit_usd", "roi_pct" },
  "entry_order": { "id", "status", "price", "filled_at" },
  "bracket": { "tp_order": { }, "sl_order": { } },
  "position": { "direction", "size", "entry_price", "mark_price", "unrealized_pnl" },
  "invalidation": { "state": "none|level_breach|signal_flip", "detail": "" },
  "activity_log": [ { "ts", "event", "detail" } ],
  "safety_gate": { "blocked": false, "reason": null },
  "lifecycle": "RUNNING|instance PAUSED|STOPPING|STOPPED",
  "open_positions_count": 0
}
```

`POST /api/instances/:id/automation/close` — **manual override**: cancels pending/bracket orders and closes the open position at market. `exit_reason = "manual"`.

### 8.2 Dashboard

The `TradeAutomationDashboard` shows: PAPER/LIVE mode badge, automation toggle, instance selector, lifecycle + safety chips; the **Active Setup card** (direction, setup type, score, source TF, entry mid + zone, SL, TP, net R:R) with the **Projected Risk and Return block** (size, notional, entry/exit fees, liquidation, projected +$ at TP / −$ at SL, ROI); the **Order board** (entry order status + reasons, bracket orders); the **Position card** (direction, size, entry, mark, unrealized PnL, TP/SL levels, invalidation banner, manual Close now); the **Activity log**; **Trade history**; and the **Equity strip**.

---

## 9. Configuration

```toml
[workspace.minimal_tae]
enabled = true
allocation_pct = 10.0        # % of equity allocated per position (1–100; Σ ≤ 100 across instances)
min_net_rr = 1.0             # fee-adjusted minimum reward:risk
# max_position_size_pct_of_equity = 50  # optional relative notional cap
max_open_positions = 10      # global concurrent-position cap (1–100)
entry_mode = "zone_midpoint" # workspace fallback; the strategy dials win
invalidate_on = "direction_flip"  # strict opposite-flip semantics
```

Instance level: `mode = "paper"` per `[[workspace.instances]]` (`ExecutionMode { Paper, Live }`).

**v10 lifecycle-hardening dials** live in the bound strategy's `tae` section (per-strategy, params frozen at entry, recharge affects new setups only) — see [03-03-07 TAE Strategy Settings](03-03-07-tae-strategy-settings.md) for the canonical JSON: posture (`setup_gone_policy`), pending re-price (`min_reprice_delta_atr`, `replace_policy`), entry dial (`entry_mode` variants incl. `chase`, `instant_fill_policy`, `spread_gate_bps`, `max_setup_age_bars`, `pending_entry_expiry_bars`), and exit dial (`sl_mode` / `sl_padding_atr` / `atr_anchor_mult` / `min_sl_atr`, `tp_placement`, `tp_refresh_min_rr_delta`, `confidence_drop_pct`).

---

## 10. Cross-References

- [TAE Layer 2 — Execution](03-03-03-tae-layer2-execution.md) — the unified ExecutionEngine + backend trait.
- [TAE Paper Trading / Simulation Backend](03-03-05-tae-paper-trading-spec.md) — PaperSimulation fills, costs, persistence, restart recovery.
- [TAE Instance Lifecycle](03-03-06-tae-instance-lifecycle-spec.md) — RUNNING / instance PAUSED / STOPPING / STOPPED behavior.
- [Decision Matrix](../../matrices/02-04-decision-matrix.md) — Input contract (readiness, expected R:R).
- [Opportunity Matrix](../../matrices/02-08-opportunity-matrix.md) — Setup geometry source.
- [PME Layer 3 — Capital](../portfolio-management-engine/03-04-04-pme-layer3-capital.md) — Equity source (informational).
- [API Gateway Contract](../../integration-and-api/06-01-api-gateway-contract.md) — Automation endpoints.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — Trade, telemetry, activity-log, recovery tables.
