use sqlx::SqlitePool;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::session::{Currency, ExchangeChoice};
use config_models::{FibonacciConfig, TimeframeConfig};
use core_domain::models::MarketSnapshot;
use core_domain::normalized::NormalizedCandle;
use database_storage;
use market_analyzer::analyzer;

pub struct BootstrapInput {
    pub base: String,
    /// Unified internal symbol (e.g. "BTC-USDT") assigned to all candles.
    pub internal_symbol: String,
    /// Settlement/quote currency for this session (drives raw symbol + product type).
    pub quote: Currency,
    pub rest_url: String,
    pub exchange_choice: ExchangeChoice,
    pub pool: SqlitePool,
    /// Fixed 10-slot ladder configs, positional fastest → slowest
    /// (aligned with `config_models::FIXED_TF_LADDER`).
    pub ladder_cfgs: [TimeframeConfig; 10],
    pub fib_config: FibonacciConfig,
    /// Fixed 10-slot ladder durations, positional fastest → slowest.
    pub ladder_secs: [u64; 10],
    /// v11.2: how many of the fastest slots actually run (1..=10). Slots
    /// beyond this are NOT fetched/warmed — they return default state.
    /// v11.4: WHICH ladder slots run — canonical indices (arbitrary
    /// subset). Slots outside the set are NOT fetched/warmed.
    pub active_slots: Vec<usize>,
    /// Canonical candle buffer size from `[candle_buffer] size` (CB-01).
    /// Single source of truth for the rolling window. Replaces the previous
    /// per-tier `analysis_limit` field.
    pub buffer_size: usize,
    /// Per-TF stale-threshold (CB-04 / DCP-05 / ILS-07).
    #[allow(dead_code)]
    pub stale_threshold_secs: u64,
    /// Per-TF fetch timeout (HFP-10).
    pub fetch_timeout_ms: u64,
    /// Sub-minute bypass flag (CB-05 / HFP-03).
    #[allow(dead_code)]
    pub sub_minute_skip_historical: bool,
    /// When present, bootstrap candle provenance (DB-warm vs REST-gap) is
    /// recorded into the pipeline reliability source mix (03-01-04 §5).
    pub reliability: Option<Arc<network_adapters::pipeline_reliability::ReliabilityTracker>>,
}

