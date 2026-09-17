use rust_decimal::prelude::ToPrimitive;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::instance::{Instance, TimeframeBuffers};
use crate::registry_context::RegistryContext;
use crate::session::{Currency, ExchangeChoice};
use config_models::{
    ApiFailoverConfig, FibonacciConfig, HeatmapConfig, IntervalsConfig, LiquidityConfig,
    OperationalMode, OrderBookConfig, SafetyConfig, StrategyConfig, TimeframeConfig,
};
use core_domain::liquidity::{ClusterRefreshStatus, ClusterStatusSnapshot};
use core_domain::models::{CandlePipelineState, MarketSnapshot};
use core_domain::normalized::{NormalizedCandle, NormalizedEvent};
use database_storage;
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use tokio_util::sync::CancellationToken;

pub struct PipelineContext {
    pub base: String,
    /// Unified internal symbol (e.g. "BTC-USDT") used across the state.
    pub internal_symbol: String,
    /// Settlement/quote currency for this session.
    pub quote: Currency,
    pub pair_key: String,
    pub exchange_choice: ExchangeChoice,
    /// v11.9: the ACTIVE timeframe configs — one per configured duration,
    /// ordered ascending fastest → slowest. Only ACTIVE durations are
    /// constructed and spawned (no inert pipelines).
    pub ladder_cfgs: Vec<TimeframeConfig>,
    pub fib_config: FibonacciConfig,
    pub safety_config: SafetyConfig,
    pub intervals_config: IntervalsConfig,
    pub cancel: CancellationToken,
    pub operational_mode: OperationalMode,
    #[allow(dead_code)]
    pub weight_overrides: Option<std::collections::HashMap<String, i32>>,
    pub liquidity_config: LiquidityConfig,
    /// API-failover tolerance knobs (derivatives poller disable threshold).
    pub api_failover: ApiFailoverConfig,
    /// Heatmap bucketing configuration (Block B). Independent from
    /// `liquidity_config` so the bucket aggregation can be disabled
    /// without affecting the rest of the liquidity pipeline. See
    /// `config_models::HeatmapConfig`.
    pub heatmap_config: HeatmapConfig,
    /// v9 (F-04): order-book depth knobs — the `[order_book]` TOML surface
    /// (previously the pipeline hardcoded `OrderBookConfig::default()`).
    pub ob_config: OrderBookConfig,
    /// v9: the effective strategy (patch-resolved).
    pub strategy: StrategyConfig,
    /// Canonical candle buffer size from `[candle_buffer] size` (CB-01).
    pub buffer_size: usize,
    /// Per-TF stale-threshold (CB-04 / DCP-05 / ILS-07).
    pub stale_threshold_secs: u64,
    /// Global `[activation]` block (CA-01…CA-15). Applied to every
    /// instance; per-instance overrides union on top.
    pub activation: config_models::ActivationConfig,
    /// Per-instance `[instances.*.activation]` overrides (union with
    /// global; `None` = no overrides).
    pub activation_instance: Option<config_models::ActivationConfig>,
    /// `WorkspaceConfig.config_version` — attributed to `metrics_config`
    /// for change attribution (CA-10).
    pub config_version: u64,
}

pub struct PipelineArtifacts {
    pub instance: Arc<Instance>,
    /// v11.9: the ACTIVE buffers — one per configured duration, ascending
    /// fastest → slowest (keeps `Instance` construction symmetric).
    pub buffers: Vec<TimeframeBuffers>,
}

