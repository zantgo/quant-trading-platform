use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

/// AUDIT-C2: how long the per-symbol WS supervisor pauses after the circuit
/// breaker trips (5 consecutive rapid-failing cycles). The loop RETRIES after
/// this; it never gives up. A venue that is genuinely down therefore costs one
/// log line per cooldown rather than a permanently dead instance.
const WS_BREAKER_COOLDOWN_SECS: u64 = 300;

/// AUDIT-C2: consecutive cycles that may fail *before the connection ever
/// establishes* before the breaker trips. Cycles that survive `connect_grace`
/// reset the counter, so this only counts rapid handshake failures.
const WS_BREAKER_TRIP_COUNT: u32 = 5;

/// AUDIT-M15: floor applied to the FINAL post-jitter delay. See
/// `WsBreaker::on_cycle_end` -- with `jitter_pct >= 1.0` the jitter multiplier
/// can be exactly zero, which would otherwise collapse the backoff to a hot
/// spin regardless of how conservatively the operator configured it.
const WS_MIN_BACKOFF: Duration = Duration::from_millis(500);

/// What the supervisor should do after a connection cycle ends.
#[derive(Debug, PartialEq, Eq)]
pub enum BreakerAction {
    /// Keep retrying after this (already-jittered) delay.
    Retry(Duration),
    /// Circuit breaker tripped: log, report `reconnecting`, wait this long,
    /// then try again. The supervisor stays alive.
    Cooldown(Duration),
}

/// AUDIT-C2: the reconnect strike/backoff policy, extracted from the supervisor
/// loop so it is directly testable.
///
/// The defect this replaces counted ANY cycle as a failure unless the session
/// survived 300 s, and `break`ed the supervisor task outright on the 5th strike.
/// Nothing re-spawns that task, so any ~30 s outage (laptop suspend, VPN drop,
/// Wi-Fi flap, a venue 429 burst) permanently killed every instance: charts
/// frozen on the last bar, every indicator still reading "Live", one log line,
/// and a daemon restart as the only recovery.
///
/// Three rules now hold:
///   * A cycle that survived `connect_grace` — the window the connect task
///     itself uses to judge a handshake genuine — is a SUCCESS and clears both
///     the strike counter and the backoff.
///   * The breaker never terminates the loop. It returns [`BreakerAction::Cooldown`].
///   * AUDIT-M15: the backoff ladder comes from `[reconnect]`
///     (`ReconnectPolicy`), which the operator can configure, instead of a
///     hardcoded 1 s → 30 s.
#[derive(Debug, Clone)]
pub struct WsBreaker {
    consecutive_failures: u32,
    policy: network_adapters::adapters::resilience::ReconnectPolicy,
    next_backoff: Duration,
    connect_grace: Duration,
}

impl WsBreaker {
    pub fn new(connect_grace: Duration) -> Self {
        Self::with_policy(
            connect_grace,
            network_adapters::adapters::resilience::ReconnectPolicy::default(),
        )
    }

    /// Build from the operator's `[reconnect]` configuration.
    pub fn with_policy(
        connect_grace: Duration,
        policy: network_adapters::adapters::resilience::ReconnectPolicy,
    ) -> Self {
        let initial = policy.initial_backoff.max(WS_MIN_BACKOFF);
        Self {
            consecutive_failures: 0,
            policy,
            next_backoff: initial,
            connect_grace,
        }
    }

