# PAE Layer 2 — Strategy Analytics Layer

**Version:** 11.0 (2026-08-26) — v7: implemented; grouping keyed by setup type.
**Status:** Specified — implemented.
**Engine:** Performance Analytics Engine (PAE)
**Layer:** 2 of 4
**Input Contract:** Trade Analytics Matrix (L1)
**Output Contract:** Strategy Analytics Matrix (grouped performance metrics + statistical significance)
**Purpose:** This document specifies the Strategy Analytics Layer — the strategy-level aggregation and significance testing system that groups closed trades by execution policy and computes mathematical significance against random market noise.

---

## 1. Purpose

The Strategy Analytics Layer determines whether the trading system generates a **statistically significant edge** over random chance. It groups trades by their originating execution policy, computes standard performance metrics, and runs Null Hypothesis Significance Testing (NHST) plus Monte Carlo sign-randomization.

```
[Trade Analytics Matrix] ──► STRATEGY ANALYTICS (L2) ──► [Strategy Analytics Matrix] ──► [Risk Analytics (L3)]
```

---

## 2. Strategy Analytics Matrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `setup_type` | `string` | Originating setup type (e.g. `TrendContinuation`) — the v7 successor of the erased per-policy grouping. |
| `total_trades` | `u32` | Number of closed trades under this policy. |
| `win_count` | `u32` | Profitable trades (net PnL > 0). |
| `loss_count` | `u32` | Losing trades (net PnL < 0). |
| `win_rate` | `f64` | `win_count / total_trades`. |
| `gross_profit` | `Decimal` | Sum of all profitable trade net PnLs (positive). |
| `gross_loss` | `Decimal` | Sum of all losing trade net PnLs **as a positive magnitude** (e.g. an aggregate loss of $250 is stored as `250.0`, not `-250.0`). |
| `profit_factor` | `f64` | `gross_profit / gross_loss` if `gross_loss > 0`, else `None` (serialized as `null` or omitted; all-winning session). When `gross_loss = 0` (no losing trades), the profit factor is mathematically undefined and is reported as "∞" or "N/A" in the GUI. |
| `average_win` | `Decimal` | Mean net PnL of winning trades (positive by construction). |
| `average_loss` | `Decimal` | Mean **magnitude** of losing-trade net PnLs (always stored as a positive value, e.g. an average loss of $10 is `10.0`, not `-10.0`). |
| `avg_win_loss_ratio` | `f64` | `|average_win| / |average_loss|`; since both are positive magnitudes, reduces to `average_win / average_loss`. |
| `expectancy` | `Decimal` | Expected net return per trade: `(win_rate × average_win) − ((1 − win_rate) × average_loss)`. With `average_loss` stored as a positive magnitude, this formula is sign-consistent: the loss term is **subtracted**, matching the standard trading-strategy expectancy formula. |
| `slippage_overhead` | `f64` | Combined drag of slippage and fees as percentage of gross PnL. |
| `t_statistic` | `f64` | Student's T-Test statistic against $H_0$: $\mu = 0$. |
| `p_value` | `f64` | Probability of observing the strategy's returns if $H_0$ is true. |
| `p_mc` | `f64` | Monte Carlo empirical significance — fraction of sign-randomized samples whose mean return meets or exceeds actual performance. |
| `monte_carlo_runs` | `u32` | Number of Monte Carlo sign-randomization samples executed. |
| `is_significant` | `bool` | `true` if `p_value < 0.05` AND `p_mc < 0.05`. |

---

## 2.1 Sign Convention for `average_loss` and `gross_loss`

> **Sign convention.** `average_loss` is stored as a positive **magnitude**. Storing it as the signed arithmetic mean of negative PnL values would corrupt the `expectancy` formula: `(win_rate × avg_win) − ((1 − win_rate) × avg_loss)` would subtract a negative number and *add* the loss magnitude, producing incorrect positive expectancy values for any losing strategy. Worked example with `win_rate = 0.5, avg_win = 20, avg_loss = 10`: `expectancy = 0.5 · 20 − 0.5 · 10 = 5`. The canonical form is `(win_rate × avg_win) − ((1 − win_rate) × avg_loss)` with both inputs positive magnitudes.

> The corrected convention stores both `gross_loss` and `average_loss` as **positive magnitudes**:
>
> - `gross_loss = Σ |pnl|` over losing trades (positive)
> - `average_loss = Σ |pnl| / loss_count` (positive)
>
> **v10.1 long/short symmetry.** `compare_direction_symmetry(trades)` runs a Welch
> two-sample t-test over per-trade `roi_pct` (size-normalized; USD expectancy is context
> only). H0: long and short returns are statistically equal. A verdict is produced only
> with ≥10 trades per side: `SYMMETRIC` / `LONG_BETTER` / `SHORT_BETTER` at α = 0.05
> (two-tailed). Surfaced on the PAE Overview card, the BTE Study Report, and the CLI
> monitor; persisted per backtest as the `dir_*` metric keys.
>
> Under this convention, the `expectancy` formula `(win_rate × avg_win) − ((1 − win_rate) × avg_loss)` is sign-consistent: the loss term is properly subtracted, giving `0.5 × 20 − 0.5 × 10 = 5` (correct). The runtime in `crates/core-domain/src/strategy_analytics.rs` (when implemented) MUST compute `average_loss = average_loss_raw.abs()` before storing, and the persistence layer MUST store the absolute value. This convention is mirrored in the [Database Schema `strategy_analytics_history.expectancy` column](../../integration-and-api/06-02-database-schema-spec.md), which receives the post-correction value.

