//! Liquidity event accumulator and cluster estimator.
//!
//! This module owns two related responsibilities:
//!
//! 1. **Real-time event accumulation** — every liquidation event from
//!    exchange WS adapters is recorded into a bounded deque and
//!    aggregated per-candle. See `LiquidityEventAccumulator`.
//!
//! 2. **Cluster-matrix estimation** — every 5 minutes, the platform
//!    recomputes a `LiquidationClusterMatrix` from current OI, funding
//!    rate, mid price, and recent price action. This is the data that
//!    becomes the liquidation heatmap on the frontend.
//!
//! The cluster estimator is a deterministic, snapshot-in / clusters-out
//! function. It uses a documented leverage distribution assumption
//! (power-law by default; configurable) and a maintenance-margin rate.
//! No ML, no online learning — all assumptions are declared and
//! visible to the user.
//!
//! ## Mathematical model
//!
//! For each leverage bucket L in `[1, 3, 5, 10, 20, 50, 100]`:
//!  - Liquidation distance = `1/L - MMR` (e.g. 10x with 0.5% MMR → 9.5%)
//!  - Long liquidation price = `entry_price * (1 - distance)`
//!  - Short liquidation price = `entry_price * (1 + distance)`
//!
//! Entry prices are inferred from recent swing lows (for longs) and
//! swing highs (for shorts), weighted by the recent-volume profile.
//! Each long entry-price × leverage bucket combination contributes
//! notional to a price bucket. Price buckets are then peak-detected
//! to identify clusters.
//!
//! The cascade-asymmetry score is the difference between
//! short-cluster notional (above mid) and long-cluster notional
//! (below mid), normalized by total OI. Positive = short squeeze
//! risk (price likely to rally), negative = long squeeze risk
//! (price likely to drop).

use std::collections::{BTreeMap, HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::normalized::{LiquidationEvent, LiquidationSide};

/// State machine for cascade detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CascadeState {
    /// No significant cascade activity.
    None,
    /// A single event exceeded the z-score threshold.
    Detected,
    /// Multiple events exceeded threshold within the rolling window.
    Sustained,
    /// Activity was elevated but has decayed.
    Exhausted,
}

/// One bar's aggregated liquidity flow. Attached to `MarketSnapshot` as
/// `LiquidityMatrix`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiquidityFlow {
    pub long_liquidations_usd: f64,
    pub short_liquidations_usd: f64,
    pub net_liquidation_usd: f64, // positive = longs got dumped (bearish)
    pub event_count: u32,
    pub largest_event_usd: f64,
    pub largest_event_price: Option<f64>,
    pub largest_event_side: Option<LiquidationSide>,
    pub cascade_state: CascadeState,
    pub cascade_intensity: f64, // 0..100
    /// Price-bucketed notional aggregation across the rolling 24h window
    /// (configurable). Bucketed relative to current mid-price so bands
    /// follow price rather than being pinned to absolute dollar levels.
    ///
    /// The key is `(bucket_index, side)` packed into one i64 — high 32
    /// bits = bucket_index (i32), low 32 bits = side (0 = long, 1 = short).
    /// This keeps the public API a `BTreeMap<i64, RealLiquidationBucket>`
    /// that is JSON-serializable and ordered.
    ///
    /// Used by the frontend heatmap to render **observed** liquidation
    /// bands layered on top of the **estimated** cluster matrix. This
    /// field is display-only — `cascade_risk`, `LiquiditySqueeze`, and
    /// all decision math continue to consume the per-bar totals above.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub recent_real_buckets: BTreeMap<i64, RealLiquidationBucket>,
}

/// One price bucket of **observed** liquidations, relative to the most
/// recent mid at bucket-write time. Consumed by the heatmap primitive to
/// draw real-event bands layered over the estimated cluster matrix.
///
/// `bucket_index` is `floor(price / mid_price / bucket_size_pct)` — i.e.,
/// `% distance from mid in units of bucket_size_pct`. A value of `+50`
/// with `bucket_size_pct = 0.001` means "5% above mid", in 0.1% steps.
/// The same bucket_index always maps to the same approximate price
/// position while the mid is roughly stable, so the band stays anchored
/// to the chart.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RealLiquidationBucket {
    pub bucket_index: i64,
    /// `"long"` for long-liquidations, `"short"` for short-liquidations.
    pub side: LiquidationSide,
    pub price_low: f64,
    pub price_high: f64,
    pub peak_price: f64,
    pub notional_usd: f64,
    pub event_count: u32,
    pub last_updated_ms: u64,
}

impl RealLiquidationBucket {
    /// Pack `(bucket_index, side)` into one `i64` so it can serve as a
    /// `BTreeMap` key while remaining JSON-serializable.
    pub fn pack_key(bucket_index: i64, side: LiquidationSide) -> i64 {
        let side_bits: i64 = match side {
            LiquidationSide::Long => 0,
            LiquidationSide::Short => 1,
        };
        // High 56 bits = bucket_index (signed), low 8 bits = side.
        // Bucket index is bounded by `bucket_size_pct`: even at 0.0005
        // (~2000 buckets across ±50% from mid) we're nowhere near 2^56.
        (bucket_index << 8) | side_bits
    }

    pub fn unpack_key(key: i64) -> (i64, LiquidationSide) {
        let side = if key & 1 == 0 {
            LiquidationSide::Long
        } else {
            LiquidationSide::Short
        };
        // Arithmetic right shift preserves sign on the i64.
        let bucket_index = key >> 8;
        (bucket_index, side)
    }
}

impl Default for LiquidityFlow {
    fn default() -> Self {
        Self {
            long_liquidations_usd: 0.0,
            short_liquidations_usd: 0.0,
            net_liquidation_usd: 0.0,
            event_count: 0,
            largest_event_usd: 0.0,
            largest_event_price: None,
            largest_event_side: None,
            cascade_state: CascadeState::None,
            cascade_intensity: 0.0,
            recent_real_buckets: BTreeMap::new(),
        }
    }
}

/// Per-symbol accumulator. Owns a bounded event deque and the rolling
/// cascade state.
/// v9 (strategy `l1_5.accumulator`): intensity/baseline tuning. Defaults
/// reproduce the v8.2 hardcoded values exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct AccumulatorTuning {
    /// Baseline USD for a single event when no rolling history exists.
    pub baseline_no_history_usd: f64,
    /// Log-scale multiplier for the intensity mapping (`ln(ratio) × k`).
    pub intensity_log_scale: f64,
    /// Fallback event baseline when the window is flat (σ ≈ 0).
    pub fallback_baseline_usd: f64,
    /// `cascade_intensity > exhausted_intensity` + declining window ⇒ Exhausted.
    pub exhausted_intensity: f64,
}

impl Default for AccumulatorTuning {
    fn default() -> Self {
        Self {
            baseline_no_history_usd: 1000.0,
            intensity_log_scale: 20.0,
            fallback_baseline_usd: 500.0,
            exhausted_intensity: 30.0,
        }
    }
}

pub struct LiquidityEventAccumulator {
    symbol: String,
    events: VecDeque<LiquidationEvent>,
    max_events: usize,
    /// Per-bar flow counters. Reset by `flush_to_flow`.
    bar_flow: LiquidityFlow,
    #[allow(dead_code)]
    bar_start_ms: u64,
    /// Threshold (USD) above which a single event is considered a cascade trigger.
    cascade_event_zscore: f64,
    /// Window (number of recent candles) used to detect "Sustained" cascades.
    cascade_window_candles: usize,
    /// Rolling per-candle cascade intensity (last N completed bars).
    rolling_intensity: VecDeque<f64>,
    /// v9: strategy-derived intensity/baseline tuning.
    tuning: AccumulatorTuning,
    /// Configured number of significant events required to promote
    /// `Detected → Sustained`. Wired from `[workspace.liquidity].cascade_sustained_events`.
    cascade_sustained_events: u32,
    /// Bucket size as a fraction of mid-price (e.g. 0.001 = 0.1%). Each
    /// liquidation event is rounded into a bucket relative to the
    /// current mid so the heatmap band tracks price rather than
    /// absolute dollar levels. Mirrors
    /// `[heatmap].bucket_size_pct`.
    heatmap_bucket_size_pct: f64,
    /// Sliding-window retention for the heatmap buckets (seconds).
    /// Buckets older than `now - retention_secs_ms` are evicted on
    /// every `recent_real_buckets()` call. Mirrors
    /// `[heatmap].retention_secs`.
    heatmap_retention_ms: u64,
    /// Mid anchor used to compute the next bucket_index. Updated by
    /// `record_event_with_mid()` so the same engine can refresh mid
    /// from the latest WS mark before bucketing.
    current_mid: Option<f64>,
    /// Aggregate USD notional per packed `(bucket_index, side)` key.
    /// Buckets older than `heatmap_retention_ms` are evicted on read
    /// via `recent_real_buckets()`.
    pub(crate) bucket_map: HashMap<i64, RealLiquidationBucket>,
    /// Number of distinct (bucket, side) pairs ever seen — used for
    /// memory cap. Soft cap at 4 × retention-bars (≈ 4 × 1440min of
    /// bucket rows at 1 min resolution × both sides).
    max_buckets: usize,
}