pub async fn build_pipelines(
    ctx: &PipelineContext,
    state: &RegistryContext,
    warmed_states: Option<Vec<analyzer::WarmedPipelineState>>,
) -> PipelineArtifacts {
    let (snapshot_tx, snapshot_rx) = mpsc::channel::<NormalizedEvent>(500);
    let cancel = ctx.cancel.clone();

    // v11.9: the ACTIVE durations drive every handle set — one entry per
    // configured duration, ascending fastest → slowest.
    let active_secs: Vec<u64> = ctx
        .ladder_cfgs
        .iter()
        .map(|c| c.candles.duration_seconds)
        .collect();
    let n = active_secs.len();

    let broadcast_txs: Vec<tokio::sync::broadcast::Sender<MarketSnapshot>> = (0..n)
        .map(|_| tokio::sync::broadcast::channel::<MarketSnapshot>(200).0)
        .collect();

    let histories: Vec<Arc<RwLock<VecDeque<NormalizedCandle>>>> = (0..n)
        .map(|_| {
            Arc::new(RwLock::new(VecDeque::<NormalizedCandle>::with_capacity(
                ctx.buffer_size,
            )))
        })
        .collect();

    let latests: Vec<Arc<RwLock<Option<MarketSnapshot>>>> = (0..n)
        .map(|_| Arc::new(RwLock::new(None::<MarketSnapshot>)))
        .collect();

    let snapshot_histories: Vec<Arc<RwLock<VecDeque<MarketSnapshot>>>> = (0..n)
        .map(|_| {
            Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::with_capacity(
                ctx.buffer_size,
            )))
        })
        .collect();

    // Per-TF cluster-matrix handles (Phase 2). Each TF pipeline gets its
    // own handle so the dashboard charts can show clusters at their own
    // horizons. Populated by the cluster refresh tasks spawned below;
    // read by `run_single` on every candle close.
    let cluster_matrices: Vec<
        Arc<RwLock<Option<core_domain::liquidity::LiquidationClusterMatrix>>>,
    > = (0..n).map(|_| Arc::new(RwLock::new(None))).collect();

    // Per-TF cluster-refresh status handles (sibling to the matrix handles).
    // The refresh task writes to both on every tick; the
    // `/api/liquidity/cluster-status` endpoint reads these so the UI can
    // distinguish "no data yet" (Pending) from "refresh task failed"
    // (Skipped with reason) — without this distinction the LIQ HEATMAP can
    // appear empty for minutes at boot with zero operator feedback.
    let cluster_statuses: Vec<Arc<RwLock<core_domain::liquidity::ClusterStatusSnapshot>>> = (0..n)
        .map(|i| {
            Arc::new(RwLock::new(
                core_domain::liquidity::ClusterStatusSnapshot::pending(
                    ctx.pair_key.as_str(),
                    &core_domain::duration_label(active_secs[i]),
                ),
            ))
        })
        .collect();

    // v6.10 (Phase 5 / E1): build the real ActiveSet from config instead
    // of the default all-enabled set. Global `[activation]` + per-instance
    // `[instances.*.activation]` union (CA-06: disabled ≡ absent), with
    // the `[liquidity] enabled` master switch folded in (CA-15). This is
    // the single production call-site — the pipeline structs below, the
    // run_single tasks, and the cluster-refresh spawn all read from it.
    // v9: the strategy's `l1_5` section is the single source of truth for
    // the liquidity pipeline (legacy `[workspace.liquidity]`/`[heatmap]`
    // remain only as the parse fallback).
    let eff_liq = market_analyzer::liquidity_params::effective_liquidity(
        Some(&ctx.liquidity_config),
        Some(&ctx.heatmap_config),
        &ctx.strategy.l1_5,
    );

    let mut active_set = market_analyzer::active_set::ActiveSet::from_config(
        &ctx.activation,
        ctx.activation_instance.as_ref(),
        ctx.config_version,
        eff_liq.cfg.enabled,
    );
    // v9: the strategy's L1.5 sub-toggles override the legacy
    // `[activation]`-derived state.
    active_set.liquidity_enabled = eff_liq.cfg.enabled;
    active_set.liquidation_feed = eff_liq.cfg.liquidation_feed;
    active_set.cluster_estimation = eff_liq.cfg.cluster_estimation;
    active_set.liquidity_signals_enabled = eff_liq.cfg.signals;

    // v11.9: per-duration pipelines, built over the ACTIVE set then moved
    // into the `ActivePair.pipelines` vec (duration-ordered).
    let pipes: Vec<analyzer::TimeframePipeline> = (0..n)
        .map(|i| analyzer::TimeframePipeline {
            slot_label: core_domain::duration_label(active_secs[i]),
            history: histories[i].clone(),
            broadcast_tx: broadcast_txs[i].clone(),
            latest_snapshot: latests[i].clone(),
            snapshot_history: snapshot_histories[i].clone(),
            timeframe_secs: active_secs[i],
            divergence_detector: Arc::new(tokio::sync::Mutex::new(DivergenceDetector::new(20))),
            sr_tracker: Arc::new(tokio::sync::Mutex::new(SrRoleTracker::new(0.003))),
            fibonacci: ctx.fib_config.clone(),
            latest_oi: Arc::new(RwLock::new(None)),
            latest_funding: Arc::new(RwLock::new(None)),
            latest_mark_px: Arc::new(RwLock::new(None)),
            latest_index_px: Arc::new(RwLock::new(None)),
            active_set: active_set.clone(),
            cluster_matrix: cluster_matrices[i].clone(),
            cluster_status: cluster_statuses[i].clone(),
            pipeline_state: Arc::new(RwLock::new(CandlePipelineState::Initializing)),
            indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
            advisory: Arc::new(RwLock::new(None)),
            tf_leverage_config: Arc::new(ctx.ladder_cfgs[i].leverage.clone()),
            buffer_size: ctx.buffer_size,
            stale_threshold_secs: ctx.stale_threshold_secs,
        })
        .collect();

    let active_pair = Arc::new(analyzer::ActivePair {
        symbol: ctx.internal_symbol.clone(),
        pipelines: pipes,
        active_secs: active_secs.clone(),
        snapshot_tx: snapshot_tx.clone(),
        cancel: cancel.clone(),
        latest_oi: Arc::new(RwLock::new(None)),
        latest_funding: Arc::new(RwLock::new(None)),
        latest_mark_px: Arc::new(RwLock::new(None)),
        latest_index_px: Arc::new(RwLock::new(None)),
        oi_history: Arc::new(RwLock::new(VecDeque::with_capacity(60))), // AUDIT-AIU-051: (timestamp_secs, value)
        funding_history: Arc::new(RwLock::new(VecDeque::with_capacity(8))),
        latency_tracker: state.latency_tracker.clone(),
    });

    spawn_tasks(
        snapshot_rx,
        &ctx.base,
        &ctx.internal_symbol,
        &ctx.pair_key,
        &ctx.ladder_cfgs,
        &active_secs,
        &ctx.fib_config,
        &cancel,
        &broadcast_txs,
        &histories,
        &latests,
        &snapshot_histories,
        &active_pair,
        state,
        warmed_states,
        ctx.exchange_choice,
        ctx.quote,
        ctx.liquidity_config.clone(),
        ctx.heatmap_config.clone(),
        ctx.api_failover,
        // v9: wired order-book config + the effective strategy.
        ctx.ob_config.clone(),
        ctx.strategy.clone(),
        &cluster_matrices,
        &cluster_statuses,
        ctx.buffer_size,
        // AUDIT-H7: configured stale threshold (CB-04/ILS-07).
        ctx.stale_threshold_secs,
        // CA-15: L2.5 `cluster_estimation` toggle from the ActiveSet.
        active_set.cluster_estimation,
    )
    .await;

    // Per-duration chart/telemetry buffers handed to the `Instance`,
    // ascending fastest → slowest.
    let buffers: Vec<TimeframeBuffers> = (0..n)
        .map(|i| TimeframeBuffers {
            history: histories[i].clone(),
            latest: latests[i].clone(),
            snapshot_history: snapshot_histories[i].clone(),
        })
        .collect();

    let instance = Arc::new(Instance::new(
        format!("inst_{}", uuid_v4_simple()),
        (ctx.base.clone(), ctx.quote.as_str().to_string()),
        ctx.exchange_choice,
        active_pair.clone(),
        state.pool.clone(),
        state.workspace.clone(),
        ctx.intervals_config.clone(),
        ctx.safety_config.clone(),
        buffers.clone(),
        active_secs.clone(),
        ctx.operational_mode.clone(),
    ));

    PipelineArtifacts { instance, buffers }
}