    /// Record one finished connection cycle and decide what happens next.
    ///
    /// `connected_for` is how long the adapter ran before returning.
    pub fn on_cycle_end(&mut self, connected_for: Duration) -> BreakerAction {
        if connected_for >= self.connect_grace {
            // AUDIT-C2: a connection that survived the connect grace window is
            // a genuine success. Reset the strike counter and the backoff here,
            // not on some arbitrary 300 s survival threshold — otherwise a
            // socket that connects and drops every 10 s accrues strikes forever
            // and eventually kills the supervisor permanently.
            self.consecutive_failures = 0;
            self.next_backoff = self.policy.initial_backoff.max(WS_MIN_BACKOFF);
        }
        self.consecutive_failures += 1;

        if self.consecutive_failures >= WS_BREAKER_TRIP_COUNT {
            self.consecutive_failures = 0;
            self.next_backoff = self.policy.initial_backoff.max(WS_MIN_BACKOFF);
            return BreakerAction::Cooldown(Duration::from_secs(WS_BREAKER_COOLDOWN_SECS));
        }

        // AUDIT-M15: the ladder starts at `initial_backoff_ms` and saturates at
        // `max_backoff_ms`, both from `[reconnect]`. The jitter percentage is
        // the configured one.
        //
        // The floor is applied to the FINAL delay, not just the input:
        // `apply_jitter` multiplies by `1.0 + gen_range(-jitter..=jitter)`, and
        // with `jitter_pct >= 1.0` that factor can be exactly 0, collapsing a
        // 1 s delay to zero and turning the supervisor into a hot spin. Flooring
        // the input alone does not prevent that, so the ceiling and floor are
        // both enforced on the result.
        let delay = network_adapters::adapters::resilience::apply_jitter(
            self.next_backoff,
            self.policy.jitter_pct,
        )
        .min(self.policy.max_backoff.max(WS_MIN_BACKOFF))
        .max(WS_MIN_BACKOFF);
        self.next_backoff = (self.next_backoff * 2).min(self.policy.max_backoff);
        BreakerAction::Retry(delay)
    }

    /// Current strike count (test/diagnostic visibility).
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

use crate::instance::{Instance, TimeframeBuffers};
use crate::registry_context::RegistryContext;
use crate::session::{Currency, ExchangeChoice};
use config_models::{
    ApiFailoverConfig, FibonacciConfig, IntervalsConfig, OperationalMode, SafetyConfig,
    StrategyConfig, TimeframeConfig,
};
use core_domain::models::{CandlePipelineState, MarketSnapshot};
use core_domain::normalized::{NormalizedCandle, NormalizedEvent};
use database_storage;
use futures_util::FutureExt;
use market_analyzer::analyzer;
use market_analyzer::indicators::DivergenceDetector;
use market_analyzer::sr_engine::SrRoleTracker;
use tokio_util::sync::CancellationToken;

pub struct PipelineContext {
    pub base: String,
    /// The venue's WIRE name for this market — `BTC` on the default crypto perp
    /// dex, `xyz:TLT` on a HIP-3 builder-deployed one.
    ///
    /// This is NOT derivable from `base`: since HIP-3 Hyperliquid runs several
    /// perp dexes and the equity / index / commodity / FX markets are named
    /// `<dex>:<coin>` on a dex the bare `meta` query never returns. The WS
    /// subscribe, the historical warm, the gap-refetch spec and the derivatives
    /// poller all send this verbatim — sending the bare base makes the venue
    /// drop the socket (verified: a bare `TLT` subscribe closes the connection
    /// with zero frames, while `xyz:TLT` streams normally).
    pub venue_coin: String,
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
    /// AUDIT-M3: start gate for the per-duration tasks.
    ///
    /// `build_pipelines` spawns `run_single`, which seeds its indicator state
    /// and buffers from the warmed state; the registry then calls
    /// `populate_buffers` to seed the SHARED history deques. Those two writers
    /// used to race: both took the same `Arc<RwLock<VecDeque>>` and both used a
    /// read-once-then-act guard (`if hist.is_empty() { push }` in `run_single`,
    /// `clear(); push` in `populate_buffers`), so an interleaving where
    /// `run_single` observed `is_empty() == true` and pushed AFTER
    /// `populate_single` finished produced up to 2x 500 duplicated warm candles.
    /// That silently corrupts Fibonacci, pivot points, S/R, chart patterns and
    /// the cluster estimator's `price_history` at the exact handover moment they
    /// are most relied upon. Byte accounting stays exact, so the budget never
    /// surfaces it — and being timing-dependent, it only appeared
    /// intermittently on the first-snapshot-after-boot path.
    ///
    /// A `watch` channel closes the window deterministically: `run_single`
    /// signals readiness, then waits for the registry to finish `populate_buffers`
    /// before touching a shared buffer. The registry ALWAYS signals, including
    /// when there is no warmed state to seed, so no path can deadlock.
    pub start_gate: Option<tokio::sync::watch::Receiver<bool>>,
    pub operational_mode: OperationalMode,
    #[allow(dead_code)]
    pub weight_overrides: Option<std::collections::HashMap<String, i32>>,
    /// API-failover tolerance knobs.
    pub api_failover: ApiFailoverConfig,
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
    warmed_states: Option<&[analyzer::WarmedPipelineState]>,
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

