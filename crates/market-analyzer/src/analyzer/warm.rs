use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::VecDeque;

use config_models::FibonacciConfig;
use config_models::TimeframeConfig;

use crate::analyzer::derive_pipeline_state;
use crate::analyzer::normalize::{series_divergence_state, ExtraDivergence};
use crate::analyzer::update_sr_levels;
use crate::indicators::normalized::PreviousBarState;
use crate::indicators::{
    detect_pattern, Adx, AnchoredVwap, Aroon, Atr, AwesomeOscillator, Bbwp, BollingerBands,
    Candlestick, CandlestickConfig, Cci, ChandeMO, Choppiness, Cmf, DivergenceDetector, Donchian,
    Ema, FibonacciRange, ForceIndex, HistoricalVolatility, HullMA, Ichimoku, Keltner, LinRegSlope,
    Macd, Mfi, Obv, ParabolicSar, PivotMethod, PivotPoints, Rsi, SeriesDivergence, SmartMoney,
    SqueezeMomentum, StdDevChannel, Stochastic, Supertrend, VolumeProfile, WilliamsR, ZScore,
};
use crate::sr_engine::SrRoleTracker;
use core_domain::models::{CandlePipelineState, MarketSnapshot};
use core_domain::normalized::{Exchange, NormalizedCandle};
use core_domain::volume_profile::VolumeProfileSnapshot;

/// **API contract cap** for both `pipeline.history` (raw candle deque)
/// and `pipeline.snapshot_history` (MarketSnapshot deque). Deliberately
/// a single constant rather than a per-TF configuration so the
/// `/api/history` endpoint can guarantee the same 1000-candle response
/// across all configured timeframes and both supported exchanges
/// (Hyperliquid + Bitget). It is the rolling live-runtime cap (oldest
/// entries are dropped on each new candle close). The warmup also trims
/// to this constant so a freshly bootstrapped engine starts at the cap
/// rather than carrying an oversized seed.
///
/// # Pin tests
/// `crates/market-analyzer/tests/hist_buffer_cap_uniformity.rs` and the
/// inline `#[cfg(test)]` tests below guard this constant. Any change to
/// the value must update those tests deliberately (the `/api/history`
/// response contract assumes 1000).
pub const HIST_BUFFER_MAX: usize = 1000;

/// Trim a `Vec<T>` to the rolling cap, dropping the oldest entries. Pure
/// helper so the cap behaviour is testable in isolation — the full
/// warmup iterates 50+ indicators per candle and is too expensive for a
/// tight regression test. Behaviour is uniform across all TFs and both
/// exchanges: the function has no per-TF / per-exchange branching.
fn trim_snapshot_history_to_cap<T: Clone>(mut snapshot_history: Vec<T>) -> Vec<T> {
    if snapshot_history.len() > HIST_BUFFER_MAX {
        snapshot_history = snapshot_history[snapshot_history.len() - HIST_BUFFER_MAX..].to_vec();
    }
    snapshot_history
}

#[cfg(test)]
mod cap_trim_tests {
    //! Pin tests for the `HIST_BUFFER_MAX` trim behaviour. The full
    //! warmup exercises 50+ indicators per candle and runs >60s for the
    //! full regression matrix; these unit tests on the pure helper run
    //! in milliseconds and prove the cap invariant holds uniformly.
    use super::{trim_snapshot_history_to_cap, HIST_BUFFER_MAX};

    #[test]
    fn trim_drops_oldest_when_over_cap() {
        let v: Vec<usize> = (0..HIST_BUFFER_MAX + 500).collect();
        assert_eq!(v.len(), HIST_BUFFER_MAX + 500);
        let trimmed = trim_snapshot_history_to_cap(v);
        assert_eq!(trimmed.len(), HIST_BUFFER_MAX);
        // The oldest entries are dropped — we keep [500..1500].
        assert_eq!(trimmed.first(), Some(&500));
        assert_eq!(trimmed.last(), Some(&(HIST_BUFFER_MAX + 500 - 1)));
    }

    #[test]
    fn trim_is_noop_at_or_below_cap() {
        // Empty
        let v: Vec<usize> = Vec::new();
        assert!(trim_snapshot_history_to_cap(v).is_empty());

        // Exactly cap
        let v: Vec<usize> = (0..HIST_BUFFER_MAX).collect();
        let trimmed = trim_snapshot_history_to_cap(v);
        assert_eq!(trimmed.len(), HIST_BUFFER_MAX);

        // One below cap
        let v: Vec<usize> = (0..HIST_BUFFER_MAX - 1).collect();
        let trimmed = trim_snapshot_history_to_cap(v);
        assert_eq!(trimmed.len(), HIST_BUFFER_MAX - 1);
    }

    #[test]
    fn trim_is_uniform_across_tf_durations_and_exchanges() {
        // The trim helper is pure: same input length → same output
        // length, regardless of T or any TF/exchange metadata. We assert
        // this by trimming Vecs of varied sizes (covering the 1s/3s/5s/
        // 15s/60s/180s/300s/900s candle-cadence scenarios) and verifying
        // the cap is applied identically.
        for &n in &[1010usize, 1500, 5000, 100_000] {
            let v: Vec<usize> = (0..n).collect();
            let trimmed = trim_snapshot_history_to_cap(v);
            assert_eq!(
                trimmed.len(),
                HIST_BUFFER_MAX.min(n),
                "trim must cap to HIST_BUFFER_MAX={HIST_BUFFER_MAX} for n={n}"
            );
        }
    }
}

/// Hard cap on the per-pipeline OI history replay length. The live
/// `oi_history: VecDeque<(u64, f64)>` used for `OI Delta` is time-bounded
/// to a 3600 s window at runtime (AUDIT-AIU-051); the cap here bounds the
/// warmup replay so the seeded deque stays within a reasonable memory
/// footprint (one sample per stored candle).
pub const OI_HISTORY_MAX: usize = 60;

/// Hard cap on the per-pipeline funding-rate history replay length.
/// Capped at 8 (vs OI's 60) because funding rate only changes on the
/// 8-hour exchange settlement, so 8 snapshots ≈ a one-week spread.
pub const FUNDING_HISTORY_MAX: usize = 8;

