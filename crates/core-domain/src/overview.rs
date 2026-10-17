//! # Overview Matrix — Market Synthesis Layer
//!
//! The Overview Matrix aggregates all Advisory Matrices and instance metadata
//! to provide a unified representation of the observed market environment.
//! It summarizes the collective state of all analyzed assets.
//!
//! Layer: L7 in the architecture (Overview).

use crate::advisory::{AdvisoryMatrix, DirectionalGuidance, StrategyEnvironment};
use crate::alignment::AlignmentMatrix;
use crate::risk::{RiskDimension, RiskLevel};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global market bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GlobalBias {
    StrongBullish,
    Bullish,
    Neutral,
    Bearish,
    StrongBearish,
    Mixed,
}

impl std::fmt::Display for GlobalBias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobalBias::StrongBullish => write!(f, "STRONG_BULLISH"),
            GlobalBias::Bullish => write!(f, "BULLISH"),
            GlobalBias::Neutral => write!(f, "NEUTRAL"),
            GlobalBias::Bearish => write!(f, "BEARISH"),
            GlobalBias::StrongBearish => write!(f, "STRONG_BEARISH"),
            GlobalBias::Mixed => write!(f, "MIXED"),
        }
    }
}

/// Market breadth level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketBreadth {
    VeryWeak,
    Weak,
    Balanced,
    Positive,
    StrongPositive,
    Negative,
    StrongNegative,
}

/// Market synchronization level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncLevel {
    HighlySynchronized,
    Synchronized,
    Mixed,
    Fragmented,
    HighlyFragmented,
}

/// Market health level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthLevel {
    Poor,
    Weak,
    Neutral,
    Healthy,
    Strong,
}

/// Per-asset ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRank {
    pub symbol: String,
    pub score: f64,
    pub bias: String,
    pub confidence: f64,
    pub regime: String,
    pub risk_level: String,
    /// `AlignmentMatrix.mtf_overall_score` for this symbol ∈ [-100, 100].
    /// `0.0` when no alignment is available for the symbol.
    #[serde(default)]
    pub mtf_score: f64,
    /// `AlignmentMatrix.mtf_overall_label` for this symbol
    /// (`STRONG_BULL_MTF` / `WEAK_BULL_MTF` / `NEUTRAL_MTF` /
    /// `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`).
    #[serde(default)]
    pub mtf_label: String,
}

/// Risk distribution summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskDistribution {
    pub low_pct: f64,
    pub moderate_pct: f64,
    pub high_pct: f64,
    pub risk_environment: String,
}

/// Overview Matrix — global market synthesis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewMatrix {
    pub global_market_bias: GlobalBias,
    pub market_breadth: MarketBreadth,
    pub regime_distribution: HashMap<String, f64>,
    pub opportunity_distribution: HashMap<String, u32>,
    pub risk_distribution: RiskDistribution,
    pub asset_ranking: Vec<AssetRank>,
    pub market_synchronization: SyncLevel,
    pub market_health: HealthLevel,
    /// Cross-symbol aggregate cascade risk (mean of all per-symbol
    /// cascade_risk scores, 0..100).
    #[serde(default)]
    pub cascade_risk_index: RiskDimension,
    /// Continuous signed breadth percentage ∈ [-100, 100].
    #[serde(default)]
    pub breadth_pct: f64,
    /// True when fewer than 4 of 12 SignalKinds are enabled.
    #[serde(default)]
    pub low_coverage: bool,
    /// Count of assets per `AlignmentMatrix.mtf_overall_label`
    /// (`STRONG_BULL_MTF`, `WEAK_BULL_MTF`, `NEUTRAL_MTF`,
    /// `WEAK_BEAR_MTF`, `STRONG_BEAR_MTF`, `NO_DATA`). `u32` because
    /// an asset can satisfy at most one label. Mirrors the existing
    /// `opportunity_distribution` shape — both are per-type counts,
    /// not partitions, so an asset counts toward exactly one entry.
    #[serde(default)]
    pub alignment_distribution: HashMap<String, u32>,
    /// Mean of all per-symbol `AlignmentMatrix.mtf_overall_score` ∈
    /// `[-100, 100]`. Cross-timeframe counterpart to `breadth_pct`
    /// (which is cross-symbol). `0.0` when no alignments are
    /// available.
    #[serde(default)]
    pub alignment_consensus_index: f64,
    /// Mean of all per-symbol
    /// `AlignmentMatrix.trend_agreement_pct` ∈ `[0, 100]`. Answers
    /// "how well do timeframes within each symbol agree?". Distinct
    /// from `market_synchronization` (which is cross-symbol,
    /// derived from `breadth_pct`). `0.0` when no alignments are
    /// available.
    #[serde(default)]
    pub multi_tf_agreement_pct: f64,
    pub global_summary: String,
    pub instance_count: u32,
    pub active_symbols: Vec<String>,
    /// Per-instance asset-ranking rows (symbol, price, bias, rank score,
    /// cross-duration agreement, updated, active) — the GUI's table and the
    /// observe shell read the same rows.
    ///
    /// CR-15: the row once carried `signal`, `direction`, `rr`, `confidence`,
    /// `risk`, `setup_side` and the entry/target/invalidation geometry. All of
    /// it was strategy and none of it is renderable in observe mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overview_rows: Vec<crate::overview_panel::OverviewRow>,
    /// v7.2 parity: market-health sub-dimension bars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_health_dims: Option<crate::overview_panel::MarketHealthDims>,
}

