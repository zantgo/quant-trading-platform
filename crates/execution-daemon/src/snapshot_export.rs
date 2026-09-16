//! # Snapshot Export — periodic task implementation
//!
//! Lives in `execution-daemon` (not `api-gateway`) because it touches
//! the on-disk filesystem + the workspace's `Arc<Instance>` map. The
//! shared types (`SnapshotExportRuntime`, `ALL_TABS`,
//! `SnapshotEnvelope`, `SnapshotMetadata`, `runtime_from_config`)
//! live in `core-domain::snapshot_export` so both this module and
//! `api-gateway::handlers::snapshot_export` can use them without a
//! circular dependency.
//!
//! ## File layout
//!
//! ```text
//! <output_path>/
//!   2026-08-13/
//!     14h30m05s/                      ← one tick (UTC timestamp)
//!       BTC-USDT.micro.alignment.json
//!       BTC-USDT.micro.analysis.json
//!       BTC-USDT.micro.advisory.json
//!       ...
//!       BTC-USDT.slow.alignment.json
//!       ...
//!       ETH-USDT.micro.alignment.json
//!       ...
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use config_models::SnapshotExportConfig;
use serde_json;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;

use core_domain::models::MarketSnapshot;
pub use core_domain::snapshot_export::{
    SnapshotEnvelope, SnapshotExportRuntime, SnapshotMetadata, ALL_TABS,
};
use portfolio_supervisor::workspace_state::WorkspaceState;

/// Build a runtime from a static `SnapshotExportConfig`. Used at
/// boot to hydrate the runtime from `config.toml`. Also used by
/// the `PUT /api/snapshot-export/config` handler to validate a
/// client-supplied patch before applying it.
pub fn runtime_from_config(cfg: &SnapshotExportConfig) -> SnapshotExportRuntime {
    let tabs = match &cfg.tabs {
        Some(requested) => {
            let mut v: Vec<String> = requested
                .iter()
                .filter(|t| ALL_TABS.contains(&t.as_str()))
                .cloned()
                .collect();
            v.sort();
            v.dedup();
            if v.is_empty() {
                ALL_TABS.iter().map(|s| s.to_string()).collect()
            } else {
                v
            }
        }
        None => ALL_TABS.iter().map(|s| s.to_string()).collect(),
    };

    SnapshotExportRuntime {
        enabled: cfg.enabled,
        output_path: cfg.output_path.clone(),
        interval_secs: cfg.interval_secs.clamp(5, 3600),
        max_snapshots_retained: cfg.max_snapshots_retained.clamp(10, 100_000),
        tabs,
        last_snapshot_at: None,
        total_snapshots_written: 0,
        last_error: None,
        last_instance_count: 0,
    }
}

// ─── Background task ───────────────────────────────────────────────────

/// Periodic task that reads `runtime`, iterates every active instance,
/// writes per-tab JSON files to `runtime.output_path`, and prunes old
/// directories. Cancellable via `cancel`.
///
/// `manual_tick` is a `tokio::sync::Notify` that the `run-now` HTTP
/// handler fires to force an immediate tick.
pub async fn run_snapshot_exporter(
    runtime: Arc<RwLock<SnapshotExportRuntime>>,
    workspace: WorkspaceState,
    cancel: CancellationToken,
    manual_tick: Arc<Notify>,
) {
    println!(
        "📸 Snapshot Export: Started (interval: configured per runtime; default OFF until enabled)"
    );

    // We read the interval at the top of every outer loop so the
    // operator can change it via `PUT /api/snapshot-export/config`
    // without restarting the daemon.
    loop {
        let interval_secs = {
            let r = runtime.read().await;
            r.interval_secs.max(5)
        };

        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // First tick fires immediately — skip it so we don't
        // double-write on startup (the initial 500 ms grace period
        // also gives the runtime time to be hydrated from TOML).
        tokio::time::sleep(Duration::from_millis(500)).await;

        let tick_result: Result<(), String>;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Snapshot Export: Cancelled, shutting down.");
                break;
            }
            _ = manual_tick.notified() => {
                tick_result = tick_once(&runtime, &workspace).await;
            }
            _ = ticker.tick() => {
                tick_result = tick_once(&runtime, &workspace).await;
            }
        }

        if let Err(e) = tick_result {
            eprintln!("📸 Snapshot Export tick failed: {}", e);
        }
    }
}

