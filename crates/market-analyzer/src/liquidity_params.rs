//! # Effective liquidity runtime parameters (v9)
//!
//! The strategy's `l1_5` section is the single source of truth for the
//! derivatives/liquidity pipeline. This resolver overlays it onto the
//! legacy `[workspace.liquidity]` / `[workspace.heatmap]` sections (which
//! stay only as a pre-v9 parse fallback) and carries the strategy-only
//! knobs (signal weights, accumulator, api failover, per-TF leverage) to
//! the consumers.

use config_models::{
    HeatmapConfig, L1_5AccumulatorParams, L1_5FailoverParams, L1_5Params, L1_5TfLeverageParams,
    LiquidityConfig,
};

/// The resolved liquidity configuration for one instance.
#[derive(Debug, Clone)]
pub struct EffectiveLiquidity {
    /// `[workspace.liquidity]` shape with every strategy `l1_5` override
    /// applied (strategy wins per-field).
    pub cfg: LiquidityConfig,
    /// Heatmap bucketing (legacy workspace section; the strategy does not
    /// carry bucketing — kept as-is when provided).
    pub heatmap: Option<HeatmapConfig>,
    /// The 11-kind trust axis (default 1.0 each).
    pub signal_weights: std::collections::HashMap<String, f64>,
    pub accumulator: L1_5AccumulatorParams,
    pub api_failover: L1_5FailoverParams,
    pub per_tf_leverage: L1_5TfLeverageParams,
}

impl EffectiveLiquidity {
    pub fn kind_weight(&self, kind: &str) -> f64 {
        self.signal_weights.get(kind).copied().unwrap_or(1.0)
    }
}

/// Overlay the strategy's `l1_5` section onto the legacy workspace config.
pub fn effective_liquidity(
    base: Option<&LiquidityConfig>,
    heatmap: Option<&HeatmapConfig>,
    l1_5: &L1_5Params,
) -> EffectiveLiquidity {
    let mut cfg = base.cloned().unwrap_or_default();
    // v9: strategy wins per-field (single source of truth). The legacy
    // workspace section only fills fields the strategy JSON never carried
    // — today every field exists on L1_5, so the strategy wins outright.
    cfg.enabled = l1_5.enabled;
    cfg.liquidation_feed = l1_5.liquidation_feed;
    cfg.cluster_estimation = l1_5.cluster_estimation;
    cfg.signals = l1_5.signals;
    cfg.mark_price_poll_ms = l1_5.mark_price_poll_ms;
    cfg.event_retention_days = l1_5.event_retention_days;
    cfg.bucket_retention_days = l1_5.bucket_retention_days;
    cfg.cluster_refresh_secs = l1_5.cluster_refresh_secs;
    cfg.maintenance_margin_rate = l1_5.maintenance_margin_rate;
    cfg.cascade_detected_zscore = l1_5.cascade_detected_zscore;
    cfg.cascade_sustained_events = l1_5.cascade_sustained_events;
    cfg.funding_extreme_pct = l1_5.funding_extreme_pct;
    cfg.magnet_activation_distance_pct = l1_5.magnet_activation_distance_pct;
    cfg.liquidity_vacuum_threshold = l1_5.liquidity_vacuum_threshold;
    cfg.oi_funding_divergence_pct = l1_5.oi_funding_divergence_pct;
    cfg.min_cluster_notional_usd = l1_5.min_cluster_notional_usd;
    cfg.signal_confidences = config_models::LiquiditySignalConfidences {
        cascade_detected: l1_5
            .signal_confidences
            .get("cascade_detected")
            .copied()
            .unwrap_or(cfg.signal_confidences.cascade_detected),
        cascade_sustained: l1_5
            .signal_confidences
            .get("cascade_sustained")
            .copied()
            .unwrap_or(cfg.signal_confidences.cascade_sustained),
        cascade_exhausted: l1_5
            .signal_confidences
            .get("cascade_exhausted")
            .copied()
            .unwrap_or(cfg.signal_confidences.cascade_exhausted),
        funding_extreme: l1_5
            .signal_confidences
            .get("funding_extreme")
            .copied()
            .unwrap_or(cfg.signal_confidences.funding_extreme),
        oi_funding_divergence: l1_5
            .signal_confidences
            .get("oi_funding_divergence")
            .copied()
            .unwrap_or(cfg.signal_confidences.oi_funding_divergence),
        liquidity_vacuum: l1_5
            .signal_confidences
            .get("liquidity_vacuum")
            .copied()
            .unwrap_or(cfg.signal_confidences.liquidity_vacuum),
        funding_flip: l1_5
            .signal_confidences
            .get("funding_flip")
            .copied()
            .unwrap_or(cfg.signal_confidences.funding_flip),
        oi_price_divergence: l1_5
            .signal_confidences
            .get("oi_price_divergence")
            .copied()
            .unwrap_or(cfg.signal_confidences.oi_price_divergence),
    };
    EffectiveLiquidity {
        cfg,
        heatmap: heatmap.cloned(),
        signal_weights: l1_5.signal_weights.clone(),
        accumulator: l1_5.accumulator.clone(),
        api_failover: l1_5.api_failover.clone(),
        per_tf_leverage: l1_5.per_tf_leverage.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_wins_per_field() {
        let base = LiquidityConfig {
            funding_extreme_pct: 0.001,
            ..Default::default()
        };
        let l1_5 = L1_5Params {
            funding_extreme_pct: 0.002,
            cascade_detected_zscore: 3.0,
            ..Default::default()
        };
        let eff = effective_liquidity(Some(&base), None, &l1_5);
        assert_eq!(eff.cfg.funding_extreme_pct, 0.002);
        assert_eq!(eff.cfg.cascade_detected_zscore, 3.0);
        // untouched field keeps the legacy value
        assert_eq!(eff.cfg.mark_price_poll_ms, l1_5.mark_price_poll_ms);
    }

    #[test]
    fn no_base_falls_back_to_strategy_only() {
        let eff = effective_liquidity(None, None, &L1_5Params::default());
        assert!(eff.cfg.enabled);
        assert_eq!(eff.cfg.funding_extreme_pct, 0.0005);
        assert!(eff.signal_weights.is_empty());
        assert_eq!(eff.kind_weight("CascadeDetected"), 1.0);
    }

    #[test]
    fn kind_weight_reads_trust_axis() {
        let mut l1_5 = L1_5Params::default();
        l1_5.signal_weights.insert("FundingFlip".into(), 2.0);
        let eff = effective_liquidity(None, None, &l1_5);
        assert_eq!(eff.kind_weight("FundingFlip"), 2.0);
        assert_eq!(eff.kind_weight("Unknown"), 1.0);
    }
}
