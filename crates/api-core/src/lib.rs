use core_domain::normalized::SymbolMapper;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use config_models::PlatformConfig;
use network_adapters::clock_monitor::ClockMonitor;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use portfolio_supervisor::execution::ExecutionEngine;
use portfolio_supervisor::instance::Instance;
use portfolio_supervisor::session::{Currency, ExchangeChoice, SessionState};
use portfolio_supervisor::workspace_state::WorkspaceState;

pub mod helpers;
pub mod math;
pub mod telemetry;
pub mod types;
pub mod ws;

pub use math::compute_support_resistance;
pub use telemetry::compile_deterministic_telemetry;
pub use types::IndicatorSnapshot;

use portfolio_supervisor::registry_context::RegistryContext;

/// Notification emitted after `registry::recharge_instance` has swapped the
/// workspace entry for a pair. The WS handler subscribes to this channel so
/// it can re-subscribe to the new `ActivePair`'s broadcast channel when the
/// previous one is silently orphaned by the swap.
///
/// Without this notification the WS handler would hold a stale
/// `Arc<ActivePair>` whose embedded `broadcast::Sender` is kept alive by the
/// handler itself, so `Receiver::recv()` blocks forever (no `Closed` error),
/// the TCP socket never emits `onclose`, the frontend sees a frozen chart.
#[derive(Clone, Debug)]
pub struct RechargeNotice {
    pub pair_key: String,
}

/// v11.6 crash recovery: what the Welcome card needs to describe (and
/// recover) the previous non-finalized session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InterruptedSessionInfo {
    pub id: i64,
    pub mode: Option<String>,
    pub exchange: Option<String>,
    pub currency: Option<String>,
    pub started_at_ms: i64,
    pub instance_count: usize,
}

pub struct AppState {
    /// The single workspace state (workspace config + live `Arc<Instance>`
    /// map). The binary supports one workspace per deployment; the field is
    /// named `workspace` not `instances` to make the hierarchy explicit
    /// (PLATFORM > WORKSPACE > INSTANCE — see
    /// `docs/conceptual-foundations/01-07-data-model-hierarchy.md`).
    pub workspace: WorkspaceState,
    pub session: Arc<SessionState>,
    /// Platform-level config (exchange endpoints, clock monitor). Separate
    /// from the workspace.
    pub platform: Arc<RwLock<PlatformConfig>>,
    pub pool: SqlitePool,
    pub symbol_mapper: Arc<SymbolMapper>,
    pub telemetry_tx: mpsc::Sender<database_storage::TelemetryMsg>,
    /// Per-(pair_key, timeframe_secs) connection-quality scopes (08-05).
    pub connection_quality: Arc<ConnectionQualityRegistry>,
    pub clock_monitor: Option<Arc<ClockMonitor>>,
    pub reliability: Arc<ReliabilityTracker>,
    pub exchange_status: Arc<ExchangeStatusTracker>,
    pub latency_tracker: core_domain::SharedLatencyTracker,
    pub ws_url: String,
    pub bitget_ws_url: String,
    pub bybit_ws_url: String,
    /// v10.1: browser-origin allowlist for the served HTTP/WS surface —
    /// built from the resolved bind/port (per-folder sessions) plus the
    /// Vite dev origins. K1 boundary: any other origin is refused.
    pub allowed_origins: Vec<String>,
    /// L7 cross-symbol market overview, refreshed periodically.
    pub overview: Arc<RwLock<Option<core_domain::overview::OverviewMatrix>>>,
    pub execution_engine: Arc<ExecutionEngine>,
    /// v7 TAE setup executor (when automation is enabled). Serves the
    /// `/api/instances/:id/automation` surface.
    pub automation: Option<Arc<portfolio_supervisor::setup_executor::SetupExecutor>>,
    /// Notification channel fired by HTTP handlers after a successful
    /// `recharge_instance`. Subscribed by the WS handler so it can swap its
    /// broadcast subscription off the orphaned `ActivePair` onto the new one.
    pub recharge_tx: tokio::sync::broadcast::Sender<RechargeNotice>,

