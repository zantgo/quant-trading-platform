# MME Strategy Configuration — Canonical Spec (v11)

**Version:** 11.7 (2026-09-16) — v11: quantity-first defaults + ladder_roles. See docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Engine:** Market Monitoring Engine (MME) — Layers L1 · L1.5 · L2 · L2.5 · L3 · L4 · L5 · L6 · L7
**Method:** Spec-driven development — this document is the contract. Code, UI,
CLI, and tests implement exactly what is written here. No backward
compatibility — deprecated code is erased; each layer must be correct.

---

## 1. Reason for this update

The platform is stable but static. Every behavior-shaping value below the
indicator level (blends, thresholds, bands, weights, gates, cost models) is
hardcoded in Rust constants scattered across `core-domain` and
`market-analyzer`. The operator can tune indicator periods and toggle
activation, but cannot express a *trading model*: "trust macro over micro",
"only trade trend continuations", "never recommend above risk 60", "only
trade with the tide".

Secondary problems this update eliminates:

- **Duplicated logic** kept in sync by convention (`advisory.rs` vs
  `decision_context.rs`; the deprecated L3 copy of the L4 decision tree).
- **Dead config** — `OpportunityMatrixConfig`, `OrderBookConfig`, and the
  net-cost model were declared but ignored by the runtime.
- **Documented-but-unwired rules** — SR_BASED protection missing its
  `0.5·ATR` proximity check; `oi_split` funding anchor hardcoded instead of
  reading `funding_extreme_pct`.
- **Settings sprawl** — behavior settings live in many frontend panels and
  config sections with no single exportable artifact.

**End state:** one **Strategy JSON** — the single source of truth for all
model behavior — edited/exported in the frontend, understood identically by
the CLI, consumed by one backend code path, and replayed verbatim by the
backtesting engine.

## 2. Container semantics

- Named strategies; the built-in `default` strategy reproduces v8.2 behavior
  byte-for-byte. Non-default strategies override any field; everything else
  inherits through `base` (patch semantics).
- `schema_version` guards migrations.
- Instances always launch bound to `default` (no selector in the Launch
  Setup wizard); rebinding later triggers a **full recharge** at the next
  candle boundary. Open positions keep the params that entered them
  (**params-at-entry freeze**); recharge affects new setups only.
- `config.toml` keeps platform infra only: exchanges, candle buffer, clock,
  reconnect, quality, snapshot export, backtest archive, per-instance TF
  ladders + mode, and `[workspace] portfolio_capital_usd` (v9 F-07 — the
  single capital dial; the strategy never carries capital).
- The backtesting engine binds a strategy (`strategy_id`), freezes the full
  JSON on the run, and replays stored params — never live config.

## 3. Canonical strategy JSON (`default`)