#[allow(clippy::too_many_arguments)]
async fn spawn_tasks(
    snapshot_rx: mpsc::Receiver<NormalizedEvent>,
    base: &str,
    internal_symbol: &str,
    pair_key: &str,
    ladder_cfgs: &[TimeframeConfig],
    active_secs: &[u64],
    fib_config: &FibonacciConfig,
    cancel: &CancellationToken,
    broadcast_txs: &[tokio::sync::broadcast::Sender<MarketSnapshot>],
    histories: &[Arc<RwLock<VecDeque<NormalizedCandle>>>],
    latests: &[Arc<RwLock<Option<MarketSnapshot>>>],
    snapshot_histories: &[Arc<RwLock<VecDeque<MarketSnapshot>>>],
    active_pair: &Arc<analyzer::ActivePair>,
    state: &RegistryContext,
    warmed_states: Option<Vec<analyzer::WarmedPipelineState>>,
    exchange_choice: ExchangeChoice,
    quote: Currency,
    liquidity_config: LiquidityConfig,
    heatmap_config: HeatmapConfig,
    api_failover: ApiFailoverConfig,
    // v9: wired order-book config + the effective strategy.
    ob_config: OrderBookConfig,
    strategy: StrategyConfig,
    cluster_matrices: &[Arc<RwLock<Option<core_domain::liquidity::LiquidationClusterMatrix>>>],
    cluster_statuses: &[Arc<RwLock<core_domain::liquidity::ClusterStatusSnapshot>>],
    buffer_size: usize,
    // AUDIT-H7: `[candle_buffer] stale_threshold_secs` (CB-04/ILS-07) —
    // threaded into run_single (was hardcoded 300 inside the analyzer).
    stale_threshold_secs: u64,
    // CA-15: `[activation] cluster_estimation` (union of global +
    // per-instance). `false` => the L2.5 refresh loop is not spawned.
    cluster_estimation: bool,
) {
    let n = active_secs.len();
    // v9: the strategy's `l1_5` section is the single source of truth for
    // the liquidity pipeline (legacy sections remain only as the fallback
    // baked into the EffectiveLiquidity resolver).
    let eff_liq = market_analyzer::liquidity_params::effective_liquidity(
        Some(&liquidity_config),
        Some(&heatmap_config),
        &strategy.l1_5,
    );
    let liquidity_config = eff_liq.cfg.clone();
    let _ = eff_liq;

    // One event channel per fixed-ladder slot (10 total, fastest → slowest).
    let mut pipeline_txs: Vec<mpsc::Sender<NormalizedEvent>> = Vec::with_capacity(10);
    let mut pipeline_rxs: Vec<mpsc::Receiver<NormalizedEvent>> = Vec::with_capacity(10);
    for _ in 0..10 {
        let (tx, rx) = mpsc::channel::<NormalizedEvent>(200);
        pipeline_txs.push(tx);
        pipeline_rxs.push(rx);
    }

    let router_symbol = internal_symbol.to_string();
    let router_cancel = cancel.clone();
    tokio::spawn(async move {
        analyzer::run_event_router(snapshot_rx, pipeline_txs, router_symbol, router_cancel).await;
    });

    let (candle_fwd_tx, mut candle_fwd_rx) = tokio::sync::mpsc::channel::<NormalizedCandle>(1000);
    let (candle_bcast_tx, candle_bcast_rx) =
        tokio::sync::broadcast::channel::<NormalizedCandle>(1200);

    tokio::spawn(async move {
        while let Some(candle) = candle_fwd_rx.recv().await {
            let _ = candle_bcast_tx.send(candle);
        }
    });

    let (agg_tx, mut agg_rx) =
        tokio::sync::mpsc::channel::<market_analyzer::candle_aggregator::AggregatedCandle>(200);
    let agg_symbol = internal_symbol.to_string();

    // Spawn the candle aggregator — the ACTIVE set IS the target set
    // (every active duration is aggregated from the fastest duration's
    // forwarded candles).
    tokio::spawn(market_analyzer::candle_aggregator::spawn_candle_aggregator(
        agg_symbol.clone(),
        candle_bcast_rx,
        agg_tx,
        active_secs.to_vec(),
    ));

    let logger_agg_telemetry = state.telemetry_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(ac) = agg_rx.recv() => {
                    let _ = logger_agg_telemetry.send(database_storage::TelemetryMsg::ConsoleLog(format!(
                        "🕯️  [{}] {}s Candle Aggregated | Close: ${:.4} | Sources: {}",
                        agg_symbol, ac.timeframe_secs, ac.candle.close, ac.source_count
                    ))).await;
                }
                else => break,
            }
        }
    });

    if let Some(ref ws) = warmed_states {
        if let Some(first) = ws.first() {
            for c in &first.history {
                let _ = candle_fwd_tx.send(c.clone()).await;
            }
        }
    }

    // Spawn one pipeline task per ACTIVE duration
    let warmed_opts: Vec<Option<analyzer::WarmedPipelineState>> = match &warmed_states {
        Some(ws) => (0..n).map(|i| ws.get(i).cloned()).collect(),
        None => (0..n).map(|_| None).collect(),
    };

    #[allow(clippy::type_complexity)]
    let mut pipeline_specs: Vec<(
        mpsc::Receiver<NormalizedEvent>,
        TimeframeConfig,
        Arc<RwLock<VecDeque<NormalizedCandle>>>,
        Arc<RwLock<Option<MarketSnapshot>>>,
        Arc<RwLock<VecDeque<MarketSnapshot>>>,
        String,
        u64,
        tokio::sync::broadcast::Sender<MarketSnapshot>,
        Arc<tokio::sync::Mutex<DivergenceDetector>>,
        Option<tokio::sync::mpsc::Sender<NormalizedCandle>>,
        Option<analyzer::WarmedPipelineState>,
        market_analyzer::active_set::ActiveSet,
    )> = Vec::with_capacity(n);
    let pair_pipes = active_pair.all();
    let mut rx_iter = pipeline_rxs.into_iter();
    // v11.9: every ACTIVE duration is spawned (there are no inert slots).
    for i in 0..n {
        pipeline_specs.push((
            rx_iter.next().expect("one rx per active duration"),
            ladder_cfgs[i].clone(),
            histories[i].clone(),
            latests[i].clone(),
            snapshot_histories[i].clone(),
            core_domain::duration_label(active_secs[i]),
            active_secs[i],
            broadcast_txs[i].clone(),
            pair_pipes[i].divergence_detector.clone(),
            // The fastest duration forwards its candles to the aggregator
            // (the ladder targets are all derived from that single stream).
            if i == 0 {
                Some(candle_fwd_tx.clone())
            } else {
                None
            },
            warmed_opts[i].clone(),
            pair_pipes[i].active_set.clone(),
        ));
    }

    // DIE L3 quarantine-refetch + runtime gap-fill coordinates (03-01-04 §2.1.2/§4.2).
    let refetch_spec = {
        let platform = state.platform.read().await;
        let rest_url = if exchange_choice == ExchangeChoice::Bitget {
            platform.bitget.rest_url()
        } else {
            platform.hyperliquid.rest_url()
        };
        analyzer::RestRefetchSpec {
            is_bitget: exchange_choice == ExchangeChoice::Bitget,
            exchange_raw: exchange_choice.raw_symbol(base, &quote),
            product_type: exchange_choice
                .bitget_product_type(&quote)
                .unwrap_or("")
                .to_string(),
            rest_url,
        }
    };

    for (
        i,
        (
            rx,
            tf_cfg,
            hist,
            snap,
            snap_hist,
            slot_label,
            tf_secs,
            bcast,
            div_det,
            candle_fwd,
            warmed,
            active_set,
        ),
    ) in pipeline_specs.into_iter().enumerate()
    {
        let a_symbol = internal_symbol.to_string();
        let a_pair_key = pair_key.to_string();
        let a_telemetry = state.telemetry_tx.clone();
        let a_cancel = cancel.clone();
        let a_fib = fib_config.clone();
        let a_latest_oi = active_pair.latest_oi.clone();
        let a_latest_funding = active_pair.latest_funding.clone();
        let a_latest_mark = active_pair.latest_mark_px.clone();
        let a_latest_index = active_pair.latest_index_px.clone();
        // Derivatives-warmup history locks. Bootstrapped from prior
        // `market_snapshots` rows by `populate_buffers` (or default-empty
        // on cold DB).
        // AUDIT-AIU-051: each TF pipeline gets its OWN clone of the
        // timestamped OI history so the 3600 s delta window is evaluated
        // against that TF's candle cadence (a shared deque would let a
        // fast TF's frequent samples dominate a slow TF's delta).
        let a_oi_history: Arc<RwLock<VecDeque<(u64, f64)>>> =
            Arc::new(RwLock::new(active_pair.oi_history.read().await.clone()));
        let a_funding_history = active_pair.funding_history.clone();
        let a_liquidity_config = eff_liq.cfg.clone();
        let a_heatmap_config = eff_liq.heatmap.clone();
        // v9: capture the wired order-book config + strategy for the
        // moved closure.
        let a_ob_config = ob_config.clone();
        let a_strategy = strategy.clone();
        // Per-TF cluster-matrix handle (Phase 2, per-TF refactor). Each TF
        // pipeline owns its own `Arc<RwLock<...>>` so every chart in the
        // dashboard sees the cluster at its own horizon. See
        // `compute_cluster_for_tf` for the per-TF history lookback.
        let a_cluster_matrix = cluster_matrices[i].clone();
        let a_latency = active_pair.latency_tracker.clone();
        let a_quality = state.platform.read().await.quality.clone();
        let a_reliability = state.reliability.clone();
        let a_refetch = refetch_spec.clone();
        // AUDIT-V9 B8: the per-scope `ConnectionQualityTracker` returned
        // here is shared between the supervisor task below (which writes
        // `record_connect`, `record_reconnect`, `record_disconnect`)
        // and this analyzer task (which writes `record_reconstructed_candle`).
        // Writes are serialised by the tracker's internal `RwLock`;
        // reads via `ConnectionQualityTracker::report()` therefore
        // observe a coherent snapshot. The two-writer pattern is by
        // design so the score reflects BOTH connect-time reliability
        // (supervisor) and data-time reconstruction events (analyzer).
        let a_cq_scope = state.connection_quality.scope(pair_key, tf_secs).await;
        let a_buffer_size = buffer_size;
        // v6.10 (Phase 2 / B3): per-TF advisory handle for write-through.
        let a_advisory: Arc<RwLock<Option<core_domain::advisory::AdvisoryMatrix>>> =
            pair_pipes[i].advisory.clone();
        // v6.10 (Phase 3 / C1 + C3): per-TF indicator_lifecycle handle
        // used as prev-state input AND as write-through target on every
        // completed candle emit.
        let a_indicator_lifecycle: Arc<RwLock<core_domain::indicator_dtos::IndicatorLifecycleMap>> =
            pair_pipes[i].indicator_lifecycle.clone();
        // v6.10 (Phase 3 / C3): per-TF pipeline_state handle for write-through.
        let a_pipeline_state: Arc<RwLock<core_domain::models::CandlePipelineState>> =
            pair_pipes[i].pipeline_state.clone();

        // Sibling handles: every OTHER fixed-ladder slot's latest-snapshot
        // lock, in ladder order, excluding this pipeline's own.
        let cross_tf_snapshots: Vec<Arc<RwLock<Option<MarketSnapshot>>>> = latests
            .iter()
            .enumerate()
            .filter_map(|(j, h)| (j != i).then_some(h.clone()))
            .collect();
        // AUDIT-H7: capture before the `move` closure.
        let stale_threshold_secs = stale_threshold_secs as u32;

        tokio::spawn(async move {
            // v9 fix: `run_single`'s future is enormous in debug builds
            // (~2 MB state machine). Box it so the outer task future stays
            // tiny — constructing it on the worker stack used to overflow
            // the default 2 MiB tokio worker stack (save_recharge_cycle
            // abort).
            Box::pin(analyzer::run_single(
                rx,
                a_telemetry,
                bcast,
                tf_cfg,
                a_fib,
                core_domain::statistics::StatisticsConfig::default(),
                div_det,
                hist,
                snap,
                snap_hist,
                a_symbol,
                a_pair_key,
                tf_secs,
                slot_label,
                a_cancel,
                candle_fwd,
                warmed,
                a_latest_oi,
                a_latest_funding,
                a_latest_mark,
                a_latest_index,
                a_oi_history,
                a_funding_history,
                a_cluster_matrix,
                Some(a_liquidity_config),
                a_heatmap_config,
                a_ob_config,
                a_strategy,
                cross_tf_snapshots,
                a_latency,
                active_set,
                a_quality,
                a_reliability,
                Some(a_refetch),
                Some(a_cq_scope),
                a_buffer_size,
                // AUDIT-H7: thread the configured CB-04/ILS-07 stale
                // threshold (was hardcoded 300 inside the analyzer).
                stale_threshold_secs,
                // v6.10 (Phase 2 / B3): per-TF advisory handle.
                a_advisory,
                // v6.10 (Phase 3 / C1 + C3): per-TF indicator_lifecycle
                // and pipeline_state handles for write-through.
                a_indicator_lifecycle,
                a_pipeline_state,
            ))
            .await;
        });
    }

    // WebSocket adapter (perpetual futures on all exchanges)
    let ws_symbol = exchange_choice.raw_symbol(base, &quote);
    let ws_product_type = exchange_choice
        .bitget_product_type(&quote)
        .unwrap_or("")
        .to_string();
    let ws_internal = internal_symbol.to_string();
    let ws_tx = active_pair.snapshot_tx.clone();
    let ws_cancel = cancel.clone();
    let ws_url = if exchange_choice == ExchangeChoice::Bitget {
        state.bitget_ws_url.clone()
    } else {
        state.ws_url.clone()
    };
    let exchange_for_spawn = exchange_choice;
    let exchange_label = exchange_for_spawn.as_str().to_string();
    let es_tracker = state.exchange_status.clone();
    {
        let es = es_tracker.clone();
        let es_label = exchange_label.clone();
        let es_url = ws_url.clone();
        // Initial active_pairs is 0 — the sync helper at the end of
        // add_instance (`crates/portfolio-supervisor/src/registry/mod.rs`)
        // walks the workspace and writes the real per-exchange count. The
        // hardcoded `1u32` that used to live here was overwritten on every
        // workspace mutation and produced a brief startup misreport on
        // multi-exchange workspaces (Bitget's bucket was always 0 because
        // it was never written).
        es.register_exchange(&es_label, 0u32, &es_url).await;
        es.set_connecting(&es_label).await;
    }
    let es_disconnect = es_tracker.clone();
    let es_disconnect_label = exchange_label.clone();
    let cq_registry = state.connection_quality.clone();
    let cq_pair_key = pair_key.to_string();
    let cq_timeframes: Vec<u64> = active_secs.to_vec();
    // AUDIT-V9 B7: capture the workspace-wide latency tracker so the
    // heartbeat task can record inter-tick drift into
    // `system_heartbeat_latency_ms`. Previously this field was always
    // 0 because no call site ever invoked `record_heartbeat`. We use
    // the drift between consecutive tick moments as a proxy for
    // scheduler / event-loop jitter.
    let hb_latency = state.latency_tracker.clone();
    // Snapshot reconnect-config once; the supervisor reads these to
    // choose grace windows for the connect/disconnect signals
    // ([resilience] in `config.toml`). Read here so a mid-flight
    // config change does not desynchronise the running cycle.
    let reconnect_cfg = state.platform.read().await.reconnect;
    let connect_grace = Duration::from_millis(reconnect_cfg.connect_grace_ms);
    let disconnect_grace_ms = reconnect_cfg.disconnect_grace_ms;
    tokio::spawn(async move {
        // Per-symbol WS supervisor (03-01-01 §4 / 08-03): exponential backoff
        // 1 s → 30 s with ±20 % jitter applied before the cap; permanent
        // disable after 5 consecutive failed cycles; the failure counter
        // resets when a connection survives longer than 300 s. Connection
        // lifecycle events feed the per-(pair, timeframe) quality scopes
        // (08-05).
        //
        // AUDIT-V9 fixes (B1/B5/B6/B10):
        //   * Disconnect marking is DEFERRED by `disconnect_grace_ms`
        //     (default 5 s). A grace task is spawned when the adapter
        //     returns; if the next iteration's `set_connected` fires
        //     first, the pending grace task is aborted and the UI
        //     never sees a "Disconnected" frame. Transient blips
        //     (server-side reconnect, brief stall, dropped packet)
        //     no longer flash red.
        //   * The heartbeat timestamp is refreshed immediately on
        //     `set_connected` so the UI doesn't show a stale (>60 s)
        //     heartbeat age right after recovery.
        //   * `record_reconnect` now receives the actual handshake RTT
        //     instead of the downtime gap (B5). The downtime gap
        //     included the backoff sleep which inflated the score's
        //     reconnect-factor penalty unnecessarily.
        //   * `connect_grace_ms` is config-driven (default 2000 ms)
        //     so operators with slow WS handshakes can tune it.
        let now_ms = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        };
        let mut backoff_secs = 1u64;
        let mut consecutive_failures = 0u32;
        let mut last_disconnect_ms: Option<u64> = None;
        // Pending deferred disconnect (B1). Holds the JoinHandle of a
        // grace task scheduled when the adapter returns. Aborted on
        // the next iteration's successful connect, on shutdown, or
        // when a new grace window supersedes it.
        let mut pending_disconnect: Option<JoinHandle<()>> = None;
        loop {
            // On shutdown, cancel any pending disconnect grace task so we
            // don't fire a spurious set_disconnected during teardown.
            if ws_cancel.is_cancelled() {
                if let Some(handle) = pending_disconnect.take() {
                    handle.abort();
                }
                break;
            }
            // On each iteration, cancel any pending grace task from the
            // previous cycle (the next set_connected means we never went
            // truly offline).
            if let Some(handle) = pending_disconnect.take() {
                handle.abort();
            }
            let connect_ms = now_ms();
            let handshake_start = Instant::now();
            // First-ever connect: emit Connected event. Subsequent
            // cycles: emit ReconnectCompleted after the adapter
            // returns, with the actual handshake duration (B5).
            for &tf in &cq_timeframes {
                let scope = cq_registry.scope(&cq_pair_key, tf).await;
                if last_disconnect_ms.is_none() {
                    scope.record_connect(connect_ms).await;
                }
            }
            let session_start = Instant::now();

            // ── Spawn heartbeat task ──────────────────────────────────────
            // Pings the exchange status tracker every 10 s while the adapter
            // loop is alive so the frontend can display a fresh heartbeat age.
            let hb_es = es_tracker.clone();
            let hb_label = exchange_label.clone();
            let hb_cancel = ws_cancel.clone();
            let hb_lat_inner = hb_latency.clone();
            let heartbeat_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                let mut prev_tick = Instant::now();
                loop {
                    tokio::select! {
                        biased;
                        _ = hb_cancel.cancelled() => break,
                        _ = interval.tick() => {
                            let now = Instant::now();
                            let ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            // AUDIT-V9 B7: record the delta since the
                            // previous tick as the system-heartbeat
                            // latency. This is the time between
                            // scheduled heartbeat ticks, which proxies
                            // for scheduler jitter / event-loop lag.
                            // Saturate at u64::MAX for an absurdly long
                            // tick (effectively "unknown").
                            let delta_ms = now.saturating_duration_since(prev_tick).as_millis() as u64;
                            prev_tick = now;
                            hb_lat_inner.record_heartbeat(delta_ms);
                            hb_es.record_heartbeat(&hb_label, ms).await;
                        }
                    }
                }
            });

            // ── Delayed connect signal ────────────────────────────────────
            // The adapter's WS handshake succeeds a few hundred ms after
            // `.await` begins. We fire `set_connected` after
            // `connect_grace_ms` (config-driven, default 2000 ms); if the
            // adapter crashes before that, the connect task is aborted
            // when the adapter returns. AUDIT-V9 B6: immediately after
            // firing set_connected, refresh the heartbeat so the panel
            // never shows a stale (>60 s) heartbeat age right after
            // recovery.
            let conn_es = es_tracker.clone();
            let conn_label = exchange_label.clone();
            let conn_cancel = ws_cancel.clone();
            let connect_task = tokio::spawn(async move {
                tokio::time::sleep(connect_grace).await;
                if !conn_cancel.is_cancelled() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    conn_es.set_connected(&conn_label).await;
                    conn_es.record_heartbeat(&conn_label, now).await;
                }
            });

            if exchange_for_spawn == ExchangeChoice::Bitget {
                network_adapters::adapters::bitget::run_for_symbol(
                    ws_symbol.clone(),
                    ws_internal.clone(),
                    ws_product_type.clone(),
                    ws_tx.clone(),
                    ws_cancel.clone(),
                    &ws_url,
                )
                .await;
            } else {
                network_adapters::adapters::hyperliquid::run_for_symbol(
                    ws_symbol.clone(),
                    ws_internal.clone(),
                    ws_tx.clone(),
                    ws_cancel.clone(),
                    &ws_url,
                )
                .await;
            }

            // AUDIT-V9 B5: measure actual handshake duration. This is the
            // wall-clock time spent in `run_for_symbol` until it returns.
            // For a healthy session this would include the entire
            // connected period; cap the recorded "reconnect duration"
            // to the connect_grace window (anything longer is the
            // connected session, not the handshake).
            let handshake_rtt_ms = handshake_start.elapsed().as_millis() as u64;
            let recorded_handshake_ms = handshake_rtt_ms.min(connect_grace.as_millis() as u64);

            heartbeat_task.abort();
            connect_task.abort();

            // AUDIT-V9 B5 (cont'd): emit ReconnectCompleted with the actual
            // handshake RTT instead of the downtime gap. The previous
            // code passed `connect_ms - disc_at` (the downtime including
            // backoff sleep) which inflated the score's reconnect-factor
            // penalty unnecessarily — a 5 s backoff already saturates
            // the 5 s reconnect ceiling even when the handshake itself
            // was instant.
            if last_disconnect_ms.take().is_some() {
                for &tf in &cq_timeframes {
                    let scope = cq_registry.scope(&cq_pair_key, tf).await;
                    scope
                        .record_reconnect(connect_ms, recorded_handshake_ms)
                        .await;
                }
            }

            // AUDIT-V9 B1: schedule `set_disconnected` and
            // `record_disconnect` after `disconnect_grace_ms` instead of
            // firing them synchronously. A transient blip that recovers
            // within the grace window is therefore invisible to the UI
            // and to the per-scope disconnect counters.
            if disconnect_grace_ms > 0 {
                let grace_es = es_tracker.clone();
                let grace_label = exchange_label.clone();
                let grace_cq = cq_registry.clone();
                let grace_pair = cq_pair_key.clone();
                let grace_tfs = cq_timeframes.clone();
                let grace_cancel = ws_cancel.clone();
                pending_disconnect = Some(tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(disconnect_grace_ms)).await;
                    if grace_cancel.is_cancelled() {
                        return;
                    }
                    grace_es.set_disconnected(&grace_label).await;
                    let ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    for &tf in &grace_tfs {
                        let scope = grace_cq.scope(&grace_pair, tf).await;
                        scope.record_disconnect(ms).await;
                    }
                }));
            } else {
                // Legacy behaviour (grace = 0): fire disconnect
                // immediately. Kept for operators who explicitly opt out.
                es_disconnect.set_disconnected(&es_disconnect_label).await;
                let disconnect_ms = now_ms();
                for &tf in &cq_timeframes {
                    let scope = cq_registry.scope(&cq_pair_key, tf).await;
                    scope.record_disconnect(disconnect_ms).await;
                }
            }
            last_disconnect_ms = Some(connect_ms);
            if ws_cancel.is_cancelled() {
                break;
            }

            if session_start.elapsed() > std::time::Duration::from_secs(300) {
                consecutive_failures = 0;
                backoff_secs = 1;
            }
            consecutive_failures += 1;
            if consecutive_failures >= 5 {
                eprintln!(
                    "🛑 WS Supervisor [{}]: permanently disabled after 5 consecutive failed cycles.",
                    ws_internal
                );
                es_disconnect.set_disabled(&es_disconnect_label).await;
                break;
            }

            es_disconnect.set_reconnecting(&es_disconnect_label).await;
            es_disconnect
                .increment_reconnect(&es_disconnect_label)
                .await;
            let delay = network_adapters::adapters::resilience::apply_jitter(
                std::time::Duration::from_secs(backoff_secs),
                0.2,
            )
            .min(std::time::Duration::from_secs(30));
            eprintln!(
                "🔁 WS Supervisor [{}]: reconnecting in {:.1}s (attempt {}/5)...",
                ws_internal,
                delay.as_secs_f64(),
                consecutive_failures
            );
            tokio::select! {
                biased;
                _ = ws_cancel.cancelled() => break,
                _ = tokio::time::sleep(delay) => {}
            }
            backoff_secs = (backoff_secs * 2).min(30);
        }
        // Clean up any straggler grace task before the supervisor exits.
        if let Some(handle) = pending_disconnect.take() {
            handle.abort();
        }
    });

    // Phase 0: HL derivatives poller (mark price + OI + funding).
    // Bitget already pushes these natively on the WS adapter above.
    if liquidity_config.enabled && exchange_choice != ExchangeChoice::Bitget {
        let hl_info_url = state
            .ws_url
            .replace("wss://", "https://")
            .replace("ws://", "http://")
            .replace("/ws", "/info");
        let poller_raw = exchange_choice.raw_symbol(base, &quote);
        let poller_internal = internal_symbol.to_string();
        let poller_tx = std::sync::Arc::new(active_pair.snapshot_tx.clone());
        let poller_cancel = cancel.clone();
        let poll_ms = liquidity_config.mark_price_poll_ms;
        network_adapters::adapters::hl_derivatives_poller::spawn_hl_derivatives_poller(
            poller_raw,
            poller_internal,
            hl_info_url,
            poller_tx,
            poller_cancel,
            poll_ms,
            api_failover.max_consecutive_failures,
        );
    }

    // Phase 2: per-timeframe Liquidation cluster-matrix refresh tasks
    // (one per TF). Each runs at the TF's own candle cadence — sub-second
    // TFs refresh at sub-second intervals (matching the cadence of every
    // other MME indicator/signal). The cluster matrix is **per-TF**: the
    // micro chart shows the fastest-magnet cluster, the macro chart shows
    // the slow-magnet cluster, with a different price-history lookback
    // per TF (200 candles of *that* TF, not just micro).
    //
    // First refresh is **immediate** at startup (no 5-min delay). Each
    // tick prints the outcome (N short + M long clusters, elapsed ms) so
    // operators can see at a glance whether the cluster refresh is alive.
    // CA-15: the L2.5 refresh loop additionally requires the per-instance
    // `cluster_estimation` toggle (union of global `[activation]` and
    // `[instances.*.activation]`) — when disabled, no task spawns and the
    // `cluster` field stays absent from the snapshot (CA-06 semantics).
    if liquidity_config.enabled && cluster_estimation {
        let pair_str = pair_key.to_string();
        type ClusterRefreshHandle<'a> = (
            u64,
            &'a Arc<RwLock<Option<core_domain::liquidity::LiquidationClusterMatrix>>>,
            &'a Arc<RwLock<core_domain::liquidity::ClusterStatusSnapshot>>,
            u64,
        );
        // One refresh handle-set per ACTIVE duration (fastest → slowest).
        let mut per_tf_handles: Vec<ClusterRefreshHandle<'_>> = (0..n)
            .map(|i| {
                (
                    active_secs[i],
                    &cluster_matrices[i],
                    &cluster_statuses[i],
                    active_secs[i],
                )
            })
            .collect();
        for (tf_key, handle, status_handle, tf_secs) in per_tf_handles {
            let slot_label = core_domain::duration_label(tf_key);
            // v6.10 (Phase 2 / B5): per-TF kill switch. Read
            // `tf_leverage_config.enabled` from the active pipeline; if false,
            // skip this TF entirely (no spawn, no cluster field on snapshot).
            let tf_leverage_enabled = active_pair
                .pipeline_for_secs(tf_key)
                .map(|p| p.tf_leverage_config.enabled)
                .unwrap_or(true);
            if !tf_leverage_enabled {
                println!(
                    "⏭  Cluster Refresh: {} {} skipped (per-TF leverage.enabled=false)",
                    pair_str, &slot_label,
                );
                write_cluster_status(
                    status_handle,
                    ClusterRefreshStatus::Skipped,
                    Some("per-TF leverage.enabled=false".to_string()),
                    None,
                )
                .await;
                continue;
            }
            // Cadence resolution:
            //   - `cluster_refresh_secs == 0` → synchronize with TF candle cadence
            //   - any value > 0                → operator override (≥ 1 s)
            // The default is 0 (== TF cadence) since v6.5; the legacy 300 s default
            // was too long for an opt-in chart overlay that users expect to react to
            // observed price action. PRI-07: the cadence adapts to the slot's
            // CONFIGURED duration — nothing is hardcoded to 60/180/300/900.
            let configured = liquidity_config.cluster_refresh_secs;
            // v11.10: decoupled per-duration refresh cadence. `0` (the
            // config default) resolves to the canonical table — sub-minute
            // durations refresh every few seconds, long horizons every few
            // minutes — instead of riding the TF's own candle cadence. A
            // nonzero `[workspace.liquidity].cluster_refresh_secs` still
            // overrides for every duration.
            let cadence_secs = if configured == 0 {
                config_models::liquidity_profile::refresh_cadence_secs(tf_secs)
            } else {
                configured.max(1)
            };
            println!(
                "🌀 Cluster Refresh: {} {} started ({}s cadence, first fire immediate)",
                pair_str, &slot_label, cadence_secs,
            );
            let pair_log = pair_str.clone();
            let handle = handle.clone();
            let status_handle = status_handle.clone();
            let active_pair_clone = active_pair.clone();
            let refresh_config = liquidity_config.clone();
            // v9: strategy-derived estimator overrides (L2.5 + per-TF
            // leverage), frozen per spawned task. v11.10: the estimator
            // geometry is scaled per duration via `liquidity_profile` —
            // every ACTIVE duration gets its own frozen row.
            let cluster_overrides = ClusterOverrides::for_duration(&strategy, tf_key);
            let cancel_for_refresh = cancel.clone();
            let exchange_for_refresh = exchange_choice;
            let slot_label = slot_label.clone();
            tokio::spawn(async move {
                // AUDIT-AIU-116: consecutive-skip counter. Every `Skipped`
                // tick keeps the LAST successful matrix in the handle, so a
                // dead OI feed would serve a stale estimate anchored to an
                // old mid indefinitely. After `MAX_CONSECUTIVE_SKIPS`
                // consecutive failures the handle is cleared (cluster → None
                // on the wire, the heatmap/panel degrade to placeholders)
                // instead of silently showing stale data as current.
                const MAX_CONSECUTIVE_SKIPS: u32 = 3;
                let mut consecutive_skips: u32 = 0;
                // ── First fire: immediate (don't wait one tick) ──
                let started = std::time::Instant::now();
                match compute_cluster_for_tf(
                    &active_pair_clone,
                    tf_key,
                    &refresh_config,
                    exchange_for_refresh,
                    &cluster_overrides,
                )
                .await
                {
                    Ok(matrix) => {
                        consecutive_skips = 0;
                        let n_short = matrix.short_clusters.len();
                        let n_long = matrix.long_clusters.len();
                        let mid = matrix.mid_price;
                        let oi = matrix.total_long_oi_usd + matrix.total_short_oi_usd;
                        println!(
                            "✅ Cluster Refresh: {} {} mid={:.2} OI=${:.0} → {} short + {} long clusters (first fire, {}ms)",
                            pair_log,
                            &slot_label,
                            mid,
                            oi,
                            n_short,
                            n_long,
                            started.elapsed().as_millis(),
                        );
                        write_cluster_status(
                            &status_handle,
                            ClusterRefreshStatus::Ok,
                            None,
                            Some(matrix.clone()),
                        )
                        .await;
                        *handle.write().await = Some(matrix);
                    }
                    Err(e) => {
                        eprintln!(
                            "⚠️  Cluster Refresh: {} {} first fire skipped: {}",
                            pair_log, &slot_label, e,
                        );
                        write_cluster_status(
                            &status_handle,
                            ClusterRefreshStatus::Skipped,
                            Some(e.to_string()),
                            None,
                        )
                        .await;
                    }
                }

                // PRI-07 (v6.10.7): candle-aligned refresh. The interval is
                // phase-locked to the TF's epoch boundaries (interval_at)
                // instead of a free-running timer, so the cluster matrix is
                // recomputed at candle-close instants — the same cadence the
                // chart's completed candles use. The first tick lands on the
                // next boundary; the immediate first-fire above still gives
                // the overlay data at startup.
                let start = core_domain::LatencyTracker::now_ms();
                let next_boundary = (start / (cadence_secs * 1000) + 1) * cadence_secs * 1000;
                let first_delay = next_boundary.saturating_sub(start);
                let mut interval = tokio::time::interval_at(
                    tokio::time::Instant::now() + std::time::Duration::from_millis(first_delay),
                    std::time::Duration::from_secs(cadence_secs),
                );
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel_for_refresh.cancelled() => {
                            println!(
                                "🛑 Cluster Refresh: {} {} cancelled, shutting down.",
                                pair_log,
                                &slot_label,
                            );
                            break;
                        }
                        _ = interval.tick() => {}
                    }
                    let started = std::time::Instant::now();
                    match compute_cluster_for_tf(
                        &active_pair_clone,
                        tf_key,
                        &refresh_config,
                        exchange_for_refresh,
                        &cluster_overrides,
                    )
                    .await
                    {
                        Ok(matrix) => {
                            consecutive_skips = 0;
                            let n_short = matrix.short_clusters.len();
                            let n_long = matrix.long_clusters.len();
                            let mid = matrix.mid_price;
                            let oi = matrix.total_long_oi_usd + matrix.total_short_oi_usd;
                            println!(
                                "✅ Cluster Refresh: {} {} mid={:.2} OI=${:.0} → {} short + {} long clusters ({}ms)",
                                pair_log,
                                &slot_label,
                                mid,
                                oi,
                                n_short,
                                n_long,
                                started.elapsed().as_millis(),
                            );
                            write_cluster_status(
                                &status_handle,
                                ClusterRefreshStatus::Ok,
                                None,
                                Some(matrix.clone()),
                            )
                            .await;
                            *handle.write().await = Some(matrix);
                        }
                        Err(e) => {
                            consecutive_skips += 1;
                            eprintln!(
                                "⚠️  Cluster Refresh: {} {} skipped this tick: {} (skip {} of {})",
                                pair_log, &slot_label, e, consecutive_skips, MAX_CONSECUTIVE_SKIPS,
                            );
                            write_cluster_status(
                                &status_handle,
                                ClusterRefreshStatus::Skipped,
                                Some(e.to_string()),
                                None,
                            )
                            .await;
                            if consecutive_skips >= MAX_CONSECUTIVE_SKIPS {
                                // AUDIT-AIU-116: the feed has been down for
                                // several refresh cycles — drop the stale
                                // matrix so the wire carries `cluster: None`
                                // and the frontend degrades to placeholders
                                // instead of showing an estimate anchored to
                                // an outdated mid.
                                let mut guard = handle.write().await;
                                if guard.is_some() {
                                    println!(
                                        "🕸️  Cluster Refresh: {} {} cleared stale matrix after {} consecutive skips",
                                        pair_log,
                                        &slot_label,
                                        consecutive_skips,
                                    );
                                }
                                *guard = None;
                            }
                        }
                    }
                }
            });
        }
    }
}

