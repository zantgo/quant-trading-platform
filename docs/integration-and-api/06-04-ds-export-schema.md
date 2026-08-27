# 06-04 — DS Export Schema (`./ds/`)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

## 1. Config

`[workspace.data_science]` — `enabled` (default true), `output_path`
(default `./ds`), `capture_market` / `capture_trading` /
`capture_analytics` toggles, `flush_interval_secs` (default 5).

## 2. Directory layout + identifiers

- Sessions: `sessions/S{session_id:04}_{mode}/` (e.g. `S0007_paper`).
- Backtests: `backtests/BT{backtest_id:04}_{mode}/` (e.g. `BT0042_historical`).

All files are **NDJSON** (one JSON object per line, append-mode, flushed
on the configured interval) except `session.json` / `run.json` (pretty).

## 3. Session files

| File | Record |
|---|---|
| `session.json` | `session_id`, `mode`, `exchange`, `currency`, `portfolio_capital_usd`, `started_at_ms`, `config_snapshot` |
| `market/{SYMBOL}.{timeframe_secs}.ndjson` | full `MarketSnapshot` (all MME matrices) per completed candle |
| `trading/trades.ndjson` | symbol, direction, entry/exit prices+ts, size, pnl, roi, allocation, trigger |
| `trading/liquidation_events.ndjson` | exchange, symbol, side, price, size_usd, ts, venue_order_id |
| `trading/equity.ndjson` | `portfolio_equity_history` rows (id, ts, total, cash, unrealized) |
| `trading/activity.ndjson` | `automation_activity` rows |
| `trading/risk_events.ndjson` | `risk_control_events` rows |
| `trading/analytics/strategy.ndjson` | `strategy_analytics_history` rows (WR/PF/expectancy/NHST) |
| `trading/analytics/risk.ndjson` | `risk_analytics_history` rows (Sharpe/Sortino/VaR…) |
| `trading/analytics/performance.ndjson` | `performance_matrix_summaries` rows |

Session-scoped tables are filtered by `session_id`; the analytics tables
are appended by id-offset (restart-safe).

## 4. Backtest files

| File | Record |
|---|---|
| `run.json` | `backtest_id`, `mode`, `params`, `summary`, `stats` (NHST block) |
| `trades.ndjson` | enriched `BacktestTrade` — close ts, entry ts, hold, MFE/MAE, ROI, pnl, exit_reason |
| `equity.ndjson` | `{ ts_secs, equity }` |
| `portfolio.ndjson` | `DsPortfolioPoint` rows |
| `signals.ndjson` | `DsSignal` rows |
| `input_bars/{SYMBOL}.{timeframe_secs}.ndjson` | OHLCV candles the run consumed |

## 5. Parity guarantee

The live exporter consumes the same telemetry channel the DB logger
consumes; backtest files are written by `persist_backtest_run` (shared by
web and CLI runs). Rows are byte-identical to the payloads the GUI renders.
