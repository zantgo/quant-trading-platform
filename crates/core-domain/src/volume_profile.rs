//! Volume Profile snapshot — per-timeframe aggregated volume distribution
//! across price levels.
//!
//! This module defines the wire-format DTO that the frontend renders as a
//! horizontal histogram on the price chart. The computation itself lives
//! in `market-analyzer::indicators::volume_profile`; this file is purely
//! the serializable shape that crosses the JSON-RPC boundary.
//!
//! ## Data shape
//!
//! The snapshot consists of a sequence of `VolumeProfileBin`s sorted by
//! `price_low` ascending, plus summary scalars:
//! - `poc_price` — Point of Control (highest-volume bin midpoint).
//! - `value_area_high` / `value_area_low` — top/bottom of the 70% value area.
//! - `range_high` / `range_low` — top/bottom of the price range covered by
//!   the bins (inclusive).
//!
//! ## Buy/sell split
//!
//! Each bin carries `buy_volume` (taker-buy aggregated volume) and
//! `sell_volume` (taker-sell aggregated volume). When the frontend renders
//! the bin it stacks the buy half on top of the bin centerline and the
//! sell half below, matching the TradingView default style.
//!
//! ## Bin count
//!
//! Production uses the static config `volume_profile_bins` (default 100,
//! min 1); the snapshot's `num_bins` reports the non-empty bins after the
//! zero-volume filter. `VolumeProfileSnapshot::dynamic_bin_count()` (the
//! tick-size/bar-duration formula clamped to `[30, 120]`) exists for
//! future activation but has no production callers (03-02-13).

use serde::{Deserialize, Serialize};

/// One price bin in the volume profile. The bin spans the half-open
/// interval `[price_low, price_high)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeProfileBin {
    pub price_low: f64,
    pub price_high: f64,
    /// Total volume in this bin (sum of buy + sell).
    pub volume: f64,
    /// Taker-buy aggregated volume in this bin.
    pub buy_volume: f64,
    /// Taker-sell aggregated volume in this bin.
    pub sell_volume: f64,
    /// `true` if this bin is the Point of Control.
    #[serde(default)]
    pub is_poc: bool,
    /// `true` if this bin lies inside the value area (VAH ≥ bin ≥ VAL).
    #[serde(default)]
    pub is_value_area: bool,
}

/// Full volume-profile snapshot for one timeframe. Sorted by `bins[].price_low`
/// ascending. Empty bins are NOT included in the `bins` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfileSnapshot {
    /// Symbol this profile applies to (e.g. "BTC-USDT").
    pub symbol: String,
    /// v11.9: derived duration label ("1s".."1d") of the timeframe that
    /// produced this profile.
    pub timeframe_label: String,
    /// Bar duration in seconds that defines the loaded candle set.
    pub timeframe_secs: u64,
    /// Bin edges, sorted ascending.
    pub bins: Vec<VolumeProfileBin>,
    /// Point of Control price (POC bin midpoint).
    pub poc_price: f64,
    /// Value Area High price (top of 70% value area).
    pub value_area_high: f64,
    /// Value Area Low price (bottom of 70% value area).
    pub value_area_low: f64,
    /// Total volume summed across all bins.
    pub total_volume: f64,
    /// Lowest price covered by any bin.
    pub range_low: f64,
    /// Highest price covered by any bin.
    pub range_high: f64,
    /// Number of bins produced by the dynamic bin-count formula.
    pub num_bins: usize,
    /// Unix epoch in milliseconds when this snapshot was computed.
    pub timestamp_ms: u64,
}

