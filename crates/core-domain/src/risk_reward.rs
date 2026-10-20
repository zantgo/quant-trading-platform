//! # Risk/Reward — three-state side R:R computation
//!
//! The L4 Opportunity Matrix publishes per-side expected reward/risk ratios
//! (`long_expected_rr_internal`, `short_expected_rr_internal`) on every
//! qualifying profile. This module owns the *computation* — the geometric
//! formula that maps a price bracket (entry zone, target zone, invalidation
//! level) and an active side (LONG/SHORT) to a three-state result.
//!
//! ## Three-state model
//!
//! The result is a [`SideRrStatus`] discriminated union that distinguishes
//! three outcomes which the previous `Option<f64>` conflated:
//!
//! 1. **`Value(f64)`** — the bracket is geometrically valid for the active
//!    side and the computation succeeded. `f64` is the unsigned reward/risk
//!    ratio (`reward / risk`) in the canonical `(0, ∞)` range.
//! 2. **`NoValue(NoValueReason)`** — the computation succeeded but no
//!    valid R:R exists for this side. The reason is an enum tag so the
//!    dashboard can surface a precise message ("SL inside entry zone",
//!    "no confluent levels", etc.) instead of a generic "0.00".
//!    Degenerate ratios below [`RR_MEANINGFUL_FLOOR`] (entry and target
//!    at effectively the same price) are rejected as
//!    `NoValueReason::RatioBelowFloor` — the geometry is "valid" but the
//!    ratio is economically noise.
//! 3. **`Error(String)`** — the computation itself failed (NaN, division
//!    by zero, malformed input type). Reserved for unexpected bugs; a
//!    non-empty `Error` should page someone.
//!
//! This replaces the legacy `compute_side_rr` closure in
//! `crates/market-analyzer/src/synthesis.rs` (which conflated `NoValue` and
//! `Error` by returning `None` for both). See `docs/matrices/02-08-opportunity-matrix.md`
//! for the L4 ownership of the per-side R:R fields.

use serde::{Deserialize, Serialize};

/// Outcome of a per-side R:R computation.
///
/// Three states (Value / NoValue / Error) instead of `Option<f64>` so the
/// caller can distinguish "no valid bracket" from "computation crashed".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SideRrStatus {
    /// Bracket is geometrically valid for the active side. `f64` is the
    /// unsigned reward/risk ratio (`reward / risk`) on the canonical
    /// `(0, ∞)` range. The UI normalizes to a `1 : N` display.
    Value(f64),
    /// Computation succeeded but no valid R:R exists for this side.
    /// `NoValueReason` carries the precise reason so the dashboard can
    /// render a specific message.
    NoValue(NoValueReason),
    /// Computation itself failed (NaN, division by zero, malformed input).
    /// Reserved for unexpected bugs.
    Error(String),
}

/// Reason a per-side R:R computation returns `NoValue` (rather than `Value`).
///
/// Each variant maps to a specific geometric or data-availability failure
/// mode that the dashboard surfaces verbatim to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoValueReason {
    /// Target and invalidation are on the same side of entry (e.g. for a
    /// LONG setup, target below entry AND invalidation above entry).
    GeometryInverted,
    /// SL is effectively at the entry mid (zero risk; R:R is undefined).
    SlAtEntry,
    /// SL is inside the entry zone (entry range and invalidation overlap).
    SlInsideEntry,
    /// Target is on the wrong side of `close` for the active side
    /// (LONG → target ≤ close, or SHORT → target ≥ close).
    TargetOnWrongSide,
    /// The geometric ratio is positive but below the meaningfulness floor
    /// (`RR_MEANINGFUL_FLOOR`). Entry and target sit effectively at the
    /// same price (e.g. reward ≈ 1% of risk), so the bracket is
    /// economically noise even though its geometry is "valid".
    RatioBelowFloor,
    /// No confluent levels and no synthetic ATR fallback was produced.
    NoValidBracket,
    /// Profile belongs to the inactive side for the macro bias
    /// (TrendRiding + Neutral bias → no resolvable direction).
    InactiveSide,
}

/// Active trade side for the R:R computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Long,
    Short,
}

/// Minimum economically meaningful reward/risk ratio.
///
/// Ratios below this floor (e.g. `0.0117` — a bracket whose reward is
/// ~1% of its risk) are geometrically "valid" but economically noise:
/// the entry and target sit effectively at the same price. They are
/// rejected as `NoValue(RatioBelowFloor)` so degenerate near-zero R:R
/// values never reach the wire (the frontend flips every exact-zero
/// check to `N/A` as a result).
pub const RR_MEANINGFUL_FLOOR: f64 = 0.1;