```json
{
  "schema_version": 1,
  "name": "default",
  "base": null,
  "description": "The platform baseline model — reproduces v8.2 behavior exactly.",

  "l1": {
    "indicator_weights": { "ema_stack": 1.0, "rsi": 1.0, "macd": 1.0 },
    "monitor_only": [],
    "context": {
      "trend_momentum_blend": [0.6, 0.4],
      "regime_gate_damp": { "trending": 1.0, "expansion": 1.0, "range": 0.6, "other": 0.5 },
      "regime_rule": { "bbwp_compression": 15, "bbwp_expansion": 85,
                       "adx_trending": 25, "chop_compression": 61.8, "chop_trending": 38.2 },
      "volatility_sources": { "bbwp": 1.0, "hv": 0.0, "atr_pct": 0.0 }
    },
    "signals": {
      "confidence_boost": {},
      "max_age_bars": null,
      "strength_buckets": [0.15, 0.6, 0.85]
    },
    "ignore_reconstructed_candles": false,
    "order_book": { "depth_levels": 20, "imbalance_threshold": 0.3,
                    "wall_threshold": 0.5, "spread_warning_pct": 0.1,
                    "spread_wide_threshold_pct": 0.05 }
  },

  "l1_5": {
    "enabled": true, "liquidation_feed": true,
    "cluster_estimation": true, "signals": true,
    "mark_price_poll_ms": 60000,
    "event_retention_days": 90, "bucket_retention_days": 7,
    "cluster_refresh_secs": 0,
    "maintenance_margin_rate": 0.005,
    "cascade_detected_zscore": 2.5, "cascade_sustained_events": 3,
    "funding_extreme_pct": 0.0005,
    "magnet_activation_distance_pct": 0.5,
    "liquidity_vacuum_threshold": 0.3,
    "oi_funding_divergence_pct": 2.0,
    "min_cluster_notional_usd": 50000.0,
    "signal_confidences": { "cascade_detected": 0.8, "cascade_sustained": 0.9,
                            "cascade_exhausted": 0.7, "funding_extreme": 0.95,
                            "oi_funding_divergence": 0.7, "liquidity_vacuum": 0.6,
                            "funding_flip": 0.75, "oi_price_divergence": 0.7 },
    "signal_weights": { "CASCADE_DETECTED": 1.0, "CASCADE_SUSTAINED": 1.0,
                        "CASCADE_EXHAUSTED": 1.0, "FUNDING_EXTREME": 1.0,
                        "FUNDING_FLIP": 1.0, "OI_FUNDING_DIVERGENCE": 1.0,
                        "OI_PRICE_DIVERGENCE": 1.0, "LIQUIDITY_VACUUM": 1.0,
                        "MAGNET_ACTIVATION": 1.0, "CLUSTER_PRESSURE_HIGH": 1.0,
                        "CLUSTER_PRESSURE_LOW": 1.0 },
    "accumulator": { "cascade_window_candles": 5, "intensity_log_scale": 20.0,
                     "baseline_no_history_usd": 1000.0, "sig_window_events": 50,
                     "fallback_baseline_usd": 500.0, "exhausted_intensity": 30.0,
                     "max_buffered_events": 1000 },
    "api_failover": { "max_retries_per_call": 5, "retry_delay_seconds": 30,
                      "max_consecutive_failures": 30 },
    "per_tf_leverage": { "enabled": true, "buckets": [1, 3, 5, 10, 20, 50, 100],
                         "weights": [0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
                         "min_cluster_notional_usd": 50000.0 }
  },

  "l2": {
    "tf_weighting": { "mode": "proportional",
                      "weights": { "micro1": 0.1, "micro2": 0.1, "fast1": 0.1, "fast2": 0.1,
                                   "slow1": 0.166, "slow2": 0.166, "macro1": 0.5, "macro2": 0.5,
                                   "longterm1": 1.0, "longterm2": 1.0 },
                      "floor": 0.2, "ceil": 1.0 },
    "overall_blend": { "trend": 0.5, "momentum": 0.3, "volatility": 0.1, "volume": 0.1 },
    "thin_volume": { "enabled": true, "threshold": 25.0,
                     "blend": { "trend": 0.55, "momentum": 0.35,
                                "volatility": 0.05, "volume": 0.05 } },
    "confluence": { "min_tfs": 2 },
    "trend_agreement_weighted": false,
    "dimension_mask": { "trend": true, "momentum": true, "volume": true,
                        "volatility": true, "structure": true, "signal": true,
                        "regime": true, "confidence": true, "liquidity": true,
                        "tradability": true },
    "states": { "signed": [0.3, 0.6], "unsigned": [20, 40, 60, 80],
                "overall_label_bands": [20, 40], "single_tf_confidence_score": 50.0 }
  },

  "l2_5": {
    "estimation": { "swing_window_bars": 200, "swing_lookback": 5,
                    "bin_size_pct": 0.001, "peak_halfwidth_divisor": 20,
                    "bound_decay": 0.5, "ttl_secs": 300 },
    "oi_split": { "funding_anchor": null, "funding_bias_scale": 0.3,
                  "price_anchor_pct": 1.0, "price_bias_scale": 0.2,
                  "clamp": [0.10, 0.90] },
    "confidence": { "oi_adequacy_anchor_usd": 1000000.0, "funding_penalty": 0.3 },
    "funding_modulation": { "shift": 0.05 },
    "signals": { "sustained_events_this_bar": 3, "vacuum_dense_events": 3,
                 "vacuum_dense_usd": 50000.0, "funding_extreme_strength_slope": 50.0 }
  },

  "l3": {
    "bias": {
      "bands": { "strong": 25, "plain": 12 },
      "grace": { "band": [10, 15], "vote_min": 2, "flat_tf": 8,
                 "agreement_min": 60, "signals_min": 2, "haircut": 0.85,
                 "hold": { "band_min": 12, "vote_min": 2 },
                 "skip_regime": "COMPRESSION" },
      "lean": { "tolerance": 10, "haircut": 0.8 }
    },
    "confidence": { "agreement": { "bonus": 0.15, "min": 75 },
                    "conflict": { "cap": 0.5, "max": 50 },
                    "signals": { "bonus": 0.10, "min": 3 },
                    "single_tf_cap": 0.5 },
    "regime": { "bbwp": { "expansion": 85, "contraction": 10 },
                "adx": 25, "trend_score": 20,
                "missing": { "bbwp": 50, "adx": 25 } },
    "assessments": { "trend": [90, 75, 50, 25], "momentum": [80, 60, 40],
                     "structure": [80, 60, 40, 20], "volatility": [90, 70, 40, 20],
                     "volume": [90, 70, 40] },
    "quality_bands": [30, 50, 70, 85],
    "phase": { "low_vol_max": 40, "trend_score": 20,
               "volume_strong": 70, "structure_healthy": 60,
               "volume_delta": 5.0 }
  },

  "l4": {
    "setups": { "enabled": ["LiquiditySqueeze", "Scalp", "TrendContinuation", "Breakout",
                            "Reversal", "Pullback", "MeanReversion"],
                "priority": ["LiquiditySqueeze", "Scalp", "TrendContinuation", "Breakout",
                             "Reversal", "Pullback", "MeanReversion"] },
    "preconditions": {
      "trend_continuation": { "trend_min": 55 },
      "breakout": { "vol_min": 55, "struct_min": 45 },
      "reversal": { "momentum_exhausted_max": 30, "structure_broken_max": 45 },
      "pullback": { "trend_min": 45 },
      "mean_reversion": { "vol_max": 40, "regimes": ["Range", "Contraction"] },
      "scalp": { "bbwp_range": [70, 95], "struct_min": 55,
                 "regimes": ["TrendingBull", "TrendingBear"] },
      "liquidity_squeeze": { "asymmetry_min": 0.25,
                             "regimes": ["Expansion", "Transition"] }
    },
    "scoring": { "blend": [0.35, 0.30, 0.20, 0.15],
                 "quality_bands": [75, 60, 45, 25] },
    "zones": { "atr_fallback": { "enabled": true, "k_entry": 1.5, "k_target": 2.5 },
               "tolerance_atr_mult": 0.2, "tolerance_close_pct": 0.1,
               "width_k": { "high": 2.0, "threshold": 70, "low": 1.5 },
               "fallback_strength": 35.0,
               "invalidation_weights": { "fib_0786": 0.5, "vp_val": 0.4 },
               "range_frame": { "entry_half_atr": 0.2, "target_k_atr": 1.5,
                                "target_spread_atr": 0.2, "inv_k_atr": 1.5 },
               "horizon_stop_budgets": { "scalp": 1.5, "swing": 3.0 } },
    "confluence_weights": { "volume_profile": 0.30, "fibonacci": 0.25,
                            "support_resistance": 0.20, "pivot_points": 0.15,
                            "liquidation_cluster": 0.10, "atr_fallback": 0.05 },
    "costs": { "taker_fee_bps": 6.0, "slippage_bps": 5.0, "funding_bps": 0.0 }
  },

  "l5": {
    "overall_weights": { "market": 0.14, "volatility": 0.14,
                         "execution_liquidity": 0.14, "structure": 0.10,
                         "momentum": 0.14, "signal": 0.10,
                         "execution": 0.10, "cascade": 0.14 },
    "bands": [80, 60, 40, 20],
    "state_delta": 10.0,
    "dimensions": {
      "market": { "baseline": 50, "weak_trend": 15, "broken_structure": 15,
                  "poor_quality": 10, "low_conf_max": 0.4, "low_conf": 10,
                  "contradicting": 10, "strong_trend": -10,
                  "high_conf_min": 0.7, "high_conf": -10 },
      "volatility": { "baseline": 30, "bbwp_extreme": 90, "bbwp_extreme_add": 30,
                      "bbwp_elevated": 70, "bbwp_elevated_add": 15,
                      "squeeze_add": 10, "micro_fast_blend": [0.7, 0.3],
                      "atr_pct_floor": 1.0, "atr_pct_max": 5.0 },
      "execution_liquidity": { "baseline": 10, "rvol_very_low": 0.4,
                               "rvol_very_low_add": 30, "rvol_low": 1.0,
                               "rvol_low_add": 15, "rvol_high": 5.0,
                               "rvol_high_add": -15, "spread_wide": 0.2,
                               "spread_wide_add": 20, "spread_tight": 0.05,
                               "spread_tight_add": -10 },
      "structure": { "baseline": 40, "broken": 30, "weak": 15,
                     "healthy": -15, "flip": 15 },
      "momentum": { "baseline": 30, "exhausted": 40, "reversing": 30,
                    "weakening": 15, "increasing": -10 },
      "signal": { "baseline": 30, "per_contradicting": 10,
                  "contradicting_cap": 40, "none_active": 10,
                  "low_conf_max": 0.5, "low_conf": 15 },
      "execution": { "baseline": 25, "spread_wide": 0.15, "spread_wide_add": 25,
                     "spread_moderate": 0.08, "spread_moderate_add": 10,
                     "rvol_low": 0.7, "rvol_add": 15,
                     "ratio_tiers": [ { "max": 1.5, "add": 15 },
                                      { "max": 3.0, "add": 5 },
                                      { "min": 10.0, "add": -5 } ] },
      "cascade": { "baseline": 30, "sustained": 30, "detected": 15,
                   "asymmetry_min": 0.3, "asymmetry_scale": 30.0,
                   "oi_divergence_max": 15, "funding_flip_max": 10 }
    }
  },

  "l6": {
    "synthesis": { "confluence_weights": [0.50, 0.30, 0.20],
                   "risk_discount_k": 1.0, "opportunity_fallback": 50.0 },
    "stance": { "risk": { "avoid": 90, "cautious": 75, "neutral": 50,
                          "constructive": 55, "aggressive": 45 } },
    "direction": { "risk_strong": 50, "risk_plain": 40 },
    "entry": { "vol_risk_no_entry": 80, "vol_risk_immediate": 50,
               "vol_risk_breakout": 40 },
    "exit": { "risk_increasing": 80, "trend_weakening": 60 },
    "protection": { "vol_risk": 60, "sr_proximity_atr_mult": 0.5 },
    "target": { "rr_based": 40, "trailing": 60 },
    "stop": { "base_multiplier": { "strong": 1.0, "weak": 1.5 },
              "base_pct": 2.0, "base_clamp": [0.5, 5.0],
              "vol_bump_scale": 10.0, "final_clamp": [0.5, 15.0] },
    "entry_danger": { "quality_penalties": { "Excellent": 10, "Good": 25,
                       "Average": 50, "Weak": 70, "Poor": 80 },
                      "blend": [0.5, 0.5] },
    "readiness": { "aside_max": 20, "ready_min": 20 },
    "probability": { "guidance_amp": 1.2, "guidance_atten": 0.5,
                     "stance_amp": 1.15, "avoid_atten": 0.5,
                     "avoid_hold_amp": 1.5, "rr_penalty": 0.6,
                     "min_pct": 2.0, "hold_cap": 60.0, "arm_floor": 15.0,
                     "geometric_offset": 0.15, "eff_conf_floor": 0.5,
                     "hold_scale": 50.0, "contributing_conf_min": 0.6 },
    "risk_ceiling": { "max_overall_risk": null }
  },

  "ladder_roles": {
    "enabled": true,
    "decision_tf": "longterm2",
    "entry_tf": "micro1",
    "stop_tf": "longterm2",
    "target_tf": "micro1"
  },

  "l7": {
    "breadth_bands": { "strong": 60, "positive": 20, "balanced": 10 },
    "global_bias": { "strong_share": 0.8, "plain_share": 0.6 },
    "sync_bands": [75, 50, 25, 10],
    "risk": { "dist_bins": { "low_max": 30, "high_min": 70 },
              "env_mean": { "high": 50, "moderate": 25 } },
    "systemic": { "weights": [0.6, 0.4],
                  "sync_penalty": { "highly_synchronized": 100, "synchronized": 60,
                                    "mixed": 30, "fragmented": 10,
                                    "highly_fragmented": 0 },
                  "tf_decay": { "micro1": 0.05, "micro2": 0.05,
                                "fast1": 0.05, "fast2": 0.1,
                                "slow1": 0.1, "slow2": 0.15,
                                "macro1": 0.15, "macro2": 0.15,
                                "longterm1": 0.1, "longterm2": 0.1 },
                  "cascade_index_fallback": 50.0,
                  "entry_veto_threshold": 80.0 },
    "asset_rank": { "slope": 0.5, "offset": 50.0 },
    "low_coverage_min_symbols": 3,
    "alignment_buckets": [75, 50],
    "breadth_entry_floor": null
  }
}
```