    // v11.12.24 (memory): per-duration WS fan-out ring. The WS handler treats
    // a `Lagged` notification as "resume at the newest frame" (it never
    // replays the backlog), so a large ring buys nothing but retention: at
    // 200 the worst case is 200 × ~120 KB ≈ 24 MB **per subscribed channel**,
    // and the dashboard subscribes one channel per (instance × duration) —
    // i.e. ~7 GB of worst-case ring at 30 channels on a 2.8 GB host. 32 still
    // covers minutes of catch-up (32 × the shadow interval), and a tab that
    // stalls past it simply loses intermediate frames, which is what the
    // frontend already handles. `broadcast_lag.rs` pins the lag semantics
    // against its own capacity-4 channel, so this constant is not load-bearing
    // for any test.
    const BROADCAST_SLOT_CAPACITY: usize = 32;
    let broadcast_txs: Vec<tokio::sync::broadcast::Sender<MarketSnapshot>> = (0..n)
        .map(|_| tokio::sync::broadcast::channel::<MarketSnapshot>(BROADCAST_SLOT_CAPACITY).0)
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

    // v11.12.24 (memory): grown on demand. This used to pre-allocate
    // `with_capacity(ctx.buffer_size)` — 500 slots of `MarketSnapshot`
    // (~1.5 KB of struct each) per duration per instance, i.e. ~7.5 MB
    // allocated up front for a pipeline that may only ever hold a handful
    // of bars on a quiet pair. `VecDeque` grows geometrically anyway and
    // `history_budget::push_retained` bounds the steady state.
    let snapshot_histories: Vec<Arc<RwLock<VecDeque<MarketSnapshot>>>> = (0..n)
        .map(|_| Arc::new(RwLock::new(VecDeque::<MarketSnapshot>::new())))
        .collect();

