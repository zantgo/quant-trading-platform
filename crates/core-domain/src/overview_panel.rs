//! # Overview Panel — server-computed dashboard payload
//!
//! Single-source-of-truth builder for the Market Overview panel fields
//! that the GUI previously derived client-side in TypeScript
//! (`tradeAggregates.ts`, `decisionRank.ts`, `marketHealth.ts`) from
//! per-instance WebSocket payloads.
//!
//! The builder is **pure**: it consumes the per-instance snapshots the L7
//! aggregation task already reads (`MarketSnapshot` matrices) plus the
//! canonical AssetRank scores, and produces the panel payload. The L7 task
//! merges it into `OverviewMatrix`; both renderers (Svelte
//! `GeneralDashboard`, `cli_renderer`) then display the same
//! server-computed values — GUI and CLI can never disagree for the same
//! instances.
//!
//! Algorithms are exact ports of the frontend derivations (kept in sync
//! with `ui/src/lib/tradeAggregates.ts`, `decisionRank.ts`,
//! `marketHealth.ts`):
//! - hero verdict: TRADE / WAIT / STAND_ASIDE (Actionable + READY gate)
//! - per-profile viability normalization (R:R < 1.0 demotes Actionable)
//! - `selectProfileSide` direction resolution (zone-presence first,
//!   DirectionFamily × MarketBias fallback)
//! - `resolveActiveRr` chain (profile wire → matrix wire → zones
//!   fallback with the canonical `compute_side_rr_v2` + 0.10 floor)
//! - signal-quality buckets, direction counts, market-health bars.

use crate::analysis::{
    DirectionFamily, MarketBias, OpportunityProfile, OpportunityType, PriceRange, TradeViability,
};
use crate::models::MarketSnapshot;
use crate::risk_reward::{compute_side_rr_v2, Side};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Panel payload types ─────────────────────────────────────────────

/// Hero verdict — the panel's "can I trade anything right now?" answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HeroVerdict {
    Trade,
    Wait,
    StandAside,
}

impl std::fmt::Display for HeroVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeroVerdict::Trade => write!(f, "TRADE"),
            HeroVerdict::Wait => write!(f, "WAIT"),
            HeroVerdict::StandAside => write!(f, "STAND_ASIDE"),
        }
    }
}

/// Hero strip payload (subtext mirrors `RecommendationHero.svelte`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewHero {
    pub verdict: HeroVerdict,
    /// Setups with viability Actionable AND readiness READY.
    pub actionable_count: u32,
    /// All qualifying setups across instances.
    pub candidate_count: u32,
    pub best_symbol: Option<String>,
    pub best_score: f64,
    /// `LONG` | `SHORT` | `NEUTRAL`
    pub best_direction: String,
    pub best_confidence: f64,
    pub best_rr: f64,
    /// Instances considered.
    pub instance_count: u32,
}

/// One asset-ranking row — mirrors the GUI's 12-column table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewRow {
    pub symbol: String,
    /// Micro-window mid price (0.0 before the first snapshot).
    pub price: f64,
    /// PascalCase `MarketBias` (`Bullish` / `Neutral` / ...).
    pub bias: String,
    /// `BUY` | `SELL` | `WAIT` — Actionable + READY gated.
    pub signal: String,
    /// `LONG` | `SHORT` | `NEUTRAL` from `directional_guidance`.
    pub direction: String,
    /// Resolved active-side geometric R:R (0.0 = N/A).
    pub rr: f64,
    /// Canonical L7 AssetRank score, with the GUI's local fallback chain.
    pub score: f64,
    /// L6 `confidence_assessment` (0..100).
    pub confidence: f64,
    /// `AlignmentMatrix.mtf_overall_score` ∈ [-100, 100].
    pub mtf_score: f64,
    /// `AlignmentMatrix.mtf_overall_label`.
    pub mtf_label: String,
    /// Micro-window L5 `overall_risk.score`.
    pub risk: f64,
    /// Top-setup of the Opportunity Layer — resolved side of the
    /// displayed bracket (`LONG` / `SHORT` / `NEUTRAL`). Computed once in
    /// `build_overview_panel` so the GUI table and the CLI renderer never
    /// disagree (parity contract 01-10 §5).
    #[serde(default)]
    pub setup_side: String,
    /// Top-setup entry zone low bound (0.0 = N/A).
    #[serde(default)]
    pub entry_low: f64,
    /// Top-setup entry zone high bound (0.0 = N/A).
    #[serde(default)]
    pub entry_high: f64,
    /// Top-setup target (take-profit) zone low bound (0.0 = N/A).
    #[serde(default)]
    pub target_low: f64,
    /// Top-setup target (take-profit) zone high bound (0.0 = N/A).
    #[serde(default)]
    pub target_high: f64,
    /// Top-setup stop-loss (invalidation) level (0.0 = N/A).
    #[serde(default)]
    pub invalidation: f64,
    /// Micro-window snapshot timestamp (seconds since epoch).
    pub updated_ts: u64,
    /// Lifecycle-active (not stopped/cancelled).
    pub active: bool,
}

/// Signal-quality buckets (confidence bands, ≥70 / 40–69 / <40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SignalQuality {
    pub strong: u32,
    pub moderate: u32,
    pub weak: u32,
}

/// Direction counts from `directional_guidance` (missing → neutral).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DirectionDistribution {
    pub long: u32,
    pub short: u32,
    pub neutral: u32,
}

/// One market-health sub-dimension bar (quality value, high = good).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthBar {
    pub label: String,
    /// 0..100 quality value (risk inverted where the spec says so).
    pub value: f64,
    /// `false` when at least one instance exists but produced no data.
    pub available: bool,
    /// How many instances fed this bar.
    pub contributing_instances: u32,
}

/// The Market Health card's four sub-dimension bars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketHealthDims {
    pub bars: Vec<HealthBar>,
    /// Instances that contributed to at least one bar.
    pub active_instance_count: u32,
}

