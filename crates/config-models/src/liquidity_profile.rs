//! Per-duration liquidity (L2.5) geometry profile — v11.10.
//!
//! The strategy's `l2_5` section is the operator BASE; this profile scales
//! the estimator geometry per ACTIVE duration. Tier provenance mirrors
//! `duration_profile` (T0 scalping → T10 macro): fast durations get finer
//! bins, tighter magnet distances and faster bound decay; slow durations
//! get coarser, wider, slower geometry.
//!
//! Constants across durations (NOT scaled — they stay in `strategy.l2_5` /
//! `[workspace.heatmap]`): leverage buckets + weights, funding_extreme_pct,
//! funding_penalty, oi_adequacy_anchor_usd, signal thresholds, ttl_secs,
//! swing_window_bars (bars, not wall-clock), heatmap bucket size.

use crate::strategy::{L1_5Params, L2_5Params};

/// One duration's estimator geometry. `price_anchor_pct` is in the
/// strategy's PERCENT convention (divide by 100 at the estimator boundary).
#[derive(Debug, Clone, PartialEq)]
pub struct LiquidityGeometry {
    pub swing_lookback: usize,
    pub bin_size_pct: f64,
    pub peak_halfwidth_divisor: usize,
    pub bound_decay: f64,
    pub magnet_activation_distance_pct: f64,
    pub price_anchor_pct: f64,
    pub price_bias_scale: f64,
    /// Per-duration OI-delta window (replaces the fixed 1-hour input).
    pub oi_delta_window_secs: u64,
}

/// Geometry scale factor per duration (fastest → slowest). `1.0` at the
/// 3 m anchor duration — the values the operator configures in `l2_5` are
/// calibrated for it.
pub fn geometry_factor(secs: u64) -> f64 {
    match secs {
        1 | 3 => 0.5,
        5 => 0.6,
        15 => 0.7,
        30 => 0.8,
        60 => 0.9,
        180 => 1.0,
        300 => 1.1,
        900 => 1.25,
        1800 => 1.5,
        3600 => 1.75,
        14400 => 2.0,
        43200 => 2.5,
        86400 => 3.0,
        _ => 1.0,
    }
}

/// Per-duration OI-delta window in seconds (v11.10). Scalping durations
/// anchor on minutes; the 1 h-and-above durations keep the historical
/// fixed 1-hour window.
pub fn oi_delta_window_secs(secs: u64) -> u64 {
    match secs {
        1 => 60,
        3 => 120,
        5 => 300,
        15 => 600,
        30 => 900,
        60 => 1800,
        _ => 3600,
    }
}

/// Decoupled per-duration cluster-refresh cadence (v11.10). The refresh
/// task no longer rides the TF's own candle cadence when
/// `[workspace.liquidity].cluster_refresh_secs = 0` (the default): each
/// duration gets a fixed wall-clock cadence tuned to how fast its price
/// level moves. TTL stays config-driven (`strategy.l2_5.estimation.ttl_secs`,
/// default 300 s) and is INDEPENDENT of this table.
pub fn refresh_cadence_secs(secs: u64) -> u64 {
    match secs {
        1 => 1,
        3 => 2,
        5 => 2,
        15 => 5,
        30 => 5,
        60 => 10,
        180 => 15,
        300 => 15,
        900 => 30,
        1800 => 60,
        3600 => 60,
        14400 => 120,
        43200 => 240,
        86400 => 300,
        _ => 60,
    }
}