/// Run a single snapshot tick. Public so the `run-now` HTTP handler
/// (and tests) can invoke it directly.
pub async fn tick_once(
    runtime: &Arc<RwLock<SnapshotExportRuntime>>,
    workspace: &WorkspaceState,
) -> Result<(), String> {
    let (enabled, output_path, tabs, retention) = {
        let r = runtime.read().await;
        (
            r.enabled,
            r.output_path.clone(),
            r.tabs.clone(),
            r.max_snapshots_retained,
        )
    };

    if !enabled {
        return Ok(());
    }

    let instances = workspace.list().await;
    let instance_count = instances.len() as u32;

    let now = Utc::now();
    let date_part = now.format("%Y-%m-%d").to_string();
    let time_part = now.format("%Hh%Mm%Ss").to_string();
    let base_dir = PathBuf::from(&output_path)
        .join(&date_part)
        .join(&time_part);

    if let Err(e) = tokio::fs::create_dir_all(&base_dir).await {
        let msg = format!("create_dir_all({}): {}", base_dir.display(), e);
        // Record on the runtime so the GUI modal / status endpoint
        // surfaces the failure (not just the daemon log).
        runtime.write().await.last_error = Some(msg.clone());
        return Err(msg);
    }

    let mut written: u64 = 0;
    let mut last_err: Option<String> = None;

    for inst in &instances {
        let pair_key = inst.pair_key();
        // Pull the latest 10-slot snapshots for this instance — same
        // pattern used by the L7 Overview aggregator at
        // `crates/execution-daemon/src/main.rs`.
        let snaps = inst.active_pair.latest_snapshots_all_tf().await;
        // Slot names come from the snapshot itself (matches the WS wire
        // and `/api/history` keys) and `timeframe_secs` comes from the
        // snapshot's actual configured duration — never hardcoded
        // defaults.
        let mut snap_slots: Vec<(String, u64, &Option<MarketSnapshot>)> = Vec::with_capacity(10);
        for slot_ref in snaps.iter() {
            if let Some(s) = slot_ref.as_ref() {
                let name = s
                    .timeframe_slot
                    .as_ref()
                    .map(|ts| ts.as_str())
                    .unwrap_or_default();
                snap_slots.push((name, s.timeframe_secs, slot_ref));
            }
        }
        for (slot_name, slot_secs, snap_opt) in snap_slots {
            let Some(snap) = snap_opt else { continue };
            for tab in &tabs {
                let payload = build_tab_payload(tab, snap);
                let envelope = SnapshotEnvelope {
                    snapshot_metadata: SnapshotMetadata {
                        datetime_utc: now.to_rfc3339(),
                        timestamp_ms: now.timestamp_millis(),
                        tab: tab.clone(),
                        pair_key: pair_key.clone(),
                        timeframe_slot: slot_name.to_string(),
                        timeframe_secs: slot_secs,
                    },
                    payload,
                };
                let file_path = base_dir.join(format!(
                    "{}.{}.{}.json",
                    sanitize(&pair_key),
                    slot_name,
                    tab
                ));
                let body = match serde_json::to_string_pretty(&envelope) {
                    Ok(s) => s,
                    Err(e) => {
                        last_err = Some(format!("serialize {}: {}", file_path.display(), e));
                        continue;
                    }
                };
                if let Err(e) = tokio::fs::write(&file_path, body).await {
                    last_err = Some(format!("write {}: {}", file_path.display(), e));
                } else {
                    written += 1;
                }
            }
        }
    }

    // Prune oldest snapshot dirs if retention exceeded.
    if let Err(e) = prune_old_dirs(Path::new(&output_path), retention).await {
        last_err = Some(format!("prune: {}", e));
    }

    {
        let mut r = runtime.write().await;
        r.last_snapshot_at = Some(now);
        r.total_snapshots_written = r.total_snapshots_written.saturating_add(written);
        r.last_instance_count = instance_count;
        if last_err.is_some() {
            r.last_error = last_err;
        } else {
            r.last_error = None;
        }
    }

    Ok(())
}