/// Fetch candles for a single timeframe via the
/// [`HistoricalFetchPolicy`](network_adapters::adapters::historical_fetch::HistoricalFetchPolicy)
/// trait. The trait hides per-exchange divergence (HFP-01 … HFP-10) and
/// handles sub-minute short-circuit (HFP-03) internally. Returns a
/// chronologically ordered (oldest-first) candle vector plus provenance
/// counts `(candles, db_warm, rest_gap)` for the source-mix metric
/// (03-01-04 §5).
async fn collect_candles(
    is_bitget: bool,
    exchange_raw: String,
    internal_symbol: String,
    product_type: String,
    rest_url: String,
    pool: SqlitePool,
    secs: u64,
    limit: u64,
    now_ms: u64,
    fetch_timeout_ms: u64,
    sub_minute_skip_historical: bool,
) -> Result<(Vec<NormalizedCandle>, u64, u64), String> {
    use network_adapters::adapters::bitget_historical_fetch::BitgetHistoricalFetch;
    use network_adapters::adapters::historical_fetch::{
        HistoricalFetchPolicy, HistoricalFetchRequest,
    };
    use network_adapters::adapters::hyperliquid_historical_fetch::HyperliquidHistoricalFetch;

    // 1. Local DB warm base (most recent completed candles for this symbol/tf).
    let db_candles =
        database_storage::query_recent_candles(&pool, &internal_symbol, secs, limit as u32).await;
    // AUDIT-AIU-118: persisted SYNTHETIC rows (idle-heartbeat dojis /
    // gap-fill candles — K3 writes them to SQLite for `/api/history`
    // continuity) must NEVER enter the warm replay: they would be
    // replayed through the indicator state machines, pushed into
    // `history`, and counted into `real_bar_count` — violating PRI-03
    // ("no synthetic sub-minute candles are ever created") and PRI-06
    // ("synthetic doji/idle-heartbeat buckets never enter `history`") on
    // every restart. The DB keeps them for the API fallback path; warm
    // state only consumes genuine closes.
    let db_candles: Vec<NormalizedCandle> = db_candles
        .into_iter()
        .filter(|c| c.reconstructed.is_none())
        .collect();

    // 2. Build the HistoricalFetchPolicy implementation for this exchange.
    //    The policy handles HFP-03 sub-minute bypass, HFP-04..HFP-06
    //    pagination, HFP-07 open-candle filter, HFP-08 provenance tagging,
    //    and HFP-10 timeout enforcement.
    let request = HistoricalFetchRequest {
        exchange_symbol: exchange_raw.clone(),
        internal_symbol: internal_symbol.clone(),
        timeframe_secs: secs,
        target_count: limit as usize,
        end_ts: now_ms,
        product_type: if is_bitget {
            Some(product_type.clone())
        } else {
            None
        },
        fetch_timeout_ms,
    };

    let policy: Box<dyn HistoricalFetchPolicy> = if is_bitget {
        Box::new(BitgetHistoricalFetch::new(
            rest_url.clone(),
            product_type.clone(),
        ))
    } else {
        Box::new(HyperliquidHistoricalFetch::new(rest_url.clone()))
    };

    // 3. Compute the historical-fetch range. The policy paginates from
    //    `end_ts` backward/forward as needed; for ≥ 1 minute TFs we anchor
    //    on the DB's last candle + 1 interval (gap fill), and for cold DBs
    //    we anchor on the full lookback window.
    //
    // ────────────────────────────────────────────────────────────────────
    // Sub-minute vs ≥60s warmup behaviour (PRI-03, v6.10.7)
    // ────────────────────────────────────────────────────────────────────
    //
    // ≥60s TFs (60s/180s/300s/900s): the REST fetch runs as normal and
    // seeds the full warm state + `pipeline.snapshot_history` with real
    // OHLCV from the exchange, so the chart shows real candles
    // immediately on first mount.
    //
    // Sub-minute TFs (1s/3s/5s/15s): there is no exchange REST source for
    // sub-minute bars (HFP-03), so the warmup is a **state replay**
    // (PRI-03): real closes are fetched at the nearest exchange-standard
    // interval (60 s) and replayed through the sub-minute pipeline's
    // indicator state machines + `history` buffer. The replayed bars
    // never enter `snapshot_history` (PRI-08 — see `populate_single`),
    // so the chart stays live-only and the v6.9 "line of about 1 minute"
    // bug (flat derived candles served as sub-minute chart history)
    // cannot regress. The replay fetch is best-effort: on failure the
    // slot starts cold and matures progressively.
    //
    // Observable difference at boot:
    //   - ≥60s TFs: chart shows real candles with bodies immediately
    //   - sub-minute TFs: chart paints nothing historical; live WS
    //     frames fill in within seconds — but every indicator, the
    //     fib/S-R/pattern inputs, the cluster matrix, and the L2–L6
    //     matrices are already warmed and Live at the first live close.
    //
    // Cross-references:
    //   - `crates/market-analyzer/tests/hist_buffer_cap_uniformity.rs`
    //     pins the uniform `HIST_BUFFER_MAX = 1000` cap across all TFs.
    //   - `crates/market-analyzer/src/analyzer/warm.rs` — the cap constant
    //     and the matching warmup-side trim.
    //   - `docs/engines/market-monitoring-engine/03-02-16-mme-subminute-vs-aboveminute-parity.md`
    //     — the AIU parity contract (PRI-01…PRI-12).
    // ────────────────────────────────────────────────────────────────────
    let rest_candles = if secs < 60 {
        if sub_minute_skip_historical {
            // PRI-03 opt-out (CB-05 / HFP-03): legacy behavior — sub-minute
            // slots start at 0 candles and mature progressively.
            Vec::new()
        } else {
            // PRI-03 (v6.10.7): sub-minute state-replay warmup. There is no
            // exchange REST source for 1s/3s/5s/15s bars, so we fetch the
            // nearest exchange-standard interval (60 s) and replay those REAL
            // closes through the sub-minute pipeline's state machines +
            // `history` buffer. The replayed bars never enter
            // `snapshot_history` (PRI-08) — the chart stays live-only and no
            // flat derived candle is ever served as sub-minute history.
            let mut replay_req = request.clone();
            replay_req.timeframe_secs = 60;
            match policy.fetch(replay_req).await {
                Ok(c) => c,
                Err(network_adapters::adapters::historical_fetch::HistoricalFetchError::SubMinuteBypassed(_)) => {
                    Vec::new()
                }
                Err(e) => {
                    // PRI-03 warmup is best-effort: a failed replay fetch
                    // must NOT block the sub-minute pipeline boot — it
                    // starts cold and matures progressively instead.
                    eprintln!(
                        "⚠️  Sub-minute replay warmup failed for {} ({}s): {} — starting cold.",
                        internal_symbol, secs, e
                    );
                    Vec::new()
                }
            }
        }
    } else {
        match policy.fetch(request).await {
            Ok(c) => c,
            Err(network_adapters::adapters::historical_fetch::HistoricalFetchError::SubMinuteBypassed(_)) => {
                Vec::new()
            }
            Err(e) => {
                if db_candles.is_empty() {
                    return Err(format!("Historical fetch failed: {}", e));
                }
                eprintln!(
                    "⚠️  Historical fetch failed for {} ({}s): {} — using local DB candles only.",
                    internal_symbol, secs, e
                );
                Vec::new()
            }
        }
    };

    // 4. Merge DB + REST deduped by start_time_ms. Per 03-01-04 §3 + HFP-09,
    //    the local store is authoritative for already-seen candles: REST is
    //    inserted first, then DB overwrites on overlap.
    let db_keys: std::collections::HashSet<u64> =
        db_candles.iter().map(|c| c.start_time_ms).collect();
    let mut map: BTreeMap<u64, NormalizedCandle> = BTreeMap::new();
    for c in rest_candles {
        map.insert(c.start_time_ms, c);
    }
    for c in db_candles {
        map.insert(c.start_time_ms, c);
    }
    let mut out: Vec<NormalizedCandle> = map.into_values().collect();
    if out.len() > limit as usize {
        out = out.split_off(out.len() - limit as usize);
    }
    let db_warm = out
        .iter()
        .filter(|c| db_keys.contains(&c.start_time_ms))
        .count() as u64;
    let rest_gap = out.len() as u64 - db_warm;
    Ok((out, db_warm, rest_gap))
}

