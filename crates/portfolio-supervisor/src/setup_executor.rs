//! v7 TAE — Setup Executor.
//!
//! Consumes the MME's top setup (the best Actionable/READY profile across
//! the 4 latest completed snapshots per symbol) and manages the trade
//! lifecycle: Idle → PendingEntry (limit @ zone midpoint) → PositionOpen
//! (TP/SL bracket) → Closed.
//!
//! Invalidation semantics (see docs/engines/trade-automation-engine/
//! 03-03-01-tae-overview-spec.md §6):
//!   - LEVEL:    price crossed the SL/invalidation level → pending cancelled /
//!     SL stop fills.
//!   - SIGNAL:   MME direction flipped opposite on a completed candle →
//!     pending cancelled / position closed at market.
//!   - REPLACED: a different setup type tops the ranking → pending cancelled.
//!
//! Neutral / STAND_ASIDE never invalidates an open position.

use config_models::{
    Direction, MinimalTaeConfig, OrderPacket, OrderSide, OrderType, StrategyConfig,
};
use core_domain::models::MarketSnapshot;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::execution::engine::ExecutionEngine;

/// The accepted setup — everything the executor needs to trade it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPlan {
    pub symbol: String,
    pub direction: String, // "LONG" | "SHORT"
    pub setup_type: String,
    pub score: f64,
    pub source_tf: String,
    pub source_tf_secs: u64,
    pub entry_mid: Decimal,
    pub entry_zone_low: Decimal,
    pub entry_zone_high: Decimal,
    pub sl: Decimal,
    pub tp: Decimal,
    /// v10: target-zone edges (TP placement dial).
    pub target_zone_low: Decimal,
    pub target_zone_high: Decimal,
    pub net_rr: f64,
    pub time_horizon: String,
    /// `decision.score_confidence` (0..=1) of the source snapshot.
    pub confidence: f64,
    /// `decision.trade_readiness` of the source snapshot.
    pub readiness: String,
    /// Idempotency key: symbol:direction:setup_type:candle_timestamp.
    pub fingerprint: String,
    /// v10: candle timestamp of the source snapshot (age gates).
    pub source_candle_ts: u64,
}

impl SetupPlan {}

/// Risk projection for the active setup (mirrors the RecommendationPanel's
/// "Projected Risk and Return" drawer, computed automatically).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupProjection {
    pub risk_capital: Decimal,
    pub position_size_units: Decimal,
    pub position_notional: Decimal,
    pub margin_required: Decimal,
    pub liquidation_price: Decimal,
    pub entry_fee_usd: Decimal,
    pub exit_fee_usd: Decimal,
    pub total_fees: Decimal,
    pub net_profit_usd: Decimal,
    pub roi_pct: Decimal,
    pub net_rr: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutorPhase {
    Idle,
    PendingEntry,
    PositionOpen,
}

/// v9 params-at-entry freeze: the exit/recovery knobs from the strategy
/// that was bound when the setup was accepted. Recharge affects new
/// setups only (V-12 / V-13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenEntryParams {
    pub breakeven_at_rr: Option<f64>,
    pub trailing_activate_rr: Option<f64>,
    pub trailing_atr_mult: Option<f64>,
    pub time_stop_bars: Option<u32>,
    pub entry_candle_ts: u64,
    pub pending_confirmation_bars_left: u32,
    pub last_seen_candle_ts: u64,
    /// v9 re-entry cooldown in bars (0 = only the close candle is guarded).
    pub reentry_cooldown_bars: u32,
    /// v9 vol-scale factor applied at entry (frozen; recharge affects new
    /// setups only).
    pub vol_factor: f64,
    /// v10: source-snapshot confidence (0..=1) at acceptance — the
    /// baseline for `tae.risk.confidence_drop_pct` exits.
    pub entry_confidence: f64,
}