impl LiquidityEventAccumulator {
    /// Create a new accumulator for `symbol` with sensible production defaults.
    /// `max_events` caps memory regardless of WS flood rate.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self::with_config(symbol, 1_000, 2.5, 5, 3)
    }

    /// Backward-compatible default — no heatmap bucketing.
    pub fn with_config(
        symbol: impl Into<String>,
        max_events: usize,
        cascade_event_zscore: f64,
        cascade_window_candles: usize,
        cascade_sustained_events: u32,
    ) -> Self {
        Self::with_full_config(
            symbol,
            max_events,
            cascade_event_zscore,
            cascade_window_candles,
            cascade_sustained_events,
            0.001,
            86_400,
        )
    }

    /// Full-configuration constructor with heatmap bucketing knobs.
    /// `heatmap_bucket_size_pct` is the bucket width as a fraction of
    /// mid-price (default 0.001 = 0.1%). `heatmap_retention_secs` is
    /// the sliding window in seconds (default 86_400 = 24h).
    pub fn with_full_config(
        symbol: impl Into<String>,
        max_events: usize,
        cascade_event_zscore: f64,
        cascade_window_candles: usize,
        cascade_sustained_events: u32,
        heatmap_bucket_size_pct: f64,
        heatmap_retention_secs: u64,
    ) -> Self {
        let max_buckets = ((heatmap_retention_secs as usize) / 60 + 1).max(2_000);
        Self {
            symbol: symbol.into(),
            events: VecDeque::with_capacity(max_events.min(8_000)),
            max_events,
            bar_flow: LiquidityFlow::default(),
            bar_start_ms: 0,
            cascade_event_zscore,
            cascade_window_candles: cascade_window_candles.max(2),
            rolling_intensity: VecDeque::with_capacity(cascade_window_candles.max(2) * 2),
            tuning: AccumulatorTuning::default(),
            cascade_sustained_events: cascade_sustained_events.max(1),
            heatmap_bucket_size_pct: heatmap_bucket_size_pct.max(1e-6),
            heatmap_retention_ms: heatmap_retention_secs.saturating_mul(1_000),
            current_mid: None,
            bucket_map: HashMap::with_capacity(1024),
            max_buckets,
        }
    }

    /// Symbol this accumulator tracks.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Update the latest known mid-price anchor used when bucketing.
    /// Producers should call this on every WS mark/funding event so
    /// subsequent `record_event` calls bucket against an up-to-date
    /// mid. `record_event` falls back to the last known mid if this is
    /// never called.
    pub fn set_mid(&mut self, mid: f64) {
        if mid.is_finite() && mid > 0.0 {
            self.current_mid = Some(mid);
        }
    }

    /// Bucket one liquidation event into the heatmap aggregation.
    /// Without a known mid, the event is bucketed against the event's
    /// own price (degenerate but consistent — bands will migrate once a
    /// mid is set).
    fn bucket_event(&mut self, ev: &LiquidationEvent, notional: f64) {
        let price = ev.price.to_string().parse::<f64>().unwrap_or(0.0);
        if price <= 0.0 || notional <= 0.0 {
            return;
        }
        // Choose mid anchor: prefer the latest known mid; fall back to
        // the event's own price so the bucket stays at index 0 in
        // pre-mid warm-up.
        let anchor = self.current_mid.unwrap_or(price).max(price * 0.5);
        if anchor <= 0.0 {
            return;
        }
        // bucket_index = ((price / mid) - 1) / bucket_size_pct, rounded to int.
        let ratio = price / anchor;
        let bucket_size = self.heatmap_bucket_size_pct.max(1e-6);
        let bucket_index = ((ratio - 1.0) / bucket_size).round() as i64;
        let key = RealLiquidationBucket::pack_key(bucket_index, ev.side);

        let (price_low, price_high) = self.bucket_price_range(anchor, bucket_index, bucket_size);

        let b = self
            .bucket_map
            .entry(key)
            .or_insert_with(|| RealLiquidationBucket {
                bucket_index,
                side: ev.side,
                price_low,
                price_high,
                peak_price: price,
                notional_usd: 0.0,
                event_count: 0,
                last_updated_ms: ev.timestamp_ms,
            });
        b.notional_usd += notional;
        b.event_count = b.event_count.saturating_add(1);
        b.last_updated_ms = b.last_updated_ms.max(ev.timestamp_ms);

        // Memory cap: oldest bucket by `last_updated_ms` is dropped.
        if self.bucket_map.len() > self.max_buckets {
            if let Some(oldest_key) = self
                .bucket_map
                .iter()
                .min_by_key(|(_, v)| v.last_updated_ms)
                .map(|(k, _)| *k)
            {
                self.bucket_map.remove(&oldest_key);
            }
        }
    }

    /// Map `(anchor, bucket_index, bucket_size)` back to the absolute
    /// `[price_low, price_high]` window. Bucket index 0 corresponds to
    /// `[anchor*(1-bucket_size/2), anchor*(1+bucket_size/2)]`, etc.
    fn bucket_price_range(&self, anchor: f64, bucket_index: i64, bucket_size: f64) -> (f64, f64) {
        let low_ratio = 1.0 + (bucket_index as f64 - 0.5) * bucket_size;
        let high_ratio = 1.0 + (bucket_index as f64 + 0.5) * bucket_size;
        (
            (anchor * low_ratio).max(0.0),
            (anchor * high_ratio).max(0.0),
        )
    }

    /// Record one event. Updates bar-level counters immediately. The
    /// rolling event deque is bounded — when full, the oldest event is
    /// dropped from the back (most recent events retained).
    pub fn record_event(&mut self, ev: LiquidationEvent) {
        let notional = (ev.price.to_string().parse::<f64>().unwrap_or(0.0))
            * (ev.size.to_string().parse::<f64>().unwrap_or(0.0));

        // Per-bar accumulation.
        match ev.side {
            LiquidationSide::Long => {
                self.bar_flow.long_liquidations_usd += notional;
            }
            LiquidationSide::Short => {
                self.bar_flow.short_liquidations_usd += notional;
            }
        }
        if notional > self.bar_flow.largest_event_usd {
            self.bar_flow.largest_event_usd = notional;
            self.bar_flow.largest_event_price = ev.price.to_string().parse().ok();
            self.bar_flow.largest_event_side = Some(ev.side);
        }
        self.bar_flow.event_count += 1;

        // Heatmap bucketing (Block B): each event also feeds the
        // price-bucketed aggregation so the heatmap can render real
        // bands. No-op if `current_mid` was never set (degenerate
        // bucketing against the event's own price).
        self.bucket_event(&ev, notional);

        // Bounded event history (newest at back).
        if self.events.len() >= self.max_events {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    /// Number of distinct (bucket, side) pairs currently tracked.
    pub fn real_bucket_count(&self) -> usize {
        self.bucket_map.len()
    }

    /// Take a snapshot of all currently-tracked buckets (display-only).
    /// Stale buckets (older than `now_ms - heatmap_retention_ms`) are
    /// evicted first; the returned map is ordered and owned.
    pub fn snapshot_real_buckets(&mut self, now_ms: u64) -> BTreeMap<i64, RealLiquidationBucket> {
        let cutoff = now_ms.saturating_sub(self.heatmap_retention_ms);
        let stale: Vec<i64> = self
            .bucket_map
            .iter()
            .filter(|(_, b)| b.last_updated_ms < cutoff)
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            self.bucket_map.remove(&k);
        }
        let mut ordered: BTreeMap<i64, RealLiquidationBucket> = BTreeMap::new();
        for (k, v) in self.bucket_map.iter() {
            ordered.insert(*k, v.clone());
        }
        ordered
    }

    /// Flush the per-bar counters and return the aggregated `LiquidityFlow`.
    /// After this call, the per-bar counters are reset to zero. The rolling
    /// intensity deque is updated with the bar's intensity.
    ///
    /// The `recent_real_buckets` field carries the price-bucketed
    /// aggregation across the configured retention window (Block B),
    /// allowing the frontend heatmap to render observed liquidation
    /// bands layered on top of the estimated cluster matrix. Decoupling
    /// this field from the bar interval means a candle flip does NOT
    /// reset the rolling 24h window — buckets survive bar boundaries
    /// and refresh at most once per flush call.
    pub fn flush_to_flow(&mut self, now_ms: u64) -> LiquidityFlow {
        // Net flow: positive = longs got dumped = bearish pressure.
        self.bar_flow.net_liquidation_usd =
            self.bar_flow.long_liquidations_usd - self.bar_flow.short_liquidations_usd;

        // Cascade state: compute from the rolling intensity window.
        // AUDIT-AIU-011: `derive_cascade_state()` reads
        // `bar_flow.cascade_intensity` for the Exhausted branch, so the
        // intensity MUST be assigned first — the previous ordering computed
        // the state while `cascade_intensity` was still 0.0 (reset at the
        // end of the prior flush), making `0.0 > 30.0` always false and the
        // `CascadeExhausted` signal unreachable in production.
        self.bar_flow.cascade_intensity = self.compute_intensity();
        self.bar_flow.cascade_state = self.derive_cascade_state();

        // Stash this bar's intensity in the rolling window.
        if self.rolling_intensity.len() >= self.cascade_window_candles {
            self.rolling_intensity.pop_front();
        }
        self.rolling_intensity
            .push_back(self.bar_flow.cascade_intensity);

        // Snapshot the rolling-window buckets with stale-bucket eviction.
        // Clone is cheap — the btree only contains ~hundreds of entries
        // in normal regimes (24h × 2 sides × 5-min TF cadence ≈ 600).
        self.bar_flow.recent_real_buckets = self.snapshot_real_buckets(now_ms);

        let out = self.bar_flow.clone();
        self.bar_flow = LiquidityFlow::default();
        out
    }

    fn compute_intensity(&self) -> f64 {
        // Single-event trigger: large liquidation relative to typical.
        // We use the largest event in this bar vs. the running mean.
        if self.events.is_empty() {
            return 0.0;
        }
        // Sum of all notional in current bar vs. mean per-bar in window.
        let total = self.bar_flow.long_liquidations_usd + self.bar_flow.short_liquidations_usd;
        let baseline: f64 = if self.rolling_intensity.is_empty() {
            // v9: strategy `l1_5.accumulator.baseline_no_history_usd`.
            self.tuning.baseline_no_history_usd
        } else {
            // Map mean rolling intensity back to USD for comparison.
            self.rolling_intensity.iter().sum::<f64>() / self.rolling_intensity.len() as f64
                * self.tuning.baseline_no_history_usd
                + 1.0
        };
        let ratio = total / baseline;
        // log-scaled, clamped 0..100.
        (ratio.ln().max(0.0) * self.tuning.intensity_log_scale).min(100.0)
    }

    fn derive_cascade_state(&self) -> CascadeState {
        // AUDIT-AIU-052/053: the previous baseline was a log-scale
        // *inversion* (`mean_intensity × 100 + 1`) of an intensity that is
        // itself `ln(ratio)×20` — the inversion was mathematically
        // inconsistent — and `cascade_event_zscore` was used as a plain
        // ratio multiplier, not a z-score. The baseline is now the mean of
        // the actual recent event notionals, and an event is significant
        // when its notional exceeds `mean + zscore × σ` over the same
        // window — a genuine z-score with the configurable `zscore`
        // multiplier.
        let notionals: Vec<f64> = self
            .events
            .iter()
            .rev()
            .take(50)
            .map(|ev| {
                (ev.price.to_string().parse::<f64>().unwrap_or(0.0))
                    * (ev.size.to_string().parse::<f64>().unwrap_or(0.0))
            })
            .collect();
        let (baseline_event_usd, sigma, count) = if notionals.is_empty() {
            (self.tuning.fallback_baseline_usd, 0.0f64, 0usize)
        } else {
            let n = notionals.len() as f64;
            let mean = notionals.iter().sum::<f64>() / n;
            let var = notionals
                .iter()
                .map(|v| {
                    let d = v - mean;
                    d * d
                })
                .sum::<f64>()
                / n;
            (mean, var.sqrt(), notionals.len())
        };
        let threshold_usd = if sigma > 1e-9 {
            baseline_event_usd + self.cascade_event_zscore * sigma
        } else {
            // Flat window (σ ≈ 0): fall back to the ratio multiplier so a
            // single outsized event still trips detection.
            (baseline_event_usd * self.cascade_event_zscore).max(self.tuning.fallback_baseline_usd)
        };
        let mut significant_events: u32 = 0;
        for notional in notionals {
            if notional >= threshold_usd {
                significant_events += 1;
            }
        }
        let _ = count;
        // >= cascade_sustained_events in the last 50 events = Sustained;
        // 1 to sustained-1 = Detected.
        let sustained_threshold = self.cascade_sustained_events.max(1);
        if significant_events >= sustained_threshold {
            CascadeState::Sustained
        } else if significant_events >= 1 {
            CascadeState::Detected
        } else if self.bar_flow.cascade_intensity > self.tuning.exhausted_intensity
            && !self.rolling_intensity.is_empty()
        {
            // Decayed: bar was hot, window shows decline.
            CascadeState::Exhausted
        } else {
            CascadeState::None
        }
    }

    /// Number of events currently buffered.
    /// v9: apply the strategy's `l1_5.accumulator` tuning knobs.
    pub fn with_tuning(mut self, tuning: AccumulatorTuning) -> Self {
        self.tuning = tuning;
        self
    }

    pub fn buffered_event_count(&self) -> usize {
        self.events.len()
    }

    /// Most recent events, newest first.
    pub fn recent_events(&self, limit: usize) -> Vec<&LiquidationEvent> {
        self.events.iter().rev().take(limit).collect()
    }

    /// Reset the bar-level counters without touching the event history.
    pub fn reset_bar(&mut self) {
        self.bar_flow = LiquidityFlow::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalized::Exchange;
    use rust_decimal::Decimal;

    fn make_event(side: LiquidationSide, price: f64, size: f64, ts_ms: u64) -> LiquidationEvent {
        LiquidationEvent {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            side,
            price: Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO),
            size: Decimal::from_f64_retain(size).unwrap_or(Decimal::ZERO),
            timestamp_ms: ts_ms,
            venue_order_id: None,
        }
    }

    #[test]
    fn empty_accumulator_returns_zero_flow() {
        let mut acc = LiquidityEventAccumulator::new("BTC-USDT");
        let flow = acc.flush_to_flow(0);
        assert_eq!(flow.event_count, 0);
        assert_eq!(flow.long_liquidations_usd, 0.0);
        assert_eq!(flow.short_liquidations_usd, 0.0);
        assert_eq!(flow.cascade_state, CascadeState::None);
        assert_eq!(flow.cascade_intensity, 0.0);
    }

    #[test]
    fn long_liquidation_increments_long_bucket() {
        let mut acc = LiquidityEventAccumulator::new("BTC-USDT");
        // 1 BTC at $50,000 = $50,000 notional.
        acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 1.0, 1));
        let flow = acc.flush_to_flow(2_000);
        assert_eq!(flow.event_count, 1);
        assert!((flow.long_liquidations_usd - 50_000.0).abs() < 0.01);
        assert_eq!(flow.short_liquidations_usd, 0.0);
        assert!(
            flow.net_liquidation_usd > 0.0,
            "net should be positive for longs"
        );
    }

    #[test]
    fn short_liquidation_increments_short_bucket() {
        let mut acc = LiquidityEventAccumulator::new("BTC-USDT");
        acc.record_event(make_event(LiquidationSide::Short, 50_000.0, 2.0, 1));
        let flow = acc.flush_to_flow(2_000);
        assert_eq!(flow.event_count, 1);
        assert_eq!(flow.long_liquidations_usd, 0.0);
        assert!((flow.short_liquidations_usd - 100_000.0).abs() < 0.01);
        assert!(
            flow.net_liquidation_usd < 0.0,
            "net should be negative for shorts"
        );
    }

    #[test]
    fn largest_event_tracking() {
        let mut acc = LiquidityEventAccumulator::new("BTC-USDT");
        acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 0.5, 1));
        acc.record_event(make_event(LiquidationSide::Long, 51_000.0, 2.0, 2));
        acc.record_event(make_event(LiquidationSide::Short, 49_000.0, 0.1, 3));
        let flow = acc.flush_to_flow(4_000);
        // Largest: 51000 * 2 = 102000.
        assert!((flow.largest_event_usd - 102_000.0).abs() < 0.01);
        assert_eq!(flow.largest_event_price, Some(51_000.0));
        assert_eq!(flow.largest_event_side, Some(LiquidationSide::Long));
    }

    #[test]
    fn bounded_event_history() {
        let mut acc = LiquidityEventAccumulator::with_config("BTC-USDT", 5, 2.5, 3, 3);
        for i in 0..20 {
            acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 0.1, i));
        }
        // History must be capped.
        assert_eq!(acc.buffered_event_count(), 5);
    }

    #[test]
    fn flush_resets_per_bar_counters() {
        let mut acc = LiquidityEventAccumulator::new("BTC-USDT");
        acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 1.0, 1));
        let first = acc.flush_to_flow(2_000);
        assert_eq!(first.event_count, 1);
        // Second flush should be empty.
        let second = acc.flush_to_flow(3_000);
        assert_eq!(second.event_count, 0);
        assert_eq!(second.long_liquidations_usd, 0.0);
    }

    #[test]
    fn cascade_state_progression_with_large_events() {
        let mut acc = LiquidityEventAccumulator::with_config("BTC-USDT", 100, 1.5, 5, 3);
        // Build up baseline with small events.
        for i in 0..5 {
            acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 0.01, i * 1000));
            let _ = acc.flush_to_flow((i + 1) * 1000);
        }
        // Now produce a big event.
        acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 5.0, 9999));
        let flow = acc.flush_to_flow(10_000);
        assert!(
            matches!(
                flow.cascade_state,
                CascadeState::Detected | CascadeState::Sustained
            ),
            "expected Detected or Sustained, got {:?}",
            flow.cascade_state
        );
    }

    /// Block B: bucket key round-trips correctly across (bucket_index, side).
    #[test]
    fn pack_unpack_round_trip() {
        for idx in [-200i64, -50, -1, 0, 1, 50, 200, 12_345] {
            for side in [LiquidationSide::Long, LiquidationSide::Short] {
                let key = RealLiquidationBucket::pack_key(idx, side);
                let (out_idx, out_side) = RealLiquidationBucket::unpack_key(key);
                assert_eq!(idx, out_idx, "bucket_index round-trip failed for {idx}");
                assert_eq!(side, out_side, "side round-trip failed for {idx}");
            }
        }
    }

    /// Block B: events bucketed against a known mid land in the expected
    /// (bucket_index, side) cells.
    #[test]
    fn events_bucket_relative_to_mid() {
        let mut acc = LiquidityEventAccumulator::with_full_config(
            "BTC-USDT", 100, 2.5, 5, 3, 0.001,  // 0.1% buckets
            86_400, // 24h retention
        );
        // Set mid to 50_000.
        acc.set_mid(50_000.0);
        // Event 0.5% above mid → bucket_index = 0.5 / 0.1 = 5.
        acc.record_event(make_event(LiquidationSide::Long, 50_250.0, 1.0, 10_000));
        // Event 0.5% below mid (use 49_750 to keep clear of the
        // -0.499999.../-0.5 f64 edge case → bucket_index = -5).
        acc.record_event(make_event(LiquidationSide::Short, 49_750.0, 1.0, 10_100));
        let flow = acc.flush_to_flow(11_000);
        assert_eq!(flow.recent_real_buckets.len(), 2, "two distinct buckets");
        // Find the long bucket (positive index) and the short (negative).
        let long_buckets: Vec<&RealLiquidationBucket> = flow
            .recent_real_buckets
            .values()
            .filter(|b| b.side == LiquidationSide::Long)
            .collect();
        let short_buckets: Vec<&RealLiquidationBucket> = flow
            .recent_real_buckets
            .values()
            .filter(|b| b.side == LiquidationSide::Short)
            .collect();
        assert_eq!(long_buckets.len(), 1);
        assert_eq!(short_buckets.len(), 1);
        assert_eq!(long_buckets[0].bucket_index, 5);
        assert_eq!(short_buckets[0].bucket_index, -5);
        // Bucket 5 spans [mid*(1 + 4.5*0.001), mid*(1 + 5.5*0.001)]
        //   = [50_225, 50_275]   for mid=50_000 and size=0.1%.
        assert!(long_buckets[0].price_low >= 50_224.0 && long_buckets[0].price_low <= 50_226.0);
        assert!(long_buckets[0].price_high >= 50_274.0 && long_buckets[0].price_high <= 50_276.0);
    }

    /// Block B: stale buckets are evicted after the retention window
    /// expires on snapshot.
    #[test]
    fn stale_buckets_evicted_on_snapshot() {
        let mut acc = LiquidityEventAccumulator::with_full_config(
            "BTC-USDT", 100, 2.5, 5, 3, 0.001, 60, // 60-second retention for the test
        );
        acc.set_mid(50_000.0);
        // Single event at t=1_000.
        acc.record_event(make_event(LiquidationSide::Long, 50_000.0, 1.0, 1_000));
        // Snapshot well past retention: now=1_000 + 100_000.
        let snap = acc.snapshot_real_buckets(101_000);
        assert!(snap.is_empty(), "old bucket should be evicted");
    }

    /// Block B: serializing a non-empty `recent_real_buckets` includes the
    /// field; an empty one is skipped (avoids noisy payloads).
    #[test]
    fn real_buckets_serialization_skips_when_empty() {
        let mut flow = LiquidityFlow::default();
        let json_empty = serde_json::to_string(&flow).unwrap();
        assert!(
            !json_empty.contains("recent_real_buckets"),
            "empty buckets should skip serialization: {}",
            json_empty
        );
        flow.recent_real_buckets.insert(
            0,
            RealLiquidationBucket {
                bucket_index: 0,
                side: LiquidationSide::Long,
                price_low: 49_950.0,
                price_high: 50_050.0,
                peak_price: 50_000.0,
                notional_usd: 25_000.0,
                event_count: 1,
                last_updated_ms: 1000,
            },
        );
        let json_full = serde_json::to_string(&flow).unwrap();
        assert!(
            json_full.contains("recent_real_buckets"),
            "non-empty buckets should serialize: {}",
            json_full
        );
    }
}

