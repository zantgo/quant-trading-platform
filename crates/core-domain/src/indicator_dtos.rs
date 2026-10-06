//! # Indicator DTOs
//!
//! Pure-data shapes shared between `core-domain` (which holds them as
//! snapshot fields) and `market-analyzer` (which produces them via the
//! `NormalizationEngine`).
//!
//! These types have no dependency on raw indicator calculators. They are
//! safe to use from any crate that links against `core-domain`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Discrete signal kind an indicator can emit. Capabilities are declared in
/// the registry (`signal_types`); occurrences are recorded per snapshot in
/// `NormalizedIndicatorValue::signals`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalKind {
    Divergence,
    Crossover,
    Threshold,
    Breakout,
    BandTouch,
    ZeroLineCross,
    CompressionRelease,
    LevelTest,
    TrendFlip,
    VolumeClimax,
    StackChange,
    PatternForming,
}

/// Directional bias of a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDirection {
    Bullish,
    Bearish,
    Neutral,
}

/// Confirmation status of a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalStatus {
    Potential,
    Confirmed,
    Active,
}

/// A coordinate on the indicator/price series (used for divergence line points).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalPoint {
    pub time: u64,
    pub value: f64,
}

/// A single discrete signal fired by an indicator on a given snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorSignal {
    pub kind: SignalKind,
    pub direction: SignalDirection,
    pub status: SignalStatus,
    pub label: String,
    #[serde(default)]
    pub strength: f64,
    /// Number of completed bars since this signal first appeared (0 = fresh
    /// this bar). Stamped by the analyzer's stateful tracker.
    #[serde(default)]
    pub age_bars: u32,
    /// v9: strength class of `|normalized|` against the strategy's
    /// `l1.signals.strength_buckets` borders — `WEAK` / `MODERATE` /
    /// `STRONG` / `EXTREME`. Stamped by the analyzer's L1 post-pass.
    #[serde(default)]
    pub strength_label: String,
    /// Pivot coordinates for divergence line drawing (future). Empty otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<SignalPoint>>,
}

impl IndicatorSignal {
    pub fn new(
        kind: SignalKind,
        direction: SignalDirection,
        status: SignalStatus,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            direction,
            status,
            label: label.into(),
            strength: 0.0,
            age_bars: 0,
            strength_label: String::new(),
            points: None,
        }
    }

    pub fn with_strength(mut self, strength: f64) -> Self {
        self.strength = strength;
        self
    }

    pub fn with_points(mut self, points: Vec<SignalPoint>) -> Self {
        self.points = Some(points);
        self
    }
}

/// Unified dual-representation indicator value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedIndicatorValue {
    /// Primary raw scalar (native indicator units).
    pub raw_value: f64,
    /// Continuous normalized score in `[-1.0, 1.0]`.
    pub normalized: f64,
    /// Context-aware level string for frontend rendering / logging.
    pub state_label: String,
    /// Auxiliary raw components for multi-line indicators (macd line/signal,
    /// bollinger bands, adx/di). `None` for single-line indicators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<HashMap<String, f64>>,
    /// Discrete signals fired on this snapshot (divergence, crossover, breakout,
    /// threshold, etc.). Empty for most snapshots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<IndicatorSignal>,
    /// Conviction of this reading in `[0.0, 1.0]`. Base = `|normalized|`, later
    /// boosted by confirmed signals in the finalization pass.
    #[serde(default)]
    pub confidence: f64,
}

impl NormalizedIndicatorValue {
    /// Whether this reading is **silent** — i.e. has a raw value but no
    /// discrete signal was emitted on this snapshot.
    ///
    /// Frontend dashboard contract (v6.6+):
    /// - `raw_value == 0.0` AND `signals.is_empty()` → **silent data-only** row
    ///   (e.g. orderbook-derived metrics on a freshly connected WS feed where
    ///   `OrderBookAnalysis` hasn't yet published a fresh enough snapshot).
    /// - `raw_value != 0.0` AND `signals.is_empty()` AND `state_label` empty →
    ///   **conditional indicator with no recent event** (BOS, CHoCH, FVG,
    ///   S/R flip, chart patterns, OI-Price Divergence, …).
    /// - `signals.is_empty()` AND `state_label` non-empty → still "silent"
    ///   on the signal axis (data is publishing, but no signal fired) but
    ///   not "silent" on the state axis (an active level / regime label).
    ///
    /// The frontend picks the right pill (AWAITING_DATA · WARMING · SILENT
    /// · LIVE) by combining this bit with the lifecycle map's
    /// `IndicatorLifecycleState` and the registry's `signal_capability`.
    pub fn is_silent(&self) -> bool {
        self.signals.is_empty()
            && self.state_label.trim().is_empty()
            && self.raw_value.abs() < f64::EPSILON
    }

    /// True if the entry has at least one discrete signal fired.
    pub fn has_signals(&self) -> bool {
        !self.signals.is_empty()
    }