/// Everything the L7 task merges into `OverviewMatrix` (serde-defaulted).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OverviewPanel {
    pub hero: Option<OverviewHero>,
    pub rows: Vec<OverviewRow>,
    pub signal_quality: Option<SignalQuality>,
    pub direction_distribution: Option<DirectionDistribution>,
    pub market_health_dims: Option<MarketHealthDims>,
}

/// Per-instance builder input — the snapshots the L7 task already holds.
pub struct PanelInstance {
    pub symbol: String,
    /// Present per-TF snapshots, fastest first (micro, fast, slow, macro).
    /// The reference snapshot is `snapshots[0]` (same "fastest present
    /// window" rule the L7 alignment handling uses).
    pub snapshots: Vec<MarketSnapshot>,
    pub is_active: bool,
    /// Canonical L7 AssetRank score for the symbol (0.0 when not ranked).
    pub rank_score: f64,
}

// ─── Internal derivation state (ports) ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViabilityLabel {
    Actionable,
    Qualifying,
    DirectionalNeutral,
    GeometryInverted,
    NoClear,
}

#[derive(Debug, Clone)]
struct SetupSummary {
    symbol: String,
    score: f64,
    direction: &'static str,
    viability: ViabilityLabel,
    rr: f64,
    confidence: f64,
    readiness: String,
}

/// `normalizeViability` port — `None` wire value falls back to the
/// precondition-qualified `Qualifying` label (v6.10.17 P1).
fn normalize_viability(v: Option<TradeViability>, preconditions_met: u32) -> ViabilityLabel {
    match v {
        Some(TradeViability::Actionable) => ViabilityLabel::Actionable,
        Some(TradeViability::Qualifying) => ViabilityLabel::Qualifying,
        Some(TradeViability::DirectionalNeutral) => ViabilityLabel::DirectionalNeutral,
        Some(TradeViability::GeometryInverted) => ViabilityLabel::GeometryInverted,
        Some(TradeViability::NoClear) | None => {
            if preconditions_met > 0 {
                ViabilityLabel::Qualifying
            } else {
                ViabilityLabel::NoClear
            }
        }
    }
}

fn is_bullish(b: MarketBias) -> bool {
    matches!(b, MarketBias::Bullish | MarketBias::StrongBullish)
}

fn is_bearish(b: MarketBias) -> bool {
    matches!(b, MarketBias::Bearish | MarketBias::StrongBearish)
}

/// `selectProfileSide` port — zone presence wins, then
/// DirectionFamily × MarketBias.
fn select_profile_side(
    profile: &OpportunityProfile,
    macro_bias: Option<MarketBias>,
) -> &'static str {
    let long_zones = profile
        .long_entry_zone
        .as_ref()
        .map(|z| z.low > 0.0)
        .unwrap_or(false);
    let short_zones = profile
        .short_entry_zone
        .as_ref()
        .map(|z| z.low > 0.0)
        .unwrap_or(false);
    if long_zones != short_zones {
        return if long_zones { "LONG" } else { "SHORT" };
    }
    let Some(bias) = macro_bias else {
        return "NEUTRAL";
    };
    match profile.direction_family.unwrap_or(DirectionFamily::Neutral) {
        DirectionFamily::TrendRiding => {
            if is_bullish(bias) {
                "LONG"
            } else if is_bearish(bias) {
                "SHORT"
            } else {
                "NEUTRAL"
            }
        }
        DirectionFamily::CounterTrend => {
            if is_bullish(bias) {
                "SHORT"
            } else if is_bearish(bias) {
                "LONG"
            } else {
                "NEUTRAL"
            }
        }
        DirectionFamily::Neutral => "NEUTRAL",
    }
}

/// `profileRR` port — per-side wire R:R wins, aggregated falls back.
fn profile_rr(profile: &OpportunityProfile, direction: &str, aggregated: f64) -> f64 {
    if direction == "LONG" {
        let v = profile.long_expected_rr_internal;
        if v > 0.0 {
            return v;
        }
    } else if direction == "SHORT" {
        let v = profile.short_expected_rr_internal;
        if v > 0.0 {
            return v;
        }
    }
    if aggregated > 0.0 {
        aggregated
    } else {
        0.0
    }
}

/// `directionLabel` port — substring contract over `DirectionalGuidance`.
fn guidance_direction(guidance: Option<crate::advisory::DirectionalGuidance>) -> &'static str {
    match guidance {
        Some(crate::advisory::DirectionalGuidance::StrongLong)
        | Some(crate::advisory::DirectionalGuidance::Long) => "LONG",
        Some(crate::advisory::DirectionalGuidance::StrongShort)
        | Some(crate::advisory::DirectionalGuidance::Short) => "SHORT",
        _ => "NEUTRAL",
    }
}

/// `signalLabel` port — BUY/SELL/WAIT from the guidance token.
fn signal_from_guidance(guidance: Option<crate::advisory::DirectionalGuidance>) -> &'static str {
    match guidance_direction(guidance) {
        "LONG" => "BUY",
        "SHORT" => "SELL",
        _ => "WAIT",
    }
}

/// `topQualifyingProfile` port — score desc, then precondition ratio,
/// then primary-opportunity priority (02-08 §6 tie rule).
fn top_qualifying_profile(
    opp: &crate::opportunity::OpportunityMatrix,
) -> Option<&OpportunityProfile> {
    let mut qualifying: Vec<&OpportunityProfile> = opp
        .profiles
        .iter()
        .filter(|p| {
            p.preconditions_met > 0 && p.opportunity_type != OpportunityType::NoClearOpportunity
        })
        .collect();
    if qualifying.is_empty() {
        return None;
    }
    let primary = opp.primary_opportunity;
    qualifying.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ar = if a.preconditions_total > 0 {
                    a.preconditions_met as f64 / a.preconditions_total as f64
                } else {
                    0.0
                };
                let br = if b.preconditions_total > 0 {
                    b.preconditions_met as f64 / b.preconditions_total as f64
                } else {
                    0.0
                };
                br.partial_cmp(&ar).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                let a_primary = a.opportunity_type == primary;
                let b_primary = b.opportunity_type == primary;
                match (a_primary, b_primary) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                }
            })
    });
    qualifying.into_iter().next()
}