/// Resolve the geometry row for one duration from the operator bases:
/// `l2_5` carries the estimator geometry, `l1_5` the magnet-activation
/// distance.
pub fn for_duration(secs: u64, base: &L2_5Params, l1_5: &L1_5Params) -> LiquidityGeometry {
    let f = geometry_factor(secs);
    let round = |v: f64| v.round() as usize;
    LiquidityGeometry {
        swing_lookback: (round(base.estimation.swing_lookback as f64 * f)).clamp(3, 50),
        bin_size_pct: (base.estimation.bin_size_pct * f).clamp(1e-5, 0.05),
        peak_halfwidth_divisor: (round(base.estimation.peak_halfwidth_divisor as f64 / f))
            .clamp(4, 200),
        bound_decay: (base.estimation.bound_decay * f).clamp(0.05, 4.0),
        magnet_activation_distance_pct: (l1_5.magnet_activation_distance_pct * f).clamp(0.01, 5.0),
        price_anchor_pct: (base.oi_split.price_anchor_pct * f).clamp(0.01, 10.0),
        price_bias_scale: base.oi_split.price_bias_scale,
        oi_delta_window_secs: oi_delta_window_secs(secs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUPPORTED_DURATIONS;

    #[test]
    fn factor_table_covers_the_full_pool() {
        for secs in SUPPORTED_DURATIONS {
            let f = geometry_factor(secs);
            assert!(f > 0.0, "{secs}s must have a positive factor");
        }
        // Canonical anchors: fastest is half, the 3 m anchor is 1.0, the
        // slowest is 3x.
        assert_eq!(geometry_factor(1), 0.5);
        assert_eq!(geometry_factor(180), 1.0);
        assert_eq!(geometry_factor(86400), 3.0);
        // Monotone non-decreasing with duration.
        let mut prev = 0.0;
        for secs in SUPPORTED_DURATIONS {
            let f = geometry_factor(secs);
            assert!(f >= prev, "factor must be monotone at {secs}s");
            prev = f;
        }
    }

    #[test]
    fn anchor_duration_preserves_the_operator_base() {
        let base = L2_5Params::default();
        let g = for_duration(180, &base, &L1_5Params::default());
        assert_eq!(g.swing_lookback, base.estimation.swing_lookback);
        assert_eq!(g.bin_size_pct, base.estimation.bin_size_pct);
        assert_eq!(
            g.peak_halfwidth_divisor,
            base.estimation.peak_halfwidth_divisor
        );
        assert_eq!(g.bound_decay, base.estimation.bound_decay);
        assert_eq!(
            g.magnet_activation_distance_pct,
            L1_5Params::default().magnet_activation_distance_pct
        );
        assert_eq!(g.price_anchor_pct, base.oi_split.price_anchor_pct);
        assert_eq!(g.price_bias_scale, base.oi_split.price_bias_scale);
    }

    #[test]
    fn fast_durations_run_finer_tighter_geometry() {
        let base = L2_5Params::default();
        let fast = for_duration(1, &base, &L1_5Params::default());
        let slow = for_duration(86400, &base, &L1_5Params::default());
        // Fast: finer bins, narrower halfwidth (larger divisor), faster
        // decay, tighter magnet/anchor distances.
        assert!(fast.bin_size_pct < slow.bin_size_pct);
        assert!(fast.peak_halfwidth_divisor > slow.peak_halfwidth_divisor);
        assert!(fast.bound_decay < slow.bound_decay);
        assert!(fast.magnet_activation_distance_pct < slow.magnet_activation_distance_pct);
        assert!(fast.price_anchor_pct < slow.price_anchor_pct);
        // Clamps hold.
        assert!(fast.swing_lookback >= 3 && slow.swing_lookback <= 50);
        assert!(fast.bin_size_pct >= 1e-5 && slow.bin_size_pct <= 0.05);
        assert!(fast.bound_decay >= 0.05 && slow.bound_decay <= 4.0);
    }

    #[test]
    fn cadence_table_is_per_duration_and_monotone() {
        assert_eq!(refresh_cadence_secs(1), 1);
        assert_eq!(refresh_cadence_secs(5), 2);
        assert_eq!(refresh_cadence_secs(60), 10);
        assert_eq!(refresh_cadence_secs(86400), 300);
        let mut prev = 0.0;
        for secs in SUPPORTED_DURATIONS {
            let c = refresh_cadence_secs(secs) as f64;
            assert!(c >= prev, "cadence must be monotone at {secs}s");
            prev = c;
        }
    }

    #[test]
    fn oi_window_table_is_per_duration() {
        assert_eq!(oi_delta_window_secs(1), 60);
        assert_eq!(oi_delta_window_secs(5), 300);
        assert_eq!(oi_delta_window_secs(60), 1800);
        // 1 h and above keep the historical fixed window.
        for secs in [3600u64, 14400, 43200, 86400] {
            assert_eq!(oi_delta_window_secs(secs), 3600);
        }
        let g = for_duration(5, &L2_5Params::default(), &L1_5Params::default());
        assert_eq!(g.oi_delta_window_secs, 300);
    }
}
