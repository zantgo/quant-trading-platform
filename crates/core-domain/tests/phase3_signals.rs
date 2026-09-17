//! Phase 3 tests: Liquidity signals + risk integration.

use core_domain::liquidity::{
    derive_liquidity_signals, CascadeState, LiquidityDirection, LiquidityFlow, LiquiditySignalKind,
    SignalInput,
};

fn empty_flow(state: CascadeState) -> LiquidityFlow {
    LiquidityFlow {
        cascade_state: state,
        ..Default::default()
    }
}

#[test]
fn no_signals_when_input_is_default() {
    let sigs = derive_liquidity_signals(&SignalInput::default());
    assert!(sigs.is_empty(), "expected no signals with default input");
}

#[test]
fn sustained_cascade_emits_one_signal() {
    let flow = LiquidityFlow {
        cascade_state: CascadeState::Sustained,
        net_liquidation_usd: 200_000.0,
        event_count: 5,
        cascade_intensity: 75.0,
        ..empty_flow(CascadeState::Sustained)
    };
    let input = SignalInput {
        flow: Some(&flow),
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let sustained: Vec<_> = sigs
        .iter()
        .filter(|s| s.kind == LiquiditySignalKind::CascadeSustained)
        .collect();
    assert_eq!(
        sustained.len(),
        1,
        "expected exactly one sustained-cascade signal"
    );
    assert_eq!(sustained[0].direction, LiquidityDirection::Bearish);
    assert!(sustained[0].strength > 0.0);
    assert!(sustained[0].strength <= 100.0);
    assert!(!sustained[0].evidence.is_empty());
}

#[test]
fn detected_cascade_uses_correct_direction_for_short_squeeze() {
    let flow = LiquidityFlow {
        cascade_state: CascadeState::Detected,
        net_liquidation_usd: -500_000.0, // shorts got dumped
        event_count: 3,
        cascade_intensity: 60.0,
        ..empty_flow(CascadeState::Detected)
    };
    let input = SignalInput {
        flow: Some(&flow),
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let det = sigs
        .iter()
        .find(|s| s.kind == LiquiditySignalKind::CascadeDetected)
        .unwrap();
    assert_eq!(det.direction, LiquidityDirection::Bullish);
}

#[test]
fn exhausted_cascade_emits_neutral_signal() {
    let flow = LiquidityFlow {
        cascade_state: CascadeState::Exhausted,
        cascade_intensity: 40.0,
        ..empty_flow(CascadeState::Exhausted)
    };
    let input = SignalInput {
        flow: Some(&flow),
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let exh = sigs
        .iter()
        .find(|s| s.kind == LiquiditySignalKind::CascadeExhausted);
    assert!(exh.is_some());
    assert_eq!(exh.unwrap().direction, LiquidityDirection::Neutral);
}

#[test]
fn multiple_signals_can_coexist() {
    let flow = LiquidityFlow {
        cascade_state: CascadeState::Sustained,
        net_liquidation_usd: 1_000_000.0,
        cascade_intensity: 80.0,
        event_count: 3,
        ..empty_flow(CascadeState::Sustained)
    };
    // OI up + funding down (shorts loading) is a divergence.
    let input = SignalInput {
        flow: Some(&flow),
        funding_rate: -0.001, // extreme negative (shorts getting paid)
        oi_delta_pct: 3.0,    // OI up sharply
        funding_extreme_pct: 0.0005,
        oi_funding_divergence_pct: 2.0,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let kinds: std::collections::HashSet<_> = sigs.iter().map(|s| s.kind).collect();
    assert!(
        kinds.len() >= 3,
        "expected ≥3 distinct signals, got {:?}",
        kinds
    );
}

#[test]
fn signal_evidence_strings_are_non_empty() {
    let flow = LiquidityFlow {
        cascade_state: CascadeState::Sustained,
        cascade_intensity: 70.0,
        net_liquidation_usd: 100_000.0,
        event_count: 3,
        largest_event_usd: 50_000.0,
        ..empty_flow(CascadeState::Sustained)
    };
    let input = SignalInput {
        flow: Some(&flow),
        funding_rate: 0.001,
        funding_extreme_pct: 0.0005,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    for s in &sigs {
        assert!(
            !s.evidence.is_empty(),
            "signal {:?} must have evidence",
            s.kind
        );
        assert!(
            s.confidence > 0.0 && s.confidence <= 1.0,
            "confidence out of range: {}",
            s.confidence
        );
    }
}

#[test]
fn all_signal_kinds_have_display() {
    use std::string::ToString;
    for kind in [
        LiquiditySignalKind::CascadeDetected,
        LiquiditySignalKind::CascadeSustained,
        LiquiditySignalKind::CascadeExhausted,
        LiquiditySignalKind::LiquidityVacuum,
        LiquiditySignalKind::FundingExtreme,
        LiquiditySignalKind::OIFundingDivergence,
        LiquiditySignalKind::MagnetActivated,
        LiquiditySignalKind::ClusterPressureHigh,
        LiquiditySignalKind::ClusterForwardPressure,
        LiquiditySignalKind::FundingFlip,
        LiquiditySignalKind::OiPriceDivergence,
    ] {
        let s = kind.to_string();
        assert!(!s.is_empty());
        // SCREAMING_SNAKE_CASE convention.
        assert!(
            s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "kind {:?} did not serialize to SCREAMING_SNAKE_CASE: {}",
            kind,
            s
        );
    }
}

#[test]
fn magnet_activates_only_near_close_cluster() {
    use core_domain::liquidity::{
        ClusterKind, LeverageAssumptions, LeverageDistributionSource, LiquidationCluster,
        LiquidationClusterMatrix,
    };
    let cluster_near = LiquidationClusterMatrix {
        symbol: "BTC".to_string(),
        generated_at_ms: 0,
        valid_until_ms: 0,
        mid_price: 50_000.0,
        leverage_assumptions: LeverageAssumptions {
            buckets: vec![],
            weights: vec![],
            funding_modulation_active: false,
            funding_extreme_pct: 0.0,
            source: LeverageDistributionSource::DefaultPowerLaw,
        },
        short_clusters: vec![],
        long_clusters: vec![LiquidationCluster {
            price_low: 49_900.0,
            price_high: 50_000.0,
            peak_price: 49_950.0,
            notional_usd: 1_000_000.0,
            dominant_leverage: 10,
            distance_from_mid_pct: 0.1, // very close
            cluster_kind: ClusterKind::BelowCurrentPrice,
            magnet_strength: 90.0,
        }],
        cascade_asymmetry: 0.0,
        total_long_oi_usd: 0.0,
        total_short_oi_usd: 0.0,
        estimation_confidence: 0.9,
    };
    let input = SignalInput {
        cluster: Some(&cluster_near),
        magnet_activation_distance_pct: 0.5,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    assert!(sigs
        .iter()
        .any(|s| s.kind == LiquiditySignalKind::MagnetActivated));
}

#[test]
fn magnet_does_not_activate_for_far_cluster() {
    use core_domain::liquidity::{
        ClusterKind, LeverageAssumptions, LeverageDistributionSource, LiquidationCluster,
        LiquidationClusterMatrix,
    };
    let cluster_far = LiquidationClusterMatrix {
        symbol: "BTC".to_string(),
        generated_at_ms: 0,
        valid_until_ms: 0,
        mid_price: 50_000.0,
        leverage_assumptions: LeverageAssumptions {
            buckets: vec![],
            weights: vec![],
            funding_modulation_active: false,
            funding_extreme_pct: 0.0,
            source: LeverageDistributionSource::DefaultPowerLaw,
        },
        short_clusters: vec![LiquidationCluster {
            price_low: 60_000.0,
            price_high: 60_100.0,
            peak_price: 60_050.0,
            notional_usd: 1_000_000.0,
            dominant_leverage: 10,
            distance_from_mid_pct: 20.0, // far away
            cluster_kind: ClusterKind::AboveCurrentPrice,
            magnet_strength: 10.0,
        }],
        long_clusters: vec![],
        cascade_asymmetry: 0.0,
        total_long_oi_usd: 0.0,
        total_short_oi_usd: 0.0,
        estimation_confidence: 0.9,
    };
    let input = SignalInput {
        cluster: Some(&cluster_far),
        magnet_activation_distance_pct: 0.5,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    assert!(
        !sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::MagnetActivated),
        "far cluster should not trigger magnet signal"
    );
}

#[test]
fn magnet_ignores_low_notional_cluster() {
    use core_domain::liquidity::{
        ClusterKind, LeverageAssumptions, LeverageDistributionSource, LiquidationCluster,
        LiquidationClusterMatrix,
    };
    let cluster_tiny = LiquidationClusterMatrix {
        symbol: "BTC".to_string(),
        generated_at_ms: 0,
        valid_until_ms: 0,
        mid_price: 50_000.0,
        leverage_assumptions: LeverageAssumptions {
            buckets: vec![],
            weights: vec![],
            funding_modulation_active: false,
            funding_extreme_pct: 0.0,
            source: LeverageDistributionSource::DefaultPowerLaw,
        },
        short_clusters: vec![],
        long_clusters: vec![LiquidationCluster {
            price_low: 49_900.0,
            price_high: 50_000.0,
            peak_price: 49_950.0,
            notional_usd: 10_000.0, // < 100k threshold
            dominant_leverage: 10,
            distance_from_mid_pct: 0.1,
            cluster_kind: ClusterKind::BelowCurrentPrice,
            magnet_strength: 50.0,
        }],
        cascade_asymmetry: 0.0,
        total_long_oi_usd: 0.0,
        total_short_oi_usd: 0.0,
        estimation_confidence: 0.9,
    };
    let input = SignalInput {
        cluster: Some(&cluster_tiny),
        magnet_activation_distance_pct: 0.5,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    assert!(
        !sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::MagnetActivated),
        "tiny cluster should not trigger magnet signal"
    );
}

#[test]
fn cluster_pressure_high_direction_follows_canonical_asymmetry_sign() {
    // Canonical sign interpretation (02-13 §Cascade asymmetry v2.1):
    // positive asymmetry = more short notional above mid = short squeeze
    // risk (price likely to rally) = Bullish; negative = long squeeze
    // risk = Bearish. Regression-locks the AUDIT-V4-029/audit-2026 fix.
    use core_domain::liquidity::{
        ClusterKind, LeverageAssumptions, LeverageDistributionSource, LiquidationCluster,
        LiquidationClusterMatrix,
    };

    let cluster = |asym: f64, short_cluster: bool| LiquidationClusterMatrix {
        symbol: "BTC".to_string(),
        generated_at_ms: 0,
        valid_until_ms: 0,
        mid_price: 50_000.0,
        leverage_assumptions: LeverageAssumptions {
            buckets: vec![],
            weights: vec![],
            funding_modulation_active: false,
            funding_extreme_pct: 0.0,
            source: LeverageDistributionSource::DefaultPowerLaw,
        },
        short_clusters: if short_cluster {
            vec![LiquidationCluster {
                price_low: 50_100.0,
                price_high: 50_200.0,
                peak_price: 50_150.0,
                notional_usd: 2_000_000.0,
                dominant_leverage: 10,
                distance_from_mid_pct: 0.3,
                cluster_kind: ClusterKind::AboveCurrentPrice,
                magnet_strength: 60.0,
            }]
        } else {
            vec![]
        },
        long_clusters: if short_cluster {
            vec![]
        } else {
            vec![LiquidationCluster {
                price_low: 49_800.0,
                price_high: 49_900.0,
                peak_price: 49_850.0,
                notional_usd: 2_000_000.0,
                dominant_leverage: 10,
                distance_from_mid_pct: 0.3,
                cluster_kind: ClusterKind::BelowCurrentPrice,
                magnet_strength: 60.0,
            }]
        },
        cascade_asymmetry: asym,
        total_long_oi_usd: 0.0,
        total_short_oi_usd: 0.0,
        estimation_confidence: 0.9,
    };

    // Positive asymmetry (short squeeze risk) → Bullish.
    let input = SignalInput {
        cluster: Some(&cluster(0.6, true)),
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let pressure: Vec<_> = sigs
        .iter()
        .filter(|s| s.kind == LiquiditySignalKind::ClusterPressureHigh)
        .collect();
    assert_eq!(pressure.len(), 1, "expected ClusterPressureHigh");
    assert_eq!(
        pressure[0].direction,
        LiquidityDirection::Bullish,
        "positive asymmetry = short squeeze risk = Bullish"
    );

    // Negative asymmetry (long squeeze risk) → Bearish.
    let input = SignalInput {
        cluster: Some(&cluster(-0.6, false)),
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let pressure: Vec<_> = sigs
        .iter()
        .filter(|s| s.kind == LiquiditySignalKind::ClusterPressureHigh)
        .collect();
    assert_eq!(pressure.len(), 1, "expected ClusterPressureHigh");
    assert_eq!(
        pressure[0].direction,
        LiquidityDirection::Bearish,
        "negative asymmetry = long squeeze risk = Bearish"
    );
}

#[test]
fn magnet_activated_direction_follows_canonical_cluster_semantics() {
    // Canonical (02-13): an above-mid cluster is a short-liq zone — a rally
    // into it forces buy-to-cover (short squeeze) = Bullish; a below-mid
    // long-liq zone drags price down = Bearish. Regression-locks the
    // MagnetActivated fix (previously inverted).
    use core_domain::liquidity::{
        ClusterKind, LeverageAssumptions, LeverageDistributionSource, LiquidationCluster,
        LiquidationClusterMatrix,
    };

    let cluster_with = |kind: ClusterKind, notional: f64, dist: f64| LiquidationClusterMatrix {
        symbol: "BTC".to_string(),
        generated_at_ms: 0,
        valid_until_ms: 0,
        mid_price: 50_000.0,
        leverage_assumptions: LeverageAssumptions {
            buckets: vec![],
            weights: vec![],
            funding_modulation_active: false,
            funding_extreme_pct: 0.0,
            source: LeverageDistributionSource::DefaultPowerLaw,
        },
        short_clusters: if matches!(kind, ClusterKind::AboveCurrentPrice) {
            vec![LiquidationCluster {
                price_low: 50_100.0,
                price_high: 50_200.0,
                peak_price: 50_150.0,
                notional_usd: notional,
                dominant_leverage: 10,
                distance_from_mid_pct: dist,
                cluster_kind: kind,
                magnet_strength: 80.0,
            }]
        } else {
            vec![]
        },
        long_clusters: if matches!(kind, ClusterKind::BelowCurrentPrice) {
            vec![LiquidationCluster {
                price_low: 49_800.0,
                price_high: 49_900.0,
                peak_price: 49_850.0,
                notional_usd: notional,
                dominant_leverage: 10,
                distance_from_mid_pct: dist,
                cluster_kind: kind,
                magnet_strength: 80.0,
            }]
        } else {
            vec![]
        },
        cascade_asymmetry: 0.0,
        total_long_oi_usd: 0.0,
        total_short_oi_usd: 0.0,
        estimation_confidence: 0.9,
    };

    // Above-mid short-liq zone (short squeeze) → Bullish.
    let input = SignalInput {
        cluster: Some(&cluster_with(
            ClusterKind::AboveCurrentPrice,
            2_000_000.0,
            0.3,
        )),
        magnet_activation_distance_pct: 0.5,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let mag: Vec<_> = sigs
        .iter()
        .filter(|s| s.kind == LiquiditySignalKind::MagnetActivated)
        .collect();
    assert_eq!(mag.len(), 1, "expected MagnetActivated");
    assert_eq!(
        mag[0].direction,
        LiquidityDirection::Bullish,
        "above-mid short-liq cluster = short squeeze = Bullish"
    );

    // Below-mid long-liq zone (long squeeze) → Bearish.
    let input = SignalInput {
        cluster: Some(&cluster_with(
            ClusterKind::BelowCurrentPrice,
            2_000_000.0,
            0.3,
        )),
        magnet_activation_distance_pct: 0.5,
        ..Default::default()
    };
    let sigs = derive_liquidity_signals(&input);
    let mag: Vec<_> = sigs
        .iter()
        .filter(|s| s.kind == LiquiditySignalKind::MagnetActivated)
        .collect();
    assert_eq!(mag.len(), 1, "expected MagnetActivated");
    assert_eq!(
        mag[0].direction,
        LiquidityDirection::Bearish,
        "below-mid long-liq cluster = long squeeze = Bearish"
    );
}
