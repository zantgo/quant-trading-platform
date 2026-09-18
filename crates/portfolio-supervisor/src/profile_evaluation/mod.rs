//! Slim decision-support helpers for the MME monitor surface (v7: the
//! decision-profile authoring/evaluation feature was erased; these pure
//! confluence/alignment helpers remain for `monitor.rs`).

pub mod scoring;

pub use scoring::{calculate_registry_confluence, RegistryConfluence};

use market_analyzer::indicators::normalized::NormalizedIndicatorValue;
use std::collections::HashMap;

pub struct SnapshotValues {
    pub indicators: HashMap<String, NormalizedIndicatorValue>,
    pub current_price: f64,
}

impl SnapshotValues {
    /// Construct from an already-computed normalized indicator map.
    pub fn from_map(
        indicators: HashMap<String, NormalizedIndicatorValue>,
        current_price: f64,
    ) -> Self {
        Self {
            indicators,
            current_price,
        }
    }

    /// Fetch an indicator entry, or a neutral `UNKNOWN` default when missing.
    pub fn ind(&self, key: &str) -> NormalizedIndicatorValue {
        self.indicators
            .get(key)
            .cloned()
            .unwrap_or_else(|| NormalizedIndicatorValue::scalar(0.0, 0.0, "UNKNOWN"))
    }

    /// Normalized `[-1.0, 1.0]` score for an indicator (0.0 when missing).
    pub fn norm(&self, key: &str) -> f64 {
        self.indicators
            .get(key)
            .map(|v| v.normalized)
            .unwrap_or(0.0)
    }

    /// Context-aware state label for an indicator ("UNKNOWN" when missing).
    pub fn label(&self, key: &str) -> String {
        self.indicators
            .get(key)
            .map(|v| v.state_label.clone())
            .unwrap_or_else(|| "UNKNOWN".to_string())
    }

    /// Primary raw scalar for an indicator.
    pub fn raw(&self, key: &str) -> Option<f64> {
        self.indicators.get(key).map(|v| v.raw_value)
    }

    /// Auxiliary raw sub-component (e.g. macd `line`, ema_stack `long`).
    pub fn sub(&self, key: &str, sub: &str) -> Option<f64> {
        self.indicators
            .get(key)
            .and_then(|v| v.values.as_ref())
            .and_then(|m| m.get(sub))
            .copied()
    }
}

pub fn indicator_to_snapshot_values(
    indicators: &HashMap<String, NormalizedIndicatorValue>,
    current_price: f64,
) -> SnapshotValues {
    SnapshotValues::from_map(indicators.clone(), current_price)
}

pub enum MarketRegime {
    Trending,
    Compression,
    Expansion,
    Range,
}

pub fn classify_market_regime(snap: &SnapshotValues) -> MarketRegime {
    let adx = snap.raw("adx").unwrap_or(0.0);
    let bbwp = snap.raw("bbwp").unwrap_or(50.0);
    let squeeze_label = snap.label("squeeze");
    let squeeze_on = squeeze_label == "COMPRESSION_COILING";
    let squeeze_release = squeeze_label.ends_with("VOLATILITY_RELEASE");
    let tangled =
        snap.label("ema_stack").contains("TANGLED") || snap.norm("ema_stack").abs() < 0.10;

    if bbwp < 10.0 || squeeze_on {
        return MarketRegime::Compression;
    }

    if squeeze_release || bbwp > 90.0 {
        return MarketRegime::Expansion;
    }

    if adx >= 25.0 && !tangled {
        return MarketRegime::Trending;
    }

    MarketRegime::Range
}

/// Bucket the EMA-stack normalized value into a coarse trend direction.
fn ema_bucket(snap: &SnapshotValues) -> i8 {
    let n = snap.norm("ema_stack");
    if n > 0.10 {
        1
    } else if n < -0.10 {
        -1
    } else {
        0
    }
}

pub struct MtfTrendAlignment {
    pub micro_aligned: bool,
    pub slow_aligned: bool,
    pub structural_trend: String,
}

/// Multi-TF trend alignment over an ACTIVE-duration snapshot slice (v11.9: the
/// legacy 4-arg `micro, fast, slow, macro` signature became a slice).
///
/// `snaps` is ordered fastest → slowest (ladder order). Semantics preserved
/// from the 4-arg version, generalized positionally:
///   - `structural_trend` reads the SLOWEST snapshot (was `macro`),
///   - `micro_aligned`  = EMA bucket of the FASTEST == second element
///     (was micro == fast),
///   - `slow_aligned`   = EMA bucket of the second == SLOWEST
///     (was fast == slow).
pub fn evaluate_mtf_alignment(snaps: &[&SnapshotValues]) -> MtfTrendAlignment {
    let empty = SnapshotValues::from_map(HashMap::new(), 0.0);
    let first = snaps.first().copied().unwrap_or(&empty);
    let second = snaps.get(1).copied().unwrap_or(&empty);
    let last = snaps.last().copied().unwrap_or(&empty);

    let structural_trend = match (last.sub("ema_stack", "long"), last.current_price) {
        (Some(ema), close) if close > ema => "BULLISH".to_string(),
        (Some(ema), close) if close < ema => "BEARISH".to_string(),
        _ => "NEUTRAL".to_string(),
    };

    let micro_aligned = ema_bucket(first) == ema_bucket(second);
    let slow_aligned = ema_bucket(second) == ema_bucket(last);

    MtfTrendAlignment {
        micro_aligned,
        slow_aligned,
        structural_trend,
    }
}