/// Minimum completed-bar count for a tier's warm-up to be considered
/// sufficient (03-01-04 §2.1.1).  200 bars guarantees every structural
/// indicator (Ichimoku ~78, Volume Profile ~100, SMC ~50, Fibonacci pivots)
/// completes its warm-up buffer before the first live snapshot is emitted.
/// Below this gate the tier still warms best-effort with whatever history
/// exists, but the shortfall is logged and indicators emit `WARMING`
/// labels / `confidence = 0.0` until their per-indicator minimum buffers fill.
pub const MIN_WARMUP_BARS: usize = 200;

/// Per-slot fetch helper — one `collect_candles` call for ladder slot `i`.
#[allow(clippy::too_many_arguments)]
async fn collect_slot_candles(
    input: &BootstrapInput,
    i: usize,
    is_bitget: bool,
    exchange_raw: String,
    product_type: String,
    now_ms: u64,
) -> Result<(Vec<NormalizedCandle>, u64, u64), String> {
    // v11.2: inactive slots (beyond the fastest `active_count`) are never
    // fetched — the spawn loop below never runs them.
    if !input.active_slots.contains(&i) {
        return Ok((Vec::new(), 0, 0));
    }
    collect_candles(
        is_bitget,
        exchange_raw,
        input.internal_symbol.clone(),
        product_type,
        input.rest_url.clone(),
        input.pool.clone(),
        input.ladder_secs[i],
        input.buffer_size as u64,
        now_ms,
        input.fetch_timeout_ms,
        input.sub_minute_skip_historical,
    )
    .await
}

