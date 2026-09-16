// HOT PATH — real-time indicator pipeline.
// Receives live NormalizedEvents from the exchange adapter layer,
// builds candles, runs 52 indicators, and broadcasts MarketSnapshots.
// This is the critical data path; operations must be non-blocking.
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

use config_models::FibonacciConfig;
use config_models::OrderBookConfig;
use config_models::QualityConfig;
use config_models::StrategyConfig;
use config_models::TimeframeConfig;
use config_models::{HeatmapConfig, LiquidityConfig};
use network_adapters::pipeline_reliability::ReliabilityTracker;

use crate::candle_generator::CandleGenerator;
use crate::indicators::normalized::NormalizedIndicatorValue;
use crate::indicators::normalized::PreviousBarState;
use crate::indicators::{
    detect_pattern, Adx, AnchoredVwap, Aroon, Atr, AwesomeOscillator, Bbwp, BollingerBands,
    Candlestick, CandlestickConfig, Cci, ChandeMO, Choppiness, Cmf, DivergenceDetector, Donchian,
    Ema, FibonacciRange, ForceIndex, HistoricalVolatility, HullMA, Ichimoku, Keltner, LinRegSlope,
    Macd, Mfi, Obv, OrderBookAnalysis, ParabolicSar, PivotMethod, PivotPoints, Rsi,
    SeriesDivergence, SmartMoney, SqueezeMomentum, StdDevChannel, Stochastic, Supertrend,
    VolumeProfile, WilliamsR, ZScore,
};
use crate::sr_engine::SrRoleTracker;
use core_domain::advisory::AdvisoryMatrix;
use core_domain::indicator_dtos::{IndicatorLifecycleMap, IndicatorLifecycleState};
use core_domain::liquidity::LiquidationClusterMatrix;
use core_domain::models::{
    CandlePipelineState, CandleQualityEnvelope, MarketSnapshot, SequenceIntegrity, TimeframeSlot,
};
use core_domain::normalized::{Exchange, NormalizedCandle, NormalizedEvent};
use core_domain::statistics::{StatisticsConfig, StatisticsEngine};
use core_domain::volume_profile::{VolumeProfileBin, VolumeProfileSnapshot};

pub mod normalize;
pub mod warm;
pub use warm::{warm_indicators_for_timeframe, WarmedPipelineState, HIST_BUFFER_MAX};

/// M2 (production audit): fire-and-forget telemetry with a bounded drop
/// policy. The 10k-slot channel is drained serially by the SQLite logger;
/// when a DB stall (or an hourly retention `DELETE`) fills it, every
/// awaited `send().await` in the hot path froze `run_single` — the whole
/// MME analysis + WS broadcast stalled. `try_send` drops instead; the
/// drop counter keeps the loss observable (logged on the first drop and
/// every 500th).
static TELEMETRY_DROPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn send_telemetry(
    tx: &tokio::sync::mpsc::Sender<database_storage::TelemetryMsg>,
    msg: database_storage::TelemetryMsg,
) {
    use std::sync::atomic::Ordering;
    if tx.try_send(msg).is_err() {
        let n = TELEMETRY_DROPS.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 || n % 500 == 0 {
            eprintln!(
                "⚠️ telemetry channel full — dropped {} message(s) (SQLite logger stalled?)",
                n
            );
        }
    }
}

/// AUDIT-M11: classify a candle's arrival order against the previously
/// processed candle — the envelope's `sequence_integrity` field was
/// hardcoded `Valid` at every construction site, so the DIE L3 sequence
/// audit never recorded an out-of-order or duplicate candle.
fn classify_sequence(prev_start_ms: Option<u64>, start_ms: u64) -> SequenceIntegrity {
    match prev_start_ms {
        Some(prev) if start_ms < prev => SequenceIntegrity::OutOfOrder,
        Some(prev) if start_ms == prev => SequenceIntegrity::Duplicate,
        _ => SequenceIntegrity::Valid,
    }
}

/// Canonical buffer size from `[candle_buffer] size` (CB-01) — the
/// historical warmup depth. Used as the higher-tier system-gate (Layer 2) —
/// the pipeline transitions `Loading → Live` when the buffer reaches this
/// count. Independent of the indicator floor (`INDICATORS_MAX_BARS_REQUIRED
/// = 300`) and the absolute cap (`HIST_BUFFER_MAX = 1000`). Default 500.
pub const DEFAULT_BUFFER_SIZE: usize = 500;

pub struct TimeframePipeline {
    /// Stable slot identity. The frontend never has to re-derive slot from
    /// `timeframe_secs` because every snapshot carries `timeframe_slot` and
    /// every chart component renders the slot the pipeline was constructed
    /// with. Allowed at construction: `Micro | Fast | Slow | Macro`.
    pub slot: TimeframeSlot,
    pub history: Arc<RwLock<VecDeque<NormalizedCandle>>>,
    pub broadcast_tx: broadcast::Sender<MarketSnapshot>,
    pub latest_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    pub snapshot_history: Arc<RwLock<VecDeque<MarketSnapshot>>>,
    pub timeframe_secs: u64,
    pub timeframe_label: &'static str,
    pub divergence_detector: Arc<tokio::sync::Mutex<DivergenceDetector>>,
    pub sr_tracker: Arc<tokio::sync::Mutex<SrRoleTracker>>,
    pub fibonacci: FibonacciConfig,
    /// Latest Open Interest (shared across timeframes, updated by WS OI events).
    pub latest_oi: Arc<RwLock<Option<Decimal>>>,
    /// Latest Funding Rate (shared across timeframes, updated by WS funding events).
    pub latest_funding: Arc<RwLock<Option<Decimal>>>,
    /// Latest Mark Price (shared across timeframes, updated by mark events).
    pub latest_mark_px: Arc<RwLock<Option<Decimal>>>,
    /// Latest Index Price (shared across timeframes).
    pub latest_index_px: Arc<RwLock<Option<Decimal>>>,
    /// Active indicator/signal activation set (from config).
    pub active_set: crate::active_set::ActiveSet,
    /// Latest LiquidationClusterMatrix **per-timeframe** (Phase 2). Updated
    /// by the per-TF cluster refresh task at the TF's own candle cadence.
    /// The analyzer reads this and attaches it to each completed snapshot
    /// as `MarketSnapshot.cluster`, so every TF chart in the dashboard
    /// shows clusters at its own horizon (micro=fast-magnet, macro=slow-magnet).
    pub cluster_matrix: Arc<RwLock<Option<core_domain::liquidity::LiquidationClusterMatrix>>>,
    /// Per-TF cluster-refresh status snapshot. Sibling to `cluster_matrix`
    /// so the `/api/liquidity/cluster-status` handler can distinguish
    /// "no data yet" (Pending) from "refresh task failed and is silently
    /// retrying" (Skipped with reason) — without this, the LIQ HEATMAP
    /// overlay can appear empty for minutes with no operator feedback.
    pub cluster_status: Arc<RwLock<core_domain::liquidity::ClusterStatusSnapshot>>,
    /// Per-TF pipeline lifecycle state. Transitions per
    /// [03-01-06](../docs/engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)
    /// DCP-01 … DCP-15. Read by `run_single` to populate
    /// `MarketSnapshot.pipeline_state`.
    pub pipeline_state: Arc<RwLock<CandlePipelineState>>,
    /// Per-indicator operational lifecycle map for this TF.
    /// Read by `run_single` to populate `MarketSnapshot.indicator_lifecycle`.
    /// See [03-02-15](../docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)
    /// ILS-01 … ILS-15.
    pub indicator_lifecycle: Arc<RwLock<IndicatorLifecycleMap>>,
    /// v6.10 (Phase 2 / B3): latest cross-TF Advisory Matrix promoted onto
    /// the pipeline struct. `synthesize_cross_tf` writes the freshly
    /// computed advisory here; `apply_snapshot_to_timeframe` reads it
    /// and binds it onto the emitted `MarketSnapshot.advisory`. This
    /// mirrors the promotion of `pipeline_state` and `indicator_lifecycle`
    /// so the pipeline struct carries authoritative state for all
    /// cross-TF outputs, not just per-TF state.
    pub advisory: Arc<RwLock<Option<AdvisoryMatrix>>>,
    /// v6.10 (Phase 2 / B4): per-TF leverage configuration feeding the
    /// cluster estimator. Defaults match the legacy hardcoded
    /// `[1, 3, 5, 10, 20, 50, 100]` / `[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05]`
    /// distribution. Per-TF kill switch `enabled = false` suppresses the
    /// refresh task for that TF.
    pub tf_leverage_config: Arc<config_models::TfLeverageConfig>,
    /// Canonical buffer size from `[candle_buffer] size` (CB-01). Used for
    /// the buffer-full check that triggers `LOADING → LIVE` (DCP-04).
    pub buffer_size: usize,
    /// Per-TF stale threshold (CB-04 / DCP-05 / ILS-07).
    pub stale_threshold_secs: u64,
}

pub struct ActivePair {
    pub symbol: String,
    /// Fixed 10-slot ladder (fastest → slowest): micro1=1s, micro2=3s,
    /// fast1=5s, fast2=15s, slow1=30s, slow2=60s, macro1=180s, macro2=300s,
    /// longterm1=900s, longterm2=3600s. Positionally aligned with
    /// `core_domain::FIXED_TF_SLOTS` / `config_models::FIXED_TF_LADDER`.
    pub micro1: TimeframePipeline,
    pub micro2: TimeframePipeline,
    pub fast1: TimeframePipeline,
    pub fast2: TimeframePipeline,
    pub slow1: TimeframePipeline,
    pub slow2: TimeframePipeline,
    pub macro1: TimeframePipeline,
    pub macro2: TimeframePipeline,
    pub longterm1: TimeframePipeline,
    pub longterm2: TimeframePipeline,
    /// Operator-defined custom pipelines (`TimeframeSlot::Custom { id }`).
    /// Always empty for the fixed ladder; retained for non-ladder duration
    /// dispatch (`Custom` slots from ad-hoc resolution).
    pub custom_pipelines: std::collections::HashMap<u16, TimeframePipeline>,
    /// v11.2: how many of the fastest slots actually run (1..=10). Slots
    /// beyond this exist as inert pipelines (never spawned, never emit).
    pub active_count: usize,
    pub snapshot_tx: tokio::sync::mpsc::Sender<NormalizedEvent>,
    pub cancel: CancellationToken,
    /// Latest Open Interest (shared across all timeframes, updated by WS events).
    pub latest_oi: Arc<RwLock<Option<Decimal>>>,
    /// Latest Funding Rate (shared across all timeframes, updated by WS funding events).
    pub latest_funding: Arc<RwLock<Option<Decimal>>>,
    /// Latest Mark Price (shared across all timeframes, updated by mark events).
    pub latest_mark_px: Arc<RwLock<Option<Decimal>>>,
    /// Latest Index Price (shared across all timeframes).
    pub latest_index_px: Arc<RwLock<Option<Decimal>>>,
    /// Rolling OI history — `(timestamp_secs, value)` samples, time-bounded
    /// to a 3600 s window (AUDIT-AIU-051). Promoted from a
    /// per-`run_single` local so warmup can restore historical samples and
    /// the first candle after boot has `OI Delta` math anchored to real
    /// data. Each TF pipeline owns a clone of this at spawn; the window is
    /// evaluated per-TF against each TF's own candle cadence.
    pub oi_history: Arc<RwLock<VecDeque<(u64, f64)>>>,
    /// Rolling funding-rate history (shared across all timeframes) —
    /// bounded to 8 samples. Restored from history at boot so
    /// `OI_FUNDING_DIVERGENCE` and `FUNDING_FLIP` have non-zero
    /// priors instead of firing on the first funding event post-boot.
    pub funding_history: Arc<RwLock<VecDeque<f64>>>,
    /// Cross-cutting latency telemetry (ingest skew, observation loop,
    /// heartbeat) for the DIE observation path.
    pub latency_tracker: core_domain::SharedLatencyTracker,
}

impl ActivePair {
    /// All ten fixed-ladder pipelines, fastest → slowest.
    pub fn all(&self) -> [&TimeframePipeline; 10] {
        [
            &self.micro1, &self.micro2, &self.fast1, &self.fast2, &self.slow1, &self.slow2,
            &self.macro1, &self.macro2, &self.longterm1, &self.longterm2,
        ]
    }

    /// O(1) slot-based dispatch. Replaces the legacy `pipeline_for(secs)`
    /// linear lookup that collapsed duplicate durations and silently
    /// defaulted to `micro` for any unmatched frame.
    pub fn pipeline_for_slot(&self, slot: TimeframeSlot) -> Option<&TimeframePipeline> {
        match slot {
            TimeframeSlot::Micro1 => Some(&self.micro1),
            TimeframeSlot::Micro2 => Some(&self.micro2),
            TimeframeSlot::Fast1 => Some(&self.fast1),
            TimeframeSlot::Fast2 => Some(&self.fast2),
            TimeframeSlot::Slow1 => Some(&self.slow1),
            TimeframeSlot::Slow2 => Some(&self.slow2),
            TimeframeSlot::Macro1 => Some(&self.macro1),
            TimeframeSlot::Macro2 => Some(&self.macro2),
            TimeframeSlot::Longterm1 => Some(&self.longterm1),
            TimeframeSlot::Longterm2 => Some(&self.longterm2),
            TimeframeSlot::Custom { id } => self.custom_pipelines.get(&id),
        }
    }

    /// Legacy shim for callers that still key on a duration. Picks the
    /// matching slot; when multiple slots share the same duration, the
    /// fastest matching pipeline is returned deterministically. Returns
    /// `Err` only when no slot at all matches the requested duration.
    pub fn pipeline_for_duration(&self, timeframe_secs: u64) -> Result<&TimeframePipeline, String> {
        for p in self.all() {
            if p.timeframe_secs == timeframe_secs {
                return Ok(p);
            }
        }
        Err(format!("No slot matches timeframe_secs={timeframe_secs}"))
    }

    pub fn subscribe_broadcast_by_slot(
        &self,
        slot: TimeframeSlot,
    ) -> Option<broadcast::Receiver<MarketSnapshot>> {
        self.pipeline_for_slot(slot)
            .map(|p| p.broadcast_tx.subscribe())
    }

    pub async fn latest_close_str(&self) -> Option<String> {
        let hist = self.micro1.history.read().await;
        hist.back().map(|c| c.close.to_string())
    }

    pub async fn latest_price(&self) -> Option<f64> {
        let snap = self.micro1.latest_snapshot.read().await;
        snap.as_ref()
            .and_then(|s| s.mid_price.to_string().parse::<f64>().ok())
    }

    /// Latest completed `MarketSnapshot` for a given timeframe slot.
    ///
    /// The WS handler uses this to bootstrap a fresh socket: it replays
    /// the most recent completed snapshot before the next live tick, so
    /// the frontend Metrics table is populated immediately rather than
    /// waiting `candle_buffer.size × timeframe_secs` for the first
    /// shadow tick.
    pub async fn latest_snapshot_for_slot(&self, slot: TimeframeSlot) -> Option<MarketSnapshot> {
        self.pipeline_for_slot(slot)?
            .latest_snapshot
            .read()
            .await
            .clone()
    }

    pub async fn snapshot_history_vec(&self, slot: TimeframeSlot) -> Vec<MarketSnapshot> {
        let Some(p) = self.pipeline_for_slot(slot) else {
            return Vec::new();
        };
        let hist = p.snapshot_history.read().await;
        hist.iter().cloned().collect()
    }

