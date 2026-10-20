//! # Opportunity Matrix — Strategic Forecast
//!
//! Per the [Decision Matrix §2.1](../matrices/02-04-decision-matrix.md) and the
//! [Opportunity Matrix spec](../matrices/02-08-opportunity-matrix.md), the L4 layer
//! publishes a strategy-agnostic forecast. The `OpportunityMatrix` struct is the
//! shape consumed by L6 (Decision Support) for the `expected_reward_risk_ratio`
//! synthesis.
//!
//! ## Field provenance
//!
//! - `primary_opportunity`, `opportunity_score`, `setup_quality` — produced by L4.
//! - `profiles[]` — per-setup-type scored profiles with precondition fractions.
//! - `forecast_confidence` — confidence in the profiling [0, 1].
//! - `long_expected_rr_internal` / `short_expected_rr_internal` — produced by L4
//!   from the per-side zones; consumers read the active side via `analysis.bias`.
//!   The legacy matrix-level `expected_rr_internal` was removed in v6.9.
//! - `invalidation_level`, `entry_zone`, `target_zone`, `time_horizon` — produced by L4 from
//!   structural context (S/R, fib, pivot, VWAP).
//! - `confluent_entry_levels`, `confluent_target_levels`, `confluent_invalidation_levels` —
//!   enriched multi-source zone derivation from structural indicators.
//!
//! This module is the **canonical source** of the L4 → L6 contract. See
//! [02-00-matrix-field-ownership.md §2.4](../matrices/02-00-matrix-field-ownership.md).
//!
//! The matrix publishes a balanced prospectus: entry / target / invalidation
//! zones for **both** directional sides (`long_*` and `short_*`). It does not
//! take a directional stance — the Decision tab picks. The legacy scalar
//! `entry_zone` / `target_zone` / `invalidation_level` are projections of the
//! per-direction fields and are retained so PME/TAE consumers do not break.

use crate::analysis::{OpportunityProfile, OpportunityType, SetupQuality};
// Re-export `PriceRange` at the `opportunity` module path so existing
// consumers that read `core_domain::opportunity::PriceRange` keep
// compiling. The canonical home is `crate::analysis::PriceRange`.
pub use crate::analysis::PriceRange;
// Re-export `DirectionFamily` and `TradeViability` at the `opportunity`
// module path so consumers can write `core_domain::opportunity::DirectionFamily`
// and `core_domain::opportunity::TradeViability` without depending on
// `core_domain::analysis` directly. Canonical home: `crate::analysis`.
pub use crate::analysis::{DirectionFamily, TradeViability};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LevelSource {
    Fibonacci,
    VolumeProfile,
    PivotPoints,
    SupportResistance,
    LiquidityCluster,
    AtrFallback,
}