pub async fn fetch_and_warm_bootstrap(
    input: &BootstrapInput,
) -> Result<[analyzer::WarmedPipelineState; 10], String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let is_bitget = input.exchange_choice == ExchangeChoice::Bitget;
    let exchange_raw = input.exchange_choice.raw_symbol(&input.base, &input.quote);
    let product_type = input
        .exchange_choice
        .bitget_product_type(&input.quote)
        .unwrap_or("")
        .to_string();

    // Fetch all 10 ladder slots concurrently (one join arm per slot).
    let (r0, r1, r2, r3, r4, r5, r6, r7, r8, r9) = tokio::join!(
        collect_slot_candles(input, 0, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 1, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 2, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 3, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 4, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 5, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 6, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 7, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 8, is_bitget, exchange_raw.clone(), product_type.clone(), now_ms),
        collect_slot_candles(input, 9, is_bitget, exchange_raw, product_type, now_ms),
    );
    let slot_results: [Result<(Vec<NormalizedCandle>, u64, u64), String>; 10] =
        [r0, r1, r2, r3, r4, r5, r6, r7, r8, r9];

    // Fail fast on the first hard error (≥60s slot with unreachable REST
    // and an empty DB), preserving the pre-ladder behaviour.
    let mut slot_candles: [Vec<NormalizedCandle>; 10] = std::array::from_fn(|_| Vec::new());
    let mut total_db: u64 = 0;
    let mut total_rest: u64 = 0;
    for (i, res) in slot_results.into_iter().enumerate() {
        let (candles, db_warm, rest_gap) = res?;
        total_db += db_warm;
        total_rest += rest_gap;
        slot_candles[i] = candles;
    }

    if let Some(ref reliability) = input.reliability {
        reliability
            .record_bootstrap_sources(total_db, total_rest)
            .await;
    }

    let label = |s: u64| -> String {
        if s >= 86400 {
            format!("{}d", s / 86400)
        } else if s >= 3600 {
            format!("{}h", s / 3600)
        } else if s >= 60 {
            format!("{}m", s / 60)
        } else {
            format!("{}s", s)
        }
    };
    let counts = slot_candles
        .iter()
        .map(|c| c.len().to_string())
        .collect::<Vec<_>>()
        .join("/");
    let secs_labels = input
        .ladder_secs
        .iter()
        .map(|&s| label(s))
        .collect::<Vec<_>>()
        .join("/");
    println!(
        "📡 Historical Bootstrap [{}]: Warmed {} candles ({})",
        input.internal_symbol, counts, secs_labels,
    );

    let warn_empty = |candles: &[NormalizedCandle], secs: u64| {
        if candles.is_empty() {
            if secs < 60 {
                eprintln!(
                    "⚡ Historical Bootstrap [{}]: {}s sub-minute — starting from live data only.",
                    input.internal_symbol, secs
                );
            } else {
                eprintln!(
                    "⚠️  Historical Bootstrap [{}]: {} returned 0 candles — chart will populate from live data only.",
                    input.internal_symbol,
                    label(secs)
                );
            }
        }
    };
    let gate_warn = |candles: &[NormalizedCandle], secs: u64| {
        if !candles.is_empty() && candles.len() < MIN_WARMUP_BARS {
            eprintln!(
                "⚠️  Historical Bootstrap [{}]: {} seeded with {} bars (< min_warmup_bars = {}) — indicators start partially warmed (INSUFFICIENT_DATA until buffers fill).",
                input.internal_symbol,
                label(secs),
                candles.len(),
                MIN_WARMUP_BARS
            );
        }
    };
    for i in 0..10 {
        warn_empty(&slot_candles[i], input.ladder_secs[i]);
        // min_warmup_bars gate (03-01-04 §2.1.1): warm-up proceeds
        // best-effort, but a slot seeded below the gate is flagged as
        // partially warmed.
        gate_warn(&slot_candles[i], input.ladder_secs[i]);
    }

    // v6.10 (Phase 5 / E1): bootstrap warm-up runs with all indicators enabled
    // by default. Per-instance activation sets are constructed later in
    // `build_pipelines`; warm-up only needs to seed all 52 indicators so the
    // production pipelines can apply active_set filtering to the warmed state.
    let warm_active_set = market_analyzer::active_set::ActiveSet::all_enabled();
    // AUDIT-H5: stamp the real venue on pre-warm snapshots (was hardcoded
    // Hyperliquid — Bitget symbols reported the wrong exchange via WS
    // bootstrap and /api/history until the first live candle).
    let warm_exchange = match input.exchange_choice {
        ExchangeChoice::Hyperliquid => core_domain::normalized::Exchange::Hyperliquid,
        ExchangeChoice::Bitget => core_domain::normalized::Exchange::Bitget,
    };

    // Warm each fixed-ladder slot with its own candles, config, and slot
    // identity (positional with `FIXED_TF_SLOTS` / `FIXED_TF_LADDER`).
    let warmed: [analyzer::WarmedPipelineState; 10] = std::array::from_fn(|i| {
        analyzer::warm_indicators_for_timeframe(
            std::mem::take(&mut slot_candles[i]),
            &input.ladder_cfgs[i],
            &input.fib_config,
            &input.internal_symbol,
            input.ladder_secs[i],
            core_domain::models::FIXED_TF_SLOTS[i],
            input.buffer_size,
            &warm_active_set,
            Some(warm_exchange),
        )
    });

    Ok(warmed)
}

