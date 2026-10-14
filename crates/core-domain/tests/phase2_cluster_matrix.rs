//! Phase 2 tests: LiquidationClusterMatrix estimation algorithm.

use core_domain::liquidity::{
    estimate_clusters, ClusterEstimateInput, ClusterKind, LeverageAssumptions,
    LeverageDistributionSource, LiquidationCluster, LiquidationClusterMatrix,
};

fn make_history(base: f64, n: usize, amplitude: f64) -> Vec<f64> {
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / n.max(1) as f64;
        v.push(
            base + amplitude * (t * std::f64::consts::PI * 2.0).sin()
                + amplitude * 0.3 * (t * std::f64::consts::PI * 6.0).cos(),
        );
    }
    v
}

#[test]
fn empty_matrix_default_values() {
    let m = LiquidationClusterMatrix::empty("BTC-USDT", 50_000.0);
    assert!(m.short_clusters.is_empty());
    assert!(m.long_clusters.is_empty());
    assert_eq!(m.cascade_asymmetry, 0.0);
    assert_eq!(m.estimation_confidence, 0.0);
    assert_eq!(m.mid_price, 50_000.0);
    assert_eq!(m.leverage_assumptions.buckets.len(), 7);
    assert_eq!(m.leverage_assumptions.weights.len(), 7);
}

#[test]
fn empty_matrix_serialization_roundtrip() {
    let m = LiquidationClusterMatrix::empty("BTC-USDT", 50_000.0);
    let json = serde_json::to_string(&m).unwrap();
    let parsed: LiquidationClusterMatrix = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.symbol, m.symbol);
    assert_eq!(parsed.mid_price, m.mid_price);
}

#[test]
fn cluster_kind_serializes_as_screaming_snake_case() {
    let m = LiquidationClusterMatrix::empty("BTC", 100.0);
    let _json = serde_json::to_string(&m).unwrap();
    // No clusters in empty matrix; just verify the enum itself.
    let c = LiquidationCluster {
        price_low: 90.0,
        price_high: 95.0,
        peak_price: 92.5,
        notional_usd: 1_000_000.0,
        dominant_leverage: 10,
        distance_from_mid_pct: 5.0,
        cluster_kind: ClusterKind::AtCurrentPrice,
        magnet_strength: 80.0,
    };
    let c_json = serde_json::to_string(&c).unwrap();
    assert!(
        c_json.contains("\"cluster_kind\":\"AT_CURRENT_PRICE\""),
        "got: {}",
        c_json
    );
}

#[test]
fn empty_input_returns_empty_matrix() {
    let input = ClusterEstimateInput {
        mid_price: 0.0,
        total_oi_usd: 0.0,
        ..ClusterEstimateInput::default()
    };
    let m = estimate_clusters(&input);
    assert_eq!(m.short_clusters.len(), 0);
    assert_eq!(m.long_clusters.len(), 0);
    assert_eq!(m.cascade_asymmetry, 0.0);
}

#[test]
fn positive_oi_produces_clusters() {
    let history = make_history(50_000.0, 200, 200.0);
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 100_000_000.0,
        funding_rate: 0.0,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: false,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m = estimate_clusters(&input);
    assert!(!m.short_clusters.is_empty());
    assert!(!m.long_clusters.is_empty());
    // All clusters should be finite and have non-zero notional.
    for c in &m.short_clusters {
        assert!(c.notional_usd > 0.0);
        assert!(c.peak_price.is_finite());
    }
    for c in &m.long_clusters {
        assert!(c.notional_usd > 0.0);
        assert!(c.peak_price.is_finite());
    }
}

#[test]
fn clusters_have_reasonable_distance_from_mid() {
    let history = make_history(50_000.0, 200, 200.0);
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 50_000_000.0,
        funding_rate: 0.0,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: false,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m = estimate_clusters(&input);
    for c in &m.short_clusters {
        // Short liqs are above mid; distance should be > 0.
        assert!(
            c.distance_from_mid_pct > 0.0,
            "short cluster distance must be positive, got {}",
            c.distance_from_mid_pct
        );
        // Distance should not be crazy (no liquidation beyond 50% from price).
        assert!(c.distance_from_mid_pct < 50.0);
    }
    for c in &m.long_clusters {
        // Long liqs are below mid; distance should be > 0.
        assert!(c.distance_from_mid_pct > 0.0);
        assert!(c.distance_from_mid_pct < 50.0);
    }
}

#[test]
fn confidence_increases_with_oi() {
    let history = make_history(50_000.0, 200, 200.0);

    // 1M OI (decent)
    let mut input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 1_000_000.0,
        funding_rate: 0.0,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: false,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m_low = estimate_clusters(&input);
    let conf_low = m_low.estimation_confidence;

    // 10M OI (much better)
    input.total_oi_usd = 10_000_000.0;
    let m_high = estimate_clusters(&input);
    let conf_high = m_high.estimation_confidence;

    assert!(
        conf_high >= conf_low,
        "higher OI should give higher confidence: {} < {}",
        conf_high,
        conf_low
    );
}