/// Discriminated failure modes for the cluster refresh task. The error
/// variant tells the operator (or the diagnostic log) **exactly why** the
/// cluster matrix could not be recomputed this tick, so a missing cluster
/// on the chart is debuggable from the logs alone.
#[derive(Debug)]
pub enum ClusterRefreshError {
    /// No snapshot has been produced for this TF yet (DIE → MME warm-up).
    NoSnapshotYet,
    /// `mid_price <= 0.0` is unphysical — usually means the snapshot's
    /// OHLC fields are NaN or empty.
    InvalidMidPrice(f64),
    /// `open_interest` is missing or non-positive. The carrier depends on
    /// the active exchange (Hyperliquid: REST poller on a 60 s cadence;
    /// Bitget: `ticker` channel carrying `holdingAmount`). The variant
    /// carries the exchange so the message can be templated correctly.
    NoOpenInterest { exchange: ExchangeChoice },
    /// TF candle history has fewer than 5 bars; cluster estimation needs
    /// swing-low/high seeds which require non-trivial history.
    InsufficientHistory(usize),
}

impl std::fmt::Display for ClusterRefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClusterRefreshError::NoSnapshotYet => {
                write!(f, "no snapshot yet (DIE → MME warm-up still in progress)")
            }
            ClusterRefreshError::InvalidMidPrice(p) => {
                write!(f, "invalid mid_price ({}); non-positive or NaN", p)
            }
            ClusterRefreshError::NoOpenInterest { exchange } => match exchange {
                ExchangeChoice::Hyperliquid => write!(
                    f,
                    "no open_interest yet (HL derivatives poller hasn't populated this symbol)"
                ),
                ExchangeChoice::Bitget => write!(
                    f,
                    "no open_interest yet (Bitget ticker channel hasn't delivered holdingAmount)"
                ),
            },
            ClusterRefreshError::InsufficientHistory(n) => write!(
                f,
                "insufficient history ({} bars; need ≥5 for swing-low/high seeds)",
                n
            ),
        }
    }
}

