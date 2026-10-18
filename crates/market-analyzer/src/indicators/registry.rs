//! # Indicator Registry — Single Source of Truth
//!
//! Every indicator in the system is declared exactly once here. The backend
//! (normalization, persistence, scoring) and the frontend (telemetry
//! matrix, chart toggles, scoring UI) all derive their behavior from this
//! manifest, which is serialized to the frontend via `/api/config`.
//!
//! Adding an indicator = one entry here + its calculator + its normalize mapper
//! + (its chart component). No scattered hardcoded lists.

use super::normalized::SignalKind;
use core_domain::axes::AxisId;
use serde::Serialize;

/// Functional category — **display only**.
///
/// CR-11 + CR-18: a cluster is a grouping of indicators that read the **same
/// source series** for the reader's convenience. It has **no weight effect**
/// whatsoever and is never consulted when scoring. The axis a reading votes on is
/// `owner_axis`, declared separately; the two must not be conflated.
///
/// `Regime` is renamed `TrendQuality` per CR-11. Every indicator in the old group
/// — `aroon`, `choppiness`, `linreg_slope`, `zscore`, `price_trend_sharpe` —
/// measures the age, strength, slope or quality of a trend. None classifies the
/// market into a state, which is what the banned word named. The distinction
/// matters: `MarketRegime`/`MarketPhase` collapsed into `diagnostics`, and
/// diagnostics are not a cluster and not an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum IndicatorCluster {
    Trend,
    Momentum,
    Volume,
    Volatility,
    Structure,
    TrendQuality,
    Institutional,
}

/// Predictive class (leading vs confirming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IndicatorClass {
    Leading,
    Hybrid,
    Lagging,
}

/// Data source for an indicator.  Candle-based indicators are gated on the
/// canonical buffer fill; WebSocket-derived indicators (derivatives, order-book
/// depth) and event-driven indicators (SMC: BOS, CHoCH, FVG, OB, sweeps) are
/// exempt from the candle count gate and appear the moment their data
/// source produces a reading.
///
/// The variant drives two sites in the analyzer:
///
///   1. `bars_needed(key)` in `crates/market-analyzer/src/analyzer/normalize.rs`
///      — `CandleBased` honors `bars_required`; `EventDriven` returns 0 so the
///      `map.retain(|key, _| ready(key))` gate at the end of
///      `build_indicator_map` doesn't evict a never-populated event entry.
///   2. The WARMING fill block in
///      `crates/market-analyzer/src/indicators/normalized/all.rs` — the fill
///      is skipped for `EventDriven` entries so we never publish a
///      `raw_value = 0.0` placeholder for an indicator whose contract is
///      "emit a value only when an event arrives".
///
///   3. (CR-10) the third site — "distinguish WS-fed rows from candle-warmup
///      rows" in the frontend — is gone with the variants it served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum IndicatorDataSource {
    /// OHLCV candles. The only source a reading may declare.
    #[default]
    CandleBased,
    /// Event-driven (BOS / CHoCH / FVG / order block / liquidity sweep).
    ///
    /// An event is still candle-derived — BOS, CHoCH, FVG and sweeps are all
    /// detected in the OHLCV series. The variant exists because those readings
    /// need no warmup, not because they need a feed.
    ///
    /// CR-10 / CH-1: `OrderBook` and `DerivativesWs` are DELETED. Every reading
    /// is computed from OHLCV, so a reading that *needs* an order book, open
    /// interest, funding or mark/index cannot be declared at all. The variants
    /// existing at all were the permission slip — an indicator could name a
    /// non-candle feed and be registered legitimately.
    ///
    /// The order-book CHANNEL stays (see `network-adapters`): it is how
    /// sub-minute candles are constructed. A channel that builds a candle is
    /// ingestion, not a reading. What is gone is the ability to publish a number
    /// derived from it directly.
    ///
    /// No candle warmup — the first reading appears the moment an event is
    /// detected in the running window. `bars_required` is still meaningful
    /// for the lifecycle's `Loading → Live` transition (the calculator needs
    /// enough bars to bootstrap), but the WARMING fill is suppressed so the
    /// indicator is absent from the snapshot's indicator map until the
    /// first event arrives.
    EventDriven,
}

/// Where the indicator renders in the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RenderKind {
    /// Its own oscillator/volume pane chart.
    Pane,
    /// A line/series drawn on the main price chart.
    PriceOverlay,
    /// Horizontal price-level lines on the main price chart.
    PriceLevels,
    /// Event markers only (no dedicated series).
    Marker,
}

/// How the indicator contributes to the directional confluence / UI Norm column.
///
/// Wired into the registry so that the Metrics table can render the right
/// placeholder for indicators that have no directional vote: gates display
/// `N/A` (the directional accumulator ignores them), event-only overlays
/// (Hull MA) display `N/A` because the spec defines them as raw-only
/// references, and standard directional indicators expose the real
/// `[-1.0, 1.0]` score. This is the single source of truth consumed by the
/// frontend's `IndicatorMeta.normalization_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum IndicatorNormalizationMode {
    /// Standard directional indicator — emits a real `[-1, 1]` score.
    #[default]
    Directional,
    /// Non-directional context gate — `normalized` is always 0.0 by contract;
    /// the directional accumulator ignores it. UI shows `N/A` in the Norm
    /// column to honor the contract.
    ContextOnly,
    /// Event-only overlay / cross-over source — `normalized` is always 0.0
    /// by contract; the value is read from `raw_value` or `values` for
    /// overlays (e.g. Hull MA). The directional contribution is conveyed
    /// via `state_label` and discrete signals — it is consumed by event-
    /// driven TAE policies, not by the directional confluence.
    EventOnly,
}

