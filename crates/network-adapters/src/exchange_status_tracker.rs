//! Per-exchange connectivity status tracker.
//!
//! Tracks adapter-level connection state for each exchange so the
//! `/api/exchange-status` endpoint can report per-venue health.
//! Updated by the `MarketDataOrchestrator` when adapters connect,
//! disconnect, or reconnect.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub enum ExchangeConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
    Disabled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeStatus {
    pub name: String,
    pub state: ExchangeConnectionState,
    pub active_pairs: u32,
    pub last_heartbeat_ms: u64,
    pub total_reconnects: u32,
    pub ws_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeStatusReport {
    pub exchanges: Vec<ExchangeStatus>,
}

#[derive(Clone)]
pub struct ExchangeStatusTracker {
    state: Arc<RwLock<HashMap<String, ExchangeStatus>>>,
}

impl Default for ExchangeStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeStatusTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Pre-seed the tracker with both supported exchanges in Disconnected
    /// state so the frontend always shows the full list even before any
    /// adapter connects.
    pub async fn seed_defaults(&self, hl_ws_url: &str, bg_ws_url: &str, by_ws_url: &str) {
        self.seed_single("Hyperliquid", hl_ws_url).await;
        self.seed_single("Bitget", bg_ws_url).await;
        self.seed_single("Bybit", by_ws_url).await;
    }

    /// Seed a single exchange into the tracker.
    pub async fn seed_single(&self, name: &str, ws_url: &str) {
        let mut map = self.state.write().await;
        map.entry(name.to_string())
            .or_insert_with(|| ExchangeStatus {
                name: name.to_string(),
                state: ExchangeConnectionState::Disconnected,
                active_pairs: 0,
                last_heartbeat_ms: 0,
                total_reconnects: 0,
                ws_url: ws_url.to_string(),
            });
    }

    pub async fn register_exchange(&self, name: &str, active_pairs: u32, ws_url: &str) {
        let mut state = self.state.write().await;
        state.entry(name.to_string()).or_insert(ExchangeStatus {
            name: name.to_string(),
            state: ExchangeConnectionState::Disconnected,
            active_pairs,
            last_heartbeat_ms: 0,
            total_reconnects: 0,
            ws_url: ws_url.to_string(),
        });
    }

    pub async fn set_connecting(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.state = ExchangeConnectionState::Connecting;
        }
    }

    pub async fn set_connected(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.state = ExchangeConnectionState::Connected;
        }
    }

    pub async fn set_disconnected(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.state = ExchangeConnectionState::Disconnected;
        }
    }

    pub async fn set_reconnecting(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.state = ExchangeConnectionState::Reconnecting;
        }
    }

    pub async fn set_disabled(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.state = ExchangeConnectionState::Disabled;
        }
    }

    pub async fn record_heartbeat(&self, name: &str, at_ms: u64) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.last_heartbeat_ms = at_ms;
        }
    }

    pub async fn increment_reconnect(&self, name: &str) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.total_reconnects += 1;
        }
    }

    pub async fn update_active_pairs(&self, name: &str, count: u32) {
        let mut state = self.state.write().await;
        if let Some(s) = state.get_mut(name) {
            s.active_pairs = count;
        }
    }

    pub async fn report(&self) -> ExchangeStatusReport {
        let state = self.state.read().await;
        let mut exchanges: Vec<ExchangeStatus> = state.values().cloned().collect();
        exchanges.sort_by(|a, b| a.name.cmp(&b.name));
        ExchangeStatusReport { exchanges }
    }
}