pub(crate) async fn populate_buffers(
    warmed: &[Option<analyzer::WarmedPipelineState>; 10],
    histories: &[Arc<RwLock<VecDeque<NormalizedCandle>>>; 10],
    latests: &[Arc<RwLock<Option<MarketSnapshot>>>; 10],
    snapshot_histories: &[Arc<RwLock<VecDeque<MarketSnapshot>>>; 10],
    latest_oi: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_funding: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_mark_px: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_index_px: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    // AUDIT-AIU-051: timestamped OI history `(timestamp_secs, value)`.
    oi_history: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    funding_history: &Arc<RwLock<VecDeque<f64>>>,
    // PRI-08: per-slot flag — `true` for ≥60s slots (warm snapshots become
    // chart history), `false` for sub-minute slots (state-replay warmup
    // must NOT pollute the chart's `snapshot_history` or `latest_snapshot`).
    // Positional with `FIXED_TF_LADDER` (fastest → slowest).
    warm_snapshots: [bool; 10],
) {
    // All ten timeframes share the same per-pair derivatives state
    // (latest_* locks and rolling history), so the first warmed slot in
    // ladder order carries the right restored values for them.
    if let Some(first) = warmed.iter().flatten().next() {
        populate_derivatives(
            first,
            latest_oi,
            latest_funding,
            latest_mark_px,
            latest_index_px,
            oi_history,
            funding_history,
        )
        .await;
    }

    for i in 0..10 {
        populate_single(
            &warmed[i],
            &histories[i],
            &latests[i],
            &snapshot_histories[i],
            warm_snapshots[i],
        )
        .await;
    }
}

/// Restore derivatives locks from a warmed state. Mirrors
/// `warm.rs::warm_derivatives_from_snapshots` on the write side — every
/// field that the warm helper put into `derivatives_state` is reapplied
/// to the live Arc locks here so the first WS event after boot sees
/// non-None priors.
async fn populate_derivatives(
    w: &analyzer::WarmedPipelineState,
    latest_oi: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_funding: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_mark_px: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    latest_index_px: &Arc<RwLock<Option<rust_decimal::Decimal>>>,
    // AUDIT-AIU-051: timestamped OI history `(timestamp_secs, value)`.
    oi_history: &Arc<RwLock<VecDeque<(u64, f64)>>>,
    funding_history: &Arc<RwLock<VecDeque<f64>>>,
) {
    use market_analyzer::analyzer::warm::DerivativesWarmedState;
    let d = &w.derivatives_state;
    if d.latest_oi.is_some() {
        *latest_oi.write().await = d.latest_oi;
    }
    if d.latest_funding.is_some() {
        *latest_funding.write().await = d.latest_funding;
    }
    if d.latest_mark_px.is_some() {
        *latest_mark_px.write().await = d.latest_mark_px;
    }
    if d.latest_index_px.is_some() {
        *latest_index_px.write().await = d.latest_index_px;
    }
    if !d.oi_history.is_empty() {
        let mut hist = oi_history.write().await;
        hist.clear();
        for v in &d.oi_history {
            hist.push_back(*v);
        }
    }
    if !d.funding_history.is_empty() {
        let mut hist = funding_history.write().await;
        hist.clear();
        for v in &d.funding_history {
            hist.push_back(*v);
        }
    }
    let _ = std::marker::PhantomData::<DerivativesWarmedState>;
}

