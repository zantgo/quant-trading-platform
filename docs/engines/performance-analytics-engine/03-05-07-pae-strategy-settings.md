# PAE Strategy Settings — Spec (v9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Engine:** Performance Analytics Engine (PAE)

PAE is the scoreboard — it judges the trades the other engines produce.
The `pae` strategy section is the **verdict bar**: the significance
thresholds, sample floor, grading curve, and cost-of-carry that decide
when the trader says "this model has proven edge" versus "still noise."
The same bar applies identically to live analytics and backtest verdicts
(one `pae` section, two consumers).

## Canonical `pae` section (`default`)

```json
"pae": {
  "verdict": {
    "alpha": 0.05,
    "monte_carlo_runs": 10000,
    "min_trades_for_verdict": 30,
    "min_profit_factor": null,
    "min_expectancy": null,
    "edge_classification": {
      "strong":   { "profit_factor_min": 1.2, "win_rate_min": 50.0, "p_max": 0.01 },
      "moderate": { "profit_factor_min": 1.5, "win_rate_min": 45.0, "p_max": 0.05 },
      "weak":     { "profit_factor_min": 1.0, "p_max": 0.10 }
    }
  },
  "risk_math": {
    "risk_free_rate_pct": 0.0
  },
  "regimes": {
    "min_regime_sample_trades": 5
  }
}
```

## Semantics

- `alpha` — both p-values must fall below it for `is_significant`.
- `monte_carlo_runs` — deterministic seed invariant preserved.
- `min_trades_for_verdict` — below this, `InsufficientData` (checked before
  the classification rows).
- `min_profit_factor` / `min_expectancy` — `null` = off; when set, a group
  failing a floor is demoted to `NoEdgeNegative` **before** the
  classification table is evaluated.
- `edge_classification` — rows evaluated in listed order, null inputs
  skipped; defaults reproduce the historical table verbatim.
- `risk_free_rate_pct` — subtracted in Sharpe/Sortino numerators
  (`R̄ − R_f`); a perp trader sets their realized funding cost.

## Consumers

- Live PAE analytics use the instance's bound strategy bar; unattributed
  legacy trades → `default` bar.
- BTE runs use the run's frozen strategy bar; the bar value is printed on
  every result so two strategies are compared honestly.

## Erasure

`[workspace.analytics]` config section + `AnalyticsConfig` erased (single
source of truth). `PerformanceSettings.svelte` analytics card removed —
moves to the Strategy builder's `pae` verdict editor.