/// Compute the per-side R:R for a price bracket.
///
/// Inputs:
/// - `entry`: `PriceRange` carrying `[low, high]`. Mid = `(low + high) / 2`.
/// - `target`: `PriceRange` carrying `[low, high]`. Mid = `(low + high) / 2`.
/// - `invalidation`: `f64` — the price whose breach nullifies the thesis.
/// - `close`: `f64` — the current snapshot price (used for `TargetOnWrongSide`).
/// - `side`: `Side::Long` or `Side::Short`.
///
/// Returns `SideRrStatus`:
/// - `Value(ratio)` when `reward > 0` and `risk > 0` and the geometry is
///   valid for the active side. `ratio = |reward| / |risk|`.
/// - `NoValue(reason)` for degenerate brackets (see `NoValueReason`).
/// - `Error(msg)` for unexpected input (NaN, Inf, division by zero).
///
/// v6.10.19 (P5): the net R:R cost model — a gross geometric R:R of
/// exactly 1:1 is rarely tradeable once entry/exit fees, slippage and
/// hold-time funding are deducted (round-trip baseline: 2× taker fee +
/// 2× slippage = 22 bps at the 6/5 defaults). The defaults are
/// config-tunable via `OpportunityMatrixConfig` (plumbed in a
/// follow-up; synthesis uses `NetCostModel::default()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetCostModel {
    /// Taker fee in basis points per side (default 6 = 0.06%).
    pub taker_fee_bps: f64,
    /// Assumed slippage in basis points per side (default 5 = 0.05%).
    pub slippage_bps: f64,
    /// Hold-time funding cost in basis points on the entry notional
    /// (default 0).
    pub funding_bps: f64,
}

impl Default for NetCostModel {
    fn default() -> Self {
        Self {
            taker_fee_bps: 6.0,
            slippage_bps: 5.0,
            funding_bps: 0.0,
        }
    }
}

impl NetCostModel {
    /// Net R:R from the geometric magnitudes. Costs: entry price ×
    /// (taker + slippage) bps at entry; target price × (taker + slippage)
    /// bps plus the entry notional × funding bps at exit. Returns 0 for
    /// degenerate brackets (the caller's `NoValue` path).
    pub fn net_rr(&self, entry_mid: f64, target_mid: f64, invalidation: f64, side: Side) -> f64 {
        let reward = if side == Side::Long {
            target_mid - entry_mid
        } else {
            entry_mid - target_mid
        };
        let risk = if side == Side::Long {
            entry_mid - invalidation
        } else {
            invalidation - entry_mid
        };
        if reward <= 0.0 || risk <= 0.0 || entry_mid <= 0.0 || target_mid <= 0.0 {
            return 0.0;
        }
        let cost_per_side = (self.taker_fee_bps + self.slippage_bps) / 10000.0;
        let entry_cost = entry_mid * cost_per_side;
        let exit_cost = target_mid * cost_per_side + entry_mid * (self.funding_bps / 10000.0);
        let reward_net = reward - entry_cost - exit_cost;
        let risk_net = risk + entry_cost;
        if reward_net <= 0.0 || risk_net <= 0.0 {
            0.0
        } else {
            let ratio = reward_net / risk_net;
            if ratio.is_finite() {
                ratio
            } else {
                0.0
            }
        }
    }
}