/// `collectActiveSetups` port.
fn collect_active_setups(instances: &[PanelInstance]) -> Vec<SetupSummary> {
    let mut out: Vec<SetupSummary> = Vec::new();
    for inst in instances {
        let Some(reference) = inst.snapshots.first() else {
            continue;
        };
        let Some(opp) = reference.opportunity.as_ref() else {
            continue;
        };
        let macro_bias = reference.analysis.as_ref().map(|a| a.bias);
        let readiness = reference
            .decision_context
            .as_ref()
            .map(|d| d.trade_readiness.clone())
            .unwrap_or_else(|| "STAND_ASIDE".to_string());
        let confidence = reference
            .advisory
            .as_ref()
            .map(|a| a.confidence_assessment)
            .unwrap_or(0.0);
        let aggregated_long = opp.long_expected_rr_internal;
        let aggregated_short = opp.short_expected_rr_internal;
        for p in &opp.profiles {
            if p.preconditions_met == 0 {
                continue;
            }
            if p.opportunity_type == OpportunityType::NoClearOpportunity {
                continue;
            }
            let direction = select_profile_side(p, macro_bias);
            let aggregated = if direction == "SHORT" {
                aggregated_short
            } else {
                aggregated_long
            };
            let mut viability = normalize_viability(p.trade_viability, p.preconditions_met);
            // v6.10.18 (I-5): ACTIONABLE additionally requires R:R ≥ 1.0.
            let side_rr = if direction == "SHORT" {
                p.short_expected_rr_internal
            } else {
                p.long_expected_rr_internal
            };
            if viability == ViabilityLabel::Actionable && side_rr < 1.0 {
                viability = ViabilityLabel::Qualifying;
            }
            out.push(SetupSummary {
                symbol: inst.symbol.clone(),
                score: p.score,
                direction,
                viability,
                rr: profile_rr(p, direction, aggregated),
                confidence,
                readiness: readiness.clone(),
            });
        }
    }
    out
}

fn is_ready(readiness: &str) -> bool {
    readiness.eq_ignore_ascii_case("READY")
}

/// `computeHeroState` + `pickBestOpportunity` port.
fn hero_and_best(
    setups: &[SetupSummary],
    instance_count: u32,
) -> (HeroVerdict, Option<SetupSummary>) {
    if instance_count == 0 || setups.is_empty() {
        return (HeroVerdict::StandAside, None);
    }
    let actionable: Vec<&SetupSummary> = setups
        .iter()
        .filter(|s| s.viability == ViabilityLabel::Actionable && is_ready(&s.readiness))
        .collect();
    let verdict = if actionable.is_empty() {
        HeroVerdict::Wait
    } else {
        HeroVerdict::Trade
    };
    let pool: Vec<&SetupSummary> = if actionable.is_empty() {
        setups.iter().collect()
    } else {
        actionable
    };
    let best = pool
        .iter()
        .max_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.rr.partial_cmp(&a.rr).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|s| (*s).clone());
    (verdict, best)
}

/// `bias_market` — the decision context's PascalCase bias string wins over
/// the analysis matrix bias, exactly like the frontend's `macroBias =
/// decisionContext?.bias ?? analysis?.bias`.
fn bias_market(
    decision_bias: Option<&str>,
    analysis_bias: Option<MarketBias>,
) -> Option<MarketBias> {
    decision_bias
        .and_then(|b| {
            let u = b.to_uppercase();
            if u.contains("BULLISH") {
                Some(MarketBias::Bullish)
            } else if u.contains("BEARISH") {
                Some(MarketBias::Bearish)
            } else {
                None
            }
        })
        .or(analysis_bias)
}

/// `resolveActiveRr` port — profile wire → matrix wire → zones fallback
/// (canonical `compute_side_rr_v2`, 0.10 meaningfulness floor).
fn resolve_active_rr(
    opp: &crate::opportunity::OpportunityMatrix,
    decision_bias: Option<&str>,
    analysis_bias: Option<MarketBias>,
    close: Option<f64>,
) -> (f64, bool) {
    let bias_market = bias_market(decision_bias, analysis_bias);
    let top = top_qualifying_profile(opp);
    let side: &'static str = if let Some(t) = top {
        select_profile_side(t, bias_market)
    } else if let Some(b) = bias_market {
        if is_bullish(b) {
            "LONG"
        } else if is_bearish(b) {
            "SHORT"
        } else {
            "NEUTRAL"
        }
    } else {
        "NEUTRAL"
    };
    if side == "NEUTRAL" {
        return (0.0, false);
    }
    let top_val = top.map(|t| {
        if side == "LONG" {
            t.long_expected_rr_internal
        } else {
            t.short_expected_rr_internal
        }
    });
    let matrix_val = if side == "LONG" {
        opp.long_expected_rr_internal
    } else {
        opp.short_expected_rr_internal
    };
    let wire_rr = top_val.unwrap_or(matrix_val);
    if wire_rr >= 0.10 {
        return (wire_rr, true);
    }
    if wire_rr > 0.0 {
        return (0.0, false);
    }
    // Respect the server's geometry verdict — a bracket flagged inverted
    // must not leak a locally-recomputed R:R.
    let server_consistent = if side == "LONG" {
        top.map(|t| t.long_geometry_consistent)
            .unwrap_or(opp.long_geometry_consistent)
    } else {
        top.map(|t| t.short_geometry_consistent)
            .unwrap_or(opp.short_geometry_consistent)
    };
    if !server_consistent {
        return (0.0, false);
    }
    let (entry, target, inv) = if side == "LONG" {
        (
            top.and_then(|t| t.long_entry_zone.clone())
                .or_else(|| Some(opp.long_entry_zone.clone())),
            top.and_then(|t| t.long_target_zone.clone())
                .or_else(|| Some(opp.long_target_zone.clone())),
            top.and_then(|t| t.long_invalidation_level)
                .or(Some(opp.long_invalidation_level)),
        )
    } else {
        (
            top.and_then(|t| t.short_entry_zone.clone())
                .or_else(|| Some(opp.short_entry_zone.clone())),
            top.and_then(|t| t.short_target_zone.clone())
                .or_else(|| Some(opp.short_target_zone.clone())),
            top.and_then(|t| t.short_invalidation_level)
                .or(Some(opp.short_invalidation_level)),
        )
    };
    let (Some(entry), Some(target), Some(inv)) = (entry, target, inv) else {
        return (0.0, false);
    };
    if entry.low <= 0.0
        || entry.high <= 0.0
        || target.low <= 0.0
        || target.high <= 0.0
        || inv <= 0.0
    {
        return (0.0, false);
    }
    let close = close.unwrap_or_else(|| (entry.low + entry.high) / 2.0);
    let side_enum = if side == "LONG" {
        Side::Long
    } else {
        Side::Short
    };
    match compute_side_rr_v2(
        entry.low,
        entry.high,
        target.low,
        target.high,
        inv,
        close,
        side_enum,
    ) {
        crate::risk_reward::SideRrStatus::Value(rr) => (rr, true),
        _ => (0.0, false),
    }
}