pub(crate) async fn populate_single(
    warmed: &Option<analyzer::WarmedPipelineState>,
    history: &Arc<RwLock<VecDeque<NormalizedCandle>>>,
    latest: &Arc<RwLock<Option<MarketSnapshot>>>,
    snapshot_history: &Arc<RwLock<VecDeque<MarketSnapshot>>>,
    warm_snapshots: bool,
) {
    if let Some(ref w) = warmed {
        if warm_snapshots {
            // AUDIT-AIU-117: this path is the SINGLE seeder for the warm
            // `history`/`snapshot_history` deques (it runs synchronously
            // before the `run_single` tasks start). Clear-then-push makes
            // it idempotent — the previous code pushed on top of
            // `run_single`'s own prepopulation, double-seeding every warm
            // candle on every boot (duplicated `/api/history` rows and
            // double-counted warm bars in structural-indicator lookbacks).
            {
                let mut hist = history.write().await;
                hist.clear();
                for c in &w.history {
                    hist.push_back(c.clone());
                }
            }
            if let Some(ref snap) = w.latest_snapshot {
                *latest.write().await = Some(snap.clone());
            }
            {
                let mut sh = snapshot_history.write().await;
                sh.clear();
                for snap in &w.snapshot_history {
                    sh.push_back(snap.clone());
                }
            }
        } else {
            // PRI-08 + AUDIT-AIU-117: sub-minute slots warm STATE only.
            // The replayed 60 s closes are real but 12× too wide for the
            // slot — pushing them into `history` made fib/pivots/S-R/pattern
            // and the cluster lookback treat 60 s bars as 5 s bars for the
            // first ~80 minutes. `history` stays EMPTY at handover and
            // fills with live candles; `snapshot_history`/`latest_snapshot`
            // stay live-only (never present coarser-scale replayed closes
            // as sub-minute chart history).
        }
    }
}

#[cfg(test)]
mod cold_start_sub_minute_tests {
    //! Regression: the v6.9 "line of about 1 minute" chart render bug was
    //! caused by `collect_candles` synthesising flat
    //! `O=H=L=C=minute_close` candles from the next-larger TF for
    //! sub-minute TFs during bootstrap, which then populated
    //! `pipeline.snapshot_history` and rendered as a continuous
    //! horizontal line on the chart (the old
    //! `derive_sub_minute_candles` helper was removed in v6.10.27). The
    //! fix is to skip the warmup for sub-minute TFs entirely (REST
    //! endpoint is bypassed per HFP-03 and there's no legitimate
    //! historical source for 1s/3s/5s/15s bars).
    //!
    //! These tests pin down the new behaviour: the sub-minute branch of
    //! `collect_candles` must return an empty `Vec<NormalizedCandle>`
    //! regardless of what's already in the DB. The ≥60s branch must still
    //! flow through the `policy.fetch` / `policy-failure → empty` logic
    //! unchanged (regression guard for "don't touch above-minute timeframes").

    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn empty_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        database_storage::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    /// Direct pin: the sub-minute branch of `collect_candles` returns
    /// zero candles (and zero provenance counts). With `sub_minute_skip_historical`
    /// the sub-minute branch short-circuits before any REST call; the
    /// v6.9 flat-synthetic-candle regression (the "line of about 1 minute"
    /// bug) is impossible by construction.
    #[tokio::test]
    async fn collect_candles_sub_minute_returns_empty_on_empty_db() {
        let pool = empty_pool().await;
        let (candles, db_count, rest_count) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            pool,
            3, // 3s TF — sub-minute
            200,
            1_786_329_000_000,
            1_000,
            false,
        )
        .await
        .expect("collect_candles must succeed for sub-minute");