/// Derivatives-warmup snapshot: the state that we replay from historical
/// `MarketSnapshot` rows so derivatives indicators (`open_interest`,
/// `oi_delta`, `funding_rate`, `mark_index_spread`, and the orderbook-
/// derived trio) read with the same statistical grounding at boot as
/// candle-based indicators do.
///
/// The orderbook-derived trio (`order_flow_imbalance`, `spread`,
/// `depth_bias`) cannot be replayed — exchanges don't publish historical
/// orderbook depth. Those keep their existing "Awaiting WS feed"
/// behaviour after this warmup change.
#[derive(Clone, Default)]
pub struct DerivativesWarmedState {
    /// Replayed OI history (oldest → newest) as `(timestamp_secs, value)`,
    /// used to seed `oi_delta`'s rolling 3600 s window (AUDIT-AIU-051).
    /// Length ≤ `OI_HISTORY_MAX`.
    pub oi_history: VecDeque<(u64, f64)>,
    /// Replayed funding rate history (oldest → newest), used to seed
    /// `OI_FUNDING_DIVERGENCE` and `FUNDING_FLIP` signals. Length ≤
    /// `FUNDING_HISTORY_MAX`.
    pub funding_history: VecDeque<f64>,
    /// Latest observed values from the most-recent snapshot. Used to
    /// seed the shared `latest_oi`/`latest_funding`/`latest_mark_px`/
    /// `latest_index_px` locks so the first WS push after boot doesn't
    /// start from `None`. `None` if the DB had no historical rows for
    /// this symbol/TF (cold DB → first-WS-push behaviour preserved).
    pub latest_oi: Option<Decimal>,
    pub latest_funding: Option<Decimal>,
    pub latest_mark_px: Option<Decimal>,
    pub latest_index_px: Option<Decimal>,
}

/// Pre-warmed indicator state produced by feeding historical candles through
/// all technical indicators before live WebSocket ingestion begins.
#[derive(Clone)]
pub struct WarmedPipelineState {
    pub ema_fast: Ema,
    pub ema_medium: Ema,
    pub ema_slow: Ema,
    pub ema_long: Ema,
    pub rsi_14: Rsi,
    pub macd: Macd,
    pub adx_14: Adx,
    pub sqz_mom: SqueezeMomentum,
    pub bollinger: BollingerBands,
    pub atr_standalone: Atr,
    pub bbwp_indicator: Bbwp,
    pub stochastic_indicator: Stochastic,
    pub chandemo_indicator: ChandeMO,
    pub supertrend_indicator: Supertrend,
    pub keltner_indicator: Keltner,
    pub donchian_indicator: Donchian,
    pub obv_indicator: Obv,
    pub cmf_indicator: Cmf,
    pub mfi_indicator: Mfi,
    pub hv_indicator: HistoricalVolatility,
    pub aroon_indicator: Aroon,
    pub choppiness_indicator: Choppiness,
    pub linreg_indicator: LinRegSlope,
    pub zscore_indicator: ZScore,
    pub divergence_detector: DivergenceDetector,
    pub stoch_div: SeriesDivergence,
    pub chandemo_div: SeriesDivergence,
    pub mfi_div: SeriesDivergence,
    pub cmf_div: SeriesDivergence,
    pub obv_div: SeriesDivergence,
    pub squeeze_div: SeriesDivergence,
    pub vwap_sum_tp_vol: Decimal,
    pub vwap_sum_vol: Decimal,
    pub volume_history: VecDeque<Decimal>,
    /// v6.11: rolling ≤300 close window backing the L1 `price_trend_sharpe`.
    /// Filled during replay so a warm-started pipeline has Sharpe values at
    /// the first live close.
    pub close_history: VecDeque<f64>,
    pub history: Vec<NormalizedCandle>,
    pub last_day_index: Option<u64>,
    pub sr_tracker: SrRoleTracker,
    pub pivot_points_indicator: PivotPoints,
    pub candlestick_indicator: Candlestick,
    pub ichimoku_indicator: Ichimoku,
    pub cci_indicator: Cci,
    pub psar_indicator: ParabolicSar,
    pub wr_indicator: WilliamsR,
    pub hma_indicator: HullMA,
    pub ao_indicator: AwesomeOscillator,
    pub fi_indicator: ForceIndex,
    pub sdc_indicator: StdDevChannel,
    pub volume_profile_indicator: VolumeProfile,
    pub smc_indicator: SmartMoney,
    pub anchored_vwap_indicator: AnchoredVwap,
    pub latest_snapshot: Option<MarketSnapshot>,
    pub snapshot_history: Vec<MarketSnapshot>,
    /// Replayed derivatives state (`oi_history`/`funding_history` plus
    /// latest values) for the `open_interest`/`oi_delta`/`funding_rate`/
    /// `mark_index_spread` indicator path. Fed back into the analyzer's
    /// shared locks during `populate_buffers` so derivatives read with
    /// the same statistical grounding as candle-based indicators at
    /// boot. Default-constructed when no historical snapshots exist.
    pub derivatives_state: DerivativesWarmedState,
}

/// Replay derivatives state from a chronological (oldest → newest)
/// sequence of historical `MarketSnapshot` rows for the symbol/timeframe
/// being warmed. Returns a `DerivativesWarmedState` whose fields mirror
/// what the live runtime populates on the first WS push:
/// - `oi_history`: bounded to `OI_HISTORY_MAX` recent OI samples (f64)
///   so `OI Delta`'s rolling math seeds with real data instead of zero.
/// - `funding_history`: bounded to `FUNDING_HISTORY_MAX` recent funding
///   samples so `OI_FUNDING_DIVERGENCE` and `FUNDING_FLIP` have non-zero
///   history at boot.
/// - `latest_oi`/`latest_funding`/`latest_mark_px`/`latest_index_px`:
///   the most-recent values, used to seed the shared per-pair locks so
///   the WS handler doesn't start from `None`.
///
/// Snapshots whose top-level `open_interest`/`funding_rate`/`mark_price`/
/// `index_price` fields are all `None` contribute nothing (they're the
/// audit-V6-301 "phase-3 writer pending" rows). An empty input vec
/// returns `DerivativesWarmedState::default()` so cold-DB installs
/// preserve today's first-WS-push behaviour.
pub fn warm_derivatives_from_snapshots(
    snapshots: &[MarketSnapshot],
    buffer_size: usize,
) -> DerivativesWarmedState {
    let oi_cap = buffer_size.min(OI_HISTORY_MAX);
    let funding_cap = buffer_size.min(FUNDING_HISTORY_MAX);

    let mut out = DerivativesWarmedState::default();

    if snapshots.is_empty() {
        return out;
    }

    // Take the most-recent `oi_cap` snapshots for the OI replay window.
    // AUDIT-AIU-051: samples are stored as `(timestamp_secs, value)` so the
    // live 3600 s window can be seeded and evaluated per-TF.
    let oi_window_start = snapshots.len().saturating_sub(oi_cap);
    for snap in &snapshots[oi_window_start..] {
        if let Some(oi) = snap.open_interest.and_then(|d| d.to_f64()) {
            out.oi_history.push_back((snap.timestamp, oi));
        }
    }

    // Same sliding-window approach for the funding-rate replay.
    let funding_window_start = snapshots.len().saturating_sub(funding_cap);
    for snap in &snapshots[funding_window_start..] {
        if let Some(f) = snap.funding_rate.and_then(|d| d.to_f64()) {
            out.funding_history.push_back(f);
        }
    }

    // Latest values: the most-recent snapshot where each field is set.
    // Walk backwards so a partial row in the latest few slots doesn't
    // blank the field.
    for snap in snapshots.iter().rev() {
        if out.latest_oi.is_none() {
            out.latest_oi = snap.open_interest;
        }
        if out.latest_funding.is_none() {
            out.latest_funding = snap.funding_rate;
        }
        if out.latest_mark_px.is_none() {
            out.latest_mark_px = snap.mark_price;
        }
        if out.latest_index_px.is_none() {
            out.latest_index_px = snap.index_price;
        }
        if out.latest_oi.is_some()
            && out.latest_funding.is_some()
            && out.latest_mark_px.is_some()
            && out.latest_index_px.is_some()
        {
            break;
        }
    }

    out
}