fn f64_from_decimal(d: rust_decimal::Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

// ─── Top-setup of the Opportunity Layer (per-row zones) ──────────────

/// The entry / take-profit / stop-loss geometry of a resolved setup.
/// `0.0` bounds mean "no bracket for this side" (renders `—`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct SetupZones {
    side: &'static str,
    entry_low: f64,
    entry_high: f64,
    target_low: f64,
    target_high: f64,
    invalidation: f64,
}

const NO_SETUP: SetupZones = SetupZones {
    side: "NEUTRAL",
    entry_low: 0.0,
    entry_high: 0.0,
    target_low: 0.0,
    target_high: 0.0,
    invalidation: 0.0,
};

/// `profileZones` port — per-profile per-side zone read with the
/// non-positive bound guard (`decisionRank.ts`). Returns the
/// (entry, target, invalidation) tuple; the R:R derivation is skipped
/// because the table only renders the geometry.
fn profile_zones<'a>(
    profile: &'a OpportunityProfile,
    side: &str,
) -> Option<(&'a PriceRange, &'a PriceRange, f64)> {
    let (entry, target, inv) = if side == "LONG" {
        (
            profile.long_entry_zone.as_ref(),
            profile.long_target_zone.as_ref(),
            profile.long_invalidation_level,
        )
    } else {
        (
            profile.short_entry_zone.as_ref(),
            profile.short_target_zone.as_ref(),
            profile.short_invalidation_level,
        )
    };
    // v11.12: `?` instead of let-else (clippy) — same semantics, the fn
    // returns Option.
    let entry = entry?;
    let target = target?;
    let inv = inv?;
    if entry.low <= 0.0
        || entry.high <= 0.0
        || target.low <= 0.0
        || target.high <= 0.0
        || inv <= 0.0
    {
        return None;
    }
    Some((entry, target, inv))
}

/// `aggregateZones` port — matrix-level per-direction bracket, used as the
/// fallback when per-profile zones are absent.
fn aggregate_zones<'a>(
    opp: &'a crate::opportunity::OpportunityMatrix,
    side: &str,
) -> Option<(&'a PriceRange, &'a PriceRange, f64)> {
    let (entry, target, inv) = if side == "LONG" {
        (
            &opp.long_entry_zone,
            &opp.long_target_zone,
            opp.long_invalidation_level,
        )
    } else {
        (
            &opp.short_entry_zone,
            &opp.short_target_zone,
            opp.short_invalidation_level,
        )
    };
    if entry.low <= 0.0
        || entry.high <= 0.0
        || target.low <= 0.0
        || target.high <= 0.0
        || inv <= 0.0
    {
        return None;
    }
    Some((entry, target, inv))
}

fn zones_from(triple: (&PriceRange, &PriceRange, f64), side: &'static str) -> SetupZones {
    SetupZones {
        side,
        entry_low: triple.0.low,
        entry_high: triple.0.high,
        target_low: triple.1.low,
        target_high: triple.1.high,
        invalidation: triple.2,
    }
}

/// `topSetupSummary` legacy zone chain port (`decisionRank.ts`): top
/// qualifying profile → `select_profile_side` → per-profile zones →
/// aggregated matrix-zone fallback (with the net-bias side fallback).
/// This is the canonical "top setup of the Opportunity Layer" the Asset
/// Rankings table's ENTRY / TARGET / STOP columns render.
fn top_setup_zones(
    opp: &crate::opportunity::OpportunityMatrix,
    bias: Option<MarketBias>,
    net_bias_pct: f64,
) -> SetupZones {
    let Some(top) = top_qualifying_profile(opp) else {
        // No qualifying profile — the aggregated bracket path. The side
        // resolves from the bias first, then `net_bias_pct`, then NEUTRAL.
        let side: &'static str = match bias {
            Some(b) if is_bullish(b) => "LONG",
            Some(b) if is_bearish(b) => "SHORT",
            _ => {
                if net_bias_pct < 0.0 {
                    "SHORT"
                } else if net_bias_pct > 0.0 {
                    "LONG"
                } else {
                    "NEUTRAL"
                }
            }
        };
        if side == "NEUTRAL" {
            return NO_SETUP;
        }
        return match aggregate_zones(opp, side) {
            Some(z) => zones_from(z, side),
            None => NO_SETUP,
        };
    };
    let side = select_profile_side(top, bias);
    if let Some(z) = (side != "NEUTRAL")
        .then(|| profile_zones(top, side))
        .flatten()
    {
        return zones_from(z, side);
    }
    // No directional resolution — prefer the gauge direction (net_bias_pct)
    // over a blind LONG default, mirroring the TS fallback.
    let fallback_side: &'static str = if side == "NEUTRAL" {
        if net_bias_pct < 0.0 {
            "SHORT"
        } else {
            "LONG"
        }
    } else {
        side
    };
    match aggregate_zones(opp, fallback_side) {
        Some(z) => zones_from(z, fallback_side),
        None => NO_SETUP,
    }
}