    // ── Snapshot Export (v6.10.4+) ─────────────────────────────────
    /// Runtime state of the periodic snapshot-export task. Shared between
    /// the task (reads config every tick), the HTTP handlers
    /// (`/api/snapshot-export/status`, `.../config`, `.../run-now`), and
    /// the CLI `setup` flow (which writes the same data into
    /// `config.toml`).
    pub snapshot_export: Arc<RwLock<core_domain::snapshot_export::SnapshotExportRuntime>>,
    /// `Notify` fired by `POST /api/snapshot-export/run-now` to wake the
    /// scheduler task for an immediate tick (the next scheduled tick is
    /// unaffected).
    pub snapshot_export_manual_tick: Arc<tokio::sync::Notify>,

    // ── v10 session identity ───────────────────────────────────────
    /// The current session id (monotonic, persisted). `None` before the
    /// session is created at boot.
    pub session_id: Arc<RwLock<Option<i64>>>,
    /// v11.6: bumped when the operator DISCARDS an interrupted session —
    /// the boot background-spawn task aborts when it observes a change,
    /// so a discarded instance can never be re-added by a stale retry.
    pub boot_spawn_epoch: Arc<std::sync::atomic::AtomicU64>,
    /// v11.6: set when the operator RECOVERS an interrupted session — the
    /// boot spawn task must then skip its final session-inactive flip
    /// (recovery keeps the session active; spawning continues).
    pub boot_session_recovered: Arc<std::sync::atomic::AtomicBool>,
    /// v11.6 crash recovery: the interrupted session detected at boot
    /// (leftover `'active'` row). `None` = previous run finalized cleanly.
    pub interrupted_session: Arc<RwLock<Option<InterruptedSessionInfo>>>,

    // ── Backtesting Engine (BTE, v8) ───────────────────────────────
    /// Single-run lock + live backfill progress registry. The BTE runs one
    /// backtest at a time; concurrent runs return 409.
    pub backtest: Arc<backtesting_engine::registry::BacktestRegistry>,
}

impl AppState {
    /// Build the `RegistryContext` view that portfolio-supervisor's registry
    /// functions take. Extracted here to keep the registry decoupled from
    /// `api-gateway`'s `AppState` type.
    pub fn registry_context(&self) -> RegistryContext {
        RegistryContext {
            workspace: self.workspace.clone(),
            session: self.session.clone(),
            platform: self.platform.clone(),
            pool: self.pool.clone(),
            symbol_mapper: self.symbol_mapper.clone(),
            telemetry_tx: self.telemetry_tx.clone(),
            latency_tracker: self.latency_tracker.clone(),
            ws_url: self.ws_url.clone(),
            bitget_ws_url: self.bitget_ws_url.clone(),
            bybit_ws_url: self.bybit_ws_url.clone(),
            exchange_status: self.exchange_status.clone(),
            reliability: self.reliability.clone(),
            connection_quality: self.connection_quality.clone(),
        }
    }

    pub async fn instance_count(&self) -> usize {
        self.workspace.len().await
    }

    pub async fn get_all_instances(&self) -> Vec<Arc<Instance>> {
        self.workspace.list().await
    }

    pub async fn get_active_pair(
        &self,
        pair_key: &str,
    ) -> Option<Arc<market_analyzer::analyzer::ActivePair>> {
        self.workspace
            .get(pair_key)
            .await
            .map(|inst| inst.active_pair.clone())
    }

    pub async fn get_instance_by_id(&self, id: &str) -> Option<Arc<Instance>> {
        for inst in self.workspace.list().await {
            if inst.id == id {
                return Some(inst);
            }
        }
        None
    }

