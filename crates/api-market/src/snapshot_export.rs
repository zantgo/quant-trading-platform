//! HTTP handlers for the snapshot-export scheduler.
//!
//! Three endpoints:
//! - `GET  /api/snapshot-export/status`  — live runtime state.
//! - `PUT  /api/snapshot-export/config`  — partial-patch the runtime.
//! - `POST /api/snapshot-export/run-now` — force an immediate tick.
//!
//! All three share the `Arc<RwLock<SnapshotExportRuntime>>` in
//! `AppState`; the underlying task reads the same runtime on every
//! tick so changes are hot-reloadable.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use api_core::AppState;
use core_domain::snapshot_export::{SnapshotExportRuntime, ALL_TABS};

// ─── GET /api/snapshot-export/status ─────────────────────────────────

pub async fn serve_snapshot_export_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let rt = state.snapshot_export.read().await;
    Json(SnapshotExportResponse::from(&*rt)).into_response()
}

// ─── PUT /api/snapshot-export/config ─────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotExportConfigPatch {
    /// Optional — omit to leave unchanged.
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub interval_secs: Option<u64>,
    #[serde(default)]
    pub max_snapshots_retained: Option<u32>,
    /// `None` is represented by an empty array (or absent). Both
    /// reset the runtime to all 9 tabs.
    #[serde(default)]
    pub tabs: Option<Vec<String>>,
}

pub async fn serve_update_snapshot_export_config(
    State(state): State<Arc<AppState>>,
    Json(patch): Json<SnapshotExportConfigPatch>,
) -> impl IntoResponse {
    let mut rt = state.snapshot_export.write().await;
    if let Some(v) = patch.enabled {
        rt.enabled = v;
    }
    if let Some(v) = patch.output_path {
        if v.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "output_path cannot be empty"
                })),
            )
                .into_response();
        }
        rt.output_path = v;
    }
    if let Some(v) = patch.interval_secs {
        rt.interval_secs = v.clamp(5, 3600);
    }
    if let Some(v) = patch.max_snapshots_retained {
        rt.max_snapshots_retained = v.clamp(10, 100_000);
    }
    if let Some(v) = patch.tabs {
        let mut filtered: Vec<String> = v
            .into_iter()
            .filter(|t| ALL_TABS.contains(&t.as_str()))
            .collect();
        filtered.sort();
        filtered.dedup();
        rt.tabs = if filtered.is_empty() {
            ALL_TABS.iter().map(|s| s.to_string()).collect()
        } else {
            filtered
        };
    }
    Json(SnapshotExportResponse::from(&*rt)).into_response()
}

// ─── POST /api/snapshot-export/run-now ────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RunNowResponse {
    pub triggered: bool,
    pub path: String,
    /// Best-effort — the next scheduled tick may also write to the
    /// same directory but with a different timestamp.
    pub note: &'static str,
}

pub async fn serve_run_snapshot_export_now(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Read the path up-front so the response is informative even if
    // the task is disabled.
    let path = state.snapshot_export.read().await.output_path.clone();
    state.snapshot_export_manual_tick.notify_one();
    Json(RunNowResponse {
        triggered: true,
        path,
        note: "Tick scheduled; snapshot will appear under <output_path>/<YYYY-MM-DD>/<HHhMMmSS>/.",
    })
    .into_response()
}

// ─── Response shape ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SnapshotExportResponse {
    pub enabled: bool,
    pub output_path: String,
    pub interval_secs: u64,
    pub max_snapshots_retained: u32,
    pub tabs: Vec<String>,
    pub last_snapshot_at: Option<DateTime<Utc>>,
    pub total_snapshots_written: u64,
    pub last_error: Option<String>,
    pub last_instance_count: u32,
}

impl From<&SnapshotExportRuntime> for SnapshotExportResponse {
    fn from(rt: &SnapshotExportRuntime) -> Self {
        Self {
            enabled: rt.enabled,
            output_path: rt.output_path.clone(),
            interval_secs: rt.interval_secs,
            max_snapshots_retained: rt.max_snapshots_retained,
            tabs: rt.tabs.clone(),
            last_snapshot_at: rt.last_snapshot_at,
            total_snapshots_written: rt.total_snapshots_written,
            last_error: rt.last_error.clone(),
            last_instance_count: rt.last_instance_count,
        }
    }
}

// ─── Internal helper used by the CLI --status subcommand ─────────────

/// Fetch the live runtime state — used by the `setup --status`
/// CLI subcommand when a daemon is reachable.
pub async fn read_runtime_blocking(
    state: &Arc<RwLock<SnapshotExportRuntime>>,
) -> SnapshotExportResponse {
    SnapshotExportResponse::from(&*state.read().await)
}