/// Build the per-tab payload — either the per-tab matrix directly
/// (alignment / opportunity / risk / analysis / advisory / decision)
/// or a small wrapper (metrics / mtf / recommendation).
fn build_tab_payload(tab: &str, snap: &MarketSnapshot) -> serde_json::Value {
    match tab {
        "alignment" => serde_json::to_value(&snap.alignment).unwrap_or(serde_json::Value::Null),
        "opportunity" => serde_json::to_value(&snap.opportunity).unwrap_or(serde_json::Value::Null),
        "risk" => serde_json::to_value(&snap.risk).unwrap_or(serde_json::Value::Null),
        "analysis" => serde_json::to_value(&snap.analysis).unwrap_or(serde_json::Value::Null),
        "advisory" => serde_json::to_value(&snap.advisory).unwrap_or(serde_json::Value::Null),
        "decision" => {
            serde_json::to_value(&snap.decision_context).unwrap_or(serde_json::Value::Null)
        }
        "metrics" => serde_json::to_value(snap).unwrap_or(serde_json::Value::Null),
        "mtf" => serde_json::json!({
            "slot": snap.timeframe_slot.as_ref().map(|ts| ts.as_str()).unwrap_or_default(),
            "timeframe_secs": snap.timeframe_secs,
            "indicators": snap.indicators.len(),
            "alignment": snap.alignment,
            "analysis": snap.analysis,
            "advisory": snap.advisory,
            "decision_context": snap.decision_context,
        }),
        "recommendation" => serde_json::json!({
            "advisory": snap.advisory,
            "decision_context": snap.decision_context,
        }),
        _ => serde_json::json!({
            "unknown_tab": tab,
            "message": "tab id not recognised by snapshot exporter",
        }),
    }
}

/// Sanitise a pair-key for use as a filename. Replaces any non-
/// alphanumeric character with `_` (pair-keys already use `-` as
/// the only non-alphanumeric character, but be defensive).
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Walk `<output_root>/*/*` (the `YYYY-MM-DD/HHhMMmSS/` tree) and
/// remove the oldest directories until `retention` or fewer remain.
/// Lexicographic sort on the `YYYY-MM-DD` and `HHhMMmSS` segments
/// matches chronological order because the date format is
/// ISO-8601-compatible.
async fn prune_old_dirs(output_root: &Path, retention: u32) -> std::io::Result<()> {
    if !tokio::fs::try_exists(output_root).await? {
        return Ok(());
    }
    let mut date_dirs = tokio::fs::read_dir(output_root).await?;
    let mut all_date_dirs: Vec<PathBuf> = Vec::new();
    while let Some(entry) = date_dirs.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            all_date_dirs.push(entry.path());
        }
    }
    all_date_dirs.sort();

    // Count total snapshot subdirs across all date dirs.
    let mut snapshots: Vec<PathBuf> = Vec::new();
    for date_dir in &all_date_dirs {
        let mut time_dirs = tokio::fs::read_dir(date_dir).await?;
        while let Some(entry) = time_dirs.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                snapshots.push(entry.path());
            }
        }
    }
    snapshots.sort();

    let retention = retention as usize;
    while snapshots.len() > retention {
        let oldest = snapshots.remove(0);
        if let Err(e) = tokio::fs::remove_dir_all(&oldest).await {
            eprintln!(
                "📸 Snapshot Export: prune failed for {}: {}",
                oldest.display(),
                e
            );
        }
    }
    Ok(())
}

// ─── Test-only helpers ─────────────────────────────────────────────────

/// In-memory manual-tick signal used by both the HTTP handler and
/// the test suite. Wrap in `Arc<Notify>` at the call site.
#[allow(dead_code)]
pub fn new_manual_tick() -> Arc<Notify> {
    Arc::new(Notify::new())
}

