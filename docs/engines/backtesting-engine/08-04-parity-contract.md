# BTE Parity Contract — why backtest = paper

**Version:** 11.0 (2026-08-26)
**Engine:** Backtesting Engine
**Code:** `crates/portfolio-supervisor/src/execution/session_tick.rs`

## 1. The mechanism: one session body

Every tick of the TAE loop — live session **and** backtest — runs the
**same function** through the **same code**:

```rust
portfolio_supervisor::execution::session_tick::run_tick(
    engine, executor, instance_id, symbol, snaps, mid, ctx,
    live_fills,           // Some(fills) only for live-mode engines
    capture_last_close,   // true only for the backtest trade log
)
```

| Session | tick source | dispatch | live_fills |
|---------|-------------|----------|------------|
| Observe (daemon) | live WS snapshots | false | None |
| Paper (daemon) | live WS snapshots | true | None (paper evaluates) |
| Live (daemon) | live WS snapshots | true | Some(broker fills) |
| Backtest recorded | `market_snapshots` replay | true | None |
| Backtest historical | archived candles → full MME pipeline | true | None |

Fills, sizing, safety ladder and fees are the same functions on the same
config structs (`FeesConfig`, `MinimalTaeConfig`, leverage) — there is no
per-mode config fork. A backtest result therefore equals the paper result
**by construction**, not by convention.

## 2. What this guarantees

- Identical setup extraction (`extract_top_setup`), identical invalidation
  semantics, identical allocation sizing (`equity × allocation_pct / 100`).
- Identical `PaperSimulation` fill math (deterministic in mid).
- Identical NHST treatment (seeded Monte Carlo — deterministic).
- **v8.2 — identical safety ladder**: the historical runner carries a
  simulated `SafetyManager` per instance fed by replayed equity, so
  `DRAWDOWN_STOP`/`SUSPENDED` block new entries in the backtest exactly
  like paper.
- **v8.2/v10.1 — identical funding**: funding settles at simulated 8h
  boundaries of replay time in BOTH runners (the recorded replay gained
  the clock in v10.1). Direction-aware perp convention:
  `settlement = −dir_sign × notional × rate` (positive rate → longs pay,
  shorts receive), the same settlement paper applies every 8h of
  wall-clock. Backtests use the config rate; live may override with the
  ingested venue rate (`settle_funding_with_rate`).
- **v10.1 — identical slippage**: every simulated fill prices at
  `mid × (1 ± (half-spread + slippage_bps))`, limit-clamped — the bound
  strategy's `tae.execution.slippage_bps` dial, shared by paper, live,
  historical and recorded. Per-trade costs persist to
  `backtest_trades.slippage_bps / commission_fees / funding_fees`.
- **v8.2 — complete ledger**: open positions are force-closed at the
  final replayed candle close with `exit_reason = "end_of_backtest"`, so
  backtest statistics cover every trade — no dangling unrealized
  positions.
- **v10 — identical strategy dials**: historical and recorded replays
  resolve the run's bound strategy JSON and pass it into the executor
  tick (`recorded::run_backtest` gained the strategy param — previously
  defaults). The lifecycle-hardening dials (posture, re-pricing, ratchet,
  entry/exit strictness) replay exactly like paper.
- **v10 — strategy intake gates in historical replay**: the historical
  runner evaluates the same `evaluate_intake_gates` /
  `evaluate_portfolio_gates` functions the daemon applies live, on the
  simulated portfolio — breadth = the cross-symbol bias share, the
  systemic veto is inert (no L7 synthesis in replay, documented), and
  exposure/margin gates read the replayed engine ledger. The recorded
  replay keeps gates off (replay parity — recorded decisions re-run
  unchanged).

## 3. Documented boundaries

Live mode runs the same logic against a real venue, so it differs in ways
a backtest cannot model:

- venue fills, partial fills, and latency;
- real funding timestamps and venue fees;
- connection gaps and reconstruction.
- **Tick granularity**: the daemon ticks every second reading the latest
  snapshot per TF; the historical replay ticks once per completed candle
  of each symbol's smallest ladder TF, and fills evaluate at candle close
  (no intrabar path).
- **Breadth semantics**: live breadth comes from the L7 Overview Matrix
  (`breadth_pct`); the historical replay approximates it as the share of
  run symbols with a directional bias — a single-symbol run is 100 % when
  directional and 0 % when neutral.

Paper↔backtest parity is exact; live is "same logic, real venue".

## 4. Enforced in tests

- `session_tick` unit tests: deterministic, dispatch-gated ticks.
- Recorded + historical runners: deterministic reruns (byte-identical
  trades/equity), burn-in respected, no future-data consumption,
  multi-symbol timestamp ordering.
- Parity fixtures: safety-ladder blocking, funding settle, end-of-run
  force-close, and paper↔backtest order-size identity.
- **v10 gate fixtures**: breadth-floor and margin close-only strategies
  block entries in the historical replay (`entry_blocked` logged,
  deterministic under the gate); the executor-level market-filter block
  unit test covers the reason-surfacing path.
- The daemon loop body is the same `run_tick` call — the existing
  failover/liquidation/engine suites are the regression net for the
  refactor.

## 5. Cross-references

- [BTE overview](08-01-bte-overview.md)
- [Historical runner](08-03-historical-runner.md)
- [TAE overview](../trade-automation-engine/03-03-01-tae-overview-spec.md)
