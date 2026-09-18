# TAE Strategy Settings — Spec (v10)

**Version:** 11.10 (2026-09-17) — v11: stop floor (l6_formula/atr_mult), max_tp_rr cap, ladder_roles. See docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Engine:** Trade Automation Engine (TAE)

The `tae` section of the strategy JSON is the execution policy: intake
quality, trade lifecycle, sizing, execution mechanics, exit/invalidation
management. **Disable-friendly rule:** `null` / `0` / empty = disabled =
today's behavior. **The one documented exception (v10):**
`tae.risk.setup_gone_policy` defaults to `"balanced"` — a deliberate
semantic bump (pending entries expire after 12 bars by default under
balanced; v9 kept them immortal). Every individual dial below it remains
disable-friendly.

## Canonical `tae` section (`default`)

```json
"tae": {
  "intake": {
    "min_net_rr": 1.0,
    "min_score": null,
    "min_confidence": null,
    "max_setup_age_bars": null,
    "confirmation_bars": 0,
    "execution_veto": [],
    "direction_policy": "both",
    "trading_hours_utc": null,
    "volatility_gate": null,
    "funding_gate": null
  },
  "lifecycle": {
    "max_open_positions": 10,
    "max_per_setup_type": {},
    "max_per_direction": {},
    "pending_entry_expiry_bars": null,
    "reentry_cooldown_bars": 1,
    "replace_policy": "cancel_and_adopt",
    "min_reprice_delta_atr": 0.25,
    "daily": { "max_trades": null, "max_loss_pct": null }
  },
  "sizing": {
    "allocation_pct": 10.0,
    "per_setup_type_multipliers": {},
    "quality_curve": null,
    "after_loss_step_down": null,
    "max_position_size_pct_of_equity": null,
    "max_total_exposure_pct": null,
    "vol_scale": { "mode": "auto", "override": null }
  },
  "execution": {
    "entry_mode": "zone_midpoint",
    "spread_gate_bps": null,
    "slippage_bps": 5.0,
    "instant_fill_policy": "take_better",
    "chase_max_atr": 0.5,
    "chase_score_floor": 75.0,
    "tp_placement": "zone_midpoint"
  },
  "risk": {
    "invalidate_on": ["direction_flip"],
    "confidence_drop_pct": null,
    "breakeven_at_rr": null,
    "trailing": { "activate_at_rr": null, "atr_mult": null },
    "time_stop_bars": null,
    "signal_exit": "market",
    "setup_gone_policy": "balanced",
    "tp_refresh_min_rr_delta": 0.3,
    "sl_mode": "invalidation",
    "sl_padding_atr": 0.0,
    "atr_anchor_mult": 1.5,
    "min_sl_atr": null
  },
  "recovery": { "stale_state_window_secs": null }
}
```

## Instance controls (operational, not strategy JSON)

`POST /api/instances/:id/lifecycle { action }` — `start` (→ RUNNING),
`pause` (→ **instance PAUSED**: no new entries, pending orders cancelled, open positions
managed normally — close-only), `terminate` (→ STOPPED: cancel all orders,
force-close every open position at market, `exit_reason = "terminated"`).
UI buttons on the TAE dashboard header (paper + live; disabled in observe).

## Sizing & risk knobs (v9 — wired)

- `tae.sizing.allocation_pct` (instance override > strategy > global),
  `per_setup_type_multipliers`, `max_total_exposure_pct`,
  `after_loss_step_down` (consecutive losses from the SafetyManager →
  `reduce_pct`).
- `tae.sizing.vol_scale`: `fixed` = `override_factor`; `auto` = source-TF
  ATR% relative to the macro TF's ATR% (`clamp(0.25, 4.0)`). The factor is
  **frozen at entry** (params-at-entry freeze).
- PME portfolio-state gates (`pme.exposure.enforce.*`,
  `pme.capital.enforce_margin_close_only`) are enforced by the daemon tick
  as intake gates — PME stays informational.
- Gates + executor params follow the **instance's bound strategy**
  (`instances[].strategy`), not the workspace default.

## Lifecycle-hardening knobs (v10 — wired)

**Posture (setup-gone management):**
- `tae.risk.setup_gone_policy = balanced | strict | risky` — see
  03-03-01 §6 for the full behavior matrix. Under `balanced` the effective
  pending-entry expiry is `pending_entry_expiry_bars ?? 12` bars; `risky`
  keeps the dial verbatim (`null` = immortal); `strict` cancels pending /
  closes open (`exit_reason = "setup_gone"`) as soon as the actionable
  setup disappears.

