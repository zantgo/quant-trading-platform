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
    /// Market-wide danger index consumed by the PME safety veto.
    /// Formula: `0.6 * high_pct + 0.4 * sync_penalty`.
    #[serde(default)]
    pub systemic_risk_score: f64,
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
    /// v7.2 parity: server-computed hero verdict (TRADE / WAIT /
    /// STAND_ASIDE) — single source for GUI + CLI overview panels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hero: Option<crate::overview_panel::OverviewHero>,
    /// v7.2 parity: per-instance asset-ranking rows (price, signal,
    /// direction, R:R, confidence, MTF, risk, updated) — the GUI's
    /// 12-column table and the CLI renderer read the same rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overview_rows: Vec<crate::overview_panel::OverviewRow>,
    /// v7.2 parity: signal-quality buckets (strong/moderate/weak).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_quality: Option<crate::overview_panel::SignalQuality>,
    /// v7.2 parity: direction counts (long/short/neutral).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_distribution: Option<crate::overview_panel::DirectionDistribution>,
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
            systemic_risk_score: 0.0,
            breadth_pct: 0.0,
            low_coverage: false,
            alignment_distribution: HashMap::new(),
            alignment_consensus_index: 0.0,
            multi_tf_agreement_pct: 0.0,
            global_summary: "No active instances — no market data available.".into(),
            instance_count: 0,
            active_symbols: Vec::new(),
            hero: None,
            overview_rows: Vec::new(),
            signal_quality: None,
            direction_distribution: None,
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
    /// Per-symbol L5 `overall_risk.score` — the canonical aggregate the L7
    /// risk distribution / risk_environment / systemic_risk_score bin on
    /// (L7-A, v6.10.13). v6.10.18: the MEAN over the TF windows.
    pub overall_risk: f64,
    /// v6.10.19 (P7): per-TF-window `(weight, risk)` pairs (micro 0.1 /
    /// fast 0.2 / slow 0.3 / macro 0.4) feeding the TF-decayed
    /// `high_pct` and `systemic_risk_score` — a transient MICRO risk
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
    pub tf_decay: [f64; 10],
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
            tf_decay: [0.05, 0.05, 0.05, 0.1, 0.1, 0.15, 0.15, 0.15, 0.1, 0.1],
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
        .map(|i| (i.symbol.as_str(), i.overall_risk))
        .collect();
    let overall_for = |symbol: &str| overall_by_symbol.get(symbol).copied().unwrap_or(50.0);
    let risk_level_for = |symbol: &str| {
        let r = overall_for(symbol);
        if r <= params.risk_low_max {
            "LOW"
        } else if r >= params.risk_high_min {
            "HIGH"
        } else {
            "MODERATE"
        }
    };

    // Asset ranking per spec §3: score = 0.5 * confidence_assessment + 50.0.
    // v6.10.18 (I-2): advisories arrive per TF WINDOW (0–4 per symbol from
    // the daemon's tf-average basis) — rankings aggregate per symbol: the
    // confidence is the MEAN over the symbol's windows, and the categorical
    // bias/regime take the MODE (ties resolve to the fastest window, i.e.
    // the first in iteration order).
    let mut by_symbol: std::collections::BTreeMap<&str, Vec<&AdvisoryMatrix>> =
        std::collections::BTreeMap::new();
    for adv in advisories {
        by_symbol.entry(adv.symbol.as_str()).or_default().push(adv);
    }
    let mode_of = |wins: &[&AdvisoryMatrix], f: &dyn Fn(&AdvisoryMatrix) -> String| -> String {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut best: Option<String> = None;
        for w in wins {
            let key = f(w);
            let cur = counts.get(&key).copied().unwrap_or(0) + 1;
            counts.insert(key.clone(), cur);
            // Strict `>` keeps the FIRST (fastest) window on ties.
            if best.as_ref().map_or(true, |bk| cur > counts[bk]) {
                best = Some(key);
            }
        }
        best.unwrap_or_default()
    };
    let mut rankings: Vec<AssetRank> = by_symbol
        .iter()
        .map(|(symbol, wins)| {
            let mean_conf =
                wins.iter().map(|a| a.confidence_assessment).sum::<f64>() / wins.len() as f64;
            let score = params.asset_rank_slope * mean_conf + params.asset_rank_offset;
            let risk_level = risk_level_for(symbol);
            // Mirror the per-symbol AlignmentMatrix onto the
            // AssetRank so REST / export consumers can show
            // multi-timeframe alignment per asset without a second
            // fetch. When no alignment is available for this symbol
            // (cold start, transient snapshot gap, or symbol not in
            // the alignment slice), default to neutral values.
            let (mtf_score, mtf_label) = match alignments_by_symbol.get(*symbol) {
                Some(aln) => (aln.mtf_overall_score, aln.mtf_overall_label.clone()),
                None => (0.0, "NO_DATA".to_string()),
            };
            AssetRank {
                symbol: symbol.to_string(),
                score,
                bias: mode_of(wins, &|a| format!("{:?}", a.directional_guidance)),
                confidence: mean_conf,
                regime: mode_of(wins, &|a| format!("{:?}", a.strategy_environment)),
                risk_level: risk_level.to_string(),
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

    // Opportunity distribution
    let mut opportunity_distribution: HashMap<String, u32> = HashMap::new();
    for adv in advisories {
        let opp_key = format!("{:?}", adv.opportunity_classification);
        *opportunity_distribution.entry(opp_key).or_insert(0) += 1;
    }

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
    let total_adv = advisories.len().max(1) as f64;
    let low_risk = advisories
        .iter()
        .filter(|a| overall_for(&a.symbol) <= params.risk_low_max)
        .count() as f64
        / total_adv
        * 100.0;
    // v6.10.19 (P7): the DESCRIPTIVE `high_pct` (risk distribution card)
    // stays on the plain per-symbol mean — the dashboard must keep parity
    // with the panels. The SYSTEMIC path below uses the TF-decayed count.
    let high_pct = advisories
        .iter()
        .filter(|a| overall_for(&a.symbol) >= params.risk_high_min)
        .count() as f64
        / total_adv
        * 100.0;

    // v6.10.19 (P7): TF-decayed systemic high-share — weights discount
    // high-frequency Micro-tier volatility so a transient 60s risk spike
    // contributes at most 10% to the PME safety veto. Falls back to the
    // plain per-symbol mean when the daemon did not supply windows.
    let systemic_high_pct = {
        let mut weight_sum = 0.0;
        let mut weighted_high = 0.0;
        for meta in instances.iter().filter(|i| i.is_active) {
            if meta.risk_windows.is_empty() {
                weight_sum += 1.0;
                if meta.overall_risk >= 70.0 {
                    weighted_high += 1.0;
                }
            } else {
                for (w, risk) in &meta.risk_windows {
                    weight_sum += w;
                    if *risk >= 70.0 {
                        weighted_high += w;
                    }
                }
            }
        }
        if weight_sum <= 0.0 {
            0.0
        } else {
            weighted_high / weight_sum * 100.0
        }
    };

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
    let mean_overall_risk = if by_symbol.is_empty() {
        0.0
    } else {
        by_symbol.keys().map(|s| overall_for(s)).sum::<f64>() / by_symbol.len() as f64
    };
    let risk_environment = if instance_count == 0 {
        "NO_DATA"
    } else if mean_overall_risk >= params.env_mean_high {
        "HIGH_RISK"
    } else if mean_overall_risk >= params.env_mean_moderate {
        "MODERATE"
    } else {
        "LOW_RISK"
    };

    // systemic_risk_score = 0.6 * high_pct + 0.4 * sync_penalty.
    // Bug-fix #10: the legacy `sync_penalty` was only computed when
    // `global_bias` was Bearish or StrongBearish — a synchronized bull
    // market produced `sync_penalty = 0.0`, understating the systemic
    // risk for a perfectly correlated long-everything environment.
    // We now apply the sync penalty for any directional bias
    // (StrongBullish, Bullish, Neutral, Bearish, StrongBearish) with
    // a magnitude that scales with directional strength. A
    // StrongBullish synchronized market is just as risky (correlated
    // longs unwind together on the next catalyst) as a StrongBearish
    // one.
    // v6.10 (Phase 6 / F5): `sync_penalty` is restricted to BEARISH /
    // STRONG_BEARISH global market biases per the spec table at
    // `docs/matrices/02-09-overview-matrix.md` lines 165-170. The previous
    // implementation applied a `directional_intensity` multiplier (0/0.3/0.7/1.0)
    // to ALL directional biases; the canonical spec restricts the penalty
    // to bearish regimes only. For Bullish / StrongBullish / Mixed /
    // Neutral we emit `sync_penalty = 0` regardless of sync level; the
    // `high_pct` term still contributes to `SystemicRisk`.
    let directional_intensity: f64 = match global_bias {
        GlobalBias::StrongBearish | GlobalBias::Bearish => 1.0,
        _ => 0.0,
    };
    let sync_penalty = directional_intensity
        * match sync {
            SyncLevel::HighlySynchronized => params.sync_penalty[0],
            SyncLevel::Synchronized => params.sync_penalty[1],
            SyncLevel::Mixed => params.sync_penalty[2],
            SyncLevel::Fragmented => params.sync_penalty[3],
            SyncLevel::HighlyFragmented => params.sync_penalty[4],
        };
    // v6.10.19 (P7): the systemic path uses the TF-DECAYED high-share —
    // the safety veto must not fire on a micro-tier noise spike.
    let systemic_risk_score = params.systemic_high_weight * systemic_high_pct
        + params.systemic_sync_weight * sync_penalty;

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

    // cascade_risk_index: mean of per-symbol cascade risk scores from AdvisoryMatrix
    let mut cascade_total = 0.0;
    let mut cascade_count = 0;
    for adv in advisories {
        cascade_total += adv.cascade_risk_score;
        cascade_count += 1;
    }
    let cascade_score = if cascade_count > 0 {
        cascade_total / cascade_count as f64
    } else {
        params.cascade_index_fallback
    };
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
        confidence: (cascade_count as f64 / total_adv * 100.0).min(100.0),
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
        systemic_risk_score,
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
        hero: None,
        overview_rows: Vec::new(),
        signal_quality: None,
        direction_distribution: None,
        market_health_dims: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_default() {
        let o = compute_overview(&[], &[], &[], &OverviewParams::default());
        // Empty input → OverviewMatrix::empty() early-return path.
        // The empty OverviewMatrix declares GlobalBias::Neutral as its
        // canonical default; the live `compute_overview` path now
        // returns Mixed (after the dead-branch removal) for the
        // empty-advisories case, so we exercise the early-return by
        // passing zero advisories AND zero active instances.
        assert!(matches!(o.global_market_bias, GlobalBias::Neutral));
        assert_eq!(o.instance_count, 0);
        assert_eq!(o.systemic_risk_score, 0.0);
        // New aggregate alignment fields default to neutral values.
        assert!(o.alignment_distribution.is_empty());
        assert_eq!(o.alignment_consensus_index, 0.0);
        assert_eq!(o.multi_tf_agreement_pct, 0.0);
    }

    #[test]
    fn all_neutral_advisories_resolve_to_mixed() {
        // Bug-fix #14: removed the `neutral_pct >= 0.6 → Neutral`
        // branch. A market with 100% neutral advisories now falls
        // through to `Mixed` (after `long_count > short_count` and
        // `short_count > long_count` both fail). The empty-input
        // case is still `Neutral` via the early-return default.
        let advs: Vec<AdvisoryMatrix> = (0..5)
            .map(|i| AdvisoryMatrix {
                symbol: format!("X-USD{}", i),
                ..AdvisoryMatrix::empty(&format!("X-USD{}", i))
            })
            .collect();
        let o = compute_overview(&advs, &[], &[], &OverviewParams::default());
        assert!(matches!(o.global_market_bias, GlobalBias::Mixed));
    }

    #[test]
    fn single_bullish_adv_produces_bullish_overview() {
        let adv = AdvisoryMatrix {
            symbol: "BTC-USD".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 75.0,
            ..AdvisoryMatrix::empty("BTC-USD")
        };
        let instances = vec![InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "slow300".into(),
            is_active: true,
            overall_risk: 50.0,
            risk_windows: vec![],
        }];
        let o = compute_overview(&[adv], &instances, &[], &OverviewParams::default());
        assert!(matches!(o.global_market_bias, GlobalBias::StrongBullish));
        assert_eq!(o.instance_count, 1);
        assert!(!o.regime_distribution.is_empty());
        assert!(!o.opportunity_distribution.is_empty());
    }

    #[test]
    fn risk_data_binds_when_meta_symbol_matches_advisory_symbol() {
        // Regression (C1): the daemon previously fed the QUOTE currency
        // ("USDT") as `InstanceMeta.symbol`. compute_overview keys the
        // per-symbol risk map by `meta.symbol` and looks it up with the
        // ADVISORY symbol — a mismatch silently fell back to 50
        // (MODERATE) everywhere: risk_distribution always 0/100/0,
        // risk_environment HIGH_RISK, AssetRank.risk_level MODERATE.
        // With the canonical symbol ("BTC-USDT") the LOW risk binds.
        let adv = AdvisoryMatrix {
            symbol: "BTC-USDT".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 80.0,
            ..AdvisoryMatrix::empty("BTC-USDT")
        };
        let instances = vec![InstanceMeta {
            symbol: "BTC-USDT".into(),
            timeframe_secs: 300,
            timeframe_label: "tf-average".into(),
            is_active: true,
            overall_risk: 20.0, // LOW band (≤ 30)
            risk_windows: vec![],
        }];
        let o = compute_overview(&[adv], &instances, &[], &OverviewParams::default());
        assert_eq!(
            o.risk_distribution.low_pct, 100.0,
            "low risk must bind to the risk distribution"
        );
        assert_eq!(o.risk_distribution.high_pct, 0.0);
        assert_eq!(o.risk_distribution.moderate_pct, 0.0);
        assert!(
            !o.risk_distribution.risk_environment.contains("HIGH_RISK"),
            "risk_environment must reflect the bound LOW score"
        );
        let rank = o
            .asset_ranking
            .iter()
            .find(|r| r.symbol == "BTC-USDT")
            .expect("asset ranking row for the symbol");
        assert_eq!(rank.risk_level, "LOW");
        assert_eq!(
            o.active_symbols,
            vec!["BTC-USDT".to_string()],
            "active_symbols must contain the symbol, not the quote"
        );
        assert_eq!(o.instance_count, 1);
    }

    #[test]
    fn mismatched_meta_symbol_leaks_quote_into_active_symbols() {
        // Negative lock: when InstanceMeta.symbol does not equal the
        // advisory symbol (the C1 corruption mode), the overview can
        // never bind risk data and the union leaks the quote currency
        // into active_symbols. Kept to document the failure shape — the
        // daemon must feed `inst.symbol()` (canonical "BTC-USDT").
        let adv = AdvisoryMatrix {
            symbol: "BTC-USDT".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 80.0,
            ..AdvisoryMatrix::empty("BTC-USDT")
        };
        let instances = vec![InstanceMeta {
            symbol: "USDT".into(), // quote currency — the old daemon bug
            timeframe_secs: 300,
            timeframe_label: "tf-average".into(),
            is_active: true,
            overall_risk: 20.0,
            risk_windows: vec![],
        }];
        let o = compute_overview(&[adv], &instances, &[], &OverviewParams::default());
        assert_eq!(o.risk_distribution.low_pct, 0.0);
        assert_eq!(o.risk_distribution.moderate_pct, 100.0);
        assert!(o.active_symbols.contains(&"USDT".to_string()));
        assert!(o.active_symbols.contains(&"BTC-USDT".to_string()));
        assert_ne!(o.active_symbols.len(), o.instance_count as usize);
    }

    // ── Alignment aggregation tests ───────────────────────────

    fn make_alignment(
        symbol: &str,
        mtf_score: f64,
        label: &str,
        agreement: f64,
    ) -> AlignmentMatrix {
        AlignmentMatrix {
            symbol: symbol.into(),
            timeframes_present: 4,
            dimensions: Vec::new(),
            mtf_trend_alignment: mtf_score / 100.0,
            mtf_momentum_alignment: mtf_score / 100.0,
            mtf_volume_alignment: 0.0,
            mtf_volatility_alignment: 0.0,
            mtf_overall_score: mtf_score,
            mtf_overall_label: label.into(),
            blend_weights: vec![
                ("Trend".into(), 0.5),
                ("Momentum".into(), 0.3),
                ("Volume".into(), 0.1),
                ("Volatility".into(), 0.1),
            ],
            timeframe_alignments: Vec::new(),
            signal_cross_tf_count: 0,
            trend_agreement_pct: agreement,
        }
    }

    #[test]
    fn alignment_distribution_counts_per_label() {
        let advs: Vec<AdvisoryMatrix> = (0..4)
            .map(|i| AdvisoryMatrix {
                symbol: format!("X-USD{}", i),
                ..AdvisoryMatrix::empty(&format!("X-USD{}", i))
            })
            .collect();
        let alignments = vec![
            make_alignment("X-USD0", 80.0, "STRONG_BULL_MTF", 90.0),
            make_alignment("X-USD1", 30.0, "WEAK_BULL_MTF", 70.0),
            make_alignment("X-USD2", 0.0, "NEUTRAL_MTF", 50.0),
            make_alignment("X-USD3", -60.0, "STRONG_BEAR_MTF", 85.0),
        ];
        let o = compute_overview(&advs, &[], &alignments, &OverviewParams::default());
        assert_eq!(o.alignment_distribution.get("STRONG_BULL_MTF"), Some(&1));
        assert_eq!(o.alignment_distribution.get("WEAK_BULL_MTF"), Some(&1));
        assert_eq!(o.alignment_distribution.get("NEUTRAL_MTF"), Some(&1));
        assert_eq!(o.alignment_distribution.get("STRONG_BEAR_MTF"), Some(&1));
        assert_eq!(o.alignment_distribution.get("WEAK_BEAR_MTF"), None);
        assert_eq!(o.alignment_distribution.get("NO_DATA"), None);
        assert_eq!(o.alignment_distribution.values().sum::<u32>(), 4);
    }

    #[test]
    fn alignment_consensus_index_is_mean_of_scores() {
        let advs: Vec<AdvisoryMatrix> = (0..3)
            .map(|i| AdvisoryMatrix {
                symbol: format!("X-USD{}", i),
                ..AdvisoryMatrix::empty(&format!("X-USD{}", i))
            })
            .collect();
        let alignments = vec![
            make_alignment("X-USD0", 50.0, "WEAK_BULL_MTF", 60.0),
            make_alignment("X-USD1", 0.0, "NEUTRAL_MTF", 50.0),
            make_alignment("X-USD2", -50.0, "WEAK_BEAR_MTF", 40.0),
        ];
        let o = compute_overview(&advs, &[], &alignments, &OverviewParams::default());
        // (50 + 0 + -50) / 3 = 0.0
        assert!((o.alignment_consensus_index - 0.0).abs() < 1e-9);
        // (60 + 50 + 40) / 3 = 50.0
        assert!((o.multi_tf_agreement_pct - 50.0).abs() < 1e-9);
    }

    #[test]
    fn asset_rank_mirrors_alignment_when_present() {
        let adv = AdvisoryMatrix {
            symbol: "BTC-USD".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 75.0,
            ..AdvisoryMatrix::empty("BTC-USD")
        };
        let instances = vec![InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "slow300".into(),
            is_active: true,
            overall_risk: 50.0,
            risk_windows: vec![],
        }];
        let alignment = make_alignment("BTC-USD", 65.5, "STRONG_BULL_MTF", 80.0);
        let o = compute_overview(&[adv], &instances, &[alignment], &OverviewParams::default());
        assert_eq!(o.asset_ranking.len(), 1);
        assert_eq!(o.asset_ranking[0].symbol, "BTC-USD");
        assert!((o.asset_ranking[0].mtf_score - 65.5).abs() < 1e-9);
        assert_eq!(o.asset_ranking[0].mtf_label, "STRONG_BULL_MTF");
    }

    #[test]
    fn risk_distribution_bins_on_overall_risk_not_cascade() {
        // L7-A (v6.10.13): the risk distribution / risk_environment bin
        // per-asset L5 OVERALL risk — a symbol with LOW cascade risk but
        // HIGH overall risk must bin HIGH (the old cascade-based code and
        // the confidence-based doc disagreed with the dashboard card).
        let adv_low = AdvisoryMatrix {
            symbol: "BTC-USD".into(),
            cascade_risk_score: 10.0,
            ..AdvisoryMatrix::empty("BTC-USD")
        };
        let adv_high = AdvisoryMatrix {
            symbol: "SOL-USD".into(),
            cascade_risk_score: 10.0,
            ..AdvisoryMatrix::empty("SOL-USD")
        };
        let instances = vec![
            InstanceMeta {
                symbol: "BTC-USD".into(),
                timeframe_secs: 300,
                timeframe_label: "slow300".into(),
                is_active: true,
                overall_risk: 20.0,
                risk_windows: vec![],
            },
            InstanceMeta {
                symbol: "SOL-USD".into(),
                timeframe_secs: 300,
                timeframe_label: "slow300".into(),
                is_active: true,
                overall_risk: 85.0,
                risk_windows: vec![],
            },
        ];
        let o = compute_overview(
            &[adv_low, adv_high],
            &instances,
            &[],
            &OverviewParams::default(),
        );
        // Despite cascade 10 on BOTH symbols, the split follows overall:
        // BTC low (20), SOL high (85).
        assert_eq!(o.risk_distribution.low_pct, 50.0);
        assert_eq!(o.risk_distribution.high_pct, 50.0);
        assert_eq!(o.risk_distribution.risk_environment, "HIGH_RISK");
        // L7-B: the cascade index confidence is a 0-100 percentage.
        assert!(
            (o.cascade_risk_index.confidence - 100.0).abs() < 1e-9,
            "cascade_risk_index.confidence must be 100% with a full sample, got {}",
            o.cascade_risk_index.confidence
        );
    }

    #[test]
    fn risk_environment_mean_based_not_high_pct() {
        // v6.10.16 (FIX-O3): the environment label bins on the MEAN overall
        // risk — an environment where 100% of assets sit in "moderate"
        // (mean 37.5) must read MODERATE, never LOW_RISK (the old high_pct-
        // only rule said LOW_RISK for any environment with <25% high assets).
        let advs: Vec<AdvisoryMatrix> = ["BTC-USD", "SOL-USD"]
            .iter()
            .map(|s| AdvisoryMatrix {
                symbol: s.to_string(),
                ..AdvisoryMatrix::empty(s)
            })
            .collect();
        let instances = vec![
            InstanceMeta {
                symbol: "BTC-USD".into(),
                timeframe_secs: 300,
                timeframe_label: "slow300".into(),
                is_active: true,
                overall_risk: 32.0,
                risk_windows: vec![],
            },
            InstanceMeta {
                symbol: "SOL-USD".into(),
                timeframe_secs: 300,
                timeframe_label: "slow300".into(),
                is_active: true,
                overall_risk: 43.0,
                risk_windows: vec![],
            },
        ];
        let o = compute_overview(&advs, &instances, &[], &OverviewParams::default());
        assert_eq!(o.risk_distribution.low_pct, 0.0);
        assert_eq!(o.risk_distribution.moderate_pct, 100.0);
        assert_eq!(o.risk_distribution.high_pct, 0.0);
        assert_eq!(o.risk_distribution.risk_environment, "MODERATE");
    }

    #[test]
    fn asset_rank_risk_level_bins_overall_risk_not_confidence() {
        // v6.10.16 (FIX-O3): `AssetRank.risk_level` is L5 OVERALL risk —
        // the field previously binned on `confidence_assessment` (75 → LOW
        // under the old rule) which disagreed with the L5 panel. 75%
        // confidence with 43 overall risk must read MODERATE (43 < 50 keeps
        // the environment at MODERATE under the mean-based rule).
        let adv = AdvisoryMatrix {
            symbol: "BTC-USD".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 75.0,
            ..AdvisoryMatrix::empty("BTC-USD")
        };
        let instances = vec![InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "slow300".into(),
            is_active: true,
            overall_risk: 43.0,
            risk_windows: vec![],
        }];
        let o = compute_overview(&[adv], &instances, &[], &OverviewParams::default());
        assert_eq!(o.asset_ranking.len(), 1);
        assert_eq!(o.asset_ranking[0].risk_level, "MODERATE");
        assert_eq!(o.risk_distribution.risk_environment, "MODERATE");
    }

    #[test]
    fn asset_rank_defaults_when_alignment_missing() {
        // Symbol present in advisories but absent from alignments —
        // must default to neutral without breaking the rest of the
        // aggregation.
        let adv = AdvisoryMatrix {
            symbol: "BTC-USD".into(),
            directional_guidance: DirectionalGuidance::Long,
            confidence_assessment: 75.0,
            ..AdvisoryMatrix::empty("BTC-USD")
        };
        let instances = vec![InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "slow300".into(),
            is_active: true,
            overall_risk: 50.0,
            risk_windows: vec![],
        }];
        let o = compute_overview(&[adv], &instances, &[], &OverviewParams::default());
        assert_eq!(o.asset_ranking.len(), 1);
        assert_eq!(o.asset_ranking[0].mtf_score, 0.0);
        assert_eq!(o.asset_ranking[0].mtf_label, "NO_DATA");
        // Aggregate alignment fields still default to neutral.
        assert!(o.alignment_distribution.is_empty());
        assert_eq!(o.alignment_consensus_index, 0.0);
        assert_eq!(o.multi_tf_agreement_pct, 0.0);
    }

    #[test]
    fn empty_alignments_yield_neutral_aggregates_without_breaking_advisories() {
        // Advisories present but no alignments — the breadth / bias /
        // sync / risk aggregates must still populate correctly; the
        // alignment aggregates must default to neutral without
        // propagating NaN.
        let advs: Vec<AdvisoryMatrix> = (0..3)
            .map(|i| AdvisoryMatrix {
                symbol: format!("X-USD{}", i),
                directional_guidance: DirectionalGuidance::Long,
                confidence_assessment: 80.0,
                ..AdvisoryMatrix::empty(&format!("X-USD{}", i))
            })
            .collect();
        let o = compute_overview(&advs, &[], &[], &OverviewParams::default());
        assert!(matches!(o.global_market_bias, GlobalBias::StrongBullish));
        assert_eq!(o.alignment_distribution.len(), 0);
        assert_eq!(o.alignment_consensus_index, 0.0);
        assert_eq!(o.multi_tf_agreement_pct, 0.0);
    }

    #[test]
    fn systemic_risk_is_tf_decayed_not_micro_driven() {
        // v6.10.19 (P7): a transient MICRO risk spike must not move the
        // systemic path (the PME veto input) — slow/macro windows anchor
        // the safety math. Weights: micro 0.1 / fast 0.2 / slow 0.3 /
        // macro 0.4.
        let mk = |symbol: &str, guidance: DirectionalGuidance| AdvisoryMatrix {
            symbol: symbol.to_string(),
            directional_guidance: guidance,
            confidence_assessment: 30.0,
            ..AdvisoryMatrix::empty(symbol)
        };
        // Micro window at risk 95 (a noise spike), the other three calm.
        let spike = InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "tf-average".into(),
            is_active: true,
            overall_risk: 48.0,
            risk_windows: vec![(0.1, 95.0), (0.2, 30.0), (0.3, 35.0), (0.4, 40.0)],
        };
        let advs = vec![mk("BTC-USD", DirectionalGuidance::Long)];
        let o = compute_overview(&advs, &[spike], &[], &OverviewParams::default());
        // Weighted high share = 0.1 (only micro ≥ 70) → systemic stays low.
        let expected_high = 0.1 / 1.0 * 100.0;
        assert!(
            (o.systemic_risk_score - 0.6 * expected_high).abs() < 1e-6,
            "micro spike must NOT drive systemic: got {}",
            o.systemic_risk_score
        );
        // Descriptive high_pct (per-symbol mean) reads 0% — the plain
        // mean 48 < 70 — panel parity preserved.
        assert_eq!(o.risk_distribution.high_pct, 0.0);
        assert_eq!(o.risk_distribution.risk_environment, "MODERATE");

        // The mirror: a MACRO high window (weight 0.4) must move it.
        let macro_high = InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "tf-average".into(),
            is_active: true,
            overall_risk: 50.0,
            risk_windows: vec![(0.1, 30.0), (0.2, 35.0), (0.3, 40.0), (0.4, 95.0)],
        };
        let o2 = compute_overview(&advs, &[macro_high], &[], &OverviewParams::default());
        assert!(
            (o2.systemic_risk_score - 0.6 * 40.0).abs() < 1e-6,
            "macro high must move systemic: got {}",
            o2.systemic_risk_score
        );
    }

    #[test]
    fn tf_window_advisories_aggregate_per_symbol() {
        // v6.10.18 (I-2): the L7 basis is the TF average — a symbol can
        // contribute up to 4 window advisories. The rankings must emit ONE
        // rank per symbol (mean confidence, mode bias/regime with the
        // fastest window winning ties), and the global bias tallies the
        // WINDOWS (3:1 bullish → BULLISH, not a degenerate 1-symbol
        // STRONG_BULLISH).
        let mk = |symbol: &str, guidance: DirectionalGuidance, conf: f64| AdvisoryMatrix {
            symbol: symbol.to_string(),
            directional_guidance: guidance,
            confidence_assessment: conf,
            ..AdvisoryMatrix::empty(symbol)
        };
        let advs = vec![
            mk("BTC-USD", DirectionalGuidance::Short, 20.0), // micro
            mk("BTC-USD", DirectionalGuidance::Long, 30.0),  // fast
            mk("BTC-USD", DirectionalGuidance::Long, 50.0),  // slow
            mk("BTC-USD", DirectionalGuidance::Long, 40.0),  // macro
        ];
        let instances = vec![InstanceMeta {
            symbol: "BTC-USD".into(),
            timeframe_secs: 300,
            timeframe_label: "tf-average".into(),
            is_active: true,
            overall_risk: 41.0,
            risk_windows: vec![],
        }];
        let o = compute_overview(&advs, &instances, &[], &OverviewParams::default());
        // ONE rank for the symbol; confidence = mean(20,30,50,40) = 35;
        // score = 0.5×35 + 50 = 67.5; bias = mode = Long (3 windows).
        assert_eq!(o.asset_ranking.len(), 1);
        assert!((o.asset_ranking[0].confidence - 35.0).abs() < 1e-9);
        assert!((o.asset_ranking[0].score - 67.5).abs() < 1e-9);
        assert_eq!(o.asset_ranking[0].bias, "Long");
        assert_eq!(o.asset_ranking[0].risk_level, "MODERATE");
        // Global bias tallies windows: 3L/1S → breadth +50 → BULLISH.
        assert!(matches!(o.global_market_bias, GlobalBias::Bullish));
        // TF-mean risk 41 → MODERATE environment.
        assert_eq!(o.risk_distribution.risk_environment, "MODERATE");
    }
}