pub fn compute_side_rr_v2(
    entry_low: f64,
    entry_high: f64,
    target_low: f64,
    target_high: f64,
    invalidation: f64,
    close: f64,
    side: Side,
) -> SideRrStatus {
    // ── Input validation (Error) ────────────────────────────────────────
    if !entry_low.is_finite()
        || !entry_high.is_finite()
        || !target_low.is_finite()
        || !target_high.is_finite()
        || !invalidation.is_finite()
        || !close.is_finite()
    {
        return SideRrStatus::Error(
            "non-finite input (NaN or Inf in entry/target/invalidation/close)".to_string(),
        );
    }
    if entry_low <= 0.0 || entry_high <= 0.0 {
        return SideRrStatus::Error(format!(
            "non-positive entry zone (low={}, high={})",
            entry_low, entry_high
        ));
    }
    if entry_low > entry_high {
        return SideRrStatus::Error(format!(
            "inverted entry zone (low={} > high={})",
            entry_low, entry_high
        ));
    }
    if target_low > target_high {
        return SideRrStatus::Error(format!(
            "inverted target zone (low={} > high={})",
            target_low, target_high
        ));
    }

    // ── Geometry guards (NoValue) ───────────────────────────────────────
    let entry_mid = (entry_low + entry_high) / 2.0;
    let target_mid = (target_low + target_high) / 2.0;

    // SL inside entry zone (overlapping ranges).
    if invalidation >= entry_low && invalidation <= entry_high {
        return SideRrStatus::NoValue(NoValueReason::SlInsideEntry);
    }
    // SL effectively at entry mid (zero risk). v6.10.19b (B1): the epsilon
    // was 0.0001 × price (0.01% ≈ 6.3 points on a $63k instrument), which
    // rejected REAL horizon-budgeted stops (the I-5b derivation anchors
    // scalp stops at ~1.5×ATR — ~4 points in low-ATR compression states)
    // as "zero risk", demoting valid brackets to GeometryInverted. Only a
    // truly-degenerate stop (sub-0.0001% of price) is zero risk now; the
    // `RR_MEANINGFUL_FLOOR` ratio guard below still rejects noise.
    let risk_dir = entry_mid - invalidation;
    if risk_dir.abs() < 1e-6 * entry_mid.abs().max(1.0) {
        return SideRrStatus::NoValue(NoValueReason::SlAtEntry);
    }
    // Target on wrong side of close for the active side.
    match side {
        Side::Long if target_mid <= close => {
            return SideRrStatus::NoValue(NoValueReason::TargetOnWrongSide);
        }
        Side::Short if target_mid >= close => {
            return SideRrStatus::NoValue(NoValueReason::TargetOnWrongSide);
        }
        _ => {}
    }

    // ── Compute reward + risk (sign-aware) ──────────────────────────────
    let reward = match side {
        Side::Long => target_mid - entry_mid,
        Side::Short => entry_mid - target_mid,
    };
    let risk = match side {
        Side::Long => entry_mid - invalidation,
        Side::Short => invalidation - entry_mid,
    };

    if reward <= 0.0 || risk <= 0.0 {
        return SideRrStatus::NoValue(NoValueReason::GeometryInverted);
    }

    let ratio = reward / risk;
    if !ratio.is_finite() {
        return SideRrStatus::Error("non-finite R:R (division by zero or overflow)".to_string());
    }
    // Meaningfulness floor (B3): a positive-but-degenerate ratio
    // (reward ≈ 0.01 × risk) is noise, not conviction. Reject it so
    // every consumer (bars, header chip, R:R blocks, decision context)
    // sees a clean 0 instead of a misleading "0.01".
    if ratio < RR_MEANINGFUL_FLOOR {
        return SideRrStatus::NoValue(NoValueReason::RatioBelowFloor);
    }
    SideRrStatus::Value(ratio)
}

/// Resolve the active-side R:R from the L4 Opportunity Matrix's per-side
/// fields, gated on the macro bias. Returns `0.0` for Neutral bias
/// (the active side is undefined) or when the matrix is absent.
///
/// `prefer_per_profile` controls whether to read the profile-level
/// `long_/short_expected_rr_internal` (preferred when a top profile is
/// available) or fall back to the matrix-level per-side fields.
pub fn active_side_rr(long_rr: f64, short_rr: f64, bias: MacroBias) -> f64 {
    match bias {
        MacroBias::StrongBullish | MacroBias::Bullish => long_rr,
        MacroBias::StrongBearish | MacroBias::Bearish => short_rr,
        MacroBias::Neutral => 0.0,
    }
}

