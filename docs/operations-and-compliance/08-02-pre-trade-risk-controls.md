# Pre-Trade Risk Controls

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Category:** Operations & Compliance

---

## 1. Purpose

Every order dispatched by the Trade Automation Engine (TAE) must pass a deterministic sequence of pre-trade gates between the Policy Layer's trigger and the Execution Layer's `ExchangeAdapter.start()` call. This document enumerates those gates, the order in which they run, the configuration sources for their thresholds, and the operator override path when a gate blocks a trade.

The gates are designed to fail closed — any failure short-circuits the dispatch and emits a logged rejection. No order reaches the exchange without passing every gate.

---

## 2. Control Catalogue

The following gates run between a Policy trigger (L1) and the Exchange dispatch (L2). Each gate references its authoritative specification.

| # | Gate | Source | Configuration |
|---|------|--------|---------------|
| 0 | **Lifecycle** (v6.2, new) | Entry orders are admitted only when the instance's `lifecycle_state = RUNNING` (see [03-03-06 IL-01/IL-05](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md); lifecycle states per `03-03-06` — see README §Feature Status for rollout). Exits (`reduce_only = true` or `is_emergency_liquidation = true`) **always bypass** Gate 0 regardless of lifecycle state. STOPPING/STOPPED/instance PAUSED entries are blocked and logged to `risk_control_events.gate_id = 0`. | Per-instance; runtime-only (no static threshold). |
| 1 | **Symbol execution stance (`active_stance`)** | The current `Stance` for the symbol must be `ACTIVE`, or the order must be a `reduce_only = true` exit under `CLOSE_ONLY`. `AVOID` blocks all dispatches except those tagged `is_emergency_liquidation = true` (Hard Exit path, see Gate 7 below and [PME Layer 4 §4.2](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md)). | Set manually by operator or automatically by the [PME Veto](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md#4-ontological-priority-veto). |
| 2 | **Decision guard (trade readiness)** | The Decision Matrix's `trade_readiness` field must be `READY` or `FORMING`. `WATCH` dispatches with a logged soft warning; `STAND_ASIDE` blocks the dispatch. | Computed by the MME Decision Layer from `directional_guidance × confidence_assessment × market_stance`. See [Decision Matrix §4](../matrices/02-04-decision-matrix.md). |
| 3 | **Capital query — available margin** | The TAE issues a synchronous request to the PME Capital Matrix for `available_margin`. The query returns 0 if the order would push `margin_usage_ratio` ≥ 0.95. | Live, computed from [PME Layer 3](../engines/portfolio-management-engine/03-04-04-pme-layer3-capital.md). |
| 4 | **Position sizing** | The Position Sizing Protocol computes $S = E \times R / (D_{sl} / 100)$. If the result exceeds `risk_parameters.max_position_size_usd`, sizing is clipped. If `risk_parameters.max_leverage` is exceeded, the order is rejected. **Bypass:** orders with `reduce_only = true` skip Gate 4 (sizing) — size is copied verbatim from the Position Matrix. | Per-policy in `config.toml` `[execution_policies.*]`. |
| 5 | **Slippage ceiling** | The Execution Layer queries the live order book and computes estimated slippage. If estimated slippage **exceeds** the configured ceiling (default 0.5 % of position size), the order is held for manual review (strict `>` — an order exactly at the ceiling is allowed through). | `config.toml` `[execution.slippage_ceiling_pct]`. |
| 6 | **Exposure concentration** | The Exposure Layer rejects new positions that would breach the single-pair concentration limit (default 0.20), the portfolio exposure limit (default 0.50), or the correlation limit (default 0.8). **Scope:** Gate 6 rejects pre-trade only; post-fill exposure drift is handled by the PME veto (see [PME Layer 4 §4.1](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) exposure row → `CLOSE_ONLY`). **Bypass:** orders with `reduce_only = true` skip Gate 6 (concentration) — exit orders must be permitted even if the portfolio is overconcentrated. | [PME Layer 2 §3](../engines/portfolio-management-engine/03-04-03-pme-layer2-exposure.md). |
| 7 | **PME safety soft gate** | Even if all previous gates pass, the PME Overview Layer publishes the safety-state ladder: when `DRAWDOWN_STOP` or `SUSPENDED` is active (drawdown beyond the limit (equivalently `current_equity`/`peak_equity` < 1 − `drawdown_limit_pct`), or loss streak ≥ dropout_threshold), the TAE setup executor blocks **new entries** (exits are always permitted). **Bypass:** none for entries while blocked; `reduce_only = true` exits are never blocked. | [PME Layer 4 §3](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md). |

---

## 3. Evaluation Order & Short-Circuiting

The gates run in the order listed in §2, first-match-wins short-circuit. The first failure aborts the dispatch and emits a rejection log entry.

**Gate 0 / Gate 1 sequencing.** Gate 0 (lifecycle) evaluates the per-instance `LifecycleState` ([03-03-06 IL-05](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md)) and admits entries only when `RUNNING`. Gate 1 (symbol stance) reads the `Stance` value set either by the operator or by the PME veto upstream. Gate 7 (PME safety veto) is the authoritative stance setter — when PME fires, it transitions the symbol `Stance` to `AVOID` or `CLOSE_ONLY`, and Gate 1 picks the value up on the next evaluation cycle. Both gates predicate on the same `Stance` field, but they are sequential, not duplicated:

- **Gate 1** is the *current* disposition read at dispatch time. A passive `AVOID` (operator-set, no PME event) is caught here.
- **Gate 7** is the *active-veto* check fired by PME just before dispatch — it re-validates the disposition against the most recent PME authority. A `PME VetoMessage` that races with the policy trigger and would otherwise have been missed between Gate 1 and Gate 7 is caught here.

```
Gate 0 lifecycle → if (lifecycle_state ≠ RUNNING AND not reduce_only AND not is_emergency_liquidation) → block
Gate 1 stance → if (stance == AVOID AND not is_emergency_liquidation) → block
              → if (stance == CLOSE_ONLY AND not reduce_only) → block
(v7: the 8-gate chain is erased — see [03-03-01 §7](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md))
Gate 3 capital → if 0 → block (insufficient margin)
Gate 4 sizing → if max_position_size_usd exceeded → clip, continue
              → if max_leverage exceeded → block
              → reduce_only orders skip Gate 4 entirely
(v7: erased with the gate chain)
Gate 6 concentration → if breach → block (reduce position size or close existing)
                     → reduce_only orders skip Gate 6 entirely
Gate 7 PME veto → if (stance == AVOID AND not is_emergency_liquidation) → block
```

Hard-stops (block) vs hold-for-review (suspend) vs modified-and-continues (clip):

| Disposition | Gates |
|---|---|
| **Hard-stop** (no order ever reaches exchange) | Gate 0 (lifecycle ≠ RUNNING for non-exit entries), Gate 1 (AVOID without is_emergency_liquidation, or CLOSE_ONLY without reduce_only), Gate 3, Gate 4 (max leverage), Gate 6 (for non-reduce_only orders), Gate 7 (AVOID without is_emergency_liquidation) |
| ~~Hold-for-review~~ | ~~Gates 2 / 5~~ | **Erased (v7)** with the policy engine and gate chain |
| **Modified and continues** (clipped, continues; logged as `CLIP_AND_CONTINUE`) | Gate 4 (size cap — clipped, not blocked) |

### 3.1 Exit / Reduce-Only Bypass Summary

| Order characteristic | Gates bypassed | Gates still applied |
|---|---|---|
| `reduce_only = true` | Gate 0 (lifecycle — exits always pass), Gate 4 (sizing), Gate 6 (concentration) | Gate 1 (stance — must be ACTIVE or CLOSE_ONLY), Gate 2 (readiness), Gate 3 (capital), Gate 5 (slippage), Gate 7 (PME veto — bypassed only with `is_emergency_liquidation = true`) |
| `is_emergency_liquidation = true` (Hard Exit) | Gate 0 (lifecycle — even STOPPING/STOPPED/lifecycle PAUSED), Gate 1 (stance — even AVOID), Gate 2, Gate 4 (sizing), Gate 5 (slippage), Gate 6, Gate 7 | Gate 3 (capital — check margin available for the closing side) |
| Neither flag | — | All gates apply |

### 3.2 Order State Pre-Dispatch (`PRE_DISPATCH`)

(v7: the pre-dispatch review path, `PRE_DISPATCH` order state, and manual open/close endpoints were erased along with the policy engine and gate chain — see [03-03-01-tae-overview-spec.md §7](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md).)

> **v7 note.** The pre-dispatch review path is erased; `risk_control_events` (see [`06-02-database-schema-spec.md §3.10`](../integration-and-api/06-02-database-schema-spec.md)) remains the persistent safety audit trail (releases/resets with `operator_id = "local"`).

---

## 4. Parameters & Configuration

| Parameter | Source | Default | Override Path |
|-----------|--------|---------|---------------|
| `allocation_pct` | global `minimal_tae` (+ per-instance override) | 10.0 | Edit `config.toml` `[workspace.minimal_tae].allocation_pct` (1–100 %; Σ of all instance allocations ≤ 100 %) |
| `max_position_size_usd` | per-policy | unlimited | Set in policy |
| `max_leverage` | per-policy | 20 | Set in policy |
| `execution.slippage_ceiling_pct` | global | 0.5 | Set in `config.toml` |
| `leverage.cross_leverage` | global | 20 | Set in `config.toml` |
| `safety.drawdown_limit_pct` | global | 0.30 (= 30 %) | Set in `config.toml` `safety.*` |
| `safety.max_daily_drawdown_pct` | global | 0.05 (= 5 %) | Set in `config.toml` `safety.*` (drives the `WARN` state — see [03-04-05 §3](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md)) |
| `safety.caution_threshold` | global | 3 losses | Set in `config.toml` `safety.*` |
| `safety.dropout_threshold` | global | 5 losses | Set in `config.toml` `safety.*` |
| `safety.systemic_risk_threshold` | global | **80** (`systemic_risk_score ≥ 80` triggers the systemic-risk veto branch of Gate 7, on the canonical `[0, 100]` scale from [02-09-overview-matrix.md §4](../matrices/02-09-overview-matrix.md)) | Set in `config.toml` `safety.systemic_risk_threshold` |
| `risk_profiles.*.max_risk_pct` | per risk-profile | 2 (= 2 %) | Edit via `POST /api/risk-profiles` |
| `risk_profiles.*.leverage` | per risk-profile | 20 | Edit via `POST /api/risk-profiles` |
| `fees.maker_fee_pct` | global | 0.02 (= 0.02 %) | Set in `config.toml` `fees.*` |
| `fees.taker_fee_pct` | global | 0.06 (= 0.06 %) | Set in `config.toml` `fees.*` |
| `fees.funding_rate_8h` | global | 0.01 (= 0.01 %) | Set in `config.toml` `fees.*` (8-hour funding rate; see [03-03-05 §4](../engines/trade-automation-engine/03-03-05-tae-paper-trading-spec.md)) |

---

## 5. Rejections & Operator Overrides

Every gate failure produces:
1. A rejection log entry in the Execution Matrix with the gate ID and reason.
2. An observability event surfaced via `GET /api/system/observability?symbol=…`.
3. No exchange order placement.

**Operator override paths:**

| Gate | Override endpoint | Behaviour |
|---|---|---|
| ~~Gate 2 (readiness override)~~ | ~~erased~~ | **Erased (v7)** — the 8-gate pre-trade chain and the readiness-override endpoint were removed with the policy engine; the setup executor applies its own gates (see [03-03-01 §7](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md)). |

| **Gate 6** (concentration breach) | (no automatic override) | Close an existing position in the affected sector first, then re-trigger. |
| **Gate 7** (PME veto) | `POST /api/instances/:id/safety/release-veto` | The operator must clear the underlying condition (e.g. equity must recover above `drawdown_limit_pct`) **and** call this endpoint. Returns `422 Unprocessable Entity` if the veto condition is still active. |
| **Gate 1** (symbol stance `AVOID` after Hard Exit) | `POST /api/instances/:id/safety/release-veto` | Same endpoint as Gate 7; restores the default stance after the Hard Exit completes. The `/safety/reset` endpoint only clears `consecutive_losses` and is **not** the right call for drawdown- or systemic-risk-based vetoes. |
- **Stance `AVOID` (Gate 1, after Hard Exit has completed).** Once the Hard Exit fires and the stance transitions to `AVOID`, the operator must use `POST /api/instances/:id/safety/release-veto` (not `/safety/reset`) to restore the default stance. The `/safety/reset` endpoint only clears the per-symbol `consecutive_losses` counter; it does **not** release a drawdown- or systemic-risk-based veto.

All overrides are logged with operator ID, timestamp, and prior state for audit.

---

## 6. Cross-References

- [TAE Layer 2 — Execution](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md) — Position Sizing Protocol & order dispatch.
- [PME Layer 3 — Capital](../engines/portfolio-management-engine/03-04-04-pme-layer3-capital.md) — Available margin source.
- [PME Layer 4 — Overview](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) — Safety-state authority.
- [TAE Overview — Risk gates](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md) — the executor's risk gates.
- [TAE Instance Lifecycle Spec](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) — Gate 0 (lifecycle), `LifecycleState` enum, automation schema.
- [User Manual](08-01-user-manual.md) — Operational context.