/// v9: strategy-derived overrides for the cluster estimator (L2.5 +
/// per-TF leverage). `Default` reproduces the v8.2 hardcoded geometry.
#[derive(Debug, Clone)]
pub struct ClusterOverrides {
    pub estimation: core_domain::liquidity::ClusterEstimationParams,
    /// v11.10: per-duration OI-delta window (replaces the fixed 1-hour
    /// constant in the analyzer's snapshot build).
    pub oi_delta_window_secs: u64,
    pub oi_split: core_domain::liquidity::ClusterOiSplitParams,
    pub confidence: core_domain::liquidity::ClusterConfidenceParams,
    pub funding_mod_shift: f64,
    /// Strategy `l1_5.per_tf_leverage` — when `enabled`, replaces the
    /// pipeline's per-TF leverage config.
    pub per_tf_leverage: Option<config_models::L1_5TfLeverageParams>,
}

impl Default for ClusterOverrides {
    fn default() -> Self {
        Self {
            estimation: core_domain::liquidity::ClusterEstimationParams::default(),
            oi_delta_window_secs: 3600,
            oi_split: core_domain::liquidity::ClusterOiSplitParams::default(),
            confidence: core_domain::liquidity::ClusterConfidenceParams::default(),
            funding_mod_shift: 0.05,
            per_tf_leverage: None,
        }
    }
}

