//! # State Matrix — System-Wide Aggregation
//!
//! The State Matrix collects all Analysis Matrices and instance metadata
//! across active symbols to produce a global dashboard summary. It answers:
//! *what is the state of the entire Trading Platform right now?*
//!
//! Layer: L5.5 in the architecture (Market Synthesis).

use crate::analysis::{AnalysisMatrix, MarketBias};
use serde::{Deserialize, Serialize};

/// Per-symbol summary within the system-wide State Matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolSummary {
    pub symbol: String,
    pub bias: MarketBias,
    pub confidence: f64,
    pub mtf_overall_score: f64,
    pub timeframes_present: u8,
    pub regime: String,
    pub supporting_signals_count: u32,
    pub contradicting_signals_count: u32,
}

/// System-wide State Matrix aggregating all instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateMatrix {
    pub instance_count: u32,
    pub active_symbols: Vec<String>,
    pub total_timeframes_active: u32,
    pub regime_distribution: std::collections::HashMap<String, u32>,
    pub global_bias_label: String,
    pub per_symbol_summary: Vec<SymbolSummary>,
    pub active_signals_total: u32,
}

impl StateMatrix {
    /// Create an empty StateMatrix for when no instances are active.
    pub fn empty() -> Self {
        Self {
            instance_count: 0,
            active_symbols: Vec::new(),
            total_timeframes_active: 0,
            regime_distribution: std::collections::HashMap::new(),
            global_bias_label: "NO_DATA".to_string(),
            per_symbol_summary: Vec::new(),
            active_signals_total: 0,
        }
    }

    /// Compute the global bias label from all per-symbol decisions.
    fn compute_global_bias(summaries: &[SymbolSummary]) -> String {
        if summaries.is_empty() {
            return "NO_DATA".to_string();
        }

        let total = summaries.len() as f64;
        let bullish: f64 = summaries
            .iter()
            .filter(|s| s.bias == MarketBias::Bullish)
            .count() as f64;
        let bearish: f64 = summaries
            .iter()
            .filter(|s| s.bias == MarketBias::Bearish)
            .count() as f64;
        let neutral: f64 = summaries
            .iter()
            .filter(|s| s.bias == MarketBias::Neutral)
            .count() as f64;

        let bull_pct = bullish / total;
        let bear_pct = bearish / total;
        let neutral_pct = neutral / total;

        if bull_pct >= 0.6 {
            "BULLISH".into()
        } else if bear_pct >= 0.6 {
            "BEARISH".into()
        } else if neutral_pct >= 0.6 {
            "NEUTRAL".into()
        } else if bull_pct > bear_pct {
            "MIXED_LEAN_BULL".into()
        } else if bear_pct > bull_pct {
            "MIXED_LEAN_BEAR".into()
        } else {
            "MIXED".into()
        }
    }
}

/// A minimal per-instance metadata record used as input to compute_state.
/// Passed from the engine analyzer to identify what's active.
#[derive(Debug, Clone)]
pub struct InstanceMeta {
    pub symbol: String,
    pub timeframe_secs: u64,
    pub timeframe_label: String,
    pub is_active: bool,
}

