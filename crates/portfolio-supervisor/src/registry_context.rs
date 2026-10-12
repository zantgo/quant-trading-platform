//! Shared context type for portfolio-supervisor's instance registry.
//!
//! Lives in `portfolio-supervisor` (not in `api-gateway`) so the registry
//! functions can take it without creating a circular `api-gateway` dep.
//! `api-gateway`'s HTTP handlers build this from their `AppState`.

use std::sync::Arc;
use tokio::sync::RwLock;

use config_models::PlatformConfig;
use core_domain::latency::SharedLatencyTracker;
use core_domain::normalized::SymbolMapper;
use database_storage::TelemetryMsg;
use network_adapters::connection_quality_tracker::ConnectionQualityRegistry;
use network_adapters::exchange_status_tracker::ExchangeStatusTracker;
use network_adapters::pipeline_reliability::ReliabilityTracker;
use tokio::sync::mpsc;

use crate::session::SessionState;
use crate::workspace_state::WorkspaceState;

#[derive(Clone)]
pub struct RegistryContext {
    /// Live workspace state (workspace config + active `Arc<Instance>` map).
    pub workspace: WorkspaceState,
    pub session: Arc<SessionState>,
    /// Platform-level config (server, DB, exchanges, clock). Held by the
    /// daemon and passed through unchanged. The registry does not mutate it.
    pub platform: Arc<RwLock<PlatformConfig>>,
    pub pool: sqlx::SqlitePool,
    pub symbol_mapper: Arc<SymbolMapper>,
    pub telemetry_tx: mpsc::Sender<TelemetryMsg>,
    pub latency_tracker: SharedLatencyTracker,
    pub ws_url: String,
    pub bitget_ws_url: String,
    pub bybit_ws_url: String,
    pub exchange_status: Arc<ExchangeStatusTracker>,
    pub reliability: Arc<ReliabilityTracker>,
    /// Per-(pair_key, timeframe_secs) connection-quality scopes (08-05).
    pub connection_quality: Arc<ConnectionQualityRegistry>,
}