    // v6.10 (Phase 5 / E1): build the real ActiveSet from config instead
    // of the default all-enabled set. Global `[activation]` + per-instance
    // `[instances.*.activation]` union (CA-06: disabled ≡ absent).
    //
    // CR-10: the `[liquidity] enabled` master switch used to be folded in
    // here (CA-15) and four per-instance L1.5 sub-toggles were written on top
    // of the result. All five are gone with the chain they gated.
    let active_set = market_analyzer::active_set::ActiveSet::from_config(
        &ctx.activation,
        ctx.activation_instance.as_ref(),
        ctx.config_version,
    );

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
            active_set: active_set.clone(),
            pipeline_state: Arc::new(RwLock::new(CandlePipelineState::Initializing)),
            indicator_lifecycle: Arc::new(RwLock::new(std::collections::HashMap::new())),
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
        latency_tracker: state.latency_tracker.clone(),
    });

    spawn_tasks(
        snapshot_rx,
        &ctx.venue_coin,
        &ctx.internal_symbol,
        &ctx.pair_key,
        &ctx.ladder_cfgs,
        &active_secs,
        &ctx.fib_config,
        &cancel,
        ctx.start_gate.clone(),
        &broadcast_txs,
        &histories,
        &latests,
        &snapshot_histories,
        &active_pair,
        state,
        warmed_states,
        ctx.exchange_choice,
        ctx.quote,
        ctx.api_failover,
        ctx.strategy.clone(),
        ctx.buffer_size,
        // AUDIT-H7: configured stale threshold (CB-04/ILS-07).
        ctx.stale_threshold_secs,
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
        ctx.venue_coin.clone(),
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
    // The venue's WIRE name (`BTC`, or `xyz:TLT` on a HIP-3 perp dex). Every
    // venue call below — WS subscribe, gap refetch, derivatives poller —
    // sends this verbatim; the bare base makes the venue drop the socket.
    venue_coin: &str,
    internal_symbol: &str,
    pair_key: &str,
    ladder_cfgs: &[TimeframeConfig],
    active_secs: &[u64],
    fib_config: &FibonacciConfig,
    cancel: &CancellationToken,
    // AUDIT-M3: start gate receiver handed to each duration's `run_single`.
    start_gate: Option<tokio::sync::watch::Receiver<bool>>,
    broadcast_txs: &[tokio::sync::broadcast::Sender<MarketSnapshot>],
    histories: &[Arc<RwLock<VecDeque<NormalizedCandle>>>],
    latests: &[Arc<RwLock<Option<MarketSnapshot>>>],
    snapshot_histories: &[Arc<RwLock<VecDeque<MarketSnapshot>>>],
    active_pair: &Arc<analyzer::ActivePair>,
    state: &RegistryContext,
    warmed_states: Option<&[analyzer::WarmedPipelineState]>,
    exchange_choice: ExchangeChoice,
    quote: Currency,
    // CR-10: `api_failover` reached the HL derivatives poller's restart policy. With
    // the poller gone the knob has no consumer here, but it is still a live
    // workspace setting for the WebSocket supervisor, so it stays on the struct
    // and is deliberately not threaded into `spawn_tasks`.
    #[allow(unused_variables)] api_failover: ApiFailoverConfig,
    // v9: the effective strategy.
    strategy: StrategyConfig,
    buffer_size: usize,
    // AUDIT-H7: `[candle_buffer] stale_threshold_secs` (CB-04/ILS-07) —
    // threaded into run_single (was hardcoded 300 inside the analyzer).
    stale_threshold_secs: u64,
) {
    let n = active_secs.len();

    // One event channel per ACTIVE duration (fastest → slowest).
    // v11.12 FIX: this was hardcoded `for _ in 0..10` — a pre-v11.9
    // fixed-slot leftover. Growing the ladder past 10 durations left
    // `rx_iter.next()` empty at the pipeline-spec build below and PANICKED
    // mid-recharge ("one rx per active duration"), killing the recharge
    // after the old pipelines were already cancelled — the new instance
    // never installed and the operator's save hung forever. The channel
    // count MUST follow the ACTIVE ladder.
    let mut pipeline_txs: Vec<mpsc::Sender<NormalizedEvent>> = Vec::with_capacity(n);
    let mut pipeline_rxs: Vec<mpsc::Receiver<NormalizedEvent>> = Vec::with_capacity(n);
    for _ in 0..n {
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

    if let Some(ws) = warmed_states {
        if let Some(first) = ws.first() {
            for c in &first.history {
                let _ = candle_fwd_tx.send(c.clone()).await;
            }
        }
    }

    // Spawn one pipeline task per ACTIVE duration
    // v11.12.25: BORROW the warm state. This used to be
    // `Vec<Option<WarmedPipelineState>>` built by cloning every duration,
    // which combined with `mod.rs`'s `.ok().cloned()` to hold two extra
    // full copies of the warm series alive across the whole spawn. The only
    // clone left is the per-task hand-off below, which `run_single` needs
    // because it moves fields out of its argument.
    let warmed_opts: Vec<Option<&analyzer::WarmedPipelineState>> = match warmed_states {
        Some(ws) => (0..n).map(|i| ws.get(i)).collect(),
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
        // AUDIT-M3: start gate receiver.
        Option<tokio::sync::watch::Receiver<bool>>,
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
            warmed_opts[i].cloned(),
            pair_pipes[i].active_set.clone(),
            start_gate.clone(),
        ));
    }

    // DIE L3 quarantine-refetch + runtime gap-fill coordinates (03-01-04 §2.1.2/§4.2).
    let refetch_spec = {
        let platform = state.platform.read().await;
        // Each venue carries its own REST base AND its own product-type
        // discriminator (Bitget partitions its mix API; the other two don't).
        let (rest_url, venue) = match exchange_choice {
            ExchangeChoice::Bitget => (
                platform.bitget.rest_url(),
                analyzer::RestVenue::Bitget {
                    product_type: exchange_choice
                        .product_type(&quote)
                        .unwrap_or("USDT-FUTURES")
                        .to_string(),
                },
            ),
            ExchangeChoice::Bybit => (platform.bybit.kline_url(), analyzer::RestVenue::Bybit),
            ExchangeChoice::Hyperliquid => (
                platform.hyperliquid.rest_url(),
                analyzer::RestVenue::Hyperliquid,
            ),
        };
        analyzer::RestRefetchSpec {
            venue,
            exchange_raw: venue_coin.to_string(),
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
            start_gate,
        ),
    ) in pipeline_specs.into_iter().enumerate()
    {
        let a_symbol = internal_symbol.to_string();
        let a_pair_key = pair_key.to_string();
        // AUDIT-M10: retained for the panic log (the call below moves its own).
        let a_pair_key_log = a_pair_key.clone();
        let a_telemetry = state.telemetry_tx.clone();
        let a_cancel = cancel.clone();
        let a_fib = fib_config.clone();
        // Derivatives-warmup history locks. Bootstrapped from prior
        // `market_snapshots` rows by `populate_buffers` (or default-empty
        // on cold DB).
        // v9: capture the strategy for the moved closure.
        let a_strategy = strategy.clone();
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
        // AUDIT-M3: this pipeline's own receiver on the shared start gate.
        let a_start_gate = start_gate.clone();
        // v6.10 (Phase 3 / C1 + C3): per-TF indicator_lifecycle handle
        // used as prev-state input AND as write-through target on every
        // completed candle emit.
        let a_indicator_lifecycle: Arc<RwLock<core_domain::indicator_dtos::IndicatorLifecycleMap>> =
            pair_pipes[i].indicator_lifecycle.clone();
        // v6.10 (Phase 3 / C3): per-TF pipeline_state handle for write-through.
        let a_pipeline_state: Arc<RwLock<core_domain::models::CandlePipelineState>> =
            pair_pipes[i].pipeline_state.clone();
        // AUDIT-M10: retained handles for the panic path (the call below moves
        // its own copies).
        let panic_state = a_pipeline_state.clone();

        // Sibling handles: every OTHER ACTIVE duration's latest-snapshot
        // lock, in ladder order, excluding this pipeline's own.
        let cross_tf_snapshots: Vec<Arc<RwLock<Option<MarketSnapshot>>>> = latests
            .iter()
            .enumerate()
            .filter_map(|(j, h)| (j != i).then_some(h.clone()))
            .collect();
        // AUDIT-H7: capture before the `move` closure.
        let stale_threshold_secs = stale_threshold_secs as u32;

        tokio::spawn(async move {
            // AUDIT-M10: supervise the pipeline task.
            //
            // The JoinHandle was dropped and there was NO `catch_unwind`
            // anywhere in the workspace, so a panic inside `run_single` was
            // completely invisible: `pipeline_state` is written only on a
            // COMPLETED candle, so it stayed at whatever it last was; the
            // per-duration broadcast ring (cap 32) drained and nothing
            // refilled it; `/api/system/pipelines` kept reporting
            // `LIVE`/`INITIALIZING` forever; and the frontend simply kept the
            // last frame on screen with every indicator reading healthy.
            //
            // The operator saw a chart that quietly stopped updating, with no
            // log line and no state change. This is the same failure shape as
            // the Bybit v11.13 defect (a silent total loss of the trade stream)
            // one layer up.
            //
            // `CandlePipelineState::Failed` already documents this case ("a
            // non-self-recoverable calculator panic propagated to the
            // pipeline"), it was simply never wired.
            let outcome = std::panic::AssertUnwindSafe(Box::pin(analyzer::run_single(
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
                a_strategy,
                cross_tf_snapshots,
                a_latency,
                active_set,
                a_start_gate,
                a_quality,
                a_reliability,
                Some(a_refetch),
                Some(a_cq_scope),
                a_buffer_size,
                // AUDIT-H7: thread the configured CB-04/ILS-07 stale
                // threshold (was hardcoded 300 inside the analyzer).
                stale_threshold_secs,
                // v6.10 (Phase 3 / C1 + C3): per-TF indicator_lifecycle
                // and pipeline_state handles for write-through.
                a_indicator_lifecycle,
                a_pipeline_state,
            )))
            .catch_unwind()
            .await;

            // AUDIT-M10: make the panic VISIBLE. Without this the state stayed
            // at whatever it last was, `/api/system/pipelines` kept reporting
            // healthy, and the frontend kept the last frame on screen — a chart
            // that quietly stopped updating with no log line and no state
            // change. `Failed` is the variant whose own doc comment already
            // reserved "a non-self-recoverable calculator panic propagated to
            // the pipeline"; it was simply never wired.
            if outcome.is_err() {
                *panic_state.write().await = core_domain::models::CandlePipelineState::Failed;
                eprintln!(
                    "💥 Pipeline task panicked ({tf_secs}s, {a_pair_key_log}): the pipeline is DEAD \
                     and will not recover until the instance is recharged or the daemon restarts."
                );
            }
        });
    }

    // WebSocket adapter (perpetual futures on all exchanges). Every venue uses a
    // dedicated socket per pair; `ws_product_type` is Bitget-only (its mix
    // topics are partitioned by product type) and empty elsewhere.
    let ws_symbol = venue_coin.to_string();
    let ws_product_type = exchange_choice
        .product_type(&quote)
        .unwrap_or("")
        .to_string();
    let ws_internal = internal_symbol.to_string();
    let ws_tx = active_pair.snapshot_tx.clone();
    let ws_cancel = cancel.clone();
    let ws_url = match exchange_choice {
        ExchangeChoice::Bitget => state.bitget_ws_url.clone(),
        ExchangeChoice::Bybit => state.bybit_ws_url.clone(),
        ExchangeChoice::Hyperliquid => state.ws_url.clone(),
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
    // AUDIT-M15: honour `[reconnect] initial_backoff_ms` / `max_backoff_ms` /
    // `jitter_pct`. These were accepted, validated, documented and exposed by the
    // platform-config API — and then ignored: the supervisor hardcoded 1 s → 30 s
    // with 0.2 jitter, and `ReconnectPolicy` had no production caller at all
    // (only tests). An operator raising `initial_backoff_ms` to 10000 to avoid
    // tripping a venue's connection rate limit got 1 s, with no warning.
    let reconnect_policy =
        network_adapters::adapters::resilience::ReconnectPolicy::from_config(&reconnect_cfg);
    tokio::spawn(async move {
        // Per-symbol WS supervisor (03-01-01 §4 / 08-03): exponential backoff
        // 1 s → 30 s with ±20 % jitter applied before the cap; a circuit
        // breaker that cools down for `WS_BREAKER_COOLDOWN_SECS` after 5
        // consecutive RAPID-failing cycles; the failure counter resets as
        // soon as a connection survives past `connect_grace`. Connection
        // lifecycle events feed the per-(pair, timeframe) quality scopes
        // (08-05).
        //
        // AUDIT-C2 (observe-mode readiness): two defects made a ~30 s
        // network blip terminally kill EVERY configured instance, with no
        // recovery path short of a daemon restart:
        //   * The counter reset only when a session survived 300 s, so a
        //     connection that succeeded and ran 10 s still counted as a
        //     "failure". Any recurring short outage accumulated strikes
        //     permanently. The real success signal is `connect_grace` —
        //     the same window the connect task uses to judge a handshake
        //     genuine — so that is what resets it now.
        //   * Hitting 5 strikes called `set_disabled` and `break`, exiting
        //     the supervisor task. Nothing re-spawns it, so the instance
        //     was dead forever while still reporting `running`. It now
        //     cools down and RETRIES, and reports `reconnecting` (not
        //     `disabled`) because it genuinely intends to come back.
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
        let mut breaker = WsBreaker::with_policy(connect_grace, reconnect_policy);
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

            match exchange_for_spawn {
                ExchangeChoice::Bitget => {
                    network_adapters::adapters::bitget::run_for_symbol(
                        ws_symbol.clone(),
                        ws_internal.clone(),
                        ws_product_type.clone(),
                        ws_tx.clone(),
                        ws_cancel.clone(),
                        &ws_url,
                    )
                    .await
                }
                ExchangeChoice::Bybit => {
                    network_adapters::adapters::bybit::run_for_symbol(
                        ws_symbol.clone(),
                        ws_internal.clone(),
                        ws_tx.clone(),
                        ws_cancel.clone(),
                        &ws_url,
                    )
                    .await
                }
                ExchangeChoice::Hyperliquid => {
                    network_adapters::adapters::hyperliquid::run_for_symbol(
                        ws_symbol.clone(),
                        ws_internal.clone(),
                        ws_tx.clone(),
                        ws_cancel.clone(),
                        &ws_url,
                    )
                    .await
                }
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

            match breaker.on_cycle_end(session_start.elapsed()) {
                BreakerAction::Cooldown(cooldown) => {
                    eprintln!(
                        "🛑 WS Supervisor [{}]: circuit breaker tripped after rapid-failing cycles — cooling down {}s before retrying.",
                        ws_internal,
                        cooldown.as_secs()
                    );
                    es_disconnect.set_reconnecting(&es_disconnect_label).await;
                    es_disconnect
                        .increment_reconnect(&es_disconnect_label)
                        .await;
                    tokio::select! {
                        biased;
                        _ = ws_cancel.cancelled() => break,
                        _ = tokio::time::sleep(cooldown) => {}
                    }
                    continue;
                }
                BreakerAction::Retry(delay) => {
                    es_disconnect.set_reconnecting(&es_disconnect_label).await;
                    es_disconnect
                        .increment_reconnect(&es_disconnect_label)
                        .await;
                    eprintln!(
                        "🔁 WS Supervisor [{}]: reconnecting in {:.1}s (strike {}/{})...",
                        ws_internal,
                        delay.as_secs_f64(),
                        breaker.consecutive_failures(),
                        WS_BREAKER_TRIP_COUNT
                    );
                    tokio::select! {
                        biased;
                        _ = ws_cancel.cancelled() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
        // Clean up any straggler grace task before the supervisor exits.
        if let Some(handle) = pending_disconnect.take() {
            handle.abort();
        }
    });

    // CR-10: the HL derivatives poller is DELETED.
    //
    // Hyperliquid has no public open-interest or funding stream, so the platform
    // polled `/info {"type":"metaAndAssetCtxs"}` for mark, OI and funding — and
    // Bitget and Bybit pushed the same three natively on their WS ticker
    // channels. The poller fed the four non-candle indicators and the
    // liquidation-cluster estimator; with both gone there is nothing to poll.
    //
    // The gating comment it replaced was careful and correct, and it is worth
    // keeping in the record: the poller was `== Hyperliquid`, never an
    // inequality, because spawning it for Bitget or Bybit would poll a
    // Hyperliquid URL for a market that is not there — at best 404 every cycle,
    // at worst silently feed the wrong venue's numbers. That reasoning dies with
    // the poller.
}

// CR-10: `tf_latest_snapshot`, `tf_history` and the cluster-status writer are
// DELETED. The first two were the per-duration readers the cluster-refresh task
// used to pull history and current price; the third wrote the status the
// `/api/liquidity/cluster-status` endpoint served. All three lost their only
// caller when the refresh task went, and a helper with no caller is just a
// second copy of the accessor it wraps.

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", ts)
}