*Notes:* `l1.indicator_weights` is abbreviated here — the full key list is
the 52 registry keys, all `1.0`. `l1.signals.confidence_boost` is `{}` =
detector defaults. `l2_5.oi_split.funding_anchor: null` = follow
`l1_5.funding_extreme_pct` (the v9 F-01 fix).

**v11.1 slot keys (fixed 10-slot ladder).** Every per-TF strategy key is now
indexed by the fixed slot names `micro1`, `micro2`, `fast1`, `fast2`, `slow1`,
`slow2`, `macro1`, `macro2`, `longterm1`, `longterm2` (the legacy `micro` /
`fast` / `slow` / `macro` keys are gone):

- `l2.tf_weighting.weights` defaults `0.1 / 0.1 / 0.1 / 0.1 / 0.166 / 0.166 / 0.5 / 0.5 / 1.0 / 1.0` (family-split: each legacy family value is split evenly across its pair).
- `l7.systemic.tf_decay` defaults `0.05 / 0.05 / 0.05 / 0.1 / 0.1 / 0.15 / 0.15 / 0.15 / 0.1 / 0.1` (Σ = 1.0).
- `l5.volatility.micro_fast_blend` `[0.7, 0.3]` pairs the `micro1` / `fast1` slots (fastest two sub-minute slots of their families).
- `ladder_roles` defaults `decision_tf` / `stop_tf` = `longterm2` (the slowest slot — bias, regime, stance, SL floor) and `entry_tf` / `target_tf` = `micro1` (the fastest slot — zones and timing); see [03-03-08-tae-ladder-roles.md](../trade-automation-engine/03-03-08-tae-ladder-roles.md).

