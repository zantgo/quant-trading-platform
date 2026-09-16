# TAE Ladder Roles — TF-Role Separation for Short-TF Execution

**Version:** 11.3 (2026-09-16) — execution model v11: TF-role separation.
**Status:** Implemented.
**Engine:** Trade Automation Engine (TAE) + Market Monitoring Engine (MME) synthesis.
**Depends on:** [03-02-01 MME Overview](../market-monitoring-engine/03-02-01-mme-overview-spec.md), [03-03-03 Execution](03-03-03-tae-layer2-execution.md), [01-04 Timeframe Model](../../conceptual-foundations/01-04-timeframe-model.md).

---

## 1. Problem

Running a swing-calibrated default strategy (trend≥75, stance Constructive) on the legacy configurable `1m/3m/5m/15m` ladder produced near-zero trades: the micro-TF (1m) read pullback noise inside a macro bull trend as `bias Neutral`, `volatility` as danger, and `market_stance Cautious` — the L4/L6 gate chain vetoed everything while the 15m macro saw a clean trend (`EMA50>200` 81-100% bullish in the verified 7-day window). The v11.1 fixed 10-slot ladder (1 s … 1 h) widens the spread further — the fastest slot (`micro1`, 1 s) is pure tick noise while the slowest (`longterm2`, 1 h) carries the trend — making explicit role separation essential rather than optional.

## 2. Solution — Roles

One strategy, ten fixed ladder slots, four roles. Roles map to slots; when the fastest slot is sub-hour (`micro1 < 3600` — always true on the fixed ladder, since `micro1` = 1 s) the roles diverge, otherwise they collapse to the legacy behavior (all roles = representative/fastest slot).

| Role | Default Slot (extremes mapping) | What it feeds |
|------|-----------------------------|---------------|
| `decision_tf` | `longterm2` (3600 s — slowest) | L3 `bias`/`regime`/`market_quality`, L5 `overall_risk`/`market_stance`, `confidence_assessment` |
| `entry_tf` | `micro1` (1 s — fastest) | L4 opportunity zones + entry timing |
| `stop_tf` | `longterm2` (3600 s) | SL distance floor (`L6 stop_loss_distance_pct` or `stop_tf` ATR) |
| `target_tf` | `micro1` (1 s) | TP zone |

Legacy (`ladder_roles.enabled = false`): all roles = the fastest slot (old behavior, no code fork). On the fixed ladder the sub-hour activation gate is always satisfied, so role separation is effectively always on when `enabled = true`.

**Active-extremes fallback (v11.2).** Role selection operates over the ACTIVE snapshots only — the fastest N of the fixed pool (`[workspace].active_timeframes`, 1..=10, default 5; see [01-04 §2](../../conceptual-foundations/01-04-timeframe-model.md)). Inactive slots produce no snapshot, so a role configured on an inactive slot degrades to the same slot's ACTIVE extreme:

| `active_timeframes` | decision/stop resolve to | entry/target resolve to |
|---------------------|--------------------------|-------------------------|
| 10 | `longterm2` (3600 s — configured extreme present) | `micro1` (1 s) |
| 5 (**default**) | **`slow1`** (30 s — slowest ACTIVE; `longterm2` not running) | `micro1` (1 s — fastest ACTIVE, every N ≥ 1) |
| 1 | **`micro1`** (1 s) — degenerate case: decision == stop == entry == target | `micro1` (1 s) |

The N=1 case collapses all four roles onto one snapshot — role separation is a no-op and the executor behaves like the legacy single-TF path. The shipped defaults stay the string extremes (`longterm2` / `micro1`); the resolution, not the config, follows the active set.

## 3. Config

```toml
[workspace.strategies.default.ladder_roles]
enabled = true          # explicit — default OFF preserves legacy behavior
decision_tf = "longterm2"
entry_tf = "micro1"
stop_tf = "longterm2"
target_tf = "micro1"
```

Schema-driven: `StrategyForm` renders the four enum selects; validation enforces the fixed slot names (`micro1`, `micro2`, `fast1`, `fast2`, `slow1`, `slow2`, `macro1`, `macro2`, `longterm1`, `longterm2`). The shipped defaults are the **extremes mapping** above — decision/stop read the slowest slot, entry/target read the fastest.

## 4. Stop Floor & TP Reachability (quantity-first)

**Stop floor — floor, don't refuse.** `SetupPlan.effective()` computes `SL = max(zone invalidation, stop_floor)` where `stop_floor` is:

- `l6_formula` (default): `advisory.stop_loss_distance_pct` from the `stop_tf` snapshot (L6: `base_mult×2% + vol/100×10`, clamp [0.5,15]) — **data-proven**: ZEC T1 replay `0.72%` zone SL → floored to `2%` → TP hit (was a loss).
- `atr_mult`: `k × ATR(stop_tf)` (strategy `tae.risk.min_sl_atr`).

The old `min_sl_atr` **refuse** (`entry_blocked` when `distance < k×ATR`) is replaced by flooring.

**TP cap:** `TP = entry ± min(net_rr, max_tp_rr) × SL_distance` where `max_tp_rr` defaults `1.5`. Prevents the unreachable +2.4% micro-targets that never fill on 1m (observed T2 MFE +0.62% vs TP +2.43%).

Both are wired in `SetupPlan::effective()`; `arm_bracket` arms the bracket at the floored values. Fees/slippage/funding and `ExecutionBackend` stay mode-neutral.

## 5. Wiring & Parity

- `OpportunityParams`/`DecisionParams`/`RiskParams` carry `LadderRoles`.
- Live: `market-analyzer/src/analyzer/mod.rs:3671`, replay: `backtesting-engine/src/historical.rs:459` — both call `synthesize_cross_tf` with the same role-selected snapshots. **One producer, three sinks** (live/paper/backtest identical).
- TAE: `extract_top_setup` picks zones per `entry_tf`; `tick_idle` floors + caps per `stop_tf`/`target_tf`. `strategy_gates` use `decision_tf` bias/breadth.

## 6. Defaults

| Dial | Old → **New** (becomes the default) |
|------|--------------------------------------|
| `min_net_rr` | 1.0 → **0.5** |
| `max_tp_rr` | — → **1.5** |
| `readiness_ready_min` | 60 → **20** |
| `stance: constructive` | 30 → **55** (neutral 40→50, cautious 60→75, avoid 80→90, aggressive 20→45) |
| `entry_vol_no_entry` | 60 → **80**, breakout 20 → **40** |
| `exec-liquidity baseline` | 30 → **10**, `rvol_high` 2.0→**5.0** |
| `l4 quality_bands` | [85,70,50,30] → **[75,60,45,25]** |

These are now `StrategyConfig::default()` in `crates/config-models/src/strategy.rs` and mirrored in `core-domain`.

## 7. Verification

- Backtest `7d × BTC/ETH/SOL @ 60/180/300/900/3600 s` (the default standalone ladder — `slow2`/`macro1`/`macro2`/`longterm1`/`longterm2`) with new defaults: **≥15 trades**, T1-style entries survive to TP, `symbol:""` bug fixed.
- Paper 1h smoke with `--tae-on`: fills now.
- New CLI `--backtest-gates <id>` table: `READY%`, stance histogram, stop-floor/tp-cap counts.
- Golden vectors unaffected (they don't use `StrategyConfig::default()` for this path).
