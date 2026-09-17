//! # Snapshot Export — shared types
//!
//! Cross-crate type definitions for the periodic snapshot-export
//! scheduler. Lives in `core-domain` (the leaf crate) so both
//! `execution-daemon` (which owns the task implementation) and
//! `api-gateway` (which owns the HTTP handlers) can depend on the
//! same `SnapshotExportRuntime` without a circular dependency.
//!
//! The on-disk writer logic itself lives in
//! `crates/execution-daemon/src/snapshot_export.rs` and is *not*
//! re-exported here. The `runtime_from_config(cfg)` helper lives in
//! `execution-daemon` because it needs the `config_models` crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// All 9 tabs the scheduler can emit. Strings are stable wire-format
/// identifiers — the same identifiers used by the GUI's per-tab
/// export-builder `SourceTab` enum (minus the legacy
/// `positions`/`orders`/`history`/`plan` which are per-account, not
/// per-instance).
pub const ALL_TABS: &[&str] = &[
    "metrics",
    "mtf",
    "alignment",
    "opportunity",
    "risk",
    "analysis",
    "advisory",
    "decision",
    "recommendation",
];

/// Snapshot of the live runtime state — returned by
/// `GET /api/snapshot-export/status` and the CLI `--status` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotExportRuntime {
    pub enabled: bool,
    pub output_path: String,
    pub interval_secs: u64,
    pub max_snapshots_retained: u32,
    /// Effective tab list after the `Option<Vec<String>>` is
    /// resolved against `ALL_TABS`. Sorted + de-duplicated.
    pub tabs: Vec<String>,
    pub last_snapshot_at: Option<DateTime<Utc>>,
    pub total_snapshots_written: u64,
    pub last_error: Option<String>,
    /// Snapshot of `WorkspaceState.instances.len()` at the time of
    /// the last successful tick.
    pub last_instance_count: u32,
}

impl Default for SnapshotExportRuntime {
    fn default() -> Self {
        Self {
            enabled: false,
            output_path: default_output_path(),
            interval_secs: 60,
            max_snapshots_retained: 1000,
            tabs: ALL_TABS.iter().map(|s| s.to_string()).collect(),
            last_snapshot_at: None,
            total_snapshots_written: 0,
            last_error: None,
            last_instance_count: 0,
        }
    }
}

fn default_output_path() -> String {
    "./snapshots".to_string()
}

/// Top-level envelope wrapping every snapshot JSON file. The `payload`
/// field is the per-tab matrix as serialised by serde; `snapshot_metadata`
/// carries the wall-clock timestamp, tab id, source instance and source
/// timeframe so downstream data-science pipelines don't have to mine
/// directory names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEnvelope<T> {
    pub snapshot_metadata: SnapshotMetadata,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// UTC timestamp at which the tick fired.
    pub datetime_utc: String,
    /// Epoch millis.
    pub timestamp_ms: i64,
    /// Tab id (`alignment`, `risk`, ...).
    pub tab: String,
    /// Source instance pair-key (`BTC-USDT`, `ETH-USDT`, ...).
    pub pair_key: String,
    /// v11.9: derived duration label ("1s".."1d") of the source timeframe.
    pub timeframe_label: String,
    /// Timeframe in seconds.
    pub timeframe_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runtime_is_disabled_with_60s_interval() {
        let rt = SnapshotExportRuntime::default();
        assert!(!rt.enabled);
        assert_eq!(rt.interval_secs, 60);
        assert_eq!(rt.max_snapshots_retained, 1000);
        assert_eq!(rt.tabs.len(), ALL_TABS.len());
        assert!(rt.last_snapshot_at.is_none());
        assert_eq!(rt.total_snapshots_written, 0);
        assert!(rt.last_error.is_none());
        assert_eq!(rt.last_instance_count, 0);
    }
}