    /// v11.12.24 (memory): the retained-history projection of this reading.
    ///
    /// `TimeframePipeline.snapshot_history` keeps a rolling window of
    /// completed snapshots per (instance × duration) purely to serve
    /// `GET /api/history`. That handler reads **only** `raw_value`,
    /// `normalized`, `state_label` and `values` off each entry
    /// (`HistoricalIndicatorArrays::push_value`); the discrete `signals`
    /// and the `confidence` scalar are never read from the retained
    /// window. Across a full 52-indicator map the per-bar `signals`
    /// (17 of 43 keys carry them in the AEON corpus) dominate the
    /// retained bytes, so dropping them is the cheapest large win
    /// available without touching anything a chart renders.
    ///
    /// The **live** frame (`latest_snapshot` + the broadcast payload)
    /// keeps the full reading — this projection is applied only on the
    /// way INTO the retained history.
    pub fn history_projection(&self) -> Self {
        Self {
            raw_value: self.raw_value,
            normalized: self.normalized,
            state_label: self.state_label.clone(),
            values: self.values.clone(),
            signals: Vec::new(),
            confidence: 0.0,
        }
    }

    /// Cheap structural weight estimate (bytes) used by the retained-history
    /// byte budget. Never serialized — a rough per-allocation approximation
    /// is enough to keep the global counter in the right order of magnitude.
    pub fn history_weight_bytes(&self) -> usize {
        // key String + struct head
        let mut n = 64 + self.state_label.len();
        if let Some(values) = self.values.as_ref() {
            // HashMap head + per-entry (String key + f64)
            n += 48 + values.len() * 48;
            for k in values.keys() {
                n += k.len();
            }
        }
        if !self.signals.is_empty() {
            n += 24 + self.signals.len() * 128;
            for s in &self.signals {
                n += s.label.len() + s.strength_label.len();
                if let Some(points) = s.points.as_ref() {
                    n += 32 + points.len() * 48;
                }
            }
        }
        n
    }
}

impl NormalizedIndicatorValue {
    /// Build a single-line normalized value.
    pub fn scalar(raw_value: f64, normalized: f64, state_label: impl Into<String>) -> Self {
        let n = clamp_unit(normalized);
        Self {
            raw_value,
            normalized: n,
            state_label: state_label.into(),
            values: None,
            signals: Vec::new(),
            confidence: n.abs(),
        }
    }

    /// Build a normalized value carrying auxiliary raw component lines.
    pub fn with_values(
        raw_value: f64,
        normalized: f64,
        state_label: impl Into<String>,
        values: HashMap<String, f64>,
    ) -> Self {
        let n = clamp_unit(normalized);
        Self {
            raw_value,
            normalized: n,
            state_label: state_label.into(),
            values: Some(values),
            signals: Vec::new(),
            confidence: n.abs(),
        }
    }

    /// Neutral/equilibrium value used for missing data or defaults.
    pub fn neutral(label: impl Into<String>) -> Self {
        Self::scalar(0.0, 0.0, label)
    }

    /// Attach discrete signals (chained builder).
    pub fn with_signals(mut self, signals: Vec<IndicatorSignal>) -> Self {
        self.signals = signals;
        self
    }

    /// Append a single signal (chained builder).
    pub fn push_signal(mut self, signal: IndicatorSignal) -> Self {
        self.signals.push(signal);
        self
    }

    /// Override the computed confidence (chained builder).
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Divergence classification input for RSI/MACD normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DivergenceState {
    #[default]
    None,
    PotentialBullish,
    PotentialBearish,
    ConfirmedBullish,
    ConfirmedBearish,
}

/// Operational lifecycle of a single indicator on a single timeframe pipeline.
///
/// This enum is **not** about market semantics (those live in
/// `NormalizedIndicatorValue::state_label`); it describes whether the current
/// reading is trustworthy, warming up, or unusable. See
/// `docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md`
/// for the full state machine (ILS-01 … ILS-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndicatorLifecycleState {
    /// Calculator has fewer than `bars_required` candles of input; output is
    /// not yet trustworthy. Pipeline-level: `bars_seen < bars_required`.
    Loading,
    /// `bars_seen ≥ bars_required` AND parent pipeline LIVE AND last calculator
    /// update succeeded. The reading can be displayed without caveat.
    Live,
    /// Last successful update is older than `stale_threshold_secs`. The
    /// reading is still present but its freshness is degraded.
    Stale,
    /// Calculator panic / `Err`, OR `now - last_updated_at > 2 × stale_threshold_secs`.
    /// The reading should not be trusted.
    Failed,
}

