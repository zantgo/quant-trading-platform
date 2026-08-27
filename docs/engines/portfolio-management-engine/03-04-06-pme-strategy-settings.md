# PME Strategy Settings — Spec (v9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Engine:** Portfolio Management Engine (PME)

PME is informational by design (v7): it computes equity, exposure, capital,
and the 5-state safety ladder — and reports. The `pme` strategy section is
the trader's **account risk envelope**. Enforcement is optional and
**off by default**: when a flag is `true`, the TAE intake gate refuses the
breaching entry; PME never blocks, cancels, or modifies anything.

## Canonical `pme` section (`default`)

```json
"pme": {
  "safety": {
    "max_daily_drawdown_pct": 5.0,
    "drawdown_limit_pct": 30.0,
    "consecutive_loss_caution": 3,
    "consecutive_loss_dropout": 5,
    "dropout_duration_hours": 8,
    "loss_streak_scope": "per_symbol",
    "warn_extra_trigger_pct": null,
    "drawdown_stop_release": {
      "mode": "manual",
      "after_hours": null,
      "recovery_pct": null,
      "rebaseline_peak_on_release": false
    }
  },
  "exposure": {
    "max_single_pair_exposure_pct": 20.0,
    "max_portfolio_exposure_pct": 50.0,
    "max_correlation": 0.8,
    "enforce": { "single_pair": false, "portfolio": false, "correlation": false }
  },
  "capital": {
    "margin_alert_bands": { "warning": 0.80, "close_only": 0.95, "emergency": 1.00 },
    "enforce_margin_close_only": false
  },
  "enforce_systemic_veto": false
}
```

## Enforcement (only when a flag is ON)

| Flag | Gate condition | Dashboard label |
|---|---|---|
| `exposure.enforce.single_pair` | Entry would push that symbol above its cap | `BLOCKED — single-pair exposure limit` |
| `exposure.enforce.portfolio` | Entry would push gross exposure above its cap | `BLOCKED — portfolio exposure limit` |
| `exposure.enforce.correlation` | New symbol correlates > `max_correlation` | `BLOCKED — correlation limit` |
| `capital.enforce_margin_close_only` | `margin_usage_ratio ≥ close_only` band | `BLOCKED — margin close-only` |
| `enforce_systemic_veto` | Systemic risk ≥ `l7.systemic.entry_veto_threshold` | `BLOCKED — systemic risk veto` |

Invariants: gates apply to **new entries only** — exits are always
permitted; the ladder contract "no state change ever blocks exits" is
preserved.

## Erasure (v9 F-06)

Scaled entries/pyramiding erased: `position_slots`, `PositionScalingConfig`,
`AllocationCurve(-Model)`, scoring allocation (`use_scoring_allocation`,
base/micro thresholds), Position Matrix scaled-entry fields. **One position
per instance, one side, one SL, one TP** — the canonical PnL formula
reduces to `direction_sign × (price − entry_price) × size`.