    pub async fn init_session(
        &self,
        currency: Currency,
        exchange: ExchangeChoice,
    ) -> Result<(), String> {
        if !matches!(
            exchange,
            ExchangeChoice::Hyperliquid | ExchangeChoice::Bitget | ExchangeChoice::Bybit
        ) {
            return Err("Unsupported exchange selected.".to_string());
        }
        if !exchange.supports_currency(&currency) {
            return Err(format!(
                "{} does not support {} settlement. {}",
                exchange.as_str(),
                currency.as_str(),
                match exchange {
                    ExchangeChoice::Hyperliquid => "Hyperliquid perpetuals settle in USDC only.",
                    ExchangeChoice::Bitget => "Select USDT or USDC.",
                    ExchangeChoice::Bybit => "Bybit USDT perps are selected here; select USDT.",
                }
            ));
        }

        *self.session.base_currency.write().await = Some(currency);
        *self.session.exchange.write().await = Some(exchange);
        self.session
            .active
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // v11.12.23: persist the operator's ACTUAL environment — the row was
        // written at boot with the workspace defaults and was never updated,
        // so crash recovery restored the wrong exchange/base currency (and
        // the boot spawn forced that quote onto every persisted instance).
        if let Some(sid) = *self.session_id.read().await {
            if let Err(e) = database_storage::queries::sessions::update_session_environment(
                &self.pool,
                sid,
                exchange.as_str(),
                currency.as_str(),
            )
            .await
            {
                eprintln!("⚠️  Could not persist session environment: {e}");
            }
        }

        println!(
            "✅ Session initialized: {} on {}",
            currency.as_str(),
            exchange.as_str(),
        );
        Ok(())
    }

    pub async fn quit_session(&self) -> Result<(), String> {
        println!("🛑 Initiating graceful shutdown of all instances...");

        // (1) Capture the live pair keys BEFORE we touch anything. The
        // `WorkspaceState` keeps two separate state holders — `config`
        // (declarative) and `instances` (live runtime map) — and
        // `set_config` does NOT reconcile the live map. We have to drop
        // every entry from both sides, otherwise the next
        // `/api/instances` call (which reads the live map) returns
        // rows that the persisted TOML says shouldn't exist.
        let live_pair_keys: Vec<String> = {
            self.workspace
                .list()
                .await
                .iter()
                .map(|i| i.pair_key())
                .collect()
        };

        // (2) Cancel every running pipeline task. `cancel.cancel()` is
        // idempotent so it's safe even when an instance is already
        // Stopped.
        for pair_key in &live_pair_keys {
            if let Some(instance) = self.workspace.get(pair_key).await {
                instance.cancel.cancel();
            }
        }

        // Let cancellation propagate so the orphaned tasks can observe
        // it before we drop their `Arc<Instance>` references.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // (3) Drop every live instance from the runtime map. Without
        // this, `/api/instances` (which reads `WorkspaceState::list()`,
        // NOT `WorkspaceState::config()`) keeps returning the just-quit
        // instances and the dashboard shows them on re-entry.
        for pair_key in &live_pair_keys {
            self.workspace.remove(pair_key).await;
        }

        // (4) Clear the declarative config + persist to TOML.
        // AUDIT-M8: persist FIRST, publish second, both under one lock.
        //
        // The old order cleared memory, published it, and only then tried to
        // write -- so a failed write left memory and disk permanently diverged:
        // the dashboard showed an empty workspace while `config.toml` still
        // declared every instance. `mutate_and_persist` writes the file and
        // stores the new config in the same locked step, so the two can only
        // ever agree. Note this also drops the contradictory old message,
        // which told the operator "nothing was changed" after memory had
        // already been cleared.
        self.workspace
            .mutate_and_persist(|ws| {
                ws.instances.clear();
                Some(ws.clone())
            })
            .await
            .map_err(|e| {
                format!(
                    "Could NOT persist the cleared workspace to config.toml ({e}). \
                     Nothing was changed in memory or on disk -- the instances \
                     are still declared and WILL respawn on the next start. \
                     Fix config.toml (or delete it) before restarting."
                )
            })?;

        // AUDIT-M8c: propagate the failure.
        //
        // `save_workspace` re-reads and re-parses `config.toml`, so it fails
        // OUTRIGHT whenever the on-disk file is unparseable — i.e. exactly the
        // operator scenario where "quit" matters most. The old code logged and
        // returned 200 `{"success": true, "message": "Session terminated. All
        // instances stopped."}`.
        //
        // The operator was told their instances were gone, the dashboard showed
        // an empty workspace, and the daemon was stopped — then the next
        // `./manage.sh run` re-read the UNCHANGED `config.toml` and respawned
        // every pair, including any live-mode ones.
        {
            println!(
                "💾 Workspace persisted: 0 instances after quit (deleted {} entries from config.toml)",
                live_pair_keys.len()
            );
        }

        // (5) Mark session inactive.
        self.session
            .active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        // v11.12: back to the Welcome gate.
        self.session.set_ui_active(false);
        *self.session.base_currency.write().await = None;
        *self.session.exchange.write().await = None;

        println!("✅ Session terminated. All instances stopped.");
        Ok(())
    }