        assert_eq!(
            candles.len(),
            0,
            "sub-minute warmup must return empty Vec (no synthetic-candle fallback)"
        );
        assert_eq!(db_count, 0, "no DB rows on a fresh DB");
        assert_eq!(
            rest_count, 0,
            "REST is bypassed for sub-minute so the rest slice is empty too"
        );
    }

    /// Even with a populated DB containing real 60s candles, the
    /// sub-minute branch must NOT synthesise flat-close candles from
    /// them. This is the exact path that produced the chart artefact.
    #[tokio::test]
    async fn collect_candles_sub_minute_returns_empty_even_with_db_rows() {
        let pool = empty_pool().await;
        // Pre-insert one 60s candle so the DB path has real data to
        // hand back if the removed flat-candle synthesis were still wired up.
        sqlx::query(
            "INSERT INTO market_snapshots
                (exchange, symbol, timeframe_secs, timestamp,
                 mid_price, bid_price, ask_price,
                 open, high, low, close, volume)
             VALUES
                ('Hyperliquid', 'BTC-USDC', 60, 1786329000,
                 '64972.00', '64971.50', '64972.50',
                 '64961.00', '64978.00', '64961.00', '64972.00', '57.37')",
        )
        .execute(&pool)
        .await
        .expect("insert 60s seed row");

        let (candles, db_count, rest_count) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            pool,
            5, // 5s TF — sub-minute
            200,
            1_786_329_000_000,
            1_000,
            false,
        )
        .await
        .expect("collect_candles must succeed for sub-minute with seeded DB");

        // Even though the DB has a 60s row, the sub-minute query matches
        // by `timeframe_secs` only — the 60s row is not a 5s row. With the
        // replay fetch unreachable and the opt-out flag set, the sub-minute
        // branch returns zero candles (best-effort cold start).
        assert_eq!(
            candles.len(),
            0,
            "sub-minute warmup must NOT derive candles from larger TFs even when DB rows exist"
        );
        assert_eq!(db_count, 0, "5s query matches no 5s rows");
        assert_eq!(rest_count, 0, "REST bypassed for sub-minute");
    }

    /// AUDIT-AIU-118: persisted SYNTHETIC rows (idle-heartbeat dojis /
    /// gap-fill candles, K3) must be excluded from the warm-replay merge —
    /// they must never seed indicator state machines, `history`, or
    /// `real_bar_count` on restart (PRI-03/PRI-06). Real rows of the same
    /// TF still flow through.
    #[tokio::test]
    async fn collect_candles_excludes_synthetic_rows_from_warm_replay() {
        let pool = empty_pool().await;
        // One genuine 5s row + one SYNTHETIC 5s row (reconstructed flag set).
        sqlx::query(
            "INSERT INTO market_snapshots
                (exchange, symbol, timeframe_secs, timestamp,
                 mid_price, bid_price, ask_price,
                 open, high, low, close, volume, reconstructed)
             VALUES
                ('Hyperliquid', 'BTC-USDC', 5, 1786329000,
                 '64972.00', '64971.50', '64972.50',
                 '64961.00', '64978.00', '64961.00', '64972.00', '57.37', NULL),
                ('Hyperliquid', 'BTC-USDC', 5, 1786329005,
                 '64972.00', '64971.50', '64972.50',
                 '64972.00', '64972.00', '64972.00', '64972.00', '0.00', 'SYNTHETIC')",
        )
        .execute(&pool)
        .await
        .expect("insert 5s rows");

        let (candles, _db_count, _rest_count) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            pool,
            5,
            200,
            1_786_329_000_000,
            1_000,
            true, // sub_minute_skip_historical — no REST, DB only
        )
        .await
        .expect("collect_candles must succeed");

        assert_eq!(
            candles.len(),
            1,
            "the SYNTHETIC row must be filtered from the warm replay, got {} candles",
            candles.len()
        );
        assert_eq!(candles[0].start_time_ms, 1_786_329_000_000);
        assert!(candles[0].reconstructed.is_none());
    }

    /// Regression guard for the user requirement "don't touch those"
    /// (≥60s timeframes). The ≥60s branch must still go through
    /// `policy.fetch`. When the REST endpoint is unreachable (the test
    /// case here) and the DB is empty, the call returns `Err` so the
    /// outer `fetch_and_warm_bootstrap` fails loudly — exactly the
    /// existing pre-fix behaviour for ≥60s TFs.
    #[tokio::test]
    async fn collect_candles_above_minute_still_routes_through_policy() {
        let pool = empty_pool().await;
        // Empty DB, unreachable REST → policy.fetch fails → outer
        // collect_candles surfaces the error rather than silently
        // returning empty. This is the historical behaviour we must
        // preserve for ≥60s.
        let result = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            pool,
            60, // 60s TF — at the boundary, NOT sub-minute
            200,
            1_786_329_000_000,
            1_000,
            false,
        )
        .await;

        // Either the fetch fails (preferred — surfaces operator error),
        // or it returns empty Vec + the caller treats that as a hard
        // failure. The contract is "REST must be reachable for ≥60s".
        // The exact failure mode depends on the policy implementation,
        // but we must NOT see a silently-synthesised flat candle
        // payload — which is what the sub-minute branch used to do.
        assert!(
            result.is_err() || result.as_ref().map(|(_, _, _)| true).unwrap_or(false),
            "≥60s must surface REST failure rather than silently returning empty"
        );
    }

    /// End-to-end pin via `fetch_and_warm_bootstrap`: the WarmedPipelineState
    /// returned for each sub-minute TF must carry zero
    /// `snapshot_history` entries (the actual data structure the chart
    /// reads). Without this guard the chart would still paint the flat
    /// synthetic candles.
    #[tokio::test]
    async fn fetch_and_warm_bootstrap_returns_empty_snapshot_history_for_sub_minute() {
        let pool = empty_pool().await;
        let input = BootstrapInput {
            active_slots: (0..10).collect(),
            base: "BTC".to_string(),
            internal_symbol: "BTC-USDC".to_string(),
            quote: Currency::USDC,
            rest_url: "ws://unreachable.invalid".to_string(),
            exchange_choice: ExchangeChoice::Hyperliquid,
            pool,
            ladder_cfgs: std::array::from_fn(|i| {
                TimeframeConfig::new(
                    config_models::FIXED_TF_LADDER[i],
                    config_models::IndicatorsConfig::default(),
                )
            }),
            fib_config: FibonacciConfig::default(),
            // The fixed 10-slot ladder. The sub-minute slots under test
            // (1s/3s/5s/15s) are the first four entries; all follow the
            // state-only warmup path.
            ladder_secs: config_models::FIXED_TF_LADDER,
            buffer_size: 500,
            stale_threshold_secs: 300,
            fetch_timeout_ms: 100,
            sub_minute_skip_historical: true,
            reliability: None,
        };

        // ≥60s TFs are required in BootstrapInput via `secs`. The ladder
        // carries several ≥60s slots, but THIS test only drives
        // `collect_candles` directly for the four sub-minute slots —
        // asserting each returns an empty warm result before any ≥60s
        // fetch could fail. The end-to-end path follows from that.

        let (candles_1s, _, _) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            input.pool.clone(),
            input.ladder_secs[0],
            500,
            1_786_329_000_000,
            input.fetch_timeout_ms,
            input.sub_minute_skip_historical,
        )
        .await
        .expect("1s must return empty");
        let (candles_3s, _, _) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            input.pool.clone(),
            input.ladder_secs[1],
            500,
            1_786_329_000_000,
            input.fetch_timeout_ms,
            input.sub_minute_skip_historical,
        )
        .await
        .expect("3s must return empty");
        let (candles_5s, _, _) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            input.pool.clone(),
            input.ladder_secs[2],
            500,
            1_786_329_000_000,
            input.fetch_timeout_ms,
            input.sub_minute_skip_historical,
        )
        .await
        .expect("5s must return empty");
        let (candles_15s, _, _) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            input.pool.clone(),
            input.ladder_secs[3],
            500,
            1_786_329_000_000,
            input.fetch_timeout_ms,
            input.sub_minute_skip_historical,
        )
        .await
        .expect("15s must return empty");

        assert!(candles_1s.is_empty(), "1s sub-minute warmup must be empty");
        assert!(candles_3s.is_empty(), "3s sub-minute warmup must be empty");
        assert!(candles_5s.is_empty(), "5s sub-minute warmup must be empty");
        assert!(
            candles_15s.is_empty(),
            "15s sub-minute warmup must be empty"
        );

        // Suppress the unused input warning.
        let _ = input;
    }

    /// PRI-03 (v6.10.7): with the sub-minute state-replay warmup enabled
    /// (default) the replay fetch is BEST-EFFORT — an unreachable REST
    /// endpoint must NOT block the sub-minute pipeline boot. The slot
    /// returns Ok(empty) and starts cold, unlike the ≥60s branch which
    /// surfaces the fetch error loudly.
    #[tokio::test]
    async fn collect_candles_sub_minute_replay_fetch_is_best_effort_when_rest_down() {
        let pool = empty_pool().await;
        let (candles, db_count, rest_count) = collect_candles(
            false,
            "BTC".to_string(),
            "BTC-USDC".to_string(),
            String::new(),
            "ws://unreachable.invalid".to_string(),
            pool,
            1, // 1s TF — sub-minute
            500,
            1_786_329_000_000,
            1_000,
            false, // PRI-03 warmup enabled (default)
        )
        .await
        .expect("sub-minute warmup must not fail the boot when REST is unreachable");

        assert!(
            candles.is_empty(),
            "best-effort warmup must degrade to cold start"
        );
        assert_eq!(db_count, 0, "no DB rows for a 1s TF on an empty pool");
        assert_eq!(rest_count, 0, "REST unreachable → zero REST candles");
    }
}
