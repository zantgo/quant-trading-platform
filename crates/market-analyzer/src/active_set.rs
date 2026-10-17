use std::collections::HashSet;

use config_models::ActivationConfig;
use core_domain::indicator_dtos::{NormalizedIndicatorValue, SignalKind};

/// Wire name of a `SignalKind` — matches the serde variant spelling
/// (`"Divergence"`, `"Threshold"`, ...) used in `disabled_signal_kinds`
/// and `disabled_signals` config entries (03-02-12 CA-02).
fn signal_kind_name(kind: SignalKind) -> &'static str {
    match kind {
        SignalKind::Divergence => "Divergence",
        SignalKind::Crossover => "Crossover",
        SignalKind::Threshold => "Threshold",
        SignalKind::Breakout => "Breakout",
        SignalKind::BandTouch => "BandTouch",
        SignalKind::ZeroLineCross => "ZeroLineCross",
        SignalKind::CompressionRelease => "CompressionRelease",
        SignalKind::LevelTest => "LevelTest",
        SignalKind::TrendFlip => "TrendFlip",
        SignalKind::VolumeClimax => "VolumeClimax",
        SignalKind::StackChange => "StackChange",
        SignalKind::PatternForming => "PatternForming",
    }
}

#[derive(Debug, Clone)]
pub struct ActiveSet {
    pub disabled_indicators: HashSet<String>,
    pub disabled_signals: HashSet<(String, String)>,
    pub disabled_signal_kinds: HashSet<String>,
    pub config_version: u64,
}

impl ActiveSet {
    pub fn all_enabled() -> Self {
        Self {
            disabled_indicators: HashSet::new(),
            disabled_signals: HashSet::new(),
            disabled_signal_kinds: HashSet::new(),
            config_version: 1,
        }
    }

    pub fn from_config(
        global: &ActivationConfig,
        instance: Option<&ActivationConfig>,
        config_version: u64,
    ) -> Self {
        let mut disabled_indicators: HashSet<String> =
            global.disabled_indicators.iter().cloned().collect();
        let mut disabled_signals: HashSet<(String, String)> = global
            .disabled_signals
            .iter()
            .map(|s| {
                let parts: Vec<&str> = s.splitn(2, ':').collect();
                (
                    parts[0].to_string(),
                    parts.get(1).map(|p| p.to_string()).unwrap_or_default(),
                )
            })
            .collect();
        let mut disabled_signal_kinds: HashSet<String> =
            global.disabled_signal_kinds.iter().cloned().collect();

        if let Some(inst) = instance {
            for ind in &inst.disabled_indicators {
                disabled_indicators.insert(ind.clone());
            }
            for sig in &inst.disabled_signals {
                let parts: Vec<&str> = sig.splitn(2, ':').collect();
                disabled_signals.insert((
                    parts[0].to_string(),
                    parts.get(1).map(|p| p.to_string()).unwrap_or_default(),
                ));
            }
            for kind in &inst.disabled_signal_kinds {
                disabled_signal_kinds.insert(kind.clone());
            }
        }

        Self {
            disabled_indicators,
            disabled_signals,
            disabled_signal_kinds,
            config_version,
        }
    }

    pub fn is_indicator_enabled(&self, key: &str) -> bool {
        !self.disabled_indicators.contains(key)
    }

    pub fn is_signal_kind_enabled(&self, kind: &str) -> bool {
        !self.disabled_signal_kinds.contains(kind)
    }

    pub fn is_signal_pair_enabled(&self, indicator: &str, signal_kind: &str) -> bool {
        !self
            .disabled_signals
            .contains(&(indicator.to_string(), signal_kind.to_string()))
    }

    /// AUDIT-H2: whether a concrete `SignalKind` emitted by `indicator` is
    /// allowed by the denylist. Allocation-free — compares the kind's wire
    /// name directly against the config sets.
    pub fn is_signal_allowed(&self, indicator: &str, kind: SignalKind) -> bool {
        let name = signal_kind_name(kind);
        if self.disabled_signal_kinds.contains(name) {
            return false;
        }
        if self
            .disabled_signals
            .iter()
            .any(|(ind, k)| ind == indicator && k == name)
        {
            return false;
        }
        true
    }

    /// AUDIT-H2: filter every indicator entry's signal vector against the
    /// denylist. Applied at every snapshot build (live + warm + shadow) so
    /// disabled kinds and (indicator, kind) pairs never reach the wire or
    /// downstream consumers. Previously the denylist was parsed but never
    /// enforced — operators disabling e.g. `VolumeClimax` still received
    /// the signal in every snapshot while `metrics_config` advertised it
    /// as disabled.
    pub fn filter_map_signals(
        &self,
        map: &mut std::collections::HashMap<String, NormalizedIndicatorValue>,
    ) {
        if self.disabled_signal_kinds.is_empty() && self.disabled_signals.is_empty() {
            return;
        }
        for (key, entry) in map.iter_mut() {
            entry
                .signals
                .retain(|sig| self.is_signal_allowed(key, sig.kind));
        }
    }