    /// v11.6 crash recovery — RECOVER: activate the session with the
    /// interrupted session's persisted defaults. The instances were
    /// already re-spawned at boot (paper/live boot PAUSED per v10.1), so
    /// recovery is purely session-state + marker bookkeeping.
    pub async fn recover_interrupted_session(&self) -> Result<(), String> {
        let info = self.interrupted_session.read().await.clone();
        let Some(info) = info else {
            return Err("no interrupted session to recover".into());
        };
        let exchange = info
            .exchange
            .as_deref()
            .map(|e| {
                if e.eq_ignore_ascii_case("bitget") {
                    portfolio_supervisor::session::ExchangeChoice::Bitget
                } else if e.eq_ignore_ascii_case("bybit") {
                    portfolio_supervisor::session::ExchangeChoice::Bybit
                } else {
                    portfolio_supervisor::session::ExchangeChoice::Hyperliquid
                }
            })
            .ok_or("interrupted session has no exchange recorded")?;
        let currency = info
            .currency
            .as_deref()
            .map(|c| {
                if c.eq_ignore_ascii_case("usdt") {
                    portfolio_supervisor::session::Currency::USDT
                } else {
                    portfolio_supervisor::session::Currency::USDC
                }
            })
            .ok_or("interrupted session has no currency recorded")?;
        // The persisted instances are the session's truth (v7.3 doctrine) —
        // NOT the boot-time `mode` column, which is a stale guess written
        // before the wizard ran. An empty workspace recovers as observe.
        let mode =
            config_models::session_mode_from_instances(&self.workspace.config().await.instances)
                .to_string();
        let capital = if mode == "paper" {
            Some(self.workspace.config().await.portfolio_capital_usd)
        } else {
            None
        };

        // Activate with the persisted defaults (same state POST /session/init sets).
        *self.session.exchange.write().await = Some(exchange);
        *self.session.base_currency.write().await = Some(currency);
        self.session
            .set_session_defaults(Some(mode.clone()), capital)
            .await;
        self.session
            .active
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // v11.12: recovery is an operator action — release the Welcome gate.
        self.session.set_ui_active(true);
        // The boot spawn task must not flip the session inactive —
        // recovery keeps it active with the instances running.
        self.boot_session_recovered
            .store(true, std::sync::atomic::Ordering::Relaxed);

        database_storage::queries::sessions::mark_session_recovered(&self.pool, info.id)
            .await
            .map_err(|e| format!("could not mark session recovered: {e}"))?;
        *self.interrupted_session.write().await = None;
        println!(
            "🔄 Session recovered: {} on {} ({} instance(s)) — mode {}",
            currency.as_str(),
            exchange.as_str(),
            info.instance_count,
            mode
        );
        Ok(())
    }