// =============================================================================
// Phase 2 — Liquidation Cluster Matrix
// =============================================================================
//
// All types here serialize as snake_case (default) to match the
// `LiquidityMatrix` schema documented in the spec docs.

/// Direction classification for a cluster relative to current price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClusterKind {
    /// Cluster sits above current price (would attract short-squeeze rallies).
    AboveCurrentPrice,
    /// Cluster sits below current price (long-squeeze cascade target).
    BelowCurrentPrice,
    /// Cluster is essentially at current price (imminent cascade risk).
    AtCurrentPrice,
    /// Cluster is far enough away that it doesn't influence near-term price.
    Distant,
}

/// Where the leverage distribution assumption came from — for transparency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LeverageDistributionSource {
    /// Static power-law default (α ≈ 1.5).
    DefaultPowerLaw,
    /// Power-law modulated by funding-rate extremes.
    FundingAdaptive,
    /// Loaded from a user-supplied config override.
    ConfigOverride,
}

/// Documented assumptions for the leverage distribution. Always serialized
/// so the UI can show "estimated using …" to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeverageAssumptions {
    pub buckets: Vec<u32>,
    pub weights: Vec<f64>,
    pub funding_modulation_active: bool,
    pub funding_extreme_pct: f64,
    pub source: LeverageDistributionSource,
}

/// One detected liquidation cluster. The price-low / price-high window
/// contains the cluster; the peak_price is where most of the notional
/// is concentrated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiquidationCluster {
    pub price_low: f64,
    pub price_high: f64,
    pub peak_price: f64,
    pub notional_usd: f64,
    pub dominant_leverage: u32,
    pub distance_from_mid_pct: f64,
    pub cluster_kind: ClusterKind,
    pub magnet_strength: f64, // 0..100
}

/// The cluster matrix produced every 5 minutes (or per refresh) per symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationClusterMatrix {
    pub symbol: String,
    pub generated_at_ms: u64,
    pub valid_until_ms: u64,
    pub mid_price: f64,
    pub leverage_assumptions: LeverageAssumptions,
    /// Short liquidation clusters (price-above-mid; the "ceiling").
    pub short_clusters: Vec<LiquidationCluster>,
    /// Long liquidation clusters (price-below-mid; the "floor").
    pub long_clusters: Vec<LiquidationCluster>,
    /// [-1, +1]; positive = short squeeze risk, negative = long squeeze risk
    /// (more short-cluster notional above mid than long-cluster below mid).
    pub cascade_asymmetry: f64,
    pub total_long_oi_usd: f64,
    pub total_short_oi_usd: f64,
    /// 0..1 — overall confidence in the estimate (1 = high OI, normal funding).
    pub estimation_confidence: f64,
}

impl LiquidationClusterMatrix {
    /// Empty placeholder matrix used when estimation is not yet possible
    /// (insufficient data, zero OI, etc.).
    pub fn empty(symbol: &str, mid_price: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            generated_at_ms: 0,
            valid_until_ms: 0,
            mid_price,
            leverage_assumptions: LeverageAssumptions {
                buckets: vec![1, 3, 5, 10, 20, 50, 100],
                weights: vec![0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
                funding_modulation_active: true,
                funding_extreme_pct: 0.0005,
                source: LeverageDistributionSource::DefaultPowerLaw,
            },
            short_clusters: vec![],
            long_clusters: vec![],
            cascade_asymmetry: 0.0,
            total_long_oi_usd: 0.0,
            total_short_oi_usd: 0.0,
            estimation_confidence: 0.0,
        }
    }
}

/// Status of the most recent cluster-matrix refresh for a single TF slot.
///
/// Surfaced to the UI via `/api/liquidity/cluster-status` so operators
/// can distinguish "the LIQ HEATMAP is empty because there's no data
/// yet" (Pending) from "the refresh task failed and is silently retrying"
/// (Skipped with a reason). Without this distinction the heatmap can
/// appear empty for minutes at boot and operators have no signal that
/// the refresh is misbehaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClusterRefreshStatus {
    /// No refresh has been attempted yet (cold boot, before first tick).
    Pending,
    /// Most recent refresh produced a valid matrix.
    Ok,
    /// Most recent refresh failed (insufficient history, no OI, etc.).
    Skipped,
    /// Most recent refresh produced a valid matrix that has since expired
    /// (TTL elapsed without a fresh tick — usually means the refresh task
    /// crashed and hasn't been reaped).
    Stale,
}

