# Backtesting Engine — Overview

**Version:** 11.0 (2026-08-26)
**Status:** Implemented (production-ready) — installer-style launcher, standalone multi-symbol runs, progress + cancel, CLI mode
**Engine:** Backtesting Engine (BTE) — the sixth logical engine
**Crate:** `crates/backtesting-engine`
**Docs:** this directory

## 1. Position in the architecture

The BTE is the research engine of the platform. It simulates the **entire
trading stack** — DIE data, MME analysis, TAE execution, PME capital, PAE
statistics — over historical data, using the **same code paths** the live
session runs (`run_tick` parity). The backtest is the whole platform nested
inside itself: same layers, same functions, same sizing, same safety ladder
— only the data source (historical archive) and the fill venue (paper
simulation) differ.

| Session | Engines on the left panel |
|---------|---------------------------|
| Observe | Data Infrastructure · Market Monitor · **Backtesting** · Profile |
| Paper   | Data Infrastructure · Market Monitor · Trade Automation · Portfolio Management · Performance Analytics · Profile |
| Live    | Data Infrastructure · Market Monitor · Trade Automation · Portfolio Management · Performance Analytics · Profile |

The BTE is **observe-only in the UI** (research happens before capital is
deployed).

## 2. The Backtest Launcher (v8.2)

Backtests are launched through an **installer-style wizard** — the same
choices as the live Launch Setup, with one extra choice (archive depth):

| Step | Name | Choices |
|------|------|---------|
| 1 | Environment | Exchange (Hyperliquid / Bitget), settlement currency (USDC/USDT per exchange), starting capital |
| 2 | Instances | One or more instances: ticker + the 4 timeframe dropdowns (the standard tier list, preseeded 1m/3m/5m/15m) + **allocation %** (1–100, Σ ≤ 100 %, ≤ 100 instances) |
| 3 | Historical Data | Archive depth 1–365 days (no date range pickers), per-TF readiness chips, burn-in note, per-exchange max-depth display |
| 4 | Run | Progress bar (Fetching → Warming → Replaying → Analyzing) with % and **Cancel** |

Rules:

- **Standalone**: a backtest does **not** require a running instance. When
  an instance is selected, the launcher is preseeded from it (backward
  compatibility: `instance_id` on the run payload is still accepted).
- **One backtest at a time** (global run lock → 409 on concurrent runs).
- **One backfill per symbol/exchange at a time** (409 while running).
- Runs are **asynchronous**: `POST /api/backtest/run` returns immediately
  with `{ run_id, status }`; progress is polled via
  `GET /api/backtest/progress/:run_id`; `POST /api/backtest/cancel/:run_id`
  aborts the run cleanly.

## 3. The two replay modes

| Mode | Source | Pipeline |
|------|--------|----------|
| `recorded` | Completed `market_snapshots` (recorded MME decisions, ≤ 7-day retention) | Replay through the unchanged setup executor |
| `historical` | `candle_archive` OHLCV (live-warm + backfilled) | Full MME pipeline over archived candles → MTF synthesis → executor |

Both modes feed the **same** `run_tick` session body as the live daemon —
see [08-04 parity contract](08-04-parity-contract.md).

**Historical mode (multi-symbol):** one run replays all launcher instances
simultaneously against a **shared virtual portfolio** — one position per
symbol, global position cap, one equity ledger. The replay tick clock is
each symbol's **smallest ladder timeframe**, merged in timestamp order.

## 4. Exchange-aware depth ceilings (v8.2)

| Exchange | Historical data availability | Effect on the launcher |
|----------|------------------------------|------------------------|
| Bitget | Per-granularity retention (measured 2026-08-21): 1m–30m ≈ 30d, 1H ≈ 45d, 4H ≈ 180d, 12H–1D ≈ 365d | Depth validated per ladder TF against the retention table; the slider clamps and the run fails naming the limiting TF |
| Hyperliquid | Most recent 5,000 candles per TF (`max_candles_per_tf`) | Per-TF max depth = `max_candles_per_tf × tf_secs` (1m ≈ 3.4d, 15m ≈ 52d); the slider clamps and the run validates, **naming the limiting TF** — never silent truncation |

No external historical-data providers are used. The local store is the
SQLite `candle_archive`; replay reads only local data. Hyperliquid deep
history accumulates organically: every completed candle is upserted into
the archive in all session modes (the grow-your-own dataset path). Deep
backtests (> 30 days) need ladders whose smallest TF is 1H or higher on
Bitget — see 08-02 §2 for the burn-in interplay. Fetching canonical 1m
candles and deriving the higher timeframes locally is planned for a future release (Unscheduled).

## 5. Layers (docs map)

| Doc | Layer |
|-----|-------|
| `08-01-bte-overview.md` | this document — boundaries, launcher, modes, ceilings |
| `08-02-archive-and-backfill.md` | the candle archive + the on-demand backfill job (standalone) |
| `08-03-historical-runner.md` | the full-pipeline multi-symbol historical replay |
| `08-04-parity-contract.md` | why backtest = paper, and what live adds |
| `08-05-study-persistence.md` | the data-science persistence schema |

## 6. UI surface

`BacktestingDashboard` (`ui/src/components/backtesting/`):

- Navbar = one tab per simulated engine (DIE · MME · TAE · PME · PAE),
  plus the Study Report, History, and Settings — tab order = layer order.
- The **Overview tab always renders the launcher** (the no-instance state
  no longer blocks backtesting). Selecting an instance preseed the wizard.
- The **Study Report** presents the finished analysis: KPI strip, equity
  curve, drawdown, rolling win-rate, trade P&L histogram, exit-reason
  table (including `end_of_backtest`), and the NHST edge verdict.

## 7. CLI surface (v8.2)

`--mode cli` gains a **Backtest** launch option mirroring the wizard, and
non-interactive flags for automation:

```text
execution-daemon --backtest --exchange hl|bitget --symbols BTC,ETH \
    --tf 60,180,300,900 --depth 180 --capital 1000 --allocation 10
```

Terminal progress bar with Ctrl+C cancel; the final JSON line carries the
run id; results persist to the same tables the GUI History/Study read. See
[01-09 CLI setup flow](../../conceptual-foundations/01-09-cli-setup-flow.md).

## 8. Config surface

```toml
[workspace.backtest]
archive_depth_days = 180     # 1..=365 (M8-validated) — archive retention + backfill depth
warmup_bars = 300            # burn-in before the first valid MTF decision
store_input_bars = true      # persist the exact input candles per run
max_equity_points = 2000     # equity-curve downsampling cap
max_snapshots = 50000        # recorded-replay tick cap

[workspace.backtest.hyperliquid]
max_candles_per_tf = 5000    # v8.2 — the candleSnapshot endpoint window; depth ceiling per TF
page_cap = 1000              # window-bounded candleSnapshot; conservative page cap
rate_limit_delay_ms = 1000
max_pages_per_run = 2000

[workspace.backtest.bitget]
page_cap = 200               # endpoint accepts limit 1..1000
rate_limit_delay_ms = 100
max_pages_per_run = 6000
```

## 9. Cross-references

- [Parity contract](08-04-parity-contract.md)
- [API gateway contract](../../integration-and-api/06-01-api-gateway-contract.md)
- [Database schema](../../integration-and-api/06-02-database-schema-spec.md)
- [Engine dashboard vocabulary](../../ui-ux/07-07-engine-dashboard-vocabulary.md)