    pub async fn snapshot_history_vec_for_secs(&self, timeframe_secs: u64) -> Vec<MarketSnapshot> {
        match self.pipeline_for_duration(timeframe_secs) {
            Ok(p) => {
                let hist = p.snapshot_history.read().await;
                hist.iter().cloned().collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// AUDIT-AIU-121: slot-authoritative history — used by `/api/history`
    /// when the caller passes `?slot=`. Unlike the duration-only shim above,
    /// this resolves the EXACT pipeline, so duplicate-duration slots never
    /// cross-wire each other's history. Falls back to the duration shim when
    /// the slot hint is absent or unknown.
    pub async fn snapshot_history_vec_for_slot_or_secs(
        &self,
        slot: Option<&str>,
        timeframe_secs: u64,
    ) -> Vec<MarketSnapshot> {
        if let Some(resolved) = slot
            .map(core_domain::models::TimeframeSlot::parse)
            .and_then(|s| self.pipeline_for_slot(s))
        {
            let hist = resolved.snapshot_history.read().await;
            return hist.iter().cloned().collect();
        }
        // Best-effort: no hint, or a custom-slot hint this pair doesn't run —
        // fall back to the duration shim (the WS live path remains
        // slot-authoritative regardless).
        match self.pipeline_for_duration(timeframe_secs) {
            Ok(p) => {
                let hist = p.snapshot_history.read().await;
                hist.iter().cloned().collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Latest completed snapshot for each of the ten fixed-ladder
    /// timeframes (fastest → slowest), for cross-timeframe synthesis.
    pub async fn latest_snapshots_all_tf(
        &self,
    ) -> [Option<MarketSnapshot>; 10] {
        let mut out = [None, None, None, None, None, None, None, None, None, None];
        for (i, p) in self.all().iter().enumerate() {
            out[i] = p.latest_snapshot.read().await.clone();
        }
        out
    }
}

pub async fn run_event_router(
    mut rx: Receiver<NormalizedEvent>,
    pipeline_txs: Vec<Sender<NormalizedEvent>>,
    symbol: String,
    cancel: CancellationToken,
) {
    println!(
        "🔄 Event Router: Started for {} (fanning out to {} timeframes)...",
        symbol,
        pipeline_txs.len()
    );

    loop {
        let event = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Event Router: {} cancelled, shutting down.", symbol);
                break;
            }
            result = rx.recv() => {
                match result {
                    Some(e) => e,
                    None => {
                        println!("🛑 Event Router: {} upstream channel closed.", symbol);
                        break;
                    }
                }
            }
        };

        // AUDIT-L6: `send().await` blocked the router when any single
        // pipeline's 200-cap channel was full — one stalled TF (slow
        // consumer, backpressure) stalled the fan-out to ALL TFs.
        // `try_send` drops the event for the congested TF only; the
        // broadcast channel's `Lagged` handling + the TF's own
        // staleness machinery keep the pipeline consistent.
        if let Some((last, others)) = pipeline_txs.split_last() {
            for tx in others {
                let _ = tx.try_send(event.clone());
            }
            let _ = last.try_send(event);
        }
    }
}

// DIE L3 median price filter (03-01-04 §4.1), owned by `network-adapters`
// (the DIE crate). See `crates/network-adapters/src/median_filter.rs`.
use network_adapters::median_filter::{FilterVerdict, MedianPriceFilter};

/// Venue REST coordinates for DIE L3 quarantine-refetch and runtime
/// gap-filling (03-01-04 §2.1.2 / §4.2). Built once per pipeline by the
/// registry from the active exchange choice.
#[derive(Clone)]
pub struct RestRefetchSpec {
    pub is_bitget: bool,
    /// Exchange-native symbol (e.g. Hyperliquid "BTC", Bitget "BTCUSDT").
    pub exchange_raw: String,
    /// Bitget product type ("" on other venues).
    pub product_type: String,
    pub rest_url: String,
}

/// Timeout for the rare quarantine/gap REST refetch so a venue stall can
/// never wedge the analysis loop.
const REFETCH_TIMEOUT_SECS: u64 = 5;

/// Fetch `[start_ms, end_ms)` candles of `duration_secs` from the venue REST
/// history. Returns an empty vector on error/timeout (callers treat that as
/// "gap remains open").
async fn fetch_interval_candles(
    spec: &RestRefetchSpec,
    internal_symbol: &str,
    start_ms: u64,
    end_ms: u64,
    duration_secs: u64,
) -> Vec<NormalizedCandle> {
    let fut = async {
        if spec.is_bitget {
            let interval =
                network_adapters::adapters::bitget_rest::timeframe_secs_to_interval(duration_secs);
            network_adapters::adapters::bitget_rest::fetch_historical_candles(
                &spec.exchange_raw,
                internal_symbol,
                &spec.product_type,
                interval,
                start_ms,
                end_ms,
                &spec.rest_url,
            )
            .await
        } else {
            let interval = network_adapters::adapters::hyperliquid_rest::timeframe_secs_to_interval(
                duration_secs,
            );
            network_adapters::adapters::hyperliquid_rest::fetch_historical_candles(
                &spec.exchange_raw,
                internal_symbol,
                interval,
                start_ms,
                end_ms,
                &spec.rest_url,
            )
            .await
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(REFETCH_TIMEOUT_SECS), fut).await {
        Ok(Ok(candles)) => candles,
        Ok(Err(e)) => {
            eprintln!(
                "⚠️  DIE L3: REST refetch failed for {}: {}",
                internal_symbol, e
            );
            Vec::new()
        }
        Err(_) => {
            eprintln!(
                "⚠️  DIE L3: REST refetch timed out after {}s for {}",
                REFETCH_TIMEOUT_SECS, internal_symbol
            );
            Vec::new()
        }
    }
}

/// Build the per-indicator operational lifecycle map for a freshly-built
/// `MarketSnapshot`. Per [03-02-15](../docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)
/// ILS-01 … ILS-15.
///
/// `bar_count` is the number of completed candles accumulated for this
/// timeframe.  An indicator transitions from `Loading → Live` only when the
/// buffer has enough bars for the calculator to produce a meaningful reading
/// AND the calculator emitted a value this bar.
///
/// Rules:
///   - Present in `indicators` AND `bar_count >= bars_required` AND the entry
///     is a real reading (not the `WARMING` placeholder the normalizer inserts
///     for every registered key when its source data is not yet available)
///     → `Live`.
///   - Otherwise → `Loading` with real `bars_seen`. This includes the case
///     where the bar buffer is full but the calculator's strict gate (e.g.
///     volume profile's `window_size / 2` gate, well above `bars_required`)
///     has not yet fired — the lifecycle stays `Loading` until a real reading
///     appears, so the frontend never shows a `Live` dot with `UNKNOWN`
///     state.
///
/// `bars_seen = bar_count` (capped at `bars_required` for the numerator,
///     but the raw count is reported so the frontend can show real progress).
///
/// `is_shadow` distinguishes a completed-candle snapshot from a live
/// shadow-tick snapshot. Close-only indicators (`updates_on_shadow: false`)
/// are intentionally absent from the shadow-tick indicators map — the
/// normalizer skips them in its WARMING fill so the frontend's per-key
/// merge preserves the last completed-candle values without overwriting
/// them with a zero-valued placeholder (see the comment block at
/// `crates/market-analyzer/src/indicators/normalized/all.rs:1726-1734`).
/// Without the shadow branch below, those close-only indicators would
/// perpetually report `Loading (N/N)` on every shadow tick even after
/// the calculator reached its warm-up gate — leaving the dashboard
/// stuck on `WARMING (50/1)` for Hull MA, Ichimoku, AVWAP, PSAR, and the
/// other 23 close-only indicators.
pub fn build_indicator_lifecycle_map(
    indicators: &std::collections::HashMap<String, NormalizedIndicatorValue>,
    prev: &IndicatorLifecycleMap,
    stale_threshold_secs: u32,
    bar_count: u32,
    real_bar_count: u32,
    is_shadow: bool,
    now_ms: u64,
    pipeline_is_live: bool,
) -> IndicatorLifecycleMap {
    use core_domain::indicator_dtos::FeedState;
    use core_domain::indicator_dtos::IndicatorLifecycleStatus;
    let mut map = IndicatorLifecycleMap::new();
    for meta in crate::indicators::registry::INDICATORS {
        let entry = indicators.get(meta.key);
        let present = entry.is_some();
        let bars_required = meta.bars_required;
        let bars_seen = bar_count;
        let prev_status = prev.get(meta.key);
        let prior_last = prev_status.and_then(|p| p.last_updated_at);
        let state_label = entry.map(|e| e.state_label.as_str()).unwrap_or("");
        let is_real_reading = present && state_label != "WARMING";
        let silent = entry.map(|e| e.is_silent()).unwrap_or(false);
        let is_close_only_on_shadow_live =
            is_shadow && !meta.updates_on_shadow && !present && bars_seen >= bars_required;
        let effective_real = is_real_reading || is_close_only_on_shadow_live;

        // Apply the ILS-01..ILS-10 transition table.
        // AUDIT-H7 (ILS-06(b)): an under-warm entry (`bars_seen <
        // bars_required`) whose last update is older than ONE stale
        // window escalates straight to `Failed` per the spec — the old
        // ordering only ever reached `Stale` from the Loading branch and
        // `Failed` required double-stale, so the documented
        // `LOADING → FAILED` transition never fired.
        let target_state = if effective_real && bars_seen >= bars_required && pipeline_is_live {
            IndicatorLifecycleState::Live
        } else if let Some(last) = prior_last {
            let age_ms = now_ms.saturating_sub(last);
            let stale_ms = (stale_threshold_secs as u64).saturating_mul(1000);
            if (bars_seen < bars_required && age_ms > stale_ms)
                || age_ms > stale_ms.saturating_mul(2)
            {
                // ILS-06(b): single-stale while under-warm → Failed.
                // ILS-09: double-stale escalation → Failed.
                IndicatorLifecycleState::Failed
            } else if age_ms > stale_ms {
                IndicatorLifecycleState::Stale
            } else if bars_seen < bars_required {
                IndicatorLifecycleState::Loading
            } else {
                IndicatorLifecycleState::Live
            }
        } else {
            IndicatorLifecycleState::Loading
        };

        let last_updated_at =
            if matches!(target_state, IndicatorLifecycleState::Live) && effective_real {
                Some(now_ms)
            } else {
                prior_last
            };
        let last_error = if matches!(target_state, IndicatorLifecycleState::Failed) {
            prev_status
                .and_then(|p| p.last_error.clone())
                .or_else(|| Some("stale_threshold exceeded".to_string()))
        } else if matches!(target_state, IndicatorLifecycleState::Live) {
            None
        } else {
            prev_status.and_then(|p| p.last_error.clone())
        };
        let feed_state = if matches!(target_state, IndicatorLifecycleState::Live) {
            // v6.10.21: close-only rows whose value is preserved by the
            // frontend per-key merge across shadow ticks (`effective_real`
            // via `is_close_only_on_shadow_live`) are genuinely current —
            // report `Live` instead of the misleading `WaitingFeed` (which
            // is reserved for rows whose upstream feed truly hasn't
            // delivered, e.g. a WS-derived indicator before its first push).
            if is_real_reading || is_close_only_on_shadow_live {
                if silent {
                    FeedState::Silent
                } else {
                    FeedState::Live
                }
            } else {
                FeedState::WaitingFeed
            }
        } else if matches!(target_state, IndicatorLifecycleState::Stale) {
            FeedState::Stale
        } else {
            FeedState::Live
        };

        map.insert(
            meta.key.to_string(),
            IndicatorLifecycleStatus {
                state: target_state,
                bars_seen,
                // PRI-12 (v6.10.7): real (non-synthetic) completed bars.
                bars_seen_real: Some(real_bar_count),
                bars_required,
                last_updated_at,
                last_error,
                stale_threshold_secs,
                silent: silent && is_real_reading,
                feed_state,
            },
        );
    }
    map
}

/// Compute the pipeline state from buffer-fill state. Used as a default when
/// the `TimeframePipeline` writer hasn't yet flushed its authoritative state
/// into the snapshot.
pub fn derive_pipeline_state(buffer_len: usize, target: usize) -> CandlePipelineState {
    // PRI-05 (v6.10.7): uniform live floor for ALL timeframes. The old
    // ≥1m requirement (`buffer_len >= target`, i.e. 500 bars) left warm
    // handovers below the venue-capped fetch length stuck `Loading` with
    // the matrix payload nulled for hours; the sub-minute floor was a
    // different formula. One formula now governs the pipeline-state
    // badge, the matrix broadcast gate, and the indicator lifecycle.
    if buffer_len >= (target / 10).max(50) {
        CandlePipelineState::Live
    } else {
        CandlePipelineState::Loading
    }
}

/// AUDIT-H7: DCP-05 / ILS-06 / ILS-09 — the documented `LIVE → STALE →
/// FAILED` pipeline transitions. `derive_pipeline_state` alone can only
/// ever produce `Live`/`Loading` (it has no time input), so the wire
/// never carried `Stale`/`Failed` despite the DTO contract documenting
/// them. Staleness is measured from the last COMPLETED candle's start
/// time: age > `stale_threshold_secs` → `Stale`, age > 2× → `Failed`.
/// A quiet ≥60 s market (no heartbeat) surfaces the state on the next
/// event-driven snapshot; sub-minute pipelines stay `Live` because their
/// idle-heartbeat closes keep the last-close age fresh.
pub fn derive_pipeline_state_with_staleness(
    buffer_len: usize,
    target: usize,
    last_completed_ms: Option<u64>,
    now_ms: u64,
    stale_threshold_secs: u32,
) -> CandlePipelineState {
    let base = derive_pipeline_state(buffer_len, target);
    let Some(last) = last_completed_ms else {
        return base;
    };
    let stale_ms = (stale_threshold_secs as u64).saturating_mul(1000);
    let age_ms = now_ms.saturating_sub(last);
    match base {
        CandlePipelineState::Live | CandlePipelineState::Loading => {
            if age_ms > stale_ms.saturating_mul(2) {
                CandlePipelineState::Failed
            } else if age_ms > stale_ms {
                CandlePipelineState::Stale
            } else {
                base
            }
        }
        other => other,
    }
}

/// Stable slot identity. Stamped onto every snapshot emitted by this task
/// so the wire and the frontend always know which slot a snapshot came from,
/// regardless of the user-chosen `timeframe_secs`.
pub async fn run_single(
    mut rx: Receiver<NormalizedEvent>,
    telemetry_tx: tokio::sync::mpsc::Sender<database_storage::TelemetryMsg>,
    broadcast_tx: broadcast::Sender<MarketSnapshot>,
    tf_config: TimeframeConfig,
    fib_config: FibonacciConfig,
    statistics_config: StatisticsConfig,
    divergence_detector: Arc<tokio::sync::Mutex<DivergenceDetector>>,
    history: Arc<RwLock<VecDeque<NormalizedCandle>>>,
    latest_snapshot: Arc<RwLock<Option<MarketSnapshot>>>,
    snapshot_history: Arc<RwLock<VecDeque<MarketSnapshot>>>,
    symbol: String,
    pair_key: String,
    timeframe_secs: u64,
    timeframe_label: &'static str,
    slot: TimeframeSlot,
    cancel: CancellationToken,
    candle_forward: Option<tokio::sync::mpsc::Sender<NormalizedCandle>>,
    warmed: Option<WarmedPipelineState>,
    latest_oi: Arc<RwLock<Option<Decimal>>>,
    latest_funding: Arc<RwLock<Option<Decimal>>>,
    latest_mark_px: Arc<RwLock<Option<Decimal>>>,
    latest_index_px: Arc<RwLock<Option<Decimal>>>,
    // Per-TF rolling OI history `(timestamp_secs, value)` — time-bounded to
    // a 3600 s window (AUDIT-AIU-051). Each run_single owns a per-TF clone
    // of the pair-level deque; warmup pre-seeds it and live WS events
    // mutate it via `read_derivative_snapshot_state`.
    oi_history: Arc<RwLock<VecDeque<(u64, f64)>>>,
    // Per-pair shared rolling funding-rate history (bounded to 8).
    funding_history: Arc<RwLock<VecDeque<f64>>>,
    // Per-timeframe cluster-matrix handle (Phase 2). Each TF pipeline
    // owns its own `Arc<RwLock<...>>`, populated by the per-TF cluster
    // refresh task spawned in `registry/pipelines.rs::spawn_tasks`.
    // Distinct from the cross-instance shared state used by L4/L5, which
    // reads the micro TF's cluster.
    cluster_matrix: Arc<RwLock<Option<LiquidationClusterMatrix>>>,
    liquidity_config: Option<LiquidityConfig>,
    // Heatmap bucketing config (Block B). Optional so callers that
    // don't need it (e.g. legacy tests) can pass `None` and fall
    // back to default 0.1% / 24h bucketing.
    heatmap_config: Option<HeatmapConfig>,
    ob_config: OrderBookConfig,
    // v9: the effective strategy (patch-resolved). The synthesis derives
    // its L4 opportunity params and the shared L6 DecisionParams from it.
    // Owned (cloned once per pipeline spawn) so callers never wrestle
    // with borrow lifetimes across the spawned task.
    strategy: StrategyConfig,
    // Sibling pipelines' latest-snapshot handles (every OTHER fixed-ladder
    // slot, excluding this pipeline's own). Ten-slot generalization of the
    // former a/b/c trio.
    cross_tf_snapshots: Vec<Arc<RwLock<Option<MarketSnapshot>>>>,
    latency_tracker: core_domain::SharedLatencyTracker,
    active_set: crate::active_set::ActiveSet,
    quality_config: Option<QualityConfig>,
    reliability: Arc<ReliabilityTracker>,
    refetch: Option<RestRefetchSpec>,
    quality_scope: Option<network_adapters::connection_quality_tracker::ConnectionQualityTracker>,
    buffer_size: usize,
    // AUDIT-H7: `[candle_buffer] stale_threshold_secs` (CB-04 / DCP-05 /
    // ILS-07). Previously the lifecycle map hardcoded `300` at every call
    // site and the config value was dead — operators tuning it saw no
    // effect on Stale/Failed escalation.
    stale_threshold_secs: u32,
    // v6.10 (Phase 2 / B3): per-TF advisory handle for write-through.
    // Mirrors `latest_snapshot` etc. — the analyzer writes the freshly
    // computed cross-TF advisory here on every completed candle so
    // consumers that read `pipeline.advisory` directly see the latest
    // cross-TF synthesis result.
    advisory: Arc<RwLock<Option<AdvisoryMatrix>>>,
    // v6.10 (Phase 3 / C1 + C3): per-TF indicator-lifecycle state map.
    // Passed in so `build_indicator_lifecycle_map` can apply the transition
    // table using the previous snapshot's state and write the result back.
    indicator_lifecycle: Arc<RwLock<IndicatorLifecycleMap>>,
    // v6.10 (Phase 3 / C3): per-TF pipeline_state handle for write-through.
    pipeline_state_handle: Arc<RwLock<CandlePipelineState>>,
) {
    println!(
        "📊 Analysis Task: Started {} ({}) — {} ({})s candles{}...",
        symbol,
        pair_key,
        slot.display_name(),
        tf_config.candles.duration_seconds,
        if warmed.is_some() {
            " [pre-warmed]"
        } else {
            ""
        }
    );

    let active_indicators = tf_config.indicators.clone();

    let (
        mut ema_fast,
        mut ema_medium,
        mut ema_slow,
        mut ema_long,
        mut rsi_14,
        mut macd,
        mut adx_14,
        mut sqz_mom,
        mut bollinger,
        mut atr_standalone,
        mut bbwp_indicator,
        mut stochastic_indicator,
        mut chandemo_indicator,
        mut supertrend_indicator,
        mut keltner_indicator,
        mut donchian_indicator,
        mut obv_indicator,
        mut cmf_indicator,
        mut mfi_indicator,
        mut hv_indicator,
        mut aroon_indicator,
        mut choppiness_indicator,
        mut linreg_indicator,
        mut zscore_indicator,
        mut stoch_div,
        mut chandemo_div,
        mut mfi_div,
        mut cmf_div,
        mut obv_div,
        mut squeeze_div,
        mut vwap_sum_tp_vol,
        mut vwap_sum_vol,
        mut last_day_index,
        mut volume_history,
        // v6.11: rolling 300-sample window for the L1 price-trend Sharpe
        // ratio. Real completed candles only (PRI-06 — synthetic doji/idle
        // buckets never enter).
        mut close_history,
        mut pivot_points_indicator,
        mut candlestick_indicator,
        mut ichimoku_indicator,
        mut cci_indicator,
        mut psar_indicator,
        mut wr_indicator,
        mut hma_indicator,
        mut ao_indicator,
        mut fi_indicator,
        mut sdc_indicator,
        mut volume_profile_indicator,
        mut smc_indicator,
        mut anchored_vwap_indicator,
    );

    // Number of completed candles processed since pipeline start (resets on
    // cold start; inherits count from warmed history for >=1m TFs).  Single
    // source of truth for `bars_seen` across all candle-based indicators.
    let mut bar_count: u32 = 0;
    // PRI-12 (v6.10.7): real (non-synthetic) completed candles — doji-fill /
    // idle-heartbeat synthetic buckets increment `bar_count` but not this.
    let mut real_bar_count: u32 = 0;

    // Strict chronological handover boundary: the start time of the newest
    // historical (REST/DB) candle used for pre-warming. Live candles at or
    // before this timestamp are discarded so partially-filled live wicks cannot
    // overwrite complete historical data or corrupt stateful indicators.
    // Defaults to 0 (no gate) for cold / sub-minute / non-warmed pipelines.
    let t_last_hist: u64 = warmed
        .as_ref()
        .and_then(|w| w.history.last().map(|c| c.start_time_ms))
        .unwrap_or(0);

    // Support/Resistance role-reversal tracker (flip tolerance 0.3%). Persists
    // across live bars; inherits warmed flip-state from the pre-warm pass.
    let mut sr_tracker = SrRoleTracker::new(0.003);

    // Statistical Intelligence Layer — per-timeframe engine.
    let mut sil_engine = StatisticsEngine::new(statistics_config);
    let mut prev_sil_close: f64 = 0.0;
    let mut mc_counter: u64 = 0;

    if let Some(w) = warmed {
        ema_fast = w.ema_fast;
        ema_medium = w.ema_medium;
        ema_slow = w.ema_slow;
        ema_long = w.ema_long;
        rsi_14 = w.rsi_14;
        macd = w.macd;
        adx_14 = w.adx_14;
        sqz_mom = w.sqz_mom;
        bollinger = w.bollinger;
        atr_standalone = w.atr_standalone;
        bbwp_indicator = w.bbwp_indicator;
        stochastic_indicator = w.stochastic_indicator;
        chandemo_indicator = w.chandemo_indicator;
        supertrend_indicator = w.supertrend_indicator;
        keltner_indicator = w.keltner_indicator;
        donchian_indicator = w.donchian_indicator;
        obv_indicator = w.obv_indicator;
        cmf_indicator = w.cmf_indicator;
        mfi_indicator = w.mfi_indicator;
        hv_indicator = w.hv_indicator;
        aroon_indicator = w.aroon_indicator;
        choppiness_indicator = w.choppiness_indicator;
        linreg_indicator = w.linreg_indicator;
        zscore_indicator = w.zscore_indicator;
        stoch_div = w.stoch_div;
        chandemo_div = w.chandemo_div;
        mfi_div = w.mfi_div;
        cmf_div = w.cmf_div;
        obv_div = w.obv_div;
        squeeze_div = w.squeeze_div;
        vwap_sum_tp_vol = w.vwap_sum_tp_vol;
        vwap_sum_vol = w.vwap_sum_vol;
        last_day_index = w.last_day_index;
        volume_history = w.volume_history;
        close_history = w.close_history;
        sr_tracker = w.sr_tracker;
        pivot_points_indicator = w.pivot_points_indicator;
        candlestick_indicator = w.candlestick_indicator;
        ichimoku_indicator = w.ichimoku_indicator;
        cci_indicator = w.cci_indicator;
        psar_indicator = w.psar_indicator;
        wr_indicator = w.wr_indicator;
        hma_indicator = w.hma_indicator;
        ao_indicator = w.ao_indicator;
        fi_indicator = w.fi_indicator;
        sdc_indicator = w.sdc_indicator;
        volume_profile_indicator = w.volume_profile_indicator;
        smc_indicator = w.smc_indicator;
        anchored_vwap_indicator = w.anchored_vwap_indicator;

        // AUDIT-AIU-117: the warmed `history` deque is seeded by the registry's
        // `populate_buffers` (the single seeder — it runs synchronously before
        // this task starts). The previous unconditional push here DOUBLE-SEEDED
        // every warm candle on every boot (run_single + populate_buffers both
        // wrote into the same `Arc<RwLock<VecDeque>>`), doubling `/api/history`
        // rows and double-counting warm bars in fib/S-R/pivot/pattern and
        // cluster lookbacks. The empty-guard makes this path idempotent under
        // any task ordering. PRI-08 + AUDIT-AIU-117: sub-minute slots (which
        // warm by replaying 60 s REST closes through the state machines) keep
        // `history` EMPTY at handover — the replayed bars are 12× too wide for
        // the slot's structural indicators (fib/pivots/S-R treat 60 s bars as
        // 5 s bars), so only live candles populate the deque.
        if timeframe_secs >= 60 {
            let mut hist = history.write().await;
            if hist.is_empty() {
                for c in &w.history {
                    hist.push_back(c.clone());
                }
            }
        }
        if timeframe_secs >= 60 {
            // PRI-08: only ≥60s slots propagate warmed snapshots as chart
            // history. Sub-minute slots warm state + `history` only (their
            // warm data is real closes replayed at a coarser scale — never
            // present it as sub-minute chart history).
            // Pre-populate latest_snapshot from warmed state
            if let Some(ref snap) = w.latest_snapshot {
                let mut filtered = snap.clone();
                // AUDIT-H2/M2: warm snapshots are seeded all-enabled —
                // reconcile them with the instance's active set so the WS
                // bootstrap replay matches the first live snapshot.
                active_set.filter_snapshot_indicators(&mut filtered);
                *latest_snapshot.write().await = Some(filtered);
            }
            // Pre-populate snapshot_history from warmed state (only ≥60s —
            // PRI-08; idempotent empty-guard for the populate_buffers
            // double-seed, AUDIT-AIU-117).
            {
                let mut snap_hist = snapshot_history.write().await;
                if snap_hist.is_empty() {
                    for snap in &w.snapshot_history {
                        let mut filtered = snap.clone();
                        active_set.filter_snapshot_indicators(&mut filtered);
                        snap_hist.push_back(filtered);
                    }
                }
            }
        }
        // Pre-populate bar_count from warmed history
        bar_count = w.history.len() as u32;
        // PRI-12: warmed candles are real closes (REST/DB) — they count as
        // real bars.
        real_bar_count = w.history.len() as u32;
        // Pre-populate divergence detector state
        {
            let mut det = divergence_detector.lock().await;
            *det = w.divergence_detector.clone();
        }
    } else {
        ema_fast = Ema::new(active_indicators.ema_fast);
        ema_medium = Ema::new(active_indicators.ema_medium);
        ema_slow = Ema::new(active_indicators.ema_slow);
        ema_long = Ema::new(active_indicators.ema_long);
        rsi_14 = Rsi::new(active_indicators.rsi_period);
        macd = Macd::new();
        adx_14 = Adx::new(active_indicators.adx_period);
        adx_14.set_thresholds(
            Decimal::from(active_indicators.adx_trend_threshold),
            Decimal::from(active_indicators.adx_exhaustion_threshold),
            active_indicators.adx_slope_lookback,
        );
        sqz_mom = SqueezeMomentum::new(active_indicators.squeeze_period);
        sqz_mom.set_min_duration(active_indicators.squeeze_min_duration);
        bollinger = BollingerBands::new(20);
        atr_standalone = Atr::new(active_indicators.atr_period);
        bbwp_indicator = Bbwp::new(
            active_indicators.bbwp_lookback,
            active_indicators.bbwp_period,
        );
        stochastic_indicator = Stochastic::new(
            active_indicators.stoch_k_period,
            active_indicators.stoch_d_period,
            active_indicators.stoch_s_period,
        );
        chandemo_indicator = ChandeMO::new(active_indicators.chandemo_period);
        supertrend_indicator = Supertrend::new(
            active_indicators.supertrend_period,
            active_indicators.supertrend_multiplier,
        );
        keltner_indicator = Keltner::new(
            active_indicators.keltner_ema_period,
            active_indicators.keltner_atr_period,
            active_indicators.keltner_multiplier,
        );
        donchian_indicator = Donchian::new(active_indicators.donchian_period);
        obv_indicator = Obv::new(active_indicators.obv_smoothing);
        cmf_indicator = Cmf::new(active_indicators.cmf_period);
        mfi_indicator = Mfi::new(active_indicators.mfi_period);
        hv_indicator = HistoricalVolatility::new(active_indicators.hv_period);
        aroon_indicator = Aroon::new(active_indicators.aroon_period);
        choppiness_indicator = Choppiness::new(active_indicators.chop_period);
        linreg_indicator = LinRegSlope::new(active_indicators.linreg_period);
        zscore_indicator = ZScore::new(active_indicators.zscore_period);
        stoch_div = SeriesDivergence::new(20);
        chandemo_div = SeriesDivergence::new(20);
        mfi_div = SeriesDivergence::new(20);
        cmf_div = SeriesDivergence::new(20);
        obv_div = SeriesDivergence::new(20);
        squeeze_div = SeriesDivergence::new(20);
        vwap_sum_tp_vol = Decimal::ZERO;
        vwap_sum_vol = Decimal::ZERO;
        last_day_index = None;
        // AUDIT-AIU-070: honor the configured `volume_average_period`.
        volume_history = VecDeque::with_capacity(active_indicators.volume_average_period.max(1));
        close_history = VecDeque::with_capacity(crate::indicators::ratio::SHARPE_WINDOW);
        pivot_points_indicator = PivotPoints::new(PivotMethod::from_str_lenient(
            &active_indicators.pivot_points_method,
        )); // AUDIT-AIU-072
        candlestick_indicator = Candlestick::new(CandlestickConfig::default());
        ichimoku_indicator = Ichimoku::new(
            active_indicators.ichimoku_tenkan,
            active_indicators.ichimoku_kijun,
            active_indicators.ichimoku_senkou_b,
            active_indicators.ichimoku_displacement,
        );
        cci_indicator = Cci::new(active_indicators.cci_period);
        psar_indicator = ParabolicSar::new(
            active_indicators.psar_af_step,
            active_indicators.psar_af_max,
        );
        wr_indicator = WilliamsR::new(active_indicators.williams_r_period);
        hma_indicator = HullMA::new(active_indicators.hull_ma_period);
        ao_indicator = AwesomeOscillator::new();
        fi_indicator = ForceIndex::new(active_indicators.force_index_smoothing);
        sdc_indicator = StdDevChannel::new(active_indicators.stddev_channel_period);
        volume_profile_indicator = VolumeProfile::new(
            active_indicators.volume_profile_window,
            active_indicators.volume_profile_bins,
            active_indicators.volume_profile_value_area,
        );
        smc_indicator = SmartMoney::new(active_indicators.smc_lookback);
        anchored_vwap_indicator = AnchoredVwap::new();
    }

    // ADX slope history for the 2-bar consecutive-deceleration hook exit.
    let mut adx_slope_history: VecDeque<Decimal> = VecDeque::with_capacity(3);

    // Signal-age tracker: maps "<indicator>:<kind>" → (first-seen bar, direction).
    // Stamps `age_bars` on each completed snapshot's signals. Live-only (resets
    // on warm handover, which is acceptable — historical bars aren't decisions).
    let mut signal_age_tracker: std::collections::HashMap<
        String,
        (u32, crate::indicators::SignalDirection),
    > = std::collections::HashMap::new();
    let mut live_bar: u32 = 0;
    let mut prev_bar_state = PreviousBarState::default();
    let mut last_pivot_count: usize = 0;
    let mut last_cascade_state: core_domain::liquidity::CascadeState =
        core_domain::liquidity::CascadeState::None;
    let mut prev_mtf_score: Option<f64> = None;
    let mut prev_regime: Option<core_domain::analysis::MarketRegime> = None;
    let mut prev_volume_dim: Option<f64> = None;
    let mut prev_bias: Option<core_domain::analysis::MarketBias> = None;

    // OI delta tracking: rolling 3600 s time window of OI values
    // `(timestamp_secs, value)` (AUDIT-AIU-051). Sourced from the per-TF
    // `oi_history` clone so the bootstrap warmup can pre-seed it with
    // historical samples. The shared lock is read in
    // `read_derivative_snapshot_state` (which also appends the live sample
    // with its timestamp) and replaced by the warmup seeding path during
    // `populate_buffers`.
    let oi_history: Arc<RwLock<VecDeque<(u64, f64)>>> = oi_history;
    let funding_history: Arc<RwLock<VecDeque<f64>>> = funding_history;

    // v9: the strategy's `l1_5` section drives the liquidity pipeline
    // (legacy `[workspace.liquidity]`/`[heatmap]` are the parse fallback).
    let eff_liq = crate::liquidity_params::effective_liquidity(
        liquidity_config.as_ref(),
        heatmap_config.as_ref(),
        &strategy.l1_5,
    );

    // Phase 1: real liquidation event accumulator. Per-candle aggregation
    // produces a `LiquidityFlow` on every completed bar.
    //
    // Constructor pulls cascade-detection knobs from the optional
    // `liquidity_config` so operator overrides in `[workspace.liquidity]`
    // (cascade_detected_zscore, cascade_sustained_events) actually take
    // effect — the legacy `LiquidityEventAccumulator::new` defaults were
    // hardcoded `2.5 / 5 / 3`.
    //
    // Block B (heatmap bucketing): the heatmap knobs come from the
    // optional `heatmap_config` (defaults to 0.1% / 24h). When the
    // heatmap is disabled in config, bucketing is a no-op but the
    // accumulator keeps the existing per-bar aggregation.
    let acc_tuning = core_domain::liquidity::AccumulatorTuning {
        baseline_no_history_usd: eff_liq.accumulator.baseline_no_history_usd,
        intensity_log_scale: eff_liq.accumulator.intensity_log_scale,
        fallback_baseline_usd: eff_liq.accumulator.fallback_baseline_usd,
        exhausted_intensity: eff_liq.accumulator.exhausted_intensity,
    };
    let mut liquidity_acc = match (Some(&eff_liq.cfg), eff_liq.heatmap.as_ref()) {
        (Some(cfg), Some(hc)) if hc.enabled => {
            core_domain::liquidity::LiquidityEventAccumulator::with_full_config(
                &symbol,
                eff_liq.accumulator.max_buffered_events.max(1),
                cfg.cascade_detected_zscore,
                eff_liq.accumulator.cascade_window_candles.max(1),
                cfg.cascade_sustained_events,
                hc.bucket_size_pct,
                hc.retention_secs,
            )
            .with_tuning(acc_tuning.clone())
        }
        (Some(cfg), _) => core_domain::liquidity::LiquidityEventAccumulator::with_config(
            &symbol,
            eff_liq.accumulator.max_buffered_events.max(1),
            cfg.cascade_detected_zscore,
            eff_liq.accumulator.cascade_window_candles.max(1),
            cfg.cascade_sustained_events,
        )
        .with_tuning(acc_tuning.clone()),
        (None, Some(hc)) if hc.enabled => {
            core_domain::liquidity::LiquidityEventAccumulator::with_full_config(
                &symbol,
                eff_liq.accumulator.max_buffered_events.max(1),
                2.5,
                eff_liq.accumulator.cascade_window_candles.max(1),
                3,
                hc.bucket_size_pct,
                hc.retention_secs,
            )
            .with_tuning(acc_tuning.clone())
        }
        (None, _) => {
            core_domain::liquidity::LiquidityEventAccumulator::new(&symbol).with_tuning(acc_tuning)
        }
    };

    let mut candle_gen = CandleGenerator::new(
        &symbol,
        tf_config.candles.duration_seconds,
        Exchange::Hyperliquid,
    );

    let mut median_filter = quality_config.as_ref().map(MedianPriceFilter::new);

    let staleness_threshold_ms = quality_config
        .as_ref()
        .map(|q| q.staleness_threshold_secs * 1000)
        .unwrap_or(600_000);

    #[allow(unused_assignments)]
    let mut last_trade_ts_ms: u64 = 0;

    // DIE L3 runtime sequence audit + gap-fill state (03-01-04 §3 / §2.1.2).
    let duration_ms = tf_config.candles.duration_seconds * 1000;
    // AUDIT-AIU-117: for sub-minute slots the warm base is a 60 s REST
    // replay — the 60 s gap between the last replayed close and the first
    // live close must NOT be gap-filled with 11-59 synthesized EMA dojis at
    // boot (one broadcast burst + a burst of persisted SYNTHETIC rows).
    // The handover resets the sequence: no `t_last_hist` anchor for
    // sub-minute, so the first live candle closes cleanly.
    let mut last_completed_start_ms: Option<u64> =
        (t_last_hist > 0 && tf_config.candles.duration_seconds >= 60).then_some(t_last_hist);
    let mut outliers_at_prev_candle: u32 = 0;
    let reconstructor = network_adapters::adapters::reconstruction::CandleReconstructor::new();
    // Cap the number of bars filled per detected hole. K3 (production
    // audit): the old cap of 60 left every outage longer than an hour as
    // a PERMANENT chart + DB hole (the API's own gap-fill had the same
    // 60-bar ceiling). Raised to 120 — ≈2 h at 1 m TF / ≈10 h at 5 m /
    // ≈30 h at 15 m. The cap must stay BELOW the per-slot broadcast
    // channel capacity (200) with headroom: a burst larger than the
    // channel lags every WS subscriber past the freshly recovered bars
    // (the first frames are overwritten), which would re-open the gap on
    // the charts. Larger recoveries belong to the bootstrap path.
    const MAX_GAP_FILL_BARS: u64 = 120;

    let mut order_book_analysis =
        OrderBookAnalysis::new(ob_config.depth_levels, ob_config.wall_threshold);
    let spread_wide_threshold_pct = ob_config.spread_wide_threshold_pct;

    let mut shadow_bid = Decimal::ZERO;
    let mut shadow_ask = Decimal::ZERO;
    #[allow(unused_assignments)]
    let mut shadow_exchange: Option<Exchange> = None;
    let mut shadow_prev_day_px: Option<Decimal> = None;
    // AUDIT-V8-002 (stale-mid guard): wall-clock timestamp of the last
    // order-book event. The force-close / doji-fill paths only use the
    // bid/ask mid as the close price while the book is fresh; otherwise
    // they fall back to the last trade close so indicators never react
    // to a phantom stale-mid price.
    let mut last_ob_ms: u64 = 0;
    // AUDIT-V8-003 (idle-bucket heartbeat): last known close price. When
    // the market goes quiet (no current candle, no events), the stale
    // check synthesizes one doji per elapsed empty bucket at this price
    // so the chart + indicator lines stay continuous per wall-clock
    // second instead of leaving gaps.
    let mut last_close: Decimal = Decimal::ZERO;

    let stale_check_interval_ms: u64 = (timeframe_secs * 1000 / 2).max(500);
    let grace_period_ms: u64 = duration_ms;
    let mut stale_check =
        tokio::time::interval(std::time::Duration::from_millis(stale_check_interval_ms));
    stale_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Shadow-broadcast throttle. The flickering "live candle" broadcast is
    // emitted on every trade tick and order-book event — at 1-s timeframes
    // that's 50+ Hz per pipeline slot × 4 slots = 200+ broadcasts/sec on the
    // frontend, which saturates the broadcast channel (cap 200) and freezes
    // the dashboard. PRI-11 (v6.10.7): the throttle scales with the chosen
    // timeframe — at most one shadow per quarter-candle (4 Hz at 1 s, 1.33 Hz
    // at 3 s, 1 Hz at 5 s+, 15 s at 60 s) instead of the old `clamp(100, 250)`
    // that pinned every sub-minute TF at 4 Hz. The candle-close path (one
    // fire per natural candle close) is unaffected and carries the
    // authoritative snapshot for the bar.
    let shadow_throttle_ms: u64 = ((timeframe_secs * 1000) / 4).max(100);
    let mut last_shadow_broadcast_ms: u64 = 0;

    // Gated on buffer fill: when bar_count reaches buffer_size, completed
    // snapshots start broadcasting to the frontend (with full synthesis and
    // decision context).  Shadow broadcasts skip close-only indicators
    // (registry `updates_on_shadow = false`) so the frontend per-key merge
    // can preserve the last completed-candle value across live ticks —
    // see `NormalizationEngine::normalize_all(..., shadow = true)` and the
    // WARMING fill block in `indicators/normalized/all.rs`.
    // (Reserved hook: a future `shadow → completed` broadcast gate will
    //  re-introduce a `pipeline_is_live: bool` here and gate the
    //  per-tick vs per-candle broadcast on it.)

    enum LoopAction {
        Process(NormalizedEvent),
        StaleCheck,
        Shutdown,
    }

    loop {
        let action = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Analysis Task: {} ({}) cancelled, shutting down.", symbol, timeframe_label);
                LoopAction::Shutdown
            }
            _ = stale_check.tick() => {
                LoopAction::StaleCheck
            }
            result = rx.recv() => {
                match result {
                    Some(e) => LoopAction::Process(e),
                    None => {
                        println!("🛑 Analysis Task: {} ({}) channel closed.", symbol, timeframe_label);
                        LoopAction::Shutdown
                    }
                }
            }
        };

        let event = match action {
            LoopAction::Shutdown => break,
            LoopAction::StaleCheck => {
                let now_ms = core_domain::LatencyTracker::now_ms();
                // Close the current candle if:
                //   a) No trade arrived within the grace-period window (existing
                //      behavior — catches genuine feed stalls), OR
                //   b) The wall clock has advanced past this candle's natural
                //      interval boundary (new: ensures sub-minute TFs close on
                //      a strict cadence even when trades keep flowing within
                //      the same bucket).
                let needs_close = candle_gen.is_stale(now_ms, grace_period_ms)
                    // Clock-driven boundary close: for sub-minute TFs the
                    // stale check fires on a sub-second cadence (500 ms for
                    // 1 s, 1500 ms for 3 s, 2500 ms for 5 s, 7500 ms for
                    // 15 s) and can catch the interval boundary within one
                    // tick.  For >=60 s TFs the trade-tick boundary
                    // detection is already accurate and the stale-check
                    // cadence (30 s for 60 s, 90 s for 180 s) is too coarse
                    // to pin down the boundary precisely.
                    || (timeframe_secs < 60 && candle_gen.is_past_interval(now_ms));

                if needs_close {
                    if let Some(forced) = candle_gen.force_close() {
                        // AUDIT-V8-002: only use the order-book mid while the
                        // book is fresh (≤ grace period). A stale mid (quiet
                        // book, one-way flow) would otherwise become the
                        // candle close — and, through the doji heartbeat
                        // below, up to 60 synthetic closes at a phantom price,
                        // dragging EMA/RSI into the "reacting to price that
                        // isn't there" distortion. Fall back to the last
                        // trade close instead.
                        let ob_fresh =
                            last_ob_ms > 0 && now_ms.saturating_sub(last_ob_ms) <= grace_period_ms;
                        let mid =
                            if ob_fresh && shadow_bid > Decimal::ZERO && shadow_ask > Decimal::ZERO
                            {
                                (shadow_bid + shadow_ask) / Decimal::from(2)
                            } else {
                                forced.close
                            };
                        // AUDIT-H8: the fresh book mid can sit OUTSIDE the
                        // candle's [low, high] (one-way book move past the
                        // last trade). The previous code stamped it as the
                        // close unclamped — emitting an INVALID OHLC (close
                        // outside range) to the indicators + broadcast, and
                        // feeding `history` a different close than the
                        // indicators saw. Widen the range to contain the
                        // mid: the book genuinely moved there, so the bar's
                        // high/low should reflect it — this keeps the close
                        // valid, `assert_validity()` meaningful, and the
                        // indicator state aligned with the broadcast candle.
                        let live_high = forced.high.max(mid);
                        let live_low = forced.low.min(mid);
                        // AUDIT-V8-003: last known close for the idle-bucket
                        // heartbeat (and the doji fill below).
                        last_close = mid;
                        let live = core_domain::normalized::NormalizedCandle {
                            exchange: forced.exchange,
                            symbol: forced.symbol.clone(),
                            start_time_ms: forced.start_time_ms,
                            duration_ms: forced.duration_ms,
                            open: forced.open,
                            high: live_high,
                            low: live_low,
                            close: mid,
                            volume: forced.volume,
                            trades_count: forced.trades_count,
                            reconstructed: forced.reconstructed,
                        };

                        // ── Force-close: advance every stateful indicator
                        // on the FORCED candle. Without this, the EMA/RSI/
                        // MACD/etc. state lags the wall clock by every
                        // second the sub-minute pipeline spent without a
                        // trade crossing a boundary. (See the regression
                        // pins in `tests/sub_minute_indicator_cadence.rs`.)
                        let candle_close_sec = forced.start_time_ms / 1000;
                        let day_index = candle_close_sec / 86400;
                        if let Some(prev_day) = last_day_index {
                            if day_index > prev_day {
                                vwap_sum_tp_vol = Decimal::ZERO;
                                vwap_sum_vol = Decimal::ZERO;
                            }
                        }
                        last_day_index = Some(day_index);
                        let force_close_readings = apply_candle_to_indicators(
                            &symbol,
                            slot,
                            timeframe_secs,
                            &live,
                            day_index,
                            &mut pivot_points_indicator,
                            &mut candlestick_indicator,
                            &mut ichimoku_indicator,
                            &mut cci_indicator,
                            &mut psar_indicator,
                            &mut wr_indicator,
                            &mut hma_indicator,
                            &mut ao_indicator,
                            &mut fi_indicator,
                            &mut sdc_indicator,
                            &mut volume_profile_indicator,
                            &mut smc_indicator,
                            &mut anchored_vwap_indicator,
                            &mut ema_fast,
                            &mut ema_medium,
                            &mut ema_slow,
                            &mut ema_long,
                            &mut rsi_14,
                            &mut macd,
                            &mut adx_14,
                            &mut sqz_mom,
                            &mut bollinger,
                            &mut atr_standalone,
                            &mut bbwp_indicator,
                            &mut stochastic_indicator,
                            &mut chandemo_indicator,
                            &mut supertrend_indicator,
                            &mut keltner_indicator,
                            &mut donchian_indicator,
                            &mut obv_indicator,
                            &mut cmf_indicator,
                            &mut mfi_indicator,
                            &mut hv_indicator,
                            &mut aroon_indicator,
                            &mut choppiness_indicator,
                            &mut linreg_indicator,
                            &mut zscore_indicator,
                            &mut vwap_sum_tp_vol,
                            &mut vwap_sum_vol,
                        );

                        // ── Broadcast the completed force-close snapshot via
                        // the shared matrix synthesis. The function builds the
                        // indicators map from the JUST-computed readings (no
                        // double-application), computes the alignment/analysis/
                        // risk/advisory/opportunity/decision_context matrices
                        // (previously only the trade-triggered path did),
                        // advances the divergence / S/R / SIL / signal-age
                        // trackers, and broadcasts the completed frame with the
                        // matrix payload — so sub-minute TFs populate the
                        // Metrics / Alignment / Risks / Analysis /
                        // Opportunities / Recommendation tabs, not just the
                        // chart.
                        // PRI-03 (v6.10.7): the force-close quality envelope is
                        // computed from the same inputs as the trade-triggered
                        // path (median-filter outlier counts, staleness vs
                        // `last_trade_ts_ms`, reconstruction provenance) — no
                        // more fabricated `quality_score = 100` on the
                        // clock-driven completion path.
                        let rejected_total = median_filter
                            .as_ref()
                            .map(|f| f.outliers_rejected())
                            .unwrap_or(0);
                        let had_outliers_this_candle = rejected_total > outliers_at_prev_candle;
                        outliers_at_prev_candle = rejected_total;
                        let candle_close_ms = forced.start_time_ms + duration_ms;
                        let candle_stale = last_trade_ts_ms > 0
                            && candle_close_ms.saturating_sub(last_trade_ts_ms)
                                > staleness_threshold_ms;
                        let quality_score = if live.assert_validity().is_err() {
                            0.0
                        } else {
                            let mut score = 100.0_f64;
                            if forced.reconstructed.is_some() {
                                score -= 20.0;
                            }
                            if had_outliers_this_candle {
                                score -= 10.0;
                            }
                            if candle_stale {
                                score -= 30.0;
                            }
                            score.clamp(0.0, 100.0)
                        };
                        // 02-03 §2: `is_valid` mirrors the validity gate
                        // (score 0 when `assert_validity` failed) — the
                        // previous hardcoded `true` contradicted the doc.
                        // AUDIT-H8: validate the ACTUAL broadcast candle
                        // (`live`, mid-clamped) — the old code validated
                        // the pre-mid `forced` candle, so a mid-adjusted
                        // close outside [low, high] passed as "valid".
                        let envelope_valid = live.assert_validity().is_ok();
                        let quality_envelope = CandleQualityEnvelope {
                            quality_score,
                            is_valid: envelope_valid,
                            is_gap_filled: forced.reconstructed.is_some(),
                            had_outliers_rejected: had_outliers_this_candle,
                            spike_detected: had_outliers_this_candle,
                            is_stale: candle_stale,
                            sequence_integrity: classify_sequence(
                                last_completed_start_ms,
                                forced.start_time_ms,
                            ),
                            gap_since_last: match last_completed_start_ms {
                                Some(prev) => forced.start_time_ms.saturating_sub(prev) / 1000,
                                None => duration_ms / 1000,
                            },
                            validated_at: now_ms,
                        };
                        let completed_snapshot = synthesize_completed_candle(
                            &symbol,
                            timeframe_label,
                            slot,
                            timeframe_secs,
                            &live,
                            candle_close_sec,
                            force_close_readings,
                            &mut bar_count,
                            &mut real_bar_count,
                            &quality_envelope,
                            shadow_exchange,
                            shadow_bid,
                            shadow_ask,
                            shadow_prev_day_px,
                            last_ob_ms,
                            grace_period_ms,
                            &active_indicators,
                            &active_set,
                            buffer_size,
                            stale_threshold_secs,
                            last_completed_start_ms,
                            now_ms,
                            &fib_config,
                            &liquidity_config,
                            spread_wide_threshold_pct,
                            &history,
                            &mut volume_history,
                            &mut close_history,
                            &mut adx_slope_history,
                            &divergence_detector,
                            &mut sr_tracker,
                            &mut last_pivot_count,
                            &mut anchored_vwap_indicator,
                            &mut stoch_div,
                            &mut chandemo_div,
                            &mut mfi_div,
                            &mut cmf_div,
                            &mut obv_div,
                            &mut squeeze_div,
                            &mut signal_age_tracker,
                            &mut live_bar,
                            &mut prev_bar_state,
                            &mut sil_engine,
                            &mut prev_sil_close,
                            &mut mc_counter,
                            &mut prev_mtf_score,
                            &mut prev_regime,
                            &mut prev_volume_dim,
                            &mut prev_bias,
                            &mut last_cascade_state,
                            &mut liquidity_acc,
                            &latest_oi,
                            &latest_funding,
                            &latest_mark_px,
                            &latest_index_px,
                            &oi_history,
                            &funding_history,
                            &order_book_analysis,
                            &cross_tf_snapshots,
                            &cluster_matrix,
                            &indicator_lifecycle,
                            &pipeline_state_handle,
                            &latest_snapshot,
                            &advisory,
                            &broadcast_tx,
                            &telemetry_tx,
                            &latency_tracker,
                            &strategy,
                        )
                        .await;
                        // AUDIT-V8-004: force-closed candles are real OHLCV —
                        // keep them in the in-memory snapshot history so
                        // `/api/history` serves the sub-minute chart
                        // continuously (they are real candles, so the
                        // wire carries no `reconstructed` flag).
                        {
                            let mut snap_hist = snapshot_history.write().await;
                            snap_hist.push_back(completed_snapshot);
                            while snap_hist.len() > HIST_BUFFER_MAX {
                                snap_hist.pop_front();
                            }
                        }
                        // PRI-06 (v6.10.7): real force-closed candles also
                        // feed the `history` buffer — the input for
                        // fib/pivots/S-R/patterns and the liquidation
                        // cluster matrix (`pipe.history`, 200-candle
                        // lookback). Previously only trade-triggered
                        // candles entered `history`, so on force-close-
                        // dominated sub-minute markets these signals were
                        // computed from a sparse history (and the cluster
                        // matrix errored `InsufficientHistory` on quiet
                        // markets). Synthetic doji/idle buckets never enter
                        // `history` — only `snapshot_history`.
                        {
                            let mut hist = history.write().await;
                            // AUDIT-H8: push the mid-adjusted `live` candle
                            // (the same close the indicators saw) — the old
                            // `forced.clone()` fed fib/pivots/S-R a different
                            // close than the indicator state.
                            hist.push_back(live.clone());
                            while hist.len() > HIST_BUFFER_MAX {
                                hist.pop_front();
                            }
                        }
                        last_completed_start_ms = Some(forced.start_time_ms);

                        // ── Doji heartbeat: fill any intervals between this
                        // candle's end and `now_ms` with flat doji candles
                        // (O=H=L=C=close from the just-closed bar). Each
                        // reconstructed doji is fed through the indicator
                        // pipeline so EMA/RSI/etc. advance once per wall-
                        // clock second even when no trade fired. The
                        // broadcast is capped at MAX_GAP_FILL_BARS so a
                        // single stale tick can never flood the channel;
                        // subsequent ticks pick up the next batch.
                        let mut fill_cursor = forced.start_time_ms + duration_ms;
                        let max_fill = now_ms.saturating_sub(fill_cursor) / duration_ms;
                        let fill_n = max_fill.min(MAX_GAP_FILL_BARS);
                        for _ in 0..fill_n {
                            // v9 `l1.ignore_reconstructed_candles`: the
                            // strategy refuses to feed DIE-synthesized gap
                            // candles into the indicator state machines.
                            // Indicators freeze on the last real reading;
                            // no synthetic snapshot is broadcast/persisted.
                            if strategy.l1.ignore_reconstructed_candles {
                                continue;
                            }
                            let doji = core_domain::normalized::NormalizedCandle {
                                exchange: forced.exchange,
                                symbol: forced.symbol.clone(),
                                start_time_ms: fill_cursor,
                                duration_ms,
                                open: mid,
                                high: mid,
                                low: mid,
                                close: mid,
                                volume: Decimal::ZERO,
                                trades_count: 0,
                                reconstructed: Some(
                                    core_domain::normalized::ReconstructionMethod::Synthetic,
                                ),
                            };

                            // Advance every stateful indicator for this
                            // reconstructed doji bucket. Without this, the
                            // indicators stay frozen on the previous
                            // bucket's reading while the wall clock moves
                            // forward — the symptom the user reported on
                            // Bitget 1s / 3s / 5s / 15s charts.
                            let doji_readings = apply_candle_to_indicators(
                                &symbol,
                                slot,
                                timeframe_secs,
                                &doji,
                                day_index,
                                &mut pivot_points_indicator,
                                &mut candlestick_indicator,
                                &mut ichimoku_indicator,
                                &mut cci_indicator,
                                &mut psar_indicator,
                                &mut wr_indicator,
                                &mut hma_indicator,
                                &mut ao_indicator,
                                &mut fi_indicator,
                                &mut sdc_indicator,
                                &mut volume_profile_indicator,
                                &mut smc_indicator,
                                &mut anchored_vwap_indicator,
                                &mut ema_fast,
                                &mut ema_medium,
                                &mut ema_slow,
                                &mut ema_long,
                                &mut rsi_14,
                                &mut macd,
                                &mut adx_14,
                                &mut sqz_mom,
                                &mut bollinger,
                                &mut atr_standalone,
                                &mut bbwp_indicator,
                                &mut stochastic_indicator,
                                &mut chandemo_indicator,
                                &mut supertrend_indicator,
                                &mut keltner_indicator,
                                &mut donchian_indicator,
                                &mut obv_indicator,
                                &mut cmf_indicator,
                                &mut mfi_indicator,
                                &mut hv_indicator,
                                &mut aroon_indicator,
                                &mut choppiness_indicator,
                                &mut linreg_indicator,
                                &mut zscore_indicator,
                                &mut vwap_sum_tp_vol,
                                &mut vwap_sum_vol,
                            );

                            // Broadcast the doji as a completed snapshot
                            // with a fully-populated indicators map (built
                            // from the just-computed readings) so the
                            // chart's overlay lines (EMA/RSI/etc.) keep
                            // advancing across sub-minute doji-fill seconds.
                            bar_count = bar_count.saturating_add(1);
                            let avg_vol = if !volume_history.is_empty() {
                                let sum: Decimal = volume_history.iter().sum();
                                Some(sum / Decimal::from(volume_history.len()))
                            } else {
                                None
                            };
                            let rvol = match (doji.volume, avg_vol) {
                                (vol, Some(avg)) if avg > Decimal::ZERO => Some(vol / avg),
                                _ => None,
                            };
                            let doji_snap = build_completed_snapshot_from_readings(
                                &doji_readings,
                                &doji,
                                &symbol,
                                &pair_key,
                                slot,
                                timeframe_secs,
                                bar_count,
                                real_bar_count,
                                shadow_exchange,
                                shadow_bid,
                                shadow_ask,
                                (
                                    active_indicators.ema_fast,
                                    active_indicators.ema_medium,
                                    active_indicators.ema_slow,
                                    active_indicators.ema_long,
                                ),
                                &active_set,
                                buffer_size,
                                avg_vol,
                                rvol,
                                stale_threshold_secs,
                                last_completed_start_ms,
                                now_ms,
                                // Synthetic doji: Sharpe window only accepts
                                // real closes (PRI-06), so the ratio is None.
                                None,
                            );
                            let _ = broadcast_tx.send(doji_snap.clone());
                            // K3 (production audit): reconstructed candles
                            // were never persisted (only the in-memory
                            // snapshot history), so a restart or the
                            // `/api/history` DB fallback lost every
                            // gap-filled candle. Persist them now — the
                            // `reconstructed` column keeps the provenance.
                            send_telemetry(
                                &telemetry_tx,
                                database_storage::TelemetryMsg::InsertSnapshot(Box::new(
                                    doji_snap.clone(),
                                )),
                            );
                            // AUDIT-V8-004 (history continuity): synthetic
                            // dojis are pushed to the in-memory snapshot
                            // history AND persisted to SQLite (K3, two blocks
                            // above) so `/api/history` and the chart's
                            // EMA/indicator series stay continuous across
                            // quiet buckets. The `quality_envelope`
                            // `is_gap_filled` flag lets the API mark them
                            // `reconstructed` on the wire so the frontend
                            // keeps them out of its persistent candle cache,
                            // and the bootstrap warm-replay filters them out
                            // of warm state (AUDIT-AIU-118 — persisted
                            // SYNTHETIC rows never seed `history` /
                            // `real_bar_count` on restart).
                            {
                                let mut snap_hist = snapshot_history.write().await;
                                snap_hist.push_back(doji_snap);
                                while snap_hist.len() > HIST_BUFFER_MAX {
                                    snap_hist.pop_front();
                                }
                            }
                            last_completed_start_ms = Some(fill_cursor);
                            fill_cursor += duration_ms;
                        }
                    }
                } else if timeframe_secs < 60 && candle_gen.current_candle.is_none() {
                    // ── AUDIT-V8-003: idle-bucket heartbeat ──
                    // After the current candle closes (force-close or
                    // trade-triggered) the generator has NO current candle
                    // until the next event arrives. Without this branch, a
                    // quiet market leaves every elapsed bucket empty: the
                    // chart shows gaps (frontend flat-Doji bridges), the
                    // EMA lines connect real points with straight segments,
                    // and the "1s candle" visually stays open for seconds.
                    // Synthesize one doji per elapsed empty bucket at the
                    // last known close, advance every stateful indicator,
                    // and broadcast — so candles + indicator lines advance
                    // once per wall-clock bucket even in total silence.
                    if last_close > Decimal::ZERO {
                        if let Some(last_ts) = last_completed_start_ms {
                            let buckets_elapsed =
                                now_ms.saturating_sub(last_ts + duration_ms) / duration_ms;
                            let fill_n = buckets_elapsed.min(MAX_GAP_FILL_BARS);
                            // M3 (production audit): the previous loop
                            // recomputed `idle_start = last_ts + duration_ms`
                            // from the once-bound `last_ts` copy on every
                            // iteration — with `fill_n > 1` every doji shared
                            // one `start_time_ms` (duplicate timestamps in
                            // snapshot_history + the history axis) while
                            // `last_completed_start_ms` advanced only one
                            // bucket. Advance a cursor per iteration.
                            let mut cursor = last_ts + duration_ms;
                            for _ in 0..fill_n {
                                let idle_start = cursor;
                                let idle_close_sec = idle_start / 1000;
                                let day_index = idle_close_sec / 86400;
                                if let Some(prev_day) = last_day_index {
                                    if day_index > prev_day {
                                        vwap_sum_tp_vol = Decimal::ZERO;
                                        vwap_sum_vol = Decimal::ZERO;
                                    }
                                }
                                last_day_index = Some(day_index);
                                let idle_doji = core_domain::normalized::NormalizedCandle {
                                    exchange: shadow_exchange.unwrap_or(Exchange::Hyperliquid),
                                    symbol: symbol.clone(),
                                    start_time_ms: idle_start,
                                    duration_ms,
                                    open: last_close,
                                    high: last_close,
                                    low: last_close,
                                    close: last_close,
                                    volume: Decimal::ZERO,
                                    trades_count: 0,
                                    reconstructed: Some(
                                        core_domain::normalized::ReconstructionMethod::Synthetic,
                                    ),
                                };
                                let idle_readings = apply_candle_to_indicators(
                                    &symbol,
                                    slot,
                                    timeframe_secs,
                                    &idle_doji,
                                    day_index,
                                    &mut pivot_points_indicator,
                                    &mut candlestick_indicator,
                                    &mut ichimoku_indicator,
                                    &mut cci_indicator,
                                    &mut psar_indicator,
                                    &mut wr_indicator,
                                    &mut hma_indicator,
                                    &mut ao_indicator,
                                    &mut fi_indicator,
                                    &mut sdc_indicator,
                                    &mut volume_profile_indicator,
                                    &mut smc_indicator,
                                    &mut anchored_vwap_indicator,
                                    &mut ema_fast,
                                    &mut ema_medium,
                                    &mut ema_slow,
                                    &mut ema_long,
                                    &mut rsi_14,
                                    &mut macd,
                                    &mut adx_14,
                                    &mut sqz_mom,
                                    &mut bollinger,
                                    &mut atr_standalone,
                                    &mut bbwp_indicator,
                                    &mut stochastic_indicator,
                                    &mut chandemo_indicator,
                                    &mut supertrend_indicator,
                                    &mut keltner_indicator,
                                    &mut donchian_indicator,
                                    &mut obv_indicator,
                                    &mut cmf_indicator,
                                    &mut mfi_indicator,
                                    &mut hv_indicator,
                                    &mut aroon_indicator,
                                    &mut choppiness_indicator,
                                    &mut linreg_indicator,
                                    &mut zscore_indicator,
                                    &mut vwap_sum_tp_vol,
                                    &mut vwap_sum_vol,
                                );
                                bar_count = bar_count.saturating_add(1);
                                let avg_vol = if !volume_history.is_empty() {
                                    let sum: Decimal = volume_history.iter().sum();
                                    Some(sum / Decimal::from(volume_history.len()))
                                } else {
                                    None
                                };
                                let rvol = match (idle_doji.volume, avg_vol) {
                                    (vol, Some(avg)) if avg > Decimal::ZERO => Some(vol / avg),
                                    _ => None,
                                };
                                let idle_snap = build_completed_snapshot_from_readings(
                                    &idle_readings,
                                    &idle_doji,
                                    &symbol,
                                    &pair_key,
                                    slot,
                                    timeframe_secs,
                                    bar_count,
                                    real_bar_count,
                                    shadow_exchange,
                                    shadow_bid,
                                    shadow_ask,
                                    (
                                        active_indicators.ema_fast,
                                        active_indicators.ema_medium,
                                        active_indicators.ema_slow,
                                        active_indicators.ema_long,
                                    ),
                                    &active_set,
                                    buffer_size,
                                    avg_vol,
                                    rvol,
                                    stale_threshold_secs,
                                    last_completed_start_ms,
                                    now_ms,
                                    // Synthetic idle heartbeat: no Sharpe
                                    // contribution (real closes only).
                                    None,
                                );
                                let _ = broadcast_tx.send(idle_snap.clone());
                                // K3: persist idle-heartbeat dojis too (see
                                // the doji-fill path — restart continuity).
                                send_telemetry(
                                    &telemetry_tx,
                                    database_storage::TelemetryMsg::InsertSnapshot(Box::new(
                                        idle_snap.clone(),
                                    )),
                                );
                                {
                                    let mut snap_hist = snapshot_history.write().await;
                                    snap_hist.push_back(idle_snap);
                                    while snap_hist.len() > HIST_BUFFER_MAX {
                                        snap_hist.pop_front();
                                    }
                                }
                                cursor += duration_ms;
                                // M3: advance the completion cursor ONLY
                                // per emitted doji — updating it outside
                                // the loop advanced the clock by one bucket
                                // even when fill_n = 0, silently starving
                                // every subsequent heartbeat.
                                last_completed_start_ms = Some(idle_start);
                            }
                        }
                    }
                }
                continue;
            }
            LoopAction::Process(e) => e,
        };

        {
            // Rolling live-runtime cap. The same `HIST_BUFFER_MAX = 1000`
            // is applied to all 4 TF slots and both supported exchanges
            // (Hyperliquid + Bitget) — the pipeline is exchange-agnostic.
            // See `crates/market-analyzer/tests/hist_buffer_cap_uniformity.rs`
            // for the regression pins and `crates/market-analyzer/src/analyzer/warm.rs`
            // for the matching warmup-side trim.
            let mut hist = history.write().await;
            while hist.len() > HIST_BUFFER_MAX {
                hist.pop_front();
            }
        }

        match event {
            NormalizedEvent::Trade(ref trade) => {
                shadow_exchange = Some(trade.exchange);
                candle_gen.set_exchange(trade.exchange);

                latency_tracker
                    .record_ingest_skew(core_domain::LatencyTracker::now_ms(), trade.timestamp_ms);

                if candle_gen.is_late_tick(trade.timestamp_ms) {
                    reliability.increment_out_of_order(1).await;
                    continue;
                }

                last_trade_ts_ms = trade.timestamp_ms;

                let trade_price_f = trade.price.to_f64().unwrap_or(0.0);
                let verdict = if let Some(ref mut filter) = median_filter {
                    filter.evaluate(trade_price_f)
                } else {
                    FilterVerdict::Accepted
                };
                match verdict {
                    FilterVerdict::Rejected => {
                        reliability.increment_outliers(1).await;
                        continue;
                    }
                    FilterVerdict::Bypassed => {
                        reliability.increment_bypassed(1).await;
                        eprintln!(
                            "🔍 DIE L3 [{} {}]: median = 0 (venue reset) — filter bypassed for tick at price {}",
                            symbol, timeframe_label, trade_price_f
                        );
                    }
                    FilterVerdict::Accepted => {}
                }

                let (completed_opt, live_candle) =
                    candle_gen.process_trade_at(trade, core_domain::LatencyTracker::now_ms());
                let mut completed_opt = completed_opt.filter(|c| c.start_time_ms > t_last_hist);
                // AUDIT-H7: one wall-clock sample for this trade arm (used by
                // the gap-fill snapshot's staleness-aware pipeline state).
                let now_ms = core_domain::LatencyTracker::now_ms();

                // v6.11 (sub-minute indicator-cadence fix): a trade-triggered
                // close whose bucket was already completed by the stale-check
                // `force_close` / doji-fill path must not advance the
                // indicators a second time. `last_completed_start_ms` tracks
                // the newest bucket already processed by ANY completion path;
                // late/reordered trades (exchange-batched timestamps) that
                // land on an already-processed bucket are deduped here.
                if let Some(prev_done) = last_completed_start_ms {
                    completed_opt = completed_opt.filter(|c| c.start_time_ms > prev_done);
                }

                // ── DIE L3 §4.2: quarantine + REST refetch on validity failure.
                // An invalid candle never reaches L4; a REST replacement is
                // attempted for its interval, and if none validates the slot
                // stays open and is counted as a gap.
                if let Some(ref candidate) = completed_opt {
                    if let Err(reason) = candidate.assert_validity() {
                        send_telemetry(
                            &telemetry_tx,
                            database_storage::TelemetryMsg::ConsoleLog(format!(
                                "DIE L3: validity check failed for {}/{} candle at {} — quarantined ({})",
                                symbol, timeframe_label, candidate.start_time_ms, reason
                            )),
                        );
                        let mut replacement: Option<NormalizedCandle> = None;
                        if tf_config.candles.duration_seconds >= 60 {
                            if let Some(ref spec) = refetch {
                                let refetched = fetch_interval_candles(
                                    spec,
                                    &symbol,
                                    candidate.start_time_ms,
                                    candidate.start_time_ms + duration_ms,
                                    tf_config.candles.duration_seconds,
                                )
                                .await;
                                replacement = refetched
                                    .into_iter()
                                    .find(|c| {
                                        c.start_time_ms == candidate.start_time_ms
                                            && c.assert_validity().is_ok()
                                    })
                                    .map(|mut c| {
                                        c.reconstructed = Some(
                                            core_domain::normalized::ReconstructionMethod::ExchangeHistorical,
                                        );
                                        c
                                    });
                            }
                        }
                        match replacement {
                            Some(good) => {
                                reliability.increment_reconstructed(1).await;
                                if let Some(ref cq) = quality_scope {
                                    cq.record_reconstructed_candle(good.start_time_ms).await;
                                }
                                completed_opt = Some(good);
                            }
                            None => {
                                reliability.increment_gaps(1).await;
                                completed_opt = None;
                            }
                        }
                    }
                }

                // ── DIE L3 §3: runtime missing-bar sequence audit. A hole
                // between consecutive completed candles flags a gap and
                // triggers recovery: REST for ≥1m tiers, EMA/linear synthesis
                // for sub-minute tiers (03-01-04 §2.1.2). Ticks arriving while
                // recovery runs queue in the pipeline channel, so indicator
                // state is not touched until reconstruction completes.
                if let Some(ref completed) = completed_opt {
                    if let Some(prev_start) = last_completed_start_ms {
                        let expected_start = prev_start + duration_ms;
                        if completed.start_time_ms > expected_start {
                            let missing = (completed.start_time_ms - expected_start) / duration_ms;
                            let fill_n = missing.min(MAX_GAP_FILL_BARS);
                            reliability.increment_gaps(missing as u32).await;
                            eprintln!(
                                "🕳️  DIE L3 [{} {}]: {} missing bar(s) detected before {} — recovering {}",
                                symbol, timeframe_label, missing, completed.start_time_ms, fill_n
                            );

                            let mut filled: Vec<NormalizedCandle> = Vec::new();
                            if tf_config.candles.duration_seconds >= 60 {
                                if let Some(ref spec) = refetch {
                                    let fetched = fetch_interval_candles(
                                        spec,
                                        &symbol,
                                        completed
                                            .start_time_ms
                                            .saturating_sub(fill_n * duration_ms),
                                        completed.start_time_ms,
                                        tf_config.candles.duration_seconds,
                                    )
                                    .await;
                                    filled = fetched
                                        .into_iter()
                                        .filter(|c| {
                                            c.start_time_ms >= expected_start
                                                && c.start_time_ms < completed.start_time_ms
                                                && c.assert_validity().is_ok()
                                        })
                                        .map(|mut c| {
                                            c.reconstructed = Some(
                                                core_domain::normalized::ReconstructionMethod::ExchangeHistorical,
                                            );
                                            c
                                        })
                                        .collect();
                                }
                            } else {
                                let recent_closes: Vec<f64> = {
                                    let hist = history.read().await;
                                    hist.iter().filter_map(|c| c.close.to_f64()).collect()
                                };
                                let fill_start = completed.start_time_ms - fill_n * duration_ms;
                                for i in 0..fill_n {
                                    let s = fill_start + i * duration_ms;
                                    if s < expected_start {
                                        continue;
                                    }
                                    if let Some(rc) = reconstructor.reconstruct(
                                        completed.exchange,
                                        s,
                                        s + duration_ms,
                                        duration_ms,
                                        &recent_closes,
                                    ) {
                                        let mut c = rc.candle;
                                        c.symbol = symbol.clone();
                                        filled.push(c);
                                    }
                                }
                            }

                            for gap_candle in filled {
                                reliability.increment_reconstructed(1).await;
                                if let Some(ref cq) = quality_scope {
                                    cq.record_reconstructed_candle(gap_candle.start_time_ms)
                                        .await;
                                }
                                {
                                    let mut hist = history.write().await;
                                    hist.push_back(gap_candle.clone());
                                }
                                if let Some(ref fwd) = candle_forward {
                                    let _ = fwd.send(gap_candle.clone()).await;
                                }
                                // AUDIT-V8-005 (reconnect-gap indicator continuity):
                                // feed the reconstructed candle through EVERY
                                // stateful indicator and broadcast a fully-
                                // populated snapshot so the chart's EMA/RSI lines
                                // advance across reconnect gaps instead of
                                // bridging them with straight lines.
                                // These snapshots are still never persisted to
                                // the DB (the `reconstructed` flag + the
                                // `quality_envelope.is_gap_filled` marker keep
                                // them out of the historical store — see the
                                // rationale in the prior comment block).
                                let gap_close_sec = gap_candle.start_time_ms / 1000;
                                let day_index = gap_close_sec / 86400;
                                if let Some(prev_day) = last_day_index {
                                    if day_index > prev_day {
                                        vwap_sum_tp_vol = Decimal::ZERO;
                                        vwap_sum_vol = Decimal::ZERO;
                                    }
                                }
                                last_day_index = Some(day_index);
                                let gap_readings = apply_candle_to_indicators(
                                    &symbol,
                                    slot,
                                    timeframe_secs,
                                    &gap_candle,
                                    day_index,
                                    &mut pivot_points_indicator,
                                    &mut candlestick_indicator,
                                    &mut ichimoku_indicator,
                                    &mut cci_indicator,
                                    &mut psar_indicator,
                                    &mut wr_indicator,
                                    &mut hma_indicator,
                                    &mut ao_indicator,
                                    &mut fi_indicator,
                                    &mut sdc_indicator,
                                    &mut volume_profile_indicator,
                                    &mut smc_indicator,
                                    &mut anchored_vwap_indicator,
                                    &mut ema_fast,
                                    &mut ema_medium,
                                    &mut ema_slow,
                                    &mut ema_long,
                                    &mut rsi_14,
                                    &mut macd,
                                    &mut adx_14,
                                    &mut sqz_mom,
                                    &mut bollinger,
                                    &mut atr_standalone,
                                    &mut bbwp_indicator,
                                    &mut stochastic_indicator,
                                    &mut chandemo_indicator,
                                    &mut supertrend_indicator,
                                    &mut keltner_indicator,
                                    &mut donchian_indicator,
                                    &mut obv_indicator,
                                    &mut cmf_indicator,
                                    &mut mfi_indicator,
                                    &mut hv_indicator,
                                    &mut aroon_indicator,
                                    &mut choppiness_indicator,
                                    &mut linreg_indicator,
                                    &mut zscore_indicator,
                                    &mut vwap_sum_tp_vol,
                                    &mut vwap_sum_vol,
                                );
                                bar_count = bar_count.saturating_add(1);
                                last_close = gap_candle.close;
                                let avg_vol = if !volume_history.is_empty() {
                                    let sum: Decimal = volume_history.iter().sum();
                                    Some(sum / Decimal::from(volume_history.len()))
                                } else {
                                    None
                                };
                                let rvol = match (gap_candle.volume, avg_vol) {
                                    (vol, Some(avg)) if avg > Decimal::ZERO => Some(vol / avg),
                                    _ => None,
                                };
                                // v6.11: reconstructed gap candles are REAL
                                // closes — roll the Sharpe window (PRI-06
                                // history continuity) and derive the ratio.
                                let gap_price_sharpe = {
                                    let close_f_gap = gap_candle.close.to_f64().unwrap_or(0.0);
                                    close_history.push_back(close_f_gap);
                                    while close_history.len()
                                        > crate::indicators::ratio::SHARPE_WINDOW
                                    {
                                        close_history.pop_front();
                                    }
                                    let closes: Vec<f64> = close_history.iter().copied().collect();
                                    crate::indicators::sharpe_ratio_annualized(
                                        &closes,
                                        timeframe_secs,
                                    )
                                };
                                let gap_snap = build_completed_snapshot_from_readings(
                                    &gap_readings,
                                    &gap_candle,
                                    &symbol,
                                    &pair_key,
                                    slot,
                                    timeframe_secs,
                                    bar_count,
                                    real_bar_count,
                                    shadow_exchange,
                                    shadow_bid,
                                    shadow_ask,
                                    (
                                        active_indicators.ema_fast,
                                        active_indicators.ema_medium,
                                        active_indicators.ema_slow,
                                        active_indicators.ema_long,
                                    ),
                                    &active_set,
                                    buffer_size,
                                    avg_vol,
                                    rvol,
                                    stale_threshold_secs,
                                    last_completed_start_ms,
                                    now_ms,
                                    gap_price_sharpe,
                                );
                                let _ = broadcast_tx.send(gap_snap.clone());
                                // AUDIT-M12: unify synthetic-candle persistence.
                                // Doji/idle heartbeats persist via
                                // `InsertSnapshot` (K3 restart continuity);
                                // gap-fill candles previously did NOT — so a
                                // restart after an outage kept different
                                // history depending on which recovery path
                                // filled the gap. Gap candles carry the
                                // `reconstructed` provenance column exactly
                                // like the other synthetic classes; persist
                                // them the same way.
                                send_telemetry(
                                    &telemetry_tx,
                                    database_storage::TelemetryMsg::InsertSnapshot(Box::new(
                                        gap_snap.clone(),
                                    )),
                                );
                                // AUDIT-V8-004: keep the in-memory snapshot
                                // history continuous across reconnect gaps.
                                {
                                    let mut snap_hist = snapshot_history.write().await;
                                    snap_hist.push_back(gap_snap);
                                    while snap_hist.len() > HIST_BUFFER_MAX {
                                        snap_hist.pop_front();
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(completed) = completed_opt {
                    // AUDIT-H3: the gap must be computed against the PREVIOUS
                    // completed candle BEFORE `last_completed_start_ms` is
                    // overwritten below — the old ordering read back the
                    // current candle's own start, so `gap_since_last` was
                    // permanently 0 on the primary trade-triggered path (the
                    // force-close path computed it correctly before updating,
                    // so the two paths disagreed).
                    let gap_secs = match last_completed_start_ms {
                        Some(prev) => completed.start_time_ms.saturating_sub(prev) / 1000,
                        None => duration_ms / 1000,
                    };
                    last_completed_start_ms = Some(completed.start_time_ms);
                    // AUDIT-V8-003: last known close feeds the idle-bucket
                    // heartbeat when the market subsequently goes quiet.
                    last_close = completed.close;
                    reliability.increment_candles(1).await;
                    // AUDIT-L5: the old `println!` + full serde_json
                    // serialization of every completed candle was
                    // synchronous stdout I/O on the hot path (a 1 s TF with
                    // multiple symbols prints ~1 line/sec/symbol) — removed.
                    // Telemetry consumers get the same data via
                    // TelemetryMsg / the WS broadcast.

                    let is_valid = completed.assert_validity().is_ok();

                    let is_reconstructed = completed.reconstructed.is_some();
                    let rejected_total = median_filter
                        .as_ref()
                        .map(|f| f.outliers_rejected())
                        .unwrap_or(0);
                    let had_outliers_this_candle = rejected_total > outliers_at_prev_candle;
                    outliers_at_prev_candle = rejected_total;

                    let now_ms = core_domain::LatencyTracker::now_ms();
                    let candle_close_ms = completed.start_time_ms + duration_ms;
                    let candle_stale = last_trade_ts_ms > 0
                        && candle_close_ms.saturating_sub(last_trade_ts_ms)
                            > staleness_threshold_ms;

                    let quality_score = if !is_valid {
                        0.0
                    } else {
                        let mut score = 100.0_f64;
                        if is_reconstructed {
                            score -= 20.0;
                        }
                        if had_outliers_this_candle {
                            score -= 10.0;
                        }
                        if candle_stale {
                            score -= 30.0;
                        }
                        score.clamp(0.0, 100.0)
                    };

                    let quality_envelope = CandleQualityEnvelope {
                        quality_score,
                        is_valid,
                        is_gap_filled: is_reconstructed,
                        had_outliers_rejected: had_outliers_this_candle,
                        spike_detected: had_outliers_this_candle,
                        is_stale: candle_stale,
                        sequence_integrity: classify_sequence(
                            last_completed_start_ms,
                            completed.start_time_ms,
                        ),
                        gap_since_last: gap_secs,
                        validated_at: now_ms,
                    };
                    let candle_close_sec = completed.start_time_ms / 1000;
                    let day_index = candle_close_sec / 86400;
                    if let Some(prev_day) = last_day_index {
                        if day_index > prev_day {
                            vwap_sum_tp_vol = Decimal::ZERO;
                            vwap_sum_vol = Decimal::ZERO;
                        }
                    }
                    last_day_index = Some(day_index);

                    // ── Apply the candle to every stateful indicator ──
                    // The single source of truth for "what the indicators
                    // become after a candle close of `duration_ms`". Called
                    // identically from the trade-triggered boundary-cross
                    // path, the stale-check `force_close` path, and the
                    // doji-fill path so all 30+ indicators (EMA/RSI/MACD/
                    // Bollinger/ATR/ADX/Stochastic/Keltner/Donchian/OBV/
                    // CMF/MFI/HV/Aroon/Choppiness/LinRegSlope/ZScore/
                    // HullMA/AO/FI/WilliamsR/CCI/PSAR/StdDev/VolumeProfile/
                    // Pivot/Candlestick/Ichimoku/SMC/AnchoredVWAP) advance
                    // on every wall-clock second. See
                    // `tests/sub_minute_indicator_cadence.rs` for the bug
                    // pins that motivated this extraction.
                    let readings = apply_candle_to_indicators(
                        &symbol,
                        slot,
                        timeframe_secs,
                        &completed,
                        day_index,
                        &mut pivot_points_indicator,
                        &mut candlestick_indicator,
                        &mut ichimoku_indicator,
                        &mut cci_indicator,
                        &mut psar_indicator,
                        &mut wr_indicator,
                        &mut hma_indicator,
                        &mut ao_indicator,
                        &mut fi_indicator,
                        &mut sdc_indicator,
                        &mut volume_profile_indicator,
                        &mut smc_indicator,
                        &mut anchored_vwap_indicator,
                        &mut ema_fast,
                        &mut ema_medium,
                        &mut ema_slow,
                        &mut ema_long,
                        &mut rsi_14,
                        &mut macd,
                        &mut adx_14,
                        &mut sqz_mom,
                        &mut bollinger,
                        &mut atr_standalone,
                        &mut bbwp_indicator,
                        &mut stochastic_indicator,
                        &mut chandemo_indicator,
                        &mut supertrend_indicator,
                        &mut keltner_indicator,
                        &mut donchian_indicator,
                        &mut obv_indicator,
                        &mut cmf_indicator,
                        &mut mfi_indicator,
                        &mut hv_indicator,
                        &mut aroon_indicator,
                        &mut choppiness_indicator,
                        &mut linreg_indicator,
                        &mut zscore_indicator,
                        &mut vwap_sum_tp_vol,
                        &mut vwap_sum_vol,
                    );

                    let completed_snapshot = synthesize_completed_candle(
                        &symbol,
                        timeframe_label,
                        slot,
                        timeframe_secs,
                        &completed,
                        candle_close_sec,
                        readings,
                        &mut bar_count,
                        &mut real_bar_count,
                        &quality_envelope,
                        shadow_exchange,
                        shadow_bid,
                        shadow_ask,
                        shadow_prev_day_px,
                        last_ob_ms,
                        grace_period_ms,
                        &active_indicators,
                        &active_set,
                        buffer_size,
                        stale_threshold_secs,
                        last_completed_start_ms,
                        now_ms,
                        &fib_config,
                        &liquidity_config,
                        spread_wide_threshold_pct,
                        &history,
                        &mut volume_history,
                        &mut close_history,
                        &mut adx_slope_history,
                        &divergence_detector,
                        &mut sr_tracker,
                        &mut last_pivot_count,
                        &mut anchored_vwap_indicator,
                        &mut stoch_div,
                        &mut chandemo_div,
                        &mut mfi_div,
                        &mut cmf_div,
                        &mut obv_div,
                        &mut squeeze_div,
                        &mut signal_age_tracker,
                        &mut live_bar,
                        &mut prev_bar_state,
                        &mut sil_engine,
                        &mut prev_sil_close,
                        &mut mc_counter,
                        &mut prev_mtf_score,
                        &mut prev_regime,
                        &mut prev_volume_dim,
                        &mut prev_bias,
                        &mut last_cascade_state,
                        &mut liquidity_acc,
                        &latest_oi,
                        &latest_funding,
                        &latest_mark_px,
                        &latest_index_px,
                        &oi_history,
                        &funding_history,
                        &order_book_analysis,
                        &cross_tf_snapshots,
                        &cluster_matrix,
                        &indicator_lifecycle,
                        &pipeline_state_handle,
                        &latest_snapshot,
                        &advisory,
                        &broadcast_tx,
                        &telemetry_tx,
                        &latency_tracker,
                        &strategy,
                    )
                    .await;

                    // The strict handover gate above guarantees this candle is
                    // strictly newer than any historical candle, so it is always
                    // a fresh append — no dedup/overwrite of historical data.
                    {
                        let mut hist = history.write().await;
                        hist.push_back(completed.clone());
                        while hist.len() > HIST_BUFFER_MAX {
                            hist.pop_front();
                        }
                        let mut snap_hist = snapshot_history.write().await;
                        snap_hist.push_back(completed_snapshot.clone());
                        while snap_hist.len() > HIST_BUFFER_MAX {
                            snap_hist.pop_front();
                        }
                    }
                    if let Some(ref tx) = candle_forward {
                        let _ = tx.send(completed.clone()).await;
                    }
                }

                // BROADCAST: Flickering snapshot from live candle (throttled
                // to `shadow_throttle_ms` so sub-60s timeframes don't drown
                // the frontend; candle-close path above is unaffected).
                {
                    let now_ms = core_domain::LatencyTracker::now_ms();
                    if now_ms.saturating_sub(last_shadow_broadcast_ms) >= shadow_throttle_ms {
                        last_shadow_broadcast_ms = now_ms;
                        let ob_fresh =
                            last_ob_ms > 0 && now_ms.saturating_sub(last_ob_ms) <= grace_period_ms;
                        let ob_bid_size = if ob_fresh {
                            order_book_analysis
                                .best_bid_size()
                                .and_then(Decimal::from_f64_retain)
                        } else {
                            None
                        };
                        let ob_ask_size = if ob_fresh {
                            order_book_analysis
                                .best_ask_size()
                                .and_then(Decimal::from_f64_retain)
                        } else {
                            None
                        };
                        broadcast_live_snapshot(
                            &broadcast_tx,
                            &symbol,
                            &live_candle,
                            shadow_exchange,
                            shadow_bid,
                            shadow_ask,
                            ob_bid_size,
                            ob_ask_size,
                            slot,
                            &ema_fast,
                            &ema_medium,
                            &ema_slow,
                            &ema_long,
                            &rsi_14,
                            &macd,
                            &adx_14,
                            &sqz_mom,
                            &bollinger,
                            &atr_standalone,
                            &bbwp_indicator,
                            &stochastic_indicator,
                            &chandemo_indicator,
                            &supertrend_indicator,
                            &keltner_indicator,
                            &donchian_indicator,
                            &obv_indicator,
                            &cmf_indicator,
                            &mfi_indicator,
                            &hv_indicator,
                            &aroon_indicator,
                            &choppiness_indicator,
                            &linreg_indicator,
                            &zscore_indicator,
                            &vwap_sum_tp_vol,
                            &vwap_sum_vol,
                            &volume_history,
                            timeframe_secs,
                            shadow_prev_day_px,
                            bar_count,
                            real_bar_count,
                            (
                                active_indicators.ema_fast,
                                active_indicators.ema_medium,
                                active_indicators.ema_slow,
                                active_indicators.ema_long,
                            ),
                            derive_pipeline_state(bar_count as usize, buffer_size),
                            &active_set,
                            false,
                        );
                    }
                }
            }

            NormalizedEvent::OrderBook(ref book) => {
                shadow_exchange = Some(book.exchange);
                // AUDIT-V8-002: stamp the order-book freshness used by the
                // stale-check force-close mid guard.
                last_ob_ms = core_domain::LatencyTracker::now_ms();
                if let (Some(best_bid), Some(best_ask)) = (book.bids.first(), book.asks.first()) {
                    shadow_bid = best_bid.0;
                    shadow_ask = best_ask.0;
                }

                // Update order book depth analysis
                {
                    let bids_f64: Vec<(f64, f64)> = book
                        .bids
                        .iter()
                        .map(|(p, s)| (p.to_f64().unwrap_or(0.0), s.to_f64().unwrap_or(0.0)))
                        .collect();
                    let asks_f64: Vec<(f64, f64)> = book
                        .asks
                        .iter()
                        .map(|(p, s)| (p.to_f64().unwrap_or(0.0), s.to_f64().unwrap_or(0.0)))
                        .collect();
                    order_book_analysis.update(&bids_f64, &asks_f64);
                }

                if candle_gen.current_candle.is_some() {
                    let mid = (shadow_bid + shadow_ask) / Decimal::from(2);
                    let shadow_candle = NormalizedCandle {
                        exchange: candle_gen.exchange,
                        symbol: symbol.clone(),
                        start_time_ms: candle_gen.current_start_ms,
                        duration_ms: candle_gen.duration_ms,
                        open: candle_gen.current_open,
                        high: candle_gen.current_high.max(mid),
                        low: candle_gen.current_low.min(mid),
                        close: mid,
                        volume: candle_gen.current_volume,
                        trades_count: candle_gen.current_trades,
                        reconstructed: None,
                    };

                    // Throttle the order-book shadow broadcast at the
                    // same cadence as the trade-tick shadow path; otherwise
                    // sub-60s timeframes emit ~200 broadcasts/sec on the
                    // order-book channel alone.
                    let now_ms = core_domain::LatencyTracker::now_ms();
                    if now_ms.saturating_sub(last_shadow_broadcast_ms) >= shadow_throttle_ms {
                        last_shadow_broadcast_ms = now_ms;
                        let ob_fresh =
                            last_ob_ms > 0 && now_ms.saturating_sub(last_ob_ms) <= grace_period_ms;
                        let ob_bid_size = if ob_fresh {
                            order_book_analysis
                                .best_bid_size()
                                .and_then(Decimal::from_f64_retain)
                        } else {
                            None
                        };
                        let ob_ask_size = if ob_fresh {
                            order_book_analysis
                                .best_ask_size()
                                .and_then(Decimal::from_f64_retain)
                        } else {
                            None
                        };
                        broadcast_live_snapshot(
                            &broadcast_tx,
                            &symbol,
                            &shadow_candle,
                            shadow_exchange,
                            shadow_bid,
                            shadow_ask,
                            ob_bid_size,
                            ob_ask_size,
                            slot,
                            &ema_fast,
                            &ema_medium,
                            &ema_slow,
                            &ema_long,
                            &rsi_14,
                            &macd,
                            &adx_14,
                            &sqz_mom,
                            &bollinger,
                            &atr_standalone,
                            &bbwp_indicator,
                            &stochastic_indicator,
                            &chandemo_indicator,
                            &supertrend_indicator,
                            &keltner_indicator,
                            &donchian_indicator,
                            &obv_indicator,
                            &cmf_indicator,
                            &mfi_indicator,
                            &hv_indicator,
                            &aroon_indicator,
                            &choppiness_indicator,
                            &linreg_indicator,
                            &zscore_indicator,
                            &vwap_sum_tp_vol,
                            &vwap_sum_vol,
                            &volume_history,
                            timeframe_secs,
                            shadow_prev_day_px,
                            bar_count,
                            real_bar_count,
                            (
                                active_indicators.ema_fast,
                                active_indicators.ema_medium,
                                active_indicators.ema_slow,
                                active_indicators.ema_long,
                            ),
                            derive_pipeline_state(bar_count as usize, buffer_size),
                            &active_set,
                            false,
                        );
                    }
                }
            }

            NormalizedEvent::AssetContext(ref ctx) => {
                shadow_prev_day_px = Some(ctx.prev_day_px);
            }

            NormalizedEvent::OpenInterest(ref oi) => {
                let mut guard = latest_oi.write().await;
                *guard = Some(oi.oi);
            }

            NormalizedEvent::FundingRate(ref fr) => {
                let mut guard = latest_funding.write().await;
                *guard = Some(fr.rate);
            }

            NormalizedEvent::MarkPrice(ref mp) => {
                let mut mark_guard = latest_mark_px.write().await;
                *mark_guard = Some(mp.mark_px);
                if let Some(idx) = mp.index_px {
                    let mut idx_guard = latest_index_px.write().await;
                    *idx_guard = Some(idx);
                }
            }

            // Phase 1 hook: Liquidation events are also persisted to DB via
            // the telemetry channel. The flow aggregation happens here in a
            // later phase (Phase 1 + accumulator) but persisting the raw events
            // now means we have data ready when the flow logic lands.
            NormalizedEvent::Liquidation(ref liq) => {
                let side_str = match liq.side {
                    core_domain::normalized::LiquidationSide::Long => "LONG",
                    core_domain::normalized::LiquidationSide::Short => "SHORT",
                };
                let size_usd = liq.price.to_f64().unwrap_or(0.0) * liq.size.to_f64().unwrap_or(0.0);
                send_telemetry(
                    &telemetry_tx,
                    database_storage::TelemetryMsg::InsertLiquidationEvent {
                        exchange: liq.exchange,
                        symbol: liq.symbol.clone(),
                        side: side_str.to_string(),
                        price: liq.price.to_f64().unwrap_or(0.0),
                        size_usd,
                        timestamp_ms: liq.timestamp_ms,
                        venue_order_id: liq.venue_order_id.clone(),
                    },
                );
                // Phase 1: feed the per-candle aggregator. This drives the
                // `LiquidityFlow` attached to the next completed snapshot.
                // CA-15: the `liquidation_feed` sub-toggle drops the feed at
                // the L1.5 boundary — events are still persisted raw (DIE
                // ingestion continues unchanged) but never aggregated, so
                // the flow stays empty and cascade detection stays silent.
                if active_set.liquidation_feed {
                    liquidity_acc.record_event(liq.clone());
                }
            }

            NormalizedEvent::Status {
                exchange,
                status,
                message,
            } => {
                println!(
                    "[STATUS {}] {}: {:?} — {}",
                    timeframe_label, exchange, status, message
                );
            }
        }
    }
}
/// Full per-candle matrix synthesis shared by the trade-triggered and
/// stale-check `force_close` completion paths. Builds the indicator map from
/// the freshly-applied [`CandleIndicatorReadings`], injects derivatives +
/// order-book indicators, advances the statistical-intelligence / divergence /
/// S/R stateful trackers, runs the cross-TF synthesis, and produces + broadcasts
/// the completed `MarketSnapshot` with the full matrix payload
/// (alignment/analysis/risk/advisory/opportunity/decision_context).
///
/// Every stateful tracker advanced here runs exactly once per closed candle:
/// the v6.11 dedup gate in `run_single` guarantees a bucket is processed by
/// either the trade-triggered path or the force-close path, never both.
#[allow(clippy::too_many_arguments)]
async fn synthesize_completed_candle(
    symbol: &str,
    timeframe_label: &str,
    slot: TimeframeSlot,
    timeframe_secs: u64,
    completed: &NormalizedCandle,
    candle_close_sec: u64,
    readings: CandleIndicatorReadings,
    bar_count: &mut u32,
    real_bar_count: &mut u32,
    quality_envelope: &CandleQualityEnvelope,
    shadow_exchange: Option<Exchange>,
    shadow_bid: Decimal,
    shadow_ask: Decimal,
    shadow_prev_day_px: Option<Decimal>,
    // AUDIT-V8-002 freshness guard inputs: the last order-book event
    // wall-clock time and the grace window. Used to gate the emitted
    // `mid_price` (fresh bid/ask mid vs candle close) and the top-of-book
    // `bid_size` / `ask_size` fields.
    last_ob_ms: u64,
    grace_period_ms: u64,
    active_indicators: &config_models::IndicatorsConfig,
    active_set: &crate::active_set::ActiveSet,
    buffer_size: usize,
    // AUDIT-H7: `[candle_buffer] stale_threshold_secs` for the lifecycle map.
    stale_threshold_secs: u32,
    // AUDIT-H7: DCP-05 staleness inputs — last completed candle start + now.
    last_completed_ms: Option<u64>,
    now_ms: u64,
    fib_config: &FibonacciConfig,
    liquidity_config: &Option<LiquidityConfig>,
    spread_wide_threshold_pct: f64,
    history: &Arc<RwLock<VecDeque<NormalizedCandle>>>,
    volume_history: &mut VecDeque<Decimal>,
    // v6.11: rolling 300-close window backing the L1 `price_trend_sharpe`.
    // Real completed candles only (PRI-06 — synthetic doji/idle buckets
    // never enter the Sharpe window either).
    close_history: &mut VecDeque<f64>,
    adx_slope_history: &mut VecDeque<Decimal>,
    divergence_detector: &Arc<tokio::sync::Mutex<DivergenceDetector>>,
    sr_tracker: &mut SrRoleTracker,
    last_pivot_count: &mut usize,
    anchored_vwap_indicator: &mut AnchoredVwap,
    stoch_div: &mut SeriesDivergence,
    chandemo_div: &mut SeriesDivergence,
    mfi_div: &mut SeriesDivergence,
    cmf_div: &mut SeriesDivergence,
    obv_div: &mut SeriesDivergence,
    squeeze_div: &mut SeriesDivergence,
    signal_age_tracker: &mut std::collections::HashMap<
        String,
        (u32, crate::indicators::SignalDirection),
    >,
    live_bar: &mut u32,
    prev_bar_state: &mut PreviousBarState,
    sil_engine: &mut StatisticsEngine,
    prev_sil_close: &mut f64,
    mc_counter: &mut u64,
    prev_mtf_score: &mut Option<f64>,
    prev_regime: &mut Option<core_domain::analysis::MarketRegime>,
    prev_volume_dim: &mut Option<f64>,
    prev_bias: &mut Option<core_domain::analysis::MarketBias>,
    last_cascade_state: &mut core_domain::liquidity::CascadeState,
    liquidity_acc: &mut core_domain::liquidity::LiquidityEventAccumulator,
    latest_oi: &Arc<RwLock<Option<Decimal>>>,
    latest_funding: &Arc<RwLock<Option<Decimal>>>,
    latest_mark_px: &Arc<RwLock<Option<Decimal>>>,
    latest_index_px: &Arc<RwLock<Option<Decimal>>>,
    oi_history: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    funding_history: &Arc<RwLock<VecDeque<f64>>>,
    order_book_analysis: &OrderBookAnalysis,
    cross_tf_snapshots: &[Arc<RwLock<Option<MarketSnapshot>>>],
    cluster_matrix: &Arc<RwLock<Option<LiquidationClusterMatrix>>>,
    indicator_lifecycle: &Arc<RwLock<IndicatorLifecycleMap>>,
    pipeline_state_handle: &Arc<RwLock<CandlePipelineState>>,
    latest_snapshot: &Arc<RwLock<Option<MarketSnapshot>>>,
    advisory: &Arc<RwLock<Option<AdvisoryMatrix>>>,
    broadcast_tx: &broadcast::Sender<MarketSnapshot>,
    telemetry_tx: &Sender<database_storage::TelemetryMsg>,
    latency_tracker: &core_domain::SharedLatencyTracker,
    // v9: the effective strategy.
    strategy: &StrategyConfig,
) -> MarketSnapshot {
    // Freshness guard for the order-book-backed envelope fields
    // (`mid_price` mid-of-book, `bid_size`/`ask_size` top-of-book depth).
    // Same AUDIT-V8-002 semantics as the doji-fill path: the book is only
    // authoritative while the last book event sits inside the grace window.
    let ob_fresh = last_ob_ms > 0
        && core_domain::LatencyTracker::now_ms().saturating_sub(last_ob_ms) <= grace_period_ms;
    // Deconstruct the just-computed readings into the named bindings the
    // synthesis block below was written against (same shape as the original
    // inline destructure of `apply_candle_to_indicators`' return).
    let CandleIndicatorReadings {
        open_f: _,
        high_f: _,
        low_f: _,
        close_f,
        volume_f,
        pivot_levels,
        candlestick_reading,
        ichimoku_reading,
        cci_reading,
        psar_reading,
        wr_reading,
        hma_reading,
        ao_reading,
        fi_reading,
        fi_mean_abs,
        sdc_reading,
        volume_profile_reading,
        volume_profile_snapshot,
        smc_reading,
        final_vwap,
        avwap_reading,
        final_ema_fast,
        final_ema_medium,
        final_ema_slow,
        final_ema_long,
        ema_stack_state,
        final_rsi,
        final_macd,
        final_adx,
        final_sqz,
        final_bb,
        final_atr,
        final_bbwp,
        final_stoch,
        final_cmo,
        final_supertrend,
        final_keltner,
        final_donchian,
        final_obv,
        final_cmf,
        final_mfi,
        final_hv,
        final_aroon,
        final_chop,
        final_linreg,
        final_zscore,
    } = readings;

    // v6.11: roll the Sharpe window with this real completed close. Only
    // real candles enter the buffer (synthetic doji/idle buckets go
    // through `build_completed_snapshot_from_readings` with `None` ratios),
    // matching the PRI-06 history-continuity convention.
    close_history.push_back(close_f);
    while close_history.len() > crate::indicators::ratio::SHARPE_WINDOW {
        close_history.pop_front();
    }
    let price_trend_sharpe = {
        let closes: Vec<f64> = close_history.iter().copied().collect();
        crate::indicators::sharpe_ratio_annualized(&closes, timeframe_secs)
    };

    // ── Generalized divergence detection ──
    // Each oscillator's SeriesDivergence is updated every bar
    // for the RSI/MACD confirmation path below. The 6 extra
    // divergence states are resolved inline inside NormalizeParams
    // (below) so sr_supports/resistances are in scope for the
    // S/R-confirmation gate.

    // Divergence detection (live — potential status; confirmation
    // applied after S/R levels are computed below).
    let mut div_result = {
        if let (Some(rsi), macd_hist) = (final_rsi, final_macd.histogram) {
            divergence_detector.lock().await.update_full(
                close_f,
                rsi.to_f64().unwrap_or(0.0),
                macd_hist.to_f64().unwrap_or(0.0),
            )
        } else {
            crate::indicators::DivergenceResult::default_div()
        }
    };

    let log_line = format!(
        "🕯️  [{}] {} Candle Closed | Start: {} | Close: ${:.4} | Vol: {:.4} | Trades: {}",
        symbol,
        timeframe_label,
        completed.start_time_ms,
        completed.close,
        completed.volume,
        completed.trades_count
    );
    send_telemetry(
        telemetry_tx,
        database_storage::TelemetryMsg::ConsoleLog(log_line),
    );

    volume_history.push_back(completed.volume);
    // AUDIT-AIU-070: configured `volume_average_period` window.
    let vol_window = active_indicators.volume_average_period.max(1);
    while volume_history.len() > vol_window {
        volume_history.pop_front();
    }
    let avg_vol = if !volume_history.is_empty() {
        let sum: Decimal = volume_history.iter().sum();
        Some(sum / Decimal::from(volume_history.len()))
    } else {
        None
    };

    let rvol = match (completed.volume, avg_vol) {
        (vol, Some(avg)) if avg > Decimal::ZERO => Some(vol / avg),
        _ => None,
    };

    // Fibonacci retracement/extension computation
    let fib = {
        let hist = history.read().await;
        let candles_high: Vec<Decimal> = hist.iter().map(|c| c.high).collect();
        let candles_low: Vec<Decimal> = hist.iter().map(|c| c.low).collect();
        FibonacciRange::compute_from_candles(
            &candles_high,
            &candles_low,
            fib_config.swing_lookback,
            fib_config.swing_scan_range,
            &fib_config.retracement_coefficients,
            &fib_config.extension_coefficients,
        )
    };

    // Chart pattern detection from pivots (reused for S/R zones)
    let pivots = {
        let hist = history.read().await;
        let candles_high: Vec<Decimal> = hist.iter().map(|c| c.high).collect();
        let candles_low: Vec<Decimal> = hist.iter().map(|c| c.low).collect();
        FibonacciRange::detect_pivots(
            &candles_high,
            &candles_low,
            fib_config.swing_lookback,
            fib_config.swing_scan_range,
        )
    };
    if pivots.len() > *last_pivot_count {
        anchored_vwap_indicator.reset_swing();
    }
    *last_pivot_count = pivots.len();
    let pattern_result = detect_pattern(&pivots);

    // Support/Resistance zones: derive role-adjusted levels from
    // the swing pivots and update the flip tracker on this close.
    let (sr_supports, sr_resistances) =
        update_sr_levels(sr_tracker, &pivots, completed.close, candle_close_sec);

    // Upgrade RSI/MACD potential divergences to Confirmed when
    // the candle close decisively breaks the nearest S/R level.
    // check_divergence_confirmation is a &self method on the
    // DivergenceDetector — we lock it again briefly.
    //
    // AUDIT-AIU-002: a bullish confirmation requires
    // `close < support` (close breaks BELOW the level), and a
    // bearish confirmation requires `close > resistance`.
    // The previous selection (`support <= close` / `resistance
    // >= close`) made both checks unsatisfiable by
    // construction, so RSI/MACD divergences could never reach
    // Confirmed in the live path. We now select the nearest
    // level on the break side: support ABOVE close, resistance
    // BELOW close — mirroring `series_divergence_confirmed`.
    {
        let near_sup = sr_supports
            .iter()
            .copied()
            .filter(|s| *s > 0.0 && *s > close_f)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let near_res = sr_resistances
            .iter()
            .copied()
            .filter(|r| *r > 0.0 && *r < close_f)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if near_sup.is_some() || near_res.is_some() {
            let det = divergence_detector.lock().await;
            div_result =
                det.check_divergence_confirmation(&div_result, close_f, near_sup, near_res);
        }
    }

    // Track ADX slope history for the 2-bar hook-exit rule.
    if let Some(a) = final_adx.as_ref() {
        adx_slope_history.push_back(a.adx_slope);
        while adx_slope_history.len() > 3 {
            adx_slope_history.pop_front();
        }
    }
    let adx_consecutive_deceleration = adx_slope_history.len() >= 2
        && adx_slope_history
            .iter()
            .rev()
            .take(2)
            .all(|s| *s < Decimal::ZERO);

    // Active position context for direction-aware normalization. The
    // paper-trading position query was removed (stub; cycle broken) — the
    // direction-aware branch always runs with the neutral 0 context.
    let active_position: Option<i8> = Some(0);

    let ema_stack_str = ema_stack_state.as_deref();
    // Increment bar_count BEFORE building the indicator map so
    // the gate sees this candle's contribution.  Must precede
    // `build_indicator_map` which uses `bar_count` for the
    // bars_required gate.
    *bar_count = bar_count.saturating_add(1);
    // PRI-12: every candle processed by the synthesis is a REAL completion
    // (trade-triggered or clock-driven force-close) — synthetic dojis never
    // reach this function.
    *real_bar_count = real_bar_count.saturating_add(1);
    let indicators = normalize::build_indicator_map(
        normalize::NormalizeParams {
            close: completed.close,
            rsi: final_rsi,
            rsi_divergence: normalize::rsi_divergence_state(&div_result),
            macd_divergence: normalize::macd_divergence_state(&div_result),
            stoch_k: final_stoch.as_ref().map(|s| s.k_value),
            stoch_d: final_stoch.as_ref().map(|s| s.d_value),
            chandemo: final_cmo,
            supertrend_line: final_supertrend.as_ref().map(|s| s.line),
            supertrend_dir: final_supertrend.as_ref().map(|s| s.direction),
            keltner: final_keltner.as_ref().map(|k| (k.upper, k.middle, k.lower)),
            donchian: final_donchian
                .as_ref()
                .map(|d| (d.upper, d.middle, d.lower)),
            obv: final_obv.as_ref().map(|o| o.obv),
            obv_sma: final_obv.as_ref().map(|o| o.obv_sma),
            cmf: final_cmf,
            mfi: final_mfi,
            hv: final_hv,
            aroon_up: final_aroon.as_ref().map(|a| a.up),
            aroon_down: final_aroon.as_ref().map(|a| a.down),
            choppiness: final_chop,
            linreg_slope: final_linreg,
            zscore: final_zscore,
            extra_div: normalize::ExtraDivergence {
                stochastic: final_stoch
                    .as_ref()
                    .map(|s| {
                        normalize::series_divergence_confirmed(
                            &stoch_div.update(close_f, s.k_value.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
                chandemo: final_cmo
                    .map(|v| {
                        normalize::series_divergence_confirmed(
                            &chandemo_div.update(close_f, v.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
                mfi: final_mfi
                    .map(|v| {
                        normalize::series_divergence_confirmed(
                            &mfi_div.update(close_f, v.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
                cmf: final_cmf
                    .map(|v| {
                        normalize::series_divergence_confirmed(
                            &cmf_div.update(close_f, v.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
                obv: final_obv
                    .as_ref()
                    .map(|o| {
                        normalize::series_divergence_confirmed(
                            &obv_div.update(close_f, o.obv.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
                squeeze: final_sqz
                    .as_ref()
                    .map(|s| {
                        normalize::series_divergence_confirmed(
                            &squeeze_div.update(close_f, s.momentum_value.to_f64().unwrap_or(0.0)),
                            close_f,
                            &sr_supports,
                            &sr_resistances,
                        )
                    })
                    .unwrap_or_default(),
            },
            macd: &final_macd,
            sqz: final_sqz.as_ref(),
            adx: final_adx.as_ref(),
            bb: final_bb,
            atr: final_atr.as_ref(),
            bbwp: final_bbwp,
            vwap: final_vwap,
            anchored_vwap: Some(avwap_reading),
            ema_stack_state: ema_stack_str,
            ema_fast: Some(final_ema_fast),
            ema_medium: Some(final_ema_medium),
            ema_slow: Some(final_ema_slow),
            ema_long: Some(final_ema_long),
            // AUDIT-V8-001: configured periods for per-line
            // ribbon gating (fast@10, medium@50, slow@100, long@200).
            ema_periods: (
                active_indicators.ema_fast,
                active_indicators.ema_medium,
                active_indicators.ema_slow,
                active_indicators.ema_long,
            ),
            rvol,
            volume: Some(completed.volume),
            average_volume: avg_vol,
            fib: Some(&fib),
            pattern: Some(&pattern_result),
            support_levels: &sr_supports,
            resistance_levels: &sr_resistances,
            active_position,
            adx_consecutive_deceleration,
            supertrend_flipped: final_supertrend
                .as_ref()
                .map(|s| s.flipped)
                .unwrap_or(false),
            adx_di_crossover: final_adx.as_ref().and_then(|a| {
                a.di_crossover.map(|c| match c {
                    crate::indicators::DiCrossoverDir::Bullish => 1i8,
                    crate::indicators::DiCrossoverDir::Bearish => -1i8,
                })
            }),
            pivot_levels,
            pivot_proximity_pct: 0.0015,
            candlestick: Some(candlestick_reading),
            // AUDIT-AIU-073: configured min-confidence gate.
            candlestick_min_confidence: active_indicators.candlestick_min_confidence,
            ichimoku: ichimoku_reading,
            cci: cci_reading,
            psar: psar_reading,
            williams_r: wr_reading,
            awesome_oscillator: ao_reading,
            force_index: fi_reading,
            force_index_mean_abs: fi_mean_abs,
            hull_ma: hma_reading,
            stddev_channel: sdc_reading,
            volume_profile: volume_profile_reading,
            smc: smc_reading,
            prev: prev_bar_state.clone(),
            // AUDIT-AIU-071: rvol thresholds from config.
            rvol_institutional_threshold: active_indicators.rvol_threshold_institutional,
            rvol_climax_threshold: active_indicators.rvol_threshold_climax,
            price_trend_sharpe,
        },
        *bar_count,
        false,
        active_set,
    );

    // Read derivative state for prev_bar_state snapshot.
    let prev_fund_f = latest_funding.read().await.and_then(|f| f.to_f64());

    // ── Save current bar's indicator values for next bar's cross-over detection ──
    *prev_bar_state = PreviousBarState {
        rsi: final_rsi.map(|d| d.to_f64().unwrap_or(0.0)),
        stoch_k: final_stoch
            .as_ref()
            .map(|s| s.k_value.to_f64().unwrap_or(0.0)),
        stoch_d: final_stoch
            .as_ref()
            .map(|s| s.d_value.to_f64().unwrap_or(0.0)),
        cmf: final_cmf.map(|d| d.to_f64().unwrap_or(0.0)),
        chandemo: final_cmo.map(|d| d.to_f64().unwrap_or(0.0)),
        aroon_up: final_aroon.as_ref().map(|a| a.up.to_f64().unwrap_or(0.0)),
        aroon_down: final_aroon.as_ref().map(|a| a.down.to_f64().unwrap_or(0.0)),
        macd_line: Some(final_macd.macd_line.to_f64().unwrap_or(0.0)),
        macd_histogram: Some(final_macd.histogram.to_f64().unwrap_or(0.0)),
        linreg_slope: final_linreg,
        zscore: final_zscore,
        obv: final_obv.as_ref().map(|o| o.obv.to_f64().unwrap_or(0.0)),
        obv_sma: final_obv
            .as_ref()
            .map(|o| o.obv_sma.to_f64().unwrap_or(0.0)),
        mfi: final_mfi.map(|d| d.to_f64().unwrap_or(0.0)),
        adx_plus_di: final_adx
            .as_ref()
            .map(|a| a.plus_di.to_f64().unwrap_or(0.0)),
        adx_minus_di: final_adx
            .as_ref()
            .map(|a| a.minus_di.to_f64().unwrap_or(0.0)),
        price: Some(close_f),
        ema_fast: Some(final_ema_fast.to_f64().unwrap_or(0.0)),
        ema_medium: Some(final_ema_medium.to_f64().unwrap_or(0.0)),
        // AUDIT-AIU-030: carried for the StackChange detector.
        ema_slow: Some(final_ema_slow.to_f64().unwrap_or(0.0)),
        ema_long: Some(final_ema_long.to_f64().unwrap_or(0.0)),
        supertrend_line: final_supertrend
            .as_ref()
            .map(|s| s.line.to_f64().unwrap_or(0.0)),
        // Populated in later phases (Pivots: P2, Ichimoku: P4).
        pivot_active_level: pivot_levels.map(|lv| {
            let p = lv.pivot.to_f64().unwrap_or(0.0);
            let c = close_f;
            if c >= p {
                1.0
            } else {
                -1.0
            }
        }),
        ichimoku_tenkan: ichimoku_reading.map(|r| r.tenkan.to_f64().unwrap_or(0.0)),
        ichimoku_kijun: ichimoku_reading.map(|r| r.kijun.to_f64().unwrap_or(0.0)),
        price_vs_cloud: ichimoku_reading.map(|r| {
            let top = r
                .senkou_a_current
                .to_f64()
                .unwrap_or(0.0)
                .max(r.senkou_b_current.to_f64().unwrap_or(0.0));
            let bot = r
                .senkou_a_current
                .to_f64()
                .unwrap_or(0.0)
                .min(r.senkou_b_current.to_f64().unwrap_or(0.0));
            let px = close_f;
            if px > top {
                1.0
            } else if px < bot {
                -1.0
            } else {
                0.0
            }
        }),
        ichimoku_future_bias: ichimoku_reading
            .map(|r| (r.senkou_a - r.senkou_b).to_f64().unwrap_or(0.0).signum()),
        hull_ma: hma_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        awesome_oscillator: ao_reading.map(|d| d.value.to_f64().unwrap_or(0.0)),
        force_index: fi_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        williams_r: wr_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        cci: cci_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        psar_sar: psar_reading.map(|d| d.sar.to_f64().unwrap_or(0.0)),
        funding_rate: prev_fund_f,
        cascade_state: Some(*last_cascade_state),
    };

    // Stamp signal freshness (age in completed bars).
    let mut indicators = indicators;
    *live_bar = live_bar.wrapping_add(1);
    stamp_signal_ages(&mut indicators, &mut *signal_age_tracker, *live_bar);
    // v9: the strategy's `l1.signals` knobs — per-SignalKind confidence
    // boost, stale-signal drop, strength-bucket classification.
    apply_l1_signal_params(&mut indicators, &strategy.l1);

    // Inject Derivatives Data indicators (OI & Funding Rate).
    // Reads from the per-pair shared `oi_history` and
    // `funding_history` Arc locks, which the bootstrap
    // path pre-seeds from historical snapshots and live
    // WS events keep mutating. `candle_close_sec` is the
    // window anchor for the 3600 s OI delta.
    let deriv = read_derivative_snapshot_state(
        latest_oi,
        latest_funding,
        latest_mark_px,
        latest_index_px,
        oi_history,
        funding_history,
        candle_close_sec,
    )
    .await;
    let DerivativeSnapshot {
        oi: oi_f,
        funding: fund_f,
        mark_px: mark_f,
        index_px: _idx_f,
        spread_pct,
        oi_delta: oi_delta_f,
        prev_oi_delta: prev_oi_delta_f,
    } = deriv;
    // AUDIT-AIU-074: unify the funding "extreme" definition
    // between the indicator and liquidity layers.
    let funding_extreme_pct = liquidity_config
        .as_ref()
        .map(|c| c.funding_extreme_pct)
        .unwrap_or(0.0005);
    inject_derivatives_indicators(
        &mut indicators,
        oi_f,
        fund_f,
        oi_delta_f,
        mark_f,
        spread_pct,
        prev_oi_delta_f,
        funding_extreme_pct,
    );

    // Inject order book depth analysis indicators
    inject_orderbook_indicators(
        &mut indicators,
        order_book_analysis,
        spread_wide_threshold_pct,
    );
    // AUDIT-H2: the order-book injection above can attach wall signals to
    // `order_flow_imbalance` after `build_indicator_map` ran its denylist
    // filter — re-apply so no disabled kind/pair reaches the wire here.
    active_set.filter_map_signals(&mut indicators);

    // Compute quantitative decision-support context.
    let atr_val = indicators.get("atr").map(|v| v.raw_value).unwrap_or(0.0);

    // Compute Statistical Intelligence Layer enrichment.
    let rsi_val = indicators.get("rsi").map(|v| v.raw_value).unwrap_or(50.0);
    let bbwp_val = indicators.get("bbwp").map(|v| v.raw_value).unwrap_or(50.0);
    let sqz_mom = indicators
        .get("squeeze")
        .and_then(|v| v.values.as_ref())
        .and_then(|vals| vals.get("momentum").copied())
        .unwrap_or(0.0);
    let sqz_on = indicators
        .get("squeeze")
        .map(|v| v.state_label == "COMPRESSION_COILING")
        .unwrap_or(false);
    let rvol_f = rvol.and_then(|r| r.to_f64()).unwrap_or(1.0);
    let adx_val = indicators.get("adx").map(|v| v.raw_value).unwrap_or(25.0);

    let current_context =
        crate::market_context_synth::synthesize_market_context(&indicators, Some(&strategy.l1));

    let current_state = derive_pipeline_state_with_staleness(
        (*bar_count) as usize,
        buffer_size,
        last_completed_ms,
        now_ms,
        stale_threshold_secs,
    );
    // PRI-05 (v6.10.7): the uniform live floor lives in
    // `derive_pipeline_state` (max(buffer_size/10, 50) bars) — the same
    // gate now applies to every timeframe, so the pipeline-state badge,
    // the matrix payload gate, and the indicator lifecycle always agree.
    let pipeline_is_live = current_state == CandlePipelineState::Live;

    let this_snapshot_for_synth = MarketSnapshot {
        timeframe_slot: Some(slot),
        exchange: shadow_exchange,
        timeframe_secs,
        timestamp: candle_close_sec,
        symbol: symbol.to_string(),
        is_completed: Some(true),
        mid_price: if ob_fresh && shadow_bid > Decimal::ZERO && shadow_ask > Decimal::ZERO {
            (shadow_bid + shadow_ask) / Decimal::from(2)
        } else {
            completed.close
        },
        bid_price: shadow_bid,
        ask_price: shadow_ask,
        bid_size: if ob_fresh {
            order_book_analysis
                .best_bid_size()
                .and_then(Decimal::from_f64_retain)
        } else {
            None
        },
        ask_size: if ob_fresh {
            order_book_analysis
                .best_ask_size()
                .and_then(Decimal::from_f64_retain)
        } else {
            None
        },
        funding_rate: fund_f.and_then(Decimal::from_f64_retain),
        open_interest: oi_f.and_then(Decimal::from_f64_retain),
        oi_delta_1h: oi_delta_f.and_then(Decimal::from_f64_retain),
        mark_price: *latest_mark_px.read().await,
        index_price: *latest_index_px.read().await,
        mark_index_spread_pct: spread_pct,
        prev_day_px: shadow_prev_day_px,
        open: Some(completed.open),
        high: Some(completed.high),
        low: Some(completed.low),
        close: Some(completed.close),
        volume: Some(completed.volume),
        average_volume: avg_vol,
        pipeline_state: current_state,
        indicator_lifecycle: build_indicator_lifecycle_map(
            &indicators.clone(),
            &*indicator_lifecycle.read().await,
            stale_threshold_secs,
            *bar_count,
            *real_bar_count,
            false,
            core_domain::LatencyTracker::now_ms(),
            pipeline_is_live,
        ),
        context: Some(current_context.clone()),
        decision_context: None,
        statistical_context: None,
        indicators: indicators.clone(),
        alignment: None,
        risk: None,
        analysis: None,
        advisory: None,
        opportunity: None,
        liquidity_signals: vec![],
        metrics_config: None,
        risk_profile: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        quality_envelope: Some(quality_envelope.clone()),
    };

    let mut cross_tf_snaps: Vec<(u64, MarketSnapshot)> =
        Vec::with_capacity(1 + cross_tf_snapshots.len());
    cross_tf_snaps.push((timeframe_secs, this_snapshot_for_synth));
    for arc in cross_tf_snapshots {
        if let Some(s) = arc.read().await.clone() {
            if !cross_tf_snaps
                .iter()
                .any(|(_, existing)| existing.timeframe_secs == s.timeframe_secs)
            {
                cross_tf_snaps.push((s.timeframe_secs, s));
            }
        }
    }

    // D4 (cross-TF freshness): the other pipelines publish
    // their `latest_snapshot` handles at THEIR OWN candle
    // closes. When this TF's close lands on a boundary that
    // a slower TF shares (e.g. a micro close at 02:48:00
    // coincides with the fast 180s close), the slower
    // pipeline may not have written its just-closed snapshot
    // yet — the cross-TF synthesis then consumes the
    // PREVIOUS close, so the alignment/analysis/risk layers
    // lag the per-TF context the dashboards show (observed:
    // ETH FAST alignment row scored -7 with 21 signals while
    // the metrics tab showed +8 / 31 for the same candle).
    // Retry with a short bounded wait until every handle that
    // was due to close at this boundary has advanced.
    // PRI-07 (v6.10.7): the spin budget adapts to the chosen timeframe —
    // `min(duration / 4, 1000 ms)` in 50 ms steps. The old hardcoded
    // 5×50 ms could consume a quarter of a 1 s candle at shared
    // boundaries (e.g. every 3rd second on a 1 s/3 s/5 s/15 s ladder).
    {
        let d4_budget_ms: u64 = ((timeframe_secs * 1000) / 4).min(1000);
        let mut spins = 0u32;
        loop {
            let stale_at_boundary = cross_tf_snaps.iter().any(|(secs, snap)| {
                *secs != timeframe_secs
                    && snap.timeframe_secs > 0
                    && candle_close_sec % snap.timeframe_secs == 0
                    && snap.timestamp < candle_close_sec
            });
            if !stale_at_boundary || spins * 50 >= d4_budget_ms as u32 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            spins += 1;
            for arc in cross_tf_snapshots {
                if let Some(s) = arc.read().await.clone() {
                    let idx = cross_tf_snaps
                        .iter()
                        .position(|(_, existing)| existing.timeframe_secs == s.timeframe_secs);
                    match idx {
                        Some(i) => cross_tf_snaps[i].1 = s,
                        None => cross_tf_snaps.push((s.timeframe_secs, s)),
                    }
                }
            }
        }
    }

    let cross_refs: Vec<(u64, &MarketSnapshot)> =
        cross_tf_snaps.iter().map(|(secs, s)| (*secs, s)).collect();

    let cluster_guard = cluster_matrix.read().await.clone();
    // Block B: feed the latest mid into the accumulator
    // before flushing so the buckets are anchored to the
    // most recent mark. `latest_mark_px` is the upstream
    // shared state updated by `MarkPrice` events.
    let mid_for_buckets = latest_mark_px.read().await.and_then(|m| m.to_f64());
    if let Some(m) = mid_for_buckets {
        liquidity_acc.set_mid(m);
    }
    let flush_now_ms = core_domain::LatencyTracker::now_ms();
    let liquidity_flow = liquidity_acc.flush_to_flow(flush_now_ms);
    *last_cascade_state = liquidity_flow.cascade_state;
    // Thread configured signal thresholds from
    // `[workspace.liquidity]` instead of using the
    // hardcoded defaults inside `SignalInput::default()`.
    let funding_extreme_pct = liquidity_config
        .as_ref()
        .map(|c| c.funding_extreme_pct)
        .unwrap_or(0.0005);
    let magnet_activation_distance_pct = liquidity_config
        .as_ref()
        .map(|c| c.magnet_activation_distance_pct)
        .unwrap_or(0.5);
    let oi_funding_divergence_pct = liquidity_config
        .as_ref()
        .map(|c| c.oi_funding_divergence_pct)
        .unwrap_or(2.0);
    // Vacuum depth band: low/high are the configured
    // threshold and its reciprocal. Legacy hardcoded
    // `0.5 / 2.0` is the `threshold = 0.5` case.
    let (vacuum_low, vacuum_high) = liquidity_config
        .as_ref()
        .map(|c| {
            let t = c.liquidity_vacuum_threshold.max(0.01);
            (t, 1.0 / t)
        })
        .unwrap_or((0.5, 2.0));
    let liquidity_signals =
        core_domain::liquidity::derive_liquidity_signals(&core_domain::liquidity::SignalInput {
            flow: Some(&liquidity_flow),
            cluster: cluster_guard.as_ref(),
            funding_rate: fund_f.unwrap_or(0.0),
            oi_delta_1h_pct: oi_delta_f
                .map(|d| {
                    if oi_f.unwrap_or(1.0).max(1.0).abs() > 1e-9 {
                        d / oi_f.unwrap_or(1.0).max(1.0) * 100.0
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            price_bias: indicators
                .get("ema_stack")
                .map(|v| v.normalized)
                .unwrap_or(0.0),
            prev_funding_rate: prev_bar_state.funding_rate,
            prev_cascade_state: prev_bar_state.cascade_state,
            funding_extreme_pct,
            magnet_activation_distance_pct,
            oi_funding_divergence_pct,
            liquidity_vacuum_depth_low: vacuum_low,
            liquidity_vacuum_depth_high: vacuum_high,
            book_depth_ratio: indicators
                .get("depth_bias")
                .map(|v| v.raw_value)
                .filter(|v| v.is_finite() && *v > 0.0),
            // AUDIT-AIU-055/057: thread the configured
            // cluster-notional floor and per-signal
            // confidences.
            min_cluster_notional_usd: liquidity_config
                .as_ref()
                .map(|c| c.min_cluster_notional_usd)
                .unwrap_or(100_000.0),
            thresholds: core_domain::liquidity::SignalThresholds {
                sustained_events_this_bar: strategy.l2_5.signals.sustained_events_this_bar,
                vacuum_dense_events: strategy.l2_5.signals.vacuum_dense_events,
                vacuum_dense_usd: strategy.l2_5.signals.vacuum_dense_usd,
                funding_extreme_strength_slope: strategy
                    .l2_5
                    .signals
                    .funding_extreme_strength_slope,
            },
            signal_confidences: liquidity_config
                .as_ref()
                .map(|c| {
                    let sc = &c.signal_confidences;
                    core_domain::liquidity::SignalConfidences {
                        cascade_detected: sc.cascade_detected,
                        cascade_sustained: sc.cascade_sustained,
                        cascade_exhausted: sc.cascade_exhausted,
                        funding_extreme: sc.funding_extreme,
                        oi_funding_divergence: sc.oi_funding_divergence,
                        liquidity_vacuum: sc.liquidity_vacuum,
                        funding_flip: sc.funding_flip,
                        oi_price_divergence: sc.oi_price_divergence,
                    }
                })
                .unwrap_or_default(),
        });

    // CA-15 (audit fix): the `[activation]` sub-toggles were parsed but
    // never applied at runtime. Master `enabled=false` ⇒ L1.5/L2.5/Phase-3
    // absent from the snapshot (`liquidity`/`cluster`/`liquidity_signals`
    // → None/empty); `liquidity_signals_enabled=false` ⇒ the LiquidityFlow
    // and cluster stay present but LiquiditySignal emission is suppressed.
    let liquidity_active = active_set.liquidity_enabled;
    let emitted_liquidity_signals: Vec<core_domain::liquidity::LiquiditySignal> =
        if liquidity_active && active_set.liquidity_signals_enabled {
            liquidity_signals
        } else {
            Vec::new()
        };

    let mut opportunity_params = crate::synthesis::OpportunityParams::from_strategy(&strategy.l4);
    opportunity_params.viability_min_net_rr = strategy.tae.intake.min_net_rr;
    opportunity_params.signal_weights = strategy.l1_5.signal_weights.clone();
    opportunity_params.ladder_roles = strategy.ladder_roles.clone();
    let decision_params = crate::strategy_params::decision_params_from_strategy(&strategy.l6);
    let analysis_params = crate::strategy_params::analysis_params_from_strategy(&strategy.l3);
    let alignment_params = crate::strategy_params::alignment_params_from_strategy(&strategy.l2);
    let mut risk_params = crate::strategy_params::risk_params_from_strategy(&strategy.l5);
    risk_params.signal_weights = strategy.l1_5.signal_weights.clone();
    let synthesis = crate::synthesis::synthesize_cross_tf(
        symbol,
        &cross_refs,
        Some(&liquidity_flow),
        cluster_guard.as_ref(),
        &emitted_liquidity_signals,
        *prev_mtf_score,
        *prev_regime,
        *prev_volume_dim,
        *prev_bias,
        // v9: the strategy's L4 opportunity params.
        &opportunity_params,
        // v9: the shared L6 DecisionParams.
        &decision_params,
        // v9: the strategy's L3 params.
        &analysis_params,
        // v9: the strategy's L2 params.
        &alignment_params,
        // v9: the strategy's L5 params.
        &risk_params,
    );

    *prev_mtf_score = Some(synthesis.alignment.mtf_overall_score);
    *prev_regime = Some(synthesis.analysis.market_regime);
    *prev_volume_dim = synthesis.alignment.dimensions.get(2).map(|d| d.score);
    *prev_bias = Some(synthesis.analysis.bias);

    let confluence_score = {
        // v6.10 (Phase 6 / F1): Unsigned 3-factor quality
        // blend in [0, 100]. All three inputs are categorical
        // quality scores in [0, 100]. The signed direction
        // (`sign × magnitude × 100`) was a v6.9 deviation that
        // turned confluence into a signed [-100, +100] range,
        // contradicting the spec at
        // `02-04-decision-matrix.md §2.3` which mandates
        // unsigned [0, 100]. The direction now lives
        // separately in `Decision.bias` (Phase 6 / F2 mirrors
        // `Analysis.bias` via ±40 thresholds).
        // v9 F-05: the blend weights come from the shared
        // `DecisionParams` (single source with the BTE runner).
        let [w_l2, w_l3, w_l4] = decision_params.confluence_weights;
        let tradability_dim = synthesis
            .alignment
            .dimensions
            .get(9)
            .map(|d| d.score)
            .unwrap_or(0.0);
        let market_quality_score = synthesis.analysis.market_quality_score;
        let opp_score = synthesis
            .opportunity
            .as_ref()
            .map(|o| o.opportunity_score)
            .unwrap_or(0.0);
        (w_l2 * tradability_dim + w_l3 * market_quality_score + w_l4 * opp_score).clamp(0.0, 100.0)
    };

    let l4_opportunity = synthesis.opportunity.clone();

    let dec_ctx = core_domain::decision_context::DecisionContext::compute(
        &indicators,
        close_f,
        atr_val,
        confluence_score,
        &synthesis.analysis,
        l4_opportunity.as_ref(),
        &synthesis.risk,
        // v9: the shared strategy-derived DecisionParams.
        &decision_params,
        &analysis_params,
    );
    let sil_ctx = sil_engine.advance_ext(
        close_f,
        atr_val,
        rsi_val,
        bbwp_val,
        sqz_mom,
        volume_f,
        rvol_f,
        adx_val,
        *prev_sil_close,
        sqz_on,
        indicators.get("macd").map(|v| v.raw_value).unwrap_or(0.0),
        indicators.get("obv").map(|v| v.raw_value).unwrap_or(0.0),
        indicators
            .get("stochastic")
            .and_then(|v| v.values.as_ref())
            .and_then(|vals| vals.get("k_line").copied())
            .unwrap_or(50.0),
        indicators
            .get("choppiness")
            .map(|v| v.raw_value)
            .unwrap_or(50.0),
        indicators
            .get("ema_stack")
            .and_then(|v| v.values.as_ref())
            .and_then(|vals| vals.get("medium").copied())
            .unwrap_or(close_f),
    );
    *prev_sil_close = close_f;

    // PRI-07 (v6.10.7): SIL Monte Carlo cadence is wall-time consistent —
    // roughly every 10 minutes of candle time on every timeframe (10 candles
    // at 60 s, 600 at 1 s, 1 at 900 s). The old hardcoded `% 10` ran it
    // every 10 s on a 1 s TF but only every 2.5 h on a 900 s TF.
    *mc_counter += 1;
    let mc_interval_candles: u64 = (600 / timeframe_secs.max(1)).max(1);
    if *mc_counter % mc_interval_candles == 0 {
        sil_engine.run_monte_carlo(close_f, atr_val);
    }

    let completed_snapshot = MarketSnapshot {
        timeframe_slot: Some(slot),
        exchange: shadow_exchange,
        timeframe_secs,
        timestamp: candle_close_sec,
        symbol: symbol.to_string(),
        is_completed: Some(true),
        mid_price: if ob_fresh && shadow_bid > Decimal::ZERO && shadow_ask > Decimal::ZERO {
            (shadow_bid + shadow_ask) / Decimal::from(2)
        } else {
            completed.close
        },
        bid_price: shadow_bid,
        ask_price: shadow_ask,
        bid_size: if ob_fresh {
            order_book_analysis
                .best_bid_size()
                .and_then(Decimal::from_f64_retain)
        } else {
            None
        },
        ask_size: if ob_fresh {
            order_book_analysis
                .best_ask_size()
                .and_then(Decimal::from_f64_retain)
        } else {
            None
        },
        funding_rate: fund_f.and_then(Decimal::from_f64_retain),
        open_interest: oi_f.and_then(Decimal::from_f64_retain),
        oi_delta_1h: oi_delta_f.and_then(Decimal::from_f64_retain),
        mark_price: *latest_mark_px.read().await,
        index_price: *latest_index_px.read().await,
        mark_index_spread_pct: spread_pct,
        prev_day_px: shadow_prev_day_px,
        open: Some(completed.open),
        high: Some(completed.high),
        low: Some(completed.low),
        close: Some(completed.close),
        volume: Some(completed.volume),
        average_volume: avg_vol,
        pipeline_state: current_state,
        indicator_lifecycle: build_indicator_lifecycle_map(
            &indicators,
            &*indicator_lifecycle.read().await,
            stale_threshold_secs,
            *bar_count,
            *real_bar_count,
            false,
            core_domain::LatencyTracker::now_ms(),
            pipeline_is_live,
        ),
        context: Some(current_context),
        decision_context: Some(dec_ctx),
        statistical_context: Some(sil_ctx),
        indicators,
        alignment: Some(synthesis.alignment),
        risk: Some(synthesis.risk),
        analysis: Some(synthesis.analysis),
        advisory: Some(synthesis.advisory.clone()),
        opportunity: synthesis.opportunity,
        risk_profile: None,
        // CA-15: master `[liquidity] enabled = false` ⇒ the liquidity
        // payloads are absent from the snapshot (disabled ≡ absent).
        liquidity: if liquidity_active {
            Some(liquidity_flow)
        } else {
            None
        },
        cluster: if liquidity_active {
            cluster_matrix.read().await.clone()
        } else {
            None
        },
        volume_profile: volume_profile_snapshot,
        liquidity_signals: emitted_liquidity_signals,
        metrics_config: active_set.to_metrics_config(),
        quality_envelope: Some(quality_envelope.clone()),
    };

    // M2 (production audit): the awaited send froze the whole pipeline
    // when the SQLite logger stalled — drop instead of blocking.
    send_telemetry(
        telemetry_tx,
        database_storage::TelemetryMsg::InsertSnapshot(Box::new(completed_snapshot.clone())),
    );

    latency_tracker.record_observation_latency(
        core_domain::LatencyTracker::now_ms().saturating_sub(completed.start_time_ms),
    );

    // Always broadcast completed candle snapshots so the
    // chart time-series stays continuous from the first bar.
    // The matrix payload (alignment/analysis/risk/advisory/
    // opportunity/decision_context) is still gated on
    // pipeline_is_live but the OHLCV candle, liquidity,
    // cluster, volume_profile, and indicators land on every
    // frame so sub-minute TFs (1s/3s/5s/15s) populate the
    // chart immediately rather than stalling for 50 bars
    // (~50 s at 1 s TF).
    let mut broadcast_snapshot = completed_snapshot.clone();
    if !pipeline_is_live {
        broadcast_snapshot.alignment = None;
        broadcast_snapshot.analysis = None;
        broadcast_snapshot.risk = None;
        broadcast_snapshot.advisory = None;
        broadcast_snapshot.opportunity = None;
        broadcast_snapshot.decision_context = None;
        broadcast_snapshot.statistical_context = None;
        broadcast_snapshot.context = None;
    }
    let _ = broadcast_tx.send(broadcast_snapshot);

    // Publish the completed snapshot as the latest for this TF.
    {
        let mut snap = latest_snapshot.write().await;
        *snap = Some(completed_snapshot.clone());
    }

    // v6.10 (Phase 2 / B3): write-through the cross-TF
    // advisory onto the pipeline struct. Mirrors the
    // pipeline_state / indicator_lifecycle write-through
    // (those are written via `build_indicator_lifecycle_map`
    // above). Consumers reading `pipeline.advisory` directly
    // see the most recent cross-TF synthesis result.
    *advisory.write().await = Some(synthesis.advisory.clone());

    // v6.10 (Phase 3 / C3): write-through the per-TF
    // pipeline_state and indicator_lifecycle onto the
    // pipeline struct so consumers that read those fields
    // directly see the authoritative state. Before this,
    // both fields were declared on `TimeframePipeline` but
    // never written to (so they remained `Initializing`
    // and empty respectively).
    *pipeline_state_handle.write().await = current_state;
    *indicator_lifecycle.write().await = completed_snapshot.indicator_lifecycle.clone();

    completed_snapshot
}

/// Snapshot of the latest WS / poller state for derivatives + order book
/// telemetry. Shared between the shadow (live-tick) and completed-candle
/// paths so the indicators surface as soon as the upstream source
/// produces data — instead of waiting for the next completed candle
/// close. Without this helper the WARMING-placeholder suppression
/// (Phase 2 of the metrics fix) would leave derivatives / OB rows stuck
/// at `--/--/Loading` for up to one full candle duration after the WS
/// push arrives, which on Bitget is misleading (HL is fine because its
/// poller only ticks every 60 s anyway).
struct DerivativeSnapshot {
    oi: Option<f64>,
    funding: Option<f64>,
    mark_px: Option<f64>,
    /// Captured for completeness; the spread math uses `mark_px` /
    /// `index_px` directly via `spread_pct`.
    index_px: Option<f64>,
    spread_pct: Option<f64>,
    oi_delta: Option<f64>,
    /// Previous bar's OI delta — for the ZeroLineCross transition detector
    /// (AUDIT-AIU-039).
    prev_oi_delta: Option<f64>,
}

/// True 1-hour window in seconds for the OI delta (AUDIT-AIU-051). The
/// previous implementation capped the deque at 60 *samples*, so at a 15 s TF
/// the "1h" delta was really 15 minutes, and at a 5 m TF it was 5 hours.
pub const OI_DELTA_WINDOW_SECS: u64 = 3600;

async fn read_derivative_snapshot_state(
    latest_oi: &Arc<RwLock<Option<Decimal>>>,
    latest_funding: &Arc<RwLock<Option<Decimal>>>,
    latest_mark_px: &Arc<RwLock<Option<Decimal>>>,
    latest_index_px: &Arc<RwLock<Option<Decimal>>>,
    oi_history: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    funding_history: &Arc<RwLock<VecDeque<f64>>>,
    now_secs: u64,
) -> DerivativeSnapshot {
    let oi_f = latest_oi.read().await.and_then(|o| o.to_f64());
    let fund_f = latest_funding.read().await.and_then(|f| f.to_f64());
    let mark_f = latest_mark_px.read().await.and_then(|m| m.to_f64());
    let idx_f = latest_index_px.read().await.and_then(|i| i.to_f64());
    let spread_pct = match (mark_f, idx_f) {
        (Some(m), Some(i)) if i > 0.0 => Some((m - i) / i * 100.0),
        _ => None,
    };
    // AUDIT-AIU-051: OI history is now `(timestamp_secs, value)` and the
    // window is a TRUE 3600 s time window — samples older than one hour are
    // pruned before the delta is computed, and each TF evaluates the window
    // against its own candle cadence (per-TF deque clone).
    let (oi_delta_f, prev_oi_delta_f) = match oi_f {
        Some(cur) => {
            let mut hist = oi_history.write().await;
            hist.push_back((now_secs, cur));
            let cutoff = now_secs.saturating_sub(OI_DELTA_WINDOW_SECS);
            while hist.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
                hist.pop_front();
            }
            // Prune stale entries at the tail that could arrive out of
            // order (WS push after a clock-sync edge case).
            while hist.back().map(|(t, _)| *t > now_secs).unwrap_or(false) {
                hist.pop_back();
            }
            let n = hist.len();
            if n >= 2 {
                // delta = current value minus the oldest value inside the
                // 3600 s window.
                let delta = cur - hist.front().copied().unwrap_or((now_secs, cur)).1;
                // Previous bar's delta = second-newest sample vs the same
                // anchor (for the zero-line transition detector).
                let prev_delta = if n >= 3 {
                    let prev_val = hist.get(n - 2).copied().unwrap_or((now_secs, cur)).1;
                    Some(prev_val - hist.front().copied().unwrap_or((now_secs, cur)).1)
                } else {
                    None
                };
                (Some(delta), prev_delta)
            } else {
                (None, None)
            }
        }
        None => (None, None),
    };

    // Append current funding rate to the shared rolling funding_history
    // (bounded to 8 samples; mirrors warm.rs::FUNDING_HISTORY_MAX). The
    // deque is fed sequentially so future OHLC divergences can compute
    // historical funding-rate deltas for the L2.5 divergence detector.
    if let Some(cur) = fund_f {
        let mut hist = funding_history.write().await;
        hist.push_back(cur);
        if hist.len() > 8 {
            hist.pop_front();
        }
    }

    DerivativeSnapshot {
        oi: oi_f,
        funding: fund_f,
        mark_px: mark_f,
        index_px: idx_f,
        spread_pct,
        oi_delta: oi_delta_f,
        prev_oi_delta: prev_oi_delta_f,
    }
}

/// Inject Derivatives Data (OI & Funding) normalized indicator entries into
/// the snapshot indicator map. Called after the main indicator map is
/// built. Public for testability — the integration tests in
/// `crates/market-analyzer/tests/integration/` exercise this helper
/// directly with synthetic WS event payloads to verify HL and Bitget
/// produce identical indicator map shapes.
pub fn inject_derivatives_indicators(
    indicators: &mut HashMap<String, NormalizedIndicatorValue>,
    oi: Option<f64>,
    funding: Option<f64>,
    oi_delta: Option<f64>,
    mark_px: Option<f64>,
    spread_pct: Option<f64>,
    prev_oi_delta: Option<f64>,
    // AUDIT-AIU-074: unified funding "extreme" threshold from config.
    funding_extreme_pct: f64,
) {
    use crate::indicators::normalized::derivatives;

    // Open Interest
    if let Some(o) = oi {
        indicators.insert(
            "open_interest".into(),
            derivatives::normalize_open_interest(o),
        );
    }

    // OI Delta (1h change)
    if let Some(delta) = oi_delta {
        indicators.insert(
            "oi_delta".into(),
            derivatives::normalize_oi_delta(delta, prev_oi_delta),
        );
    }

    // Funding Rate (non-directional gate)
    if let Some(f) = funding {
        indicators.insert(
            "funding_rate".into(),
            derivatives::normalize_funding_rate(f, funding_extreme_pct),
        );
    }

    // OI-Price Divergence
    if let (Some(_o), Some(delta)) = (oi, oi_delta) {
        let ema_bias = indicators
            .get("ema_stack")
            .map(|v| v.normalized)
            .unwrap_or(0.0);
        indicators.insert(
            "oi_price_divergence".into(),
            derivatives::normalize_oi_price_divergence(delta, ema_bias),
        );
    }

    // Mark-Index Spread (Phase 0: derivatives telemetry activation).
    // Positive spread = mark premium (perp trades above index, bullish bias).
    // Negative spread = perp discount (bearish bias). Wide spread signals
    // market stress and is a leading indicator of forced liquidations.
    if let Some(spread) = spread_pct {
        indicators.insert(
            "mark_index_spread".into(),
            derivatives::normalize_mark_index_spread(spread, mark_px),
        );
    }
}

/// Inject Order Book Depth Analysis normalized indicator entries into the
/// snapshot indicator map. Called after the main indicator map is built.
/// Public for testability (see `inject_derivatives_indicators`).
pub fn inject_orderbook_indicators(
    indicators: &mut HashMap<String, NormalizedIndicatorValue>,
    ob: &OrderBookAnalysis,
    spread_wide_threshold_pct: f64,
) {
    use crate::indicators::normalized::derivatives;

    // Order Flow Imbalance
    if let Some(ofi) = ob.order_flow_imbalance() {
        indicators.insert(
            "order_flow_imbalance".into(),
            derivatives::normalize_order_flow_imbalance(ofi),
        );
    }

    // Spread (non-directional gate)
    // AUDIT-AIU-001: `spread_pct` is ALREADY a percentage (order_book.rs
    // computes (ask-bid)/mid*100). The previous `* 100.0` double-scaled the
    // value, making a real 0.015% spread render as 1.5% and firing
    // SPREAD_WIDE on every snapshot, which permanently inflated the
    // execution-risk dimensions in the Risk Matrix (core-domain/risk.rs).
    if let Some(spread) = ob.spread_pct() {
        indicators.insert(
            "spread".into(),
            derivatives::normalize_spread(spread, spread_wide_threshold_pct),
        );
    }

    // Depth Bias (bid depth / ask depth ratio)
    if let Some(ratio) = ob.depth_imbalance_ratio(1.0) {
        if ratio.is_finite() {
            indicators.insert(
                "depth_bias".into(),
                derivatives::normalize_depth_bias(ratio),
            );
        }
    }

    // Wall signals: attach to order_flow_imbalance entry if it exists
    if let Some(ref wall) = ob.wall_detected() {
        use crate::indicators::normalized::{
            IndicatorSignal, SignalDirection, SignalKind, SignalStatus,
        };
        match wall.as_str() {
            "BID_WALL" => {
                if let Some(ofier) = indicators.get_mut("order_flow_imbalance") {
                    ofier.signals.push(IndicatorSignal {
                        kind: SignalKind::Threshold,
                        direction: SignalDirection::Bullish,
                        status: SignalStatus::Active,
                        label: "BID_WALL".to_string(),
                        strength: 0.8,
                        age_bars: 0,
                        strength_label: "STRONG".to_string(),
                        points: None,
                    });
                }
            }
            "ASK_WALL" => {
                if let Some(ofier) = indicators.get_mut("order_flow_imbalance") {
                    ofier.signals.push(IndicatorSignal {
                        kind: SignalKind::Threshold,
                        direction: SignalDirection::Bearish,
                        status: SignalStatus::Active,
                        label: "ASK_WALL".to_string(),
                        strength: 0.8,
                        age_bars: 0,
                        strength_label: "STRONG".to_string(),
                        points: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Derive current support/resistance levels from swing pivots, updating the
/// role-reversal tracker with the latest levels and candle close. Swing highs
/// act as resistance, swing lows as support; the tracker flips a level's role
/// when a candle closes decisively beyond it. Returns the current role-adjusted
/// `(support_levels, resistance_levels)` for normalization.
pub(crate) fn update_sr_levels(
    tracker: &mut SrRoleTracker,
    pivots: &[crate::indicators::PivotPoint],
    close: Decimal,
    timestamp_sec: u64,
) -> (Vec<f64>, Vec<f64>) {
    let mut raw_sup: Vec<f64> = Vec::new();
    let mut raw_res: Vec<f64> = Vec::new();
    for p in pivots {
        let price = p.price.to_f64().unwrap_or(0.0);
        if price <= 0.0 {
            continue;
        }
        match p.pivot_type {
            crate::indicators::PivotType::High => raw_res.push(price),
            crate::indicators::PivotType::Low => raw_sup.push(price),
        }
    }
    tracker.register_levels(&raw_sup, &raw_res);
    let _ = tracker.process_candle_close(close.to_f64().unwrap_or(0.0), timestamp_sec);
    (tracker.get_supports(), tracker.get_resistances())
}

/// Build a `VolumeProfileSnapshot` from the indicator output and the bin-level
/// aggregates returned by `VolumeProfile::compute_bins()` (and the matching
/// reading from `compute()`). Returns `None` when the indicator has not yet
/// accumulated enough bars to produce a profile. The strict `window_size / 2`
/// gate lives inside `VolumeProfile::{compute, compute_bins}`; the seeded
/// (warm-up) path bypasses it via the `*_with_min_bars(25)` variants so
/// sub-minute TFs still produce a profile from whatever history the venue
/// actually delivered (typically 26–51 bars for 15 s / 30 s).
///
/// `pub(super)` because both the live per-candle path (in this module) and the
/// warm-up per-candle path (in `super::warm`) build snapshots from the same
/// source-of-truth function, so warm-up snapshots stay in full parity with
/// live snapshots and `/api/history` returns the bin-level profile on first
/// mount without waiting for the first live candle close.
pub(super) fn build_volume_profile_snapshot(
    symbol: &str,
    slot: TimeframeSlot,
    timeframe_secs: u64,
    reading: &Option<crate::indicators::VolumeProfileOutput>,
    bins: Option<&Vec<crate::indicators::volume_profile::BinAggregate>>,
    candle_start_time_ms: u64,
    // AUDIT-AIU-045: the value-area target was hardcoded at 0.70 while the
    // profile's `compute()` honored the configurable
    // `volume_profile_value_area` — the two desynced when operators changed
    // the config, so the chart overlay's VAH/VAL disagreed with the POC
    // summary. Thread the configured value through.
    value_area_pct: f64,
) -> Option<VolumeProfileSnapshot> {
    let reading = reading.as_ref()?;
    let bins = bins?;
    if bins.is_empty() {
        return None;
    }
    let d2f = |d: Decimal| d.to_f64().unwrap_or(0.0);

    let mut out_bins: Vec<VolumeProfileBin> = Vec::with_capacity(bins.len());
    let mut range_low = f64::INFINITY;
    let mut range_high = f64::NEG_INFINITY;
    let mut total_volume = 0.0;
    for b in bins {
        let pl = d2f(b.price_low);
        let ph = d2f(b.price_high);
        let v = d2f(b.total);
        let buy = d2f(b.buy);
        let sell = d2f(b.sell);
        if v <= 0.0 {
            continue;
        }
        range_low = range_low.min(pl);
        range_high = range_high.max(ph);
        total_volume += v;
        out_bins.push(VolumeProfileBin {
            price_low: pl,
            price_high: ph,
            volume: v,
            buy_volume: buy,
            sell_volume: sell,
            is_poc: false,
            is_value_area: false,
        });
    }
    if out_bins.is_empty() {
        return None;
    }

    // Identify POC (highest-volume bin) and value-area bounds using the same
    // algorithm as `VolumeProfile::compute`.
    let mut poc_idx = 0usize;
    let mut max_vol = 0.0;
    for (i, b) in out_bins.iter().enumerate() {
        if b.volume > max_vol {
            max_vol = b.volume;
            poc_idx = i;
        }
    }
    out_bins[poc_idx].is_poc = true;
    let target_vol = total_volume * value_area_pct.clamp(0.05, 0.95);
    let mut lo = poc_idx;
    let mut hi = poc_idx;
    let mut va_vol = out_bins[poc_idx].volume;
    let n = out_bins.len();
    while va_vol < target_vol && (lo > 0 || hi + 1 < n) {
        if lo == 0 || (hi + 1 < n && out_bins[lo - 1].volume < out_bins[hi + 1].volume) {
            hi += 1;
            va_vol += out_bins[hi].volume;
        } else {
            lo -= 1;
            va_vol += out_bins[lo].volume;
        }
    }
    for b in &mut out_bins[lo..=hi] {
        b.is_value_area = true;
    }
    let value_area_high = out_bins[hi].price_high;
    let value_area_low = out_bins[lo].price_low;
    let poc_price = d2f(reading.poc);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(candle_start_time_ms);

    Some(VolumeProfileSnapshot {
        symbol: symbol.to_string(),
        // Canonical slot string (`micro`/`fast`/`slow`/`macro`/`custom-N`,
        // same as the WS envelope and TimeframeSlot::as_str) — the old
        // `format!("{:?}", slot)` produced `"custom { id: 3 }"` for
        // operator-defined pipelines.
        timeframe_slot: slot.as_str(),
        timeframe_secs,
        bins: out_bins,
        poc_price,
        value_area_high,
        value_area_low,
        total_volume,
        range_low,
        range_high,
        num_bins: n,
        timestamp_ms: now_ms,
    })
}

/// Stamp `age_bars` on every signal using a persistent tracker keyed by
/// `<indicator>:<kind>`. A signal resets to age 0 when it first appears or flips
/// direction; otherwise its age is the number of completed bars since first seen.
fn stamp_signal_ages(
    map: &mut std::collections::HashMap<String, crate::indicators::NormalizedIndicatorValue>,
    tracker: &mut std::collections::HashMap<String, (u32, crate::indicators::SignalDirection)>,
    bar: u32,
) {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (key, entry) in map.iter_mut() {
        for sig in entry.signals.iter_mut() {
            let tk = format!("{}:{:?}", key, sig.kind);
            seen.insert(tk.clone());
            match tracker.get(&tk) {
                Some((first, dir)) if *dir == sig.direction => {
                    sig.age_bars = bar.saturating_sub(*first);
                }
                _ => {
                    tracker.insert(tk, (bar, sig.direction));
                    sig.age_bars = 0;
                }
            }
        }
    }
    // Evict trackers whose signal no longer fires so a re-appearance is "fresh".
    tracker.retain(|k, _| seen.contains(k));
}

/// v9 L1 signal post-pass: apply the strategy's `l1.signals` knobs after the
/// normalizer + age tracker have run —
///   - `max_age_bars`: drop signals older than N bars (None = keep forever);
///   - `confidence_boost`: per-SignalKind multiplier on the entry's
///     confidence (keys = the Rust `SignalKind` names, e.g. `"Crossover"`);
///   - `strength_buckets`: `[weak, strong, extreme]` borders on
///     `|normalized|`-derived strength → `strength_label` WEAK / MODERATE /
///     STRONG / EXTREME.
fn apply_l1_signal_params(
    map: &mut std::collections::HashMap<String, crate::indicators::NormalizedIndicatorValue>,
    l1: &config_models::L1Params,
) {
    let [b_weak, b_strong, b_extreme] = l1.signals.strength_buckets;
    let max_age = l1.signals.max_age_bars;
    for (key, entry) in map.iter_mut() {
        if max_age.is_some() {
            entry
                .signals
                .retain(|s| max_age.map(|m| s.age_bars <= m).unwrap_or(true));
        }
        let boosts = &l1.signals.confidence_boost;
        let mut confidence_scale: f64 = 1.0;
        for sig in entry.signals.iter_mut() {
            let strength = sig.strength.clamp(0.0, 1.0);
            sig.strength_label = if strength >= b_extreme {
                "EXTREME"
            } else if strength >= b_strong {
                "STRONG"
            } else if strength >= b_weak {
                "MODERATE"
            } else {
                "WEAK"
            }
            .to_string();
            if let Some(boost) = boosts.get(&format!("{:?}", sig.kind)) {
                confidence_scale = confidence_scale.max(*boost);
            }
        }
        if (confidence_scale - 1.0).abs() > 1e-9 {
            entry.confidence = (entry.confidence * confidence_scale).clamp(0.0, 1.0);
        }
        let _ = key;
    }
}

/// Per-candle indicator readings produced by
/// [`apply_candle_to_indicators`]. The struct exists so the same code path
/// drives the trade-triggered completed-candle handler, the stale-check
/// `force_close` path, and the doji-fill path. Every indicator value in
/// here was just computed by the per-pipeline indicator state; no field
/// is read from a sibling TF's state. (See `tests/sub_minute_indicator_cadence.rs`
/// for the regression pins.)
pub(super) struct CandleIndicatorReadings {
    // `open_f` / `high_f` / `low_f` are exposed for callers that need them
    // to build the `MarketSnapshot` (the trade-triggered path uses
    // `close_f` directly). They're computed once and held here so the
    // helper has a single return type.
    #[allow(dead_code)]
    pub open_f: f64,
    #[allow(dead_code)]
    pub high_f: f64,
    #[allow(dead_code)]
    pub low_f: f64,
    pub close_f: f64,
    pub volume_f: f64,
    pub pivot_levels: Option<crate::indicators::PivotLevels>,
    pub candlestick_reading: crate::indicators::CandlestickResult,
    pub ichimoku_reading: Option<crate::indicators::IchimokuOutput>,
    pub cci_reading: Option<Decimal>,
    pub psar_reading: Option<crate::indicators::PsarOutput>,
    pub wr_reading: Option<Decimal>,
    pub hma_reading: Option<Decimal>,
    pub ao_reading: Option<crate::indicators::AoOutput>,
    pub fi_reading: Option<Decimal>,
    /// Rolling mean of |FI| — scale-relative extreme-threshold baseline
    /// (AUDIT-AIU-043).
    pub fi_mean_abs: Option<Decimal>,
    pub sdc_reading: Option<crate::indicators::SdChannelOutput>,
    pub volume_profile_reading: Option<crate::indicators::VolumeProfileOutput>,
    pub volume_profile_snapshot: Option<core_domain::volume_profile::VolumeProfileSnapshot>,
    pub smc_reading: Option<crate::indicators::SmcOutput>,
    pub final_vwap: Option<Decimal>,
    pub avwap_reading: crate::indicators::AvwapOutput,
    pub final_ema_fast: Decimal,
    pub final_ema_medium: Decimal,
    pub final_ema_slow: Decimal,
    pub final_ema_long: Decimal,
    pub ema_stack_state: Option<String>,
    pub final_rsi: Option<Decimal>,
    pub final_macd: crate::indicators::MacdOutput,
    pub final_adx: Option<crate::indicators::AdxOutput>,
    pub final_sqz: Option<crate::indicators::SqueezeOutput>,
    pub final_bb: Option<(Decimal, Decimal, Decimal)>,
    pub final_atr: Option<crate::indicators::AtrOutput>,
    pub final_bbwp: Option<Decimal>,
    pub final_stoch: Option<crate::indicators::StochasticOutput>,
    pub final_cmo: Option<Decimal>,
    pub final_supertrend: Option<crate::indicators::SupertrendOutput>,
    pub final_keltner: Option<crate::indicators::KeltnerOutput>,
    pub final_donchian: Option<crate::indicators::DonchianOutput>,
    pub final_obv: Option<crate::indicators::ObvOutput>,
    pub final_cmf: Option<Decimal>,
    pub final_mfi: Option<Decimal>,
    pub final_hv: Option<f64>,
    pub final_aroon: Option<crate::indicators::AroonOutput>,
    pub final_chop: Option<Decimal>,
    pub final_linreg: Option<f64>,
    pub final_zscore: Option<f64>,
}

/// Apply a candle (real OR reconstructed doji) to **every** stateful
/// indicator in the per-TF pipeline. Returns the freshly-computed
/// readings so the caller can build a `MarketSnapshot` (for live or
/// completed broadcasts) without losing the mutation that lets the next
/// candle compute correctly.
///
/// This function is the single source of truth for "what the indicators
/// become after a candle of duration `duration_ms`". It must be called
/// from every path that closes a candle for this TF — trade-triggered
/// boundary crossing, stale-check `force_close`, and the doji-fill loop —
/// otherwise the EMA (and every other mutating indicator) lags the
/// wall-clock by however many wall-clock seconds elapsed without a
/// candle close. (See `tests/sub_minute_indicator_cadence.rs`.)
#[allow(clippy::too_many_arguments)]
fn apply_candle_to_indicators(
    symbol: &str,
    slot: TimeframeSlot,
    timeframe_secs: u64,
    completed: &NormalizedCandle,
    day_index: u64,
    pivot_points_indicator: &mut PivotPoints,
    candlestick_indicator: &mut Candlestick,
    ichimoku_indicator: &mut Ichimoku,
    cci_indicator: &mut Cci,
    psar_indicator: &mut ParabolicSar,
    wr_indicator: &mut WilliamsR,
    hma_indicator: &mut HullMA,
    ao_indicator: &mut AwesomeOscillator,
    fi_indicator: &mut ForceIndex,
    sdc_indicator: &mut StdDevChannel,
    volume_profile_indicator: &mut VolumeProfile,
    smc_indicator: &mut SmartMoney,
    anchored_vwap_indicator: &mut AnchoredVwap,
    ema_fast: &mut Ema,
    ema_medium: &mut Ema,
    ema_slow: &mut Ema,
    ema_long: &mut Ema,
    rsi_14: &mut Rsi,
    macd: &mut Macd,
    adx_14: &mut Adx,
    sqz_mom: &mut SqueezeMomentum,
    bollinger: &mut BollingerBands,
    atr_standalone: &mut Atr,
    bbwp_indicator: &mut Bbwp,
    stochastic_indicator: &mut Stochastic,
    chandemo_indicator: &mut ChandeMO,
    supertrend_indicator: &mut Supertrend,
    keltner_indicator: &mut Keltner,
    donchian_indicator: &mut Donchian,
    obv_indicator: &mut Obv,
    cmf_indicator: &mut Cmf,
    mfi_indicator: &mut Mfi,
    hv_indicator: &mut HistoricalVolatility,
    aroon_indicator: &mut Aroon,
    choppiness_indicator: &mut Choppiness,
    linreg_indicator: &mut LinRegSlope,
    zscore_indicator: &mut ZScore,
    vwap_sum_tp_vol: &mut Decimal,
    vwap_sum_vol: &mut Decimal,
) -> CandleIndicatorReadings {
    let open_f = completed.open.to_f64().unwrap_or(0.0);
    let high_f = completed.high.to_f64().unwrap_or(0.0);
    let low_f = completed.low.to_f64().unwrap_or(0.0);
    let close_f = completed.close.to_f64().unwrap_or(0.0);
    let volume_f = completed.volume.to_f64().unwrap_or(0.0);

    let pivot_levels = pivot_points_indicator.update(high_f, low_f, close_f, day_index);

    let candlestick_reading = candlestick_indicator.update(open_f, high_f, low_f, close_f);

    // AUDIT-AIU-004/005: single soft-floor call — the previous
    // `.update().or_else(|| update_with_min_bars(…))` chain double-pushed the
    // same bar (update() pushes before returning None, then the fallback
    // pushed again), corrupting ichimoku/hull_ma windows during warmup.
    // update_with_min_bars collapses to the strict output once the buffer
    // reaches the configured period, so one call is sufficient.
    let ichimoku_reading = ichimoku_indicator.update_with_min_bars(high_f, low_f, close_f, 9);

    let cci_reading = cci_indicator.update(high_f, low_f, close_f);
    let psar_reading = psar_indicator.update(high_f, low_f);
    let wr_reading = wr_indicator.update(high_f, low_f, close_f);
    let hma_reading = hma_indicator.update_with_min_bars(close_f, 5);
    let ao_reading = ao_indicator.update(high_f, low_f);
    let fi_reading = fi_indicator.update(close_f, volume_f);
    let fi_mean_abs = fi_indicator.mean_abs();
    let sdc_reading = sdc_indicator.update(close_f);

    let volume_profile_reading =
        volume_profile_indicator.update_with_open(high_f, low_f, open_f, close_f, volume_f);

    let live_reading: Option<crate::indicators::VolumeProfileOutput> =
        if volume_profile_reading.is_some() {
            volume_profile_reading.clone()
        } else {
            volume_profile_indicator.compute_with_min_bars(25)
        };
    let volume_profile_snapshot = build_volume_profile_snapshot(
        symbol,
        slot,
        timeframe_secs,
        &live_reading,
        volume_profile_indicator
            .compute_bins_with_min_bars(25)
            .as_ref(),
        completed.start_time_ms,
        volume_profile_indicator.value_area_pct(),
    );

    let smc_reading = smc_indicator.update(open_f, high_f, low_f, close_f);

    let typical_price = (completed.high + completed.low + completed.close) / Decimal::from(3);
    *vwap_sum_tp_vol += typical_price * completed.volume;
    *vwap_sum_vol += completed.volume;

    let final_vwap = if *vwap_sum_vol > Decimal::ZERO {
        Some(*vwap_sum_tp_vol / *vwap_sum_vol)
    } else {
        None
    };

    let avwap_reading = anchored_vwap_indicator.update(
        high_f,
        low_f,
        close_f,
        volume_f,
        day_index,
        final_vwap.unwrap_or(Decimal::ZERO).to_f64().unwrap_or(0.0),
    );

    let final_ema_fast = ema_fast.update(close_f);
    let final_ema_medium = ema_medium.update(close_f);
    let final_ema_slow = ema_slow.update(close_f);
    let final_ema_long = ema_long.update(close_f);

    let ema_stack_state = if final_ema_fast > final_ema_medium
        && final_ema_medium > final_ema_slow
        && final_ema_slow > final_ema_long
        && completed.close > final_ema_fast
    {
        Some("bullish".to_string())
    } else if final_ema_fast < final_ema_medium
        && final_ema_medium < final_ema_slow
        && final_ema_slow < final_ema_long
        && completed.close < final_ema_fast
    {
        Some("bearish".to_string())
    } else {
        Some("tangled".to_string())
    };

    let final_rsi = rsi_14.update(close_f);
    let final_macd = macd.update(close_f);
    let final_adx = adx_14.update(high_f, low_f, close_f);
    let final_sqz = sqz_mom.update(high_f, low_f, close_f);
    let final_bb = bollinger.update(close_f);
    let final_atr = atr_standalone.update(high_f, low_f, close_f);
    let final_bbwp = bbwp_indicator.update(close_f);
    let final_stoch = stochastic_indicator.update(high_f, low_f, close_f);
    let final_cmo = chandemo_indicator.update(close_f);
    let final_supertrend = supertrend_indicator.update(high_f, low_f, close_f);
    let final_keltner = keltner_indicator.update(high_f, low_f, close_f);
    let final_donchian = donchian_indicator.update(high_f, low_f);
    let final_obv = obv_indicator.update(close_f, volume_f);
    let final_cmf = cmf_indicator.update(high_f, low_f, close_f, volume_f);
    let final_mfi = mfi_indicator.update(high_f, low_f, close_f, volume_f);
    let final_hv = hv_indicator.update(close_f);
    let final_aroon = aroon_indicator.update(high_f, low_f);
    let final_chop = choppiness_indicator.update(high_f, low_f, close_f);
    let final_linreg = linreg_indicator.update(close_f);
    let final_zscore = zscore_indicator.update(close_f);

    CandleIndicatorReadings {
        open_f,
        high_f,
        low_f,
        close_f,
        volume_f,
        pivot_levels,
        candlestick_reading,
        ichimoku_reading,
        cci_reading,
        psar_reading,
        wr_reading,
        hma_reading,
        ao_reading,
        fi_reading,
        fi_mean_abs,
        sdc_reading,
        volume_profile_reading,
        volume_profile_snapshot,
        smc_reading,
        final_vwap,
        avwap_reading,
        final_ema_fast,
        final_ema_medium,
        final_ema_slow,
        final_ema_long,
        ema_stack_state,
        final_rsi,
        final_macd,
        final_adx,
        final_sqz,
        final_bb,
        final_atr,
        final_bbwp,
        final_stoch,
        final_cmo,
        final_supertrend,
        final_keltner,
        final_donchian,
        final_obv,
        final_cmf,
        final_mfi,
        final_hv,
        final_aroon,
        final_chop,
        final_linreg,
        final_zscore,
    }
}

/// Build a `MarketSnapshot` for a closed candle (real OR reconstructed
/// doji) using the readings returned by [`apply_candle_to_indicators`].
/// The produced snapshot has `is_completed = Some(true)` and a populated
/// `indicators` map (so the chart's EMAs/Rsi/etc. lines advance per
/// wall-clock second), but skips the heavier matrix payloads
/// (alignment/analysis/risk/advisory/opportunity/decision_context/
/// statistical_context/context) — those require the full synthesis
/// pipeline that runs in the trade-triggered path. See the regression
/// pins in `tests/sub_minute_indicator_cadence.rs` for why this matters.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_completed_snapshot_from_readings(
    readings: &CandleIndicatorReadings,
    candle: &NormalizedCandle,
    symbol: &str,
    _pair_key: &str,
    slot: TimeframeSlot,
    timeframe_secs: u64,
    bar_count: u32,
    real_bar_count: u32,
    shadow_exchange: Option<Exchange>,
    shadow_bid: Decimal,
    shadow_ask: Decimal,
    // Configured EMA periods `(fast, medium, slow, long)` — per-line
    // ribbon availability gate (AUDIT-V8-001).
    ema_periods: (usize, usize, usize, usize),
    active_set: &crate::active_set::ActiveSet,
    // AUDIT-H7: buffer target for the staleness-aware pipeline state.
    buffer_size: usize,
    avg_vol: Option<Decimal>,
    rvol: Option<Decimal>,
    // AUDIT-H7: `[candle_buffer] stale_threshold_secs` — threaded from the
    // pipeline so doji/idle/gap-fill lifecycle maps honour the config
    // (was hardcoded 300).
    stale_threshold_secs: u32,
    // AUDIT-H7: DCP-05 staleness inputs — last completed candle start + now.
    last_completed_ms: Option<u64>,
    now_ms: u64,
    // v6.11: L1 Sharpe ratio for this completed candle. `None` for
    // synthetic doji/idle buckets (only real closes enter the Sharpe
    // window — PRI-06); `Some` for real/force-close/gap candles.
    price_trend_sharpe: Option<f64>,
) -> MarketSnapshot {
    let close_f = readings.close_f;
    let ema_stack_str = readings.ema_stack_state.as_deref();

    let indicators = normalize::build_indicator_map(
        normalize::NormalizeParams {
            close: candle.close,
            rsi: readings.final_rsi,
            rsi_divergence: crate::indicators::DivergenceState::None,
            macd_divergence: crate::indicators::DivergenceState::None,
            stoch_k: readings.final_stoch.as_ref().map(|s| s.k_value),
            stoch_d: readings.final_stoch.as_ref().map(|s| s.d_value),
            chandemo: readings.final_cmo,
            supertrend_line: readings.final_supertrend.as_ref().map(|s| s.line),
            supertrend_dir: readings.final_supertrend.as_ref().map(|s| s.direction),
            keltner: readings
                .final_keltner
                .as_ref()
                .map(|k| (k.upper, k.middle, k.lower)),
            donchian: readings
                .final_donchian
                .as_ref()
                .map(|d| (d.upper, d.middle, d.lower)),
            obv: readings.final_obv.as_ref().map(|o| o.obv),
            obv_sma: readings.final_obv.as_ref().map(|o| o.obv_sma),
            cmf: readings.final_cmf,
            mfi: readings.final_mfi,
            hv: readings.final_hv,
            aroon_up: readings.final_aroon.as_ref().map(|a| a.up),
            aroon_down: readings.final_aroon.as_ref().map(|a| a.down),
            choppiness: readings.final_chop,
            linreg_slope: readings.final_linreg,
            zscore: readings.final_zscore,
            extra_div: normalize::ExtraDivergence::default(),
            macd: &readings.final_macd,
            sqz: readings.final_sqz.as_ref(),
            adx: readings.final_adx.as_ref(),
            bb: readings.final_bb,
            atr: readings.final_atr.as_ref(),
            bbwp: readings.final_bbwp,
            vwap: readings.final_vwap,
            anchored_vwap: Some(readings.avwap_reading.clone()),
            ema_stack_state: ema_stack_str,
            ema_fast: Some(readings.final_ema_fast),
            ema_medium: Some(readings.final_ema_medium),
            ema_slow: Some(readings.final_ema_slow),
            ema_long: Some(readings.final_ema_long),
            ema_periods,
            rvol,
            volume: Some(candle.volume),
            average_volume: avg_vol,
            fib: None,
            pattern: None,
            support_levels: &[],
            resistance_levels: &[],
            active_position: None,
            adx_consecutive_deceleration: false,
            supertrend_flipped: readings
                .final_supertrend
                .as_ref()
                .map(|s| s.flipped)
                .unwrap_or(false),
            adx_di_crossover: readings.final_adx.as_ref().and_then(|a| {
                a.di_crossover.map(|c| match c {
                    crate::indicators::DiCrossoverDir::Bullish => 1i8,
                    crate::indicators::DiCrossoverDir::Bearish => -1i8,
                })
            }),
            pivot_levels: readings.pivot_levels,
            pivot_proximity_pct: 0.0015,
            candlestick: Some(readings.candlestick_reading),
            // AUDIT-AIU-073: config default (helper path carries no config).
            candlestick_min_confidence: 0.3,
            ichimoku: readings.ichimoku_reading,
            cci: readings.cci_reading,
            psar: readings.psar_reading,
            williams_r: readings.wr_reading,
            awesome_oscillator: readings.ao_reading,
            force_index: readings.fi_reading,
            force_index_mean_abs: readings.fi_mean_abs,
            hull_ma: readings.hma_reading,
            stddev_channel: readings.sdc_reading,
            volume_profile: readings.volume_profile_reading.clone(),
            smc: readings.smc_reading.clone(),
            prev: PreviousBarState::default(),
            // AUDIT-AIU-071: config defaults (no tf_config in this helper).
            rvol_institutional_threshold: 1.5,
            rvol_climax_threshold: 3.0,
            price_trend_sharpe,
        },
        bar_count,
        false,
        active_set,
    );

    // Stamp a basic prev-bar state so cross-bar detection has something to
    // diff against on the next candle. Full synthesis (which has access
    // to fib/pattern/SR) is not wired into this helper — that's the
    // trade-triggered path's job. The chart still gets a complete
    // indicators map including ema_stack, rsi, macd, etc.
    let prev_bar_state = PreviousBarState {
        rsi: readings.final_rsi.map(|d| d.to_f64().unwrap_or(0.0)),
        stoch_k: readings
            .final_stoch
            .as_ref()
            .map(|s| s.k_value.to_f64().unwrap_or(0.0)),
        stoch_d: readings
            .final_stoch
            .as_ref()
            .map(|s| s.d_value.to_f64().unwrap_or(0.0)),
        cmf: readings.final_cmf.map(|d| d.to_f64().unwrap_or(0.0)),
        chandemo: readings.final_cmo.map(|d| d.to_f64().unwrap_or(0.0)),
        aroon_up: readings
            .final_aroon
            .as_ref()
            .map(|a| a.up.to_f64().unwrap_or(0.0)),
        aroon_down: readings
            .final_aroon
            .as_ref()
            .map(|a| a.down.to_f64().unwrap_or(0.0)),
        macd_line: Some(readings.final_macd.macd_line.to_f64().unwrap_or(0.0)),
        macd_histogram: Some(readings.final_macd.histogram.to_f64().unwrap_or(0.0)),
        linreg_slope: readings.final_linreg,
        zscore: readings.final_zscore,
        obv: readings
            .final_obv
            .as_ref()
            .map(|o| o.obv.to_f64().unwrap_or(0.0)),
        obv_sma: readings
            .final_obv
            .as_ref()
            .map(|o| o.obv_sma.to_f64().unwrap_or(0.0)),
        mfi: readings.final_mfi.map(|d| d.to_f64().unwrap_or(0.0)),
        adx_plus_di: readings
            .final_adx
            .as_ref()
            .map(|a| a.plus_di.to_f64().unwrap_or(0.0)),
        adx_minus_di: readings
            .final_adx
            .as_ref()
            .map(|a| a.minus_di.to_f64().unwrap_or(0.0)),
        price: Some(close_f),
        ema_fast: Some(readings.final_ema_fast.to_f64().unwrap_or(0.0)),
        ema_medium: Some(readings.final_ema_medium.to_f64().unwrap_or(0.0)),
        // AUDIT-AIU-030: carried for the StackChange detector.
        ema_slow: Some(readings.final_ema_slow.to_f64().unwrap_or(0.0)),
        ema_long: Some(readings.final_ema_long.to_f64().unwrap_or(0.0)),
        supertrend_line: readings
            .final_supertrend
            .as_ref()
            .map(|s| s.line.to_f64().unwrap_or(0.0)),
        pivot_active_level: readings.pivot_levels.map(|lv| {
            let p = lv.pivot.to_f64().unwrap_or(0.0);
            if close_f >= p {
                1.0
            } else {
                -1.0
            }
        }),
        ichimoku_tenkan: readings
            .ichimoku_reading
            .map(|r| r.tenkan.to_f64().unwrap_or(0.0)),
        ichimoku_kijun: readings
            .ichimoku_reading
            .map(|r| r.kijun.to_f64().unwrap_or(0.0)),
        price_vs_cloud: readings.ichimoku_reading.map(|r| {
            let top = r
                .senkou_a_current
                .to_f64()
                .unwrap_or(0.0)
                .max(r.senkou_b_current.to_f64().unwrap_or(0.0));
            let bot = r
                .senkou_a_current
                .to_f64()
                .unwrap_or(0.0)
                .min(r.senkou_b_current.to_f64().unwrap_or(0.0));
            let px = close_f;
            if px > top {
                1.0
            } else if px < bot {
                -1.0
            } else {
                0.0
            }
        }),
        ichimoku_future_bias: readings
            .ichimoku_reading
            .map(|r| (r.senkou_a - r.senkou_b).to_f64().unwrap_or(0.0).signum()),
        hull_ma: readings.hma_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        awesome_oscillator: readings.ao_reading.map(|d| d.value.to_f64().unwrap_or(0.0)),
        force_index: readings.fi_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        williams_r: readings.wr_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        cci: readings.cci_reading.map(|d| d.to_f64().unwrap_or(0.0)),
        psar_sar: readings.psar_reading.map(|d| d.sar.to_f64().unwrap_or(0.0)),
        funding_rate: None,
        cascade_state: None,
    };
    let _ = prev_bar_state; // future cross-bar detection hook

    let indicator_lifecycle = build_indicator_lifecycle_map(
        &indicators,
        &IndicatorLifecycleMap::new(),
        stale_threshold_secs,
        bar_count,
        real_bar_count,
        false,
        core_domain::LatencyTracker::now_ms(),
        bar_count > 0,
    );

    MarketSnapshot {
        timeframe_slot: Some(slot),
        exchange: shadow_exchange,
        timeframe_secs,
        timestamp: candle.start_time_ms / 1000,
        symbol: symbol.to_string(),
        is_completed: Some(true),
        mid_price: candle.close,
        bid_price: shadow_bid,
        ask_price: shadow_ask,
        // AUDIT-H6: never mislabel traded volume as top-of-book depth.
        // The old `Some(candle.volume)` published candle volume as
        // bid/ask size on doji/idle/gap-fill frames (the warm path fixed
        // this exact bug; this path had not). These synthetic frames have
        // no order-book snapshot — depth is honestly `None`.
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        open: Some(candle.open),
        high: Some(candle.high),
        low: Some(candle.low),
        close: Some(candle.close),
        volume: Some(candle.volume),
        average_volume: avg_vol,
        // AUDIT-H7: DCP-05 staleness-aware pipeline state (Live→Stale→
        // Failed from the last completed candle). The passed-in
        // `pipeline_state` param reflected buffer fill only.
        pipeline_state: derive_pipeline_state_with_staleness(
            bar_count as usize,
            buffer_size,
            last_completed_ms,
            now_ms,
            stale_threshold_secs,
        ),
        indicator_lifecycle,
        context: None,
        decision_context: None,
        statistical_context: None,
        indicators,
        alignment: None,
        risk: None,
        analysis: None,
        advisory: None,
        opportunity: None,
        risk_profile: None,
        liquidity: None,
        cluster: None,
        volume_profile: readings.volume_profile_snapshot.clone(),
        liquidity_signals: vec![],
        metrics_config: active_set.to_metrics_config(),
        quality_envelope: Some(CandleQualityEnvelope {
            // 02-03 §4: synthetic gap-fill candles carry the −20 penalty
            // on this construction site too (the other two sites already
            // apply it; this one hardcoded 100.0).
            quality_score: if candle.reconstructed.is_some() {
                80.0
            } else {
                100.0
            },
            is_valid: true,
            is_gap_filled: candle.reconstructed.is_some(),
            had_outliers_rejected: false,
            spike_detected: false,
            is_stale: false,
            // AUDIT-M11: DIE L3 sequence audit on the synthetic path too.
            sequence_integrity: classify_sequence(last_completed_ms, candle.start_time_ms),
            gap_since_last: candle.duration_ms / 1000,
            validated_at: candle.start_time_ms + candle.duration_ms,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn broadcast_live_snapshot(
    broadcast_tx: &broadcast::Sender<MarketSnapshot>,
    symbol: &str,
    candle: &NormalizedCandle,
    exchange: Option<Exchange>,
    bid_price: Decimal,
    ask_price: Decimal,
    // Top-of-book depth sizes (None when the book is stale/empty — shadow
    // frames never carry candle volume as depth, 02-07 §2.1 semantics).
    ob_bid_size: Option<Decimal>,
    ob_ask_size: Option<Decimal>,
    slot: TimeframeSlot,
    ema_fast: &Ema,
    ema_medium: &Ema,
    ema_slow: &Ema,
    ema_long: &Ema,
    rsi_14: &Rsi,
    macd: &Macd,
    adx_14: &Adx,
    sqz_mom: &SqueezeMomentum,
    bollinger: &BollingerBands,
    atr_standalone: &Atr,
    bbwp_indicator: &Bbwp,
    stochastic_indicator: &Stochastic,
    chandemo_indicator: &ChandeMO,
    supertrend_indicator: &Supertrend,
    keltner_indicator: &Keltner,
    donchian_indicator: &Donchian,
    obv_indicator: &Obv,
    cmf_indicator: &Cmf,
    mfi_indicator: &Mfi,
    hv_indicator: &HistoricalVolatility,
    aroon_indicator: &Aroon,
    choppiness_indicator: &Choppiness,
    linreg_indicator: &LinRegSlope,
    zscore_indicator: &ZScore,
    vwap_sum_tp_vol: &Decimal,
    vwap_sum_vol: &Decimal,
    volume_history: &VecDeque<Decimal>,
    timeframe_secs: u64,
    prev_day_px: Option<Decimal>,
    // Number of completed candles for this TF.  Passed through to
    // `build_indicator_map` so the shadow path uses the real count.
    bar_count: u32,
    // PRI-12: real (non-synthetic) completed candles.
    real_bar_count: u32,
    // Configured EMA periods `(fast, medium, slow, long)` — per-line
    // ribbon availability gate (AUDIT-V8-001).
    ema_periods: (usize, usize, usize, usize),
    // Pipeline lifecycle state derived from buffer fill.  Carried on the
    // shadow snapshot so the frontend never sees a spurious `Initializing`
    // state that would flash the pipeline banner.
    pipeline_state: CandlePipelineState,
    // v6.10 (Phase 5 / E1): active_set passed for disabled-indicator filtering
    // (CA-06). Disabled indicators become absent from the indicator map.
    active_set: &crate::active_set::ActiveSet,
    // Whether the snapshot represents a completed (closed) candle.  Shadow
    // ticks always carry `false`; stale-check force-close and gap-fill paths
    // pass `true` so the frontend can correctly distinguish live from
    // completed candles.
    is_completed: bool,
) {
    let close_f = candle.close.to_f64().unwrap_or(0.0);

    let avg_vol = if !volume_history.is_empty() {
        let sum: Decimal = volume_history.iter().sum();
        Some(sum / Decimal::from(volume_history.len()))
    } else {
        None
    };

    let rvol = match (candle.volume, avg_vol) {
        (vol, Some(avg)) if avg > Decimal::ZERO => Some(vol / avg),
        _ => None,
    };

    let high_f = candle.high.to_f64().unwrap_or(0.0);
    let low_f = candle.low.to_f64().unwrap_or(0.0);
    let volume_f = candle.volume.to_f64().unwrap_or(0.0);

    let val_ema_fast = ema_fast.clone().update(close_f);
    let val_ema_medium = ema_medium.clone().update(close_f);
    let val_ema_slow = ema_slow.clone().update(close_f);
    let val_ema_long = ema_long.clone().update(close_f);
    let val_rsi = rsi_14.clone().update(close_f);
    let val_macd = macd.clone().update(close_f);
    let val_adx = adx_14.clone().update(high_f, low_f, close_f);
    let val_sqz = sqz_mom.clone().update(high_f, low_f, close_f);
    let val_bb = bollinger.clone().update(close_f);
    let val_atr = atr_standalone.clone().update(high_f, low_f, close_f);
    let val_bbwp = bbwp_indicator.clone().update(close_f);
    let val_stoch = stochastic_indicator.clone().update(high_f, low_f, close_f);
    let val_cmo = chandemo_indicator.clone().update(close_f);
    let val_supertrend = supertrend_indicator.clone().update(high_f, low_f, close_f);
    let val_keltner = keltner_indicator.clone().update(high_f, low_f, close_f);
    let val_donchian = donchian_indicator.clone().update(high_f, low_f);
    let val_obv = obv_indicator.clone().update(close_f, volume_f);
    let val_cmf = cmf_indicator
        .clone()
        .update(high_f, low_f, close_f, volume_f);
    let val_mfi = mfi_indicator
        .clone()
        .update(high_f, low_f, close_f, volume_f);
    let val_hv = hv_indicator.clone().update(close_f);
    let val_aroon = aroon_indicator.clone().update(high_f, low_f);
    let val_chop = choppiness_indicator.clone().update(high_f, low_f, close_f);
    let val_linreg = linreg_indicator.clone().update(close_f);
    let val_zscore = zscore_indicator.clone().update(close_f);

    let typical_price = (candle.high + candle.low + candle.close) / Decimal::from(3);
    let temp_sum_tp_vol = *vwap_sum_tp_vol + typical_price * candle.volume;
    let temp_sum_vol = *vwap_sum_vol + candle.volume;
    let val_vwap = if temp_sum_vol > Decimal::ZERO {
        Some(temp_sum_tp_vol / temp_sum_vol)
    } else {
        None
    };

    let ema_stack_state = if val_ema_fast > val_ema_medium
        && val_ema_medium > val_ema_slow
        && val_ema_slow > val_ema_long
        && candle.close > val_ema_fast
    {
        Some("bullish")
    } else if val_ema_fast < val_ema_medium
        && val_ema_medium < val_ema_slow
        && val_ema_slow < val_ema_long
        && candle.close < val_ema_fast
    {
        Some("bearish")
    } else {
        Some("tangled")
    };

    let indicators = normalize::build_indicator_map(
        normalize::NormalizeParams {
            close: candle.close,
            rsi: val_rsi,
            rsi_divergence: crate::indicators::DivergenceState::None,
            macd_divergence: crate::indicators::DivergenceState::None,
            stoch_k: val_stoch.as_ref().map(|s| s.k_value),
            stoch_d: val_stoch.as_ref().map(|s| s.d_value),
            chandemo: val_cmo,
            supertrend_line: val_supertrend.as_ref().map(|s| s.line),
            supertrend_dir: val_supertrend.as_ref().map(|s| s.direction),
            keltner: val_keltner.as_ref().map(|k| (k.upper, k.middle, k.lower)),
            donchian: val_donchian.as_ref().map(|d| (d.upper, d.middle, d.lower)),
            obv: val_obv.as_ref().map(|o| o.obv),
            obv_sma: val_obv.as_ref().map(|o| o.obv_sma),
            cmf: val_cmf,
            mfi: val_mfi,
            hv: val_hv,
            aroon_up: val_aroon.as_ref().map(|a| a.up),
            aroon_down: val_aroon.as_ref().map(|a| a.down),
            choppiness: val_chop,
            linreg_slope: val_linreg,
            zscore: val_zscore,
            extra_div: normalize::ExtraDivergence::default(),
            macd: &val_macd,
            sqz: val_sqz.as_ref(),
            adx: val_adx.as_ref(),
            bb: val_bb,
            atr: val_atr.as_ref(),
            bbwp: val_bbwp,
            vwap: val_vwap,
            anchored_vwap: None,
            ema_stack_state,
            ema_fast: Some(val_ema_fast),
            ema_medium: Some(val_ema_medium),
            ema_slow: Some(val_ema_slow),
            ema_long: Some(val_ema_long),
            ema_periods,
            rvol,
            volume: Some(candle.volume),
            average_volume: avg_vol,
            fib: None,
            pattern: None,
            support_levels: &[],
            resistance_levels: &[],
            active_position: None,
            adx_consecutive_deceleration: false,
            supertrend_flipped: false,
            adx_di_crossover: None,
            pivot_levels: None,
            pivot_proximity_pct: 0.0015,
            candlestick: None,
            // AUDIT-AIU-073: config default (shadow path carries no config).
            candlestick_min_confidence: 0.3,
            ichimoku: None,
            cci: None,
            psar: None,
            williams_r: None,
            awesome_oscillator: None,
            force_index: None,
            force_index_mean_abs: None,
            hull_ma: None,
            stddev_channel: None,
            volume_profile: None,
            smc: None,
            prev: PreviousBarState::default(),
            // AUDIT-AIU-071: config defaults (shadow path carries no config).
            rvol_institutional_threshold: 1.5,
            rvol_climax_threshold: 3.0,
            // Close-only (updates_on_shadow: false): shadow ticks never
            // carry the Sharpe value — the frontend preserves the last
            // completed-candle reading via its per-key merge.
            price_trend_sharpe: None,
        },
        bar_count,
        true,
        active_set,
    );

    let snapshot = MarketSnapshot {
        timeframe_slot: Some(slot),
        exchange,
        timeframe_secs,
        timestamp: candle.start_time_ms / 1000,
        symbol: symbol.to_string(),
        is_completed: Some(is_completed),
        mid_price: candle.close,
        bid_price,
        ask_price,
        bid_size: ob_bid_size,
        ask_size: ob_ask_size,
        funding_rate: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px,
        open: Some(candle.open),
        high: Some(candle.high),
        low: Some(candle.low),
        close: Some(candle.close),
        volume: Some(candle.volume),
        average_volume: avg_vol,
        pipeline_state,
        indicator_lifecycle: build_indicator_lifecycle_map(
            &indicators,
            &IndicatorLifecycleMap::new(),
            300,
            bar_count,
            real_bar_count,
            true,
            core_domain::LatencyTracker::now_ms(),
            // AUDIT-AIU-013: the previous hardcoded `false` made every
            // indicator report `Loading` on shadow ticks even when the
            // pipeline is `Live` (the ILS Live gate requires
            // `pipeline_is_live`), so the dashboard badge could never show
            // Live on live ticks. The pipeline state passed in is already
            // derived from the buffer fill, so a Live pipeline now surfaces
            // Live badges on shadow snapshots too.
            pipeline_state == CandlePipelineState::Live,
        ),
        context: None,
        decision_context: None,
        statistical_context: None,
        indicators,
        alignment: None,
        risk: None,
        analysis: None,
        advisory: None,
        opportunity: None,
        liquidity_signals: vec![],
        metrics_config: None,
        risk_profile: None,
        liquidity: None,
        cluster: None,
        volume_profile: None,
        quality_envelope: None,
    };

    let _ = broadcast_tx.send(snapshot);
}

#[cfg(test)]
mod age_tests {
    use super::stamp_signal_ages;
    use crate::indicators::{
        IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus,
    };
    use std::collections::HashMap;

    fn entry_with_signal(dir: SignalDirection) -> NormalizedIndicatorValue {
        NormalizedIndicatorValue::scalar(0.0, 0.5, "X").push_signal(IndicatorSignal::new(
            SignalKind::Divergence,
            dir,
            SignalStatus::Potential,
            "DIV",
        ))
    }

    #[test]
    fn age_increments_while_signal_persists() {
        let mut tracker = HashMap::new();
        let mut m = HashMap::new();
        m.insert(
            "rsi".to_string(),
            entry_with_signal(SignalDirection::Bullish),
        );
        stamp_signal_ages(&mut m, &mut tracker, 1);
        assert_eq!(m["rsi"].signals[0].age_bars, 0, "fresh signal age 0");

        let mut m2 = HashMap::new();
        m2.insert(
            "rsi".to_string(),
            entry_with_signal(SignalDirection::Bullish),
        );
        stamp_signal_ages(&mut m2, &mut tracker, 4);
        assert_eq!(m2["rsi"].signals[0].age_bars, 3, "3 bars since first seen");
    }

    #[test]
    fn age_resets_on_direction_flip() {
        let mut tracker = HashMap::new();
        let mut m = HashMap::new();
        m.insert(
            "rsi".to_string(),
            entry_with_signal(SignalDirection::Bullish),
        );
        stamp_signal_ages(&mut m, &mut tracker, 1);

        let mut m2 = HashMap::new();
        m2.insert(
            "rsi".to_string(),
            entry_with_signal(SignalDirection::Bearish),
        );
        stamp_signal_ages(&mut m2, &mut tracker, 5);
        assert_eq!(m2["rsi"].signals[0].age_bars, 0, "flip resets age");
    }
}

#[cfg(test)]
mod lifecycle_tests {
    //! Tests for `build_indicator_lifecycle_map` — in particular the
    //! non-WARMING entry guard added so the lifecycle flips to `Live` only
    //! when a real reading is present (closes the
    //! `Live + UNKNOWN state_label` race that produced a `0.00 / UNKNOWN`
    //! row in the indicators table for indicators whose strict compute
    //! gate fires later than `bars_required`, e.g. `volume_profile`).
    use super::build_indicator_lifecycle_map;
    use crate::indicators::NormalizedIndicatorValue;
    use core_domain::indicator_dtos::{IndicatorLifecycleMap, IndicatorLifecycleState};
    use std::collections::HashMap;

    /// A WARMING placeholder mirrors the one the normalizer inserts for
    /// every registered key when its source data is not yet available.
    fn warming_placeholder() -> NormalizedIndicatorValue {
        NormalizedIndicatorValue::scalar(0.0, 0.0, "WARMING").with_confidence(0.0)
    }

    /// A real reading with non-zero confidence and a non-WARMING label.
    fn real_reading() -> NormalizedIndicatorValue {
        NormalizedIndicatorValue::scalar(50.0, 0.4, "RSI_NEUTRAL")
    }

    #[test]
    fn lifecycle_is_loading_when_bar_count_below_bars_required() {
        let mut m = HashMap::new();
        m.insert("rsi".to_string(), real_reading());
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            5,
            5,
            false,
            1000,
            true,
        );
        let rsi = map.get("rsi").expect("rsi present");
        assert_eq!(rsi.state, IndicatorLifecycleState::Loading);
        assert_eq!(rsi.bars_seen, 5);
    }

    #[test]
    fn lifecycle_is_loading_when_entry_is_warming_placeholder() {
        // Regression: bar_count (300) comfortably exceeds rsi's `bars_required`
        // (1) but the entry is still the WARMING placeholder inserted by
        // the warming fill. The lifecycle must NOT flip to Live — otherwise
        // the frontend renders `Live` + `UNKNOWN` in the indicators table.
        let mut m = HashMap::new();
        m.insert("rsi".to_string(), warming_placeholder());
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            false,
            1000,
            true,
        );
        let rsi = map.get("rsi").expect("rsi present");
        assert_eq!(
            rsi.state,
            IndicatorLifecycleState::Loading,
            "WARMING entry must keep lifecycle in Loading"
        );
    }

    #[test]
    fn lifecycle_flips_live_once_real_reading_arrives() {
        // First the warm-up phase: bars_seen=300, only the WARMING placeholder.
        let mut m = HashMap::new();
        m.insert("rsi".to_string(), warming_placeholder());
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            false,
            1000,
            true,
        );
        assert_eq!(map["rsi"].state, IndicatorLifecycleState::Loading);

        // Then a real reading arrives: lifecycle flips to Live.
        m.insert("rsi".to_string(), real_reading());
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            false,
            1000,
            true,
        );
        assert_eq!(map["rsi"].state, IndicatorLifecycleState::Live);
    }

    #[test]
    fn lifecycle_is_loading_for_volume_profile_until_strict_gate_fires() {
        // Volume profile's `bars_required` is 50, but the strict `compute()`
        // gate fires only at `window_size / 2` (150 with the default 300-bar
        // window). Until then the indicators map carries the WARMING
        // placeholder. The lifecycle must stay Loading through bars 50..249
        // so the frontend never renders the misleading `Live + UNKNOWN` row.
        let mut m = HashMap::new();
        m.insert("volume_profile".to_string(), warming_placeholder());
        for bar_count in [50u32, 75, 100, 150, 200, 249] {
            let map = build_indicator_lifecycle_map(
                &m,
                &IndicatorLifecycleMap::new(),
                300,
                bar_count,
                bar_count,
                false,
                1000,
                true,
            );
            let vp = map.get("volume_profile").expect("volume_profile present");
            assert_eq!(
                vp.state,
                IndicatorLifecycleState::Loading,
                "bar_count={bar_count}: WARMING placeholder must keep volume_profile Loading"
            );
        }
    }

    #[test]
    fn lifecycle_flips_live_with_neutral_label_and_zero_confidence() {
        // Contract: the lifecycle gate is `state_label != "WARMING"`. The
        // previous `confidence > 0` clause was removed because the
        // normalizer derives `confidence = |normalized|` for `scalar(...)`
        // entries, which would have permanently trapped ContextOnly gates
        // (BBWP, ATR, RVOL, …) and event-only overlays (Hull MA) in
        // `Loading` — the regression that surfaced as
        // `Raw 0.00 / Norm 0.00 / State UNKNOWN` rows in the Metrics
        // Indicators table.
        let m_entry =
            NormalizedIndicatorValue::scalar(0.0, 0.0, "RSI_NEUTRAL").with_confidence(0.0);
        let mut m = HashMap::new();
        m.insert("rsi".to_string(), m_entry);
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            false,
            1000,
            true,
        );
        assert_eq!(
            map["rsi"].state,
            IndicatorLifecycleState::Live,
            "non-WARMING entry with normalized=0.0 and confidence=0.0 must still flip to Live",
        );
    }

    #[test]
    fn lifecycle_flips_live_with_context_only_gate_zero_normalized() {
        // Mirrors the production case for BBWP / ATR / RVOL / etc.: the
        // registry `directional = false` gates emit `normalized = 0.0`
        // by contract, with a non-WARMING label and non-zero confidence.
        // The lifecycle must flip to `Live` so the Metrics table renders
        // the badge correctly instead of permanently showing `Warming`.
        let bbwp = NormalizedIndicatorValue::scalar(50.0, 0.0, "NORMAL_VOLATILITY_BULL_CYCLE")
            .with_confidence(0.50);
        let mut m = HashMap::new();
        m.insert("bbwp".to_string(), bbwp);
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            false,
            1000,
            true,
        );
        assert_eq!(
            map["bbwp"].state,
            IndicatorLifecycleState::Live,
            "ContextOnly gate with normalized=0.0 must reach Live once bars_seen ≥ bars_required",
        );
    }

    #[test]
    fn lifecycle_flips_live_when_indicator_emits_neutral_with_levels() {
        // Event-driven indicators (fibonacci, support_resistance,
        // pivot_points, chart_patterns) emit
        // `NormalizedIndicatorValue::scalar(0.0, 0.0, "..._NEUTRAL")` in
        // their resting state — `scalar()` derives
        // `confidence = |normalized| = 0.0`. Their *level* data lives in
        // the `values` submap. The lifecycle must treat a populated
        // `values` submap as a real reading, so the frontend does not show
        // "Warming (n/50)" forever for indicators that have already
        // produced valid resting-level output.
        let mut levels = std::collections::HashMap::new();
        levels.insert("gp_top".to_string(), 12_345.0);
        levels.insert("gp_bottom".to_string(), 12_000.0);
        levels.insert("fib_0618".to_string(), 12_117.0);
        levels.insert("ext_1618".to_string(), 12_690.0);
        let fib_entry =
            NormalizedIndicatorValue::with_values(0.0, 0.0, "FIBONACCI_NEUTRAL", levels);
        let mut m = HashMap::new();
        m.insert("fibonacci".to_string(), fib_entry);
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            100,
            100,
            false,
            1000,
            true,
        );
        let fib = map.get("fibonacci").expect("fibonacci present");
        assert_eq!(
            fib.state,
            IndicatorLifecycleState::Live,
            "Fibonacci with populated `values` must flip to Live even with confidence=0"
        );
    }

    /// Regression: SMC indicators are tagged `data_source = Some(EventDriven)`
    /// and the WARMING fill is suppressed for them. The lifecycle builder
    /// must therefore see them as **missing** (no entry in the map) when no
    /// event has fired, and keep them in `Loading` until an event populates
    /// the entry. This is the contract the UI relies on to render
    /// `--/--/Warming (X/Y)` rows in the Metrics Indicators table instead
    /// of the misleading `Raw 0.00 / Norm 0.00 / State UNKNOWN` that
    /// surfaced when the WARMING placeholder was emitted.
    #[test]
    fn smc_lifecycle_stays_loading_when_no_event_fired() {
        // SMC has `bars_required = 50`. Even after 50 candles, with no
        // event, the entry must remain absent from the indicator map and
        // the lifecycle must stay `Loading`.
        let mut m = HashMap::new();
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            50,
            50,
            false,
            1000,
            true,
        );
        for key in [
            "smc_structure",
            "smc_liquidity",
            "smc_fvg",
            "smc_order_blocks",
        ] {
            let lc = map
                .get(key)
                .unwrap_or_else(|| panic!("{key} must be present in the lifecycle map"));
            assert_eq!(
                lc.state,
                IndicatorLifecycleState::Loading,
                "{key}: no event yet → Loading (entry is absent by design; WARMING fill is suppressed for EventDriven)"
            );
            assert_eq!(lc.bars_required, 50, "{key}: bars_required must be 50");
            assert_eq!(
                lc.bars_seen, 50,
                "{key}: bars_seen must reflect the running count"
            );
        }

        // Once an event fires and the entry is inserted with a real
        // (non-WARMING) reading, the lifecycle must flip to `Live`.
        m.insert(
            "smc_structure".to_string(),
            NormalizedIndicatorValue::scalar(0.7, 0.7, "BOS_BULLISH"),
        );
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            50,
            50,
            false,
            1000,
            true,
        );
        let smc = map.get("smc_structure").expect("smc_structure present");
        assert_eq!(
            smc.state,
            IndicatorLifecycleState::Live,
            "smc_structure: a real BOS_BULLISH reading must flip to Live"
        );
    }

    /// Regression: close-only indicators (`updates_on_shadow: false`) such
    /// as Hull MA, Ichimoku, Anchored VWAP, and Parabolic SAR are
    /// intentionally absent from shadow-tick indicators maps (the WARMING
    /// fill skips them at `normalized/all.rs:1746-1762` so the frontend's
    /// per-key merge preserves the last completed-candle values). On the
    /// completed-candle path they are present and report normally — but
    /// on every shadow tick (the dominant snapshot during live trading,
    /// especially on sub-minute TFs) the lifecycle would otherwise stay
    /// `Loading (N/N)` even after the calculator reached its warm-up gate.
    ///
    /// The lifecycle builder now recognizes the close-only-on-shadow
    /// pattern: when `is_shadow && !updates_on_shadow && !present &&
    /// bars_seen >= bars_required`, the indicator is `Live from the last
    /// completed candle`. This is what makes the dashboard show a real
    /// State column for Hull MA / Ichimoku / AVWAP / PSAR / and the 23
    /// other close-only entries instead of perpetually reporting
    /// `WARMING (50/X)`.
    #[test]
    fn lifecycle_is_live_for_close_only_on_shadow_when_bar_count_sufficient() {
        // Hull MA: bars_required=14, updates_on_shadow=false (close-only).
        // No entry in the shadow-tick indicators map; bar_count=50 (well
        // above 14). Lifecycle must be Live.
        let m = HashMap::new();
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            50,
            50,
            true,
            1000,
            true,
        );
        for key in ["hull_ma", "ichimoku", "anchored_vwap", "psar"] {
            let lc = map
                .get(key)
                .unwrap_or_else(|| panic!("{key} must be present in the lifecycle map"));
            assert_eq!(
                lc.state,
                IndicatorLifecycleState::Live,
                "{key}: close-only-on-shadow with bars_seen >= bars_required must be Live",
            );
        }

        // Same indicators must stay Loading on the completed path with no
        // entry — the WARMING fill is not skipped there, so a missing
        // entry means "calculator has not produced a value yet" and must
        // not flip to Live. This is the regression guard: a careless
        // implementation could "always mark absent entries Live" and
        // break the WARMING contract on the completed path.
        let map_completed = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            50,
            50,
            false,
            1000,
            true,
        );
        for key in ["hull_ma", "ichimoku", "anchored_vwap", "psar"] {
            let lc = map_completed
                .get(key)
                .unwrap_or_else(|| panic!("{key} must be present in the lifecycle map"));
            assert_eq!(
                lc.state,
                IndicatorLifecycleState::Loading,
                "{key}: completed path with absent entry must stay Loading (the calculator has not yet produced a value)",
            );
        }
    }

    /// Close-only indicators with `updates_on_shadow: false` must still
    /// honor their warm-up gate on shadow ticks: when `bars_seen <
    /// bars_required` the calculator has not yet produced enough
    /// completed candles and the lifecycle must stay `Loading`.
    #[test]
    fn lifecycle_is_loading_for_close_only_on_shadow_when_below_warmup_gate() {
        // Hull MA bars_required=14; bar_count=5 < 14. Even though the
        // entry is absent and updates_on_shadow=false, the gate has not
        // fired yet — lifecycle stays Loading.
        let m = HashMap::new();
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            5,
            5,
            true,
            1000,
            true,
        );
        let hma = map.get("hull_ma").expect("hull_ma present");
        assert_eq!(
            hma.state,
            IndicatorLifecycleState::Loading,
            "hull_ma on shadow tick with bars_seen < bars_required must stay Loading",
        );

        // AVWAP bars_required=1; bar_count=5 ≥ 1 — gate satisfied, Live.
        let avwap = map.get("anchored_vwap").expect("anchored_vwap present");
        assert_eq!(
            avwap.state,
            IndicatorLifecycleState::Live,
            "anchored_vwap on shadow tick with bars_seen >= bars_required must be Live",
        );
    }

    /// Indicators with `updates_on_shadow: true` (RSI, EMA, Supertrend,
    /// Donchian, Keltner, ADX, …) are computed on every shadow tick and
    /// produce a real entry in the indicators map. They must NOT be
    /// affected by the close-only-on-shadow branch — their lifecycle is
    /// governed by the standard `is_real_reading` check. This is a
    /// regression guard against the close-only branch accidentally
    /// short-circuiting real-reading paths.
    #[test]
    fn lifecycle_for_shadow_enabled_indicator_unaffected_by_close_only_branch() {
        let mut m = HashMap::new();
        m.insert("rsi".to_string(), real_reading());
        let map = build_indicator_lifecycle_map(
            &m,
            &IndicatorLifecycleMap::new(),
            300,
            300,
            300,
            true,
            1000,
            true,
        );
        let rsi = map.get("rsi").expect("rsi present");
        assert_eq!(
            rsi.state,
            IndicatorLifecycleState::Live,
            "rsi with updates_on_shadow=true and a real reading must be Live via the standard branch",
        );
    }
}