impl ClusterOverrides {
    /// Build from the bound strategy's `l2_5` + `l1_5` sections.
    pub fn from_strategy(strategy: &config_models::StrategyConfig) -> Self {
        let l2_5 = &strategy.l2_5;
        Self {
            estimation: core_domain::liquidity::ClusterEstimationParams {
                swing_window_bars: l2_5.estimation.swing_window_bars,
                swing_lookback: l2_5.estimation.swing_lookback,
                bin_size_pct: l2_5.estimation.bin_size_pct,
                peak_halfwidth_divisor: l2_5.estimation.peak_halfwidth_divisor,
                bound_decay: l2_5.estimation.bound_decay,
                ttl_secs: l2_5.estimation.ttl_secs,
            },
            oi_split: core_domain::liquidity::ClusterOiSplitParams {
                funding_anchor: l2_5.oi_split.funding_anchor,
                funding_bias_scale: l2_5.oi_split.funding_bias_scale,
                price_anchor_pct: l2_5.oi_split.price_anchor_pct / 100.0,
                price_bias_scale: l2_5.oi_split.price_bias_scale,
                clamp: l2_5.oi_split.clamp,
            },
            confidence: core_domain::liquidity::ClusterConfidenceParams {
                oi_adequacy_anchor_usd: l2_5.confidence.oi_adequacy_anchor_usd,
                funding_penalty: l2_5.confidence.funding_penalty,
            },
            funding_mod_shift: l2_5.funding_modulation.shift,
            per_tf_leverage: Some(strategy.l1_5.per_tf_leverage.clone()),
            oi_delta_window_secs: 3600,
        }
    }
}