impl OverviewMatrix {
    pub fn empty() -> Self {
        Self {
            global_market_bias: GlobalBias::Neutral,
            market_breadth: MarketBreadth::Balanced,
            regime_distribution: HashMap::new(),
            opportunity_distribution: HashMap::new(),
            risk_distribution: RiskDistribution {
                low_pct: 0.0,
                moderate_pct: 100.0,
                high_pct: 0.0,
                risk_environment: "NO_DATA".into(),
            },
            asset_ranking: Vec::new(),
            market_synchronization: SyncLevel::HighlyFragmented,
            market_health: HealthLevel::Neutral,
            cascade_risk_index: RiskDimension::default(),
            breadth_pct: 0.0,
            low_coverage: false,
            alignment_distribution: HashMap::new(),
            alignment_consensus_index: 0.0,
            multi_tf_agreement_pct: 0.0,
            global_summary: "No active instances — no market data available.".into(),
            instance_count: 0,
            active_symbols: Vec::new(),
            overview_rows: Vec::new(),
            market_health_dims: None,
        }
    }
}

/// A minimal instance metadata record.
#[derive(Debug, Clone)]
pub struct InstanceMeta {
    pub symbol: String,
    pub timeframe_secs: u64,
    pub timeframe_label: String,
    pub is_active: bool,
    /// CR-15: per-symbol `RiskMatrix.overall_risk.score`, the aggregate the L7
    /// risk distribution / risk_environment bin on.
    ///
    /// The L5 matrix is strategy and no longer exists, so this is `None` for
    /// every rank until P10 derives the axis-based substitute. It is
    /// deliberately `Option`: a fabricated `0.0` would read as "no danger
    /// measured", which is a claim nothing supports.
    pub overall_risk: Option<f64>,
    /// v6.10.19 (P7): per-TF-window `(weight, risk)` pairs (micro 0.1 /
    /// fast 0.2 / slow 0.3 / macro 0.4) feeding the TF-decayed
    /// `high_pct` — a transient MICRO risk
    /// spike contributes at most 10% to the systemic path, so the PME
    /// safety veto stays anchored to macro stability. Empty for legacy
    /// callers → the systemic path falls back to `overall_risk`.
    pub risk_windows: Vec<(f64, f64)>,
}

/// Compute the Overview Matrix from all Advisory Matrices, instance
/// metadata, and per-symbol Alignment Matrices. The Alignment
/// Matrices are optional (empty slice is permitted); when omitted,
/// the three new aggregate fields (`alignment_distribution`,
/// `alignment_consensus_index`, `multi_tf_agreement_pct`) default to
/// neutral / empty values while the existing breadth / sync / health
/// aggregates remain populated from the advisories.
/// v9: the L7 runtime parameters — the strategy's `l7` section. Every
/// default reproduces the pre-v9 bands, shares, weights, and thresholds.
#[derive(Debug, Clone, PartialEq)]
pub struct OverviewParams {
    pub breadth_strong: f64,
    pub breadth_positive: f64,
    pub breadth_balanced: f64,
    pub global_bias_strong_share: f64,
    pub global_bias_plain_share: f64,
    pub sync_bands: [f64; 4],
    pub risk_low_max: f64,
    pub risk_high_min: f64,
    pub env_mean_high: f64,
    pub env_mean_moderate: f64,
    pub systemic_high_weight: f64,
    pub systemic_sync_weight: f64,
    pub sync_penalty: [f64; 5],
    /// v11.9: per-duration systemic decay weights, aligned with
    /// `core_domain::SUPPORTED_DURATIONS` (fastest → slowest).
    pub tf_decay: [f64; 14],
    pub cascade_index_fallback: f64,
    pub entry_veto_threshold: f64,
    pub asset_rank_slope: f64,
    pub asset_rank_offset: f64,
    pub low_coverage_min_symbols: u32,
    pub alignment_buckets: [f64; 2],
}

