# Price Trend Sharpe

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

## Fundamental Mechanism

The **Price Trend Sharpe** measures the relative smoothness and return consistency of the raw price series on a single timeframe: the annualized Sharpe ratio of price **log returns** over the trailing **300-bar** window. It answers "is this price trend smooth and persistent, or noisy and unreliable?" — a high positive value (e.g. `+2.40`) indicates a highly consistent, low-noise price trend suitable for trend-riding; values near zero indicate chop; strongly negative values indicate a smooth *down* trend.

$$
\text{Price Log Return}_t = \ln\left(\frac{\text{Close}_t}{\text{Close}_{t-1}}\right)
$$

$$
\text{Price-Trend Sharpe} = \frac{\text{mean}(\text{Price Log Returns})}{\sigma(\text{Price Log Returns})} \times \underbrace{\sqrt{\frac{86\,400}{\text{timeframe\_secs}} \times 365}}_{\text{Annualization Factor}}
$$

- **Window:** trailing **300** completed candles — equal to the canonical `[candle_buffer] size`, so the indicator reaches `Live` exactly when the pipeline buffer fills (no lifecycle lock; `bars_required = 300`).
- **Annualization:** `sqrt(candles_per_day × 365)` — the crypto-native 365-day convention shared with HV (`04-02-29-hv.md`).
- **Returns `None`** when fewer than 2 samples exist or the return standard deviation is ≈ 0 (perfectly flat series — division guard).

## Data Source

The value is computed by the pipeline from its rolling **`close_history`** window (real completed candles only — synthetic doji/idle buckets never enter the window, matching the PRI-06 history-continuity convention) and injected into the indicator map in `crates/market-analyzer/src/analyzer/normalize.rs` after `normalize_all`. Warm-started pipelines replay the buffer from historical closes (≥ 1 min TFs fetch exactly `[candle_buffer] size` candles on boot), so the reading is available at the first live close.

The indicator is **close-only** (`updates_on_shadow: false`): shadow ticks never carry it — the frontend preserves the last completed-candle value via its per-key merge. On shadow frames the lifecycle row reports `Live` with `feed_state: Live` (v6.10.21 — the merged value is genuinely current; `WaitingFeed` is reserved for rows whose upstream feed truly hasn't delivered). The value shown is the merged last-completed reading.

**v6.10.21 hardening:** the annualized output is clamped to ±20 (`SHARPE_MAX_ABS`) and returns `None` when the log-return σ < 1e-9 (`SHARPE_STDDEV_FLOOR`) — the Sharpe `σ → 0` pathology on near-flat series can no longer surface absurd values (e.g. −117) on the wire or the dashboard.

## Interpretation

| Sharpe | State label | Meaning |
|--------|-------------|---------|
| ≥ +2.0 | `STRONG_POSITIVE_SHARPE` | Highly consistent, low-noise uptrend. |
| (0, +2.0) | `POSITIVE_SHARPE` | Positive drift but noisier. |
| (−2.0, 0] | `NEGATIVE_SHARPE` | Negative drift; chop or weak downtrend. |
| ≤ −2.0 | `STRONG_NEGATIVE_SHARPE` | Highly consistent downtrend. |

## Signals

None. `signal_types = []` — the indicator is a data-only regime statistic; it never emits a discrete `IndicatorSignal`.

## Normalization

```
raw_value  = annualized Sharpe (signed f64, 2-dp display)
normalized = (raw_value / 3.0).clamp(-1.0, 1.0)      # ±3 significance band → unit interval
state_label = STRONG_POSITIVE_SHARPE | POSITIVE_SHARPE | NEGATIVE_SHARPE | STRONG_NEGATIVE_SHARPE
confidence  = |normalized|
```

## Registry

```
key: "price_trend_sharpe"
display_name: "Price Trend Sharpe"
group: Regime
class: Lagging
render: Pane
directional: true
data_source: CandleBased
normalization_mode: Directional
value_format: "ratio2"
value_source: "raw"
bars_required: 300          # = [candle_buffer] size → Live exactly at buffer-full
updates_on_shadow: false
```

The registry-wide `INDICATORS_MAX_BARS_REQUIRED` invariant is carried by this entry (300) — see the BBWP warmup note in `03-02-15-mme-indicator-lifecycle-states.md` for why BBWP's own gate stays at 200.

## Companion ratios

| Ratio | Layer | Home | Formula |
|-------|-------|------|---------|
| `volatility_to_spread_ratio` | L5 Risk | `RiskMatrix.execution_risk.volatility_to_spread_ratio` | `ATR(14) ÷ (ask − bid)` — execution-friction gauge. |
| `quality_to_risk_ratio` | L6 Advisory | `AdvisoryMatrix.quality_to_risk_ratio` | `market_quality_score ÷ overall_risk.score` — setup-efficiency metric. |

> **v6.14:** the v6.11 companion `trend_stability_sharpe` (EMA-50 log-return Sharpe) was **removed** with its L3 matrix field, Trend-card badge, and export pair. This indicator is now the sole Sharpe family member on the wire.

---

## Cross-References

- [Metrics Matrix §3.3.1](../../../matrices/02-07-metrics-matrix.md) — dual-representation wire format.
- [Risk Matrix §4.7](../../../matrices/02-11-risk-matrix.md) — `volatility_to_spread_ratio` scoring rules.
- [Decision Matrix §2.1](../../../matrices/02-04-decision-matrix.md) — `quality_to_risk_ratio`.
- [Candle Buffer Spec](../../../operations-and-compliance/08-08-candle-buffer-spec.md) — the 300-bar canonical window (CB-01).
- [Z-Score](04-02-39-zscore.md) · [LinReg Slope](04-02-38-linreg-slope.md) · [Aroon](04-02-36-aroon.md) — companion Regime-group indicators.