/// Helper for the CLI / tests — `Arc<Mutex<()>>` lock used by
/// `tick_once` to serialise concurrent manual + scheduled ticks.
#[allow(dead_code)]
pub fn new_tick_lock() -> Arc<Mutex<()>> {
    Arc::new(Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_non_alphanumeric() {
        assert_eq!(sanitize("BTC-USDT"), "BTC_USDT");
        assert_eq!(sanitize("eth/usdt"), "eth_usdt");
        assert_eq!(sanitize("BTCUSDT"), "BTCUSDT");
    }

    #[test]
    fn runtime_from_config_uses_defaults_when_tabs_none() {
        let cfg = SnapshotExportConfig::default();
        let rt = runtime_from_config(&cfg);
        assert_eq!(rt.tabs.len(), ALL_TABS.len());
        assert!(!rt.enabled);
        assert_eq!(rt.interval_secs, 60);
        assert_eq!(rt.max_snapshots_retained, 1000);
    }

    #[test]
    fn runtime_from_config_filters_unknown_tabs() {
        let cfg = SnapshotExportConfig {
            tabs: Some(vec![
                "alignment".into(),
                "not_a_real_tab".into(),
                "risk".into(),
                "alignment".into(),
            ]),
            ..SnapshotExportConfig::default()
        };
        let rt = runtime_from_config(&cfg);
        assert_eq!(rt.tabs, vec!["alignment".to_string(), "risk".to_string()]);
    }

    #[test]
    fn runtime_from_config_empty_tabs_falls_back_to_all() {
        let cfg = SnapshotExportConfig {
            tabs: Some(vec![]),
            ..SnapshotExportConfig::default()
        };
        let rt = runtime_from_config(&cfg);
        assert_eq!(rt.tabs.len(), ALL_TABS.len());
    }

    #[test]
    fn runtime_from_config_clamps_interval_and_retention() {
        let cfg = SnapshotExportConfig {
            interval_secs: 1,
            max_snapshots_retained: 5,
            ..SnapshotExportConfig::default()
        };
        let rt = runtime_from_config(&cfg);
        assert_eq!(rt.interval_secs, 5);
        assert_eq!(rt.max_snapshots_retained, 10);
    }

    #[tokio::test]
    async fn tick_once_disabled_writes_nothing() {
        let rt = Arc::new(RwLock::new(SnapshotExportRuntime::default())); // enabled = false
        let ws = WorkspaceState::empty();
        let out = std::env::temp_dir().join(format!("snap_disabled_{}", std::process::id()));
        {
            let mut r = rt.write().await;
            r.output_path = out.to_string_lossy().to_string();
        }
        let res = tick_once(&rt, &ws).await;
        assert!(res.is_ok());
        assert!(
            !out.exists(),
            "disabled runtime must not create the output dir"
        );
    }

    #[tokio::test]
    async fn tick_once_enabled_creates_dir_and_updates_runtime() {
        let rt = Arc::new(RwLock::new(SnapshotExportRuntime::default()));
        let ws = WorkspaceState::empty();
        let out = std::env::temp_dir().join(format!("snap_enabled_{}", std::process::id()));
        // Fresh temp dir per run.
        let _ = tokio::fs::remove_dir_all(&out).await;
        {
            let mut r = rt.write().await;
            r.enabled = true;
            r.output_path = out.to_string_lossy().to_string();
        }
        let res = tick_once(&rt, &ws).await;
        assert!(res.is_ok(), "tick should succeed: {:?}", res.err());
        let now = chrono::Utc::now();
        let date_part = now.format("%Y-%m-%d").to_string();
        let time_part = now.format("%Hh%Mm%Ss").to_string();
        let tick_dir = out.join(&date_part).join(&time_part);
        assert!(
            tick_dir.is_dir(),
            "tick dir should exist: {}",
            tick_dir.display()
        );
        // No instances → no per-tab files, but counters must be updated.
        let r = rt.read().await;
        assert!(r.last_snapshot_at.is_some());
        assert_eq!(r.total_snapshots_written, 0);
        assert_eq!(r.last_instance_count, 0);
        assert!(r.last_error.is_none());
        // Cleanup.
        drop(r);
        let _ = tokio::fs::remove_dir_all(&out).await;
    }

    #[tokio::test]
    async fn tick_once_captures_disk_error_in_last_error() {
        let rt = Arc::new(RwLock::new(SnapshotExportRuntime::default()));
        let ws = WorkspaceState::empty();
        // Point the output path at a location that cannot be created:
        // a path whose parent is a FILE (not a directory).
        let bad_parent = std::env::temp_dir().join(format!("snap_bad_{}.txt", std::process::id()));
        tokio::fs::write(&bad_parent, b"x").await.unwrap();
        let out = bad_parent.join("sub");
        {
            let mut r = rt.write().await;
            r.enabled = true;
            r.output_path = out.to_string_lossy().to_string();
        }
        let res = tick_once(&rt, &ws).await;
        assert!(res.is_err(), "tick should fail on unwritable path");
        let r = rt.read().await;
        assert!(r.last_error.is_some());
        assert!(r.last_snapshot_at.is_none());
        drop(r);
        let _ = tokio::fs::remove_file(&bad_parent).await;
    }
}
