# PAE Layer 5 — Backtest (moved to the Backtesting Engine)

**Version:** 11.0 (2026-08-26) — delivered with the v7 PAE release.
**Status:** MOVED — as of v8 the backtest layer lives in the **Backtesting
Engine** (`crates/backtesting-engine/src/recorded.rs` +
`historical.rs`; see
`docs/engines/backtesting-engine/08-01-bte-overview.md`). This document is
kept for historical reference; the recorded-replay contract below still
describes `mode: "recorded"` exactly.

## 1. Design principle: record today, replay tomorrow

Every completed candle snapshot already embeds the **full MTF-synthesized decision** (opportunity profiles with entry/SL/TP zones, decision context, analysis bias, advisory). Since the v7 migration, these matrices are **persisted** to `market_snapshots` (`opportunity_json`, `decision_context_json`, `analysis_json`, `advisory_json`, `market_regime`) by the WAL snapshot logger.

The backtest therefore replays **exactly what the MME recommended at the time**, through **exactly the same code the live executor runs**:

```
recorded snapshots (ascending ts) ──► [ExecutionEngine (fresh, seeded capital)]
                                          │  mark_to_market(mid)
                                          │  evaluate_order_fills(mid)
                                          ▼
                                    [SetupExecutor.tick(snapshot, mid)]
                                          │  (unchanged live logic)
                                          ▼
                                    simulated closes ──► stats + NHST + equity curve
```

**Why this is honest:** the executor consumes the same `MarketSnapshot` wire format live and in backtest; the only difference is the source (DB replay vs. live buffers) and the destination (statistics vs. persisted telemetry). No decision logic is re-implemented.

## 2. Replay contract

- **Source:** `market_snapshots` rows for `(symbol, timeframe_secs)` with `timestamp ∈ [from_secs, to_secs]` (v8: the API accepts ms and converts), reconstructed rows excluded, ordered ascending. Single timeframe per run — each recorded snapshot already carries the MTF decision.
- **Bound:** the window is capped (≤ 50,000 snapshots per run, now `[workspace.backtest].max_snapshots`) to keep runs synchronous and fast.
- **Engine:** a fresh `ExecutionEngine` with the configured fee/slippage/leverage config, seeded with `initial_capital`. Paper fills only — the same `PaperSimulation` backend as live paper trading.
- **Executor:** the same `SetupExecutor` (same `extract_top_setup`, same invalidation, same gates) driven once per snapshot with the snapshot's `mid_price`; the loop body is the shared `run_tick` (v8 parity contract — see `docs/engines/backtesting-engine/08-04-parity-contract.md`).
- **Determinism:** identical inputs ⇒ identical results (no wall clock, no randomness in the executor path).
- **Isolation:** backtest runs never write trade telemetry, activity logs, or open state; they only write the `backtest_runs` row + the normalized DS tables.
- **Validation:** an empty window fails loudly with `400 not_enough_data` + coverage numbers (no silent zero-trade 200s).

## 3. Result shape (`BacktestResult`)

| Block | Fields |
|-------|--------|
| `params` | symbol, timeframe_secs, from_ms, to_ms, initial_capital |
| `summary` | total trades, win count, loss count, win rate, gross profit, gross loss, profit factor, expectancy, max drawdown %, avg R:R |
| `stats` (NHST — same machinery as L2) | `t_statistic`, `p_value`, `p_mc`, `monte_carlo_runs` (10,000), `alpha` (0.05), `is_significant`, `classification` (edge verdict; `InsufficientData` under 30 simulated trades) |
| `trades` | simulated closes: timestamp, side, entry, exit, size, PnL, fees, exit reason (tp/sl/invalidated_signal/manual/stop_flatten) |
| `equity_curve` | points `{ ts, equity }` sampled per close (and per fill for open positions) |

## 4. API

| Endpoint | Behavior |
|----------|----------|
| `POST /api/backtest/run` | Body `{ symbol, timeframe_secs, from_ms, to_ms, initial_capital, instance_id?, mode? }`. `mode` is `"recorded"` (this doc) or `"historical"` (BTE deep-history). Runs synchronously (global lock → 409), persists the `BacktestResult` to `backtest_runs` + the DS tables, returns it with `backtest_id`. |
| `GET /api/backtest/:id` | Returns the persisted run (or 404). |
| `GET` | `/api/backtest/:id/trades` | Normalized trade rows (data-science tables). |
| `GET` | `/api/backtest/:id/equity` | Normalized equity curve rows. |
| `GET` | `/api/backtest/:id/portfolio` | Capital/exposure/drawdown samples. |
| `GET` | `/api/backtest/:id/signals` | Per-tick decision snapshots. |
| `GET` | `/api/backtest/:id/metrics` | Summary + NHST key/values. |

v8 coverage: `GET /api/backtest/coverage` now serves the extended
`{ snapshots, archive, backfill_jobs }` shape (see
`docs/engines/backtesting-engine/08-02-archive-and-backfill.md`).

## 5. Trader-facing interpretation

The backtest answers two questions at once:

1. **"Would the setup executor have been profitable over this window?"** — win rate, profit factor, expectancy, max drawdown, equity curve, trade-by-trade log with exit reasons.
2. **"Is that result real or luck?"** — the NHST verdict: if `is_significant` (both p-values < α = 0.05 over ≥ 30 simulated trades), the edge is statistically distinguishable from random; otherwise the result may be noise. The dashboard renders this as an **edge verdict card** (e.g. "Significant at α = 0.05 — t-test p = 0.003, Monte Carlo p = 0.001, 10,000 runs").

## 6. Cross-References

- [PAE Overview](03-05-01-pae-overview-spec.md) — layer map + statistical contract.
- [PAE Layer 2 — Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md) — the shared NHST machinery.
- [TAE Overview](../trade-automation-engine/03-03-01-tae-overview-spec.md) — the replayed executor + engine.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `market_snapshots` matrix columns, `backtest_runs`.