/// Feed a sequence of historical candles through all technical indicators in
/// chronological order, returning fully warmed state ready for live ingestion.
///
/// `buffer_size` is the canonical `[candle_buffer] size` from `config.toml`
/// (CB-01). It replaces the per-instance `analysis_limit` field that lived on
/// `TimeframeConfig.candles` before v6.5.
///
/// Derivatives warmup (`open_interest` / `oi_delta` / `funding_rate` /
/// `mark_index_spread`) is replayed from the `snapshot_history` already
/// built by this warm loop, which mirrors what the bootstrap path
/// persisted in `market_snapshots.auxiliary_normalized_data` (the
/// `MarketSnapshot.top_level` derivatives fields on each row). Cold
/// databases — where no historical rows exist — produce a default
/// `DerivativesWarmedState`, preserving the first-WS-push behaviour.
pub fn warm_indicators_for_timeframe(
    mut candles: Vec<NormalizedCandle>,
    tf_config: &TimeframeConfig,
    fib_config: &FibonacciConfig,
    symbol: &str,
    timeframe_secs: u64,
    slot_label: String,
    buffer_size: usize,
    active_set: &crate::active_set::ActiveSet,
    // AUDIT-H5: the venue for the pre-warm snapshots (was hardcoded
    // Hyperliquid). `None` for callers that don't know the venue yet —
    // the snapshot then reports no exchange instead of a wrong one.
    exchange: Option<Exchange>,
) -> WarmedPipelineState {
    let active_indicators = tf_config.indicators.clone();

    let mut ema_fast = Ema::new(active_indicators.ema_fast);
    let mut ema_medium = Ema::new(active_indicators.ema_medium);
    let mut ema_slow = Ema::new(active_indicators.ema_slow);
    let mut ema_long = Ema::new(active_indicators.ema_long);
    let mut rsi_14 = Rsi::new(active_indicators.rsi_period);

    let mut macd = Macd::new();
    let mut adx_14 = Adx::new(active_indicators.adx_period);
    adx_14.set_thresholds(
        Decimal::from(active_indicators.adx_trend_threshold),
        Decimal::from(active_indicators.adx_exhaustion_threshold),
        active_indicators.adx_slope_lookback,
    );
    let mut sqz_mom = SqueezeMomentum::new(active_indicators.squeeze_period);
    sqz_mom.set_min_duration(active_indicators.squeeze_min_duration);
    let mut bollinger = BollingerBands::new(20);
    let mut atr_standalone = Atr::new(active_indicators.atr_period);
    let mut bbwp_indicator = Bbwp::new(
        active_indicators.bbwp_lookback,
        active_indicators.bbwp_period,
    );
    let mut stochastic_indicator = Stochastic::new(
        active_indicators.stoch_k_period,
        active_indicators.stoch_d_period,
        active_indicators.stoch_s_period,
    );
    let mut chandemo_indicator = ChandeMO::new(active_indicators.chandemo_period);
    let mut supertrend_indicator = Supertrend::new(
        active_indicators.supertrend_period,
        active_indicators.supertrend_multiplier,
    );
    let mut keltner_indicator = Keltner::new(
        active_indicators.keltner_ema_period,
        active_indicators.keltner_atr_period,
        active_indicators.keltner_multiplier,
    );
    let mut donchian_indicator = Donchian::new(active_indicators.donchian_period);
    let mut obv_indicator = Obv::new(active_indicators.obv_smoothing);
    let mut cmf_indicator = Cmf::new(active_indicators.cmf_period);
    let mut mfi_indicator = Mfi::new(active_indicators.mfi_period);
    let mut hv_indicator = HistoricalVolatility::new(active_indicators.hv_period);
    let mut aroon_indicator = Aroon::new(active_indicators.aroon_period);
    let mut choppiness_indicator = Choppiness::new(active_indicators.chop_period);
    let mut linreg_indicator = LinRegSlope::new(active_indicators.linreg_period);
    let mut zscore_indicator = ZScore::new(active_indicators.zscore_period);
    let mut divergence_detector = DivergenceDetector::new(20);
    let mut stoch_div = SeriesDivergence::new(20);
    let mut chandemo_div = SeriesDivergence::new(20);
    let mut mfi_div = SeriesDivergence::new(20);
    let mut cmf_div = SeriesDivergence::new(20);
    let mut obv_div = SeriesDivergence::new(20);
    let mut squeeze_div = SeriesDivergence::new(20);

    let mut vwap_sum_tp_vol = Decimal::ZERO;
    let mut vwap_sum_vol = Decimal::ZERO;
    let mut last_day_index: Option<u64> = None;
    let mut volume_history: VecDeque<Decimal> =
        VecDeque::with_capacity(active_indicators.volume_average_period);
    // v6.11: Sharpe windows — replayed real closes only (PRI-06), capped at
    // the canonical 300-bar window so a warm-started pipeline has values at
    // the first live close.
    let mut close_history: VecDeque<f64> =
        VecDeque::with_capacity(crate::indicators::ratio::SHARPE_WINDOW);
    // S/R role-reversal tracker, warmed through the full history so live
    // ingestion inherits accurate flip-state (matches run_single tolerance).
    let mut sr_tracker = SrRoleTracker::new(0.003);
    // Session Pivot Points, warmed so live ingestion inherits published levels.
    let mut pivot_points_indicator = PivotPoints::new(PivotMethod::from_str_lenient(
        &active_indicators.pivot_points_method,
    )); // AUDIT-AIU-072
        // Candlestick recognizer, warmed so its pending-confirmation buffer is live.
    let mut candlestick_indicator = Candlestick::new(CandlestickConfig::default());
    // Ichimoku Cloud, warmed so live ingestion inherits the 52-bar window.
    let mut ichimoku_indicator = Ichimoku::new(
        active_indicators.ichimoku_tenkan,
        active_indicators.ichimoku_kijun,
        active_indicators.ichimoku_senkou_b,
        active_indicators.ichimoku_displacement,
    );
    let mut cci_indicator = Cci::new(active_indicators.cci_period);
    let mut psar_indicator = ParabolicSar::new(
        active_indicators.psar_af_step,
        active_indicators.psar_af_max,
    );
    let mut wr_indicator = WilliamsR::new(active_indicators.williams_r_period);
    let mut hma_indicator = HullMA::new(active_indicators.hull_ma_period);
    let mut ao_indicator = AwesomeOscillator::new();
    let mut fi_indicator = ForceIndex::new(active_indicators.force_index_smoothing);
    let mut sdc_indicator = StdDevChannel::new(active_indicators.stddev_channel_period);
    let mut volume_profile_indicator = VolumeProfile::new(
        active_indicators.volume_profile_window,
        active_indicators.volume_profile_bins,
        active_indicators.volume_profile_value_area,
    );
    let mut smc_indicator = SmartMoney::new(active_indicators.smc_lookback);

    let mut anchored_vwap_indicator = AnchoredVwap::new();

    let mut latest_snapshot: Option<MarketSnapshot> = None;

    // Sort candles chronologically (oldest first)
    candles.sort_by_key(|c| c.start_time_ms);

    let analysis_limit = buffer_size;
    let mut snapshot_history: Vec<MarketSnapshot> = Vec::with_capacity(analysis_limit);

    // Feed each historical candle through every indicator sequentially
    for completed in &candles {
        let candle_close_sec = completed.start_time_ms / 1000;
        let day_index = candle_close_sec / 86400;
        if let Some(prev_day) = last_day_index {
            if day_index > prev_day {
                vwap_sum_tp_vol = Decimal::ZERO;
                vwap_sum_vol = Decimal::ZERO;
            }
        }
        last_day_index = Some(day_index);

        // ── f64 batch inputs for indicator update calls ──
        let open_f = completed.open.to_f64().unwrap_or(0.0);
        let high_f = completed.high.to_f64().unwrap_or(0.0);
        let low_f = completed.low.to_f64().unwrap_or(0.0);
        let close_f = completed.close.to_f64().unwrap_or(0.0);
        let volume_f = completed.volume.to_f64().unwrap_or(0.0);

        // Session Pivot Points: accumulate H/L/C; publish on day rollover.
        let pivot_levels = pivot_points_indicator.update(high_f, low_f, close_f, day_index);

        // Candlestick recognition (warmed through history).
        let candlestick_reading = candlestick_indicator.update(open_f, high_f, low_f, close_f);

        // Ichimoku Cloud (warmed through history).
        // Soft-floor (min_bars=9) mirrors the Volume Profile pattern below
        // and Hull MA's soft-floor variant — the strict `update()` returns
        // `None` until `senkou_b_period=52` candles are accumulated, which
        // would otherwise leave the seeded history producing no Ichimoku
        // reading on sub-minute TFs whose venue-capped fetch falls short of
        // 52 bars. With min_bars=9 (the smallest configured window),
        // `update_with_min_bars` produces a partial reading that converges
        // to the strict result once the live path takes over.
        // AUDIT-AIU-004: single soft-floor call — the previous
        // `.update().or_else(|| update_with_min_bars(…))` chain double-pushed
        // the same bar during warmup (update() pushes before returning None),
        // corrupting the ichimoku rolling windows until the deque cycled.
        let ichimoku_reading = ichimoku_indicator.update_with_min_bars(high_f, low_f, close_f, 9);

        // CCI (warmed through history).
        let cci_reading = cci_indicator.update(high_f, low_f, close_f);

        // Parabolic SAR (warmed through history).
        let psar_reading = psar_indicator.update(high_f, low_f);

        let wr_reading = wr_indicator.update(high_f, low_f, close_f);
        // Hull MA soft-floor: the strict `update()` returns `None` until
        // `hull_ma_period` bars are accumulated. Sub-minute timeframes whose
        // historical fetch was bypassed (HFP-03) or capped below the
        // configured period would otherwise stay stuck in `WARMING`. The
        // soft-floor mirrors Volume Profile's `compute_with_min_bars(25)`
        // pattern (see this file ~L256) — the warm path uses a relaxed floor
        // (sqrt(period) ≈ 5 for period=21) so the seeded history produces a
        // partial Hull MA reading that converges to the strict reading once
        // the live path takes over and `values.len() >= period`.
        // AUDIT-AIU-005: single soft-floor call (fixes the same double-push
        // corruption class as ichimoku above).
        let hma_reading = hma_indicator.update_with_min_bars(close_f, 5);
        let ao_reading = ao_indicator.update(high_f, low_f);
        let fi_reading = fi_indicator.update(close_f, volume_f);
        let sdc_reading = sdc_indicator.update(close_f);

        let volume_profile_reading =
            volume_profile_indicator.update_with_open(high_f, low_f, open_f, close_f, volume_f);
        // Per-warm-candle bin snapshot — same source-of-truth builder as the
        // live per-candle path uses (see `super::build_volume_profile_snapshot`).
        // The strict `window_size / 2` gate is preserved for *live* correctness
        // (no half-formed profiles), but the *seeded* path passes a relaxed
        // floor of 25 bars so sub-minute TFs (where the venue returns only
        // 26–51 bars of history regardless of `analysis_limit`) still paint a
        // bin distribution on first mount, just like every other indicator.
        // For the same reason we re-derive the reading with the relaxed gate
        // when `update_with_open` returned None below the strict gate.
        let seeded_reading: Option<crate::indicators::VolumeProfileOutput> =
            if volume_profile_reading.is_some() {
                volume_profile_reading.clone()
            } else {
                volume_profile_indicator.compute_with_min_bars(25)
            };
        let volume_profile_snapshot = super::build_volume_profile_snapshot(
            symbol,
            &slot_label,
            timeframe_secs,
            &seeded_reading,
            volume_profile_indicator
                .compute_bins_with_min_bars(25)
                .as_ref(),
            completed.start_time_ms,
            volume_profile_indicator.value_area_pct(),
        );
        let smc_reading = smc_indicator.update(open_f, high_f, low_f, close_f);

        let typical_price = (completed.high + completed.low + completed.close) / Decimal::from(3);
        vwap_sum_tp_vol += typical_price * completed.volume;
        vwap_sum_vol += completed.volume;

        let final_vwap = if vwap_sum_vol > Decimal::ZERO {
            Some(vwap_sum_tp_vol / vwap_sum_vol)
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

        let extra_div = ExtraDivergence {
            stochastic: final_stoch
                .as_ref()
                .map(|s| {
                    series_divergence_state(
                        &stoch_div.update(close_f, s.k_value.to_f64().unwrap_or(0.0)),
                    )
                })
                .unwrap_or_default(),
            chandemo: final_cmo
                .map(|v| {
                    series_divergence_state(
                        &chandemo_div.update(close_f, v.to_f64().unwrap_or(0.0)),
                    )
                })
                .unwrap_or_default(),
            mfi: final_mfi
                .map(|v| {
                    series_divergence_state(&mfi_div.update(close_f, v.to_f64().unwrap_or(0.0)))
                })
                .unwrap_or_default(),
            cmf: final_cmf
                .map(|v| {
                    series_divergence_state(&cmf_div.update(close_f, v.to_f64().unwrap_or(0.0)))
                })
                .unwrap_or_default(),
            obv: final_obv
                .as_ref()
                .map(|o| {
                    series_divergence_state(&obv_div.update(close_f, o.obv.to_f64().unwrap_or(0.0)))
                })
                .unwrap_or_default(),
            squeeze: final_sqz
                .as_ref()
                .map(|s| {
                    series_divergence_state(
                        &squeeze_div.update(close_f, s.momentum_value.to_f64().unwrap_or(0.0)),
                    )
                })
                .unwrap_or_default(),
        };

        let div_result = if let (Some(rsi), macd_hist) = (final_rsi, final_macd.histogram) {
            divergence_detector.update_full(
                close_f,
                rsi.to_f64().unwrap_or(0.0),
                macd_hist.to_f64().unwrap_or(0.0),
            )
        } else {
            crate::indicators::DivergenceResult::default_div()
        };
        let rsi_divergence = crate::analyzer::normalize::rsi_divergence_state(&div_result);
        let macd_divergence = crate::analyzer::normalize::macd_divergence_state(&div_result);

        volume_history.push_back(completed.volume);
        // AUDIT-AIU-070: honor the configured `volume_average_period`
        // window (was hardcoded 20).
        let vol_window = active_indicators.volume_average_period.max(1);
        while volume_history.len() > vol_window {
            volume_history.pop_front();
        }

        // v6.11: roll the Sharpe window with this replayed real close.
        close_history.push_back(close_f);
        while close_history.len() > crate::indicators::ratio::SHARPE_WINDOW {
            close_history.pop_front();
        }

        // Build snapshot using only the history accumulated so far
        let snapshot = build_historical_snapshot(
            &candles,
            completed,
            symbol,
            timeframe_secs,
            slot_label.clone(),
            final_vwap,
            avwap_reading,
            final_ema_fast,
            final_ema_medium,
            final_ema_slow,
            final_ema_long,
            ema_stack_state,
            // AUDIT-V8-001: configured EMA periods for per-line ribbon gating.
            (
                active_indicators.ema_fast,
                active_indicators.ema_medium,
                active_indicators.ema_slow,
                active_indicators.ema_long,
            ),
            final_rsi,
            &final_macd,
            final_adx.as_ref(),
            final_sqz.as_ref(),
            final_bb,
            final_atr.as_ref(),
            final_bbwp,
            final_stoch.as_ref(),
            final_cmo,
            final_supertrend.as_ref(),
            final_keltner.as_ref(),
            final_donchian.as_ref(),
            final_obv.as_ref(),
            final_cmf,
            final_mfi,
            final_hv,
            final_aroon.as_ref(),
            final_chop,
            final_linreg,
            final_zscore,
            extra_div,
            rsi_divergence,
            macd_divergence,
            &volume_history,
            &close_history,
            fib_config,
            &mut sr_tracker,
            pivot_levels,
            Some(candlestick_reading),
            ichimoku_reading,
            cci_reading,
            psar_reading,
            wr_reading,
            hma_reading,
            ao_reading,
            fi_reading,
            sdc_reading,
            volume_profile_reading,
            smc_reading,
            volume_profile_snapshot,
            // AUDIT-H5: thread the real venue + pipeline state through —
            // previously the warm snapshot hardcoded `Exchange::Hyperliquid`
            // and `CandlePipelineState::default()` (Initializing), so a
            // fully-warmed Bitget pipeline flashed the wrong exchange and an
            // "Initializing" banner on WS bootstrap. Warm snapshots are
            // seeded all-enabled; the ActiveSet denylist is applied at the
            // consume point (run_single) so the seeded calculators stay warm.
            exchange,
            derive_pipeline_state(snapshot_history.len(), buffer_size),
            active_set,
        );

        latest_snapshot = Some(snapshot.clone());
        snapshot_history.push(snapshot);
    }

    // The raw-candle `history` is bounded by the bootstrap's effective seed cap
    // (`buffer_size` = `[candle_buffer] size`), not by the raw fetch count —
    // for ≥ 1 minute TFs the bootstrap paginates up to exactly `buffer_size`
    // (CB-08), for sub-minute TFs the warm `history` is the 60 s state-replay
    // set (PRI-03) and is NOT pushed into the pipeline's `history` deque at
    // handover (AUDIT-AIU-117 — the replayed bars are 12× too wide for the
    // slot's structural indicators; live candles fill the deque). The
    // downstream live path trims to `buffer_size` on each new candle close
    // anyway.
    let seed_cap = analysis_limit.max(crate::analyzer::warm::HIST_BUFFER_MAX);
    let history: Vec<NormalizedCandle> = if candles.len() > seed_cap {
        candles[candles.len() - seed_cap..].to_vec()
    } else {
        candles
    };

    snapshot_history = trim_snapshot_history_to_cap(snapshot_history);

    // Clone once so we can both move `snapshot_history` into the struct
    // literal AND borrow it for the derivatives warmup.
    let trimmed_snapshot_history = snapshot_history.clone();

    WarmedPipelineState {
        ema_fast,
        ema_medium,
        ema_slow,
        ema_long,
        rsi_14,
        macd,
        adx_14,
        sqz_mom,
        bollinger,
        atr_standalone,
        bbwp_indicator,
        stochastic_indicator,
        chandemo_indicator,
        supertrend_indicator,
        keltner_indicator,
        donchian_indicator,
        obv_indicator,
        cmf_indicator,
        mfi_indicator,
        hv_indicator,
        aroon_indicator,
        choppiness_indicator,
        linreg_indicator,
        zscore_indicator,
        divergence_detector,
        stoch_div,
        chandemo_div,
        mfi_div,
        cmf_div,
        obv_div,
        squeeze_div,
        vwap_sum_tp_vol,
        vwap_sum_vol,
        volume_history,
        close_history,
        history,
        last_day_index,
        sr_tracker,
        pivot_points_indicator,
        candlestick_indicator,
        ichimoku_indicator,
        cci_indicator,
        psar_indicator,
        wr_indicator,
        hma_indicator,
        ao_indicator,
        fi_indicator,
        sdc_indicator,
        volume_profile_indicator,
        smc_indicator,
        anchored_vwap_indicator,
        latest_snapshot,
        snapshot_history: trimmed_snapshot_history.clone(),
        derivatives_state: warm_derivatives_from_snapshots(&trimmed_snapshot_history, buffer_size),
    }
}

/// Build a `MarketSnapshot` from indicator state during historical pre-warming.
///
/// v6.10 (Phase 5 / E1): active_set is passed so disabled indicators
/// are filtered out of the warmed snapshot. Defaults to all-enabled
/// when the function is called from warm-up paths that don't have
/// a per-instance ActiveSet yet.
#[allow(clippy::too_many_arguments)]
fn build_historical_snapshot(
    all_candles: &[NormalizedCandle],
    completed: &NormalizedCandle,
    symbol: &str,
    timeframe_secs: u64,
    slot_label: String,
    final_vwap: Option<Decimal>,
    avwap_reading: crate::indicators::AvwapOutput,
    final_ema_fast: Decimal,
    final_ema_medium: Decimal,
    final_ema_slow: Decimal,
    final_ema_long: Decimal,
    ema_stack_state: Option<String>,
    // Configured EMA periods `(fast, medium, slow, long)` — per-line
    // ribbon availability gate (AUDIT-V8-001).
    ema_periods: (usize, usize, usize, usize),
    final_rsi: Option<Decimal>,
    final_macd: &crate::indicators::MacdOutput,
    final_adx: Option<&crate::indicators::AdxOutput>,
    final_sqz: Option<&crate::indicators::SqueezeOutput>,
    final_bb: Option<(Decimal, Decimal, Decimal)>,
    final_atr: Option<&crate::indicators::AtrOutput>,
    final_bbwp: Option<Decimal>,
    final_stoch: Option<&crate::indicators::StochasticOutput>,
    final_cmo: Option<Decimal>,
    final_supertrend: Option<&crate::indicators::SupertrendOutput>,
    final_keltner: Option<&crate::indicators::KeltnerOutput>,
    final_donchian: Option<&crate::indicators::DonchianOutput>,
    final_obv: Option<&crate::indicators::ObvOutput>,
    final_cmf: Option<Decimal>,
    final_mfi: Option<Decimal>,
    final_hv: Option<f64>,
    final_aroon: Option<&crate::indicators::AroonOutput>,
    final_chop: Option<Decimal>,
    final_linreg: Option<f64>,
    final_zscore: Option<f64>,
    extra_div: ExtraDivergence,
    rsi_divergence: crate::indicators::DivergenceState,
    macd_divergence: crate::indicators::DivergenceState,
    volume_history: &VecDeque<Decimal>,
    // v6.11: replayed Sharpe window — used to derive the L1
    // `price_trend_sharpe` for the pre-warm snapshot (same math as the
    // live path).
    close_history: &VecDeque<f64>,
    fib_config: &FibonacciConfig,
    sr_tracker: &mut SrRoleTracker,
    pivot_levels: Option<crate::indicators::PivotLevels>,
    candlestick: Option<crate::indicators::CandlestickResult>,
    ichimoku: Option<crate::indicators::IchimokuOutput>,
    cci: Option<Decimal>,
    psar: Option<crate::indicators::PsarOutput>,
    wr: Option<Decimal>,
    hma: Option<Decimal>,
    ao: Option<crate::indicators::AoOutput>,
    fi: Option<Decimal>,
    sdc: Option<crate::indicators::SdChannelOutput>,
    volume_profile: Option<crate::indicators::VolumeProfileOutput>,
    smc: Option<crate::indicators::SmcOutput>,
    volume_profile_snapshot: Option<VolumeProfileSnapshot>,
    // AUDIT-H5: the warm-up path must not stamp the wrong venue onto the
    // pre-warm snapshot (it hardcoded `Exchange::Hyperliquid`, so Bitget
    // symbols reported the wrong exchange on WS bootstrap + /api/history
    // until the first live candle replaced the snapshot).
    shadow_exchange: Option<Exchange>,
    pipeline_state: CandlePipelineState,
    active_set: &crate::active_set::ActiveSet,
) -> MarketSnapshot {
    let candle_close_sec = completed.start_time_ms / 1000;

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

    let candles_high: Vec<Decimal> = all_candles.iter().map(|c| c.high).collect();
    let candles_low: Vec<Decimal> = all_candles.iter().map(|c| c.low).collect();

    let fib = FibonacciRange::compute_from_candles(
        &candles_high,
        &candles_low,
        fib_config.swing_lookback,
        fib_config.swing_scan_range,
        &fib_config.retracement_coefficients,
        &fib_config.extension_coefficients,
    );

    let pivots = FibonacciRange::detect_pivots(
        &candles_high,
        &candles_low,
        fib_config.swing_lookback,
        fib_config.swing_scan_range,
    );
    let pattern_result = detect_pattern(&pivots);

    // Support/Resistance zones: role-adjusted levels from the swing pivots,
    // updating the flip tracker on this historical close.
    let (sr_supports, sr_resistances) =
        update_sr_levels(sr_tracker, &pivots, completed.close, candle_close_sec);

    let indicators = crate::analyzer::normalize::build_indicator_map(
        crate::analyzer::normalize::NormalizeParams {
            close: completed.close,
            rsi: final_rsi,
            rsi_divergence,
            macd_divergence,
            stoch_k: final_stoch.map(|s| s.k_value),
            stoch_d: final_stoch.map(|s| s.d_value),
            chandemo: final_cmo,
            supertrend_line: final_supertrend.map(|s| s.line),
            supertrend_dir: final_supertrend.map(|s| s.direction),
            keltner: final_keltner.map(|k| (k.upper, k.middle, k.lower)),
            donchian: final_donchian.map(|d| (d.upper, d.middle, d.lower)),
            obv: final_obv.map(|o| o.obv),
            obv_sma: final_obv.map(|o| o.obv_sma),
            cmf: final_cmf,
            mfi: final_mfi,
            hv: final_hv,
            aroon_up: final_aroon.map(|a| a.up),
            aroon_down: final_aroon.map(|a| a.down),
            choppiness: final_chop,
            linreg_slope: final_linreg,
            zscore: final_zscore,
            extra_div,
            macd: final_macd,
            sqz: final_sqz,
            adx: final_adx,
            bb: final_bb,
            atr: final_atr,
            bbwp: final_bbwp,
            vwap: final_vwap,
            anchored_vwap: Some(avwap_reading),
            ema_stack_state: ema_stack_state.as_deref(),
            ema_fast: Some(final_ema_fast),
            ema_medium: Some(final_ema_medium),
            ema_slow: Some(final_ema_slow),
            ema_long: Some(final_ema_long),
            ema_periods,
            rvol,
            volume: Some(completed.volume),
            average_volume: avg_vol,
            fib: Some(&fib),
            pattern: Some(&pattern_result),
            support_levels: &sr_supports,
            resistance_levels: &sr_resistances,
            active_position: None,
            adx_consecutive_deceleration: false,
            supertrend_flipped: false,
            adx_di_crossover: None,
            pivot_levels,
            pivot_proximity_pct: 0.0015,
            candlestick,
            // AUDIT-AIU-073: config default (historical builder has no config).
            candlestick_min_confidence: 0.3,
            ichimoku,
            cci,
            psar,
            williams_r: wr,
            awesome_oscillator: ao,
            force_index: fi,
            force_index_mean_abs: None,
            hull_ma: hma,
            stddev_channel: sdc,
            volume_profile,
            smc,
            prev: PreviousBarState::default(),
            // AUDIT-AIU-071: rvol thresholds — config defaults (this
            // historical-snapshot builder carries no tf_config).
            rvol_institutional_threshold: 1.5,
            rvol_climax_threshold: 3.0,
            // v6.11: derive the L1 ratio from the replayed window.
            price_trend_sharpe: {
                let closes: Vec<f64> = close_history.iter().copied().collect();
                crate::indicators::sharpe_ratio_annualized(&closes, timeframe_secs)
            },
        },
        all_candles.len() as u32,
        false,
        active_set,
    );

    MarketSnapshot {
        timeframe_label: Some(slot_label.clone()),
        exchange: shadow_exchange,
        timeframe_secs,
        timestamp: candle_close_sec,
        symbol: symbol.to_string(),
        is_completed: Some(true),
        // Warm-up snapshots seed internal state only (never broadcast):
        // emit the retired order-book contract neutrally — candle close as
        // reference mid, no depth sizes (the old `Some(volume)` mislabeled
        // candle volume as top-of-book depth).
        mid_price: completed.close,
        bid_price: Decimal::ZERO,
        ask_price: Decimal::ZERO,
        bid_size: None,
        ask_size: None,
        funding_rate: None,
        open_interest: None,
        oi_delta_1h: None,
        mark_price: None,
        index_price: None,
        mark_index_spread_pct: None,
        prev_day_px: None,
        open: Some(completed.open),
        high: Some(completed.high),
        low: Some(completed.low),
        close: Some(completed.close),
        volume: Some(completed.volume),
        average_volume: avg_vol,
        pipeline_state,
        indicator_lifecycle: std::collections::HashMap::new(),
        context: Some(crate::market_context_synth::synthesize_market_context(
            &indicators,
            // v9: warm snapshots are transient (replaced at the first live
            // close) — the strategy's L1 section applies from then on.
            None,
        )),
        // AUDIT-H5: the warm-up cycle has no L3/L4/L5 wiring — the previous
        // code fed empty Analysis/Risk matrices into `DecisionContext::compute`
        // and served the resulting zeroed guidance (readiness, probabilities,
        // R:R) on WS bootstrap. Honest `None` — the first live close computes
        // the real decision context a candle later.
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
        volume_profile: volume_profile_snapshot,
        quality_envelope: None,
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the derivatives warmup path. Asserts that
    //! `warm_derivatives_from_snapshots` correctly replays the
    //! `oi_history`/`funding_history` rolling buffers and the
    //! `latest_*` Decimal fields from chronological `MarketSnapshot`
    //! rows so that derivatives indicators (`open_interest`,
    //! `oi_delta`, `funding_rate`, `mark_index_spread`) read with
    //! non-zero priors at boot instead of starting from `None` for
    //! the first WS frame.
    use super::*;
    use core_domain::models::{CandlePipelineState, MarketSnapshot};
    use rust_decimal_macros::dec;

    fn make_snap_with_derivs(
        ts_ms: u64,
        oi: Option<Decimal>,
        funding: Option<Decimal>,
        mark: Option<Decimal>,
        index: Option<Decimal>,
    ) -> MarketSnapshot {
        // Mints a minimal MarketSnapshot that the warmup helper can
        // consume.
        MarketSnapshot {
            exchange: Some(core_domain::normalized::Exchange::Bitget),
            timeframe_secs: 60,
            timestamp: ts_ms / 1000,
            symbol: "BTC-USDT".into(),
            mid_price: Decimal::from(50_000),
            open: Some(Decimal::from(50_000)),
            high: Some(Decimal::from(50_000)),
            low: Some(Decimal::from(50_000)),
            close: Some(Decimal::from(50_000)),
            volume: Some(Decimal::ZERO),
            bid_size: None,
            ask_size: None,
            bid_price: Decimal::ZERO,
            ask_price: Decimal::ZERO,
            average_volume: None,
            is_completed: Some(true),
            open_interest: oi,
            funding_rate: funding,
            mark_price: mark,
            index_price: index,
            mark_index_spread_pct: None,
            oi_delta_1h: None,
            prev_day_px: None,
            pipeline_state: CandlePipelineState::Loading,
            timeframe_label: Some("1s".to_string()),
            indicator_lifecycle: std::collections::HashMap::new(),
            indicators: Default::default(),
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
            context: None,
            decision_context: None,
            statistical_context: None,
        }
    }

    #[test]
    fn derivatives_warmup_with_empty_history_yields_default_state() {
        let state = warm_derivatives_from_snapshots(&[], 500);
        assert!(state.oi_history.is_empty());
        assert!(state.funding_history.is_empty());
        assert!(state.latest_oi.is_none());
        assert!(state.latest_funding.is_none());
        assert!(state.latest_mark_px.is_none());
        assert!(state.latest_index_px.is_none());
    }

    #[test]
    fn derivatives_warmup_seeds_oi_history_with_recent_60_samples() {
        // Generate 100 rows of monotonically increasing OI. Warmup should
        // keep only the most recent 60 (= OI_HISTORY_MAX).
        let snapshots: Vec<MarketSnapshot> = (0..100u64)
            .map(|i| {
                let ts = 1_700_000_000_000 + i * 60_000;
                let oi = dec!(100_000_000) + Decimal::from(i) * dec!(1_000);
                make_snap_with_derivs(ts, Some(oi), None, None, None)
            })
            .collect();
        let state = warm_derivatives_from_snapshots(&snapshots, 500);
        assert_eq!(
            state.oi_history.len(),
            60,
            "oi_history must cap at OI_HISTORY_MAX = 60"
        );
        // The most-recent 60 snapshots. The 99th sample corresponds to
        // 100_000_000 + 99_000 = 100_099_000; the 40th (= 100 - 60) is
        // 100_040_000. `snap.timestamp` is in seconds (see
        // `make_snap_with_derivs` line ~1059: `timestamp: ts_ms / 1000`),
        // so the 99th-sample timestamp is `1_700_000_000 + 99 * 60 =
        // 1_700_005_940` and the 40th is `1_700_000_000 + 40 * 60 =
        // 1_700_002_400`.
        assert_eq!(
            state.oi_history.back().copied(),
            Some((1_700_005_940_u64, 100_099_000.0)),
            "back of oi_history should be the most recent sample"
        );
        assert_eq!(
            state.oi_history.front().copied(),
            Some((1_700_002_400_u64, 100_040_000.0)),
            "front of oi_history should be the (len-60)th sample"
        );
        assert!(state.latest_oi.is_some(), "latest_oi should be populated");
    }

    #[test]
    fn derivatives_warmup_seeds_funding_history_with_recent_8_samples() {
        // 50 rows of funding rate. Warmup should keep only the most
        // recent 8 (= FUNDING_HISTORY_MAX).
        let snapshots: Vec<MarketSnapshot> = (0..50u64)
            .map(|i| {
                let ts = 1_700_000_000_000 + i * 60_000;
                let f = dec!(0.0001) + Decimal::from(i) * dec!(0.00001);
                make_snap_with_derivs(ts, None, Some(f), None, None)
            })
            .collect();
        let state = warm_derivatives_from_snapshots(&snapshots, 500);
        assert_eq!(
            state.funding_history.len(),
            8,
            "funding_history must cap at FUNDING_HISTORY_MAX = 8"
        );
    }

    #[test]
    fn derivatives_warmup_with_buffer_size_smaller_than_max_uses_buffer_size() {
        // buffer_size = 10 caps replay to 10 samples even though max
        // would allow 60.
        let snapshots: Vec<MarketSnapshot> = (0..100u64)
            .map(|i| {
                let ts = 1_700_000_000_000 + i * 60_000;
                make_snap_with_derivs(ts, Some(Decimal::from(100_000 + i)), None, None, None)
            })
            .collect();
        let state = warm_derivatives_from_snapshots(&snapshots, 10);
        assert_eq!(state.oi_history.len(), 10);
        // The first kept sample is the 90th (i = 100 - 10), so
        // `snap.timestamp = (1_700_000_000_000 + 90 * 60_000) / 1000 =
        // 1_700_005_400` seconds and the OI value is `100_000 + 90 =
        // 100_090`.
        assert_eq!(
            state.oi_history.front().copied(),
            Some((1_700_005_400_u64, 100_090.0))
        );
    }

    #[test]
    fn derivatives_warmup_latest_values_pick_most_recent_non_null_field() {
        // Most-recent rows have None for mark/index; the warmup must
        // walk backwards to find the previous non-null value, so cold
        // spots in the schema don't blank the field.
        let snapshots = vec![
            make_snap_with_derivs(
                1_700_000_000_000,
                Some(dec!(100_000)),
                Some(dec!(0.0001)),
                Some(dec!(50_000)),
                Some(dec!(50_000)),
            ),
            make_snap_with_derivs(
                1_700_000_120_000,
                Some(dec!(101_000)),
                Some(dec!(0.0002)),
                None,
                None,
            ),
            make_snap_with_derivs(1_700_000_180_000, Some(dec!(102_000)), None, None, None),
        ];
        let state = warm_derivatives_from_snapshots(&snapshots, 500);
        // latest_oi / funding → most recent values
        assert_eq!(state.latest_oi, Some(dec!(102_000)));
        assert_eq!(state.latest_funding, Some(dec!(0.0002)));
        // latest_mark/index → walk backwards to first non-null row
        assert_eq!(state.latest_mark_px, Some(dec!(50_000)));
        assert_eq!(state.latest_index_px, Some(dec!(50_000)));
    }

    #[test]
    fn derivatives_warmup_with_no_snapshots_behaves_like_today() {
        // Cold DB scenario: empty input → default state, so the first
        // WS push after boot continues to start from `None`. This
        // preserves pre-warmup behaviour for fresh installs.
        let state = warm_derivatives_from_snapshots(&[], 500);
        assert_eq!(state.oi_history.len(), 0);
        assert_eq!(state.funding_history.len(), 0);
        assert!(state.latest_oi.is_none());
    }
}