impl Default for OverviewParams {
    fn default() -> Self {
        Self {
            breadth_strong: 60.0,
            breadth_positive: 20.0,
            breadth_balanced: 10.0,
            global_bias_strong_share: 0.8,
            global_bias_plain_share: 0.6,
            sync_bands: [75.0, 50.0, 25.0, 10.0],
            risk_low_max: 30.0,
            risk_high_min: 70.0,
            env_mean_high: 50.0,
            env_mean_moderate: 25.0,
            systemic_high_weight: 0.6,
            systemic_sync_weight: 0.4,
            sync_penalty: [100.0, 60.0, 30.0, 10.0, 0.0],
            tf_decay: [
                0.04, 0.04, 0.04, 0.07, 0.07, 0.10, 0.10, 0.10, 0.08, 0.07, 0.08, 0.07, 0.07, 0.07,
            ],
            cascade_index_fallback: 50.0,
            entry_veto_threshold: 80.0,
            asset_rank_slope: 0.5,
            asset_rank_offset: 50.0,
            low_coverage_min_symbols: 3,
            alignment_buckets: [75.0, 50.0],
        }
    }
}

pub fn compute_overview(
    advisories: &[AdvisoryMatrix],
    instances: &[InstanceMeta],
    alignments: &[AlignmentMatrix],
    params: &OverviewParams,
) -> OverviewMatrix {
    if advisories.is_empty() && instances.iter().all(|i| !i.is_active) {
        return OverviewMatrix::empty();
    }

    let active_instances: Vec<&InstanceMeta> = instances.iter().filter(|i| i.is_active).collect();
    let instance_count = active_instances.len() as u32;

    // Per-symbol alignment lookup. Built once so the per-asset
    // AssetRank enrichment below is O(n) rather than O(n²).
    let alignments_by_symbol: HashMap<&str, &AlignmentMatrix> =
        alignments.iter().map(|a| (a.symbol.as_str(), a)).collect();

    let mut symbols_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for inst in &active_instances {
        symbols_set.insert(inst.symbol.clone());
    }
    for a in advisories {
        symbols_set.insert(a.symbol.clone());
    }
    let mut active_symbols: Vec<String> = symbols_set.into_iter().collect();
    active_symbols.sort();

    let mut long_count = 0u32;
    let mut short_count = 0u32;
    let mut neutral_count = 0u32;
    for adv in advisories {
        match adv.directional_guidance {
            DirectionalGuidance::StrongLong | DirectionalGuidance::Long => long_count += 1,
            DirectionalGuidance::StrongShort | DirectionalGuidance::Short => short_count += 1,
            _ => neutral_count += 1,
        }
    }
    // v6.10 (Phase 6 / F4): `breadth_pct` formula is the canonical spec
    // calculation `(L - S) / (L + S + N) × 100` per
    // `docs/engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md` line 30
    // and `docs/matrices/02-09-overview-matrix.md` line 104. The
    // previous v6.9 implementation used the advance-decline formula
    // `(L - S) / (L + S)` which excluded neutrals from the
    // denominator, producing non-canonical values for mixed-mood
    // markets. The corrected formula includes neutrals in the
    // denominator so that a 50L/0S/50N mix yields +50% (vs the
    // v6.9 result of +100%).
    let total = (long_count + short_count + neutral_count).max(1) as f64;

    // Market breadth
    let breadth_pct = (long_count as f64 - short_count as f64) / total * 100.0;
    let breadth = if breadth_pct > params.breadth_strong {
        MarketBreadth::StrongPositive
    } else if breadth_pct > params.breadth_positive {
        MarketBreadth::Positive
    } else if breadth_pct < -params.breadth_strong {
        MarketBreadth::StrongNegative
    } else if breadth_pct < -params.breadth_positive {
        MarketBreadth::Negative
    } else if breadth_pct.abs() < params.breadth_balanced {
        MarketBreadth::Balanced
    } else if breadth_pct > 0.0 {
        MarketBreadth::Weak
    } else {
        MarketBreadth::VeryWeak
    };

    // Synchronization
    let [s0, s1, s2, s3] = params.sync_bands;
    let sync = if breadth_pct.abs() > s0 {
        SyncLevel::HighlySynchronized
    } else if breadth_pct.abs() > s1 {
        SyncLevel::Synchronized
    } else if breadth_pct.abs() > s2 {
        SyncLevel::Mixed
    } else if breadth_pct.abs() > s3 {
        SyncLevel::Fragmented
    } else {
        SyncLevel::HighlyFragmented
    };

    // Global bias: priority-ordered per spec §3.1
    let long_pct = long_count as f64 / total;
    let short_pct = short_count as f64 / total;
    let is_synced = matches!(
        sync,
        SyncLevel::HighlySynchronized | SyncLevel::Synchronized
    );

    // Bug-fix #14: removed the `neutral_pct >= 0.6 → GlobalBias::Neutral`
    // branch. It was semantically dead: a 60%+ neutral market is, by
    // construction, also a 0% directional market, and the resulting
    // `Neutral` bias was indistinguishable from a 100% neutral market
    // (which falls through to `Mixed` after the `long_count >
    // short_count` / `short_count > long_count` checks both fail).
    // Removing the branch makes the priority chain consistent: any
    // market where no direction has a clear majority defaults to
    // `Mixed`, and the empty-input case is handled by the early
    // `OverviewMatrix::empty()` return at the top of the function.
    let global_bias = if long_pct >= params.global_bias_strong_share && is_synced {
        GlobalBias::StrongBullish
    } else if short_pct >= params.global_bias_strong_share && is_synced {
        GlobalBias::StrongBearish
    } else if long_pct >= params.global_bias_plain_share {
        GlobalBias::Bullish
    } else if short_pct >= params.global_bias_plain_share {
        GlobalBias::Bearish
    } else if long_count > short_count {
        GlobalBias::Bullish
    } else if short_count > long_count {
        GlobalBias::Bearish
    } else {
        GlobalBias::Mixed
    };

    // v6.10.16 (FIX-O3): per-symbol L5 OVERALL risk map, hoisted above the
    // rankings so AssetRank.risk_level and the risk distribution bin on the
    // SAME canonical risk data. Bands: ≤30 low, ≥70 high, else moderate —
    // missing symbols fall back to 50 (moderate), the same default the
    // dashboard card uses.
    let overall_by_symbol: HashMap<&str, f64> = active_instances
        .iter()
        .filter_map(|i| i.overall_risk.map(|r| (i.symbol.as_str(), r)))
        .collect::<HashMap<&str, f64>>();
    // CR-15: an unmeasured risk must not be reported as a risk level. This
    // closure used to default a missing risk to 50.0, which lands in the
    // "MODERATE" band — so an instance that was never scored rendered as a
    // measured moderate risk. `NO_DATA` is the honest reading; P10 replaces
    // this with the axis-derived substitute.
    let overall_for = |symbol: &str| overall_by_symbol.get(symbol).copied();
    let risk_level_for = |symbol: &str| match overall_for(symbol) {
        None => "NO_DATA",
        Some(r) if r <= params.risk_low_max => "LOW",
        Some(r) if r >= params.risk_high_min => "HIGH",
        Some(_) => "MODERATE",
    };

    // Asset ranking per spec §3.
    //
    // CR-15: this table used to be keyed on `by_symbol`, a BTreeMap built from
    // the advisories, and its `score` was `0.5 * mean_confidence + 50.0` over
    // each symbol's per-duration advisory windows. With no advisory there would
    // be no rows at all — the Asset Rankings card would render empty in observe
    // mode, which is the opposite of narrowing the surface.
    //
    // The rows are now keyed on the ACTIVE INSTANCES, which is what the card
    // actually displays, and the model supplies the columns: the cross-duration
    // alignment is mirrored onto the rank as before. `score`, `bias`,
    // `confidence` and `regime` are the advisory-derived columns and are P10
    // work — until the four typed axes exist there is nothing honest to put in
    // them, so they report `NO_DATA` rather than a fabricated zero. The sort is
    // by cross-duration score, which IS a measurement, and that is the order the
    // dashboard's default (unsorted) state wants.
    let mut rankings: Vec<AssetRank> = active_instances
        .iter()
        .map(|i| {
            let symbol = i.symbol.as_str();
            // Mirror the per-symbol AlignmentMatrix onto the AssetRank so REST /
            // export consumers can show multi-timeframe alignment per asset
            // without a second fetch. When no alignment is available for this
            // symbol (cold start, transient snapshot gap, or symbol not in the
            // alignment slice), default to neutral values.
            let (mtf_score, mtf_label) = match alignments_by_symbol.get(symbol) {
                Some(aln) => (aln.mtf_overall_score, aln.mtf_overall_label.clone()),
                None => (0.0, "NO_DATA".to_string()),
            };
            AssetRank {
                symbol: symbol.to_string(),
                // Cross-duration agreement is the one measured ranking signal
                // that survives the boundary; it maps [-100, 100] onto [0, 100]
                // so the column keeps its documented range.
                score: (mtf_score + 100.0) / 2.0,
                bias: "NO_DATA".to_string(),
                // Nothing was measured, so the confidence that would qualify this
                // placeholder is 0.0 — a consumer can see it is not a reading.
                confidence: 0.0,
                regime: "NO_DATA".to_string(),
                risk_level: risk_level_for(symbol).to_string(),
                mtf_score,
                mtf_label,
            }
        })
        .collect();
    rankings.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Regime distribution
    let mut regime_counts: HashMap<String, f64> = HashMap::new();
    for adv in advisories {
        let regime_key = match adv.strategy_environment {
            StrategyEnvironment::TrendFollowing => "TRENDING",
            StrategyEnvironment::Breakout => "EXPANSION",
            StrategyEnvironment::MeanReversion => "RANGE",
            StrategyEnvironment::HighVolatility => "HIGH_VOLATILITY",
            StrategyEnvironment::LowActivity => "LOW_ACTIVITY",
            StrategyEnvironment::Unfavorable => "UNFAVORABLE",
        };
        *regime_counts.entry(regime_key.to_string()).or_insert(0.0) += 1.0;
    }
    let mut regime_distribution: HashMap<String, f64> = HashMap::new();
    for (regime, count) in &regime_counts {
        regime_distribution.insert(regime.clone(), count / total);
    }

    // CR-15: the opportunity distribution came from `advisory.opportunity_classification`.
    // The advisory no longer exists, so there is nothing to bin and the map is
    // empty by construction — it is kept as a field so the wire shape does not
    // churn, and it will be derived from the axes in P10.
    let opportunity_distribution: HashMap<String, u32> = HashMap::new();

    // Risk distribution. L7-A (v6.10.13): bins per-asset L5 OVERALL risk
    // (`overall_risk.score` from the active instances) — the canonical
    // aggregate the dashboard's RiskDistributionCard uses. The previous
    // implementation binned on `advisory.cascade_risk_score` (chosen only
    // because it was the single risk scalar the signature carried), which
    // made the L7 export disagree with the dashboard card for the same
    // labelled split; the 02-09 doc's confidence-based definition was
    // conceptually wrong (confidence ≠ risk). Bands: ≤30 low, ≥70 high,
    // else moderate — missing symbols fall back to 50 (moderate), the
    // same default the dashboard card uses. `overall_by_symbol` /
    // `overall_for` are hoisted above the rankings (FIX-O3, v6.10.16) and
    // shared with `AssetRank.risk_level`.
    // CR-15: `advisories` is empty — MME no longer produces an advisory — so both
    // risk percentages below are 0.0 by construction. That is "not measured",
    // not "no danger". The PME safety veto reads `systemic_high_pct` and
    // therefore fails OPEN in this build: with no risk signal it does not veto.
    // Failing CLOSED instead would be the safer default, so this is recorded as
    // a known consequence of CR-15 rather than left to be discovered, and paper
    // and live remain WIP and not selectable.
    // CR-15: these two bins iterated `advisories`, so with the advisory gone
    // they counted zero rows and divided by 1 — permanently 0.0% HIGH and
    // 100.0% MODERATE. They now bin the ACTIVE instances that carry a measured
    // risk. An instance with no measurement is excluded from BOTH the numerator
    // and the denominator: it is not "low risk", it is unscored, and counting it
    // as cleared is the failure mode CR-15 is removing.
    let measured: Vec<f64> = active_instances
        .iter()
        .filter_map(|i| i.overall_risk)
        .collect();
    let total_measured = measured.len().max(1) as f64;
    let low_risk = measured
        .iter()
        .filter(|r| **r <= params.risk_low_max)
        .count() as f64
        / total_measured
        * 100.0;
    // v6.10.19 (P7): the DESCRIPTIVE `high_pct` (risk distribution card)
    // stays on the plain per-symbol mean — the dashboard must keep parity
    // with the panels. The SYSTEMIC path below uses the TF-decayed count.
    let high_pct = measured
        .iter()
        .filter(|r| **r >= params.risk_high_min)
        .count() as f64
        / total_measured
        * 100.0;

    // CR-18: the TF-decayed systemic high-share is DELETED with
    // `systemic_risk_score`. It existed only to be `0.4`-weighted against a
    // `sync_penalty` array of hand-picked values, and its "per-duration decay"
    // was a 14-element constant table (`tf_decay`) with no derivation from
    // candles — a tuned weight dressed as a measurement. It is now dead code and
    // is removed rather than left to rot behind an unused binding.

    // risk_environment per spec §2.3 (v6.10.16 FIX-O3): bins on the MEAN
    // per-asset overall risk — an environment where every asset sits in
    // "moderate" must read MODERATE_RISK, never LOW_RISK. NO_DATA →
    // HIGH_RISK (mean ≥50) → MODERATE (mean ≥25) → LOW_RISK.
    // v6.10.18 (I-2): the mean is over DISTINCT symbols (repeated windows
    // must not inflate the denominator).
    // v6.10.19a: each symbol's canonical risk is the MICRO-window L5 score
    // (the operator-visible number) — the daemon feeds it via
    // `InstanceMeta.overall_risk`; the earlier TF-window mean made
    // HIGH_RISK environments appear next to a visible avg-risk of 41–46.
    let measured_risks: Vec<f64> = active_instances
        .iter()
        .filter_map(|i| i.overall_risk)
        .collect();
    let mean_overall_risk = if measured_risks.is_empty() {
        0.0
    } else {
        measured_risks.iter().sum::<f64>() / measured_risks.len() as f64
    };
    let risk_environment = if instance_count == 0 {
        "NO_DATA"
    } else if measured_risks.is_empty() {
        // CR-15: instances exist but none carries a measured risk (the L5 matrix
        // is gone). "NO_DATA" is the honest environment — "LOW_RISK" would read
        // as a measurement of safety that nothing supports.
        "NO_DATA"
    } else if mean_overall_risk >= params.env_mean_high {
        "HIGH_RISK"
    } else if mean_overall_risk >= params.env_mean_moderate {
        "MODERATE"
    } else {
        "LOW_RISK"
    };

    // CR-18: `systemic_risk_score` is DELETED. It was an MME number whose only
    // consumer was the PME safety veto — a layer observe mode never reaches — and
    // its formula was `0.6 * high_pct + 0.4 * sync_penalty`, i.e. two tuned
    // constants over a re-bucketed breadth number. MME should not compute a number
    // for an engine that cannot see it. See `01-22` §"The redundancy rule".

    // market_health per spec §3.4: POOR when HIGH_RISK, then bias-based
    let health = if risk_environment == "HIGH_RISK" {
        HealthLevel::Poor
    } else {
        match global_bias {
            GlobalBias::StrongBullish | GlobalBias::StrongBearish => HealthLevel::Strong,
            GlobalBias::Bullish | GlobalBias::Bearish => HealthLevel::Healthy,
            GlobalBias::Mixed => HealthLevel::Weak,
            _ => HealthLevel::Neutral,
        }
    };

    // cascade_risk_index: mean of per-symbol cascade risk scores from
    // AdvisoryMatrix.
    //
    // CR-15: the advisory is gone, so there is no cascade score to average and
    // the index is the configured fallback. The dimension is reported with
    // `confidence: 0.0` — "nothing was measured" — which is the whole point of
    // carrying a confidence: a consumer can see that this number is a placeholder
    // rather than a reading. P10 replaces it with the axis-derived substitute.
    let cascade_score = params.cascade_index_fallback;
    let cascade_risk = RiskDimension {
        score: cascade_score,
        level: if cascade_score >= 80.0 {
            RiskLevel::Extreme
        } else if cascade_score >= 60.0 {
            RiskLevel::High
        } else if cascade_score >= 40.0 {
            RiskLevel::Moderate
        } else if cascade_score >= 20.0 {
            RiskLevel::Low
        } else {
            RiskLevel::VeryLow
        },
        // L7-B (v6.10.13): the fraction must scale to 0-100 (`×100`) —
        // the legacy `count / total.min(1)` yielded ≈1% for every sample.
        // Nothing was measured, so the confidence that would qualify this
        // placeholder is 0.0 — a consumer can see it is not a reading.
        confidence: 0.0,
        ..RiskDimension::default()
    };

    // ── Alignment aggregation ─────────────────────────────────
    // `alignment_distribution` — count of assets per
    // `mtf_overall_label`. Mirrors `opportunity_distribution` shape
    // (HashMap<String, u32>); unlike `regime_distribution` (which is
    // a partition that sums to 1.0), an asset satisfies exactly
    // one label, so this is a count, not a fraction.
    let mut alignment_distribution: HashMap<String, u32> = HashMap::new();
    let mut alignment_score_sum = 0.0_f64;
    let mut trend_agreement_sum = 0.0_f64;
    let alignment_count = alignments.len();
    for aln in alignments {
        *alignment_distribution
            .entry(aln.mtf_overall_label.clone())
            .or_insert(0) += 1;
        alignment_score_sum += aln.mtf_overall_score;
        trend_agreement_sum += aln.trend_agreement_pct;
    }
    let (alignment_consensus_index, multi_tf_agreement_pct) = if alignment_count > 0 {
        let n = alignment_count as f64;
        (alignment_score_sum / n, trend_agreement_sum / n)
    } else {
        (0.0, 0.0)
    };

    let summary = format!(
        "{} active instances across {} symbols. Global bias: {} with {} market breadth. Risk environment: {}.",
        instance_count,
        active_symbols.len(),
        global_bias,
        match breadth {
            MarketBreadth::StrongPositive | MarketBreadth::Positive => "positive",
            MarketBreadth::StrongNegative | MarketBreadth::Negative => "negative",
            _ => "balanced",
        },
        risk_environment
    );

    OverviewMatrix {
        global_market_bias: global_bias,
        market_breadth: breadth,
        regime_distribution,
        opportunity_distribution,
        risk_distribution: RiskDistribution {
            low_pct: low_risk.round(),
            moderate_pct: (100.0 - low_risk - high_pct).max(0.0).round(),
            high_pct: high_pct.round(),
            risk_environment: risk_environment.to_string(),
        },
        asset_ranking: rankings,
        market_synchronization: sync,
        market_health: health,
        cascade_risk_index: cascade_risk,
        breadth_pct,
        // Bug-fix #15: `low_coverage` was hardcoded to `false` on every
        // call, so the dashboard never knew when the overview was
        // computed on a thin sample. The frontend used the flag to
        // dim global aggregates that are statistically unreliable
        // below 3 active symbols. The threshold of 3 matches the
        // §3.5 "minimum coverage for breadth / sync / health
        // aggregation" rule in the L7 spec; below this, market_breadth
        // is just a count of 1-2 advisories and market_synchronization
        // is undefined for n < 3.
        low_coverage: active_symbols.len() < 3,
        global_summary: summary,
        instance_count,
        active_symbols,
        alignment_distribution,
        alignment_consensus_index,
        multi_tf_agreement_pct,
        // v7.2 parity: the L7 task merges the overview-panel payload
        // (hero / rows / signal quality / directions / health bars)
        // after `compute_overview` returns — empty here by default.
        overview_rows: Vec::new(),
        market_health_dims: None,
    }
}

