# BTE Layer 1 — Candle Archive & Backfill

**Version:** 11.0 (2026-08-26)
**Engine:** Backtesting Engine
**Tables:** `candle_archive`, `backfill_jobs`
**Code:** `crates/database-storage/src/queries/archive.rs`,
`crates/backtesting-engine/src/backfill.rs`

## 1. The candle archive

`candle_archive` is a lightweight OHLCV store (Unix seconds, consistent
with `market_snapshots.timestamp`) — the deep-history input of the
historical runner:

| Column | Notes |
|--------|-------|
| exchange, symbol, timeframe_secs, ts_secs | UNIQUE — one row per (venue, pair, TF, candle) |
| open/high/low/close/volume | TEXT decimals |
| source | `live` · `reconstructed` (live pipeline) · `backfill` (on-demand job) |

Two write paths:

1. **Live path (every session mode)** — `insert_snapshot_internal`
   upserts the completed snapshot's OHLCV in the same worker, so the
   archive stays warm as long as the daemon runs (source `live`, or
   `reconstructed` for gap-filled candles).
2. **Backfill path** — the on-demand job below (source `backfill`).

Retention: `prune_candle_archive` deletes rows older than
`[workspace.backtest].archive_depth_days` (1..=365) hourly.

## 2. Exchange limits (what bounds the depth)

| Exchange | Endpoint | Limit behaviour |
|----------|----------|-----------------|
| Hyperliquid | `candleSnapshot` (POST `/info`) | **Most recent 5,000 candles per TF** (`[workspace.backtest.hyperliquid].max_candles_per_tf`) — the platform pages conservatively at `page_cap = 1000` candles/request |
| Bitget | GET `/api/v2/mix/market/candles` | `limit` 1..1000; the platform pages at `page_cap = 200`/request; **per-granularity retention** (measured 2026-08-21): 1m–30m ≈ 30 days, 1H ≈ 45 days, 4H ≈ 180 days, 12H–1D ≈ 365 days — deeper windows return empty pages |

The practical bound per (exchange, TF):

```
max_depth_days(exchange, tf) =
    Hyperliquid: max_candles_per_tf × tf_secs / 86400   (1m ≈ 3.4d, 15m ≈ 52d, 4h ≈ 833d)
    Bitget:      bitget_retention_days(tf_secs)         (1m–30m = 30d, 1H = 45d, 4H = 180d, 12H/1D = 365d)
```

The run payload, the launcher slider, the CLI, and the backfill job all
validate the requested depth against this per-TF ceiling and **fail
loudly naming the limiting TF** — never silent truncation. The coverage
endpoint (`GET /api/backtest/coverage`) reports the ceiling per
(symbol, TF) as `max_depth_secs`.

**Operator note:** deep backtests (> 30 days) require a ladder whose
smallest TF is 1H or higher (Bitget retention), and the burn-in
(`warmup_bars × macro TF`) must stay under the retention ceiling — e.g. a
1H/4H/12H/1D ladder needs `warmup_bars` reduced so the 1D burn-in fits.
Hyperliquid deep history accumulates organically via the live archive
upsert (grow-your-own dataset); canonical-1m derivation for Bitget is
planned for a future release (Unscheduled).

## 3. The backfill job

`POST /api/backtest/archive/backfill` — two accepted payloads:

- Bound form `{ instance_id, depth_days? }` (v8 backward compatibility):
  validates the bound instance (exists + running) and the depth
  (1..=365); rejects a second active job for the instance (409).
- **Standalone form (v8.2)** `{ exchange, symbol, timeframes[], depth_days }`:
  no running instance required; job key = `exchange:symbol`; the same
  exclusivity rule applies.

Both forms:

- Page the exchange backward from `now` to `now − depth_days` for every
  **≥ 1-minute** TF in the requested ladder (sub-minute TFs bypass
  exchange history — HFP-03; their archive coverage comes from the live
  path only).
- Validate the Hyperliquid per-TF ceiling (see §2).
- **Resumable** — the cursor starts just below the earliest archived
  candle, so covered spans cost zero requests.
- **Rate-limited** — config-driven per-page delay; hard page ceiling.
- Progress updates per page into the in-memory registry
  (`BacktestRegistry`) and persists to `backfill_jobs` every 10 pages;
  `GET /api/backtest/archive/progress/:id` serves live progress;
  `POST /api/backtest/archive/cancel/:id` cancels.

## 4. Coverage surface

`GET /api/backtest/coverage?instance_id=` (bound) or
`?symbol=&exchange=` (standalone, v8.2) returns:

- `archive_depth_days` — the configured depth ceiling;
- `burn_in_secs` + `ladder` — the instance ladder the UI derives
  required-coverage math from;
- `snapshots[]` — recorded-snapshot coverage (the recorded mode source);
- `archive[]` — per (symbol, TF): `candle_count`, `earliest_secs`,
  `latest_secs`, `covered_span_secs`, `max_lookback_secs`, `coverage_pct`,
  and the exchange ceiling `max_depth_secs`;
- `backfill_jobs[]` — recent job rows.

## 5. Cross-references

- [BTE overview](08-01-bte-overview.md)
- [Historical runner](08-03-historical-runner.md)
- [Database schema](../../integration-and-api/06-02-database-schema-spec.md)