impl FrozenEntryParams {
    pub fn from_strategy(
        st: &StrategyConfig,
        candle_ts: u64,
        vol_factor: f64,
        confidence: f64,
    ) -> Self {
        Self {
            breakeven_at_rr: st.tae.risk.breakeven_at_rr,
            trailing_activate_rr: st.tae.risk.trailing.as_ref().and_then(|t| t.activate_at_rr),
            trailing_atr_mult: st.tae.risk.trailing.as_ref().and_then(|t| t.atr_mult),
            time_stop_bars: st.tae.risk.time_stop_bars,
            entry_candle_ts: candle_ts,
            pending_confirmation_bars_left: st.tae.intake.confirmation_bars,
            last_seen_candle_ts: candle_ts,
            reentry_cooldown_bars: st.tae.lifecycle.reentry_cooldown_bars,
            vol_factor,
            entry_confidence: confidence,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolState {
    pub phase: ExecutorPhase,
    pub fingerprint: String,
    pub tracked_setup: Option<SetupPlan>,
    pub projection: Option<SetupProjection>,
    pub entry_order_id: Option<String>,
    pub tp_order_id: Option<String>,
    pub sl_order_id: Option<String>,
    /// v10: the resting entry is a market order (chase / market_on_ready).
    pub entry_is_market: bool,
    /// Candle timestamp that produced the last close — re-entry guard.
    pub last_closed_candle_ts: u64,
    /// v9 params-at-entry freeze (set on acceptance, cleared on reset).
    pub frozen: Option<FrozenEntryParams>,
}

impl Default for SymbolState {
    fn default() -> Self {
        Self {
            phase: ExecutorPhase::Idle,
            fingerprint: String::new(),
            tracked_setup: None,
            projection: None,
            entry_order_id: None,
            tp_order_id: None,
            sl_order_id: None,
            entry_is_market: false,
            last_closed_candle_ts: 0,
            frozen: None,
        }
    }
}

/// Per-tick context supplied by the daemon (reads instance state).
#[derive(Clone)]
pub struct TickContext {
    pub safety_allows_entry: bool,
    pub lifecycle_running: bool,
    /// v9 intake gates (strategy-derived, enforced by the daemon):
    /// breadth floor, exposure limits, margin close-only, systemic veto.
    pub market_filter_allows_entry: bool,
    /// Human-readable block label when a gate refuses the entry
    /// (rendered in the activity log + dashboard).
    pub entry_block_reason: Option<String>,
    /// Candle timestamp of the top snapshot (re-entry guard).
    pub candle_ts: u64,
    /// Per-instance SafetyManager (informational PME state). When present,
    /// the executor feeds `record_trade_outcome` on position close so the
    /// CAUTIOUS / SUSPENDED ladder stays accurate.
    pub safety: Option<Arc<crate::safety::SafetyManager>>,
    /// When `false` the executor evaluates setups and populates
    /// `tracked_setup` / `projection` (the would-be preview) but never
    /// submits, cancels, or closes orders. The daemon sets this to `false`
    /// for observe instances — the "ghost" radar.
    pub dispatch: bool,
    /// v8.2 per-instance allocation override (percent of portfolio equity,
    /// 1..=100). `None` = the global `[workspace.minimal_tae].allocation_pct`.
    pub allocation_pct: Option<f64>,
    /// v9: the instance's bound strategy snapshot (patch-resolved). The
    /// executor freezes its exit params at entry and enforces the intake
    /// gates from it. `None` = the legacy global defaults apply.
    pub strategy: Option<StrategyConfig>,
}

/// v10 lifecycle-hardening policy — the executor's resolved view of the
/// strategy's `tae` dials (defaults when no strategy is bound).
#[derive(Debug, Clone)]
pub(crate) struct StrategyPolicy {
    pub setup_gone: String,     // balanced | strict | risky
    pub replace_policy: String, // cancel_and_adopt | cancel
    pub min_reprice_delta_atr: f64,
    pub entry_mode: String, // zone_midpoint | zone_edge | zone_any | market_on_ready | chase
    pub chase_max_atr: f64,
    pub chase_score_floor: f64,
    pub instant_fill_policy: String, // take_better | cancel
    pub spread_gate_bps: Option<f64>,
    pub tp_placement: String, // zone_near_edge | zone_midpoint | zone_far_edge
    pub sl_mode: String,      // invalidation | invalidation_padded | atr_anchored
    pub sl_padding_atr: f64,
    pub atr_anchor_mult: f64,
    pub min_sl_atr: Option<f64>,
    pub stop_floor_source: String, // l6_formula | atr_mult | zone_only
    pub stop_floor_atr_mult: f64,
    pub max_tp_rr: f64,
    pub max_setup_age_bars: Option<u32>,
    pub pending_entry_expiry_bars: Option<u32>,
    pub tp_refresh_min_rr_delta: f64,
    pub confidence_drop_pct: Option<f64>,
}

impl StrategyPolicy {
    pub fn from_strategy(st: Option<&StrategyConfig>) -> Self {
        let d_intake = config_models::TaeIntake::default();
        let d_life = config_models::TaeLifecycle::default();
        let d_exec = config_models::TaeExecution::default();
        let d_risk = config_models::TaeRisk::default();
        let intake = st.map(|s| &s.tae.intake).unwrap_or(&d_intake);
        let life = st.map(|s| &s.tae.lifecycle).unwrap_or(&d_life);
        let exec = st.map(|s| &s.tae.execution).unwrap_or(&d_exec);
        let risk = st.map(|s| &s.tae.risk).unwrap_or(&d_risk);
        // v11: ladder_roles = short-TF intent → market entries (a resting
        // zone limit at 1m rarely fills). Explicit `entry_mode` still wins.
        let roles_on = st.map(|s| s.ladder_roles.enabled).unwrap_or(false);
        let entry_mode = if roles_on && exec.entry_mode == "zone_midpoint" {
            "market_on_ready".to_string()
        } else {
            exec.entry_mode.clone()
        };
        Self {
            setup_gone: risk.setup_gone_policy.clone(),
            replace_policy: life.replace_policy.clone(),
            min_reprice_delta_atr: life.min_reprice_delta_atr,
            entry_mode,
            chase_max_atr: exec.chase_max_atr,
            chase_score_floor: exec.chase_score_floor,
            instant_fill_policy: exec.instant_fill_policy.clone(),
            spread_gate_bps: exec.spread_gate_bps,
            tp_placement: exec.tp_placement.clone(),
            sl_mode: risk.sl_mode.clone(),
            sl_padding_atr: risk.sl_padding_atr,
            atr_anchor_mult: risk.atr_anchor_mult,
            min_sl_atr: risk.min_sl_atr,
            stop_floor_source: risk.stop_floor_source.clone(),
            stop_floor_atr_mult: risk.stop_floor_atr_mult,
            max_tp_rr: exec.max_tp_rr,
            max_setup_age_bars: intake.max_setup_age_bars,
            pending_entry_expiry_bars: life.pending_entry_expiry_bars,
            tp_refresh_min_rr_delta: risk.tp_refresh_min_rr_delta,
            confidence_drop_pct: risk.confidence_drop_pct,
        }
    }

    /// Effective pending-entry expiry: balanced posture defaults to 12 bars
    /// when the dial is unset; risky keeps the dial (None = immortal);
    /// strict cancels on gone long before expiry matters but honors the
    /// dial otherwise.
    pub fn effective_expiry_bars(&self) -> Option<u32> {
        match self.setup_gone.as_str() {
            "balanced" | "strict" => Some(self.pending_entry_expiry_bars.unwrap_or(12)),
            _ => self.pending_entry_expiry_bars,
        }
    }
}

/// Source-TF ATR from the completed snapshots (raw value, > 0).
fn source_atr(snapshots: &[&MarketSnapshot], source_tf_secs: u64) -> f64 {
    snapshots
        .iter()
        .find(|s| s.is_completed == Some(true) && s.timeframe_secs == source_tf_secs)
        .and_then(|s| s.indicators.get("atr"))
        .map(|v| v.raw_value)
        .filter(|a| *a > 0.0)
        .unwrap_or(0.0)
}

/// Marketable when the resting limit would cross the mid in our favor:
/// a LONG buy limit is marketable when mid ≤ limit (we get filled at ≤
/// limit); a SHORT sell limit when mid ≥ limit.
fn marketable(direction: &str, limit: Decimal, mid: Decimal) -> bool {
    if direction == "LONG" {
        mid <= limit
    } else {
        mid >= limit
    }
}

/// R-multiple distance from entry to TP against the stop.
fn rr_units(direction: &str, entry: Decimal, tp: Decimal, sl: Decimal) -> f64 {
    let risk = if direction == "LONG" {
        entry - sl
    } else {
        sl - entry
    };
    if risk <= dec!(0) {
        return 0.0;
    }
    let reward = if direction == "LONG" {
        tp - entry
    } else {
        entry - tp
    };
    (reward / risk).to_f64().unwrap_or(0.0)
}

impl SetupPlan {
    /// v10 effective plan: entry/SL/TP re-derived from the strategy dials.
    /// `entry` is `None` for market-on-* modes (market order at dispatch).
    pub(crate) fn effective(
        &self,
        policy: &StrategyPolicy,
        atr: f64,
        mid: Decimal,
    ) -> (SetupPlan, Option<Decimal>) {
        let mut eff = self.clone();
        let is_long = self.direction == "LONG";
        let mut market_entry = false;

        let resting = match policy.entry_mode.as_str() {
            "zone_edge" => {
                if is_long {
                    self.entry_zone_low
                } else {
                    self.entry_zone_high
                }
            }
            "zone_any" => {
                if is_long {
                    self.entry_zone_high
                } else {
                    self.entry_zone_low
                }
            }
            "market_on_ready" => {
                market_entry = true;
                self.entry_mid
            }
            "chase" => {
                let near = if is_long {
                    self.entry_zone_high
                } else {
                    self.entry_zone_low
                };
                let beyond = if is_long { mid > near } else { mid < near };
                if beyond && atr > 0.0 && self.score >= policy.chase_score_floor {
                    let distance = if is_long { mid - near } else { near - mid };
                    let tol =
                        Decimal::from_f64_retain(policy.chase_max_atr * atr).unwrap_or(dec!(0));
                    if distance <= tol {
                        market_entry = true;
                    }
                }
                near
            }
            _ => self.entry_mid,
        };

        let entry = if market_entry { mid } else { resting };
        eff.entry_mid = entry;

        // SL dial.
        let sl = match policy.sl_mode.as_str() {
            "invalidation_padded" => {
                let pad = Decimal::from_f64_retain(policy.sl_padding_atr * atr).unwrap_or(dec!(0));
                if is_long {
                    (self.sl - pad).max(dec!(0))
                } else {
                    self.sl + pad
                }
            }
            "atr_anchored" => {
                let span =
                    Decimal::from_f64_retain(policy.atr_anchor_mult * atr).unwrap_or(dec!(0));
                if is_long {
                    (entry - span).max(dec!(0))
                } else {
                    entry + span
                }
            }
            _ => self.sl,
        };
        eff.sl = sl;

        // TP placement dial (target-zone edge selection).
        eff.tp = match policy.tp_placement.as_str() {
            "zone_near_edge" => {
                if is_long {
                    self.target_zone_low
                } else {
                    self.target_zone_high
                }
            }
            "zone_far_edge" => {
                if is_long {
                    self.target_zone_high
                } else {
                    self.target_zone_low
                }
            }
            _ => self.tp,
        };

        let entry_order = if market_entry { None } else { Some(entry) };
        (eff, entry_order)
    }

    /// v11 execution dials — stop floor + TP reachability cap.
    /// Applied at entry acceptance AND bracket refresh (ratchet) so a fresh
    /// setup can never undo the floor by ratcheting the SL into noise.
    /// `anchor` is the reference price: the zone midpoint at acceptance, the
    /// actual fill price during a ratchet (floor must track the real entry).
    pub(crate) fn apply_execution_dials(
        &mut self,
        policy: &StrategyPolicy,
        atr: f64,
        _snapshots: &[&MarketSnapshot],
        anchor: Decimal,
    ) {
        let is_long = self.direction == "LONG";
        // Stop floor: SL = max(zone, anchor ∓ floor).
        let floor_dist = match policy.stop_floor_source.as_str() {
            // CR-15: the `l6_formula` floor was read from
            // `advisory.stop_loss_distance_pct`. The advisory no longer crosses
            // the model boundary, so this branch has no input and yields no
            // floor. `atr_mult` is unaffected because it needs nothing but the
            // candle-derived ATR. Restoring the formula is TAE work: it now
            // has to compute its own advisory from model state.
            "l6_formula" => dec!(0),
            "atr_mult" => {
                Decimal::from_f64_retain(policy.stop_floor_atr_mult * atr).unwrap_or(dec!(0))
            }
            _ => dec!(0),
        };
        if floor_dist > dec!(0) {
            let floored_sl = if is_long {
                (anchor - floor_dist).max(dec!(0))
            } else {
                anchor + floor_dist
            };
            let zone_dist = if is_long {
                anchor - self.sl
            } else {
                self.sl - anchor
            };
            if zone_dist < floor_dist {
                self.sl = floored_sl;
            }
        } else if let Some(k) = policy.min_sl_atr {
            if atr > 0.0 {
                let distance = if is_long {
                    anchor - self.sl
                } else {
                    self.sl - anchor
                };
                let min_dist = Decimal::from_f64_retain(k * atr).unwrap_or(dec!(0));
                if distance < min_dist {
                    self.sl = if is_long {
                        anchor - min_dist
                    } else {
                        anchor + min_dist
                    };
                }
            }
        }

        // TP reachability cap: TP = anchor ± min(net_rr, max_tp_rr) × SL distance.
        let sl_dist = if is_long {
            anchor - self.sl
        } else {
            self.sl - anchor
        };
        if sl_dist > dec!(0) {
            let tp_dist = if is_long {
                self.tp - anchor
            } else {
                anchor - self.tp
            };
            let rr = (tp_dist / sl_dist).to_f64().unwrap_or(0.0);
            if rr > policy.max_tp_rr {
                let capped =
                    sl_dist * Decimal::from_f64_retain(policy.max_tp_rr).unwrap_or(dec!(1.5));
                self.tp = if is_long {
                    anchor + capped
                } else {
                    anchor - capped
                };
            }
        }
    }
}

/// Extract the top setup from the latest completed snapshots of the
/// ACTIVE durations (one per duration, fastest → slowest).
///
/// Selection: candidates = profiles with `preconditions_met > 0`,
/// `trade_viability == Actionable`, geometry consistent for the active side
/// (from `analysis.bias`); then the RR filter; then the highest score across
/// all timeframes (ties → faster TF wins).
/// CR-15 — this function no longer derives anything, and that is the point.
///
/// It used to pick the best setup out of `snap.decision_context` +
/// `snap.opportunity`, both of which were L4/L6 strategy matrices computed by
/// the Market Monitoring Engine. MME now stops at the model
/// ([01-21](../../../conceptual-foundations/01-21-engine-split-and-model-boundary.md)
/// §3): it reports what the market is doing and never what to do about it. So
/// the two inputs the derivation needs do not cross the boundary, and no
/// derivation is possible from a live snapshot.
///
/// **The consequence, stated plainly:** the setup executor has no setups in this
/// build. Paper and live are WIP and not selectable, which is precisely why the
/// boundary could be drawn here. Restoring the executor means computing the
/// advisory and the opportunity **inside TAE** from model state — future TAE
/// work, and not a thing this phase pretends to have done.
///
/// `min_net_rr` is accepted and ignored so the signature stays stable for that
/// future work and for its tests.
pub fn extract_top_setup(_snapshots: &[&MarketSnapshot], _min_net_rr: f64) -> Option<SetupPlan> {
    None
}

/// The v7 setup executor — one instance per daemon, one `SymbolState` per
/// symbol. All effects go through the unified `ExecutionEngine`.
pub struct SetupExecutor {
    pub min_net_rr: f64,
    pub default_allocation_pct: f64,
    pub max_position_size_pct_of_equity: Option<f64>,
    pub max_open_positions: u32,
    pub engine: Arc<ExecutionEngine>,
    state: RwLock<HashMap<String, SymbolState>>,
    paused_log: RwLock<HashMap<String, u64>>,
}

impl SetupExecutor {
    pub fn new(engine: Arc<ExecutionEngine>, cfg: &MinimalTaeConfig) -> Self {
        Self {
            min_net_rr: cfg.min_net_rr,
            default_allocation_pct: cfg.allocation_pct,
            max_position_size_pct_of_equity: cfg.max_position_size_pct_of_equity,
            max_open_positions: cfg.max_open_positions,
            engine,
            state: RwLock::new(HashMap::new()),
            paused_log: RwLock::new(HashMap::new()),
        }
    }

    pub async fn state(&self, symbol: &str) -> SymbolState {
        self.state
            .read()
            .await
            .get(symbol)
            .cloned()
            .unwrap_or_default()
    }

    /// One executor tick per instance: evaluate the current top setup and
    /// advance the per-symbol state machine.
    pub async fn tick(
        &self,
        instance_id: &str,
        symbol: &str,
        snapshots: Vec<&MarketSnapshot>,
        mid: Decimal,
        ctx: TickContext,
    ) {
        let top = extract_top_setup(&snapshots, self.min_net_rr);
        let policy = StrategyPolicy::from_strategy(ctx.strategy.as_ref());
        let mut state = self.state.write().await;
        let entry = state.entry(symbol.to_string()).or_default();

        match entry.phase {
            ExecutorPhase::Idle => {
                self.tick_idle(
                    instance_id,
                    symbol,
                    &top,
                    &snapshots,
                    mid,
                    ctx,
                    &policy,
                    entry,
                )
                .await
            }
            ExecutorPhase::PendingEntry => {
                self.tick_pending(
                    instance_id,
                    symbol,
                    &top,
                    &snapshots,
                    mid,
                    ctx,
                    &policy,
                    entry,
                )
                .await
            }
            ExecutorPhase::PositionOpen => {
                self.tick_position(
                    instance_id,
                    symbol,
                    &top,
                    &snapshots,
                    mid,
                    ctx,
                    &policy,
                    entry,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn tick_idle(
        &self,
        instance_id: &str,
        symbol: &str,
        top: &Option<SetupPlan>,
        snapshots: &[&MarketSnapshot],
        mid: Decimal,
        ctx: TickContext,
        policy: &StrategyPolicy,
        entry: &mut SymbolState,
    ) {
        let Some(plan) = top else { return };
        if !ctx.lifecycle_running || !ctx.safety_allows_entry || !ctx.market_filter_allows_entry {
            if !ctx.lifecycle_running {
                // Rate-limited heartbeat (60s per symbol) so operator sees why no trades
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let mut last = self.paused_log.write().await;
                let last_ts = last.get(symbol).copied().unwrap_or(0);
                if now - last_ts >= 60 {
                    last.insert(symbol.to_string(), now);
                    drop(last);
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        "TAE PAUSED — instance not activated (use --tae-on or POST /api/instances/:id/lifecycle {\"action\":\"start\"})",
                    )
                    .await;
                }
            }
            if !ctx.market_filter_allows_entry {
                self.log(
                    instance_id,
                    symbol,
                    "entry_blocked",
                    ctx.entry_block_reason
                        .as_deref()
                        .unwrap_or("market filter blocked"),
                )
                .await;
            }
            return;
        }
        // No re-entry on the candle that produced the last close.
        if ctx.candle_ts > 0 && ctx.candle_ts == entry.last_closed_candle_ts {
            return;
        }
        // One position per symbol + global cap.
        let positions = self.engine.positions.read().await;
        if positions.contains_key(symbol) {
            return;
        }

        // ── v9 strategy intake gates (TAE `intake` section) ──
        if let Some(st) = &ctx.strategy {
            let intake = &st.tae.intake;
            if let Some(min_score) = intake.min_score {
                if plan.score < min_score {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        &format!(
                            "strategy min_score {:.0} — setup scored {:.0}",
                            min_score, plan.score
                        ),
                    )
                    .await;
                    return;
                }
            }
            if let Some(min_conf) = intake.min_confidence {
                if plan.confidence < min_conf {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        &format!(
                            "strategy min_confidence {:.2} — setup confidence {:.2}",
                            min_conf, plan.confidence
                        ),
                    )
                    .await;
                    return;
                }
            }
            match intake.direction_policy.as_str() {
                "long_only" if plan.direction == "SHORT" => {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        "strategy direction_policy=long_only rejected a SHORT setup",
                    )
                    .await;
                    return;
                }
                "short_only" if plan.direction == "LONG" => {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        "strategy direction_policy=short_only rejected a LONG setup",
                    )
                    .await;
                    return;
                }
                _ => {}
            }
            if intake.execution_veto.iter().any(|v| v == "risk_blocked")
                && plan.readiness == "STAND_ASIDE"
            {
                self.log(
                    instance_id,
                    symbol,
                    "entry_blocked",
                    "strategy execution_veto=risk_blocked — source snapshot is STAND_ASIDE",
                )
                .await;
                return;
            }
            // v9 re-entry cooldown: refuse entries within N bars of the
            // last close (0 = the close candle itself is guarded below).
            let cooldown_bars = st.tae.lifecycle.reentry_cooldown_bars;
            if cooldown_bars > 0
                && ctx.candle_ts > 0
                && entry.last_closed_candle_ts > 0
                && ctx.candle_ts >= entry.last_closed_candle_ts
                && plan.source_tf_secs > 0
            {
                let bars_since_close =
                    (ctx.candle_ts - entry.last_closed_candle_ts) / plan.source_tf_secs;
                if bars_since_close < cooldown_bars as u64 {
                    return;
                }
            }
        }
        let open_count = positions.len() as u32;
        drop(positions);
        if open_count >= self.max_open_positions {
            return;
        }

        // ── v10 policy gates (entry dial) ──
        let atr = source_atr(snapshots, plan.source_tf_secs);
        if let Some(age_bars) = policy.max_setup_age_bars {
            if ctx.candle_ts > 0
                && plan.source_tf_secs > 0
                && ctx.candle_ts >= plan.source_candle_ts
            {
                let age = (ctx.candle_ts - plan.source_candle_ts) / plan.source_tf_secs;
                if age >= age_bars as u64 {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        &format!("setup too old — {age} bars (max {age_bars})"),
                    )
                    .await;
                    return;
                }
            }
        }
        if let Some(gate_bps) = policy.spread_gate_bps {
            let spread_bps = snapshots
                .iter()
                .find(|s| s.is_completed == Some(true) && s.timeframe_secs == plan.source_tf_secs)
                .and_then(|s| {
                    let b = s.bid_price.to_f64()?;
                    let a = s.ask_price.to_f64()?;
                    let m = s.mid_price.to_f64()?;
                    if m > 0.0 {
                        Some((a - b) / m * 10000.0)
                    } else {
                        None
                    }
                });
            if spread_bps.is_some_and(|s| s > gate_bps) {
                self.log(
                    instance_id,
                    symbol,
                    "entry_blocked",
                    &format!(
                        "spread {:.1} bps exceeds gate {gate_bps} bps",
                        spread_bps.unwrap_or(0.0)
                    ),
                )
                .await;
                return;
            }
        }

        // Effective plan: entry/SL/TP re-derived from the strategy dials.
        let (mut eff_plan, entry_price) = plan.effective(policy, atr, mid);

        // v11 stop floor + TP cap — shared with the ratchet (never undone).
        eff_plan.apply_execution_dials(policy, atr, snapshots, eff_plan.entry_mid);
        // instant_fill_policy = cancel: refuse when the price is already
        // beyond the zone and the resting limit would be marketable.
        if policy.instant_fill_policy == "cancel" {
            if let Some(limit) = entry_price {
                let beyond_zone = if eff_plan.direction == "LONG" {
                    mid < eff_plan.entry_zone_low
                } else {
                    mid > eff_plan.entry_zone_high
                };
                if beyond_zone && marketable(&eff_plan.direction, limit, mid) {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        "instant_fill_policy=cancel — price beyond zone at dispatch",
                    )
                    .await;
                    return;
                }
            }
        }

        // ── v9 sizing: strategy `tae.sizing` is the source of truth ──
        let Some((projection, vol_factor)) = self
            .resolve_projection(instance_id, symbol, &eff_plan, snapshots, &ctx)
            .await
        else {
            return;
        };
        let size = projection.position_size_units;
        if size <= dec!(0) {
            return;
        }

        let side = if eff_plan.direction == "LONG" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("trigger_source".to_string(), eff_plan.setup_type.clone());

        let packet = OrderPacket {
            client_order_id: format!("setup_entry_{}", symbol),
            symbol: symbol.to_string(),
            side,
            order_type: if entry_price.is_some() {
                OrderType::Limit
            } else {
                OrderType::Market
            },
            price: entry_price,
            size,
            reduce_only: false,
            is_emergency_liquidation: false,
            associated_position_id: None,
            metadata,
        };

        // v9 params-at-entry freeze: stamp the exit knobs NOW (recharge
        // affects new setups only). v10: always stamped — without a bound
        // strategy the platform defaults apply, so the balanced posture's
        // expiry and confidence-drop baselines work identically.
        let frozen = match &ctx.strategy {
            Some(st) => {
                FrozenEntryParams::from_strategy(st, ctx.candle_ts, vol_factor, eff_plan.confidence)
            }
            None => {
                let def = StrategyConfig::default();
                FrozenEntryParams::from_strategy(
                    &def,
                    ctx.candle_ts,
                    vol_factor,
                    eff_plan.confidence,
                )
            }
        };
        entry.frozen = Some(frozen);

        let entry_label = entry_price
            .map(|p| p.to_string())
            .unwrap_or_else(|| "MARKET".to_string());
        if !ctx.dispatch {
            // Ghost (observe) evaluation: record the would-be setup and
            // projection without dispatching any order. No entry_order_id
            // is stored, so tick_pending never sees a fill; the LEVEL /
            // SIGNAL / REPLACED invalidation logic keeps re-evaluating the
            // candidate until it dies or is replaced.
            entry.phase = ExecutorPhase::PendingEntry;
            entry.fingerprint = eff_plan.fingerprint.clone();
            entry.tracked_setup = Some(eff_plan.clone());
            entry.entry_is_market = entry_price.is_none();
            entry.projection = Some(projection);
            self.log(
                instance_id,
                symbol,
                "setup_accepted",
                &format!(
                    "GHOST {} {} entry={} sl={} tp={} rr={:.2} score={:.0} tf={}",
                    eff_plan.direction,
                    eff_plan.setup_type,
                    entry_label,
                    eff_plan.sl,
                    eff_plan.tp,
                    eff_plan.net_rr,
                    eff_plan.score,
                    eff_plan.source_tf
                ),
            )
            .await;
        } else if entry
            .frozen
            .as_ref()
            .is_some_and(|f| f.pending_confirmation_bars_left > 0)
        {
            // v9 confirmation hold: the strategy demands N completed bars
            // before dispatch — tick_pending submits once the countdown
            // elapses.
            entry.phase = ExecutorPhase::PendingEntry;
            entry.fingerprint = eff_plan.fingerprint.clone();
            entry.tracked_setup = Some(eff_plan.clone());
            entry.entry_is_market = entry_price.is_none();
            entry.projection = Some(projection);
            self.log(
                instance_id,
                symbol,
                "setup_accepted",
                &format!(
                    "CONFIRMING {} {} — dispatch after {} bar(s)",
                    eff_plan.direction,
                    eff_plan.setup_type,
                    entry
                        .frozen
                        .as_ref()
                        .unwrap()
                        .pending_confirmation_bars_left
                ),
            )
            .await;
        } else {
            match self.engine.submit_order(packet, mid).await {
                Ok(order_id) => {
                    entry.phase = ExecutorPhase::PendingEntry;
                    entry.fingerprint = eff_plan.fingerprint.clone();
                    entry.tracked_setup = Some(eff_plan.clone());
                    entry.entry_is_market = entry_price.is_none();
                    entry.projection = Some(projection);
                    entry.entry_order_id = Some(order_id);
                    self.log(
                        instance_id,
                        symbol,
                        "setup_accepted",
                        &format!(
                            "{} {} entry={} sl={} tp={} rr={:.2} score={:.0} tf={}",
                            eff_plan.direction,
                            eff_plan.setup_type,
                            entry_label,
                            eff_plan.sl,
                            eff_plan.tp,
                            eff_plan.net_rr,
                            eff_plan.score,
                            eff_plan.source_tf
                        ),
                    )
                    .await;
                }
                Err(e) => {
                    self.log(instance_id, symbol, "entry_rejected", &e).await;
                }
            }
        }
    }