    /// v11.6 crash recovery — DISCARD: wipe instances/settings back to the
    /// default config (same persistence path as the Quit flow) and mark the
    /// interrupted session discarded. Telemetry history is kept.
    pub async fn discard_interrupted_session(&self) -> Result<(), String> {
        // v11.6: abort the boot background-spawn task BEFORE tearing down —
        // otherwise its in-flight retries would re-add the discarded instances.
        self.boot_spawn_epoch
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let info = self.interrupted_session.read().await.clone();
        // Same teardown as quit_session — minus closing the session row
        // (it stays inactive so the wizard runs).
        let live_pair_keys: Vec<String> = {
            self.workspace
                .list()
                .await
                .iter()
                .map(|i| i.pair_key())
                .collect()
        };
        for pair_key in &live_pair_keys {
            if let Some(instance) = self.workspace.get(pair_key).await {
                instance.cancel.cancel();
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        for pair_key in &live_pair_keys {
            self.workspace.remove(pair_key).await;
        }
        // AUDIT-M8: same persist-then-publish ordering as quit. This previously
        // swallowed the error while still claiming "settings back to defaults",
        // with memory already cleared -- and since the file was untouched, the
        // next boot restored the discarded workspace anyway.
        self.workspace
            .mutate_and_persist(|ws| {
                ws.instances.clear();
                Some(ws.clone())
            })
            .await
            .map_err(|e| {
                format!(
                    "Discard could NOT be persisted ({e}). Nothing was changed in \
                     memory or on disk -- config.toml is unchanged and the \
                     workspace will be restored on the next start."
                )
            })?;
        // AUDIT-M8c: propagate. This previously swallowed the error while still
        // claiming "settings back to defaults" — and the config was untouched,
        // so the next boot restored the discarded workspace anyway.

        if let Some(info) = &info {
            database_storage::queries::sessions::mark_session_discarded(&self.pool, info.id)
                .await
                .map_err(|e| format!("could not mark session discarded: {e}"))?;
        }
        // v11.12: discard keeps the operator at the Welcome gate.
        self.session.set_ui_active(false);
        *self.interrupted_session.write().await = None;
        println!(
            "🗑️  Interrupted session discarded — {} instance(s) removed; settings back to defaults",
            live_pair_keys.len()
        );
        Ok(())
    }
}

// ── Stratified state types for Axum FromRef ──────────────────────

#[derive(Clone)]
pub struct DbState(pub SqlitePool);

impl axum::extract::FromRef<Arc<AppState>> for DbState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.pool.clone())
    }
}

#[derive(Clone)]
pub struct WsState(pub mpsc::Sender<database_storage::TelemetryMsg>);

impl axum::extract::FromRef<Arc<AppState>> for WsState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.telemetry_tx.clone())
    }
}

#[derive(Clone)]
pub struct ConfigState(pub Arc<RwLock<PlatformConfig>>);

impl axum::extract::FromRef<Arc<AppState>> for ConfigState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        Self(state.platform.clone())
    }
}

/// Origins the dashboard itself can be served from — built from the
/// resolved bind/port (v10.1, per-folder sessions) plus the Vite dev
/// server origins so operator bookmarks and `bun run dev` keep working
/// on any port. Any other origin is refused.
pub fn default_allowed_origins(bind: &str, port: u16) -> Vec<String> {
    let mut out = vec![
        format!("http://{bind}:{port}"),
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        "http://127.0.0.1:5173".to_string(),
        "http://localhost:5173".to_string(),
    ];
    out.sort();
    out.dedup();
    out
}