impl ClusterOverrides {
    /// v11.10: per-duration geometry — the strategy's `l2_5`/`l1_5` base
    /// scaled by the `liquidity_profile` row for `tf_secs` (fast durations
    /// run finer bins, tighter magnet/anchor distances and faster bound
    /// decay; see `config_models::liquidity_profile`). The percent→fraction
    /// conversion of `price_anchor_pct` is preserved exactly as in
    /// [`Self::from_strategy`].
    pub fn for_duration(strategy: &config_models::StrategyConfig, tf_secs: u64) -> Self {
        let mut this = Self::from_strategy(strategy);
        let g =
            config_models::liquidity_profile::for_duration(tf_secs, &strategy.l2_5, &strategy.l1_5);
        this.estimation.swing_lookback = g.swing_lookback;
        this.estimation.bin_size_pct = g.bin_size_pct;
        this.estimation.peak_halfwidth_divisor = g.peak_halfwidth_divisor;
        this.estimation.bound_decay = g.bound_decay;
        this.oi_split.price_anchor_pct = g.price_anchor_pct / 100.0;
        this.oi_delta_window_secs = g.oi_delta_window_secs;
        this
    }
}

/// Compute a cluster matrix for one specific timeframe slot. Each TF sees
/// the same OI/funding (shared at ActivePair level) but a different
/// price-history lookback — the last 200 candles of its own TF. This gives
/// the micro chart the fastest-magnet cluster and the macro chart the
/// slow-magnet cluster, matching the multi-TF synthesis model.
///
/// `pub` (not `pub(crate)`) so the integration tests under `tests/`
/// can drive it without instantiating the full pipeline machinery.
pub async fn compute_cluster_for_tf(
    active_pair: &Arc<analyzer::ActivePair>,
    tf_secs: u64,
    config: &config_models::LiquidityConfig,
    exchange: ExchangeChoice,
    overrides: &ClusterOverrides,
) -> Result<core_domain::liquidity::LiquidationClusterMatrix, ClusterRefreshError> {
    use core_domain::liquidity::{estimate_clusters, ClusterEstimateInput};

    // 1. Pull latest snapshot from the TF we are computing for.
    let tf_snapshot = tf_latest_snapshot(active_pair, tf_secs).await;
    let tf_snapshot = match tf_snapshot {
        Some(s) => s,
        None => return Err(ClusterRefreshError::NoSnapshotYet),
    };
    let mid = match tf_snapshot.mid_price.to_f64() {
        Some(p) if p > 0.0 => p,
        Some(p) => return Err(ClusterRefreshError::InvalidMidPrice(p)),
        None => return Err(ClusterRefreshError::InvalidMidPrice(0.0)),
    };
    let funding = tf_snapshot
        .funding_rate
        .and_then(|d| d.to_f64())
        .unwrap_or(0.0);

    // 2. Get OI from the snapshot. OI is pair-level (shared at ActivePair
    //    level), not per-TF.
    let oi = tf_snapshot
        .open_interest
        .and_then(|d| d.to_f64())
        .unwrap_or(0.0);
    if oi <= 0.0 {
        return Err(ClusterRefreshError::NoOpenInterest { exchange });
    }

    // 3. Build price history (last 200 candles of *this* TF, not micro).
    let history_arc = match tf_history(active_pair, tf_secs) {
        Some(h) => h,
        None => return Err(ClusterRefreshError::NoSnapshotYet),
    };
    let history_handle = history_arc.read().await;
    // v9: the window is the strategy's `l2_5.estimation.swing_window_bars`.
    let price_history: Vec<f64> = history_handle
        .iter()
        .rev()
        .take(overrides.estimation.swing_window_bars.max(5))
        .filter_map(|c| c.close.to_f64())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    drop(history_handle);

    if price_history.len() < 5 {
        return Err(ClusterRefreshError::InsufficientHistory(
            price_history.len(),
        ));
    }

    // 4. Compute. v6.10 (Phase 2 / B5): pull leverage_buckets / leverage_weights
    //    / min_cluster_notional_usd from the per-TF `TfLeverageConfig` on the
    //    active pipeline, replacing the hardcoded legacy distribution.
    //    v9: the strategy's `l1_5.per_tf_leverage` (when enabled) replaces
    //    the pipeline config.
    let tf_cfg = active_pair
        .pipeline_for_secs(tf_secs)
        .map(|p| p.tf_leverage_config.as_ref().clone())
        .unwrap_or_default();
    let (buckets, weights, min_notional) = match overrides.per_tf_leverage.as_ref() {
        Some(pl) if pl.enabled => (
            pl.buckets.clone(),
            pl.weights.clone(),
            pl.min_cluster_notional_usd,
        ),
        _ => (
            tf_cfg.buckets.clone(),
            tf_cfg.weights.clone(),
            tf_cfg.min_cluster_notional_usd,
        ),
    };
    let symbol = tf_snapshot.symbol.clone();
    let input = ClusterEstimateInput {
        symbol: &symbol,
        mid_price: mid,
        price_history: &price_history,
        total_oi_usd: oi,
        funding_rate: funding,
        long_oi_pct: None,
        maintenance_margin_rate: config.maintenance_margin_rate,
        funding_extreme_pct: config.funding_extreme_pct,
        funding_modulation_active: true,
        leverage_buckets: &buckets,
        leverage_weights: &weights,
        min_cluster_notional_usd: min_notional,
        estimation: overrides.estimation.clone(),
        oi_split: overrides.oi_split.clone(),
        confidence: overrides.confidence.clone(),
        funding_mod_shift: overrides.funding_mod_shift,
    };
    Ok(estimate_clusters(&input))
}