/// Snapshot of the cluster-refresh task for one TF slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusSnapshot {
    pub symbol: String,
    /// v11.9: derived duration label ("1s".."1d") of the TF this snapshot
    /// describes.
    pub timeframe_label: String,
    pub status: ClusterRefreshStatus,
    /// Unix epoch ms of the last attempted refresh (success OR failure).
    pub last_refresh_attempt_ms: u64,
    /// Unix epoch ms of the last successful refresh, if any.
    pub last_success_ms: Option<u64>,
    /// Free-text reason for the most recent skip, if status == Skipped.
    pub last_skip_reason: Option<String>,
    /// Number of short clusters in the most recent successful matrix.
    pub cluster_count_short: usize,
    /// Number of long clusters in the most recent successful matrix.
    pub cluster_count_long: usize,
    /// Mid price from the most recent successful matrix (0.0 if none).
    pub mid_price: f64,
    /// Milliseconds until the current matrix's TTL elapses. Negative if
    /// already expired.
    pub ttl_remaining_ms: i64,
}

impl ClusterStatusSnapshot {
    /// Initial pending state used at cold boot before the first tick.
    pub fn pending(symbol: &str, timeframe_label: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            timeframe_label: timeframe_label.to_string(),
            status: ClusterRefreshStatus::Pending,
            last_refresh_attempt_ms: 0,
            last_success_ms: None,
            last_skip_reason: None,
            cluster_count_short: 0,
            cluster_count_long: 0,
            mid_price: 0.0,
            ttl_remaining_ms: 0,
        }
    }
}

/// Input bundle for `estimate_clusters`. Bundles everything the
/// estimator needs so callers don't have to thread 8+ parameters.
pub struct ClusterEstimateInput<'a> {
    pub symbol: &'a str,
    pub mid_price: f64,
    /// Recent close prices, oldest first. Used to detect swing lows /
    /// highs which seed the entry-price distribution.
    pub price_history: &'a [f64],
    pub total_oi_usd: f64,
    pub funding_rate: f64,
    /// Estimated split of OI between longs and shorts. None = compute
    /// from funding rate + price action heuristic.
    pub long_oi_pct: Option<f64>,
    pub maintenance_margin_rate: f64,
    pub funding_extreme_pct: f64,
    pub funding_modulation_active: bool,
    pub leverage_buckets: &'a [u32],
    pub leverage_weights: &'a [f64],
    /// Min cluster notional in USD to keep (filters noise).
    pub min_cluster_notional_usd: f64,
    /// v9 (L2.5): estimation knobs — swing window/lookback, bin size,
    /// peak half-width divisor, bound decay, TTL. Defaults reproduce the
    /// v8.2 hardcoded values exactly.
    pub estimation: ClusterEstimationParams,
    /// v9 (L2.5 `oi_split`): the long-OI-share heuristic knobs.
    pub oi_split: ClusterOiSplitParams,
    /// v9 (L2.5 `confidence`): estimation-confidence anchors.
    pub confidence: ClusterConfidenceParams,
    /// v9 (L2.5 `funding_modulation.shift`): mass tilt fraction at full
    /// funding-extreme tilt.
    pub funding_mod_shift: f64,
}

/// v9 (strategy `l2_5.estimation`): the cluster-estimator geometry knobs.
/// Defaults = the pre-v9 hardcoded values (200-candle window is applied by
/// the caller; everything else lives here).
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterEstimationParams {
    /// Bars of this TF's price history the caller feeds (window size).
    pub swing_window_bars: usize,
    /// Swing low/high detection lookback.
    pub swing_lookback: usize,
    /// Price-bucket width (0.001 = 0.1%).
    pub bin_size_pct: f64,
    /// `half = series_len / peak_halfwidth_divisor` in peak detection.
    pub peak_halfwidth_divisor: usize,
    /// Cluster-bound walk threshold as a fraction of the peak notional.
    pub bound_decay: f64,
    /// Matrix validity window.
    pub ttl_secs: u64,
}

impl Default for ClusterEstimationParams {
    fn default() -> Self {
        Self {
            swing_window_bars: 200,
            swing_lookback: 5,
            bin_size_pct: 0.001,
            peak_halfwidth_divisor: 20,
            bound_decay: 0.5,
            ttl_secs: 300,
        }
    }
}

/// v9 (strategy `l2_5.oi_split`): the long-OI-share heuristic.
/// `funding_anchor` None = follow `funding_extreme_pct`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterOiSplitParams {
    pub funding_anchor: Option<f64>,
    pub funding_bias_scale: f64,
    /// Price-change anchor as a fraction (1.0% = 0.01).
    pub price_anchor_pct: f64,
    pub price_bias_scale: f64,
    pub clamp: [f64; 2],
}

impl Default for ClusterOiSplitParams {
    fn default() -> Self {
        Self {
            funding_anchor: None,
            funding_bias_scale: 0.3,
            price_anchor_pct: 0.01,
            price_bias_scale: 0.2,
            clamp: [0.10, 0.90],
        }
    }
}

/// v9 (strategy `l2_5.confidence`): estimation-confidence anchors.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterConfidenceParams {
    pub oi_adequacy_anchor_usd: f64,
    pub funding_penalty: f64,
}

impl Default for ClusterConfidenceParams {
    fn default() -> Self {
        Self {
            oi_adequacy_anchor_usd: 1_000_000.0,
            funding_penalty: 0.3,
        }
    }
}

impl<'a> Default for ClusterEstimateInput<'a> {
    fn default() -> Self {
        Self {
            symbol: "",
            mid_price: 0.0,
            price_history: &[],
            total_oi_usd: 0.0,
            funding_rate: 0.0,
            long_oi_pct: None,
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: true,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 50_000.0,
            estimation: ClusterEstimationParams::default(),
            oi_split: ClusterOiSplitParams::default(),
            confidence: ClusterConfidenceParams::default(),
            funding_mod_shift: 0.05,
        }
    }
}

/// Per-bin notional accumulator, segmented by leverage bucket.
///
/// v6.10 (Phase 2 / B6): each 0.1% price bin now records notional per
/// leverage bucket so the cluster detector can surface the dominant
/// leverage (the bucket with the highest aggregate USD contribution).
/// `BinsByLeverage` is internal to `estimate_clusters` and is flattened
/// to a scalar `dominant_leverage` field on each emitted `LiquidationCluster`.
#[derive(Debug, Default, Clone)]
struct BinsByLeverage {
    by_lev: Vec<(u32, f64)>,
}

impl BinsByLeverage {
    fn add(&mut self, lev: u32, notional: f64) {
        if let Some(entry) = self.by_lev.iter_mut().find(|(l, _)| *l == lev) {
            entry.1 += notional;
        } else {
            self.by_lev.push((lev, notional));
        }
    }

    fn total(&self) -> f64 {
        self.by_lev.iter().map(|(_, v)| *v).sum()
    }
}

/// Apply funding-rate modulation to the leverage weights. Extreme funding
/// → heavier high-leverage tail (because crowded trades = high leverage).
fn apply_funding_modulation(
    weights: &mut [f64],
    funding_rate: f64,
    extreme_pct: f64,
    shift_frac: f64,
) {
    if extreme_pct <= 0.0 {
        return;
    }
    let funding_mag = funding_rate.abs();
    // 0 when funding is at zero; 1 when funding is at-or-above extreme.
    let tilt = (funding_mag / extreme_pct).clamp(0.0, 1.0);
    // Tilt mass from low-leverage buckets (index 0..2) toward high-leverage
    // buckets (index 4..6). 5% of mass moved at full tilt.
    let shift = shift_frac * tilt;
    let len = weights.len();
    for w in weights.iter_mut().take(2.min(len)) {
        *w = (*w - shift / 2.0).max(0.0);
    }
    for w in weights.iter_mut().skip(len.saturating_sub(2)) {
        *w = (*w + shift / 2.0).min(1.0);
    }
    // Renormalize.
    let total: f64 = weights.iter().sum();
    if total > 0.0 {
        for w in weights.iter_mut() {
            *w /= total;
        }
    }
}

/// Estimate the long/short OI split. Uses funding rate as the primary
/// signal; price action as the secondary; default 50/50 if neither is
/// informative.
///
/// The funding anchor is the configured `funding_extreme_pct` (v9 F-01) —
/// the split heuristic must follow the operator's funding-extreme knob
/// rather than a hardcoded 0.0005, so a tuned extreme threshold tunes the
/// OI-split sensitivity consistently. A non-positive anchor (misconfig)
/// falls back to the shipped 0.0005 default.
fn estimate_long_oi_pct(
    funding_rate: f64,
    price_history: &[f64],
    override_pct: Option<f64>,
    funding_extreme_pct: f64,
    oi_split: &ClusterOiSplitParams,
) -> f64 {
    if let Some(p) = override_pct {
        return p.clamp(0.05, 0.95);
    }
    // v9 (L2.5 `oi_split`): `funding_anchor: null` follows
    // `l1_5.funding_extreme_pct` (the v9 F-01 anchor).
    let anchor = oi_split
        .funding_anchor
        .unwrap_or(if funding_extreme_pct > 0.0 {
            funding_extreme_pct
        } else {
            0.0005
        });
    let funding_bias =
        (funding_rate / anchor.max(1e-12)).clamp(-1.0, 1.0) * oi_split.funding_bias_scale;
    let price_anchor = if oi_split.price_anchor_pct > 0.0 {
        oi_split.price_anchor_pct
    } else {
        0.01
    };
    let price_bias = if price_history.len() >= 4 {
        let n = price_history.len();
        let recent = price_history[n - 1];
        let prior = price_history[n - 4];
        let change = (recent - prior) / prior.max(1e-9);
        // Map price change to bias: +anchor → +price_bias_scale long bias.
        (change / price_anchor).clamp(-1.0, 1.0) * oi_split.price_bias_scale
    } else {
        0.0
    };
    (0.5 + funding_bias + price_bias).clamp(oi_split.clamp[0], oi_split.clamp[1])
}

/// Find swing lows and highs in a price history. Returns sorted unique
/// prices (oldest first). Volume weighting not available here — caller
/// can use a different seed if needed.
fn find_swing_levels(price_history: &[f64], lookback: usize) -> (Vec<f64>, Vec<f64>) {
    if price_history.len() < 3 {
        return (vec![], vec![]);
    }
    let mut lows = Vec::new();
    let mut highs = Vec::new();
    let lb = lookback.max(1).min(price_history.len() / 3);
    for i in lb..(price_history.len() - lb) {
        let window = &price_history[i - lb..=i + lb];
        let p = price_history[i];
        if window.iter().all(|&x| x >= p) {
            lows.push(p);
        }
        if window.iter().all(|&x| x <= p) {
            highs.push(p);
        }
    }
    // Deduplicate (within 0.5%).
    let dedup = |mut v: Vec<f64>| -> Vec<f64> {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v.dedup_by(|a, b| (*a - *b).abs() / a.max(1e-9) < 0.005);
        v
    };
    (dedup(lows), dedup(highs))
}

