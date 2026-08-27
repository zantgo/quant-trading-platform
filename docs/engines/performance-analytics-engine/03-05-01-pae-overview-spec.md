# Performance Analytics Engine — Overview Specification (v7)

**Version:** 11.0 (2026-08-26) — the v7 release adds the **L5 Backtest layer**: recorded MME decisions are replayed through the unchanged setup executor + unified engine (paper only), and every result carries the full statistical treatment (t-test, Monte Carlo, α = 0.05, edge classification).
**Status:** Specified — implemented; backtest delivered 2026-08-18.
**Engine:** Performance Analytics Engine (PAE)
**Purpose:** This document specifies the boundaries, performance database, scheduled tasks, report templates, and the backtest layer of the Performance Analytics Engine — the engine that evaluates historical trading records to isolate strategy efficacy, quantify the statistical significance of the edge, and answer **"would the setup executor have been profitable over this history?"**

---

## 1. Mission & Boundaries

The PAE is the platform's **retrospective analyst and scoreboard**. It consumes closed-trade ledgers, reconstructs trades, computes statistics and significance tests, maps strategy performance to market regimes, and runs **backtests** by replaying recorded MME decisions through the TAE setup executor in paper mode. It is **read-only with respect to live trading** — it never influences active positions or market interpretation.

```
[Closed Trade Ledgers] ──► PAE ──► [Performance Matrix] ──► [GUI]
[Recorded Decisions]   ──► L5 Backtest ──► [NHST verdict] ──► [GUI]
```

### 1.1 Layer Structure

| Layer | Name | Output |
|-------|------|--------|
| L1 | [Trade Analytics](03-05-02-pae-layer1-trade-analytics.md) | Trade Analytics Matrix |
| L2 | [Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md) | Strategy Analytics Matrix (NHST, grouped by **setup type**) |
| L3 | [Risk Analytics](03-05-04-pae-layer3-risk-analytics.md) | Risk Analytics Matrix |
| L4 | [Performance](03-05-05-pae-layer4-performance.md) | Performance Matrix (regime compatibility) |
| L5 | [Backtest](03-05-06-pae-layer5-backtest.md) | BacktestResult (trades, stats, NHST verdict, equity curve) |


**Dashboard tabs ↔ layers (v7.3).** Overview (landing) · Trades (L1) · Strategy (L2 NHST) · Risk (L3) · Performance (L4, renamed from "Regime Map") · Backtesting (L5) · History · Methodology (cross-cutting last). Observe mode keeps Overview (Edge Validator) · Backtesting · History · Methodology. The significance treatment is config-driven via `[workspace.analytics]` (see [07-07 §2](../../ui-ux/07-07-engine-dashboard-vocabulary.md)).

---

## 2. Performance Database

The PAE reads from the shared telemetry store (see [Database Schema](../../integration-and-api/06-02-database-schema-spec.md)):

| Table | Role |
|-------|------|
| `paper_trades` | Closed paper-trade records. |
| `trade_telemetry_history` | Automated trade telemetry (entry/exit, fees, PnL, ROI, `trigger_source` = setup type). |
| `trade_learning_journal` | Human-annotated trade journal. |
| `portfolio_equity_history` | Equity time-series for drawdown/Sharpe. |
| `market_snapshots` | Completed-candle snapshots with **recorded decision matrices** (`opportunity_json`, `decision_context_json`, `analysis_json`, `advisory_json`, `market_regime`) — the backtest replay source. |
| `backtest_runs` | **Written by PAE** — persisted backtest results (params, summary, NHST stats, trades, equity curve). |
| `performance_matrix_snapshots` | **Written by PAE** — Performance Matrix snapshots at scheduled cadence (default 300 s). |
| `strategy_analytics_history` | **Written by PAE** — Statistical-significance history per setup type. |

---

## 3. Scheduled Tasks

| Task | Cadence | Module |
|------|---------|--------|
| Dashboard stat compilation | On demand (`/api/dashboard/stats`) | `stats_compiler.rs` |
| Performance evaluation | 300 s | `performance_evaluator.rs` |
| Strategy optimization | 3600 s | `strategy_optimizer.rs` |

---

## 4. Statistical contract (edge, alpha, Monte Carlo, null hypothesis)

Every strategy/backtest verdict uses the same tested machinery (`strategy_analytics.rs`):

| Concept | Definition | Wire field |
|---------|-----------|------------|
| **Null hypothesis** | H₀: the setup's true mean PnL ≤ 0 (no edge) vs H₁: mean PnL > 0. | — (documented) |
| **t-statistic** | `mean / (std_dev / √n)` (one-tailed Student t). | `t_statistic` |
| **p-value** | One-tailed t p-value: probability of observing ≥ this profit under H₀. | `p_value` |
| **Monte Carlo p** | 10,000-run deterministic sign-randomization: fraction of shuffled portfolios that beat the actual result. | `p_mc`, `monte_carlo_runs` |
| **Alpha (α)** | Significance bar, **α = 0.05** (named constant). `is_significant = p_value < α && p_mc < α`. | `alpha`, `is_significant` |
| **Edge** | Verdict: `StrongEdge` / `ModerateEdge` / `WeakMarginalEdge` / `NoEdgeNegative` / `InsufficientData` (< 30 trades). | `classification` |

Grouping is by **setup type** (`trigger_source` on telemetry — e.g. `TrendContinuation`), the post-v7 successor of the erased per-policy grouping.

---

## 5. Backtest (L5) in one paragraph

`POST /api/backtest/run` takes `{ symbol, timeframe_secs, from_ms, to_ms, initial_capital }`; the runner loads the **recorded completed snapshots** for that symbol/timeframe/window (each already embeds the full MTF-synthesized decision), feeds them in time order through a **fresh paper `ExecutionEngine` + the unchanged `SetupExecutor`** (mark-to-market → fills → executor tick per snapshot), and returns `{ backtest_id, summary, stats, trades, equity_curve }` where `stats` includes the classic metrics (win rate, profit factor, expectancy, max drawdown) **and the full NHST block** (§4) computed over the *simulated* trades. `GET /api/backtest/:id` returns a persisted run. Full spec: [03-05-06](03-05-06-pae-layer5-backtest.md).

---

## 6. Cross-References

- [PAE Layer 1 — Trade Analytics](03-05-02-pae-layer1-trade-analytics.md)
- [PAE Layer 2 — Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md)
- [PAE Layer 3 — Risk Analytics](03-05-04-pae-layer3-risk-analytics.md)
- [PAE Layer 4 — Performance](03-05-05-pae-layer4-performance.md)
- [PAE Layer 5 — Backtest](03-05-06-pae-layer5-backtest.md)
- [TAE Overview — Setup Executor](../trade-automation-engine/03-03-01-tae-overview-spec.md) — the replayed logic.
- [UI Dashboard Layout](../../ui-ux/07-02-ui-dashboard-layout.md) — Report rendering.