/// Macro bias enum (mirror of `core_domain::analysis::MarketBias`).
///
/// Lives here so this module is self-contained and doesn't pull in the
/// whole `analysis` module (which would create a cycle if the analysis
/// module ever depends on `risk_reward`). The mapping is identical; the
/// `analysis::MarketBias` ↔ `risk_reward::MacroBias` conversion lives in
/// `synthesis.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroBias {
    StrongBullish,
    Bullish,
    Neutral,
    StrongBearish,
    Bearish,
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: Side = Side::Long;
    const SHORT: Side = Side::Short;

    #[test]
    fn value_for_valid_long_bracket() {
        // Entry: 63937-64278 (mid 64107.5), Target: 64359-65089 (mid 64724),
        // SL: 63900, close 64279. LONG side.
        // reward = 64724 - 64107.5 = 616.5; risk = 64107.5 - 63900 = 207.5;
        // ratio ≈ 2.97.
        let r = compute_side_rr_v2(63937.0, 64278.0, 64359.0, 65089.0, 63900.0, 64279.0, LONG);
        assert!(matches!(r, SideRrStatus::Value(v) if (v - 2.97).abs() < 0.01));
    }

    #[test]
    fn value_for_valid_short_bracket() {
        // Entry: 65500-66000 (mid 65750), Target: 63000-64000 (mid 63500),
        // SL: 66500, close 64279. SHORT side.
        // reward = 65750 - 63500 = 2250; risk = 66500 - 65750 = 750;
        // ratio = 3.0.
        let r = compute_side_rr_v2(65500.0, 66000.0, 63000.0, 64000.0, 66500.0, 64279.0, SHORT);
        assert!(matches!(r, SideRrStatus::Value(v) if (v - 3.0).abs() < 0.01));
    }

    #[test]
    fn tight_real_stop_not_rejected_as_sl_at_entry() {
        // v6.10.19b (B1) regression: the live 20:28 MeanReversion bracket —
        // a ~4.12-point stop on a $63k instrument (1.5×ATR in a low-ATR
        // compression state). The OLD 0.01%-of-price epsilon (≈6.3 points)
        // rejected it as SlAtEntry and demoted a geometrically valid
        // 1.13 R:R bracket to GeometryInverted.
        let r = compute_side_rr_v2(
            63045.96928083956,
            63047.0,
            63050.092157481304,
            63052.15359580217,
            63042.36176377805,
            63047.0,
            LONG,
        );
        match r {
            SideRrStatus::Value(v) => {
                assert!((v - 1.126).abs() < 0.01, "expected ≈1.13, got {v}")
            }
            other => panic!("valid tight bracket must be a Value, got {other:?}"),
        }
    }

    #[test]
    fn truly_zero_risk_stop_still_rejected() {
        // v6.10.19b (B1): with the tightened 1e-6 epsilon, a degenerate
        // stop within ~0.0001% of the entry mid is always INSIDE the entry
        // zone — `SlInsideEntry` (which precedes SlAtEntry) is now the
        // effective guard for zero-risk brackets; SlAtEntry remains the
        // defensive fallback for exact-boundary edge cases.
        let r = compute_side_rr_v2(
            63000.0, 63100.0, 63200.0, 63300.0, 63050.0004, 63050.0, LONG,
        );
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::SlInsideEntry));
        let r2 = compute_side_rr_v2(
            63000.0, 63100.0, 63200.0, 63300.0, 63050.0004, 63050.0, SHORT,
        );
        assert!(matches!(r2, SideRrStatus::NoValue(_)));
    }

    #[test]
    fn no_value_when_sl_inside_entry() {
        // Reproduces the screenshot bug: SL $63937 == entry.low (sits on the
        // boundary of the entry zone, so SL-inside-entry is the right reason).
        let r = compute_side_rr_v2(63937.0, 64288.0, 64359.0, 65089.0, 63937.0, 64279.0, LONG);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::SlInsideEntry));
    }

    #[test]
    fn no_value_when_sl_at_entry_mid_zero_width() {
        // Degenerate bracket: entry is a single price (low == high), SL at
        // that exact price. SlInsideEntry triggers first since inv is
        // technically inside the entry range; this test confirms the
        // degenerate zero-width case is still surfaced as NoValue.
        let r = compute_side_rr_v2(100.0, 100.0, 115.0, 120.0, 100.0, 110.0, LONG);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::SlInsideEntry));
    }

    #[test]
    fn no_value_when_target_on_wrong_side_long() {
        // LONG: target below close.
        let r = compute_side_rr_v2(100.0, 105.0, 90.0, 95.0, 95.0, 110.0, LONG);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::TargetOnWrongSide));
    }

    #[test]
    fn no_value_when_target_on_wrong_side_short() {
        // SHORT: target above close.
        let r = compute_side_rr_v2(115.0, 120.0, 125.0, 130.0, 125.0, 110.0, SHORT);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::TargetOnWrongSide));
    }

    #[test]
    fn no_value_when_geometry_inverted() {
        // LONG setup: target ABOVE close (so TargetOnWrongSide doesn't
        // fire) but ABOVE entry_mid (positive reward) AND SL ABOVE entry
        // (negative risk_dir). reward > 0 but risk < 0 → geometry inverted.
        let r = compute_side_rr_v2(100.0, 105.0, 107.0, 112.0, 110.0, 100.0, LONG);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::GeometryInverted));
    }

    #[test]
    fn no_value_when_ratio_below_meaningfulness_floor() {
        // LONG: entry 100 (mid 100), target 100.25 (mid 100.25),
        // invalidation 95 → reward = 0.25, risk = 5 → ratio = 0.05 (< 0.1).
        let r = compute_side_rr_v2(100.0, 100.0, 100.25, 100.25, 95.0, 100.1, LONG);
        assert_eq!(r, SideRrStatus::NoValue(NoValueReason::RatioBelowFloor));
    }

    #[test]
    fn value_at_exact_floor_boundary() {
        // LONG: entry 100 (mid 100), target 100.5 (mid 100.5),
        // invalidation 95 → reward = 0.5, risk = 5 → ratio = 0.1.
        // The floor is a strict lower bound: exactly 0.1 stays a Value.
        let r = compute_side_rr_v2(100.0, 100.0, 100.5, 100.5, 95.0, 100.2, LONG);
        assert!(matches!(r, SideRrStatus::Value(v) if (v - 0.1).abs() < 1e-9));
    }

    #[test]
    fn error_on_nan_input() {
        let r = compute_side_rr_v2(f64::NAN, 105.0, 115.0, 120.0, 95.0, 110.0, LONG);
        assert!(matches!(r, SideRrStatus::Error(_)));
    }

    #[test]
    fn net_cost_model_demotes_a_gross_one_to_one_bracket() {
        // v6.10.19 (P5): a gross 1:1 bracket nets ~0.98 at the 22 bps
        // round-trip baseline (2×6 fees + 2×5 slippage) — the Actionable
        // gate (net ≥ 1.0) must reject it. The gross stays on the wire.
        let model = NetCostModel::default();
        // entry 100.00, target 101.00, inv 99.00 → reward 1.0, risk 1.0.
        let net = model.net_rr(100.0, 101.0, 99.0, Side::Long);
        assert!(net < 1.0, "net {net} must be below 1.0 after costs");
        // Exact: reward 1.0 − entry cost 0.11 − exit cost 0.1111 =
        // 0.7789 over risk 1.0 + 0.11 = 0.7017 — friction eats 30% of a
        // tight 1% bracket, which is precisely why the net gate exists.
        assert!((net - 0.7017).abs() < 1e-3, "net {net}");
        // A zero-cost model returns the gross exactly.
        let free = NetCostModel {
            taker_fee_bps: 0.0,
            slippage_bps: 0.0,
            funding_bps: 0.0,
        };
        assert!((free.net_rr(100.0, 101.0, 99.0, Side::Long) - 1.0).abs() < 1e-9);
        // Degenerate brackets → 0 (the NoValue path).
        assert_eq!(model.net_rr(100.0, 99.0, 101.0, Side::Long), 0.0);
        // SHORT mirror is symmetric (entry 101, target 100, inv 102).
        let short_net = model.net_rr(101.0, 100.0, 102.0, Side::Short);
        assert!((short_net - 0.7017).abs() < 1e-3, "short net {short_net}");
    }

    #[test]
    fn error_on_inverted_entry_zone() {
        let r = compute_side_rr_v2(110.0, 100.0, 115.0, 120.0, 95.0, 110.0, LONG);
        assert!(matches!(r, SideRrStatus::Error(_)));
    }

    #[test]
    fn active_side_rr_picks_long_for_bullish_bias() {
        assert_eq!(active_side_rr(2.5, 1.8, MacroBias::StrongBullish), 2.5);
        assert_eq!(active_side_rr(2.5, 1.8, MacroBias::Bullish), 2.5);
    }

    #[test]
    fn active_side_rr_picks_short_for_bearish_bias() {
        assert_eq!(active_side_rr(2.5, 1.8, MacroBias::StrongBearish), 1.8);
        assert_eq!(active_side_rr(2.5, 1.8, MacroBias::Bearish), 1.8);
    }

    #[test]
    fn active_side_rr_returns_zero_for_neutral_bias() {
        assert_eq!(active_side_rr(2.5, 1.8, MacroBias::Neutral), 0.0);
    }
}