#[cfg(test)]
mod tests {
    //! L7 overview aggregation tests, re-pointed by CR-15.
    //!
    //! Most of this suite used to build `AdvisoryMatrix` fixtures and assert on
    //! the L4 opportunity bins, the L5 risk distribution and the L6 systemic-risk
    //! aggregates. The advisory no longer exists, so none of those fixtures could
    //! survive the boundary.
    //!
    //! The aggregates are **kept as fields** rather than deleted, and this suite
    //! now pins the CR-15 contract for them: with no advisory input they report
    //! *unmeasured* — an empty distribution, `NO_DATA`, zero scores — and they
    //! never invent a value. P10 re-derives them from the four typed axes, at
    //! which point these tests are replaced by axis-derived expectations.
    //!
    //! The alignment-derived aggregates, which were never strategy, keep their
    //! coverage.

    use super::*;

    fn meta(symbol: &str, active: bool, risk: Option<f64>) -> InstanceMeta {
        InstanceMeta {
            symbol: symbol.to_string(),
            timeframe_secs: 60,
            timeframe_label: "1m".to_string(),
            is_active: active,
            overall_risk: risk,
            risk_windows: vec![],
        }
    }

    fn align(symbol: &str, score: f64, label: &str) -> AlignmentMatrix {
        let mut a = AlignmentMatrix::empty(symbol);
        a.mtf_overall_score = score;
        a.mtf_overall_label = label.to_string();
        a
    }