/// Static metadata describing one indicator end-to-end.
#[derive(Debug, Clone, Serialize)]
pub struct IndicatorMeta {
    pub key: &'static str,
    pub display_name: &'static str,
    pub group: IndicatorCluster,
    pub class: IndicatorClass,
    pub render: RenderKind,
    /// `true` = signed [-1,1] scoring contributor; `false` = non-directional gate/multiplier.
    pub directional: bool,
    pub supports_divergence: bool,
    pub signal_types: &'static [SignalKind],
    pub default_weight: f64,
    pub default_enabled: bool,
    pub config_params: &'static [&'static str],
    /// Frontend raw-cell render hint: "decimals1|decimals2|decimals4|percent1|price|ratio2|onoff".
    pub value_format: &'static str,
    /// Where the raw cell reads from: "raw" | "sub:<key>" | "state".
    pub value_source: &'static str,
    /// Primary chart color (hex).
    pub color: &'static str,
    /// Documentation section id in docs/indicators-guide.md.
    pub guide_section: &'static str,
    /// The specification file that documents THIS indicator, relative to
    /// `docs/engines/market-monitoring-engine/indicators/`. The registry is the
    /// single authority for that mapping: `04-02-00-indicator-index.md` is
    /// generated from these values, so a renamed file and a stale index cannot
    /// disagree (CR-4, CR-5).
    pub doc_file: &'static str,
    /// Minimum number of input bars the calculator needs before it can emit
    /// a non-`None` reading. Drives the per-indicator `Loading → Live`
    /// transition in [03-02-15 ILS-11](../docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md).
    /// For 52 indicators at the canonical `[candle_buffer] size = 500`,
    /// every `bars_required ≤ 300` so a fully-warmed pipeline is `Live`
    /// across all indicators (DCP-04 / ILS-05). Note: `ema_stack` carries
    /// `bars_required = 1` — its per-line availability is gated by period
    /// inside `inject_ema_values` (AUDIT-V8-001).
    pub bars_required: u32,
    /// Whether this indicator recomputes on shadow (live) ticks via clone.
    /// `true` = candle-close-independent (RSI, MACD, EMA…); the value
    /// updates every shadow tick. `false` = requires a completed candle
    /// (Fibonacci, patterns, S/R zones…); the value is only fresh after a
    /// candle closes. The frontend uses this to visually distinguish fresh
    /// from confirmed-on-close values.
    pub updates_on_shadow: bool,
    /// Data source category.  Candle-based indicators are gated on the
    /// pipeline buffer fill; `OrderBook` / `DerivativesWs` indicators
    /// bypass the candle-count gate and appear when their live WS data
    /// arrives.  (Phase 4, v6.6 — pipeline gate exemption.)
    /// `None` defaults to `CandleBased` at the gate site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<IndicatorDataSource>,
    /// How this indicator contributes to the directional confluence and the
    /// Normalized Indicator column. Drives both the backend scoring
    /// accumulator (which gates must skip) and the frontend Metrics-table
    /// rendering (`N/A` vs `0.00` vs the real `[-1, 1]` score). See
    /// [`IndicatorNormalizationMode`] for the canonical per-mode contract.
    /// `None` defaults to `Directional` at the gate site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalization_mode: Option<IndicatorNormalizationMode>,
    /// Whether this indicator's chart overlay can be **silent** — i.e. emit
    /// a raw value without firing a discrete signal for many bars in a row.
    ///
    /// The frontend 4-state badge (AWAITING_DATA · WARMING · SILENT · LIVE)
    /// uses this field to decide which states are reachable. Indicators that
    /// always have meaningful state on every bar (e.g. RSI, MACD, BBWP)
    /// are marked `AlwaysActive` and never show SILENT — only AWAITING_DATA,
    /// WARMING or LIVE. Indicators that depend on rare structural events
    /// (BOS, CHoCH, FVG, S/R flip, chart patterns, OI-Price Divergence) are
    /// marked `Conditional` — they can be SILENT for many bars between
    /// signals. Indicators that only emit a raw scalar with no state_label
    /// at all (spread, depth bias, OFI, OI funding — the "data-only" set)
    /// are `DataOnly` — they reach SILENT once the WS feed is live but
    /// their lifecycle never becomes LIVE in the old sense.
    pub signal_capability: SignalCapability,
}

/// Whether an indicator's chart overlay is always active, fires discrete
/// signals only on rare structural events, or carries data only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SignalCapability {
    /// The indicator emits a fresh `state_label` on every bar (RSI, MACD,
    /// BBWP, ATR, EMA family, etc.). The frontend only renders
    /// AWAITING_DATA / WARMING / LIVE — never SILENT.
    AlwaysActive,
    /// The indicator only fires a discrete signal on rare structural
    /// events (BOS, CHoCH, FVG, S/R flip, pivot touches, chart patterns,
    /// OI-Price Divergence). The frontend can render SILENT between events.
    Conditional,
    /// The indicator publishes a raw scalar (spread, depth bias, OFI,
    /// mark_index_spread, OI, funding rate, OI Δ) and never emits a
    /// `state_label`. The frontend reaches SILENT once WS is live; the
    /// `dataSource === 'DerivativesWs'|'OrderBook'` row is then read as a
    /// pure value badge.
    DataOnly,
}

use IndicatorClass::*;
use IndicatorCluster::*;
use IndicatorDataSource::*;
use IndicatorNormalizationMode::*;
use RenderKind::*;
use SignalKind::*;