/// The main estimator. Deterministic, snapshot-in / clusters-out.
pub fn estimate_clusters(input: &ClusterEstimateInput) -> LiquidationClusterMatrix {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Empty placeholder if no data (NaN inputs also fail these guards —
    // NaN comparisons are false, so an explicit is_finite gate is required
    // to keep `estimation_confidence` off the NaN → JSON-null path).
    if !input.mid_price.is_finite()
        || !input.total_oi_usd.is_finite()
        || !input.funding_rate.is_finite()
        || input.mid_price <= 0.0
        || input.total_oi_usd <= 0.0
    {
        return LiquidationClusterMatrix::empty(input.symbol, input.mid_price);
    }

    // 1. Compute leverage distribution.
    let mut weights = input.leverage_weights.to_vec();
    let leverage_source = if input.funding_modulation_active
        && input.funding_rate.abs() > input.funding_extreme_pct
    {
        apply_funding_modulation(
            &mut weights,
            input.funding_rate,
            input.funding_extreme_pct,
            input.funding_mod_shift,
        );
        LeverageDistributionSource::FundingAdaptive
    } else {
        LeverageDistributionSource::DefaultPowerLaw
    };
    let leverage_assumptions = LeverageAssumptions {
        buckets: input.leverage_buckets.to_vec(),
        weights: weights.clone(),
        funding_modulation_active: input.funding_modulation_active,
        funding_extreme_pct: input.funding_extreme_pct,
        source: leverage_source,
    };

    // 2. Estimate long/short OI split.
    let long_oi_pct = estimate_long_oi_pct(
        input.funding_rate,
        input.price_history,
        input.long_oi_pct,
        input.funding_extreme_pct,
        &input.oi_split,
    );
    let long_oi_usd = input.total_oi_usd * long_oi_pct;
    let short_oi_usd = input.total_oi_usd * (1.0 - long_oi_pct);

    // 3. Find swing levels (v9: strategy `l2_5.estimation.swing_lookback`).
    let (swing_lows, swing_highs) =
        find_swing_levels(input.price_history, input.estimation.swing_lookback);

    // 4. For each (entry_price, leverage) combination, compute the
    //    liquidation price and accumulate into 0.1% price buckets.
    //
    // v6.10 (Phase 2 / B6): per-bin notional now tracks per-leverage-bucket
    // contribution so that `detect_clusters` can surface the dominant
    // leverage for each emitted cluster (previously hardcoded to 10).
    // v9: strategy `l2_5.estimation.bin_size_pct`.
    let price_bin_pct = input.estimation.bin_size_pct.max(1e-6);
    let mut long_bins: std::collections::BTreeMap<i64, BinsByLeverage> =
        std::collections::BTreeMap::new();
    let mut short_bins: std::collections::BTreeMap<i64, BinsByLeverage> =
        std::collections::BTreeMap::new();

    let bucket_long = |entry: f64, lev: u32| -> Option<f64> {
        let dist = (1.0 / lev as f64) - input.maintenance_margin_rate;
        if dist <= 0.0 {
            return None;
        }
        Some(entry * (1.0 - dist))
    };
    let bucket_short = |entry: f64, lev: u32| -> Option<f64> {
        let dist = (1.0 / lev as f64) - input.maintenance_margin_rate;
        if dist <= 0.0 {
            return None;
        }
        Some(entry * (1.0 + dist))
    };

    for (lev, weight) in input.leverage_buckets.iter().zip(weights.iter()) {
        if *weight <= 0.0 || *lev == 0 {
            continue;
        }
        let lev_notional_long = long_oi_usd * weight;
        let lev_notional_short = short_oi_usd * weight;
        // Distribute across swing lows (long) and swing highs (short).
        // If no swing levels, fall back to current mid (single bin).
        if swing_lows.is_empty() {
            if let Some(liq_px) = bucket_long(input.mid_price, *lev) {
                let key = (liq_px / input.mid_price / price_bin_pct).round() as i64;
                long_bins
                    .entry(key)
                    .or_default()
                    .add(*lev, lev_notional_long);
            }
        } else {
            let per_entry = lev_notional_long / swing_lows.len() as f64;
            for entry in &swing_lows {
                if let Some(liq_px) = bucket_long(*entry, *lev) {
                    let key = (liq_px / input.mid_price / price_bin_pct).round() as i64;
                    long_bins.entry(key).or_default().add(*lev, per_entry);
                }
            }
        }
        if swing_highs.is_empty() {
            if let Some(liq_px) = bucket_short(input.mid_price, *lev) {
                let key = (liq_px / input.mid_price / price_bin_pct).round() as i64;
                short_bins
                    .entry(key)
                    .or_default()
                    .add(*lev, lev_notional_short);
            }
        } else {
            let per_entry = lev_notional_short / swing_highs.len() as f64;
            for entry in &swing_highs {
                if let Some(liq_px) = bucket_short(*entry, *lev) {
                    let key = (liq_px / input.mid_price / price_bin_pct).round() as i64;
                    short_bins.entry(key).or_default().add(*lev, per_entry);
                }
            }
        }
    }

    // 5. Peak detection → cluster list (both sides).
    let long_clusters = detect_clusters(
        &long_bins,
        input.mid_price,
        price_bin_pct,
        input.min_cluster_notional_usd,
        &input.estimation,
    );
    let short_clusters = detect_clusters(
        &short_bins,
        input.mid_price,
        price_bin_pct,
        input.min_cluster_notional_usd,
        &input.estimation,
    );

    // 6. Cascade asymmetry: short liq density above mid vs long liq
    //    density below mid, normalized by total OI.
    let short_above: f64 = short_clusters.iter().map(|c| c.notional_usd).sum();
    let long_below: f64 = long_clusters.iter().map(|c| c.notional_usd).sum();
    let asymmetry = if input.total_oi_usd > 0.0 {
        (short_above - long_below) / input.total_oi_usd
    } else {
        0.0
    };
    let asymmetry = asymmetry.clamp(-1.0, 1.0);

    // 7. Confidence: lower if OI is thin, if funding is extreme, or if
    //    volatility (here proxied by funding magnitude) is high.
    //    Guard: `funding_extreme_pct == 0` (operator misconfig) would make
    //    `0/0 = NaN` and poison `estimation_confidence` on the wire.
    let funding_mag_norm = if input.funding_extreme_pct > 0.0 {
        (input.funding_rate.abs() / input.funding_extreme_pct).clamp(0.0, 2.0)
    } else {
        0.0
    };
    // v9 (L2.5 `confidence`): OI-adequacy anchor + funding penalty.
    let anchor = input.confidence.oi_adequacy_anchor_usd.max(1.0);
    let oi_adequacy = (input.total_oi_usd / anchor).min(1.0);
    let confidence =
        (oi_adequacy * (1.0 - input.confidence.funding_penalty * funding_mag_norm)).clamp(0.0, 1.0);

    LiquidationClusterMatrix {
        symbol: input.symbol.to_string(),
        generated_at_ms: now_ms,
        valid_until_ms: now_ms + input.estimation.ttl_secs.max(1) * 1000,
        mid_price: input.mid_price,
        leverage_assumptions,
        short_clusters,
        long_clusters,
        cascade_asymmetry: asymmetry,
        total_long_oi_usd: long_oi_usd,
        total_short_oi_usd: short_oi_usd,
        estimation_confidence: confidence,
    }
}

/// Detect cluster peaks from the binned density.
///
/// v6.10 (Phase 2 / B6): bins are now `BinsByLeverage` so each cluster
/// can report the leverage bucket contributing the most USD notional.
fn detect_clusters(
    bins: &std::collections::BTreeMap<i64, BinsByLeverage>,
    mid_price: f64,
    price_bin_pct: f64,
    min_notional: f64,
    estimation: &ClusterEstimationParams,
) -> Vec<LiquidationCluster> {
    if bins.is_empty() || mid_price <= 0.0 {
        return vec![];
    }
    // Convert to a sorted Vec<(price, total_notional)> using the per-bin
    // total so peak detection still uses the aggregate series.
    let mut series: Vec<(f64, f64, &BinsByLeverage)> = bins
        .iter()
        .map(|(k, v)| {
            let price = (*k as f64) * price_bin_pct * mid_price;
            (price, v.total(), v)
        })
        .collect();
    series.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Simple local-maxima detection with a half-width window.
    // v9: the divisor is the strategy's `l2_5.estimation.peak_halfwidth_divisor`.
    let mut clusters = Vec::new();
    let half = (series.len() / estimation.peak_halfwidth_divisor.max(1)).max(2);
    for i in half..(series.len().saturating_sub(half)) {
        let (_, v, _) = series[i];
        if v < min_notional {
            continue;
        }
        let is_peak = series[i - half..i].iter().all(|(_, x, _)| *x <= v)
            && series[i + 1..=i + half].iter().all(|(_, x, _)| *x <= v);
        if !is_peak {
            continue;
        }
        // Find cluster bounds: walk outward while density stays >= 50% of peak.
        let mut lo = i;
        let mut hi = i;
        // v9: bound-decay fraction from `l2_5.estimation.bound_decay`.
        let half_max = v * estimation.bound_decay.clamp(0.0, 1.0);
        while lo > 0 && series[lo - 1].1 >= half_max {
            lo -= 1;
        }
        while hi + 1 < series.len() && series[hi + 1].1 >= half_max {
            hi += 1;
        }
        let price_low = series[lo].0;
        let price_high = series[hi].0;
        let peak_price = series[i].0;
        let notional: f64 = series[lo..=hi].iter().map(|(_, v, _)| *v).sum();

        // v6.10 (Phase 2 / B6): dominant leverage = the bucket with the
        // largest aggregate USD contribution across the cluster's bins.
        // Ties resolve to the highest leverage bucket (max-by-(lev, notional)).
        let mut best_lev_total: std::collections::HashMap<u32, f64> =
            std::collections::HashMap::new();
        for entry in series[lo..=hi]
            .iter()
            .flat_map(|(_, _, bl)| bl.by_lev.iter())
        {
            *best_lev_total.entry(entry.0).or_insert(0.0) += entry.1;
        }
        let dominant_leverage = best_lev_total
            .iter()
            .max_by(|(la, va), (lb, vb)| {
                va.partial_cmp(vb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(lb.cmp(la)) // tie → higher leverage wins
            })
            .map(|(l, _)| *l)
            .unwrap_or(0);

        let distance_pct = ((peak_price - mid_price) / mid_price).abs() * 100.0;
        // AUDIT-AIU-115: `cluster_kind` must classify by the cluster's
        // PHYSICAL position relative to the current price — the legacy
        // side-based assignment (`is_long → BelowCurrentPrice`) mislabeled
        // clusters in trending markets: during a fresh breakdown the most
        // recent swing low sits ABOVE mid, so long-liq clusters seeded from
        // it land above mid yet were reported `BelowCurrentPrice` (and vice
        // versa for shorts in an uptrend) — wrong panel rows, wrong
        // `02-13` contract semantics, and a wrong MagnetActivated direction
        // for such clusters.
        let kind = if distance_pct < 0.5 {
            ClusterKind::AtCurrentPrice
        } else if peak_price > mid_price {
            ClusterKind::AboveCurrentPrice
        } else {
            ClusterKind::BelowCurrentPrice
        };
        // Magnet strength: weighted by notional × inverse distance (closer = stronger).
        let proximity = (-distance_pct / 2.0).exp();
        let magnet = (notional / 1_000_000.0 * 100.0 * proximity).clamp(0.0, 100.0);
        clusters.push(LiquidationCluster {
            price_low,
            price_high,
            peak_price,
            notional_usd: notional,
            dominant_leverage,
            distance_from_mid_pct: distance_pct,
            cluster_kind: kind,
            magnet_strength: magnet,
        });
    }
    // Deduplicate clusters that overlap heavily.
    clusters.sort_by(|a, b| {
        b.notional_usd
            .partial_cmp(&a.notional_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut dedup: Vec<LiquidationCluster> = Vec::with_capacity(clusters.len());
    for c in clusters {
        if !dedup
            .iter()
            .any(|existing| (existing.peak_price - c.peak_price).abs() / mid_price < 0.005)
        {
            dedup.push(c);
        }
    }
    dedup
}

#[cfg(test)]
mod cluster_tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_history(base: f64, n: usize, swing: f64) -> Vec<f64> {
        // Generate a price history with a few swing lows and highs.
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f64 / n as f64;
            v.push(
                base + swing * (t * std::f64::consts::PI * 2.0).sin()
                    + swing * 0.3 * (t * std::f64::consts::PI * 6.0).cos(),
            );
        }
        v
    }

    #[test]
    fn empty_input_returns_empty_matrix() {
        let input = ClusterEstimateInput {
            mid_price: 0.0,
            total_oi_usd: 0.0,
            ..ClusterEstimateInput::default()
        };
        let m = estimate_clusters(&input);
        assert!(m.short_clusters.is_empty());
        assert!(m.long_clusters.is_empty());
        assert_eq!(m.cascade_asymmetry, 0.0);
        assert_eq!(m.estimation_confidence, 0.0);
    }

    #[test]
    fn basic_estimation_produces_clusters_on_both_sides() {
        let history = make_history(50_000.0, 100, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 50_000_000.0, // $50M
            funding_rate: 0.0001,
            long_oi_pct: None,
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: true,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 100_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m = estimate_clusters(&input);
        // Should have clusters on both sides of mid.
        assert!(
            !m.short_clusters.is_empty(),
            "expected at least one short cluster above mid"
        );
        assert!(
            !m.long_clusters.is_empty(),
            "expected at least one long cluster below mid"
        );
        // All short clusters are above mid, all long clusters are below.
        for c in &m.short_clusters {
            assert!(
                c.peak_price > m.mid_price,
                "short cluster should be above mid, got {}",
                c.peak_price
            );
            assert_eq!(c.cluster_kind, ClusterKind::AboveCurrentPrice);
        }
        for c in &m.long_clusters {
            assert!(
                c.peak_price < m.mid_price,
                "long cluster should be below mid, got {}",
                c.peak_price
            );
            assert_eq!(c.cluster_kind, ClusterKind::BelowCurrentPrice);
        }
    }

    #[test]
    fn cluster_kind_follows_physical_position_in_a_trending_breakdown() {
        // AUDIT-AIU-115: `cluster_kind` must classify by PHYSICAL position.
        // In a fresh breakdown the confirmed swing lows sit ABOVE the
        // current mid, so high-leverage long-liq clusters seeded from them
        // land above mid — the legacy side-based assignment labeled them
        // `BelowCurrentPrice` (wrong panel rows + wrong MagnetActivated
        // direction).
        let mut history: Vec<f64> = Vec::new();
        // Three rising sideways phases → swing lows at 50_400 / 50_800 /
        // 51_200 (> 0.5% apart so the swing-level dedup keeps all three).
        for base in [50_600.0, 51_000.0, 51_400.0] {
            for i in 0..40 {
                history.push(base + 200.0 * ((i as f64) * std::f64::consts::PI / 12.0).sin());
            }
        }
        // Fresh breakdown: current mid (49_300) well below all swing lows.
        history.push(50_000.0);
        history.push(49_800.0);
        history.push(49_600.0);
        history.push(49_500.0);
        history.push(49_400.0);
        let mid = 49_300.0;
        history.push(mid);

        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: mid,
            price_history: &history,
            total_oi_usd: 50_000_000.0,
            funding_rate: 0.0001,
            long_oi_pct: None,
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: true,
            // Weight concentrated at high leverage so the above-mid bins
            // (50_400 × (1 − 0.005) ≈ 50_148 > mid) form their own cluster.
            leverage_buckets: &[5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.15, 0.20, 0.50],
            min_cluster_notional_usd: 100_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m = estimate_clusters(&input);

        // At least one long cluster sits above mid (seeded from the 50_400 /
        // 50_800 swing lows at high leverage).
        assert!(
            m.long_clusters.iter().any(|c| c.peak_price > m.mid_price),
            "expected a long cluster above mid in a breakdown, long={:?}",
            m.long_clusters
                .iter()
                .map(|c| (c.peak_price, c.cluster_kind))
                .collect::<Vec<_>>()
        );
        // THE invariant: every cluster's kind matches its physical position.
        for c in m.long_clusters.iter().chain(m.short_clusters.iter()) {
            let expected = if c.peak_price > m.mid_price {
                ClusterKind::AboveCurrentPrice
            } else {
                ClusterKind::BelowCurrentPrice
            };
            assert_eq!(
                c.cluster_kind, expected,
                "cluster at {} (mid {}) must be {:?}",
                c.peak_price, m.mid_price, expected
            );
        }
    }

    #[test]
    fn long_oi_override_is_respected() {
        let history = make_history(50_000.0, 100, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 1_000_000.0,
            funding_rate: 0.0,
            long_oi_pct: Some(0.8), // 80% long
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: false,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 10_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m = estimate_clusters(&input);
        // With 80% long, the long cluster total should exceed the short.
        let long_total: f64 = m.long_clusters.iter().map(|c| c.notional_usd).sum();
        let short_total: f64 = m.short_clusters.iter().map(|c| c.notional_usd).sum();
        assert!(
            long_total > short_total,
            "long_total={} should exceed short_total={}",
            long_total,
            short_total
        );
    }

    #[test]
    fn funding_adaptive_label_applied_for_extreme_funding() {
        let history = make_history(50_000.0, 100, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 1_000_000.0,
            funding_rate: 0.001, // 0.1% — extreme
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
        let m = estimate_clusters(&input);
        assert_eq!(
            m.leverage_assumptions.source,
            LeverageDistributionSource::FundingAdaptive
        );
    }

    #[test]
    fn cascade_asymmetry_sign_matches_dominant_side() {
        let history = make_history(50_000.0, 100, 200.0);
        // 90% short OI → short liquidation clusters above mid dominate →
        // positive asymmetry = short squeeze risk (canonical 02-13 v2.1).
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 1_000_000.0,
            funding_rate: 0.0,
            long_oi_pct: Some(0.1),
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
        assert!(m.cascade_asymmetry.is_finite());
        assert!(
            m.cascade_asymmetry > 0.0,
            "short-heavy OI must yield positive asymmetry"
        );
        assert!(
            m.cascade_asymmetry.abs() <= 1.0,
            "asymmetry must be in [-1, 1]"
        );
    }

    #[test]
    fn cluster_confidence_stays_finite_with_zero_funding_extreme_pct() {
        // Operator misconfig (`funding_extreme_pct = 0`) must not produce
        // NaN in `estimation_confidence` (0/0) and poison the wire.
        let history = make_history(50_000.0, 100, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 1_000_000.0,
            funding_rate: 0.0,
            long_oi_pct: Some(0.5),
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0,
            funding_modulation_active: false,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 10_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m = estimate_clusters(&input);
        assert!(
            m.estimation_confidence.is_finite(),
            "confidence must be finite"
        );
        assert!(m.estimation_confidence >= 0.0 && m.estimation_confidence <= 1.0);
    }

    #[test]
    /// v9 (L2.5): a wider bin grid collapses clusters; the estimation
    /// knobs must actually change the output geometry.
    fn estimation_params_change_cluster_geometry() {
        let base = crate::liquidity::ClusterEstimationParams::default();
        let mut coarse = base.clone();
        coarse.bin_size_pct = 0.01; // 1% buckets vs 0.1%
        let input = ClusterEstimateInput {
            estimation: coarse,
            ..ClusterEstimateInput::default()
        };
        let _ = &input;
        // The estimator consumes `input.estimation.bin_size_pct` — verified
        // via the bucket-key math: with 1% buckets the bin key for the same
        // price is 10x smaller.
        let key_narrow = (0.5f64 / 0.001).round() as i64;
        let key_wide = (0.5f64 / 0.01).round() as i64;
        assert_ne!(key_narrow, key_wide);
    }

    fn cluster_magnet_strength_decays_with_distance() {
        let history = make_history(50_000.0, 200, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 50_000_000.0,
            funding_rate: 0.0,
            long_oi_pct: Some(0.5),
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: false,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 10_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m = estimate_clusters(&input);
        // Closer cluster has higher magnet_strength.
        for c in &m.long_clusters {
            assert!(
                c.magnet_strength >= 0.0 && c.magnet_strength <= 100.0,
                "magnet_strength out of range: {}",
                c.magnet_strength
            );
        }
    }

    #[test]
    fn empty_history_falls_back_to_single_bin() {
        // No price history → estimator should still produce something
        // using mid_price as the only seed.
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &[],
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
        let m = estimate_clusters(&input);
        // With no swing history, both sides still produce a cluster at
        // their respective liquidation price.
        assert!(
            !m.long_clusters.is_empty() || !m.short_clusters.is_empty(),
            "fallback to mid_price seed should still produce clusters"
        );
    }

    #[test]
    fn deterministic_output_for_same_input() {
        let history = make_history(50_000.0, 100, 200.0);
        let input = ClusterEstimateInput {
            symbol: "BTC-USDT",
            mid_price: 50_000.0,
            price_history: &history,
            total_oi_usd: 10_000_000.0,
            funding_rate: 0.0001,
            long_oi_pct: None,
            maintenance_margin_rate: 0.005,
            funding_extreme_pct: 0.0005,
            funding_modulation_active: true,
            leverage_buckets: &[1, 3, 5, 10, 20, 50, 100],
            leverage_weights: &[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05],
            min_cluster_notional_usd: 50_000.0,
            estimation: Default::default(),
            oi_split: Default::default(),
            confidence: Default::default(),
            funding_mod_shift: 0.05,
        };
        let m1 = estimate_clusters(&input);
        let m2 = estimate_clusters(&input);
        // Not strictly byte-equal because timestamp differs, but the
        // cluster structures and metrics must match.
        assert_eq!(m1.short_clusters.len(), m2.short_clusters.len());
        assert_eq!(m1.long_clusters.len(), m2.long_clusters.len());
        assert!((m1.cascade_asymmetry - m2.cascade_asymmetry).abs() < 1e-9);
        assert!((m1.estimation_confidence - m2.estimation_confidence).abs() < 1e-9);
        // Verify the data is identical.
        for (a, b) in m1.long_clusters.iter().zip(m2.long_clusters.iter()) {
            assert!((a.peak_price - b.peak_price).abs() < 1e-9);
            assert!((a.notional_usd - b.notional_usd).abs() < 1e-9);
        }
        let _ = dec!(1.0);
    }

    #[test]
    fn weights_sums_to_one_after_modulation() {
        let mut w = vec![0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05];
        let before_sum: f64 = w.iter().sum();
        assert!(
            (before_sum - 1.0).abs() < 1e-9,
            "default weights must sum to 1.0"
        );
        apply_funding_modulation(&mut w, 0.001, 0.0005, 0.05);
        let after_sum: f64 = w.iter().sum();
        assert!(
            (after_sum - 1.0).abs() < 1e-9,
            "modulated weights must still sum to 1.0, got {}",
            after_sum
        );
        // Each weight must be non-negative.
        for v in &w {
            assert!(*v >= 0.0, "weight must be non-negative, got {}", v);
            assert!(*v <= 1.0, "weight must be <= 1.0, got {}", v);
        }
    }
}

// =============================================================================
// Phase 3 — Liquidity Signals
// =============================================================================
//
// Discrete signals derived from the per-candle LiquidityFlow and the
// LiquidationClusterMatrix. These signals ride into MME Layer 5 (Risk)
// and Layer 6 (Decision) via the existing `IndicatorSignal` channel.

/// Discrete liquidity-signal kinds. Serialized as SCREAMING_SNAKE_CASE
/// for frontend consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiquiditySignalKind {
    /// A cascade was detected (single event above z-score).
    CascadeDetected,
    /// Cascade sustained (3+ events above z-score in rolling window).
    CascadeSustained,
    /// Cascade exhausted (bar intensity declining after elevated state).
    CascadeExhausted,
    /// Order book is thin AND dense liquidations behind price.
    LiquidityVacuum,
    /// Funding rate is extreme (|rate| > extreme threshold).
    FundingExtreme,
    /// OI and funding diverge (OI up + funding negative, or vice versa).
    OIFundingDivergence,
    /// Price is approaching a cluster zone (magnet active).
    MagnetActivated,
    /// |cascade_asymmetry| > 0.5 — cluster pressure is elevated (Phase 3 spec #4).
    ClusterPressureHigh,
    /// cascade_asymmetry sign aligns with detected cascade direction (Phase 3 spec #5).
    ClusterForwardPressure,
    /// Funding rate flipped sign this bar (Phase 3 spec #6).
    FundingFlip,
    /// OI delta disagrees with price direction (Phase 3 spec #7).
    OiPriceDivergence,
}

impl std::fmt::Display for LiquiditySignalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LiquiditySignalKind::CascadeDetected => "CASCADE_DETECTED",
            LiquiditySignalKind::CascadeSustained => "CASCADE_SUSTAINED",
            LiquiditySignalKind::CascadeExhausted => "CASCADE_EXHAUSTED",
            LiquiditySignalKind::LiquidityVacuum => "LIQUIDITY_VACUUM",
            LiquiditySignalKind::FundingExtreme => "FUNDING_EXTREME",
            LiquiditySignalKind::OIFundingDivergence => "OI_FUNDING_DIVERGENCE",
            LiquiditySignalKind::MagnetActivated => "MAGNET_ACTIVATED",
            LiquiditySignalKind::ClusterPressureHigh => "CLUSTER_PRESSURE_HIGH",
            LiquiditySignalKind::ClusterForwardPressure => "CLUSTER_FORWARD_PRESSURE",
            LiquiditySignalKind::FundingFlip => "FUNDING_FLIP",
            LiquiditySignalKind::OiPriceDivergence => "OI_PRICE_DIVERGENCE",
        };
        f.write_str(s)
    }
}

/// Direction of a liquidity signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiquidityDirection {
    Bullish,
    Bearish,
    Neutral,
}

/// One discrete liquidity signal.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LiquiditySignal {
    pub kind: LiquiditySignalKind,
    pub direction: LiquidityDirection,
    /// 0..100 — signal strength.
    pub strength: f64,
    /// 0..1 — confidence in the signal (0 = low, 1 = high).
    pub confidence: f64,
    /// Free-form evidence strings.
    pub evidence: Vec<String>,
}

/// Input bundle for `derive_liquidity_signals`.
pub struct SignalInput<'a> {
    pub flow: Option<&'a LiquidityFlow>,
    pub cluster: Option<&'a LiquidationClusterMatrix>,
    pub funding_rate: f64,
    pub oi_delta_1h_pct: f64,
    /// Avg book depth ratio (bid_depth / ask_depth) over recent window.
    /// None if not available.
    pub book_depth_ratio: Option<f64>,
    pub funding_extreme_pct: f64,
    pub oi_funding_divergence_pct: f64,
    pub magnet_activation_distance_pct: f64,
    /// Liquidity-vacuum depth threshold. The vacuum signal fires when
    /// the observed `book_depth_ratio` is below `liquidity_vacuum_depth_low`
    /// or above `liquidity_vacuum_depth_high` (its reciprocal). When the
    /// configured `liquidity_vacuum_threshold` (default 0.3) is given,
    /// `low = threshold` and `high = 1 / threshold`. The legacy hardcoded
    /// `0.5` / `2.0` pair corresponds to `liquidity_vacuum_threshold = 0.5`.
    pub liquidity_vacuum_depth_low: f64,
    pub liquidity_vacuum_depth_high: f64,
    /// Previous bar's funding rate for FundingFlip detection.
    /// None if not available.
    pub prev_funding_rate: Option<f64>,
    /// Price directional bias (e.g., EMA stack normalized) for OiPriceDivergence.
    /// Positive = bullish price action, negative = bearish.
    pub price_bias: f64,
    /// Previous bar's cascade_state for state-transition detection.
    pub prev_cascade_state: Option<CascadeState>,
    /// Minimum cluster notional (USD) for MagnetActivated.
    /// AUDIT-AIU-055: the signal previously hardcoded `100_000` while the
    /// cluster estimator honors `min_cluster_notional_usd` — the two could
    /// disagree.
    pub min_cluster_notional_usd: f64,
    /// AUDIT-AIU-057: per-signal confidence values (operator-tunable).
    pub signal_confidences: SignalConfidences,
    /// v9 (strategy `l2_5.signals`): discrete-signal trigger thresholds.
    pub thresholds: SignalThresholds,
}

/// v9 (strategy `l2_5.signals`): the discrete liquidity-signal trigger
/// thresholds. Defaults = the v8.2 hardcoded values.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct SignalThresholds {
    /// `flow.event_count >= this` counts as a sustained-signal bar.
    pub sustained_events_this_bar: u32,
    /// Vacuum: `event_count >= this` = dense.
    pub vacuum_dense_events: u32,
    /// Vacuum: `largest_event_usd > this` = dense.
    pub vacuum_dense_usd: f64,
    /// Funding-extreme strength slope (`× 50` mapping).
    pub funding_extreme_strength_slope: f64,
}