pub fn origin_allowed(origin: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IndicatorSnapshot;

    #[test]
    fn test_support_resistance_calculations() {
        let prices = vec![
            3110.0, 3135.0, 3105.0, 3140.0, 3100.0, 3150.0, 3115.0, 3145.0, 3120.0, 3130.0,
        ];
        let current_price = 3125.0;

        let (support, resistance) = compute_support_resistance(&prices, current_price);

        for s in &support {
            let s_val: f64 = s.parse().unwrap();
            assert!(
                s_val < current_price,
                "Support {} should be below current price",
                s_val
            );
        }

        for r in &resistance {
            let r_val: f64 = r.parse().unwrap();
            assert!(
                r_val > current_price,
                "Resistance {} should be above current price",
                r_val
            );
        }

        assert!(support.len() <= 3);
        assert!(resistance.len() <= 3);
    }

    #[test]
    fn test_compile_deterministic_telemetry() {
        use market_analyzer::indicators::normalized::NormalizedIndicatorValue;
        use std::collections::HashMap;

        let mut map: HashMap<String, NormalizedIndicatorValue> = HashMap::new();
        map.insert(
            "rsi".into(),
            NormalizedIndicatorValue::scalar(25.0, 0.8, "OVERSOLD_ACCUMULATION"),
        );
        map.insert(
            "bbwp".into(),
            NormalizedIndicatorValue::scalar(5.0, 0.0, "MAX_VOLATILITY_COMPRESSION"),
        );
        // RVOL fallback: non-directional gate (normalized=0.0, band in values.rvol_band).
        map.insert(
            "rvol".into(),
            NormalizedIndicatorValue::with_values(
                0.8,
                0.0,
                "LOW_PARTICIPATION_VOLUME",
                [("rvol_band".to_string(), -0.5)].into_iter().collect(),
            ),
        );
        map.insert(
            "squeeze".into(),
            NormalizedIndicatorValue::scalar(-0.05, -0.2, "BEARISH_MOMENTUM_EXHAUSTING"),
        );
        map.insert("macd".into(), {
            let mut v = HashMap::new();
            v.insert("line".to_string(), -0.5);
            v.insert("signal".to_string(), -0.3);
            v.insert("histogram".to_string(), -0.2);
            NormalizedIndicatorValue::with_values(-0.2, -0.5, "BEARISH_MOMENTUM_EXPANDING", v)
        });
        map.insert("adx".into(), {
            let mut v = HashMap::new();
            v.insert("adx".to_string(), 15.0);
            v.insert("plus_di".to_string(), 12.0);
            v.insert("minus_di".to_string(), 18.0);
            NormalizedIndicatorValue::with_values(15.0, 0.0, "TRENDLESS_CONGESTION", v)
        });
        map.insert("ema_stack".into(), {
            let mut v = HashMap::new();
            v.insert("long".to_string(), 3200.0);
            NormalizedIndicatorValue::with_values(3125.0, -1.0, "ESTABLISHED_BEARISH_STACK", v)
        });
        map.insert("vwap".into(), {
            let mut v = HashMap::new();
            v.insert("vwap".to_string(), 3130.0);
            NormalizedIndicatorValue::with_values(3130.0, 0.8, "EXTREME_DISCOUNT_REVERSION_ZONE", v)
        });
        // Push a Divergence signal onto the RSI entry (divergence lives on parent).
        if let Some(rsi_entry) = map.get_mut("rsi") {
            rsi_entry.signals.push(
                market_analyzer::indicators::normalized::IndicatorSignal::new(
                    market_analyzer::indicators::normalized::SignalKind::Divergence,
                    market_analyzer::indicators::normalized::SignalDirection::Bullish,
                    market_analyzer::indicators::normalized::SignalStatus::Potential,
                    "POTENTIAL_BULLISH_DIVERGENCE",
                ),
            );
        }
        map.insert("atr".into(), {
            let mut v = HashMap::new();
            v.insert("atr_14".to_string(), 1.5);
            NormalizedIndicatorValue::with_values(1.5, 0.0, "ATR_RAW", v)
        });
        let indicators = IndicatorSnapshot::new(map, Some(3125.0));

        let support_levels: Vec<String> = vec!["3100.00".to_string(), "3050.00".to_string()];
        let resistance_levels: Vec<String> = vec!["3150.00".to_string(), "3200.00".to_string()];

        let telemetry = telemetry::compile_deterministic_telemetry(
            &indicators,
            &support_levels,
            &resistance_levels,
        );

        assert_eq!(telemetry.market_regime, "COMPRESSION");
        assert!(telemetry.total_confluence_score < 0);
        assert_eq!(telemetry.rvol, 0.8);
        assert_eq!(telemetry.adx_value, 15.0);
        assert_eq!(telemetry.adx_regime, "congestion");
        assert!((telemetry.bbwp_percentile - 5.0).abs() < 0.001);
        assert!(!telemetry.squeeze_on);
        assert_eq!(telemetry.vwap_bias, "discount");
        assert_eq!(telemetry.rsi_divergence_state, "potential_bullish");
        assert_eq!(telemetry.macd_divergence_state, "none");
        assert_eq!(telemetry.macd_crossover_state, "none");
        assert_eq!(telemetry.squeeze_release_state, "none");
        assert_eq!(telemetry.support_levels.len(), 2);
        assert_eq!(telemetry.resistance_levels.len(), 2);
    }
}
