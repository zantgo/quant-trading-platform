# BTE Layer 3 — Study Persistence (data-science schema)

**Version:** 11.0 (2026-08-26)
**Engine:** Backtesting Engine
**Code:** `crates/database-storage/src/queries/backtest_ds.rs`
**Migration:** `20260820000001_backtest_ds.sql`

## 1. Why normalized tables

Every run persists **both** the legacy JSON columns on `backtest_runs`
(quick History list, unchanged) **and** normalized rows the operator can
query with plain SQL — trades, equity, portfolio samples, decision
snapshots, and metrics — for later data-science work.

## 2. Tables

| Table | Grain | Columns |
|-------|-------|---------|
| `backtest_runs` | one run | + `instance_id`, `mode`, `config_snapshot_json` (legacy JSON columns kept); multi-symbol runs carry the symbols in `params_json` |
| `backtest_trades` | one simulated close | `seq`, `ts_close_secs`, `direction`, `entry_price`, `exit_price`, `size`, `pnl`, `exit_reason` — **v8.2: `exit_reason = "end_of_backtest"`** for the end-of-run force-close; rows are tagged per symbol (`params_json` + the trade row's symbol column) |
| `backtest_equity` | one curve point | `ts_secs`, `equity` (downsampled ≤ `max_equity_points`) |
| `backtest_portfolio` | one tick sample | `equity`, `cash`, `margin_used`, `exposure_pct`, `drawdown_pct`, `positions_open` |
| `backtest_signals` | one decision snapshot | `ts_secs`, `timeframe_secs`, `label`, `kind`, `value` (`decision/bias`, `decision/trade_readiness`, `opportunity/score`) |
| `backtest_metrics` | one key/value | `mode`, `total_trades`, `win_rate`, `profit_factor`, `max_drawdown_pct`, `classification`, `p_value`, `p_mc`, `instance_id` |
| `backtest_input_bars` | one input candle | the exact archived OHLCV per ladder TF (gated by `store_input_bars`) — full reproducibility |

## 3. API surface

| Endpoint | Purpose |
|----------|---------|
| `POST /api/backtest/run` | start a run (returns `{ run_id, status }` immediately; v8.2 async) |
| `GET /api/backtest/progress/:run_id` | phase progress (`fetching/warming/replaying/analyzing`, pct) |
| `POST /api/backtest/cancel/:run_id` | cancel a running backtest |
| `GET /api/backtest/:id/trades?limit=` | paginated trade rows |
| `GET /api/backtest/:id/equity` | equity curve rows |
| `GET /api/backtest/:id/portfolio` | capital/exposure/drawdown samples |
| `GET /api/backtest/:id/signals` | decision snapshots |
| `GET /api/backtest/:id/metrics` | summary + NHST key/values |
| `GET /api/backtest/list?limit=` | runs with `instance_id` + `mode` |

## 4. Frontend consumption

The Study Report renders the finished analysis from these rows (KPI strip,
equity curve, drawdown, rolling win-rate, P&L histogram, exit-reason
table — including `end_of_backtest` — edge verdict); the DIE / MME / TAE /
PME / PAE tabs render the per-engine breakdowns of the same study.

## 5. Cross-references

- [BTE overview](08-01-bte-overview.md)
- [Database schema](../../integration-and-api/06-02-database-schema-spec.md)
- [API gateway contract](../../integration-and-api/06-01-api-gateway-contract.md)