    /// v9 sizing cascade (factored for accept + re-price): allocation
    /// resolution, per-setup multiplier, after-loss step-down, vol-scale,
    /// exposure cap, and the canonical projection. Returns the projection
    /// with the vol factor (for the params-at-entry freeze).
    async fn resolve_projection(
        &self,
        instance_id: &str,
        symbol: &str,
        plan: &SetupPlan,
        snapshots: &[&MarketSnapshot],
        ctx: &TickContext,
    ) -> Option<(SetupProjection, f64)> {
        let sizing = ctx
            .strategy
            .as_ref()
            .map(|st| &st.tae.sizing)
            .cloned()
            .unwrap_or_default();
        // Per-instance override wins; then the strategy; then the global.
        let mut allocation = ctx.allocation_pct.unwrap_or_else(|| {
            ctx.strategy
                .as_ref()
                .map(|st| st.tae.sizing.allocation_pct)
                .unwrap_or(self.default_allocation_pct)
        });
        // Per-setup-type multiplier (default 1.0).
        if let Some(mult) = sizing.per_setup_type_multipliers.get(&plan.setup_type) {
            allocation *= *mult;
        }
        // After-loss step-down: consecutive losses on this symbol shrink
        // the next allocation by `reduce_pct`.
        if let Some(step) = &sizing.after_loss_step_down {
            if let Some(safety) = &ctx.safety {
                let losses = safety
                    .consecutive_losses
                    .read()
                    .await
                    .get(symbol)
                    .copied()
                    .unwrap_or(0);
                if losses >= step.after_losses {
                    allocation *= (1.0 - step.reduce_pct / 100.0).max(0.0);
                }
            }
        }
        // v9 vol-scale: fixed = override factor; auto = source-TF ATR%
        // relative to the macro TF's ATR% (calmer source ⇒ larger size,
        // hotter source ⇒ smaller). Frozen at entry.
        let vol_factor = ctx
            .strategy
            .as_ref()
            .map(|st| {
                match st.tae.sizing.vol_scale.mode.as_str() {
                    "fixed" => st.tae.sizing.vol_scale.override_factor.unwrap_or(1.0),
                    // auto (default)
                    _ => {
                        let atr_pct = |tf_secs: Option<u64>| -> Option<f64> {
                            snapshots
                                .iter()
                                .find(|s| {
                                    s.is_completed == Some(true)
                                        && tf_secs.map(|t| s.timeframe_secs == t).unwrap_or(true)
                                })
                                .and_then(|s| {
                                    let atr = s
                                        .indicators
                                        .get("atr")
                                        .and_then(|v| v.values.as_ref())
                                        .and_then(|m| m.get("atr_14").copied())?;
                                    let mid = s.mid_price.to_f64()?;
                                    if mid > 0.0 && atr > 0.0 {
                                        Some(atr / mid)
                                    } else {
                                        None
                                    }
                                })
                        };
                        let src = atr_pct(Some(plan.source_tf_secs));
                        let mac = atr_pct(
                            snapshots
                                .iter()
                                .filter(|s| s.is_completed == Some(true))
                                .map(|s| s.timeframe_secs)
                                .max(),
                        );
                        match (src, mac) {
                            (Some(s), Some(m)) if m > 0.0 => (m / s).clamp(0.25, 4.0),
                            _ => 1.0,
                        }
                    }
                }
            })
            .unwrap_or(1.0);
        allocation *= vol_factor;
        // v9 `max_total_exposure_pct`: refuse when the GROSS portfolio
        // exposure after this entry would exceed the cap.
        if let Some(cap_pct) = sizing.max_total_exposure_pct {
            let equity = self.engine.get_equity_decimal().await;
            if equity > dec!(0) {
                let gross: Decimal = self
                    .engine
                    .positions
                    .read()
                    .await
                    .values()
                    .map(|p| p.size * p.entry_price)
                    .fold(dec!(0), |acc, v| acc + v);
                // v11.12: `?` instead of let-else (clippy) — the closure
                // already returns Option.
                let alloc_dec = Decimal::from_f64_retain(allocation / 100.0)?;
                let prospective = gross + equity * alloc_dec;
                let cap_dec = Decimal::from_f64_retain(cap_pct)?;
                if prospective / equity * dec!(100) > cap_dec {
                    self.log(
                        instance_id,
                        symbol,
                        "entry_blocked",
                        &format!("max_total_exposure_pct {cap_pct} would be exceeded"),
                    )
                    .await;
                    return None;
                }
            }
        }
        let projection = self.project(plan, allocation, vol_factor).await?;
        Some((projection, vol_factor))
    }