/// Helper: read the latest snapshot from one ACTIVE duration.
async fn tf_latest_snapshot(
    active_pair: &Arc<analyzer::ActivePair>,
    tf_secs: u64,
) -> Option<core_domain::models::MarketSnapshot> {
    let pipe = active_pair.pipeline_for_secs(tf_secs)?;
    pipe.latest_snapshot.read().await.clone()
}

/// Helper: get a reference to the history `VecDeque` of one ACTIVE duration.
fn tf_history(
    active_pair: &Arc<analyzer::ActivePair>,
    tf_secs: u64,
) -> Option<Arc<RwLock<std::collections::VecDeque<core_domain::normalized::NormalizedCandle>>>> {
    active_pair
        .pipeline_for_secs(tf_secs)
        .map(|p| p.history.clone())
}

/// Write the cluster-status snapshot for one TF slot after a refresh tick.
/// Always updates `last_refresh_attempt_ms`; on success also bumps
/// `last_success_ms` and clears `last_skip_reason`; on skip carries the
/// reason string forward so the operator can hover the UI pill to see why.
async fn write_cluster_status(
    handle: &Arc<RwLock<ClusterStatusSnapshot>>,
    status: ClusterRefreshStatus,
    skip_reason: Option<String>,
    matrix: Option<core_domain::liquidity::LiquidationClusterMatrix>,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut guard = handle.write().await;
    guard.last_refresh_attempt_ms = now_ms;
    guard.status = status;
    match (status, matrix) {
        (ClusterRefreshStatus::Ok, Some(m)) => {
            guard.last_success_ms = Some(now_ms);
            guard.last_skip_reason = None;
            guard.cluster_count_short = m.short_clusters.len();
            guard.cluster_count_long = m.long_clusters.len();
            guard.mid_price = m.mid_price;
            guard.ttl_remaining_ms = (m.valid_until_ms as i64) - (now_ms as i64);
        }
        _ => {
            // Skip / pending — leave cluster counts and mid_price as-is
            // (still reflects the last successful matrix if any) but
            // record the skip reason string so the operator can hover
            // the UI pill to see why.
            guard.last_skip_reason = skip_reason;
        }
    }
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", ts)
}