    /// AUDIT-H2/M2: filter a snapshot's indicator map against the denylist —
    /// disabled indicators AND disabled signals. Warm-seeded snapshots are
    /// built all-enabled (the bootstrap intentionally warms every
    /// calculator); this reconciles them with the instance's active set
    /// before they are served via the WS bootstrap replay and `/api/history`.
    pub fn filter_snapshot_indicators(&self, snap: &mut core_domain::models::MarketSnapshot) {
        snap.indicators
            .retain(|key, _| self.is_indicator_enabled(key));
        self.filter_map_signals(&mut snap.indicators);
    }

    pub fn has_any_disabled(&self) -> bool {
        !self.disabled_indicators.is_empty()
            || !self.disabled_signals.is_empty()
            || !self.disabled_signal_kinds.is_empty()
    }

    pub fn to_metrics_config(&self) -> Option<core_domain::models::MetricsConfig> {
        if !self.has_any_disabled() {
            return None;
        }
        Some(core_domain::models::MetricsConfig {
            disabled_indicators: self.disabled_indicators.iter().cloned().collect(),
            disabled_signals: self
                .disabled_signals
                .iter()
                .map(|(ind, kind)| (ind.clone(), kind.clone()))
                .collect(),
            disabled_signal_kinds: self.disabled_signal_kinds.iter().cloned().collect(),
            config_version: self.config_version,
        })
    }
}

impl Default for ActiveSet {
    fn default() -> Self {
        Self::all_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_enabled_has_no_disabled_sets() {
        let s = ActiveSet::all_enabled();
        assert!(s.disabled_indicators.is_empty());
        assert!(s.disabled_signals.is_empty());
        assert!(s.disabled_signal_kinds.is_empty());
    }

    #[test]
    fn default_matches_all_enabled() {
        let s = ActiveSet::default();
        assert_eq!(s.config_version, ActiveSet::all_enabled().config_version);
        assert!(s.disabled_indicators.is_empty());
    }

    #[test]
    fn has_any_disabled_returns_true_when_indicator_listed() {
        let mut s = ActiveSet::all_enabled();
        s.disabled_indicators.insert("rsi".to_string());
        assert!(s.has_any_disabled());
    }

    // CR-10: `liquidity_master_off_reaches_metrics_config` is DELETED. It
    // proved that the `[liquidity] enabled` master switch reached
    // `MetricsConfig`, which is the field that switch no longer has. The
    // per-feed activation model is gone with the chain it gated.

    fn map_with_signals() -> std::collections::HashMap<String, NormalizedIndicatorValue> {
        use core_domain::indicator_dtos::{IndicatorSignal, SignalDirection, SignalStatus};
        let mut m = std::collections::HashMap::new();
        let mut rsi = NormalizedIndicatorValue::scalar(70.0, 0.5, "OVERBOUGHT");
        rsi.signals.push(IndicatorSignal::new(
            SignalKind::Threshold,
            SignalDirection::Bearish,
            SignalStatus::Active,
            "OVERBOUGHT",
        ));
        rsi.signals.push(IndicatorSignal::new(
            SignalKind::Divergence,
            SignalDirection::Bullish,
            SignalStatus::Confirmed,
            "RSI_DIVERGENCE",
        ));
        m.insert("rsi".to_string(), rsi);
        let mut vol = NormalizedIndicatorValue::scalar(3.2, 0.9, "VOLUME_CLIMAX");
        vol.signals.push(IndicatorSignal::new(
            SignalKind::VolumeClimax,
            SignalDirection::Neutral,
            SignalStatus::Active,
            "VOLUME_CLIMAX",
        ));
        m.insert("volume".to_string(), vol);
        m
    }

    #[test]
    fn filter_map_signals_enforces_kind_and_pair_denylists() {
        let mut s = ActiveSet::all_enabled();
        s.disabled_signal_kinds.insert("VolumeClimax".to_string());
        s.disabled_signals
            .insert(("rsi".to_string(), "Threshold".to_string()));

        let mut m = map_with_signals();
        s.filter_map_signals(&mut m);

        // VolumeClimax kind disabled → volume entry loses its signal.
        assert!(
            m["volume"].signals.is_empty(),
            "disabled kind VolumeClimax must be filtered"
        );
        // Pair-level rsi:Threshold disabled, Divergence untouched.
        let kinds: Vec<SignalKind> = m["rsi"].signals.iter().map(|sig| sig.kind).collect();
        assert_eq!(kinds, vec![SignalKind::Divergence]);
    }

    #[test]
    fn filter_is_noop_when_denylist_empty() {
        let s = ActiveSet::all_enabled();
        let mut m = map_with_signals();
        s.filter_map_signals(&mut m);
        assert_eq!(m["rsi"].signals.len(), 2);
        assert_eq!(m["volume"].signals.len(), 1);
    }

    #[test]
    fn is_signal_allowed_matches_pascal_case_wire_names() {
        let mut s = ActiveSet::all_enabled();
        s.disabled_signal_kinds.insert("Threshold".to_string());
        assert!(!s.is_signal_allowed("rsi", SignalKind::Threshold));
        assert!(s.is_signal_allowed("rsi", SignalKind::Divergence));
        // Pair-level denylist is independent of the kind-level one.
        s.disabled_signal_kinds.clear();
        s.disabled_signals
            .insert(("macd".to_string(), "Crossover".to_string()));
        assert!(!s.is_signal_allowed("macd", SignalKind::Crossover));
        assert!(s.is_signal_allowed("macd", SignalKind::Threshold));
    }
}
