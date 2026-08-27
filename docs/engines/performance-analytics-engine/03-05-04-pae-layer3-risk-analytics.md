# PAE Layer 3 — Risk Analytics Layer

**Version:** 11.0 (2026-08-26) — v7: implemented; grouping keyed by setup type.
**Status:** Specified — implemented.
**Engine:** Performance Analytics Engine (PAE)
**Layer:** 3 of 4
**Input Contract:** Portfolio equity history, Strategy Analytics Matrix (L2)
**Output Contract:** Risk Analytics Matrix (drawdown and risk-adjusted performance metrics)
**Purpose:** This document specifies the Risk Analytics Layer — the capital safety analytics system that evaluates historical drawdown patterns, recovery timelines, and risk-adjusted return metrics.

---

## 1. Purpose

The Risk Analytics Layer evaluates how the trading system handles adversity. It analyzes historical equity curves to compute drawdown depths, recovery durations, and risk-adjusted performance ratios (Sharpe, Sortino, Ulcer Index) — providing the objective measures of capital safety that inform strategy optimization.

```
[Portfolio Equity History] ──► RISK ANALYTICS (L3) ──► [Risk Analytics Matrix] ──► [Performance Layer (L4)]
[Strategy Analytics (L2)  ] ──┘
```

---

## 2. Risk Analytics Matrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `maximum_drawdown_pct` | `f64` | Largest historical peak-to-trough equity decline (percentage). |
| `max_drawdown_duration_days` | `f64` | Longest time spent in drawdown before reclaiming the prior peak. |
| `average_drawdown_pct` | `f64` | Mean drawdown depth across all drawdown events. |
| `drawdown_count` | `u32` | Number of distinct drawdown events. |
| `sharpe_ratio` | `f64` | Risk-adjusted return based on standard deviation of daily returns. |
| `sharpe_ratio_log` | `f64` | v10.1 — Sharpe over **log** daily returns (`ln(v_end/v_start)` per UTC day bucket); time-additive and unbiased for skewed curves. `None` on flat/short curves. The configured `pae.risk_math.risk_free_rate_pct` is subtracted from the mean in both Sharpe families (`R̄ − R_f`). |
| `sortino_ratio` | `f64` | Risk-adjusted return using only downside deviation. |
| `ulcer_index` | `f64` | Measure of drawdown depth and duration. |
| `calmar_ratio` | `f64` | Annualized return / maximum drawdown. |
| `daily_volatility` | `f64` | Standard deviation of daily returns. |
| `downside_deviation` | `f64` | Standard deviation of negative daily returns only. |
| `value_at_risk_95` | `f64` | 95% daily Value-at-Risk (worst expected loss in 95% of days). |
| `expected_shortfall_95` | `f64` | Conditional VaR — average loss beyond VaR 95%. |

---

## 3. Drawdown Analysis

### 3.1 Drawdown Computation

A drawdown is measured from each equity peak to the subsequent trough:

$$\text{drawdown\_pct}(t) = \frac{\text{peak\_equity} - \text{equity}(t)}{\text{peak\_equity}} \times 100$$

The maximum drawdown is the largest `drawdown_pct` value across the entire equity history.

### 3.2 Recovery Tracking

A drawdown is considered **recovered** when equity exceeds the prior peak. The drawdown duration is `t_recovery − t_peak`.

---

## 4. Risk-Adjusted Ratios

### 4.1 Sharpe Ratio

$$\text{Sharpe} = \frac{\bar{R} - R_f}{\sigma_R}$$

where $\bar{R}$ = mean daily return, $R_f$ = risk-free rate (typically 0 for crypto), $\sigma_R$ = standard deviation of daily returns.

| Sharpe | Interpretation |
|--------|---------------|
| `> 2.0` | Excellent |
| `1.0 – 2.0` | Good |
| `0.5 – 1.0` | Acceptable |
| `< 0.5` | Poor |
| `< 0` | Negative returns |

### 4.2 Sortino Ratio

$$\text{Sortino} = \frac{\bar{R} - R_f}{\sigma_{\text{downside}}}$$

Uses only downside deviation — penalizes negative volatility while ignoring upside volatility. A Sortino > Sharpe indicates the strategy has skewed positive returns.

### 4.3 Ulcer Index

$$\text{Ulcer} = \sqrt{\frac{\sum_{t=1}^{N} (\text{pct\_drawdown}(t))^2}{N}}$$

Measures both depth and duration of drawdowns. Lower is better.

---

## 5. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Full equity curve** | Ratios are computed from the complete `portfolio_equity_history` — no subsampling. |
| **Annualization** | Sharpe/Sortino are annualized assuming 365 trading days for crypto. |
| **Negative handling** | Sharpe is reported as computed (negative for losing strategies); it is `null` only when σ = 0 (undefined when σ = 0). |

---

## 6. Cross-References

- [PAE Overview](../performance-analytics-engine/03-05-01-pae-overview-spec.md) — Engine boundaries and scheduled tasks.
- [PAE Layer 2 — Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md) — Return distribution source.
- [PAE Layer 4 — Performance](03-05-05-pae-layer4-performance.md) — Regime-mapped performance finalization.
- [Database Schema](../../integration-and-api/06-02-database-schema-spec.md) — `portfolio_equity_history`.
- [Ontology — Performance Analytics](../../conceptual-foundations/01-01-ontology.md) — Conceptual definitions.