// ─── Public builder ──────────────────────────────────────────────────

/// Build the full panel payload from per-instance snapshots.
/// `ranks` maps symbol → canonical L7 AssetRank score (from
/// `compute_overview` output); symbols absent from the map get the GUI's
/// local fallback chain (max profile score → opportunity score).
pub fn build_overview_panel(
    instances: &[PanelInstance],
    ranks: &HashMap<String, f64>,
) -> OverviewPanel {
    // ── Hero ─────────────────────────────────────────────────────────
    let setups = collect_active_setups(instances);
    let (verdict, best) = hero_and_best(&setups, instances.len() as u32);
    let actionable_count = setups
        .iter()
        .filter(|s| s.viability == ViabilityLabel::Actionable && is_ready(&s.readiness))
        .count() as u32;
    let hero = Some(OverviewHero {
        verdict,
        actionable_count,
        candidate_count: setups.len() as u32,
        best_symbol: best.as_ref().map(|s| s.symbol.clone()),
        best_score: best.as_ref().map(|s| s.score).unwrap_or(0.0),
        best_direction: best
            .as_ref()
            .map(|s| s.direction)
            .unwrap_or("NEUTRAL")
            .to_string(),
        best_confidence: best.as_ref().map(|s| s.confidence).unwrap_or(0.0),
        best_rr: best.as_ref().map(|s| s.rr).unwrap_or(0.0),
        instance_count: instances.len() as u32,
    });

    // ── Rows (asset ranking table) ───────────────────────────────────
    let mut rows: Vec<OverviewRow> = Vec::new();
    for inst in instances {
        let reference = inst.snapshots.first();
        let opp = reference.and_then(|s| s.opportunity.as_ref());
        let analysis = reference.and_then(|s| s.analysis.as_ref());
        let advisory = reference.and_then(|s| s.advisory.as_ref());
        let risk = reference.and_then(|s| s.risk.as_ref());
        let alignment = reference.and_then(|s| s.alignment.as_ref());
        let decision = reference.and_then(|s| s.decision_context.as_ref());

        let price = reference
            .map(|s| f64_from_decimal(s.mid_price))
            .unwrap_or(0.0);
        let bias = analysis
            .map(|a| format!("{:?}", a.bias))
            .unwrap_or_else(|| "Neutral".to_string());
        let guidance = advisory.map(|a| a.directional_guidance);
        let confidence = advisory.map(|a| a.confidence_assessment).unwrap_or(0.0);

        // Signal gate: Actionable top profile + READY readiness.
        let top_profile = opp.and_then(top_qualifying_profile);
        let top_viability = top_profile
            .map(|p| normalize_viability(p.trade_viability, p.preconditions_met))
            .unwrap_or(ViabilityLabel::NoClear);
        let readiness = decision
            .map(|d| d.trade_readiness.as_str())
            .unwrap_or("STAND_ASIDE");
        let signal = if is_ready(readiness) && top_viability == ViabilityLabel::Actionable {
            signal_from_guidance(guidance)
        } else {
            "WAIT"
        };

        // Score: canonical AssetRank first, then the GUI fallback chain.
        let rank_score = ranks.get(&inst.symbol).copied().unwrap_or(0.0);
        let score = if rank_score > 0.0 {
            rank_score
        } else if let Some(opp) = opp {
            let max_profile = opp.profiles.iter().map(|p| p.score).fold(0.0_f64, f64::max);
            if max_profile > 0.0 {
                max_profile
            } else {
                opp.opportunity_score
            }
        } else {
            0.0
        };

        let (rr, _) = opp
            .map(|o| {
                resolve_active_rr(
                    o,
                    decision.map(|d| d.bias.as_str()),
                    analysis.map(|a| a.bias),
                    reference
                        .and_then(|s| s.close.as_ref())
                        .map(|d| f64_from_decimal(*d)),
                )
            })
            .unwrap_or((0.0, false));

        // Top-setup of the Opportunity Layer — entry / target / stop-loss
        // geometry for the Asset Rankings table (GUI + CLI share the row).
        let setup = opp
            .map(|o| {
                top_setup_zones(
                    o,
                    bias_market(decision.map(|d| d.bias.as_str()), analysis.map(|a| a.bias)),
                    decision.map(|d| d.net_bias_pct).unwrap_or(0.0),
                )
            })
            .unwrap_or(NO_SETUP);

        let mtf_score = alignment.map(|a| a.mtf_overall_score).unwrap_or(0.0);
        let mtf_label = alignment
            .map(|a| a.mtf_overall_label.clone())
            .unwrap_or_else(|| "NO_DATA".to_string());
        let risk_score = risk.map(|r| r.overall_risk.score).unwrap_or(0.0);
        let updated_ts = reference.map(|s| s.timestamp).unwrap_or(0);

        rows.push(OverviewRow {
            symbol: inst.symbol.clone(),
            price,
            bias,
            signal: signal.to_string(),
            direction: guidance_direction(guidance).to_string(),
            rr,
            score,
            confidence,
            mtf_score,
            mtf_label,
            risk: risk_score,
            setup_side: setup.side.to_string(),
            entry_low: setup.entry_low,
            entry_high: setup.entry_high,
            target_low: setup.target_low,
            target_high: setup.target_high,
            invalidation: setup.invalidation,
            updated_ts,
            active: inst.is_active,
        });
    }

    // ── Signal quality + direction distribution ─────────────────────
    let mut signal_quality = SignalQuality::default();
    let mut directions = DirectionDistribution::default();
    for inst in instances {
        let reference = match inst.snapshots.first() {
            Some(s) => s,
            None => {
                directions.neutral += 1;
                signal_quality.weak += 1;
                continue;
            }
        };
        let c = reference
            .advisory
            .as_ref()
            .map(|a| a.confidence_assessment)
            .unwrap_or(0.0);
        if c >= 70.0 {
            signal_quality.strong += 1;
        } else if c >= 40.0 {
            signal_quality.moderate += 1;
        } else {
            signal_quality.weak += 1;
        }
        match reference.advisory.as_ref().map(|a| a.directional_guidance) {
            Some(crate::advisory::DirectionalGuidance::StrongLong)
            | Some(crate::advisory::DirectionalGuidance::Long) => directions.long += 1,
            Some(crate::advisory::DirectionalGuidance::StrongShort)
            | Some(crate::advisory::DirectionalGuidance::Short) => directions.short += 1,
            _ => directions.neutral += 1,
        }
    }
    let total_instances = instances.len() as u32;
    let signal_quality = if total_instances > 0 {
        Some(signal_quality)
    } else {
        None
    };
    let direction_distribution = if total_instances > 0 {
        Some(directions)
    } else {
        None
    };

    // ── Market health sub-dimension bars (marketHealth.ts port) ─────
    // Trend Strength   = 100 − structure_risk.score
    // Liquidity         = 100 − execution_liquidity_risk.score (confidence > 0 gate)
    // Volatility Regime = volatility_risk.score (NOT inverted)
    // Signal Stability  = 100 − signal_risk.score
    let mut trend: (f64, u32) = (0.0, 0);
    let mut liq: (f64, u32) = (0.0, 0);
    let mut vol: (f64, u32) = (0.0, 0);
    let mut sig: (f64, u32) = (0.0, 0);
    for inst in instances {
        let Some(reference) = inst.snapshots.first() else {
            continue;
        };
        let Some(r) = reference.risk.as_ref() else {
            continue;
        };
        trend.0 += 100.0 - r.structure_risk.score;
        trend.1 += 1;
        if r.execution_liquidity_risk.confidence > 0.0 {
            liq.0 += 100.0 - r.execution_liquidity_risk.score;
            liq.1 += 1;
        }
        vol.0 += r.volatility_risk.score;
        vol.1 += 1;
        sig.0 += 100.0 - r.signal_risk.score;
        sig.1 += 1;
    }
    let avg = |b: (f64, u32)| if b.1 > 0 { b.0 / b.1 as f64 } else { 0.0 };
    let bar = |label: &str, bucket: (f64, u32)| HealthBar {
        label: label.to_string(),
        value: avg(bucket),
        available: bucket.1 > 0,
        contributing_instances: bucket.1,
    };
    let market_health_dims = if instances.is_empty() {
        None
    } else {
        Some(MarketHealthDims {
            bars: vec![
                bar("TREND STRENGTH", trend),
                bar("LIQUIDITY", liq),
                bar("VOLATILITY", vol),
                bar("SIGNAL STABILITY", sig),
            ],
            active_instance_count: trend.1.max(liq.1).max(vol.1).max(sig.1),
        })
    };

    OverviewPanel {
        hero,
        rows,
        signal_quality,
        direction_distribution,
        market_health_dims,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{AnalysisMatrix, OpportunityProfile, OpportunityType};
    use crate::decision_context::DecisionContext;
    use crate::models::MarketSnapshot;
    use crate::opportunity::OpportunityMatrix;
    use crate::risk::RiskDimension;
    use crate::risk::RiskMatrix;
    use rust_decimal::Decimal;

    fn base_snapshot(symbol: &str, price: f64) -> MarketSnapshot {
        MarketSnapshot {
            symbol: symbol.to_string(),
            mid_price: Decimal::from_f64_retain(price).unwrap_or_default(),
            close: Decimal::from_f64_retain(price),
            timestamp: 1_700_000_000,
            ..MarketSnapshot::default()
        }
    }

    fn profile(
        ot: OpportunityType,
        score: f64,
        pre: u32,
        rr: f64,
        viability: Option<TradeViability>,
    ) -> OpportunityProfile {
        OpportunityProfile {
            opportunity_type: ot,
            score,
            preconditions_met: pre,
            preconditions_total: 5,
            notes: String::new(),
            direction_family: Some(DirectionFamily::TrendRiding),
            long_entry_zone: None,
            long_target_zone: None,
            long_invalidation_level: None,
            short_entry_zone: None,
            short_target_zone: None,
            short_invalidation_level: None,
            long_expected_rr_internal: rr,
            short_expected_rr_internal: 0.0,
            trade_viability: viability,
            long_geometry_consistent: true,
            short_geometry_consistent: true,
            scoring_factors: None,
            display_score: None,
        }
    }

    fn instance_with(
        symbol: &str,
        opp: Option<OpportunityMatrix>,
        analysis_bias: Option<MarketBias>,
        readiness: &str,
        confidence: f64,
    ) -> PanelInstance {
        let mut snap = base_snapshot(symbol, 100.0);
        snap.opportunity = opp;
        snap.analysis = analysis_bias.map(|b| {
            let mut a = AnalysisMatrix::empty(symbol);
            a.bias = b;
            a
        });
        snap.advisory = Some({
            let mut adv = crate::advisory::AdvisoryMatrix::empty(symbol);
            adv.confidence_assessment = confidence;
            adv.directional_guidance = crate::advisory::DirectionalGuidance::Long;
            adv
        });
        snap.decision_context = Some(DecisionContext {
            score: 0.0,
            bias: "Neutral".to_string(),
            score_confidence: 0.5,
            entry_danger: RiskDimension::default(),
            expected_reward_risk_ratio: 0.0,
            trade_readiness: readiness.to_string(),
            contributing_indicators: Vec::new(),
            long_probability: 0.0,
            short_probability: 0.0,
            hold_probability: 0.0,
            net_bias_pct: 0.0,
            lean_floor_applied: false,
        });
        PanelInstance {
            symbol: symbol.to_string(),
            snapshots: vec![snap],
            is_active: true,
            rank_score: 0.0,
        }
    }

    fn opp_with_profile(symbol: &str, p: OpportunityProfile) -> OpportunityMatrix {
        OpportunityMatrix {
            symbol: symbol.to_string(),
            profiles: vec![p],
            ..OpportunityMatrix::default()
        }
    }

    fn profile_with_long_zones(
        ot: OpportunityType,
        score: f64,
        pre: u32,
        entry: (f64, f64),
        target: (f64, f64),
        inv: f64,
    ) -> OpportunityProfile {
        let mut p = profile(ot, score, pre, 2.5, Some(TradeViability::Actionable));
        p.long_entry_zone = Some(PriceRange {
            low: entry.0,
            high: entry.1,
        });
        p.long_target_zone = Some(PriceRange {
            low: target.0,
            high: target.1,
        });
        p.long_invalidation_level = Some(inv);
        p
    }

    fn opp_with_matrix_zones(symbol: &str, p: OpportunityProfile) -> OpportunityMatrix {
        let mut o = opp_with_profile(symbol, p);
        o.long_entry_zone = PriceRange {
            low: 63000.0,
            high: 63400.0,
        };
        o.long_target_zone = PriceRange {
            low: 66000.0,
            high: 66500.0,
        };
        o.long_invalidation_level = 62800.0;
        o.short_entry_zone = PriceRange {
            low: 64500.0,
            high: 64900.0,
        };
        o.short_target_zone = PriceRange {
            low: 62000.0,
            high: 62500.0,
        };
        o.short_invalidation_level = 65200.0;
        o
    }

    fn instance_with_net_bias(
        symbol: &str,
        opp: Option<OpportunityMatrix>,
        net_bias_pct: f64,
        readiness: &str,
    ) -> PanelInstance {
        let mut inst = instance_with(symbol, opp, None, readiness, 50.0);
        if let Some(snap) = inst.snapshots.first_mut() {
            if let Some(dc) = snap.decision_context.as_mut() {
                dc.net_bias_pct = net_bias_pct;
            }
        }
        inst
    }

    #[test]
    fn empty_inputs_yield_stand_aside_and_no_rows() {
        let panel = build_overview_panel(&[], &HashMap::new());
        let hero = panel.hero.unwrap();
        assert_eq!(hero.verdict, HeroVerdict::StandAside);
        assert!(panel.rows.is_empty());
        assert!(panel.signal_quality.is_none());
        assert!(panel.market_health_dims.is_none());
    }

    #[test]
    fn actionable_ready_profile_yields_trade_hero() {
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_profile("BTC", p)),
            Some(MarketBias::Bullish),
            "READY",
            80.0,
        );
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let hero = panel.hero.unwrap();
        assert_eq!(hero.verdict, HeroVerdict::Trade);
        assert_eq!(hero.actionable_count, 1);
        assert_eq!(hero.best_symbol.as_deref(), Some("BTC"));
        assert_eq!(hero.best_direction, "LONG");
        assert!(hero.best_rr >= 2.5);
    }

    #[test]
    fn sub_1_rr_demotes_actionable_to_qualifying() {
        let p = profile(
            OpportunityType::Scalp,
            60.0,
            4,
            0.5,
            Some(TradeViability::Actionable),
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_profile("BTC", p)),
            Some(MarketBias::Bullish),
            "READY",
            70.0,
        );
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let hero = panel.hero.unwrap();
        // Actionable demoted to Qualifying by the R:R < 1.0 rule → WAIT.
        assert_eq!(hero.verdict, HeroVerdict::Wait);
        assert_eq!(hero.actionable_count, 0);
    }

    #[test]
    fn readiness_gate_keeps_hero_on_wait() {
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_profile("BTC", p)),
            Some(MarketBias::Bullish),
            "STAND_ASIDE",
            80.0,
        );
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let hero = panel.hero.unwrap();
        assert_eq!(hero.verdict, HeroVerdict::Wait);
        assert_eq!(hero.actionable_count, 0);
    }

    #[test]
    fn row_carries_signal_direction_and_rank_score() {
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_profile("BTC", p)),
            Some(MarketBias::Bullish),
            "READY",
            80.0,
        );
        let mut ranks = HashMap::new();
        ranks.insert("BTC".to_string(), 61.0);
        let panel = build_overview_panel(&[inst], &ranks);
        let row = &panel.rows[0];
        assert_eq!(row.symbol, "BTC");
        assert_eq!(row.signal, "BUY");
        assert_eq!(row.direction, "LONG");
        assert_eq!(row.score, 61.0);
        assert!(row.rr >= 2.5);
        assert_eq!(row.confidence, 80.0);
    }

    #[test]
    fn signal_quality_and_direction_buckets_match_trade_aggregates() {
        let a = instance_with("BTC", None, None, "STAND_ASIDE", 85.0);
        let b = instance_with("ETH", None, None, "STAND_ASIDE", 45.0);
        let c = instance_with("SOL", None, None, "STAND_ASIDE", 10.0);
        let panel = build_overview_panel(&[a, b, c], &HashMap::new());
        let sq = panel.signal_quality.unwrap();
        assert_eq!(sq.strong, 1);
        assert_eq!(sq.moderate, 1);
        assert_eq!(sq.weak, 1);
        // The fixture advisory carries Long guidance on all three.
        let dir = panel.direction_distribution.unwrap();
        assert_eq!(dir.long, 3);
        assert_eq!(dir.neutral, 0);
    }

    #[test]
    fn missing_advisory_counts_as_neutral_and_weak() {
        // GUI contract: an instance without an advisory contributes a
        // neutral direction and a weak signal bucket (tradeAggregates.ts).
        let snap = base_snapshot("BTC", 100.0);
        let panel = build_overview_panel(
            &[PanelInstance {
                symbol: "BTC".to_string(),
                snapshots: vec![snap],
                is_active: true,
                rank_score: 0.0,
            }],
            &HashMap::new(),
        );
        let dir = panel.direction_distribution.unwrap();
        assert_eq!(dir.neutral, 1);
        let sq = panel.signal_quality.unwrap();
        assert_eq!(sq.weak, 1);
    }

    #[test]
    fn market_health_bars_invert_risk_dimensions() {
        let mut snap = base_snapshot("BTC", 100.0);
        let mut risk = RiskMatrix::empty("BTC");
        risk.structure_risk = RiskDimension::from_score(40.0);
        risk.execution_liquidity_risk = RiskDimension::from_score(30.0);
        risk.execution_liquidity_risk.confidence = 100.0;
        risk.volatility_risk = RiskDimension::from_score(60.0);
        risk.signal_risk = RiskDimension::from_score(20.0);
        snap.risk = Some(risk);
        let panel = build_overview_panel(
            &[PanelInstance {
                symbol: "BTC".to_string(),
                snapshots: vec![snap],
                is_active: true,
                rank_score: 0.0,
            }],
            &HashMap::new(),
        );
        let dims = panel.market_health_dims.unwrap();
        assert_eq!(dims.bars.len(), 4);
        assert!((dims.bars[0].value - 60.0).abs() < 1e-9); // 100 - 40
        assert!((dims.bars[1].value - 70.0).abs() < 1e-9); // 100 - 30
        assert!((dims.bars[2].value - 60.0).abs() < 1e-9); // not inverted
        assert!((dims.bars[3].value - 80.0).abs() < 1e-9); // 100 - 20
        assert_eq!(dims.active_instance_count, 1);
    }

    #[test]
    fn liquidity_bar_excludes_feed_off_instances() {
        let mut snap = base_snapshot("BTC", 100.0);
        let mut risk = RiskMatrix::empty("BTC");
        risk.structure_risk = RiskDimension::from_score(40.0);
        risk.execution_liquidity_risk = RiskDimension::from_score(30.0);
        risk.execution_liquidity_risk.confidence = 0.0; // feed OFF
        risk.volatility_risk = RiskDimension::from_score(60.0);
        risk.signal_risk = RiskDimension::from_score(20.0);
        snap.risk = Some(risk);
        let panel = build_overview_panel(
            &[PanelInstance {
                symbol: "BTC".to_string(),
                snapshots: vec![snap],
                is_active: true,
                rank_score: 0.0,
            }],
            &HashMap::new(),
        );
        let dims = panel.market_health_dims.unwrap();
        assert_eq!(dims.bars[1].contributing_instances, 0);
        assert!(!dims.bars[1].available);
    }

    #[test]
    fn row_carries_top_setup_zones_from_profile() {
        let p = profile_with_long_zones(
            OpportunityType::Scalp,
            70.0,
            4,
            (63200.0, 63400.0),
            (66000.0, 66500.0),
            62800.0,
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_profile("BTC", p)),
            Some(MarketBias::Bullish),
            "READY",
            80.0,
        );
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.setup_side, "LONG");
        assert_eq!(row.entry_low, 63200.0);
        assert_eq!(row.entry_high, 63400.0);
        assert_eq!(row.target_low, 66000.0);
        assert_eq!(row.target_high, 66500.0);
        assert_eq!(row.invalidation, 62800.0);
    }

    #[test]
    fn row_top_setup_defaults_to_none_without_opportunity() {
        let inst = instance_with("BTC", None, None, "STAND_ASIDE", 0.0);
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.setup_side, "NEUTRAL");
        assert_eq!(row.entry_low, 0.0);
        assert_eq!(row.entry_high, 0.0);
        assert_eq!(row.target_low, 0.0);
        assert_eq!(row.target_high, 0.0);
        assert_eq!(row.invalidation, 0.0);
    }

    #[test]
    fn row_top_setup_falls_back_to_matrix_zones() {
        // Profile without per-profile zones → aggregated matrix bracket.
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let inst = instance_with(
            "BTC",
            Some(opp_with_matrix_zones("BTC", p)),
            Some(MarketBias::Bullish),
            "READY",
            80.0,
        );
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.setup_side, "LONG");
        assert_eq!(row.entry_low, 63000.0);
        assert_eq!(row.entry_high, 63400.0);
        assert_eq!(row.target_low, 66000.0);
        assert_eq!(row.target_high, 66500.0);
        assert_eq!(row.invalidation, 62800.0);
    }

    #[test]
    fn row_top_setup_side_falls_back_to_net_bias() {
        // Neutral bias + no profile zones → the gauge direction picks the
        // side (net_bias_pct < 0 → SHORT aggregate bracket).
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let inst =
            instance_with_net_bias("BTC", Some(opp_with_matrix_zones("BTC", p)), -5.0, "READY");
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.setup_side, "SHORT");
        assert_eq!(row.entry_low, 64500.0);
        assert_eq!(row.entry_high, 64900.0);
        assert_eq!(row.target_low, 62000.0);
        assert_eq!(row.target_high, 62500.0);
        assert_eq!(row.invalidation, 65200.0);
    }

    #[test]
    fn top_setup_zones_ignore_positive_guard_violations() {
        // A matrix entry zone with non-positive bounds must read N/A.
        let p = profile(
            OpportunityType::Scalp,
            70.0,
            4,
            2.5,
            Some(TradeViability::Actionable),
        );
        let mut opp = opp_with_matrix_zones("BTC", p);
        opp.long_entry_zone = PriceRange {
            low: 0.0,
            high: 0.0,
        };
        let inst = instance_with("BTC", Some(opp), Some(MarketBias::Bullish), "READY", 80.0);
        let panel = build_overview_panel(&[inst], &HashMap::new());
        let row = &panel.rows[0];
        assert_eq!(row.setup_side, "NEUTRAL");
        assert_eq!(row.entry_low, 0.0);
        assert_eq!(row.invalidation, 0.0);
    }
}