    fn rank<'a>(o: &'a OverviewMatrix, symbol: &str) -> &'a AssetRank {
        o.asset_ranking
            .iter()
            .find(|r| r.symbol == symbol)
            .unwrap_or_else(|| panic!("no rank for {symbol}"))
    }

    // ── CR-15: unmeasured, never invented ────────────────────────────

    #[test]
    fn no_advisory_yields_no_opportunity_distribution() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, Some(20.0))],
            &[],
            &OverviewParams::default(),
        );
        assert!(
            o.opportunity_distribution.is_empty(),
            "CR-15: the distribution bins `advisory.opportunity_classification`, which \
             does not exist — it must stay empty rather than fall back to a fake bin"
        );
    }

    #[test]
    fn an_unmeasured_risk_yields_an_unmeasured_risk_distribution() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None)], // never scored: the L5 matrix is gone
            &[],
            &OverviewParams::default(),
        );
        assert_eq!(
            o.risk_distribution.high_pct, 0.0,
            "CR-15: the risk bins counted advisories. Nothing is measured here, so \
             the distribution must be empty of readings rather than report 0% HIGH \
             as if the instance had been checked and cleared."
        );
        assert_eq!(
            o.risk_distribution.risk_environment, "NO_DATA",
            "an instance that was never scored must not put the market in a risk band"
        );
    }

    #[test]
    fn systemic_risk_is_zero_without_a_measurement() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None)],
            &[],
            &OverviewParams::default(),
        );
        // CR-18: there is no `systemic_risk_score` to assert any more. It was
        // deleted because it was an MME number computed solely for the PME safety
        // veto — a layer observe mode never reaches — from two tuned constants
        // over a re-bucketed breadth figure.
    }

    #[test]
    fn an_unmeasured_instance_does_not_dilute_the_systemic_percentage() {
        // The regression: the denominator counted every active instance, even
        // ones with no measurement. One measured HIGH instance beside one
        // unmeasured instance must read 100% high, not 50%.
        let measured = compute_overview(
            &[],
            &[meta("HIGH", true, Some(90.0))],
            &[],
            &OverviewParams::default(),
        );
        let diluted = compute_overview(
            &[],
            &[meta("HIGH", true, Some(90.0)), meta("UNSCORED", true, None)],
            &[],
            &OverviewParams::default(),
        );
        assert_eq!(measured.risk_distribution.high_pct, 100.0);
        assert_eq!(
            diluted.risk_distribution.high_pct, 100.0,
            "an unmeasured instance must not halve the systemic risk reading"
        );
    }

    #[test]
    fn an_unmeasured_risk_reports_none_on_the_rank() {
        // `overall_risk` became `Option<f64>` in this phase. The old code
        // defaulted a missing risk to 50.0 ("moderate"), which renders as a real
        // measurement of moderate risk for an instance that was never scored.
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None)],
            &[],
            &OverviewParams::default(),
        );
        assert_eq!(
            rank(&o, "BTC-USD").risk_level,
            "NO_DATA",
            "an unmeasured risk must not read as a risk level"
        );
    }

    #[test]
    fn a_measured_risk_still_bins() {
        // The binning itself is untouched — only the missing-value handling moved.
        let o = compute_overview(
            &[],
            &[
                meta("LOW", true, Some(10.0)),
                meta("HIGH", true, Some(90.0)),
            ],
            &[],
            &OverviewParams::default(),
        );
        assert_eq!(rank(&o, "LOW").risk_level, "LOW");
        assert_eq!(rank(&o, "HIGH").risk_level, "HIGH");
    }

    #[test]
    fn an_inactive_instance_is_not_ranked_as_active() {
        // `compute_overview` builds its aggregate set from the ACTIVE instances,
        // so a stopped instance contributes no risk and no breadth.
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", false, Some(95.0))],
            &[],
            &OverviewParams::default(),
        );
        assert!(
            o.risk_distribution.high_pct == 0.0,
            "an inactive instance must not contribute to the risk distribution"
        );
    }

    // ── alignment: model state, unchanged coverage ────────────────────

    #[test]
    fn alignment_distribution_counts_per_label() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None), meta("ETH-USD", true, None)],
            &[align("BTC-USD", 70.0, "STRONG_BULL_MTF")],
            &OverviewParams::default(),
        );
        assert_eq!(o.alignment_distribution.get("STRONG_BULL_MTF"), Some(&1));
    }

    #[test]
    fn alignment_consensus_index_is_mean_of_scores() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None), meta("ETH-USD", true, None)],
            &[
                align("BTC-USD", 70.0, "STRONG_BULL_MTF"),
                align("ETH-USD", 50.0, "NEUTRAL_MTF"),
            ],
            &OverviewParams::default(),
        );
        assert_eq!(o.alignment_consensus_index, 60.0);
    }

    #[test]
    fn asset_rank_mirrors_alignment_when_present() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None)],
            &[align("BTC-USD", 70.0, "STRONG_BULL_MTF")],
            &OverviewParams::default(),
        );
        assert_eq!(rank(&o, "BTC-USD").mtf_score, 70.0);
        assert_eq!(rank(&o, "BTC-USD").mtf_label, "STRONG_BULL_MTF");
    }

    #[test]
    fn asset_rank_defaults_when_alignment_missing() {
        let o = compute_overview(
            &[],
            &[meta("BTC-USD", true, None)],
            &[],
            &OverviewParams::default(),
        );
        assert_eq!(rank(&o, "BTC-USD").mtf_score, 0.0);
        assert_eq!(rank(&o, "BTC-USD").mtf_label, "NO_DATA");
    }
}