## 4. Bug fixes & erasures (implementation order — COMPLETED)

| # | Item | Location |
|---|---|---|
| F-01 | Wire `oi_split` funding anchor → `funding_extreme_pct` | `core-domain/src/liquidity/mod.rs` |
| F-02 | Wire SR_BASED `distance_to_SR < mult · ATR` check | `core-domain/src/advisory.rs` |
| F-03 | Erase deprecated L3 `opportunity_analysis` mirror + consumers | `analysis.rs`, `advisory.rs`, `synthesis.rs`, UI |
| F-04 | Wire `OpportunityMatrixConfig` + `OrderBookConfig` + `NetCostModel` | `synthesis.rs`, `analyzer/mod.rs`, registry, BTE |
| F-05 | Unify `advisory.rs` ↔ `decision_context.rs` into `DecisionParams` | `core-domain/src/decision_params.rs` |
| F-06 | Erase scaled entries + scoring allocation legacy | config-models, portfolio-supervisor, UI |
| F-07 | Erase per-instance `initial_capital_usd` → `portfolio_capital_usd` | config-models, daemon, API, CLI, UI |
| F-08 | `max_position_size_usd` → `max_position_size_pct_of_equity` | config-models, executor, API, UI |

## 5. Layer wiring status (v9 — COMPLETED)