## 3. Statistical Significance Testing

### 3.1 Null Hypothesis ($H_0$)

The platform evaluates **positive edges** only — a strategy is considered "validated" if it produces a mean return that is statistically significantly **greater than zero**. The null and alternative hypotheses are therefore **one-tailed positive**:

$$H_0: \mu_{\text{returns}} \leq 0 \quad \text{(strategy returns are at most random — no positive edge)}$$

$$H_1: \mu_{\text{returns}} > 0 \quad \text{(strategy generates a statistically significant positive edge)}$$

> **One-tailed test.** Both the parametric $t$-test and the Monte Carlo sign-randomization test are one-tailed in the **positive** direction:

- The T-test reports its $p$-value from a **one-tailed** Student t-distribution: $p = 1 - \Phi_{t,n-1}(\bar{x} / (s/\sqrt{n}))$.
- The Monte Carlo test counts samples where randomized mean $\geq$ actual mean (unchanged — already correct).

The `is_significant` flag therefore evaluates `p_value < 0.05 AND p_mc < 0.05` on aligned one-tailed tests; a strategy with significant positive edge sets `is_significant = true`; a strategy with zero or negative edge keeps `is_significant = false` regardless of $p$-value symmetry.

### 3.2 T-Statistic

$$t = \frac{\bar{x} - 0}{s / \sqrt{n}}$$

where $\bar{x}$ = mean trade return, $s$ = standard deviation of returns, $n$ = number of trades.

The associated **one-tailed** $p$-value is $p = 1 - \Phi_{t,\,n-1}(t)$, where $\Phi_{t,\,n-1}$ is the CDF of the Student t-distribution with $n-1$ degrees of freedom. (A previous two-tailed version would have used $p = 2 \cdot (1 - \Phi_{t,\,n-1}(|t|))$; the one-tailed form is consistent with the positive-edge null hypothesis in §3.1.)

### 3.3 Monte Carlo Sign-Randomization Testing

The null hypothesis $H_0$ is that the strategy has **zero directional edge** — i.e., each trade's outcome is as likely to have been a win as a loss. This is tested by randomizing the **signs** of the realized returns, not their order. (Merely shuffling the order of a fixed set of PnL values leaves the sum and mean unchanged, so a permutation-of-order test would yield $p_{mc} = 1.0$ by construction and is invalid here.)

1. Collect the sequence of closed-trade net PnL values $\{x_1, \dots, x_n\}$.
2. Generate $N$ randomized samples (default: 10,000). For each sample, multiply every trade's PnL by an independent fair-coin sign $\varepsilon_i \in \{-1, +1\}$, producing $\{\varepsilon_1 x_1, \dots, \varepsilon_n x_n\}$.
3. For each randomized sample, compute the mean return.
4. Count randomized samples where the randomized mean ≥ the actual strategy mean.
5. $p_{mc} = \frac{\text{count}}{N}$

A low $p_{mc}$ (< 0.05) confirms the strategy's positive mean return is unlikely to arise from a zero-edge process (random win/loss direction on the same trade magnitudes).

---

## 4. Performance Classification

| Criteria | Classification |
|----------|---------------|
| `profit_factor > 1.2 AND win_rate > 50% AND p_value < 0.01 AND p_mc < 0.01` | **Strong Edge** |
| `profit_factor > 1.5 AND win_rate > 45% AND p_value < 0.05 AND p_mc < 0.05` | **Moderate Edge** |
| `profit_factor ≥ 1.0 AND p_value ≤ 0.10` | **Weak / Marginal Edge** |
| `profit_factor < 1.0 OR p_value > 0.10` | **No Edge / Negative** |
| `total_trades < 30` | **Insufficient Data** |

Evaluate rows in the listed order; a row whose required significance inputs are null is skipped; if `total_trades < 30` → **Insufficient Data** (checked before all rows).

---

## 5. Interaction with Regime Mapping

Strategy performance is segmented by the market regime active at trade entry (from MME Analysis Matrix). This segmentation feeds the [Performance Layer (L4)](03-05-05-pae-layer4-performance.md) for regime-compatibility mapping.

---

## 6. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Statistical rigor** | Both parametric (T-Test) and non-parametric (Monte Carlo) tests are applied. |
| **Minimum sample size** | NHST requires ≥ 30 trades; below this, significance fields are null with a warning. |
| **Deterministic randomization** | Monte Carlo sign-randomization uses a fixed random seed for reproducibility. |

---

## 7. Cross-References

- [PAE Overview](../performance-analytics-engine/03-05-01-pae-overview-spec.md) — Engine boundaries and scheduled tasks.
- [PAE Layer 1 — Trade Analytics](03-05-02-pae-layer1-trade-analytics.md) — Upstream data source.
- [PAE Layer 3 — Risk Analytics](03-05-04-pae-layer3-risk-analytics.md) — Drawdown and risk-adjusted metrics.
- [PAE Layer 4 — Performance](03-05-05-pae-layer4-performance.md) — Regime compatibility mapping.
- [Ontology — Performance Analytics](../../conceptual-foundations/01-01-ontology.md) — Conceptual definitions.