/// Compute the system-wide State Matrix from per-symbol Decision Matrices
/// and active instance metadata.
pub fn compute_state(decisions: &[AnalysisMatrix], instances: &[InstanceMeta]) -> StateMatrix {
    if instances.is_empty() && decisions.is_empty() {
        return StateMatrix::empty();
    }

    let active_instances: Vec<&InstanceMeta> = instances.iter().filter(|i| i.is_active).collect();
    let instance_count = active_instances.len() as u32;

    // Unique symbols from active instances
    let mut symbols_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for inst in &active_instances {
        symbols_set.insert(inst.symbol.clone());
    }

    // Also include symbols from decisions
    for d in decisions {
        symbols_set.insert(d.symbol.clone());
    }

    let mut active_symbols: Vec<String> = symbols_set.into_iter().collect();
    active_symbols.sort();

    // Sum of active timeframes
    let total_timeframes_active = active_instances.len() as u32;

    // Regime distribution: collect from decision contexts
    let mut regime_dist: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    // Regime info is embedded in decision's supporting/contradicting lines.
    // Extract from supporting signals which contain regime info like "TRENDING" or "RANGE".
    for d in decisions {
        for line in &d.supporting_signals {
            for regime in &["TRENDING", "RANGE", "EXPANSION", "COMPRESSION"] {
                if line.contains(regime) {
                    *regime_dist.entry(regime.to_string()).or_insert(0) += 1;
                    break;
                }
            }
        }
        for line in &d.contradicting_signals {
            for regime in &["TRENDING", "RANGE", "EXPANSION", "COMPRESSION"] {
                if line.contains(regime) {
                    *regime_dist.entry(regime.to_string()).or_insert(0) += 1;
                    break;
                }
            }
        }
    }

    // Per-symbol summaries from decisions
    let mut per_symbol_summary: Vec<SymbolSummary> = Vec::new();
    let mut active_signals_total: u32 = 0;

    for d in decisions {
        // Determine dominant regime for this symbol from TF alignments
        // (best-effort from supporting/contradicting signal strings)
        let regime = if d.supporting_signals.iter().any(|s| s.contains("TRENDING")) {
            "TRENDING"
        } else if d.supporting_signals.iter().any(|s| s.contains("RANGE")) {
            "RANGE"
        } else if d.supporting_signals.iter().any(|s| s.contains("EXPANSION")) {
            "EXPANSION"
        } else if d
            .supporting_signals
            .iter()
            .any(|s| s.contains("COMPRESSION"))
        {
            "COMPRESSION"
        } else {
            "UNKNOWN"
        };

        active_signals_total += d.supporting_signals.len() as u32;
        active_signals_total += d.contradicting_signals.len() as u32;

        per_symbol_summary.push(SymbolSummary {
            symbol: d.symbol.clone(),
            bias: d.bias,
            confidence: d.state_confidence,
            mtf_overall_score: 0.0, // score is not directly available here; populated by engine
            timeframes_present: d.timeframes_considered,
            regime: regime.to_string(),
            supporting_signals_count: d.supporting_signals.len() as u32,
            contradicting_signals_count: d.contradicting_signals.len() as u32,
        });
    }

    // If no decisions but we have active instances, populate from instance metadata
    if per_symbol_summary.is_empty() && !active_symbols.is_empty() {
        for sym in &active_symbols {
            per_symbol_summary.push(SymbolSummary {
                symbol: sym.clone(),
                bias: MarketBias::Neutral,
                confidence: 0.0,
                mtf_overall_score: 0.0,
                timeframes_present: 0,
                regime: "UNKNOWN".to_string(),
                supporting_signals_count: 0,
                contradicting_signals_count: 0,
            });
        }
    }

    let global_bias_label = StateMatrix::compute_global_bias(&per_symbol_summary);

    StateMatrix {
        instance_count,
        active_symbols,
        total_timeframes_active,
        regime_distribution: regime_dist,
        global_bias_label,
        per_symbol_summary,
        active_signals_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        MarketPhase, MarketRegime, MomentumAssessment, QualityLevel, StructureAssessment,
        TrendAssessment, VolatilityAssessment, VolumeAssessment,
    };

    fn sample_decision(symbol: &str, bias: MarketBias, confidence: f64, tfs: u8) -> AnalysisMatrix {
        AnalysisMatrix {
            symbol: symbol.to_string(),
            bias,
            // v6.10.18 (I-1): the wire unit is the FRACTION `score/100`
            // (docs 02-02 §2.1) — the legacy `confidence * 100.0` sample
            // fed `bias_lifted` a 0–100 value and silently marked every
            // directional sample as margin-lifted.
            market_bias_score: confidence,
            state_confidence: confidence,
            confidence,
            market_quality_score: 50.0,
            trend_score: None,
            momentum_score: None,
            structure_score: None,
            volatility_score: None,
            volume_score: None,
            representative_bbwp: None,
            representative_adx: None,
            market_regime: MarketRegime::TrendingBull,
            trend_assessment: TrendAssessment::Strong,
            momentum_assessment: MomentumAssessment::Increasing,
            structure_assessment: StructureAssessment::Strong,
            volatility_assessment: VolatilityAssessment::Normal,
            volume_assessment: VolumeAssessment::Strong,
            market_quality: QualityLevel::Good,
            market_phase: MarketPhase::Unknown,
            market_interpretation: format!("Test interpretation for {}", symbol),
            rationale: format!("MTF test for {}", symbol),
            supporting_signals: vec![
                "3m (bullish): score +72, TRENDING regime, 2 active signals".into(),
                "macro900 (bullish): score +85, TRENDING regime, 1 active signals".into(),
            ],
            contradicting_signals: vec![
                "micro60 (bearish): score -15, RANGE regime, 0 active signals".into(),
            ],
            timeframes_considered: tfs,
        }
    }

    fn sample_instance(symbol: &str) -> Vec<InstanceMeta> {
        vec![
            InstanceMeta {
                symbol: symbol.into(),
                timeframe_secs: 60,
                timeframe_label: "micro60".into(),
                is_active: true,
            },
            InstanceMeta {
                symbol: symbol.into(),
                timeframe_secs: 180,
                timeframe_label: "3m".into(),
                is_active: true,
            },
            InstanceMeta {
                symbol: symbol.into(),
                timeframe_secs: 300,
                timeframe_label: "slow300".into(),
                is_active: true,
            },
            InstanceMeta {
                symbol: symbol.into(),
                timeframe_secs: 900,
                timeframe_label: "macro900".into(),
                is_active: true,
            },
        ]
    }

    #[test]
    fn empty_inputs_produce_empty_state() {
        let s = compute_state(&[], &[]);
        assert_eq!(s.instance_count, 0);
        assert_eq!(s.global_bias_label, "NO_DATA");
    }

    #[test]
    fn single_bullish_symbol_produces_bullish_state() {
        let decisions = vec![sample_decision("BTC-USD", MarketBias::Bullish, 0.85, 4)];
        let instances = sample_instance("BTC-USD");
        let s = compute_state(&decisions, &instances);
        assert_eq!(s.instance_count, 4);
        assert_eq!(s.active_symbols.len(), 1);
        assert_eq!(s.global_bias_label, "BULLISH");
    }

    #[test]
    fn mixed_symbols_produces_mixed_state() {
        let decisions = vec![
            sample_decision("BTC-USD", MarketBias::Bullish, 0.85, 4),
            sample_decision("ETH-USD", MarketBias::Bearish, 0.70, 4),
        ];
        let mut instances = sample_instance("BTC-USD");
        instances.extend(sample_instance("ETH-USD"));
        let s = compute_state(&decisions, &instances);
        assert_eq!(s.instance_count, 8);
        assert_eq!(s.active_symbols.len(), 2);
        assert!(s.global_bias_label.contains("MIXED"));
    }
}
