# 01-12 — Data → Information → Learning Ontology

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

## tier-mapping

The tables below map every D/I/L artifact (tier-mapping anchors: D =
`market_snapshots`/`backtest_trades`/`ds/*.ndjson`; I = indicators + PAE
statistics + `backtest_metrics`; L = edge verdicts + drift + Study Report).

## 1. Two orthogonal axes

The existing ontology (01-01 Ch.12/13) describes an **aggregation axis**
(ticks → candles → symbol → systemic → historical). This document adds the
orthogonal **epistemic axis**: the status of an artifact along
Data → Information → Learning. Both axes coexist: a raw tick is D×L1;
RSI is I×L2; an edge verdict is L×L5.

## Data (D)

Raw observations. Immutable once written, zero interpretation.
**Owner: DIE, TAE/PME write.**

| Artifact | Storage |
|---|---|
| Candle archive (`candle_archive`) | SQLite + `ds/backtests/*/input_bars/` |
| Market snapshots (`market_snapshots` + JSON matrices) | SQLite + `ds/sessions/*/market/*.ndjson` |
| Liquidation events | SQLite + `ds/.../trading/liquidation_events.ndjson` |
| Trades (`trade_telemetry_history`, `paper_trades`, `backtest_trades`) | SQLite + `ds/.../trading/trades.ndjson` |
| Equity (`portfolio_equity_history`, `backtest_equity`) | SQLite + `ds/.../trading/equity.ndjson` |
| Executor activity + risk events | SQLite + `ds/.../trading/{activity,risk_events}.ndjson` |
| Backtest input bars + signals | SQLite + `ds/backtests/*/` |

## Information (I)

Deterministic statistics/features computed from D with semantic meaning.
**Owner: MME (features), PAE, BTE.**

| Artifact | Storage |
|---|---|
| 52 indicators + signals + MarketContext | snapshot JSON (D row) |
| Dashboard stats (WR, PF, expectancy) | `stats_compiler` |
| Sharpe/Sortino/Calmar/Ulcer/VaR95/ES95 | `risk_analytics_history`, `backtest_metrics` |
| Strategy analytics rows (NHST: t, p, p_mc) | `strategy_analytics_history` |
| Regime matrices / performance summaries | `performance_matrix_*` |
| Backtest coverage tables | `backtest_coverage` |

## Learning (L)

Verdicts/conclusions with evidence — actionable, carry decisions.
**Owner: PAE, BTE.**

| Artifact | Evidence carried |
|---|---|
| Edge classification (`StrongEdge`/`ModerateEdge`/`WeakMarginalEdge`/`NoEdgeNegative`) | p, p_mc, sample size, grading curve |
| InsufficientData | `min_trades_for_verdict` |
| Drift verdict (live vs backtest) | sample counts both sides |
| Study Report verdict banner | full NHST block |

## Invariants

1. **Lineage** — every I artifact cites its D source + params; every L
   artifact cites its I inputs + thresholds. No L without I, no I without D
   (enforced e.g. by `min_trades_for_verdict`).
2. **Reproducibility** — I and L are recomputable from D + the session's
   `config_snapshot_json` (why sessions snapshot the full workspace config).
3. **Storage boundary** — D stored raw at high fidelity (SQLite + `ds/`);
   I stored as computed outputs (analytics tables, `backtest_metrics`);
   L stored as small verdict records that always carry evidence.
4. **Temporal** — D append-only; I recomputed on cadence (300 s / 3600 s
   loops); L re-verified when new I passes the bar.
5. **Parity** — GUI, CLI, and `ds/` files render the same D/I/L artifacts
   (one producer, three sinks).

## Tier lifecycle through the engines

- **DIE** produces D (candles, snapshots) and MME consumes them.
- **MME** produces I (indicators, context, matrices) stamped onto D rows.
- **TAE/PME** write D (trades, equity, activity).
- **PAE/BTE** produce I (statistics) and L (verdicts).

## Cross-references

- [DS layer](./01-11-data-science-layer.md)
- [Ontology](./01-01-ontology.md) Ch.12/13 (aggregation axis)
- [DS export schema](../integration-and-api/06-04-ds-export-schema.md)