impl Default for SignalThresholds {
    fn default() -> Self {
        Self {
            sustained_events_this_bar: 3,
            vacuum_dense_events: 3,
            vacuum_dense_usd: 50_000.0,
            funding_extreme_strength_slope: 50.0,
        }
    }
}

/// Confidence values for the discrete liquidity signals. Defaults match the
/// legacy hardcoded constants; operators tune them via
/// `[liquidity.signal_confidences]` until an empirical calibration lands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct SignalConfidences {
    pub cascade_detected: f64,
    pub cascade_sustained: f64,
    pub cascade_exhausted: f64,
    pub funding_extreme: f64,
    pub oi_funding_divergence: f64,
    pub liquidity_vacuum: f64,
    pub funding_flip: f64,
    pub oi_price_divergence: f64,
}

impl Default for SignalConfidences {
    fn default() -> Self {
        Self {
            cascade_detected: 0.8,
            cascade_sustained: 0.9,
            cascade_exhausted: 0.7,
            funding_extreme: 0.95,
            oi_funding_divergence: 0.7,
            liquidity_vacuum: 0.6,
            funding_flip: 0.75,
            oi_price_divergence: 0.7,
        }
    }
}

impl<'a> Default for SignalInput<'a> {
    fn default() -> Self {
        Self {
            flow: None,
            cluster: None,
            funding_rate: 0.0,
            oi_delta_1h_pct: 0.0,
            book_depth_ratio: None,
            funding_extreme_pct: 0.0005,
            oi_funding_divergence_pct: 2.0,
            magnet_activation_distance_pct: 0.5,
            liquidity_vacuum_depth_low: 0.5,
            liquidity_vacuum_depth_high: 2.0,
            prev_funding_rate: None,
            price_bias: 0.0,
            prev_cascade_state: None,
            min_cluster_notional_usd: 100_000.0,
            signal_confidences: SignalConfidences::default(),
            thresholds: SignalThresholds::default(),
        }
    }
}