    #[allow(clippy::too_many_arguments)]
    async fn tick_pending(
        &self,
        instance_id: &str,
        symbol: &str,
        top: &Option<SetupPlan>,
        snapshots: &[&MarketSnapshot],
        mid: Decimal,
        ctx: TickContext,
        policy: &StrategyPolicy,
        entry: &mut SymbolState,
    ) {
        // ── v10.1 lifecycle gate: PAUSED (close-only) cancels any resting
        // entry — a pending limit must not fill after the operator
        // deactivated the TAE. Brackets/positions are never touched here
        // (tick_position manages them in every lifecycle state).
        if !ctx.lifecycle_running {
            if let Some(id) = entry.entry_order_id.take() {
                let _ = self.engine.cancel_order(&id, symbol).await;
            }
            if entry.frozen.is_some() || entry.entry_order_id.is_some() {
                self.log(
                    instance_id,
                    symbol,
                    "tae_paused",
                    "TAE paused — pending entry cancelled (close-only)",
                )
                .await;
                self.reset(entry);
            }
            return;
        }

        // ── v9 confirmation hold: submit once the countdown elapses ──
        if entry.entry_order_id.is_none() {
            if let Some(f) = entry.frozen.as_mut() {
                if f.pending_confirmation_bars_left > 0 {
                    if ctx.candle_ts > f.last_seen_candle_ts {
                        f.last_seen_candle_ts = ctx.candle_ts;
                        f.pending_confirmation_bars_left -= 1;
                    }
                    if f.pending_confirmation_bars_left > 0 {
                        return;
                    }
                    if ctx.dispatch {
                        if let Some(plan) = entry.tracked_setup.clone() {
                            let side = if plan.direction == "LONG" {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            };
                            let mut metadata = std::collections::HashMap::new();
                            metadata.insert("trigger_source".to_string(), plan.setup_type.clone());
                            let packet = OrderPacket {
                                client_order_id: format!("setup_entry_{}", symbol),
                                symbol: symbol.to_string(),
                                side,
                                order_type: if entry.entry_is_market {
                                    OrderType::Market
                                } else {
                                    OrderType::Limit
                                },
                                price: if entry.entry_is_market {
                                    None
                                } else {
                                    Some(plan.entry_mid)
                                },
                                size: entry
                                    .projection
                                    .as_ref()
                                    .map(|p| p.position_size_units)
                                    .unwrap_or(dec!(0)),
                                reduce_only: false,
                                is_emergency_liquidation: false,
                                associated_position_id: None,
                                metadata,
                            };
                            if packet.size > dec!(0) {
                                match self.engine.submit_order(packet, mid).await {
                                    Ok(order_id) => {
                                        entry.entry_order_id = Some(order_id);
                                        self.log(
                                            instance_id,
                                            symbol,
                                            "setup_dispatched",
                                            "confirmation window elapsed — entry order submitted",
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        self.log(instance_id, symbol, "entry_rejected", &e).await;
                                        self.reset(entry);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── v10 pending-entry expiry (posture-aware) ──
        if let (Some(f), Some(plan)) = (entry.frozen.as_ref(), entry.tracked_setup.as_ref()) {
            if let Some(expiry_bars) = policy.effective_expiry_bars() {
                if ctx.candle_ts >= f.entry_candle_ts && plan.source_tf_secs > 0 {
                    let age_bars = (ctx.candle_ts - f.entry_candle_ts) / plan.source_tf_secs;
                    if age_bars >= expiry_bars as u64 {
                        if let Some(id) = entry.entry_order_id.take() {
                            let _ = self.engine.cancel_order(&id, symbol).await;
                        }
                        self.log(
                            instance_id,
                            symbol,
                            "expired",
                            &format!("pending entry expired after {age_bars} bars"),
                        )
                        .await;
                        self.reset(entry);
                        return;
                    }
                }
            }
        }

        // ── Entry filled? ──
        let has_position = self.engine.get_position(symbol).await.is_some();
        if has_position {
            entry.phase = ExecutorPhase::PositionOpen;
            self.arm_bracket(instance_id, symbol, entry, snapshots, policy)
                .await;
            self.log(instance_id, symbol, "entry_filled", &entry.fingerprint)
                .await;
            return;
        }

        // ── LEVEL invalidation: price crossed the SL while pending ──
        if let Some(plan) = &entry.tracked_setup {
            let breached = if plan.direction == "LONG" {
                mid < plan.sl
            } else {
                mid > plan.sl
            };
            if breached {
                if let Some(id) = entry.entry_order_id.take() {
                    let _ = self.engine.cancel_order(&id, symbol).await;
                }
                self.log(
                    instance_id,
                    symbol,
                    "invalidated_level",
                    &format!("pending entry cancelled — price crossed SL {}", plan.sl),
                )
                .await;
                self.reset(entry);
                return;
            }
        }

        // ── top-setup handling: SIGNAL / REPLACED / re-price / gone ──
        let tracked = entry.tracked_setup.clone();
        match top {
            Some(plan) => {
                let Some(tracked) = tracked else { return };
                if plan.direction != tracked.direction {
                    // SIGNAL invalidation: direction flipped (posture-independent).
                    if let Some(id) = entry.entry_order_id.take() {
                        let _ = self.engine.cancel_order(&id, symbol).await;
                    }
                    self.log(
                        instance_id,
                        symbol,
                        "invalidated_signal",
                        &format!(
                            "pending entry cancelled — recommendation flipped to {}",
                            plan.direction
                        ),
                    )
                    .await;
                    self.reset(entry);
                    return;
                }
                if plan.setup_type != tracked.setup_type {
                    // REPLACED: different setup type tops the ranking.
                    if let Some(id) = entry.entry_order_id.take() {
                        let _ = self.engine.cancel_order(&id, symbol).await;
                    }
                    match policy.replace_policy.as_str() {
                        "cancel" => {
                            self.log(
                                instance_id,
                                symbol,
                                "cancelled_replaced",
                                &format!(
                                    "pending entry cancelled — setup replaced by {}",
                                    plan.setup_type
                                ),
                            )
                            .await;
                            self.reset(entry);
                        }
                        _ => {
                            self.log(
                                instance_id,
                                symbol,
                                "replaced_adopted",
                                &format!(
                                    "pending entry replaced — adopting {} in the same tick",
                                    plan.setup_type
                                ),
                            )
                            .await;
                            self.reset(entry);
                            // Adopt the replacement now (gates re-run).
                            self.tick_idle(
                                instance_id,
                                symbol,
                                top,
                                snapshots,
                                mid,
                                ctx,
                                policy,
                                entry,
                            )
                            .await;
                        }
                    }
                    return;
                }
                // Same direction + type: re-price the pending entry behind
                // the min-delta gate on fresh candles.
                if plan.fingerprint != tracked.fingerprint {
                    self.reprice_pending(
                        instance_id,
                        symbol,
                        plan,
                        snapshots,
                        mid,
                        &ctx,
                        policy,
                        entry,
                    )
                    .await;
                }
            }
            None => {
                // v10 R3: setup gone while pending — posture decides.
                if policy.setup_gone == "strict" {
                    if let Some(id) = entry.entry_order_id.take() {
                        let _ = self.engine.cancel_order(&id, symbol).await;
                    }
                    self.log(
                        instance_id,
                        symbol,
                        "setup_gone_cancel",
                        "pending entry cancelled — actionable setup gone (setup_gone_policy=strict)",
                    )
                    .await;
                    self.reset(entry);
                }
                // balanced/risky: keep pending (expiry governs).
            }
        }
    }

    /// v10 R4: re-price a pending entry to a fresh same-direction setup.
    /// Cancel-first-then-place (no double-fill window); the projection and
    /// tracked setup follow the new geometry.
    async fn reprice_pending(
        &self,
        instance_id: &str,
        symbol: &str,
        plan: &SetupPlan,
        snapshots: &[&MarketSnapshot],
        mid: Decimal,
        ctx: &TickContext,
        policy: &StrategyPolicy,
        entry: &mut SymbolState,
    ) {
        let Some(tracked) = entry.tracked_setup.clone() else {
            return;
        };
        let atr = source_atr(snapshots, plan.source_tf_secs);
        let (eff, entry_price) = plan.effective(policy, atr, mid);
        let delta = (eff.entry_mid - tracked.entry_mid).abs();
        let min_delta =
            Decimal::from_f64_retain(policy.min_reprice_delta_atr * atr).unwrap_or(dec!(0));
        if delta < min_delta {
            return;
        }

        // Fresh sizing/projection — the same cascade a fresh accept runs.
        let Some((projection, _vol)) = self
            .resolve_projection(instance_id, symbol, &eff, snapshots, ctx)
            .await
        else {
            return;
        };
        if projection.position_size_units <= dec!(0) {
            return;
        }

        if ctx.dispatch {
            let still_confirming = entry
                .frozen
                .as_ref()
                .is_some_and(|f| f.pending_confirmation_bars_left > 0);
            if let Some(old_id) = entry.entry_order_id.take() {
                let _ = self.engine.cancel_order(&old_id, symbol).await;
            }
            if !still_confirming {
                let side = if eff.direction == "LONG" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("trigger_source".to_string(), eff.setup_type.clone());
                let packet = OrderPacket {
                    client_order_id: format!("setup_entry_{}", symbol),
                    symbol: symbol.to_string(),
                    side,
                    order_type: if entry_price.is_some() {
                        OrderType::Limit
                    } else {
                        OrderType::Market
                    },
                    price: entry_price,
                    size: projection.position_size_units,
                    reduce_only: false,
                    is_emergency_liquidation: false,
                    associated_position_id: None,
                    metadata,
                };
                match self.engine.submit_order(packet, mid).await {
                    Ok(order_id) => {
                        entry.entry_order_id = Some(order_id);
                    }
                    Err(e) => {
                        self.log(instance_id, symbol, "entry_rejected", &e).await;
                        self.reset(entry);
                        return;
                    }
                }
            }
        }
        entry.tracked_setup = Some(eff.clone());
        entry.fingerprint = eff.fingerprint.clone();
        entry.projection = Some(projection);
        entry.entry_is_market = entry_price.is_none();
        self.log(
            instance_id,
            symbol,
            "reprice_pending",
            &format!(
                "entry re-priced {} → {} (sl={} tp={})",
                tracked.entry_mid, eff.entry_mid, eff.sl, eff.tp
            ),
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn tick_position(
        &self,
        instance_id: &str,
        symbol: &str,
        top: &Option<SetupPlan>,
        snapshots: &[&MarketSnapshot],
        mid: Decimal,
        ctx: TickContext,
        policy: &StrategyPolicy,
        entry: &mut SymbolState,
    ) {
        let has_position = self.engine.get_position(symbol).await.is_some();
        if !has_position {
            // Closed by TP/SL/manual — feed the PME safety ladder, then
            // back to Idle, armed against the candle that produced the close.
            if let Some(outcome) = self.engine.take_last_close(symbol).await {
                if let Some(safety) = &ctx.safety {
                    safety.record_trade_outcome(symbol, outcome.is_loss).await;
                }
            }
            let closed_on = if ctx.candle_ts > 0 {
                ctx.candle_ts
            } else {
                entry.last_closed_candle_ts
            };
            entry.last_closed_candle_ts = closed_on;
            entry.tp_order_id = None;
            entry.sl_order_id = None;
            entry.entry_order_id = None;
            self.log(instance_id, symbol, "position_closed", &entry.fingerprint)
                .await;
            self.reset(entry);
            return;
        }

        // ── v9 frozen exit params (stamped at entry) ──
        if let Some(f) = entry.frozen.clone() {
            // time stop: N bars elapsed since entry without reaching TP.
            if let Some(bars) = f.time_stop_bars {
                let tf_secs = entry
                    .tracked_setup
                    .as_ref()
                    .map(|p| p.source_tf_secs)
                    .unwrap_or(0);
                if tf_secs > 0 && ctx.candle_ts >= f.entry_candle_ts {
                    let age_bars = (ctx.candle_ts - f.entry_candle_ts) / tf_secs;
                    if age_bars >= bars as u64 {
                        match self.engine.close_position(symbol, mid, "time_stop").await {
                            Ok(_) => {
                                self.log(
                                    instance_id,
                                    symbol,
                                    "time_stop",
                                    &format!("position closed at market after {age_bars} bars"),
                                )
                                .await;
                                return;
                            }
                            Err(e) => self.log(instance_id, symbol, "close_error", &e).await,
                        }
                    }
                }
            }

            // breakeven: move the SL to the entry price once unrealized R
            // reaches the frozen threshold.
            if let (Some(plan), Some(rr_at)) = (entry.tracked_setup.clone(), f.breakeven_at_rr) {
                let rr_now = if plan.direction == "LONG" {
                    ((mid - plan.entry_mid) / (plan.entry_mid - plan.sl))
                        .to_f64()
                        .unwrap_or(0.0)
                } else {
                    ((plan.entry_mid - mid) / (plan.sl - plan.entry_mid))
                        .to_f64()
                        .unwrap_or(0.0)
                };
                if rr_now >= rr_at {
                    if let Some(sl_id) = entry.sl_order_id.clone() {
                        let _ = self.engine.cancel_order(&sl_id, symbol).await;
                        entry.sl_order_id = None;
                    }
                    if entry.sl_order_id.is_none() {
                        let Some(pos) = self.engine.get_position(symbol).await else {
                            return;
                        };
                        let exit_side = match pos.direction {
                            Direction::Long => OrderSide::Sell,
                            Direction::Short => OrderSide::Buy,
                        };
                        let mut meta = std::collections::HashMap::new();
                        meta.insert("exit_reason".to_string(), "breakeven".to_string());
                        meta.insert("trigger_source".to_string(), plan.setup_type.clone());
                        let be_packet = OrderPacket {
                            client_order_id: format!("breakeven_sl_{}", symbol),
                            symbol: symbol.to_string(),
                            side: exit_side,
                            order_type: OrderType::Stop,
                            price: Some(plan.entry_mid),
                            size: pos.size,
                            reduce_only: true,
                            is_emergency_liquidation: false,
                            associated_position_id: None,
                            metadata: meta,
                        };
                        match self.engine.submit_order(be_packet, mid).await {
                            Ok(id) => {
                                entry.sl_order_id = Some(id);
                                self.log(
                                    instance_id,
                                    symbol,
                                    "breakeven",
                                    &format!("stop moved to entry {}", plan.entry_mid),
                                )
                                .await;
                            }
                            Err(e) => self.log(instance_id, symbol, "close_error", &e).await,
                        }
                    }
                }
            }

            // trailing stop: activate at the frozen R threshold, trail the
            // SL by `atr_mult × ATR` (ATR read from the source snapshot).
            if let (Some(plan), Some(activate_rr)) =
                (entry.tracked_setup.clone(), f.trailing_activate_rr)
            {
                if let Some(atr_mult) = f.trailing_atr_mult {
                    let rr_now = if plan.direction == "LONG" {
                        ((mid - plan.entry_mid) / (plan.entry_mid - plan.sl))
                            .to_f64()
                            .unwrap_or(0.0)
                    } else {
                        ((plan.entry_mid - mid) / (plan.sl - plan.entry_mid))
                            .to_f64()
                            .unwrap_or(0.0)
                    };
                    if rr_now >= activate_rr {
                        let atr = snapshots
                            .iter()
                            .find(|s| {
                                s.is_completed == Some(true)
                                    && s.timeframe_secs == plan.source_tf_secs
                            })
                            .and_then(|s| s.indicators.get("atr"))
                            .map(|v| v.raw_value)
                            .filter(|a| *a > 0.0)
                            .unwrap_or(0.0);
                        if atr > 0.0 {
                            let trail_price = if plan.direction == "LONG" {
                                (mid - Decimal::from_f64_retain(atr * atr_mult).unwrap_or(dec!(0)))
                                    .max(plan.entry_mid)
                            } else {
                                (mid + Decimal::from_f64_retain(atr * atr_mult).unwrap_or(dec!(0)))
                                    .min(plan.entry_mid)
                            };
                            if let Some(sl_id) = entry.sl_order_id.clone() {
                                let _ = self.engine.cancel_order(&sl_id, symbol).await;
                                entry.sl_order_id = None;
                            }
                            if entry.sl_order_id.is_none() {
                                let Some(pos) = self.engine.get_position(symbol).await else {
                                    return;
                                };
                                let exit_side = match pos.direction {
                                    Direction::Long => OrderSide::Sell,
                                    Direction::Short => OrderSide::Buy,
                                };
                                let mut meta = std::collections::HashMap::new();
                                meta.insert("exit_reason".to_string(), "trailing_stop".to_string());
                                meta.insert("trigger_source".to_string(), plan.setup_type.clone());
                                let tr_packet = OrderPacket {
                                    client_order_id: format!("trailing_sl_{}", symbol),
                                    symbol: symbol.to_string(),
                                    side: exit_side,
                                    order_type: OrderType::Stop,
                                    price: Some(trail_price),
                                    size: pos.size,
                                    reduce_only: true,
                                    is_emergency_liquidation: false,
                                    associated_position_id: None,
                                    metadata: meta,
                                };
                                match self.engine.submit_order(tr_packet, mid).await {
                                    Ok(id) => {
                                        entry.sl_order_id = Some(id);
                                        self.log(
                                            instance_id,
                                            symbol,
                                            "trailing_stop",
                                            &format!("trailing stop at {}", trail_price),
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        self.log(instance_id, symbol, "close_error", &e).await
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── v10 confidence-drop exit (same-direction setup) ──
        if let Some(drop_pct) = policy.confidence_drop_pct {
            if let (Some(f), Some(plan), Some(tracked)) = (
                entry.frozen.as_ref(),
                top.as_ref(),
                entry.tracked_setup.as_ref(),
            ) {
                if plan.direction == tracked.direction {
                    let dropped = (f.entry_confidence - plan.confidence) * 100.0;
                    if dropped >= drop_pct {
                        match self
                            .engine
                            .close_position(symbol, mid, "confidence_drop")
                            .await
                        {
                            Ok(_) => {
                                self.log(
                                    instance_id,
                                    symbol,
                                    "confidence_drop",
                                    &format!(
                                        "position closed at market — confidence fell {dropped:.1} pts (≥ {drop_pct})"
                                    ),
                                )
                                .await;
                                return;
                            }
                            Err(e) => self.log(instance_id, symbol, "close_error", &e).await,
                        }
                    }
                }
            }
        }

        // ── v10 setup-gone posture while open ──
        if top.is_none() && policy.setup_gone == "strict" {
            match self.engine.close_position(symbol, mid, "setup_gone").await {
                Ok(_) => {
                    self.log(
                        instance_id,
                        symbol,
                        "setup_gone_close",
                        "position closed at market — actionable setup gone (setup_gone_policy=strict)",
                    )
                    .await;
                    return;
                }
                Err(e) => self.log(instance_id, symbol, "close_error", &e).await,
            }
        }

        // ── v10 asymmetric bracket refresh (ratchet) ──
        if let Some(plan) = top {
            if let Some(tracked) = &entry.tracked_setup {
                if plan.direction == tracked.direction
                    && (plan.fingerprint != tracked.fingerprint
                        || plan.setup_type != tracked.setup_type)
                {
                    self.ratchet_bracket(instance_id, symbol, plan, snapshots, mid, policy, entry)
                        .await;
                }
            }
        }

        // ── SIGNAL invalidation while open: close at market ──
        if let Some(plan) = top {
            if let Some(tracked) = &entry.tracked_setup {
                if plan.direction != tracked.direction {
                    match self
                        .engine
                        .close_position(symbol, mid, "invalidated_signal")
                        .await
                    {
                        Ok(_) => {
                            self.log(
                                instance_id,
                                symbol,
                                "invalidated_signal",
                                &format!(
                                    "position closed at market — recommendation flipped to {}",
                                    plan.direction
                                ),
                            )
                            .await;
                        }
                        Err(e) => {
                            self.log(instance_id, symbol, "close_error", &e).await;
                        }
                    }
                }
            }
        }
    }

    /// v10 R9/R10: asymmetric bracket refresh. The SL only tightens in our
    /// favor (never widens — invariant I2); the TP refreshes only when the
    /// fresh setup improves net RR by ≥ `tp_refresh_min_rr_delta` (I3).
    /// Both moves are gated by the min-reprice delta so the bracket does
    /// not churn on every completed bar.
    async fn ratchet_bracket(
        &self,
        instance_id: &str,
        symbol: &str,
        plan: &SetupPlan,
        snapshots: &[&MarketSnapshot],
        mid: Decimal,
        policy: &StrategyPolicy,
        entry: &mut SymbolState,
    ) {
        let Some(tracked) = entry.tracked_setup.clone() else {
            return;
        };
        let Some(pos) = self.engine.get_position(symbol).await else {
            return;
        };
        let atr = source_atr(snapshots, plan.source_tf_secs);
        let (mut eff, _entry) = plan.effective(policy, atr, mid);
        // v11: the ratchet must respect the stop floor + TP cap — a fresh
        // setup can never tighten the SL into noise nor place an
        // unreachable target (invariant: floor survives bracket refresh).
        // Anchor = the actual fill price so the floor tracks the real entry.
        eff.apply_execution_dials(policy, atr, snapshots, pos.entry_price);
        let min_delta =
            Decimal::from_f64_retain(policy.min_reprice_delta_atr * atr).unwrap_or(dec!(0));
        let is_long = eff.direction == "LONG";

        let sl_now = match &entry.sl_order_id {
            Some(id) => {
                let orders = self.engine.orders.read().await;
                orders
                    .get(id)
                    .and_then(|o| o.packet.price)
                    .unwrap_or(tracked.sl)
            }
            None => tracked.sl,
        };
        let tp_now = match &entry.tp_order_id {
            Some(id) => {
                let orders = self.engine.orders.read().await;
                orders
                    .get(id)
                    .and_then(|o| o.packet.price)
                    .unwrap_or(tracked.tp)
            }
            None => tracked.tp,
        };

        let exit_side = match pos.direction {
            Direction::Long => OrderSide::Sell,
            Direction::Short => OrderSide::Buy,
        };
        let mut sl_changed = false;
        let mut tp_changed = false;

        // SL ratchet: tighten only.
        let sl_improves = if is_long {
            eff.sl > sl_now
        } else {
            eff.sl < sl_now
        };
        if sl_improves && (eff.sl - sl_now).abs() >= min_delta {
            if let Some(sl_id) = entry.sl_order_id.clone() {
                let _ = self.engine.cancel_order(&sl_id, symbol).await;
                entry.sl_order_id = None;
            }
            let mut meta = std::collections::HashMap::new();
            meta.insert("exit_reason".to_string(), "sl".to_string());
            meta.insert("trigger_source".to_string(), eff.setup_type.clone());
            let sl_packet = OrderPacket {
                client_order_id: format!("sl_{}_{}", symbol, eff.fingerprint),
                symbol: symbol.to_string(),
                side: exit_side,
                order_type: OrderType::Stop,
                price: Some(eff.sl),
                size: pos.size,
                reduce_only: true,
                is_emergency_liquidation: false,
                associated_position_id: None,
                metadata: meta,
            };
            if let Ok(id) = self.engine.submit_order(sl_packet, mid).await {
                entry.sl_order_id = Some(id);
                sl_changed = true;
            }
        }

        // TP refresh: only when the fresh setup improves net RR by ≥ delta.
        let sl_after = if sl_changed { eff.sl } else { sl_now };
        let rr_now = rr_units(&eff.direction, pos.entry_price, tp_now, sl_now);
        let rr_new = rr_units(&eff.direction, pos.entry_price, eff.tp, sl_after);
        if rr_new >= rr_now + policy.tp_refresh_min_rr_delta {
            if let Some(tp_id) = entry.tp_order_id.clone() {
                let _ = self.engine.cancel_order(&tp_id, symbol).await;
                entry.tp_order_id = None;
            }
            let mut meta = std::collections::HashMap::new();
            meta.insert("exit_reason".to_string(), "tp".to_string());
            meta.insert("trigger_source".to_string(), eff.setup_type.clone());
            let tp_packet = OrderPacket {
                client_order_id: format!("tp_{}_{}", symbol, eff.fingerprint),
                symbol: symbol.to_string(),
                side: exit_side,
                order_type: OrderType::Limit,
                price: Some(eff.tp),
                size: pos.size,
                reduce_only: true,
                is_emergency_liquidation: false,
                associated_position_id: None,
                metadata: meta,
            };
            if let Ok(id) = self.engine.submit_order(tp_packet, mid).await {
                entry.tp_order_id = Some(id);
                tp_changed = true;
            }
        }

        if sl_changed || tp_changed {
            self.log(
                instance_id,
                symbol,
                "bracket_refresh",
                &format!(
                    "ratchet — SL {} → {}, TP {} → {}",
                    sl_now,
                    if sl_changed { eff.sl } else { sl_now },
                    tp_now,
                    if tp_changed { eff.tp } else { tp_now }
                ),
            )
            .await;
            entry.tracked_setup = Some(eff);
            entry.fingerprint = plan.fingerprint.clone();
        }
    }

    /// Arm the TP limit + SL stop bracket after an entry fill.
    async fn arm_bracket(
        &self,
        instance_id: &str,
        symbol: &str,
        entry: &mut SymbolState,
        snapshots: &[&MarketSnapshot],
        policy: &StrategyPolicy,
    ) {
        let Some(mut plan) = entry.tracked_setup.clone() else {
            return;
        };
        if entry.tp_order_id.is_some() && entry.sl_order_id.is_some() {
            return;
        }
        let pos = match self.engine.get_position(symbol).await {
            Some(p) => p,
            None => return,
        };
        // v11: re-apply the stop floor + TP cap anchored on the ACTUAL fill
        // price. The acceptance-time floor used the zone midpoint; a fill
        // inside the zone must never leave an SL above entry (LONG) or below
        // (SHORT) — the bracket always respects the floor relative to reality.
        let atr = source_atr(snapshots, plan.source_tf_secs);
        if atr > 0.0 {
            plan.apply_execution_dials(policy, atr, snapshots, pos.entry_price);
        }
        let exit_side = match pos.direction {
            Direction::Long => OrderSide::Sell,
            Direction::Short => OrderSide::Buy,
        };

        let mut tp_meta = std::collections::HashMap::new();
        tp_meta.insert("exit_reason".to_string(), "tp".to_string());
        tp_meta.insert("trigger_source".to_string(), plan.setup_type.clone());
        let tp_packet = OrderPacket {
            client_order_id: format!("tp_{}_{}", symbol, entry.fingerprint),
            symbol: symbol.to_string(),
            side: exit_side,
            order_type: OrderType::Limit,
            price: Some(plan.tp),
            size: pos.size,
            reduce_only: true,
            is_emergency_liquidation: false,
            associated_position_id: None,
            metadata: tp_meta,
        };

        let mut sl_meta = std::collections::HashMap::new();
        sl_meta.insert("exit_reason".to_string(), "sl".to_string());
        sl_meta.insert("trigger_source".to_string(), plan.setup_type.clone());
        let sl_packet = OrderPacket {
            client_order_id: format!("sl_{}_{}", symbol, entry.fingerprint),
            symbol: symbol.to_string(),
            side: exit_side,
            order_type: OrderType::Stop,
            price: Some(plan.sl),
            size: pos.size,
            reduce_only: true,
            is_emergency_liquidation: false,
            associated_position_id: None,
            metadata: sl_meta,
        };

        let mid = pos.entry_price;
        if entry.tp_order_id.is_none() {
            if let Ok(id) = self.engine.submit_order(tp_packet, mid).await {
                entry.tp_order_id = Some(id);
            }
        }
        if entry.sl_order_id.is_none() {
            if let Ok(id) = self.engine.submit_order(sl_packet, mid).await {
                entry.sl_order_id = Some(id);
            }
        }
        self.log(
            instance_id,
            symbol,
            "bracket_armed",
            &format!("TP {} / SL {}", plan.tp, plan.sl),
        )
        .await;
    }

    /// v8.2 canonical sizing + projection: portfolio-share allocation.
    ///
    /// `notional = equity × allocation_pct / 100`, `size = notional /
    /// entry_mid`. The stop-loss no longer sizes the position — it defines
    /// the risk budget / invalidation level only. Leverage still applies to
    /// margin; the notional is clamped to the optional
    /// `max_position_size_pct_of_equity` (v9 F-08 — a relative cap so the
    /// same strategy behaves identically at any capital size).
    /// (`risk_calculator::compute_risk` remains the engine behind
    /// `POST /api/risk/calculate` — a manual preview only.)
    async fn project(
        &self,
        plan: &SetupPlan,
        allocation_pct: f64,
        vol_factor: f64,
    ) -> Option<SetupProjection> {
        let equity = self.engine.get_equity_decimal().await;
        if equity <= dec!(0) {
            return None;
        }
        let leverage = *self.engine.cross_leverage.read().await;
        let taker_pct = self.engine.fee_config.taker_fee_pct;

        let allocation = Decimal::from_f64_retain(allocation_pct / 100.0)?;
        // v9 vol-scale factor (already clamped 0.25..=4.0 by the caller).
        let vol_dec = Decimal::from_f64_retain(vol_factor)?.max(dec!(0));
        let notional_uncapped = equity * allocation * vol_dec;
        let mut size = notional_uncapped / plan.entry_mid;
        // v9 F-08: the notional cap is a PERCENTAGE of equity — the
        // strategy stays capital-size invariant.
        if let Some(cap_pct) = self.max_position_size_pct_of_equity {
            let cap_f = Decimal::from_f64_retain(cap_pct / 100.0)?;
            let cap_notional = equity * cap_f;
            if size * plan.entry_mid > cap_notional {
                size = cap_notional / plan.entry_mid;
            }
        }
        if size <= dec!(0) {
            return None;
        }
        let notional = size * plan.entry_mid;

        let is_long = plan.direction == "LONG";
        let margin = if leverage > 0 {
            notional / Decimal::from(leverage)
        } else {
            notional
        };
        let liquidation_distance = plan.entry_mid / Decimal::from(leverage.max(1));
        let liquidation_price = if is_long {
            plan.entry_mid - liquidation_distance
        } else {
            plan.entry_mid + liquidation_distance
        };

        let taker_dec = Decimal::from_f64_retain(taker_pct / 100.0)?;
        let entry_fee = notional * taker_dec;
        let exit_fee = entry_fee;
        let total_fees = entry_fee
            + exit_fee
            + Decimal::from_f64_retain(self.engine.fee_config.funding_rate_8h / 100.0)? * notional
            + Decimal::from_f64_retain(self.engine.fee_config.simulated_spread_pct)?;

        let estimated_profit = if is_long {
            (plan.tp - plan.entry_mid) * size
        } else {
            (plan.entry_mid - plan.tp) * size
        };
        let net_profit = estimated_profit - total_fees;
        let roi = if margin > dec!(0) {
            net_profit / margin * dec!(100)
        } else {
            dec!(0)
        };
        // Net R:R = fee-adjusted TP gain relative to the allocated capital.
        let net_rr = if notional > dec!(0) {
            Some(net_profit / notional)
        } else {
            None
        };

        Some(SetupProjection {
            // The allocated capital IS the capital at risk now.
            risk_capital: notional,
            position_size_units: size,
            position_notional: notional,
            margin_required: margin,
            liquidation_price,
            entry_fee_usd: entry_fee,
            exit_fee_usd: exit_fee,
            total_fees,
            net_profit_usd: net_profit,
            roi_pct: roi,
            net_rr,
        })
    }

    fn reset(&self, entry: &mut SymbolState) {
        entry.phase = ExecutorPhase::Idle;
        entry.fingerprint = String::new();
        entry.tracked_setup = None;
        entry.projection = None;
        entry.entry_order_id = None;
        entry.tp_order_id = None;
        entry.sl_order_id = None;
        entry.entry_is_market = false;
        entry.frozen = None;
    }

    async fn log(&self, instance_id: &str, symbol: &str, event: &str, detail: &str) {
        self.engine
            .log_activity(instance_id, symbol, event, detail)
            .await;
    }

    /// Restart recovery: persist the tracked setup fingerprint so the daemon
    /// can log a recovery-flatten on boot. Pending entries are cancelled and
    /// positions are flattened at the last known mark (recovery flatten).
    pub async fn persist_open_state(&self, instance_id: &str, symbol: &str) {
        let state = self.state.read().await;
        if let Some(entry) = state.get(symbol) {
            if entry.phase != ExecutorPhase::Idle {
                let payload = serde_json::to_string(&entry.tracked_setup).unwrap_or_default();
                self.engine
                    .persist_open_state(instance_id, symbol, &payload)
                    .await;
                return;
            }
        }
        self.engine.clear_open_state(instance_id).await;
    }

    /// On boot: if a tracked setup was persisted, log a recovery-flatten
    /// event and start fresh (equity is restored separately by the daemon).
    pub async fn recover(&self, instance_id: &str, symbol: &str) {
        if let Some((payload, _saved_at)) = self.engine.load_open_state(instance_id).await {
            if !payload.is_empty() {
                self.log(
                    instance_id,
                    symbol,
                    "recovery_flatten",
                    "daemon restarted with open state — pending entries cancelled, \
                     positions flattened at last known mark; equity restored",
                )
                .await;
            }
            self.engine.clear_open_state(instance_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    //! CR-15 removed this module's subject.
    //!
    //! The two suites that used to live here — the setup-extraction suite and
    //! the safety-ladder suite — exercised `extract_top_setup` against
    //! `snapshot.opportunity` / `snapshot.decision_context`, the L4/L6 strategy
    //! matrices that no longer exist. None of their fixtures could survive the
    //! boundary, and none of their assertions still describe shipped behaviour.
    //!
    //! What replaces them is the pin on the boundary itself: the executor must
    //! produce no setups, and it must do so for a reason rather than by accident.
    //! A future TAE implementation that re-derives setups inside TAE from model
    //! state will fail these tests, which is the correct direction for it to fail.

    use super::*;

    #[test]
    fn no_setup_is_ever_extracted() {
        assert!(
            extract_top_setup(&[], 0.5).is_none(),
            "an empty input must produce no setup"
        );
    }

    #[test]
    fn the_stub_is_not_an_unreachable_arm() {
        // `min_net_rr` is deliberately ignored. If a later change ever starts
        // filtering on it, this fails: the parameter is accepted for signature
        // stability only, and using it would imply a derivation that no longer
        // exists.
        let never_matching: f64 = f64::NAN;
        assert!(extract_top_setup(&[], never_matching).is_none());
    }
}