/// Per-indicator feed status published on every `MarketSnapshot` alongside
/// the lifecycle map. Distinguishes **why** a `Live` lifecycle indicator is
/// not showing a real value:
///
/// - `Live` — a value-map entry exists with non-zero raw_value or a non-empty
///   `state_label`. Normal display.
/// - `WaitingFeed` — lifecycle is `Live` (enough candles have been seen) but
///   no value-map entry exists. Common for **DataOnly** / **Conditional**
///   indicators whose upstream feed hasn't delivered yet (e.g. OI from the
///   Bitget ticker channel on cold start). Frontend renders `WAITING FEED ⏳`.
/// - `Silent` — a value-map entry exists but `is_silent()` returns true (raw
///   value ≈ 0, no signals, no state label). Frontend renders `SILENT ⚡`.
/// - `Stale` — entry exists but is older than the staleness threshold.
///
/// `WaitingFeed` and `Silent` are deliberately distinct so the operator can
/// tell at a glance whether the issue is a missing wire feed (WaitingFeed)
/// or a true zero reading (Silent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FeedState {
    #[default]
    Live,
    WaitingFeed,
    Silent,
    Stale,
}

/// Per-indicator operational lifecycle metadata published on every
/// `MarketSnapshot` alongside the `indicators` map. The two maps share keys;
/// `indicator_lifecycle` describes the **status** of each calculator, while
/// `indicators` carries the latest computed **value**.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorLifecycleStatus {
    pub state: IndicatorLifecycleState,
    pub bars_seen: u32,
    /// PRI-12 (v6.10.7): real completed candles only — `bars_seen` counts
    /// every bar the pipeline processed, including synthetic doji/idle-
    /// heartbeat buckets on sub-minute timeframes. Analytics that must not
    /// count synthetic bars read this field (0 on a cold handover, growing
    /// per real live close).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars_seen_real: Option<u32>,
    pub bars_required: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub stale_threshold_secs: u32,
    /// `true` if the latest reading is **silent**: the calculator produced a
    /// raw value but no discrete signal was emitted. Drives the frontend's
    /// SILENT pill — distinguishes "indicator is computing, no recent
    /// event" from "indicator hasn't warmed up yet". `false` when the
    /// entry is missing (WARMING/AWAITING_DATA) or when a signal/state was
    /// emitted on the last snapshot.
    #[serde(default)]
    pub silent: bool,
    /// Feed classification (v6.6+). Distinguishes "feed hasn't arrived
    /// yet" (`WaitingFeed`) from "feed arrived but says zero" (`Silent`)
    /// for the SILENT / WAITING FEED frontend pills. Defaults to `Live`
    /// for backward compatibility — older clients / older snapshots that
    /// omit this field deserialize as a normal reading.
    #[serde(default)]
    pub feed_state: FeedState,
}

impl IndicatorLifecycleStatus {
    /// Build a fresh `Loading` entry for a just-constructed pipeline.
    pub fn loading(bars_required: u32, stale_threshold_secs: u32) -> Self {
        Self {
            state: IndicatorLifecycleState::Loading,
            bars_seen: 0,
            bars_seen_real: None,
            bars_required,
            last_updated_at: None,
            last_error: None,
            stale_threshold_secs,
            silent: false,
            feed_state: FeedState::Live,
        }
    }

    /// Promote to `Live` after the first successful calculator update.
    pub fn live(
        bars_seen: u32,
        bars_required: u32,
        last_updated_at: u64,
        stale_threshold_secs: u32,
        silent: bool,
    ) -> Self {
        Self {
            state: IndicatorLifecycleState::Live,
            bars_seen,
            bars_seen_real: None,
            bars_required,
            last_updated_at: Some(last_updated_at),
            last_error: None,
            stale_threshold_secs,
            silent,
            feed_state: FeedState::Live,
        }
    }

    /// Promote to `Live` with an explicit feed-state stamp (v6.6+).
    pub fn live_with_feed_state(
        bars_seen: u32,
        bars_required: u32,
        last_updated_at: u64,
        stale_threshold_secs: u32,
        silent: bool,
        feed_state: FeedState,
    ) -> Self {
        Self {
            state: IndicatorLifecycleState::Live,
            bars_seen,
            bars_seen_real: None,
            bars_required,
            last_updated_at: Some(last_updated_at),
            last_error: None,
            stale_threshold_secs,
            silent,
            feed_state,
        }
    }

    /// Mark as `Failed` with a free-text reason (calculator panic / double-stale).
    pub fn failed(bars_seen: u32, bars_required: u32, last_error: impl Into<String>) -> Self {
        Self {
            state: IndicatorLifecycleState::Failed,
            bars_seen,
            bars_seen_real: None,
            bars_required,
            last_updated_at: None,
            last_error: Some(last_error.into()),
            stale_threshold_secs: 0,
            silent: false,
            feed_state: FeedState::Live,
        }
    }
}

/// Type alias for the per-snapshot indicator lifecycle map. Keys are the
/// same registry keys as `MarketSnapshot.indicators` (e.g. `rsi`, `macd`,
/// `vwap`). Disabled indicators are absent from both maps.
pub type IndicatorLifecycleMap = HashMap<String, IndicatorLifecycleStatus>;

/// Clamp a value into the `[-1.0, 1.0]` unit interval.
#[inline]
pub fn clamp_unit(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(-1.0, 1.0)
    }
}
