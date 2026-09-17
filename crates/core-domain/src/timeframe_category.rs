//! # TimeframeCategory — abstract trading horizon
//!
//! Each active duration (passed at the operator level via `[workspace].timeframes`)
//! is auto-classified into a `TimeframeCategory` based on its `timeframe_secs`.
//! The category controls the cross-engine recommendation chain: stop widths,
//! confidence thresholds, which indicators feed the L4/L5/L6 chain, and how
//! categories are aggregated into the single `final_recommendation`.
//!
//! Three categories (decision locked in v6.9): `Scalp` (< 5min), `Intraday`
//! (5min–1h), `Swing` (1h–1d). Timeframes ≥ 1d are rejected by the validator
//! to keep the architecture's risk semantics bounded.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 3-category trading horizon. Derived from `timeframe_secs` at pipeline
/// construction. Influences:
/// - L4 zone selection (Fibonacci, Volume Profile, Liquidation Clusters)
/// - L5 risk formulas (per-category stop widths and confidence thresholds)
/// - L6 aggregation weights (Swing > Intraday > Scalp)
/// - Multi-scale recommendation final string
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeframeCategory {
    /// < 5 minutes. Ultra-short, mean-reversion, micro-structure.
    Scalp,
    /// 5 minutes – 1 hour. Intraday momentum, news-driven.
    Intraday,
    /// 1 hour – 1 day. Multi-day trend, structure, fib/VP.
    Swing,
}

impl TimeframeCategory {
    /// Canonical mapping from `timeframe_secs` to category.
    /// `< 300 s = Scalp`, `300 s – 3,599 s = Intraday`, `3,600 s – 86,400 s = Swing`.
    pub fn for_secs(secs: u32) -> Self {
        match secs {
            s if s < 300 => Self::Scalp,
            s if s < 3_600 => Self::Intraday,
            s if s <= 86_400 => Self::Swing,
            _ => Self::Swing,
        }
    }

    /// Lowercase identifier used on the wire.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scalp => "scalp",
            Self::Intraday => "intraday",
            Self::Swing => "swing",
        }
    }

    /// Uppercase label rendered in the UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Scalp => "SCALP",
            Self::Intraday => "INTRADAY",
            Self::Swing => "SWING",
        }
    }

    /// Default aggregation weight used by the multi-scale final recommendation.
    /// Swing > Intraday > Scalp. The operator can override per-instance.
    pub fn default_aggregation_weight(&self) -> f64 {
        match self {
            Self::Scalp => 0.20,
            Self::Intraday => 0.30,
            Self::Swing => 0.50,
        }
    }

    /// Default stop-loss range used by the per-category `compute_advisory`.
    /// Returns `(min_pct, max_pct)`. The operator can override per-instance.
    pub fn default_stop_loss_range(&self) -> (f64, f64) {
        match self {
            Self::Scalp => (0.001, 0.003),    // 0.1% – 0.3%
            Self::Intraday => (0.003, 0.010), // 0.3% – 1.0%
            Self::Swing => (0.010, 0.030),    // 1.0% – 3.0%
        }
    }

    /// Default minimum confidence threshold for the per-category
    /// `trade_readiness` to be `READY`. Below this, the category
    /// folds to `WATCH` or `STAND_ASIDE`.
    pub fn default_confidence_threshold(&self) -> f64 {
        match self {
            Self::Scalp => 0.80,
            Self::Intraday => 0.60,
            Self::Swing => 0.50,
        }
    }

    /// Default minimum entry-danger threshold (the `entry_danger.score`
    /// must be ≤ this value for the category to be `READY`).
    pub fn default_entry_danger_threshold(&self) -> f64 {
        match self {
            Self::Scalp => 60.0,
            Self::Intraday => 50.0,
            Self::Swing => 40.0,
        }
    }

    /// Default persistence retention (days) for snapshots tagged with this
    /// category. Per-TF rows are aggregated at higher categories.
    pub fn default_retention_days(&self) -> u32 {
        match self {
            Self::Scalp => 30,
            Self::Intraday => 90,
            Self::Swing => 365,
        }
    }

    /// Iterate the three categories in aggregation order (highest weight first).
    pub fn all_in_order() -> [Self; 3] {
        [Self::Swing, Self::Intraday, Self::Scalp]
    }
}

impl fmt::Display for TimeframeCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_for_secs_thresholds() {
        assert_eq!(TimeframeCategory::for_secs(1), TimeframeCategory::Scalp);
        assert_eq!(TimeframeCategory::for_secs(60), TimeframeCategory::Scalp);
        assert_eq!(TimeframeCategory::for_secs(299), TimeframeCategory::Scalp);
        assert_eq!(
            TimeframeCategory::for_secs(300),
            TimeframeCategory::Intraday
        );
        assert_eq!(
            TimeframeCategory::for_secs(1800),
            TimeframeCategory::Intraday
        );
        assert_eq!(
            TimeframeCategory::for_secs(3599),
            TimeframeCategory::Intraday
        );
        assert_eq!(TimeframeCategory::for_secs(3600), TimeframeCategory::Swing);
        assert_eq!(TimeframeCategory::for_secs(14400), TimeframeCategory::Swing);
        assert_eq!(TimeframeCategory::for_secs(86400), TimeframeCategory::Swing);
    }

    #[test]
    fn category_strings_roundtrip() {
        for (secs, expected) in [
            (60u32, TimeframeCategory::Scalp),
            (1800u32, TimeframeCategory::Intraday),
            (14400u32, TimeframeCategory::Swing),
        ] {
            assert_eq!(TimeframeCategory::for_secs(secs), expected);
        }
    }

    #[test]
    fn category_default_aggregation_weights_sum_to_one() {
        let total: f64 = [
            TimeframeCategory::Scalp,
            TimeframeCategory::Intraday,
            TimeframeCategory::Swing,
        ]
        .iter()
        .map(|c| c.default_aggregation_weight())
        .sum();
        assert!(
            (total - 1.0).abs() < 1e-9,
            "weights must sum to 1.0, got {}",
            total
        );
    }

    #[test]
    fn category_serde_roundtrip() {
        let s = "scalp";
        let c: TimeframeCategory = serde_json::from_str(&format!("\"{}\"", s)).unwrap();
        assert_eq!(c, TimeframeCategory::Scalp);
        let s = "swing";
        let c: TimeframeCategory = serde_json::from_str(&format!("\"{}\"", s)).unwrap();
        assert_eq!(c, TimeframeCategory::Swing);
    }
}
