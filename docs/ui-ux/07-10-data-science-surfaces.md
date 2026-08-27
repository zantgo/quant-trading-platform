# 07-10 — Data-Science Surfaces

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

## 1. Session chip

The engine sidebar brand row shows `SESSION #0007` (from
`GET /api/session/status` → `session_id`); the Launch Setup welcome screen
and the CLI header line carry the same number. Backtest surfaces use the
`BTxxxx` sequence.

## 2. Backtesting — Chart tab

New `chart` tab in the BTE navbar (after Study Report): input candles as
candlesticks, entry **arrows** at `ts_entry_secs` (LONG = below-bar up
arrow, SHORT = above-bar down arrow), exit **markers** colored by PnL sign
with the exit-reason label. Controls: `MICRO/FAST/SLOW/MACRO` slot pills
(mapped from the run's distinct timeframe ladder; the `Ns` suffix shows
the resolved seconds) + a symbol selector for multi-symbol runs.

## 3. PAE — Comparison tab

New `comparison` tab (before History; present in every mode): rows =
persisted sessions + backtest runs; columns = mode, trades, WR %, PF,
expectancy, Sharpe, maxDD %, verdict badge (StrongEdge/ModerateEdge/
WeakMarginalEdge/NoEdgeNegative/InsufficientData color-coded). A session
picker loads `GET /api/sessions/:id/analytics` (snapshot + trade counts
plus the PAE stats) above the table.

## 4. Observe-mode collapse

Comparison stays visible in observe mode (data-bearing); the backtest
Chart tab is only reachable with a finished run (paper/live modes).