**Pending-entry management:**
- `tae.lifecycle.replace_policy = cancel_and_adopt | cancel` — a different
  setup type topping the ranking while pending: adopt the replacement in
  the same tick (gates re-run) vs the v9 cancel-only behavior.
- `tae.lifecycle.min_reprice_delta_atr` — a fresh same-direction setup
  re-prices the pending entry only when the entry moved ≥ N × ATR
  (cancel-first, then place; projection recomputed). `0` = every candle.
- `tae.lifecycle.pending_entry_expiry_bars` — bars-based pending expiry
  (posture-aware effective default above).

**Entry dial (entry-point strictness):**
- `tae.execution.entry_mode = zone_midpoint | zone_edge | zone_any |
  market_on_ready | chase`.
  - `zone_edge`: the best-price edge of the zone (low for longs, high for
    shorts) — strictest fill, lowest probability.
  - `zone_any`: the first-touch edge (high for longs, low for shorts).
  - `market_on_ready`: market order at dispatch.
  - `chase`: market order when the mid is within `chase_max_atr × ATR`
    beyond the zone **and** the setup scores ≥ `chase_score_floor`
    (conditional tolerance — loosen entry only for high-conviction setups);
    otherwise a resting limit at the near edge.
- `tae.execution.instant_fill_policy = take_better | cancel` — when the
  price is already beyond the zone at dispatch, take the better marketable
  fill (v9) or refuse the entry.
- `tae.execution.spread_gate_bps` — refuse entries when the source-TF
  spread exceeds N bps.
- `tae.intake.max_setup_age_bars` — refuse setups whose source candle is
  older than N bars (stale-signal guard; `null` = disabled).

**Exit dial (SL/TP strictness):**
- `tae.risk.sl_mode = invalidation | invalidation_padded | atr_anchored`;
  `sl_padding_atr` widens the invalidation level by N × ATR (loose,
  noise-tolerant); `atr_anchor_mult` anchors the stop at entry ∓ N × ATR.
- `tae.risk.min_sl_atr` — **floor the stop** when the zone sits closer
  than N × ATR to the entry (v11: floors instead of skipping; `null` =
  no floor when `stop_floor_source = zone_only`).
- **v11 stop floor:** `tae.risk.stop_floor_source = l6_formula (default) |
  atr_mult | zone_only` — `SL = max(zone invalidation, floor)`; `l6_formula`
  uses the stop-TF `advisory.stop_loss_distance_pct` (2% base + vol/10,
  clamp [0.5,15]); `atr_mult` uses `stop_floor_atr_mult × ATR`.
- **v11 TP reachability:** `tae.execution.max_tp_rr` (default 1.5) caps the
  target so `TP = entry ± min(net_rr, max_tp_rr) × SL_distance` — no more
  unreachable micro-targets on short ladders.
- `tae.execution.tp_placement = zone_near_edge | zone_midpoint |
  zone_far_edge` — where inside the target zone the 100 % TP limit sits
  (conservative / balanced / aggressive). **The TP always closes the full
  position — scale-out is not part of v10.**
- `tae.risk.tp_refresh_min_rr_delta` — an open position's TP refreshes to
  a fresh same-direction setup only when the new bracket improves net RR
  by ≥ this delta (`0` = never refresh).
- `tae.risk.confidence_drop_pct` — close at market when the
  same-direction setup's confidence fell ≥ N percentage points vs entry
  (`exit_reason = "confidence_drop"`).
- **Asymmetric ratchet (always on):** a bracket refresh moves the SL only
  in favor (never widens) and is gated by `min_reprice_delta_atr` so the
  bracket doesn't churn every bar. Frozen breakeven/trailing/time-stop
  params are never touched by a refresh.

## Exit-reason vocabulary (extended)

`tp` · `sl` · `invalidated_signal` · `manual` · `stop_flatten` ·
`end_of_backtest` · `time_stop` · `breakeven` · `trailing_stop` ·
`expired` · `terminated` · `daily_budget` · **`setup_gone`** (v10) ·
**`confidence_drop`** (v10).

## Guarantees

- Intake always evaluates all ACTIVE TF snapshots of the fixed ladder (v11.2 — the ACTIVE durations (`[workspace].timeframes`); no TF-preference knob).
- Params-at-entry freeze: trailing/breakeven/time-stop/confidence
  baselines are stamped at entry; recharge affects new setups only.
- Safety precedence: the PME soft gate and the L6/L7 strategy gates (risk
  ceiling, breadth floor) veto intake above any TAE setting.
- One position per instance, one side, one SL, one TP (scaled entries
  erased — v9 F-06).
- Historical and recorded backtests replay the same executor tick with
  the run's bound strategy — the v10 dials behave identically across
  paper, live, and both backtest modes (parity contract).
