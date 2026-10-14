//! # Workspace State
//!
//! The runtime, in-memory mirror of one workspace. Held by the binary in an
//! `Arc<RwLock<WorkspaceState>>` so that:
//!
//! - portfolio-supervisor (PME + TAE) can read/mutate instances and the
//!   workspace config without going through any HTTP layer,
//! - api-gateway can serialize a snapshot for the UI,
//! - execution-daemon can hand a fresh state to background tasks.
//!
//! The single source of truth for *what instances exist* is the workspace
//! config TOML file. `WorkspaceState` is the live projection of that file:
//! `save_workspace` rewrites the TOML and the in-memory state is reloaded by
//! the next read through `AppState`. Concurrent mutations are guarded by the
//! surrounding `Arc<RwLock<...>>`.
//!
//! What lives here:
//! - The deserialized `WorkspaceConfig` (workspace + instances).
//! - The live `Arc<Instance>` map for active pipelines (keyed by pair key).
//! - Cached `registry::InstanceSummary` lists for fast dashboard reads.
//!
//! What does NOT live here:
//! - Telemetry / DB write channels — those are on `AppState` directly.
//! - `ConnectionQualityTracker` — also on `AppState` (platform-level).

use std::collections::HashMap;
use std::sync::Arc;

use config_models::{InstanceEntry, WorkspaceConfig};
use tokio::sync::RwLock;

use crate::instance::Instance;

/// Runtime workspace state. One per binary. Holds both the deserialized
/// workspace config (single source of truth for what instances exist) and
/// the live `Arc<Instance>` map (active pipelines).
#[derive(Clone)]
pub struct WorkspaceState {
    inner: Arc<WorkspaceStateInner>,
}

struct WorkspaceStateInner {
    /// The workspace config (workspace-level settings + instances[]).
    config: RwLock<WorkspaceConfig>,
    /// Live `Arc<Instance>` map for the running pipelines. The map keys
    /// (pair_key, e.g. `"BTC-USDT"`) are the same keys the workspace config
    /// uses under `instances[].symbol`.
    instances: RwLock<HashMap<String, Arc<Instance>>>,
}

impl WorkspaceState {
    /// Create an empty workspace state with the given config.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self {
            inner: Arc::new(WorkspaceStateInner {
                config: RwLock::new(config),
                instances: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Build an empty workspace state seeded with `WorkspaceConfig::default()`.
    /// Used by the execution-daemon when `config.toml` is missing the
    /// `[workspace]` table — the daemon panics before reaching this code
    /// today, but `default()` is here so future tooling (CLI bootstrapping,
    /// tests) can construct a state without a config file.
    pub fn empty() -> Self {
        Self::new(WorkspaceConfig::default())
    }

    /// Read the current workspace config.
    pub async fn config(&self) -> WorkspaceConfig {
        self.inner.config.read().await.clone()
    }

    /// Replace the entire workspace config. Does NOT touch live instances —
    /// callers must reconcile the diff (e.g. via `reconcile_instances`).
    pub async fn set_config(&self, new_config: WorkspaceConfig) {
        *self.inner.config.write().await = new_config;
    }

    /// AUDIT-M8: mutate the workspace config and persist it ATOMICALLY.
    ///
    /// The nine workspace mutators (`POST /api/config`,
    /// `POST /api/instances/:id/config`, `add_instance`, `delete_instance`,
    /// `recharge_instance`, the strategy upsert, `POST /api/account/capital`,
    /// `quit_session`, and the background boot-spawn retry loop which runs for
    /// minutes) each did `config()` -> mutate -> `save_workspace` ->
    /// `set_config`. Each step is individually locked but the SEQUENCE is not,
    /// so two mutators could interleave: A's ladder change was silently
    /// overwritten by B's instance write (A returning 200), or a `500` came
    /// from a rename race.
    ///
    /// Holding the write lock across read -> mutate -> persist -> publish
    /// closes the window. `mutate` receives the CURRENT config and returns the
    /// new one; returning `None` aborts without writing.
    ///
    /// `save_workspace` itself is also internally serialised and uses a unique
    /// temp filename, so even a caller that cannot take this lock cannot
    /// corrupt the file or lose the rename.
    pub async fn mutate_and_persist<F>(&self, mutate: F) -> Result<WorkspaceConfig, String>
    where
        F: for<'a> FnOnce(&'a mut WorkspaceConfig) -> Option<WorkspaceConfig>,
    {
        let mut guard = self.inner.config.write().await;
        // AUDIT-M8: mutate a DRAFT, not the stored config. Handing the closure
        // `&mut guard` directly would let a caller that mutates and then
        // returns `None` (or whose write fails) leave the in-memory config
        // permanently diverged from disk.
        let mut draft = guard.clone();
        let Some(updated) = mutate(&mut draft) else {
            // Caller declined; leave the stored config untouched.
            return Ok(guard.clone());
        };
        config_models::save_workspace(&updated)
            .map_err(|e| format!("could not persist config.toml: {e}"))?;
        *guard = updated.clone();
        Ok(updated)
    }

    /// Look up a live `Arc<Instance>` by pair key.
    pub async fn get(&self, pair_key: &str) -> Option<Arc<Instance>> {
        self.inner.instances.read().await.get(pair_key).cloned()
    }

    /// List all live instances.
    pub async fn list(&self) -> Vec<Arc<Instance>> {
        self.inner
            .instances
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// List all live instance keys (pair keys).
    pub async fn keys(&self) -> Vec<String> {
        self.inner.instances.read().await.keys().cloned().collect()
    }

    /// Insert an instance into the live map. Overwrites any existing entry
    /// under the same pair key.
    pub async fn insert(&self, pair_key: String, instance: Arc<Instance>) {
        self.inner
            .instances
            .write()
            .await
            .insert(pair_key, instance);
    }

    /// Remove an instance from the live map.
    pub async fn remove(&self, pair_key: &str) -> Option<Arc<Instance>> {
        self.inner.instances.write().await.remove(pair_key)
    }

    /// Number of live instances.
    pub async fn len(&self) -> usize {
        self.inner.instances.read().await.len()
    }

    /// True if no live instances exist.
    pub async fn is_empty(&self) -> bool {
        self.inner.instances.read().await.is_empty()
    }

    /// Read the workspace config's instance entries (static config, not
    /// live). Use this to enumerate what the workspace *says* should be
    /// running, then cross-check with `list()` to find drift.
    pub async fn declared_instances(&self) -> Vec<InstanceEntry> {
        self.inner.config.read().await.instances.clone()
    }

    /// Live instances declared but not yet running (drift from workspace
    /// config to runtime state). Returns pair keys.
    pub async fn declared_but_not_running(&self) -> Vec<String> {
        let cfg = self.inner.config.read().await;
        let live = self.inner.instances.read().await;
        cfg.instances
            .iter()
            .map(|i| i.symbol.clone())
            .filter(|k| !live.contains_key(k))
            .collect()
    }

    /// Live instances running but not declared in workspace config (drift
    /// the other way — typically a temporary add that was never persisted).
    pub async fn running_but_not_declared(&self) -> Vec<String> {
        let cfg = self.inner.config.read().await;
        let live = self.inner.instances.read().await;
        live.keys()
            .filter(|k| !cfg.instances.iter().any(|i| &i.symbol == *k))
            .cloned()
            .collect()
    }
}