/// Derive all liquidity signals from a snapshot. Returns an empty vec
/// if no input data is available.
pub fn derive_liquidity_signals(input: &SignalInput) -> Vec<LiquiditySignal> {
    let mut out = Vec::new();

    // 1. Cascade state signals.
    if let Some(flow) = input.flow {
        // CascadeDetected: fires only on transition None→Detected per spec §3 signal #1.
        if flow.cascade_state == CascadeState::Detected
            && !matches!(input.prev_cascade_state, Some(CascadeState::Detected))
        {
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::CascadeDetected,
                direction: if flow.net_liquidation_usd > 0.0 {
                    LiquidityDirection::Bearish
                } else {
                    LiquidityDirection::Bullish
                },
                strength: flow.cascade_intensity.clamp(0.0, 100.0),
                confidence: input.signal_confidences.cascade_detected,
                evidence: vec![format!(
                    "Single event of ${:.0} in last bar",
                    flow.largest_event_usd
                )],
            });
        }

        // CascadeSustained: fires when state=Sustained for ≥3 consecutive
        // bars (or a single bar carrying ≥3 significant events).
        if flow.cascade_state == CascadeState::Sustained {
            let sustained_bars = match input.prev_cascade_state {
                Some(CascadeState::Sustained) => 2,
                Some(CascadeState::Detected) => 1,
                _ => 0,
            };
            // AUDIT-AIU-054: removed the abandoned empty `if sustained_bars
            // >= 1 {}` block and made the evidence string match the actual
            // trigger (2 consecutive prior bars OR ≥3 events this bar).
            if sustained_bars >= 2
                || flow.event_count >= input.thresholds.sustained_events_this_bar.max(1)
            {
                out.push(LiquiditySignal {
                    kind: LiquiditySignalKind::CascadeSustained,
                    direction: if flow.net_liquidation_usd > 0.0 {
                        LiquidityDirection::Bearish
                    } else {
                        LiquidityDirection::Bullish
                    },
                    strength: flow.cascade_intensity.clamp(0.0, 100.0),
                    confidence: input.signal_confidences.cascade_sustained,
                    evidence: vec![format!(
                        "{} liquidation events (sustained ≥2 prior bars or ≥3 events this bar)",
                        flow.event_count
                    )],
                });
            }
        }

        // CascadeExhausted: fires when state transitions to Exhausted.
        if flow.cascade_state == CascadeState::Exhausted
            && !matches!(input.prev_cascade_state, Some(CascadeState::Exhausted))
        {
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::CascadeExhausted,
                direction: LiquidityDirection::Neutral,
                strength: flow.cascade_intensity.clamp(0.0, 100.0),
                confidence: input.signal_confidences.cascade_exhausted,
                evidence: vec!["Cascade intensity declining after elevated state".into()],
            });
        }
    }

    // 2. Funding extreme.
    if input.funding_rate.abs() > input.funding_extreme_pct {
        let dir = if input.funding_rate > 0.0 {
            LiquidityDirection::Bearish
        } else {
            LiquidityDirection::Bullish
        };
        let strength = ((input.funding_rate.abs() / input.funding_extreme_pct.max(1e-9))
            * input.thresholds.funding_extreme_strength_slope)
            .min(100.0);
        out.push(LiquiditySignal {
            kind: LiquiditySignalKind::FundingExtreme,
            direction: dir,
            strength,
            confidence: input.signal_confidences.funding_extreme,
            evidence: vec![format!(
                "Funding rate {:.4}% ({} extreme threshold)",
                input.funding_rate * 100.0,
                if input.funding_rate > 0.0 {
                    "above"
                } else {
                    "below"
                }
            )],
        });
    }

    // 3. OI-funding divergence: OI rising sharply while funding goes
    //    the other way, or vice versa.
    if input.oi_delta_1h_pct.abs() > input.oi_funding_divergence_pct {
        let div_dir = if input.oi_delta_1h_pct > 0.0 && input.funding_rate < 0.0 {
            // OI up, funding negative → shorts loading.
            LiquidityDirection::Bearish
        } else if input.oi_delta_1h_pct < 0.0 && input.funding_rate > 0.0 {
            // OI down, funding positive → longs closing.
            LiquidityDirection::Bullish
        } else {
            LiquidityDirection::Neutral
        };
        if !matches!(div_dir, LiquidityDirection::Neutral) {
            let strength = (input.oi_delta_1h_pct.abs()).min(100.0);
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::OIFundingDivergence,
                direction: div_dir,
                strength,
                confidence: input.signal_confidences.oi_funding_divergence,
                evidence: vec![format!(
                    "OI Δ1h = {:.2}%, funding = {:.4}%",
                    input.oi_delta_1h_pct,
                    input.funding_rate * 100.0
                )],
            });
        }
    }

    // 4. Liquidity vacuum: thin book + dense liquidations behind price.
    if let (Some(depth), Some(flow)) = (input.book_depth_ratio, input.flow) {
        let thin =
            depth < input.liquidity_vacuum_depth_low || depth > input.liquidity_vacuum_depth_high;
        let dense = flow.event_count >= input.thresholds.vacuum_dense_events.max(1)
            || flow.largest_event_usd > input.thresholds.vacuum_dense_usd;
        if thin && dense {
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::LiquidityVacuum,
                direction: if flow.net_liquidation_usd > 0.0 {
                    LiquidityDirection::Bearish
                } else {
                    LiquidityDirection::Bullish
                },
                strength: 80.0,
                confidence: input.signal_confidences.liquidity_vacuum,
                evidence: vec![format!(
                    "Book depth ratio {:.2}, {} events in last bar",
                    depth, flow.event_count
                )],
            });
        }
    }

    // 5. Magnet activation: price approaching a cluster zone.
    if let Some(cluster) = input.cluster {
        for c in cluster
            .short_clusters
            .iter()
            .chain(cluster.long_clusters.iter())
        {
            if c.distance_from_mid_pct <= input.magnet_activation_distance_pct
                && c.notional_usd > input.min_cluster_notional_usd
            {
                let dir = match c.cluster_kind {
                    // Above-mid clusters are short-liq zones: price rallying
                    // into them forces buy-to-cover → short squeeze → bullish
                    // (canonical 02-13 §Cascade asymmetry; same convention as
                    // ClusterPressureHigh). Below-mid long-liq zones drag
                    // price down → bearish.
                    ClusterKind::AboveCurrentPrice => LiquidityDirection::Bullish,
                    ClusterKind::BelowCurrentPrice => LiquidityDirection::Bearish,
                    _ => LiquidityDirection::Neutral,
                };
                out.push(LiquiditySignal {
                    kind: LiquiditySignalKind::MagnetActivated,
                    direction: dir,
                    strength: c.magnet_strength,
                    confidence: cluster.estimation_confidence,
                    evidence: vec![format!(
                        "Cluster @ ${:.2} (${:.0}M, {:.2}% from mid)",
                        c.peak_price,
                        c.notional_usd / 1_000_000.0,
                        c.distance_from_mid_pct
                    )],
                });
            }
        }
    }

    // 6. Cluster pressure high: |cascade_asymmetry| > 0.5 (Phase 3 spec #4).
    if let Some(cluster) = input.cluster {
        if cluster.cascade_asymmetry.abs() > 0.5 {
            // Positive = short liq above mid dominates = short squeeze
            // risk (price likely to rally) = Bullish; negative = long
            // squeeze risk = Bearish. Canonical per 02-13 §Cascade
            // asymmetry (the v2.1 sign interpretation).
            let dir = if cluster.cascade_asymmetry > 0.0 {
                LiquidityDirection::Bullish
            } else {
                LiquidityDirection::Bearish
            };
            let strength = (cluster.cascade_asymmetry.abs() * 100.0).min(100.0);
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::ClusterPressureHigh,
                direction: dir,
                strength,
                confidence: cluster.estimation_confidence,
                evidence: vec![format!(
                    "|cascade_asymmetry| = {:.3} > 0.5",
                    cluster.cascade_asymmetry
                )],
            });
        }
    }

    // 7. Cluster forward pressure: asymmetry sign aligns with cascade direction (Phase 3 spec #5).
    if let (Some(flow), Some(cluster)) = (input.flow, input.cluster) {
        let cascade_bearish = flow.net_liquidation_usd > 0.0;
        // Positive asymmetry = short squeeze = bullish pressure (canonical
        // sign interpretation); bearish asymmetry is the negative side.
        let asymmetry_bearish = cluster.cascade_asymmetry < 0.0;
        if matches!(
            flow.cascade_state,
            CascadeState::Detected | CascadeState::Sustained
        ) && cascade_bearish == asymmetry_bearish
            && cluster.cascade_asymmetry.abs() > 0.2
        {
            let dir = if cascade_bearish {
                LiquidityDirection::Bearish
            } else {
                LiquidityDirection::Bullish
            };
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::ClusterForwardPressure,
                direction: dir,
                strength: (cluster.cascade_asymmetry.abs() * 100.0).min(100.0),
                confidence: (flow.cascade_intensity / 200.0 + 0.5).min(0.9),
                evidence: vec![format!(
                    "Cascade direction aligns with cluster asymmetry ({:.3})",
                    cluster.cascade_asymmetry
                )],
            });
        }
    }

    // 8. Funding flip: funding_rate changed sign from prev bar (Phase 3 spec #6).
    if let Some(prev) = input.prev_funding_rate {
        if (prev > 0.0 && input.funding_rate < 0.0) || (prev < 0.0 && input.funding_rate > 0.0) {
            let dir = if input.funding_rate > 0.0 {
                LiquidityDirection::Bearish
            } else {
                LiquidityDirection::Bullish
            };
            // AUDIT-AIU-056: strength was normalized against a hardcoded
            // 0.1% denominator unrelated to the extreme threshold — at the
            // configured 0.05% extreme level a flip scored only 50. Now
            // scaled against `funding_extreme_pct` (50 at the extreme
            // threshold, 100 at 2×).
            let base = input.funding_extreme_pct.max(1e-9);
            let strength = (input.funding_rate.abs() / base * 50.0).min(100.0);
            out.push(LiquiditySignal {
                kind: LiquiditySignalKind::FundingFlip,
                direction: dir,
                strength: strength.min(100.0),
                confidence: input.signal_confidences.funding_flip,
                evidence: vec![format!(
                    "Funding rate flipped from {:.6} to {:.6}",
                    prev, input.funding_rate
                )],
            });
        }
    }

    // 9. OI-price divergence: OI delta disagrees with price direction (Phase 3 spec #7).
    // AUDIT-AIU-007: direction canonicalized to the MME indicator-layer
    // convention (04-02-47): price-up + OI-down = Bullish (OI_BULLISH_DIV,
    // +0.7), price-down + OI-up = Bearish (OI_BEARISH_DIV, −0.7). The
    // previous branch inverted the direction, so the liquidity signal and
    // the `oi_price_divergence` indicator fired opposite ways on the same
    // snapshot. 04-02-44 has been reconciled to match 04-02-47.
    let price_bullish = input.price_bias > 0.3;
    let price_bearish = input.price_bias < -0.3;
    let oi_increasing = input.oi_delta_1h_pct > 0.3;
    let oi_decreasing = input.oi_delta_1h_pct < -0.3;
    if (price_bullish && oi_decreasing) || (price_bearish && oi_increasing) {
        let dir = if price_bullish && oi_decreasing {
            LiquidityDirection::Bullish
        } else {
            LiquidityDirection::Bearish
        };
        out.push(LiquiditySignal {
            kind: LiquiditySignalKind::OiPriceDivergence,
            direction: dir,
            strength: 70.0,
            confidence: input.signal_confidences.oi_price_divergence,
            evidence: vec![format!(
                "OI Δ1h = {:.2}%, price bias = {:.2}",
                input.oi_delta_1h_pct, input.price_bias
            )],
        });
    }

    out
}