/// The authoritative indicator manifest — **44 entries across 7 groups**.
///
/// CR-10 purged the eight non-candle entries. `open_interest`, `oi_delta`,
/// `funding_rate`, `oi_price_divergence`, `order_flow_imbalance`, `spread`,
/// `depth_bias` and `mark_index_spread` were all computed from a venue feed
/// that is not OHLCV — open interest, funding, the order book, mark/index. Under
/// charter rule CH-1 they could not be readings at all, and a reading that cannot
/// be recomputed from a candle archive can never be validated by replaying
/// history.
///
/// The `DerivativesData` group is therefore **empty and deleted**: it existed to
/// hold exactly those eight. Ichimoku and Pivot Points are documented in the
/// guides; the comment claiming they were "intentionally omitted" predates their
/// inclusion.
pub const INDICATORS: &[IndicatorMeta] = &[
    // ─────────── TREND ───────────
    IndicatorMeta {
        key: "ema_stack",
        display_name: "EMA Ribbon",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[StackChange, Crossover],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["ema_fast", "ema_medium", "ema_slow", "ema_long"],
        value_format: "price",
        value_source: "sub:fast",
        color: "#fdd835",
        guide_section: "6",
        doc_file: "04-02-01-ema-stack.md",
        // AUDIT-V8-001: bars_required dropped 200 → 1. The per-line
        // availability gate now lives in `inject_ema_values` (each line
        // appears once `bar_count >=` its configured period: fast@10,
        // medium@50, slow@100, long@200). The registry gate only decides
        // whether the entry survives the normalize retain; the lifecycle
        // (Loading → Live) is unaffected for fully-warmed pipelines and
        // sub-minute TFs now surface partial ribbons instead of nothing.
        bars_required: 1,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "supertrend",
        display_name: "Supertrend",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[TrendFlip, Crossover, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["supertrend_period", "supertrend_multiplier"],
        value_format: "price",
        value_source: "sub:line",
        color: "#26a69a",
        guide_section: "14",
        doc_file: "04-02-02-supertrend.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "donchian",
        display_name: "Donchian",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, BandTouch, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["donchian_period"],
        value_format: "price",
        value_source: "sub:upper",
        color: "#ec407a",
        guide_section: "16",
        doc_file: "04-02-03-donchian.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "keltner",
        display_name: "Keltner",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, BandTouch, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[
            "keltner_ema_period",
            "keltner_atr_period",
            "keltner_multiplier",
        ],
        value_format: "price",
        value_source: "sub:middle",
        color: "#78909c",
        guide_section: "15",
        doc_file: "04-02-04-keltner.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "adx",
        display_name: "ADX",
        group: Trend,
        class: Lagging,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[TrendFlip, Threshold],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["adx_period"],
        value_format: "decimals2",
        value_source: "sub:adx",
        color: "#ff9800",
        guide_section: "4",
        doc_file: "04-02-05-adx.md",
        bars_required: 14,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "vwap",
        display_name: "VWAP",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "price",
        value_source: "sub:vwap",
        color: "#2962ff",
        guide_section: "7",
        doc_file: "04-02-06-vwap.md",
        bars_required: 1,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "anchored_vwap",
        display_name: "Anchored VWAP",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Crossover, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "price",
        // The normalizer publishes the weekly level under the `weekly`
        // sub-key (see crates/market-analyzer/src/indicators/normalized/all.rs
        // ::anchored_vwap block).  The previous `sub:vwap_weekly` key never
        // matched and rendered `--` in the Metrics Raw column.
        value_source: "sub:weekly",
        color: "#ffab40",
        guide_section: "34",
        doc_file: "04-02-07-anchored-vwap.md",
        bars_required: 1,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "ichimoku",
        display_name: "Ichimoku Cloud",
        group: Trend,
        class: Hybrid,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Crossover, Breakout, TrendFlip, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[
            "ichimoku_tenkan",
            "ichimoku_kijun",
            "ichimoku_senkou_b",
            "ichimoku_displacement",
        ],
        value_format: "price",
        value_source: "sub:tenkan",
        color: "#7e57c2",
        guide_section: "25",
        doc_file: "04-02-08-ichimoku.md",
        // Ichimoku's smallest configured window is Tenkan (9). The strict
        // `update()` requires `senkou_b_period=52` for the full cloud, but
        // the soft-floor `update_with_min_bars(min_bars=9)` variant in
        // ichimoku.rs surfaces a partial reading (Tenkan-line only, no
        // cloud) at 9 bars and progressively fills in Kijun (26) and Senkou
        // B (52) as history grows. The lifecycle gate therefore matches the
        // soft-floor floor, so the dashboard flips to Live as soon as
        // Tenkan becomes computable.
        bars_required: 9,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    // ─────────── MOMENTUM ───────────
    IndicatorMeta {
        key: "rsi",
        display_name: "RSI",
        // AUDIT-AIU-080: real warmup is period+1 closes (15 for 14); the
        // previous 14 made the lifecycle flip Live one bar early.
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[Divergence, Threshold, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["rsi_period"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#7e57c2",
        guide_section: "1",
        doc_file: "04-02-11-rsi.md",
        bars_required: 15,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "stochastic",
        display_name: "Stochastic",
        // AUDIT-AIU-080: real warmup is k+s+d−2 = 30 at default 18/5/9;
        // the previous 14 flipped Live ~16 bars early.
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[Crossover, Threshold, Divergence, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["stoch_k_period", "stoch_d_period", "stoch_s_period"],
        value_format: "percent1",
        value_source: "sub:k_line",
        color: "#2962ff",
        guide_section: "12",
        doc_file: "04-02-12-stochastic.md",
        bars_required: 30,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "chandemo",
        display_name: "Chande MO",
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[ZeroLineCross, Threshold, Divergence],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["chandemo_period"],
        value_format: "decimals1",
        value_source: "raw",
        color: "#e040fb",
        guide_section: "13",
        doc_file: "04-02-13-chandemo.md",
        bars_required: 14,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "williams_r",
        display_name: "Williams %R",
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[Threshold, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["williams_r_period"],
        value_format: "decimals1",
        value_source: "raw",
        color: "#81c784",
        guide_section: "28",
        doc_file: "04-02-14-williams-r.md",
        bars_required: 14,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "hull_ma",
        display_name: "Hull MA",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Crossover],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["hull_ma_period"],
        value_format: "price",
        value_source: "raw",
        color: "#ff8a65",
        guide_section: "29",
        doc_file: "04-02-10-hull-ma.md",
        // Hull MA's defining feature is *low lag* — the registry gate was
        // previously set to 200, which is wildly out of line with both the
        // default `hull_ma_period=21` and the soft-floor min_bars=5 floor
        // used in warm/live paths. 14 matches `period/2` rounded up with a
        // safety margin and is small enough that sub-minute TFs where the
        // live pipeline has just 20 candles still surface a Live state.
        bars_required: 14,
        data_source: None,
        updates_on_shadow: false,
        // HMA is a chart overlay (directional Crossover source whose
        // directional contribution is conveyed via state_label and discrete
        // signals, not via the normalized score).  See
        // docs/engines/market-monitoring-engine/indicators/04-02-10-hull-ma.md
        normalization_mode: Some(EventOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "awesome_oscillator",
        display_name: "AO",
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[ZeroLineCross, Threshold],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "decimals4",
        value_source: "raw",
        color: "#4dd0e1",
        guide_section: "30",
        doc_file: "04-02-15-awesome-oscillator.md",
        bars_required: 34,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "force_index",
        display_name: "Force Idx",
        group: Volume,
        class: Hybrid,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[ZeroLineCross, Threshold],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["force_index_smoothing"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#ce93d8",
        guide_section: "31",
        doc_file: "04-02-24-force-index.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "stddev_channel",
        display_name: "StdDev Chnl",
        group: Volatility,
        class: Hybrid,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, BandTouch, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["stddev_channel_period"],
        value_format: "price",
        value_source: "sub:center",
        color: "#a1887f",
        guide_section: "32",
        doc_file: "04-02-30-stddev-channel.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "cci",
        display_name: "CCI",
        group: Momentum,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[Threshold, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["cci_period"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#ffb74d",
        guide_section: "26",
        doc_file: "04-02-16-cci.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "macd",
        display_name: "MACD",
        group: Momentum,
        class: Lagging,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[Crossover, ZeroLineCross, Divergence, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["macd_fast", "macd_slow", "macd_signal"],
        value_format: "decimals4",
        value_source: "raw",
        color: "#26a69a",
        guide_section: "2",
        doc_file: "04-02-17-macd.md",
        bars_required: 26,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    // ─────────── VOLUME ───────────
    IndicatorMeta {
        key: "volume",
        display_name: "Volume",
        group: Volume,
        class: Hybrid,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[VolumeClimax],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["volume_average_period"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#26a69a",
        guide_section: "6",
        doc_file: "04-02-18-volume.md",
        bars_required: 1,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional gate: `normalized` is contractually 0.0; the raw
        // volume value + climax label carry the signal. See
        // docs/engines/market-monitoring-engine/indicators/04-02-18-volume.md §6.
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "rvol",
        display_name: "RVOL",
        group: Volume,
        class: Hybrid,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[VolumeClimax],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["rvol_threshold_institutional", "rvol_threshold_climax"],
        value_format: "ratio2",
        value_source: "raw",
        color: "#ffa726",
        guide_section: "6",
        doc_file: "04-02-19-rvol.md",
        bars_required: 20,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional gate: per the v2.1 contract, `normalized` is
        // contractually 0.0 and the signed 4-band value lives in
        // `values.rvol_band`. See
        // docs/engines/market-monitoring-engine/indicators/04-02-19-rvol.md §3.
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "volume_profile",
        display_name: "Volume Profile",
        group: Volume,
        class: Hybrid,
        render: PriceLevels,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, LevelTest, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[
            "volume_profile_bins",
            "volume_profile_window",
            "volume_profile_value_area",
        ],
        value_format: "price",
        value_source: "sub:poc",
        color: "#bcaaa4",
        guide_section: "33",
        doc_file: "04-02-20-volume-profile.md",
        bars_required: 250, // AUDIT-M5: shipped `volume_profile_window = 500` → gate = window/2 = 250 (was 50, the library-default doc value)
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "obv",
        display_name: "OBV",
        group: Volume,
        class: Lagging,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[Divergence, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["obv_smoothing"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#29b6f6",
        guide_section: "17",
        doc_file: "04-02-21-obv.md",
        bars_required: 21, // AUDIT-M3: OBV emits only after the smoothing SMA fills (seed + 20-value SMA at obv_smoothing = 20) — the lifecycle `bars_required = 1` understated the real warmup
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "cmf",
        display_name: "Chaikin MF",
        group: Volume,
        class: Hybrid,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[ZeroLineCross, Divergence],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["cmf_period"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#26c6da",
        guide_section: "18",
        doc_file: "04-02-22-cmf.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "mfi",
        display_name: "Money Flow Idx",
        group: Volume,
        class: Hybrid,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[Threshold, Divergence, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["mfi_period"],
        value_format: "decimals1",
        value_source: "raw",
        color: "#ab47bc",
        guide_section: "19",
        doc_file: "04-02-23-mfi.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    // ─────────── VOLATILITY ───────────
    IndicatorMeta {
        key: "atr",
        display_name: "ATR",
        group: Volatility,
        class: Lagging,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[Threshold, CompressionRelease],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["atr_period"],
        value_format: "price",
        value_source: "raw",
        color: "#ef5350",
        guide_section: "5",
        doc_file: "04-02-25-atr.md",
        bars_required: 14,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional gate: `normalized` is contractually 0.0; the
        // regime classifier (Expanding/Contracting/Stable) carries the
        // signal. See
        // docs/engines/market-monitoring-engine/indicators/04-02-25-atr.md §6.
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "bollinger",
        display_name: "Bollinger",
        group: Volatility,
        class: Hybrid,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, BandTouch, LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "price",
        value_source: "sub:middle",
        color: "#00e5ff",
        guide_section: "5",
        doc_file: "04-02-26-bollinger.md",
        bars_required: 20,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "bbwp",
        display_name: "BBWP",
        // AUDIT-AIU-080: real warmup is period + lookback = 272; the gate
        // stays 200 (well below the INDICATORS_MAX_BARS_REQUIRED = 300
        // invariant, which is carried by price_trend_sharpe) and the
        // lifecycle doc notes BBWP shows WARMING from 200→272.
        group: Volatility,
        class: Leading,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[CompressionRelease],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["bbwp_period", "bbwp_lookback"],
        value_format: "percent1",
        value_source: "raw",
        color: "#ffca28",
        guide_section: "9",
        doc_file: "04-02-27-bbwp.md",
        bars_required: 200,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional gate: BBWP carries no directional bias;
        // `normalized` is contractually 0.0 and the confidence axis / raw
        // band drives the regime. See
        // docs/engines/market-monitoring-engine/indicators/04-02-27-bbwp.md §6.
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "squeeze",
        display_name: "TTM Squeeze",
        // AUDIT-AIU-080: real warmup is 20 SMA + 20 val history = 39-40;
        // the previous 20 flipped Live ~20 bars early.
        group: Volatility,
        class: Hybrid,
        render: Pane,
        directional: true,
        supports_divergence: true,
        signal_types: &[CompressionRelease, Divergence],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["squeeze_period"],
        value_format: "onoff",
        value_source: "state",
        color: "#b2ff59",
        guide_section: "3",
        doc_file: "04-02-28-squeeze.md",
        bars_required: 39,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "hv",
        display_name: "Hist. Volatility",
        // AUDIT-AIU-080: real warmup is period+1 closes (21 for 20); the
        // previous 20 flipped Live one bar early.
        group: Volatility,
        class: Lagging,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[Threshold],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["hv_period"],
        value_format: "percent1",
        value_source: "raw",
        color: "#ff7043",
        guide_section: "20",
        doc_file: "04-02-29-hv.md",
        bars_required: 21,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional gate: `normalized` is contractually 0.0; the
        // volatility regime label carries the signal. See
        // docs/engines/market-monitoring-engine/indicators/04-02-29-hv.md §3.
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    // ─────────── MARKET STRUCTURE ───────────
    IndicatorMeta {
        key: "fibonacci",
        display_name: "Fibonacci",
        group: Structure,
        class: Leading,
        render: PriceLevels,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "price",
        value_source: "sub:gp_top",
        color: "#ffd54f",
        guide_section: "8",
        doc_file: "04-02-31-fibonacci.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "support_resistance",
        display_name: "Support/Resistance",
        group: Structure,
        class: Leading,
        render: PriceLevels,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest, Breakout],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "price",
        value_source: "raw",
        color: "#90a4ae",
        guide_section: "8",
        doc_file: "04-02-32-support-resistance.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "pivot_points",
        display_name: "Pivot Points",
        group: Structure,
        class: Leading,
        render: PriceLevels,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest, Breakout, Crossover],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["pivot_points_method"],
        value_format: "price",
        value_source: "sub:pivot",
        color: "#8d6e63",
        guide_section: "8",
        doc_file: "04-02-33-pivot-points.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "psar",
        display_name: "Parabolic SAR",
        group: Trend,
        class: Lagging,
        render: PriceOverlay,
        directional: true,
        supports_divergence: false,
        signal_types: &[TrendFlip, Crossover],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["psar_af_step", "psar_af_max"],
        value_format: "price",
        value_source: "sub:sar",
        color: "#ffab40",
        guide_section: "27",
        doc_file: "04-02-09-psar.md",
        // PSAR seeds on the very first bar (see psar.rs::ParabolicSar::update),
        // so the calculator produces a meaningful trailing-stop reading from
        // bar 1. The previous gate of 50 had no mathematical basis and only
        // served to evict otherwise valid readings via the lifecycle retain
        // check in analyzer/normalize.rs.
        bars_required: 1,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "patterns",
        display_name: "Patterns",
        group: Structure,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[PatternForming],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "percent1",
        value_source: "raw",
        color: "#ba68c8",
        guide_section: "10",
        doc_file: "04-02-34-patterns.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "candlestick",
        display_name: "Candlestick",
        group: Structure,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[PatternForming],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["candlestick_min_confidence"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#4db6ac",
        guide_section: "10",
        doc_file: "04-02-35-candlestick.md",
        bars_required: 50,
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    // ─────────── MARKET REGIME ───────────
    IndicatorMeta {
        key: "aroon",
        display_name: "Aroon",
        group: TrendQuality,
        class: Hybrid,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[Crossover, Threshold, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["aroon_period"],
        value_format: "decimals1",
        value_source: "raw",
        color: "#26a69a",
        guide_section: "21",
        doc_file: "04-02-36-aroon.md",
        bars_required: 26, // AUDIT-L5: window is period + 1 (26 bars at aroon_period = 25)
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "choppiness",
        display_name: "Choppiness",
        group: TrendQuality,
        class: Hybrid,
        render: Pane,
        directional: false,
        supports_divergence: false,
        signal_types: &[Threshold, CompressionRelease],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["chop_period"],
        value_format: "decimals1",
        value_source: "raw",
        color: "#ffa726",
        guide_section: "22",
        doc_file: "04-02-37-choppiness.md",
        bars_required: 14,
        data_source: None,
        updates_on_shadow: true,
        // Non-directional regime gate: `normalized` is contractually 0.0.
        // See
        // docs/engines/market-monitoring-engine/indicators/04-02-37-choppiness.md
        normalization_mode: Some(ContextOnly),
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "linreg_slope",
        display_name: "LinReg Slope",
        group: TrendQuality,
        class: Lagging,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["linreg_period"],
        value_format: "decimals4",
        value_source: "raw",
        color: "#42a5f5",
        guide_section: "23",
        doc_file: "04-02-38-linreg-slope.md",
        bars_required: 20, // AUDIT-M4: first emit needs `linreg_period` bars (shipped 20) — was 14
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    IndicatorMeta {
        key: "zscore",
        display_name: "Z-Score",
        group: TrendQuality,
        class: Leading,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[Threshold, ZeroLineCross],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["zscore_period"],
        value_format: "decimals2",
        value_source: "raw",
        color: "#ec407a",
        guide_section: "24",
        doc_file: "04-02-39-zscore.md",
        bars_required: 20, // AUDIT-M4: first emit needs `zscore_period` bars (shipped 20) — was 14
        data_source: None,
        normalization_mode: None,
        updates_on_shadow: true,
        signal_capability: SignalCapability::AlwaysActive,
    },
    // ─────────── ADVANCED ───────────
    IndicatorMeta {
        key: "smc_structure",
        display_name: "SMC Structure",
        group: Institutional,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[Breakout, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["smc_lookback"],
        value_format: "decimals2",
        value_source: "sub:structure",
        color: "#ffab40",
        guide_section: "34",
        doc_file: "04-02-40-smc-structure.md",
        bars_required: 50,
        // Event-driven: an entry only appears once BOS/CHoCH is detected.
        // Suppresses the WARMING placeholder so a ranging market doesn't
        // leak a misleading 0.0 into the indicator map.
        data_source: Some(EventDriven),
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "smc_liquidity",
        display_name: "SMC Liquidity",
        group: Institutional,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[Threshold, PatternForming],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["smc_lookback"],
        value_format: "decimals2",
        value_source: "sub:sweep_buy",
        color: "#ff7043",
        guide_section: "34",
        doc_file: "04-02-41-smc-liquidity.md",
        bars_required: 50,
        data_source: Some(EventDriven),
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "smc_fvg",
        display_name: "SMC Fair Value Gap",
        group: Institutional,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["smc_lookback"],
        value_format: "decimals2",
        value_source: "sub:fvg_top",
        color: "#ffca28",
        guide_section: "34",
        doc_file: "04-02-42-smc-fvg.md",
        bars_required: 50,
        data_source: Some(EventDriven),
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    IndicatorMeta {
        key: "smc_order_blocks",
        display_name: "SMC Order Blocks",
        group: Institutional,
        class: Leading,
        render: Marker,
        directional: true,
        supports_divergence: false,
        signal_types: &[LevelTest, TrendFlip],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &["smc_lookback"],
        value_format: "decimals2",
        value_source: "sub:ob_bullish_high",
        color: "#8d6e63",
        guide_section: "34",
        doc_file: "04-02-43-smc-order-blocks.md",
        bars_required: 50,
        data_source: Some(EventDriven),
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::Conditional,
    },
    // ─────────── DERIVATIVES DATA (Phase 11) ───────────
    // ─────────── ORDER BOOK DEPTH (Phase 2) ───────────
    // ── MARK-INDEX SPREAD (cross-cuts derivatives + orderbook telemetry) ──
    // Injected by `analyzer::inject_derivatives_indicators` from
    // `latest_mark_px` / `latest_index_px`. Until v6.6 it was emitted into
    // the indicator map but had no registry entry, so the Metrics table
    // filtered it out via `filterRegistry`. Tagging it here makes it
    // visible to the dashboard (Raw column = spread %, Norm column = N/A
    // because `normalization_mode = ContextOnly`).
    // ── PRICE-TREND SHARPE (L1 regime statistical proof) ──
    // Annualized Sharpe of price log returns over the trailing 300-bar
    // window. Computed by the pipeline from the rolling `close_history`
    // buffer (see `indicators/ratio.rs`); injected into the indicator map
    // in `analyzer/normalize.rs` after `normalize_all`. `bars_required`
    // (300) is well below the canonical `[candle_buffer] size` (500), so
    // the indicator reaches `Live` at its own 300-bar requirement,
    // independent of the buffer-fill gate — no lifecycle lock.
    IndicatorMeta {
        key: "price_trend_sharpe",
        display_name: "Price Trend Sharpe",
        group: TrendQuality,
        class: Lagging,
        render: Pane,
        directional: true,
        supports_divergence: false,
        signal_types: &[],
        default_weight: 1.0,
        default_enabled: true,
        config_params: &[],
        value_format: "ratio2",
        value_source: "raw",
        color: "#facc15",
        guide_section: "24",
        doc_file: "04-02-52-price-trend-sharpe.md",
        bars_required: 300,
        data_source: Some(CandleBased),
        normalization_mode: None,
        updates_on_shadow: false,
        signal_capability: SignalCapability::AlwaysActive,
    },
];

/// Return the full manifest as an owned vector (for API serialization).
pub fn all() -> Vec<IndicatorMeta> {
    INDICATORS.to_vec()
}

/// Maximum `bars_required` across all indicators in the registry.
/// Every candle-based indicator is mathematically correct once the buffer
/// holds at least this many bars (300 = the L1 `price_trend_sharpe` window).
/// This is the **indicator-tier floor** — one of three independent candle
/// numbers: 300 here (calculation minimum), `[candle_buffer] size` (default
/// 500, the historical warmup), and the one cap `HISTORY_CAP` (500, the absolute
/// in-memory cap — never more than 1000 candles, sub-minute and
/// above-minute, same behavior). Used as the lower-tier mathematical-gate
/// (Layer 1); the higher-tier system-gate (Layer 2) is `[candle_buffer]
/// size` (default 500).
/// (AUDIT-V8-001: ema_stack dropped to 1 — per-line period gating replaced
/// the whole-ribbon 200-bar gate; BBWP's registry gate stays 200 below the
/// 272-bar true warmup — see 03-02-15.)
pub const INDICATORS_MAX_BARS_REQUIRED: u32 = 300;

/// Look up an indicator's metadata by key.
pub fn get(key: &str) -> Option<&'static IndicatorMeta> {
    INDICATORS.iter().find(|m| m.key == key)
}

/// Return the effective normalization mode for an indicator entry.
///
/// `Some(mode)` uses the registry-declared value; `None` defaults to
/// [`IndicatorNormalizationMode::Directional`]. This is the canonical
/// helper for backend scoring consumers and the frontend Metrics table —
/// keeping the default-resolution here means the wired registry metadata
/// is the only source of truth and ad-hoc `is ContextOnly?` checks cannot
/// drift from the manifest.
pub fn normalization_mode_for(meta: &IndicatorMeta) -> IndicatorNormalizationMode {
    meta.normalization_mode.unwrap_or_default()
}

/// `true` when the indicator's `normalized` score should be displayed
/// verbatim (`-1.0..=1.0`) in the UI Norm column. Returns `false` for
/// [`IndicatorNormalizationMode::ContextOnly`] (gate) and
/// [`IndicatorNormalizationMode::EventOnly`] (overlay) indicators, both
/// of which must render `N/A` to honor the published contract.
pub fn is_directional_norm(meta: &IndicatorMeta) -> bool {
    matches!(
        normalization_mode_for(meta),
        IndicatorNormalizationMode::Directional
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_keys_unique() {
        let mut seen = std::collections::HashSet::new();
        for m in INDICATORS {
            assert!(seen.insert(m.key), "duplicate registry key: {}", m.key);
        }
    }

    /// CR-4/CR-5: the registry owns each indicator's specification filename.
    ///
    /// `04-02-00-indicator-index.md` is generated from these values and
    /// `scripts/check_docs.py` runs `registry_docs --check` inside
    /// `./manage.sh test-doc`, so a stale index cannot survive a rename.
    /// What this test catches is the other half: a `doc_file` that is
    /// malformed, duplicated, or points outside the `04-02-` series.
    #[test]
    fn test_doc_file_is_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for m in INDICATORS {
            assert!(
                m.doc_file.starts_with("04-02-") && m.doc_file.ends_with(".md"),
                "{}: doc_file must be an 04-02-NN-*.md spec file, got {:?}",
                m.key,
                m.doc_file
            );
            assert!(
                seen.insert(m.doc_file),
                "duplicate doc_file {:?} (keys: {} / already claimed)",
                m.doc_file,
                m.key
            );
            // NN must be the zero-padded two-digit series number.
            let nn = m
                .doc_file
                .split('-')
                .nth(2)
                .expect("doc_file has an NN segment");
            assert_eq!(
                nn.len(),
                2,
                "{}: doc_file NN must be zero-padded: {}",
                m.key,
                m.doc_file
            );
            assert!(
                nn.parse::<u32>().is_ok(),
                "{}: doc_file NN must be numeric: {}",
                m.key,
                m.doc_file
            );
        }
    }

    /// The generated index reads these, so a non-`&'static str` display name or
    /// key would silently change a published document.
    #[test]
    fn test_registry_fields_used_by_generated_docs_are_populated() {
        for m in INDICATORS {
            assert!(!m.key.is_empty(), "registry entry with empty key");
            assert!(!m.display_name.is_empty(), "{}: empty display_name", m.key);
            // An empty signal_types is legitimate for a data-only indicator
            // (price_trend_sharpe emits no events); the generated index renders
            // it as "— (data-only)". Anything else with no signals would render
            // as a silent empty cell, which is indistinguishable from a bug.
            // An indicator may be computed unconditionally and declare no
            // typed signal kind at all (`price_trend_sharpe` does). The
            // generated index renders that as an explicit "—", so it is legal;
            // what is NOT legal is a non-empty cell for an indicator with none.
            if m.signal_types.is_empty() {
                assert!(
                    m.doc_file.starts_with("04-02-"),
                    "{}: indicator with no signal_types must still be documented",
                    m.key
                );
            }
        }
    }

    /// AUDIT-AIU-125: the registry manifest is a normative contract —
    /// The Metrics dashboard's indicator table is registry-driven, so the
    /// registry is a contract: deleting an entry (or its `signal_types`) must
    /// fail here rather than pass CI silently.
    ///
    /// CR-10 moved the count from 52 to **44**. The eight removed entries were
    /// all computed from a venue feed that is not OHLCV, which charter rule
    /// CH-1 forbids — and a reading that cannot be recomputed from a candle
    /// archive can never be validated by replaying history. This pin and
    /// `04-03-00-new-indicators-index.md` are generated from the same source,
    /// so they cannot drift apart.
    #[test]
    fn registry_cardinality_matches_the_documented_contract() {
        assert_eq!(
            INDICATORS.len(),
            44,
            "CR-10: the registry declares exactly 44 candle-only indicators; a \
             registry edit must update the docs + this pin together"
        );
        // CR-10: 101 -> 92. The nine removed signal-kind declarations belonged
        // to the eight purged entries; `oi_price_divergence` declared two.
        let declared: usize = INDICATORS.iter().map(|m| m.signal_types.len()).sum();
        assert_eq!(
            declared, 92,
            "CR-10: the registry declares exactly 92 signal-kind entries across the \
             44 candle-only indicators"
        );
        let kinds: std::collections::HashSet<String> = INDICATORS
            .iter()
            .flat_map(|m| m.signal_types.iter().map(|s| format!("{:?}", s)))
            .collect();
        assert_eq!(
            kinds.len(),
            12,
            "the union of declared signal kinds must be exactly 12 (03-02-01 §Registry), got {}",
            kinds.len()
        );
        // Group-level contract from 03-02-02 §1.3 (10 Trend / 7 Momentum /
        // 7 Volume / 6 Volatility / 5 Structure / 5 TrendQuality / 4 Institutional
        // / 8 Derivatives = 52).
        let group_counts: std::collections::HashMap<IndicatorCluster, usize> = INDICATORS
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, m| {
                *acc.entry(m.group).or_insert(0) += 1;
                acc
            });
        let expect: std::collections::HashMap<IndicatorCluster, usize> = [
            (IndicatorCluster::Trend, 10),
            (IndicatorCluster::Momentum, 7),
            (IndicatorCluster::Volume, 7),
            (IndicatorCluster::Volatility, 6),
            (IndicatorCluster::Structure, 5),
            (IndicatorCluster::TrendQuality, 5),
            (IndicatorCluster::Institutional, 4),
        ]
        .into_iter()
        .collect();
        for (group, expected) in &expect {
            assert_eq!(
                group_counts.get(group).copied().unwrap_or(0),
                *expected,
                "group {:?} must hold {} indicators (03-02-02 §1.3)",
                group,
                expected
            );
        }
        assert_eq!(
            group_counts.values().sum::<usize>(),
            44,
            "CR-10: group counts must sum to 44 — the 44 candle-only indicators"
        );
    }

    #[test]
    fn test_registry_serializes() {
        let json = serde_json::to_string(&all()).expect("registry serializes");
        assert!(json.contains("supertrend"));
        assert!(json.contains("choppiness"));
    }

    #[test]
    fn test_directional_and_gate_counts() {
        let gates = INDICATORS.iter().filter(|m| !m.directional).count();
        // atr, bbwp, hv, volume, rvol, choppiness
        //
        // CR-10 removed four of the ten: funding_rate, spread, open_interest and
        // mark_index_spread were all non-candle.
        assert_eq!(gates, 6, "CR-10: 6 non-directional gate indicators remain");
    }

    /// CR-10 inverted this regression.
    ///
    /// The original test existed because `mark_index_spread` was computed by
    /// the derivatives injector but had no registry entry, so the Metrics table
    /// filtered it out and the row was invisible. The fix was to register it.
    ///
    /// The real defect was one level up: an indicator fed by mark/index prices
    /// was being computed at all. CH-1 forbids it, so the injector is gone
    /// (P5) and the entry with it. The test now asserts the absence, because an
    /// "is it registered?" check cannot fail any more — there is nothing to
    /// register.
    #[test]
    fn no_indicator_declares_a_non_candle_data_source() {
        for key in [
            "mark_index_spread",
            "spread",
            "depth_bias",
            "order_flow_imbalance",
            "open_interest",
            "oi_delta",
            "funding_rate",
            "oi_price_divergence",
        ] {
            assert!(
                get(key).is_none(),
                "CR-10 / CH-1: `{key}` is computed from a non-OHLCV feed and must \
                 not be registered. A candle archive cannot reproduce it, so replay \
                 can never validate it."
            );
        }
    }

    #[test]
    fn test_anchored_vwap_sub_key_resolves_to_weekly() {
        // Regression: the registry's `value_source` must match the key
        // that `normalize_anchored_vwap` inserts into the `values` submap.
        // The legacy `sub:vwap_weekly` never matched any inserted key
        // (which used `weekly` / `monthly` / `swing`) and rendered `--`
        // in the Metrics Raw column.
        let avwap = get("anchored_vwap").expect("anchored_vwap registered");
        assert_eq!(
            avwap.value_source, "sub:weekly",
            "anchored_vwap registry sub-key must point at the `weekly` \
             level that the normalizer inserts (not `sub:vwap_weekly`)",
        );
    }

    #[test]
    fn test_all_weights_are_one() {
        for m in INDICATORS {
            assert!(
                (m.default_weight - 1.0).abs() < 1e-9,
                "indicator '{}' has non-1.0 weight: {}",
                m.key,
                m.default_weight
            );
        }
    }

    #[test]
    fn test_max_bars_required_is_300() {
        let max_bars = INDICATORS
            .iter()
            .map(|m| m.bars_required)
            .max()
            .unwrap_or(0);
        assert_eq!(
            max_bars, INDICATORS_MAX_BARS_REQUIRED,
            "INDICATORS_MAX_BARS_REQUIRED must match the actual maximum bars_required"
        );
        // The invariant is the price_trend_sharpe window — equal to the
        // canonical [candle_buffer] size (300) so the indicator goes Live
        // exactly when the pipeline buffer fills.
        let sharpe = get("price_trend_sharpe").expect("price_trend_sharpe registered");
        assert_eq!(sharpe.bars_required, INDICATORS_MAX_BARS_REQUIRED);
    }

    #[test]
    fn test_normalization_mode_context_only_matches_non_directional_gates() {
        // Every registry indicator flagged `directional: false` must also be
        // wired to `ContextOnly` so the Metrics column renders `N/A` and
        // the directional accumulator skips them. Drift between the two
        // flags is the regression this test catches.
        for m in INDICATORS {
            if !m.directional {
                assert_eq!(
                    normalization_mode_for(m),
                    IndicatorNormalizationMode::ContextOnly,
                    "non-directional gate '{}' must be ContextOnly",
                    m.key,
                );
            }
        }
    }

    #[test]
    fn test_normalization_mode_hull_ma_is_event_only() {
        let hma = get("hull_ma").expect("hull_ma registered");
        assert_eq!(
            normalization_mode_for(hma),
            IndicatorNormalizationMode::EventOnly,
            "Hull MA is the canonical event-only overlay indicator"
        );
    }

    #[test]
    fn test_normalization_mode_default_is_directional() {
        // Verify the explicit per-entry wire: 10 context-only + 1 event-only
        // entries; everything else stays Directional (registry default).
        // `mark_index_spread` was added in the parity sweep (Phase 1.2).
        let ctx_only: Vec<&str> = INDICATORS
            .iter()
            .filter(|m| {
                matches!(
                    normalization_mode_for(m),
                    IndicatorNormalizationMode::ContextOnly
                )
            })
            .map(|m| m.key)
            .collect();
        let event_only: Vec<&str> = INDICATORS
            .iter()
            .filter(|m| {
                matches!(
                    normalization_mode_for(m),
                    IndicatorNormalizationMode::EventOnly
                )
            })
            .map(|m| m.key)
            .collect();
        assert_eq!(
            ctx_only.len(),
            6,
            "CR-10: 6 context-only gate indicators remain"
        );
        assert!(
            !ctx_only.iter().any(|k| k.contains("mark_index")
                || k.contains("spread")
                || k.contains("open_interest")
                || k.contains("funding")),
            "CR-10: no non-candle indicator may sit in the ContextOnly bucket"
        );
        assert_eq!(
            event_only.len(),
            1,
            "1 event-only indicator expected (Hull MA)"
        );
        assert!(event_only.contains(&"hull_ma"));
    }

    #[test]
    fn test_is_directional_norm_helper_matches_normalization_mode() {
        // The `is_directional_norm()` helper is the single source of truth
        // for the UI Norm column. Every Directional indicator must report
        // `true`; every other mode must report `false`.
        for m in INDICATORS {
            let expected = matches!(
                normalization_mode_for(m),
                IndicatorNormalizationMode::Directional
            );
            assert_eq!(
                is_directional_norm(m),
                expected,
                "is_directional_norm mismatch for {}",
                m.key,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Axis identity — CR-18
// ---------------------------------------------------------------------------

/// **One observable = one indicator = exactly one owner axis.**
///
/// The rule is about what a *trader recognises*, not how many passes over the
/// candles an indicator costs. `rsi`, `rsi_slope` and `rsi_divergence` are three
/// recognisable facts about the same calculation, so they are three observables on
/// three axes — expressed with [`calculator_for`], not by duplicating the series.
///
/// The cluster in [`IndicatorCluster`] is **not** this. A cluster groups indicators
/// that read the same source series for the reader's convenience and has no weight
/// effect. An axis is where a reading votes.
///
/// Every one of the 44 shipped indicators is listed. Progression has **no**
/// present entry, which is not an oversight: the corpus puts 3 present readings on
/// Progression and CR-16's measurement-quality audit cuts the 22 planned ones to
/// 11–13. Progression is therefore almost entirely Block 2 work, and the axis is
/// empty until those batches land.
// ---------------------------------------------------------------------------
// The reading registry — CR-4 / CR-5, invariants I27–I36
// ---------------------------------------------------------------------------

/// What a reading is *for*.
///
/// `diagnostic` readings describe the market but never vote on an axis — they
/// are the reason `diagnostics` is not a fifth axis (CR-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingRole {
    /// Enters its owner axis's aggregation.
    Scoring,
    /// Describes; never votes.
    Diagnostic,
}

/// One reading — the unit CR-5 makes authoritative.
///
/// **A reading is not an indicator.** One indicator can yield several readings,
/// and a reading can be derived from other readings rather than produced
/// directly. That is why this table, not `INDICATORS`, is the source of every
/// displayed total: the registry answers "how many readings are there?", and the
/// answer is not "how many indicators are there?".
///
/// # I27 — producer XOR derived_from
///
/// Every reading resolves to a declared producer **or** an explicit
/// `derived_from` list. Never both, never neither (I33).
///
/// # I30 — ownership single, consumption many
///
/// `owner_axis` is exactly one axis. `consumers` may name others. An indicator
/// that three axes read still votes once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Stable ID (I28). Never renumbered; a retired reading is tombstoned.
    pub id: &'static str,
    /// The indicator key that produces this reading, or `None` when derived.
    pub producer: Option<&'static str>,
    /// Other reading IDs this is derived from. Non-empty iff `producer` is
    /// `None` (I33).
    pub derived_from: &'static [&'static str],
    /// Exactly one axis (I30).
    pub owner_axis: AxisId,
    /// Axes permitted to consume this reading. May exceed one (I30).
    pub consumers: &'static [AxisId],
    /// Readings that cannot disagree and share one weight. Rare (CR-18).
    pub family: Option<&'static str>,
    /// Indicators reading the same source series. **Display only, no weight.**
    pub cluster: &'static str,
    /// Whether it votes (I34).
    pub role: ReadingRole,
    /// The calculator whose code produces the series. Shared code, distinct keys.
    pub calculator: &'static str,
    /// The candle fields it reads (CH-1). The only legal roots.
    pub inputs: &'static [CandleField],
}

const DIRECTION: [AxisId; 1] = [AxisId::Direction];
const MOMENTUM: [AxisId; 1] = [AxisId::Momentum];
const STRUCTURE: [AxisId; 1] = [AxisId::Structure];

/// Declare a reading produced directly by an indicator that owns its axis.
///
/// The overwhelmingly common case, so it gets a one-liner. The verbosity of the
/// full struct at 157 rows is not affordable and not worth paying for.
macro_rules! direct {
    ($id:literal, $axis:expr, $cluster:literal, $inputs:expr) => {
        Reading {
            id: $id,
            producer: Some($id),
            derived_from: &[],
            owner_axis: $axis,
            consumers: &[$axis],
            family: None,
            cluster: $cluster,
            role: ReadingRole::Scoring,
            calculator: $id,
            inputs: $inputs,
        }
    };
}

/// Every reading the platform knows about, present and planned.
///
/// **157 when complete** (CR-18): Direction 50 · Momentum 48 · Structure 34 ·
/// Progression 25. Today it carries the 44 that exist; P12 appends the batches
/// and the totals are counted from here, never typed by hand (I31, I36).
pub const READINGS: &[Reading] = &[
    // ── Direction — where price is going ─────────────────────────────────
    direct!(
        "ema_stack",
        AxisId::Direction,
        "trend",
        &[CandleField::Close]
    ),
    direct!(
        "supertrend",
        AxisId::Direction,
        "trend",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "donchian",
        AxisId::Direction,
        "trend",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "keltner",
        AxisId::Direction,
        "trend",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "vwap",
        AxisId::Direction,
        "trend",
        &[
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "anchored_vwap",
        AxisId::Direction,
        "trend",
        &[
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "ichimoku",
        AxisId::Direction,
        "trend",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!("hull_ma", AxisId::Direction, "trend", &[CandleField::Close]),
    direct!(
        "psar",
        AxisId::Direction,
        "trend",
        &[CandleField::High, CandleField::Low]
    ),
    direct!(
        "linreg_slope",
        AxisId::Direction,
        "trend_quality",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "aroon",
        AxisId::Direction,
        "trend_quality",
        &[CandleField::High, CandleField::Low]
    ),
    direct!(
        "zscore",
        AxisId::Direction,
        "trend_quality",
        &[CandleField::Close]
    ),
    // ── Momentum — how fast, and whether participation supports it ────────
    // `rsi` is the calculator for three observables, not one: `rsi` here,
    // `rsi_slope` (Momentum) and `rsi_divergence` (Structure) arrive in P12 as
    // separate readings with `calculator: "rsi"`. That is the whole point of the
    // `calculator` field — three keys, one pass over the candles.
    direct!("rsi", AxisId::Momentum, "momentum", &[CandleField::Close]),
    Reading {
        id: "stochastic",
        producer: Some("stochastic"),
        derived_from: &[],
        owner_axis: AxisId::Momentum,
        consumers: &MOMENTUM,
        family: Some("oscillator_saturated"),
        cluster: "momentum",
        role: ReadingRole::Scoring,
        calculator: "stochastic",
        inputs: &[CandleField::High, CandleField::Low, CandleField::Close],
    },
    Reading {
        id: "williams_r",
        producer: Some("williams_r"),
        derived_from: &[],
        owner_axis: AxisId::Momentum,
        consumers: &MOMENTUM,
        family: Some("oscillator_saturated"),
        cluster: "momentum",
        role: ReadingRole::Scoring,
        calculator: "williams_r",
        inputs: &[CandleField::High, CandleField::Low, CandleField::Close],
    },
    direct!(
        "chandemo",
        AxisId::Momentum,
        "momentum",
        &[CandleField::Close]
    ),
    direct!(
        "awesome_oscillator",
        AxisId::Momentum,
        "momentum",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "cci",
        AxisId::Momentum,
        "momentum",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!("macd", AxisId::Momentum, "momentum", &[CandleField::Close]),
    direct!(
        "adx",
        AxisId::Momentum,
        "trend_quality",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "choppiness",
        AxisId::Momentum,
        "trend_quality",
        &[CandleField::High, CandleField::Low]
    ),
    direct!(
        "price_trend_sharpe",
        AxisId::Momentum,
        "trend_quality",
        &[CandleField::Close]
    ),
    direct!(
        "bbwp",
        AxisId::Momentum,
        "volatility",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "squeeze",
        AxisId::Momentum,
        "volatility",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!("volume", AxisId::Momentum, "volume", &[CandleField::Volume]),
    direct!("rvol", AxisId::Momentum, "volume", &[CandleField::Volume]),
    direct!(
        "obv",
        AxisId::Momentum,
        "volume",
        &[CandleField::Close, CandleField::Volume]
    ),
    direct!(
        "cmf",
        AxisId::Momentum,
        "volume",
        &[
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "mfi",
        AxisId::Momentum,
        "volume",
        &[
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "force_index",
        AxisId::Momentum,
        "volume",
        &[CandleField::Close, CandleField::Volume]
    ),
    // ── Structure — is the structure intact, and what is priced ──────────
    direct!(
        "fibonacci",
        AxisId::Structure,
        "structure",
        &[CandleField::High, CandleField::Low]
    ),
    direct!(
        "support_resistance",
        AxisId::Structure,
        "structure",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "pivot_points",
        AxisId::Structure,
        "structure",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "patterns",
        AxisId::Structure,
        "structure",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "candlestick",
        AxisId::Structure,
        "structure",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "smc_structure",
        AxisId::Structure,
        "institutional",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "smc_liquidity",
        AxisId::Structure,
        "institutional",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "smc_fvg",
        AxisId::Structure,
        "institutional",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "smc_order_blocks",
        AxisId::Structure,
        "institutional",
        &[
            CandleField::Open,
            CandleField::High,
            CandleField::Low,
            CandleField::Close
        ]
    ),
    direct!(
        "volume_profile",
        AxisId::Structure,
        "volume",
        &[
            CandleField::High,
            CandleField::Low,
            CandleField::Close,
            CandleField::Volume
        ]
    ),
    direct!(
        "bollinger",
        AxisId::Structure,
        "volatility",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "stddev_channel",
        AxisId::Structure,
        "volatility",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!(
        "atr",
        AxisId::Structure,
        "volatility",
        &[CandleField::High, CandleField::Low, CandleField::Close]
    ),
    direct!("hv", AxisId::Structure, "volatility", &[CandleField::Close]),
];

/// Look up a reading by its stable ID.
pub fn reading(id: &str) -> Option<&'static Reading> {
    READINGS.iter().find(|r| r.id == id)
}

/// Every reading that enters its axis's aggregation (I34).
pub fn scoring_readings() -> impl Iterator<Item = &'static Reading> {
    READINGS.iter().filter(|r| r.role == ReadingRole::Scoring)
}

/// The count per axis, computed rather than typed (I31, I36).
pub fn axis_totals() -> std::collections::BTreeMap<AxisId, usize> {
    let mut m = std::collections::BTreeMap::new();
    for r in READINGS {
        *m.entry(r.owner_axis).or_insert(0) += 1;
    }
    m
}

pub const AXIS_ASSIGNMENT: &[(&str, AxisId)] = &[];

/// The axis a reading votes on.
pub fn owner_axis(key: &str) -> Option<AxisId> {
    reading(key).map(|r| r.owner_axis)
}

/// Which calculator produces this observable's series.
///
/// Defaults to the observable's own key. Only observables that share code declare
/// a different calculator — `rsi`, `rsi_slope` and `rsi_divergence` are three keys
/// over one `rsi` calculator (Block 2, when the latter two exist).
///
/// Takes `&'static str` because a registry key is already static, so the default
/// path allocates nothing. An empty [`CALCULATOR_OVERRIDES`] is not an omission —
/// it means every current observable is its own calculator, and a non-empty table
/// is what appears the moment two observables share one.
pub fn calculator_for(key: &'static str) -> &'static str {
    reading(key).map(|r| r.calculator).unwrap_or(key)
}

/// Observables whose series comes from a calculator other than their own key.
pub const CALCULATOR_OVERRIDES: &[(&str, &str)] = &[];

/// **Families** — indicators that *cannot disagree* and therefore share one weight.
///
/// A family is a genuine redundancy. `williams_r ≡ stochastic_k − 100` is the only
/// confirmed case today: if both vote, they vote identically, so counting them
/// twice would double-weight one number. A second candidate must survive the
/// incremental-information test before being admitted.
///
/// This is deliberately not the same as a cluster. A cluster has no weight effect
/// at all; a family has exactly one.
pub const FAMILIES: &[(&str, &[&str])] = &[];

/// The family this observable belongs to, if any.
pub fn family_for(key: &str) -> Option<&'static str> {
    reading(key).and_then(|r| r.family)
}

/// Every observable in a family, for the single-weight aggregation.
pub fn family_members(family: &str) -> Vec<&'static str> {
    READINGS
        .iter()
        .filter(|r| r.family == Some(family))
        .map(|r| r.id)
        .collect()
}

impl IndicatorMeta {
    /// The display-only cluster this indicator belongs to.
    ///
    /// The struct field stays named `group` because it is **on the wire** — the
    /// frontend reads `group` off `/api/config` and the Metrics table groups by it.
    /// Renaming the serialised key would break the UI contract for no behavioural
    /// gain, so the Rust type is `IndicatorCluster` and this accessor is the
    /// Rust-side name. The rename is about what a cluster *is* (display-only, no
    /// weight), not about its field spelling.
    pub fn cluster(&self) -> IndicatorCluster {
        self.group
    }

    /// The axis this indicator's reading votes on.
    pub fn owner_axis(&self) -> Option<AxisId> {
        owner_axis(self.key)
    }

    /// The calculator that produces this observable's series.
    pub fn calculator(&self) -> &'static str {
        calculator_for(self.key)
    }

    /// The family this observable belongs to, if any.
    pub fn family(&self) -> Option<&'static str> {
        family_for(self.key)
    }
}

// ---------------------------------------------------------------------------
// Candle purity — CH-1
// ---------------------------------------------------------------------------

/// The **only** fields a reading may be derived from (CH-1).
///
/// `NormalizedCandle` also carries `exchange`, `symbol`, `start_time_ms`,
/// `duration_ms`, `trades_count` and `reconstructed`. Those *describe* the
/// candle rather than measure the market, so they are deliberately absent: a
/// reading that reported "this candle came from Bitget" would be a fact about
/// ingestion, not about price.
///
/// Declaring the roots as an enum rather than strings means the purity test
/// cannot be satisfied by a typo — an unlisted field does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CandleField {
    Open,
    High,
    Low,
    Close,
    Volume,
}

/// The candle fields each indicator reads.
///
/// Every one of the 44 shipped readings is declared. A new indicator that does
/// not appear here has no declared root, and [`candle_purity`] fails — which is
/// the point: "I did not think about what it reads" must not be able to pass.
pub const CANDLE_INPUTS: &[(&str, &[CandleField])] = &[];

/// The candle fields this indicator reads. `None` means "undeclared", which
/// [`candle_purity`] treats as a failure rather than as an omission.
pub fn candle_inputs(key: &str) -> Option<&'static [CandleField]> {
    reading(key).map(|r| r.inputs)
}
