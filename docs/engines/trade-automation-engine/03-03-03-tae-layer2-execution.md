# TAE Layer ④ — Unified ExecutionEngine

**Version:** 11.0 (2026-08-26) — v11: stop floor (L6 formula), TP reachability cap (1.5), TF-role separation.
**Status:** Implemented (v11) — Hyperliquid + Bitget live backends, quantity-first execution model.
**Previous:** 11.0 (2026-08-26) — v7 redesign + v7.1 Bitget.
**Engine:** Trade Automation Engine (TAE)
**Input Contract:** SetupPlan (Setup Executor), [Decision Matrix](../../matrices/02-04-decision-matrix.md) (via the snapshot the executor passes through)
**Output Contract:** orders, positions, equity, fees — surfaced via [Layer ⑦ API](03-03-01-tae-overview-spec.md#81-api)
**Purpose:** This document specifies the unified execution engine — one engine for paper and live, where the *only* mode-dependent part is the `ExecutionBackend` dispatch at the very end of the execution path.

---

## 1. Design Principle: One Engine, Two Backends

The v7 engine is built so that **paper and live are the same program**:

- same order lifecycle, position ledger, equity ledger, fee accounting, slippage model, funding settlement, bracket management;
- the only difference is *where orders are dispatched and where fills come from*.

```
                     ┌───────────────────────────────────────────┐
                     │            UNIFIED ExecutionEngine        │
                     │  orders · fills · positions · equity      │
                     │  fees · slippage · funding · brackets     │
                     └───────────────▲───────────────────────────┘
                                     │ ExecutionBackend trait
                     ┌───────────────┴───────────────┐
                     │                                │
            ┌────────┴────────┐             ┌─────────┴────────┐
            │  PaperSimulation │             │    LiveBroker    │
            │  (v7, default)   │             │  (future phase)  │
            └─────────────────┘             └──────────────────┘
```

**Why the mode lives at the end.** The trader needs the same economics in both modes: when live, the engine must still account commissions, fees, slippage, and funding exactly as paper does — only the fill source changes. A mode switch anywhere else would fork the accounting and make paper results untrustworthy as a live predictor.

---

## 2. ExecutionMode & ExecutionBackend

```rust
pub enum ExecutionMode { Paper, Live }

pub trait ExecutionBackend: Send + Sync {
    async fn submit_order(&self, packet: OrderPacket) -> Result<String, String>;
    async fn cancel_order(&self, order_id: &str) -> Result<(), String>;
    async fn fetch_fills(&self) -> Vec<Fill>;
    async fn fetch_balance(&self) -> Balance;
}
```

| Backend | Status | Behavior |
|---------|--------|----------|
| `PaperSimulation` | **v7** | Internal matching against the live mid-price: limit/stop/market fills, spread + slippage, instant marketable-limit fills. No external I/O. |
| `LiveBroker` | Future phase | Same trait against Hyperliquid / Bitget REST + WS. The engine needs no changes. |

`mode = "paper"` is a per-instance config field (`[[workspace.instances]].mode`), default `Paper`. The engine exposes the mode in every API response so the frontend can render the PAPER/LIVE badge.

---

## 3. Shared State & Accounting (mode-agnostic)

| Concern | Design |
|---------|--------|
| **Order lifecycle** | `OrderLifecycle` state machine: `Pending → Submitted → Open → PartiallyFilled → Closed`, plus `Cancelled` / `Rejected`. Every transition timestamped; partial fills tracked. (`execution/state_machine.rs`) |
| **Order registry** | In-memory `HashMap<order_id, OrderLifecycle>`; order ids `paper_xxxxxx` (simulation) or exchange ids (live). |
| **Position ledger** | Per-symbol `PaperPosition`: direction, size, entry price, realized PnL, fees paid. Opened on entry fill, closed on exit fill/market close. |
| **Equity ledger** | Master account balance: cash, realized PnL, unrealized PnL, daily PnL. Seeded from persisted state or `sum(initial_capital_usd)`; updated on every fill and mark. **Identical code path in both modes.** |
| **Fee accounting** | Taker/maker % on notional per leg, slippage bps on fills, 8h funding on open positions. Applied to PnL in both modes. |
| **Fill priority** | If TP and SL are both marketable on the same tick (gap), **SL fills first** — risk before profit. |
| **Bracket management** | Entry fill arms the bracket (TP limit + SL stop). Any non-bracket close (signal flip, manual, stop flatten) cancels remaining bracket orders first. |

### 3.1 Fill Semantics (PaperSimulation)

| Order | Fill rule |
|-------|-----------|
| Market | Fills immediately at current mid ± spread, slippage applied. |
| Limit Buy | Fills when `mid ≤ limit` (fills at mid, i.e. price or better). If already marketable at submission (`mid < limit`), fills immediately — **instant fill**. |
| Limit Sell | Fills when `mid ≥ limit`; instant if already marketable. |
| Stop Sell (SL, LONG) | Triggers when `mid ≤ stop`; then fills as market. |
| Stop Buy (SL, SHORT) | Triggers when `mid ≥ stop`; then fills as market. |
| Gap through SL | Stop triggers with mid already beyond → fills at current mid (worse price). |

---

## 4. Order Construction

The executor builds `OrderPacket`s for the setup geometry:

| Field | Value |
|-------|-------|
| `symbol` | Instance symbol |
| `side` | LONG entry → `Buy`; SHORT entry → `Sell`; TP → opposite of entry; SL → opposite of entry |
| `order_type` | Entry: `Limit` at `entry_mid` (or `Market` when `entry_mode = market_on_ready`); TP: `Limit` at `tp`; SL: `Stop` at `sl` |
| `size` | From Layer ③ allocation sizing (`position_size_units = equity × allocation_pct/100 ÷ entry_mid`), notional clamped to `max_position_size_usd` |
| `reduce_only` | `true` for all bracket/exit orders |

Size for exits is **copied from the open position** (never re-sized) — closing can never fail for lack of margin.

### 4a. Stop Floor & TP Reachability (v11 — quantity-first)

**Problem:** micro-structural zones on `1m` produce SL ≈ 0.7% (inside 1m noise) and TP +2.4% (unreachable — MFE only +0.62% in the verified 7-day window). Both observed in `BT0002` (2 losses, hold 240s/720s).

**Stop floor — floor, don't refuse.** `SetupPlan.effective()` computes `SL = max(zone invalidation, floor)` where floor is:

- `l6_formula` (default): `entry × stop_loss_distance_pct / 100` from the `stop_tf` snapshot's `advisory.stop_loss_distance_pct` (L6: `base_mult×2% + vol/10`, clamp [0.5,15]) — **data-proven**: ZEC T1 replay `0.72%` zone SL → floored to `2%` → TP hit.
- `atr_mult`: `k × ATR(stop_tf)` (`tae.risk.stop_floor_atr_mult`).
- `zone_only`: legacy (no floor).

`min_sl_atr` now **floors** instead of refusing (`entry_blocked`).

**TP cap — keep targets reachable.** `TP = entry ± min(net_rr, max_tp_rr) × SL_distance` where `max_tp_rr` defaults `1.5` (`tae.execution.max_tp_rr`). Prevents the +2.4% micro-targets that never fill.

Both are wired in `SetupPlan::effective()`; `arm_bracket` arms at the floored/capped values. Fees/slippage/funding and `ExecutionBackend` stay mode-neutral.

### 4b. Ladder Roles (v11 — TF-role separation)

One strategy, four slots, four roles. When `micro < 3600` the roles diverge, otherwise they collapse to legacy (all = micro).

| Role | Default (`micro < 1h`) | Feeds |
|------|------------------------|-------|
| `decision_tf` | `macro` | L3 `bias`/`regime`/`market_quality`, L5 `overall_risk`/`market_stance`, `confidence_assessment` |
| `entry_tf` | `micro` | L4 zones + entry timing |
| `stop_tf` | `macro` | SL floor |
| `target_tf` | `micro` | TP zone |

Config: `[workspace.strategies.<name>.ladder_roles] enabled, decision_tf, entry_tf, stop_tf, target_tf` (schema-driven, `StrategyForm` renders enums). Live (`analyzer/mod.rs:3671`) and replay (`backtesting-engine/src/historical.rs:459`) pass the same role-selected snapshots → parity by construction.

---

## 5. Mode-Neutrality Guarantee

For any sequence of ticks, running the executor against `PaperSimulation` produces the exact same ledger math (fees, PnL, funding) as the live path will once `LiveBroker` exists — the only differences are fill prices sourced from the venue instead of the simulated mid, and real order ids. This is the property that makes paper results meaningful as a live predictor.

---


---

## 5b. Venue Implementation Matrix (v7.1 — canonical)

| Aspect | **Hyperliquid** (`LiveBroker`) | **Bitget** (`BitgetLiveBroker`) |
|---|---|---|
| Signing | EIP-712 typed data + secp256k1 ECDSA (wallet private key); domain `HyperliquidSignTransaction`, primaryType `HyperliquidTransaction:Order` | HMAC-SHA256 of `timestamp + method + requestPath + body`; headers `ACCESS-KEY` / `ACCESS-SIGN` / `ACCESS-TIMESTAMP` / `ACCESS-PASSPHRASE` |
| Credentials (`exchange_keys`) | `api_key` = wallet address (`0x…`), `api_secret` = private key hex | `api_key`, `api_secret`, `passphrase` |
| REST base | `https://api.hyperliquid.xyz` (`/info` + `/exchange`) | `https://api.bitget.com` (V2 REST) |
| Place order | `POST /exchange` `{action:{type:"order"}, nonce, signature}` | `POST /api/v2/mix/order/place-order` (signed headers) |
| Stop-loss | stop-market (t=4, `p` = trigger price) | `POST /api/v2/mix/order/place-tpsl-order` (trigger, market execution) |
| Cancel order | `POST /exchange` `{action:{type:"cancel"}}` — `(asset_index, oid)` | `POST /api/v2/mix/order/cancel-order` — `(symbol, orderId)` |
| Fills | `POST /info {type:"userFills"}` — REST polling | `GET /api/v2/mix/order/fills` — REST polling |
| Equity | `POST /info {type:"clearinghouseState"}` → `marginSummary.accountValue` | `GET /api/v2/mix/account/accounts` → equity |
| Symbol mapping | `BTC-USDC → coin "BTC"` + asset index from `meta` | `BTC-USDT → "BTCUSDT"` (strip `-`); `productType` = `USDT-FUTURES` / `USDC-FUTURES` by instance quote |
| Order types | limit (t=1), market (t=2), stop-market (t=4) | limit, market, trigger (tpsl) |
| Reduce-only | `r` flag in the order action | `reduceOnly` param |
| Rate limits | generous; one request per order | ~20 req/s per key (client throttles to 10/s) |

**Live credential flow:** set `EXCHANGE_SECRET_KEY` → add the key via `POST /api/keys` (or the Settings UI) → launch the session in **Execute** mode (wizard) or set `mode = "live"` on an instance in `config.toml` and restart → the daemon decrypts the key at boot/dispatch and the engine routes orders to the venue. The mode is fixed at launch — there is **no** `POST /api/instances/:id/mode` endpoint (removed v7.2). The engine is **globally paper or globally live** (one workspace, one account per exchange); PME/PAE read the shared ledgers regardless of mode. Observe instances run the executor with `dispatch: false` (ghost evaluation): setups are evaluated and surfaced on the radar, but no order is ever submitted.

## 5c. Mode-Neutrality Guarantee (extended)

Running the executor against `PaperSimulation` or either live backend produces the same ledger math (fees, PnL, funding). Live differences are limited to: fill prices sourced from the venue, real order ids, and venue-reported fill sizes. This is the property that makes paper results meaningful as a live predictor on either venue.

## 6. Cross-References

- [TAE Overview §2/§4/§5](03-03-01-tae-overview-spec.md) — layer map, lifecycle, sizing.
- [Paper Trading / Simulation Backend](03-03-05-tae-paper-trading-spec.md) — fills, costs, persistence, recovery.
- [TAE Instance Lifecycle](03-03-06-tae-instance-lifecycle-spec.md) — pause/stop behavior.
- [Risk Calculator](../../../crates/portfolio-supervisor/src/risk_calculator.rs) — canonical sizing + projection math.