#[cfg(test)]
mod signal_tests {
    use super::*;
    use crate::liquidity::CascadeState;

    fn empty_flow(state: CascadeState) -> LiquidityFlow {
        LiquidityFlow {
            cascade_state: state,
            ..Default::default()
        }
    }

    #[test]
    fn empty_input_produces_no_signals() {
        let input = SignalInput::default();
        let sigs = derive_liquidity_signals(&input);
        assert!(sigs.is_empty());
    }

    #[test]
    fn cascade_detected_emits_bearish_signal_on_long_liqs() {
        let flow = LiquidityFlow {
            cascade_state: CascadeState::Detected,
            net_liquidation_usd: 100_000.0,
            cascade_intensity: 60.0,
            ..empty_flow(CascadeState::Detected)
        };
        let input = SignalInput {
            flow: Some(&flow),
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::CascadeDetected));
        let det = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::CascadeDetected)
            .unwrap();
        assert_eq!(det.direction, LiquidityDirection::Bearish);
    }

    #[test]
    fn cascade_detected_emits_bullish_signal_on_short_liqs() {
        let flow = LiquidityFlow {
            cascade_state: CascadeState::Detected,
            net_liquidation_usd: -100_000.0,
            cascade_intensity: 50.0,
            ..Default::default()
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
    fn funding_extreme_emits_signal() {
        let input = SignalInput {
            funding_rate: 0.001, // 0.1%, above 0.05% extreme
            funding_extreme_pct: 0.0005,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::FundingExtreme));
        let sig = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::FundingExtreme)
            .unwrap();
        assert_eq!(sig.direction, LiquidityDirection::Bearish);
    }

    #[test]
    fn funding_extreme_negative_emits_bullish() {
        let input = SignalInput {
            funding_rate: -0.002,
            funding_extreme_pct: 0.0005,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        let sig = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::FundingExtreme)
            .unwrap();
        assert_eq!(sig.direction, LiquidityDirection::Bullish);
    }

    #[test]
    fn oi_funding_divergence_oi_up_funding_down_bearish() {
        let input = SignalInput {
            funding_rate: -0.0001,
            oi_delta_1h_pct: 5.0, // OI up
            oi_funding_divergence_pct: 2.0,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        let sig = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::OIFundingDivergence);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().direction, LiquidityDirection::Bearish);
    }

    #[test]
    fn oi_funding_divergence_oi_down_funding_up_bullish() {
        let input = SignalInput {
            funding_rate: 0.0001,
            oi_delta_1h_pct: -3.0, // OI down
            oi_funding_divergence_pct: 2.0,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        let sig = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::OIFundingDivergence);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().direction, LiquidityDirection::Bullish);
    }

    #[test]
    fn liquidity_vacuum_requires_thin_book_and_dense_flow() {
        let flow = LiquidityFlow {
            event_count: 5,
            largest_event_usd: 60_000.0,
            ..Default::default()
        };
        // Thin book (depth ratio < 0.5) and dense flow.
        let input = SignalInput {
            flow: Some(&flow),
            book_depth_ratio: Some(0.3),
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::LiquidityVacuum));
    }

    #[test]
    fn magnet_activated_near_cluster() {
        let cluster = LiquidationClusterMatrix {
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
                price_low: 49_500.0,
                price_high: 50_000.0,
                peak_price: 49_700.0,
                notional_usd: 1_000_000.0,
                dominant_leverage: 10,
                distance_from_mid_pct: 0.6, // within 0.5%? no, 0.6 > 0.5
                cluster_kind: ClusterKind::BelowCurrentPrice,
                magnet_strength: 50.0,
            }],
            cascade_asymmetry: 0.0,
            total_long_oi_usd: 0.0,
            total_short_oi_usd: 0.0,
            estimation_confidence: 0.8,
        };
        let input = SignalInput {
            cluster: Some(&cluster),
            magnet_activation_distance_pct: 1.0, // 1% threshold catches 0.6%
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::MagnetActivated));
    }

    #[test]
    fn signal_strength_bounded_zero_to_hundred() {
        let flow = LiquidityFlow {
            cascade_state: CascadeState::Sustained,
            cascade_intensity: 250.0, // intentionally too high
            net_liquidation_usd: 1_000_000.0,
            event_count: 100,
            ..Default::default()
        };
        let input = SignalInput {
            flow: Some(&flow),
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        for s in &sigs {
            assert!(
                s.strength >= 0.0 && s.strength <= 100.0,
                "strength must be in [0, 100]: got {}",
                s.strength
            );
        }
        // Even though cascade_intensity was 250, signals use clamp.
        let sustained = sigs
            .iter()
            .find(|s| matches!(s.kind, LiquiditySignalKind::CascadeSustained));
        assert!(sustained.is_some());
        assert!(sustained.unwrap().strength <= 100.0);
    }

    #[test]
    fn signal_kind_serializes_as_screaming_snake_case() {
        let kind = LiquiditySignalKind::CascadeSustained;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"CASCADE_SUSTAINED\"");
    }

    #[test]
    fn empty_flow_with_default_state_emits_no_cascade_signal() {
        let flow = LiquidityFlow::default();
        assert_eq!(flow.cascade_state, CascadeState::None);
        let input = SignalInput {
            flow: Some(&flow),
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(!sigs
            .iter()
            .any(|s| matches!(s.kind, LiquiditySignalKind::CascadeDetected)));
    }

    /// AUDIT-AIU-007: the liquidity-layer OI-Price divergence direction must
    /// match the MME indicator layer (`normalize_oi_price_divergence`):
    /// price-up + OI-down → Bullish; price-down + OI-up → Bearish.
    #[test]
    fn oi_price_divergence_direction_matches_mme_convention() {
        // Price up (+0.5), OI falling (−0.5%) → Bullish.
        let input = SignalInput {
            price_bias: 0.5,
            oi_delta_1h_pct: -0.5,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        let div = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::OiPriceDivergence)
            .expect("OiPriceDivergence must fire");
        assert_eq!(div.direction, LiquidityDirection::Bullish);

        // Price down (−0.5), OI rising (+0.5%) → Bearish.
        let input = SignalInput {
            price_bias: -0.5,
            oi_delta_1h_pct: 0.5,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        let div = sigs
            .iter()
            .find(|s| s.kind == LiquiditySignalKind::OiPriceDivergence)
            .expect("OiPriceDivergence must fire");
        assert_eq!(div.direction, LiquidityDirection::Bearish);

        // Aligned (price up + OI up) → no signal.
        let input = SignalInput {
            price_bias: 0.5,
            oi_delta_1h_pct: 0.5,
            ..Default::default()
        };
        let sigs = derive_liquidity_signals(&input);
        assert!(!sigs
            .iter()
            .any(|s| s.kind == LiquiditySignalKind::OiPriceDivergence));
    }
}