impl VolumeProfileSnapshot {
    /// Empty placeholder returned when the analyzer cannot yet produce a
    /// profile (insufficient candle history, zero volume, etc.).
    pub fn empty(symbol: &str, timeframe_label: &str, timeframe_secs: u64, mid_price: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            timeframe_label: timeframe_label.to_string(),
            timeframe_secs,
            bins: Vec::new(),
            poc_price: mid_price,
            value_area_high: mid_price,
            value_area_low: mid_price,
            total_volume: 0.0,
            range_low: mid_price,
            range_high: mid_price,
            num_bins: 0,
            timestamp_ms: 0,
        }
    }

    /// Compute the dynamic bin count for a given price range, tick size,
    /// and bar duration. Clamped to `[30, 120]`.
    pub fn dynamic_bin_count(price_range: f64, tick_size: f64, bar_duration_secs: u64) -> usize {
        if price_range <= 0.0 || tick_size <= 0.0 {
            return 30;
        }
        let raw = (price_range / tick_size).round() as usize;
        // Higher-TF bars add a small additive bonus (clamped 0..8) so the
        // histogram stays visually balanced on the 15m/1h columns.
        let tf_bonus = ((bar_duration_secs as f64).log2().max(0.0) as usize).min(8);
        let candidate = raw.saturating_add(tf_bonus);
        candidate.clamp(30, 120)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_round_trip() {
        let s = VolumeProfileSnapshot::empty("BTC-USDT", "micro", 60, 50_000.0);
        let json = serde_json::to_string(&s).unwrap();
        let back: VolumeProfileSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.symbol, "BTC-USDT");
        assert_eq!(back.poc_price, 50_000.0);
        assert!(back.bins.is_empty());
    }

    #[test]
    fn dynamic_bin_count_clamps_low() {
        // Tiny range + large tick → raw count way below 30 → clamped to 30.
        assert_eq!(VolumeProfileSnapshot::dynamic_bin_count(0.5, 1.0, 60), 30);
    }

    #[test]
    fn dynamic_bin_count_clamps_high() {
        // Massive range / tiny tick → raw count way above 120 → clamped to 120.
        assert_eq!(
            VolumeProfileSnapshot::dynamic_bin_count(100_000.0, 0.0001, 60),
            120
        );
    }

    #[test]
    fn dynamic_bin_count_within_range() {
        // Mid-range example: 1000 / 1.0 = 1000 + tf_bonus → clamped to 120.
        assert_eq!(
            VolumeProfileSnapshot::dynamic_bin_count(1000.0, 1.0, 60),
            120
        );
        // Small but valid: 100 / 1.0 = 100 + tf_bonus(5) = 105, clamped to 120.
        assert_eq!(
            VolumeProfileSnapshot::dynamic_bin_count(100.0, 1.0, 60),
            105
        );
    }

    #[test]
    fn dynamic_bin_count_zero_inputs() {
        assert_eq!(VolumeProfileSnapshot::dynamic_bin_count(0.0, 1.0, 60), 30);
        assert_eq!(VolumeProfileSnapshot::dynamic_bin_count(100.0, 0.0, 60), 30);
    }

    #[test]
    fn dynamic_bin_count_higher_tf_more_bins() {
        let micro = VolumeProfileSnapshot::dynamic_bin_count(100.0, 1.0, 60);
        let macro_tf = VolumeProfileSnapshot::dynamic_bin_count(100.0, 1.0, 900);
        // Higher TF bonus should produce equal or higher bin count.
        assert!(macro_tf >= micro);
    }

    #[test]
    fn dynamic_bin_count_handles_sub_minute_tfs() {
        // Sub-minute timeframes (1s, 5s, 15s, 30s). The formula must
        // remain sane — i.e. every TF produces a bin count in [30, 120].
        for tf_secs in [1u64, 5, 15, 30, 60, 180, 300, 900] {
            let n = VolumeProfileSnapshot::dynamic_bin_count(
                500.0, // $500 price range
                0.01,  // 1-cent tick
                tf_secs,
            );
            assert!(
                (30..=120).contains(&n),
                "TF={}s bin count {} must be in [30, 120]",
                tf_secs,
                n,
            );
        }
    }

    #[test]
    fn dynamic_bin_count_sub_minute_clamped_to_30() {
        // For a tight range at a 1s TF, the formula clamps to 30 bins
        // (lower bound). The result must remain a valid usize, never 0.
        let n = VolumeProfileSnapshot::dynamic_bin_count(0.10, 0.01, 1);
        assert_eq!(n, 30);
    }
}
