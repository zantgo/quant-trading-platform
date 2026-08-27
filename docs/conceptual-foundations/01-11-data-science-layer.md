# 01-11 — Data-Science Layer

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

## 1. Reason for this update

The platform already separates the three data-science layers correctly —
**Data** (market snapshots, candle archive, trades, equity), **Information**
(52 indicators, PAE statistics, NHST) and **Learning** (edge verdicts,
drift, Study Report) — but the glue a data scientist needs was missing:
no persisted session identity, no file-based capture of TAE/PME/PAE state,
and no backtest chart with entry/exit markers. **No new engine**: this is
cross-cutting work on top of the existing PAE + BTE.

## 2. Session identity

Every boot (web and CLI) creates a persisted, monotonic, never-reused
session row (`sessions` table) — `SESSION #0007` is the join key for every
telemetry row, trade, equity sample, risk event and bound backtest run.
Surfaces: GUI sidebar chip + Launch Setup welcome, CLI header line,
`GET /api/sessions`, `session_id` in `GET /api/session/status`.

## 3. DS export layer (`./ds/`)

One producer, three sinks: **SQLite**, the **WS/GUI**, and **NDJSON files**
under `[data_science].output_path` (default `./ds/`). The live exporter
(`execution-daemon/src/ds_exporter.rs`) consumes the same telemetry stream
the DB logger consumes (fan-out in `main.rs`); backtest artifacts are
written inside `persist_backtest_run` — web and CLI runs share the exact
path. Layout:

```
./ds/
├── sessions/S0007_paper/
│   ├── session.json                       # mode, capital, config snapshot
│   ├── market/BTC-USDT.60.ndjson          # full MarketSnapshot per candle
│   └── trading/
│       ├── trades.ndjson | liquidation_events.ndjson | equity.ndjson
│       ├── activity.ndjson | risk_events.ndjson
│       └── analytics/strategy.ndjson | risk.ndjson | performance.ndjson
└── backtests/BT0042_historical/
    ├── run.json                           # params + summary + NHST stats
    └── trades/equity/portfolio/signals ndjson + input_bars/BTC-USDT.60.ndjson
```

NDJSON = one JSON object per line — directly loadable with
`pandas.read_json(..., lines=True)` / DuckDB for Jupyter workflows.

## 4. Backtest enrichment

- `backtest_trades` gains `ts_entry_secs`, `hold_secs`, `mfe_pct`,
  `mae_pct`, `roi_pct` (MFE/MAE tracked by `mark_to_market` during the
  hold; entry ts recorded runner-side per bar).
- Per-run risk metrics (Sharpe/Sortino/Calmar/Ulcer/VaR95/ES95/drawdown
  duration) via the shared pure function `compute_risk_metrics_from_curve`.
- `GET /api/backtest/:id/input_bars` + the **Chart tab** (candlesticks +
  entry arrows + exit markers colored by PnL, MICRO/FAST/SLOW/MACRO slot
  pills + symbol selector).

## 5. CLI parity

`--sessions`, `--session-report <id>`, `--backtest-show <id>` — headless
JSON payloads that mirror the PAE tabs and the Study Report data (the same
server-computed structs the GUI renders).

## 6. Comparison (the learning surface)

`GET /api/analytics/comparison` + the PAE **Comparison tab**: rows =
sessions + backtest runs; columns = mode, trades, WR, PF, expectancy,
Sharpe, maxDD, verdict badge. Session picker drills into
`GET /api/sessions/:id/analytics`.

## 7. Cross-references

- [D/I/L ontology](./01-12-data-information-learning-ontology.md)
- [DS export schema](../integration-and-api/06-04-ds-export-schema.md)
- [DS surfaces](../ui-ux/07-10-data-science-surfaces.md)
- [CLI↔GUI parity](./01-10-cli-gui-parity.md)
