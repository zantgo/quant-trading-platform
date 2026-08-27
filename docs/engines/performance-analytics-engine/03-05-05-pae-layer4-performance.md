# PAE Layer 4 — Performance Layer

**Version:** 11.0 (2026-08-26) — v7: implemented; grouping keyed by setup type.
**Status:** Specified — implemented.
**Engine:** Performance Analytics Engine (PAE)
**Layer:** 4 of 4
**Input Contract:** Trade Analytics (L1), Strategy Analytics (L2), Risk Analytics (L3), historical MME regime logs
**Output Contract:** Performance Matrix (complete regime-mapped performance profile)
**Purpose:** This document specifies the Performance Layer — the top-level synthesis layer that correlates strategy performance against historical market regimes, producing the definitive performance profile and strategy optimization guidance.

---

## 1. Purpose

The Performance Layer is the PAE's **final synthesis stage**. It combines trade-level analytics, strategy-level significance, and risk-adjusted metrics with the market regime conditions active during each trade to produce the **Performance Matrix's regime_compatibility section** — the definitive map of where the strategy excels and where it degrades.

```
[Trade Analytics (L1)   ] ─┐
[Strategy Analytics (L2)] ─┼──► PERFORMANCE LAYER (L4) ──► [Performance Matrix]
[Risk Analytics (L3)    ] ─┘                                  │
[MME Regime Logs        ] ─┘                                  └──► [GUI / optimization feedback]
```

---

## 2. Performance Matrix Schema

| Field | Type | Description |
|-------|------|-------------|
| `setup_type` | `string` | Originating setup type (v7 grouping key). |
| `total_trades` | `u32` | Total closed trades under this policy. |
| `overall_profit_factor` | `f64` | Aggregate profit factor. |
| `overall_expectancy` | `Decimal` | Expected net return per trade. |
| `overall_sharpe` | `f64` | Annualized Sharpe ratio. |
| `overall_sortino` | `f64` | Annualized Sortino ratio. |
| `max_drawdown_pct` | `f64` | Maximum historical drawdown. |
| `regime_compatibility` | `RegimeCompatibility` | Per-regime performance grid (§3). |
| `regime_strength_summary` | `RegimeStrength[]` | Ranked list of most/least compatible regimes. |
| `optimization_recommendations` | `string[]` | Metric-driven parameter tuning suggestions. |
| `overall_rating` | `Rating` | `STRONG_EDGE` / `MODERATE_EDGE` / `WEAK_EDGE` / `NO_EDGE` / `INSUFFICIENT_DATA`. |
| `last_evaluated_at` | `u64` | Timestamp of the most recent evaluation. |

---

## 3. the Performance Matrix's regime_compatibility section

The `RegimeCompatibility` grid maps each MME market regime to the strategy's performance within that regime. *Illustrative example below — `ACCUMULATION`, `DISTRIBUTION`, and `TRANSITION` rows follow the same schema.*

| Regime | Trades | Win Rate | Profit Factor | Avg Return | Sharpe | Mapping |
|--------|--------|----------|---------------|------------|--------|---------|
| `TRENDING_BULL` | 45 | 72% | 2.8 | +1.2% | 2.1 | `STRONG` |
| `TRENDING_BEAR` | 12 | 25% | 0.6 | −0.8% | −0.5 | `AVOID` |
| `RANGE` | 30 | 48% | 1.3 | +0.3% | 0.7 | `MARGINAL` |
| `EXPANSION` | 18 | 65% | 1.9 | +0.9% | 1.4 | `FAVORABLE` |
| `CONTRACTION` | 8 | 35% | 0.8 | −0.4% | −0.2 | `AVOID` |

### 3.1 Compatibility Classifications

| Label | Interpretation |
|-------|---------------|
| `STRONG` | Primary regime — highest win rate, profit factor, and Sharpe. |
| `FAVORABLE` | Positive performance but less consistent than `STRONG`. |
| `MARGINAL` | Near-zero edge; deploy with caution. |
| `AVOID` | Negative expectancy — strategy should be disabled in this regime. |

---

## 4. Optimization Recommendations

The Performance Layer generates metric-driven recommendations:

| Condition | Recommendation |
|-----------|---------------|
| Win rate > 60% but profit factor < 1.2 | "Average loss exceeds average win. Tighten stop-loss or reduce risk per trade." |
| Win rate < 40% but profit factor > 2.0 | "Trend-following profile: losses are small, wins are large. Increase risk per trade within drawdown limits." |
| Strong performance in `TRENDING_BULL`, weak in `TRENDING_BEAR` | "Consider directional filter: only trigger longs in bullish regimes." |
| Regime-specific drawdown > 2× average | "Apply tighter position sizing in [regime]." |
| Slippage overhead > 15% of gross PnL | "Review order routing — excessive execution friction." |

---

## 5. Dual-Mode Analytics

Per [Global Architecture — §4.4](../../conceptual-foundations/01-02-global-architecture.md), the PAE supports retroactive analysis of headless CLI runs:

1. During CLI operation, trades and equity snapshots are persisted to the database.
2. When the GUI is later launched, the PAE reads the full historical record.
3. Trade reconstruction, significance tests, and regime mapping run against the complete history.
4. The Performance Matrix reflects all trades, regardless of whether they were executed in GUI or CLI mode.

---

## 6. Scheduled Evaluation

Per [PAE Overview](../performance-analytics-engine/03-05-01-pae-overview-spec.md) §3:

| Task | Cadence |
|------|---------|
| Performance evaluation | 300 s (5 min) |
| Strategy optimization | 3600 s (1 h) |

The regime compatibility matrix is updated on every performance evaluation cycle.

---

## 7. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Regime traceability** | Every trade's entry regime is recorded at trade time from the MME Analysis Matrix — not retroactively inferred. |
| **Minimum regime sample** | Regimes with < 5 trades are marked `Insufficient Data`. |
| **Deterministic scoring** | Identical trade histories produce identical Performance Matrices. |

---

## 8. Cross-References

- [PAE Overview](../performance-analytics-engine/03-05-01-pae-overview-spec.md) — Engine boundaries and report templates.
- [PAE Layer 1 — Trade Analytics](03-05-02-pae-layer1-trade-analytics.md) — Trade-level reconstruction.
- [PAE Layer 2 — Strategy Analytics](03-05-03-pae-layer2-strategy-analytics.md) — Significance testing.
- [PAE Layer 3 — Risk Analytics](03-05-04-pae-layer3-risk-analytics.md) — Risk-adjusted metrics.
- [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) — Regime classifications.
- [Global Architecture — §4.4](../../conceptual-foundations/01-02-global-architecture.md) — Dual-mode analytics.
- [Ontology — Performance Analytics](../../conceptual-foundations/01-01-ontology.md) — Conceptual definitions.