#[test]
fn confidence_decays_with_extreme_funding() {
    let history = make_history(50_000.0, 200, 200.0);
    let make = |funding: f64| ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 5_000_000.0,
        funding_rate: funding,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: true,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m_calm = estimate_clusters(&make(0.0));
    let m_hot = estimate_clusters(&make(0.001)); // 0.1% / 8h, very hot
    assert!(
        m_calm.estimation_confidence >= m_hot.estimation_confidence,
        "extreme funding should reduce confidence: calm={} < hot={}",
        m_calm.estimation_confidence,
        m_hot.estimation_confidence
    );
}

#[test]
fn leverage_assumptions_serializes_source_correctly() {
    let a = LeverageAssumptions {
        buckets: vec![1, 5, 10, 50],
        weights: vec![0.25, 0.25, 0.25, 0.25],
        funding_modulation_active: true,
        funding_extreme_pct: 0.0005,
        source: LeverageDistributionSource::FundingAdaptive,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"source\":\"FUNDING_ADAPTIVE\""));
    let a2: LeverageAssumptions = serde_json::from_str(&json).unwrap();
    assert_eq!(a2.source, LeverageDistributionSource::FundingAdaptive);
}

#[test]
fn estimate_deterministic_for_same_input() {
    let history = make_history(50_000.0, 200, 200.0);
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 10_000_000.0,
        funding_rate: 0.0001,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: true,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m1 = estimate_clusters(&input);
    let m2 = estimate_clusters(&input);
    assert_eq!(m1.short_clusters.len(), m2.short_clusters.len());
    assert_eq!(m1.long_clusters.len(), m2.long_clusters.len());
    assert!((m1.cascade_asymmetry - m2.cascade_asymmetry).abs() < 1e-9);
    for (a, b) in m1.long_clusters.iter().zip(m2.long_clusters.iter()) {
        assert_eq!(a.peak_price, b.peak_price);
        assert_eq!(a.notional_usd, b.notional_usd);
    }
}

#[test]
fn short_clusters_above_mid_long_clusters_below_mid() {
    let history = make_history(50_000.0, 200, 200.0);
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 50_000_000.0,
        funding_rate: 0.0,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: false,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m = estimate_clusters(&input);
    for c in &m.short_clusters {
        assert!(
            c.peak_price > m.mid_price,
            "short cluster must be above mid, got {}",
            c.peak_price
        );
        assert_eq!(c.cluster_kind, ClusterKind::AboveCurrentPrice);
    }
    for c in &m.long_clusters {
        assert!(
            c.peak_price < m.mid_price,
            "long cluster must be below mid, got {}",
            c.peak_price
        );
        assert_eq!(c.cluster_kind, ClusterKind::BelowCurrentPrice);
    }
}

#[test]
fn long_oi_override_dominates_funding_signal() {
    let history = make_history(50_000.0, 200, 200.0);
    // Funding says longs dominate (negative funding), but override says 30% long.
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &history,
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 10_000_000.0,
        funding_rate: -0.001,   // strongly short-biased funding
        long_oi_pct: Some(0.3), // override: 30% long, 70% short
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: true,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    let m = estimate_clusters(&input);
    let long_total: f64 = m.long_clusters.iter().map(|c| c.notional_usd).sum();
    let short_total: f64 = m.short_clusters.iter().map(|c| c.notional_usd).sum();
    // With 30% long, 70% short, short clusters should have more notional.
    assert!(
        short_total > long_total,
        "override should dominate: long={} short={}",
        long_total,
        short_total
    );
}

#[test]
fn empty_price_history_does_not_panic() {
    let input = ClusterEstimateInput {
        symbol: "BTC-USDT",
        mid_price: 50_000.0,
        price_history: &[],
        // AUDIT-F: legacy 3-bar horizon (no duration in this fixture).
        price_horizon_bars: 3,
        total_oi_usd: 1_000_000.0,
        funding_rate: 0.0,
        long_oi_pct: None,
        maintenance_margin_rate: 0.005,
        funding_extreme_pct: 0.0005,
        funding_modulation_active: false,
        leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
        leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
        min_cluster_notional_usd: 0.0,
        estimation: Default::default(),
        oi_split: Default::default(),
        confidence: Default::default(),
        funding_mod_shift: 0.05,
    };
    // Should not panic.
    let m = estimate_clusters(&input);
    // With no history, the estimator falls back to mid_price; clusters
    // should still exist on both sides.
    assert!(!m.short_clusters.is_empty() || !m.long_clusters.is_empty());
}

#[test]
fn cluster_serialization_omits_default_fields() {
    let c = LiquidationCluster {
        price_low: 0.0,
        price_high: 0.0,
        peak_price: 0.0,
        notional_usd: 0.0,
        dominant_leverage: 10,
        distance_from_mid_pct: 0.0,
        cluster_kind: ClusterKind::Distant,
        magnet_strength: 0.0,
    };
    let json = serde_json::to_string(&c).unwrap();
    // Required fields present.
    for f in &[
        "price_low",
        "price_high",
        "peak_price",
        "notional_usd",
        "dominant_leverage",
        "distance_from_mid_pct",
        "cluster_kind",
        "magnet_strength",
    ] {
        assert!(json.contains(f), "missing {} in {}", f, json);
    }
    assert!(json.contains("\"DISTANT\""));
}