| Layer | Knobs | Status |
|---|---|---|
| L1 | `indicator_weights`, `monitor_only`, `context.trend_momentum_blend`, `context.regime_gate_damp`, `context.regime_rule`, `context.volatility_sources`, `signals.confidence_boost` (per-SignalKind), `signals.max_age_bars`, `signals.strength_buckets` → `strength_label`, `ignore_reconstructed_candles`, `order_book` | ✅ `market_context_synth.rs`, `analyzer/mod.rs` (`apply_l1_signal_params`), pipeline spawn |
| L1.5 | toggles (`enabled`/`liquidation_feed`/`cluster_estimation`/`signals`), poll/retention ms, `cluster_refresh_secs`, `maintenance_margin_rate`, cascade thresholds, `funding_extreme_pct`, magnet/vacuum/divergence, `min_cluster_notional_usd`, `signal_confidences`, `signal_weights` → L5 discrete-signal bonuses + L4 signal-strength factor, `accumulator` (baselines/log-scale/exhausted/window/buffer), `api_failover`, `per_tf_leverage` | ✅ `liquidity_params.rs` resolver + `registry/pipelines.rs` + `analyzer/mod.rs` |
| L2.5 | `estimation` (swing window/lookback, bin size, peak half-width divisor, bound decay, TTL), `oi_split`, `confidence`, `funding_modulation.shift`, `signals` (sustained/vacuum/funding-slope thresholds) | ✅ `core-domain/src/liquidity/mod.rs` (`ClusterEstimateInput`) + `compute_cluster_for_tf` |
| L2–L7 | analysis/alignment/risk/opportunity/decision/overview params | ✅ (v9 earlier phases) |