impl std::fmt::Display for LevelSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelSource::Fibonacci => write!(f, "FIB"),
            LevelSource::VolumeProfile => write!(f, "VP"),
            LevelSource::PivotPoints => write!(f, "PP"),
            LevelSource::SupportResistance => write!(f, "SR"),
            LevelSource::LiquidityCluster => write!(f, "LIQ"),
            LevelSource::AtrFallback => write!(f, "ATR"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfluentLevel {
    pub price: f64,
    pub confluence_count: u32,
    pub sources: Vec<LevelSource>,
    pub strength: f64,
    /// v6.10.17 (F23): which trade direction this level serves, derived
    /// from its price relative to close — below close = LONG-side level,
    /// above close = SHORT-side level (a SHORT entry sits ABOVE price,
    /// its target BELOW). `None` when the level sits on the close.
    #[serde(default)]
    pub side: Option<String>,
}

/// v6.10.21 (NBR): a direction-agnostic range reference bracket emitted
/// when the market reads as a range (`is_range`) under a
/// `NoClearOpportunity` primary — no directional setup qualifies, but a
/// mathematically valid range-fade frame (entry band near the close,
/// target at the upper range bound, invalidation below the lower bound)
/// gives the operator reference geometry. Purely informational: it is
/// never a trade, never Actionable, and does not alter `profiles` or any
/// per-direction invariant. `None` when no valid frame can be derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeutralBracket {
    /// Range-fade entry band (centered on close, ±0.2×ATR width).
    pub entry_zone: PriceRange,
    /// Target band at the upper range bound.
    pub target_zone: PriceRange,
    /// Invalidation below the lower range bound (never inside `entry_zone`).
    pub invalidation_level: f64,
    /// NET expected reward/risk ratio (gross minus friction), gated by
    /// the same `compute_side_rr_v2` guards as the directional sides.
    pub expected_rr_internal: f64,
    /// Geometry-consistency flag: `invalidation_level < entry_zone.low`
    /// AND `target_zone.low > entry_zone.high`.
    pub geometry_consistent: bool,
    /// Human-readable origin, e.g. "range reference — no directional setup".
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OpportunityMatrix {
    pub symbol: String,
    pub primary_opportunity: OpportunityType,
    pub opportunity_score: f64,
    pub setup_quality: SetupQuality,
    pub profiles: Vec<OpportunityProfile>,
    pub forecast_confidence: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributing_signals: Vec<String>,
    pub invalidation_note: String,
    /// Legacy single-bias projection of `long_entry_zone` / `short_entry_zone`.
    /// PME/TAE consumers read this. The Opportunities tab reads the per-direction
    /// siblings instead.
    pub entry_zone: PriceRange,
    /// Legacy single-bias projection of `long_target_zone` / `short_target_zone`.
    pub target_zone: PriceRange,
    /// Legacy single-bias projection of `long_invalidation_level` /
    /// `short_invalidation_level`.
    pub invalidation_level: f64,
    /// Per-direction entry zone for a long setup (entry below close).
    pub long_entry_zone: PriceRange,
    /// Per-direction target zone for a long setup (target above close).
    pub long_target_zone: PriceRange,
    /// Per-direction invalidation trigger for a long setup (price below which the
    /// long thesis is invalidated).
    pub long_invalidation_level: f64,
    /// Per-direction entry zone for a short setup (entry above close).
    pub short_entry_zone: PriceRange,
    /// Per-direction target zone for a short setup (target below close).
    pub short_target_zone: PriceRange,
    /// Per-direction invalidation trigger for a short setup (price above which
    /// the short thesis is invalidated).
    pub short_invalidation_level: f64,
    /// Per-direction expected reward/risk ratio for a long setup, computed
    /// from `long_target_zone` vs `long_entry_zone` and `long_invalidation_level`.
    /// This is the canonical per-side R:R. The legacy `expected_rr_internal`
    /// matrix-level scalar was removed in v6.9; consumers now read this
    /// per-side value gated on `analysis.bias` to recover the active side.
    #[serde(default)]
    pub long_expected_rr_internal: f64,
    /// Per-direction expected reward/risk ratio for a short setup, computed
    /// from `short_target_zone` vs `short_entry_zone` and `short_invalidation_level`.
    #[serde(default)]
    pub short_expected_rr_internal: f64,
    /// v6.10.19 (P5): the GROSS geometric R:R (pre-cost) per side — the
    /// NET (gross minus estimated entry/exit fees + slippage) lives in
    /// `long_expected_rr_internal` / `short_expected_rr_internal`; the
    /// gross stays on the wire for offline/data-science analysis.
    #[serde(default)]
    pub long_gross_rr_internal: f64,
    #[serde(default)]
    pub short_gross_rr_internal: f64,
    pub time_horizon: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confluent_entry_levels: Vec<ConfluentLevel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confluent_target_levels: Vec<ConfluentLevel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confluent_invalidation_levels: Vec<ConfluentLevel>,
    /// Bias of the active setup. `Long` for bullish, `Short` for bearish,
    /// `None` for Neutral. The frontend `selectProfileSide` reads this to
    /// determine the per-card direction arrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_family: Option<crate::analysis::DirectionFamily>,
    /// Server-side geometry-consistency flag for the LONG side at matrix
    /// level. `true` when the aggregated bracket satisfies the L4 invariant:
    /// `long_invalidation_level < long_entry_zone.low` AND
    /// `long_target_zone.low > long_entry_zone.high`.
    #[serde(default)]
    pub long_geometry_consistent: bool,
    /// Server-side geometry-consistency flag for the SHORT side at matrix
    /// level.
    #[serde(default)]
    pub short_geometry_consistent: bool,
    /// v6.10.21 (NBR): direction-agnostic range reference bracket, present
    /// only when the primary is `NoClearOpportunity` and the regime is a
    /// range. Informational only — never actionable. Legacy payloads
    /// (field absent) deserialize to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neutral_reference_bracket: Option<NeutralBracket>,
}