Strategy wins per-field over the legacy `[workspace.liquidity]` /
`[order_book]` sections (pre-v9 parse fallback only).

## 6. Behavioral guarantees

- **Default invariance:** the `default` strategy reproduces v8.2 outputs
  byte-for-byte (except F-01/F-02, both documented divergences from spec,
  not regressions).
- **Layer contracts:** L4/L5 orthogonality, L2.5 feedback-loop avoidance,
  L5 measurement purity, L6-never-reads-L7, `NoClearOpportunity` sentinel —
  all preserved.
- **Recharge:** strategy change applies at the next candle boundary; open
  positions frozen at entry params; rolling state reseeded from in-memory
  buffers.
- **Attribution:** snapshots, trades, and backtest runs carry
  `strategy_id + schema_version + config_version`.

## 7. Cross-references

- [TAE Strategy Settings](../trade-automation-engine/03-03-07-tae-strategy-settings.md)
- [PME Strategy Settings](../portfolio-management-engine/03-04-06-pme-strategy-settings.md)
- [PAE Strategy Settings](../performance-analytics-engine/03-05-07-pae-strategy-settings.md)
- [Account Profile (UI)](../../ui-ux/07-08-account-profile.md)
- [Strategies Builder (UI)](../../ui-ux/07-09-strategies-builder.md)
- [CLI ↔ GUI Parity](../../integration-and-api/06-00-cli-gui-parity.md)
