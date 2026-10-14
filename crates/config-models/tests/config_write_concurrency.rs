//! AUDIT-M8 regression: concurrent `config.toml` writes must not lose data.
//!
//! Nine call sites mutate the workspace — `POST /api/config`,
//! `POST /api/instances/:id/config`, `registry::add_instance`,
//! `registry::delete_instance`, `recharge_instance`, the strategy upsert,
//! `POST /api/account/capital`, `quit_session`, and the background boot-spawn
//! retry loop (which runs for up to ~10 minutes per entry). Each did
//! clone -> mutate -> `save_workspace` with no lock, and `WorkspaceState`'s
//! `RwLock` is a whole-value COPY, so it serialised nothing for that sequence.
//!
//! Two writers interleaving on ONE shared `config.toml.tmp` produced either:
//!   * a LOST UPDATE — A's ladder change silently overwritten by B's instance
//!     write, with A returning 200; or
//!   * a spurious `500 "Failed to persist configuration file"` from a rename
//!     that lost the race, while the data WAS in fact on disk.
//!
//! The write itself was always atomic (tmp + rename), so `config.toml` could
//! never be truncated — this is lost-update, not corruption.
//!
//! This file pins the FILE-LEVEL guarantees, which is what `save_workspace`
//! alone can provide:
//!   * concurrent saves never error (no rename race, no temp-file clobber),
//!   * the file always stays parseable with a `[workspace]` table,
//!   * no temp files are left behind,
//!   * a failed save leaves the existing file byte-identical,
//!   * platform-level tables survive.
//!
//! Cross-CALL-SITE lost updates (A's ladder change overwritten by B's instance
//! write) are a different problem: each caller reads via
//! `WorkspaceState::config()`, a whole-value clone, so its read happens OUTSIDE
//! any file lock. That is closed one level up by
//! `WorkspaceState::mutate_and_persist`, which holds the write lock across
//! read -> mutate -> persist -> publish. See
//! `portfolio-supervisor/tests/workspace_mutation_atomicity.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use config_models::{InstanceEntry, InstanceStatus, WorkspaceConfig};

/// Parse just the `[workspace]` table out of a full `config.toml`.
fn workspace_from(raw: &str) -> WorkspaceConfig {
    let doc: toml::Value = toml::from_str(raw).expect(
        "config.toml must remain parseable after concurrent writes — a torn \
         or empty file here would make the daemon fall back to the factory \
         template and lose every instance",
    );
    doc.get("workspace")
        .expect("config.toml must still carry a [workspace] table")
        .clone()
        .try_into()
        .expect("[workspace] must deserialize")
}

fn entry(symbol: &str) -> InstanceEntry {
    InstanceEntry {
        id: symbol.to_lowercase(),
        symbol: symbol.into(),
        quote: symbol
            .rsplit_once('-')
            .map(|(_, q)| q.into())
            .unwrap_or_default(),
        status: InstanceStatus::Running,
        timeframes: std::collections::BTreeMap::new(),
        automation: config_models::AutomationConfig::default(),
        operational_mode: config_models::OperationalMode::Advisory,
        mode: config_models::ExecutionMode::Observe,
        strategy: None,
        allocation_pct: None,
        weight_overrides: None,
        activation: None,
    }
}

/// Point `config_models` at a sandbox file and return its path.
fn sandbox(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cfgdur_{}_{}_{:?}",
        tag,
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("sandbox dir");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[workspace]\nid = \"main\"\nname = \"T\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n[workspace.candles]\nduration_seconds = 60\nanalysis_limit = 100\n",
    )
    .expect("seed config");
    std::env::set_var("MARKET_MONITOR_CONFIG", &path);
    path
}

/// AUDIT-M8: the whole suite runs sequentially in ONE test because
/// `MARKET_MONITOR_CONFIG` is a process-global env var — separate `#[test]`
/// functions would run in parallel threads and overwrite each other's sandbox
/// path, which is exactly the kind of cross-talk these tests exist to catch.
#[test]
fn config_writes_are_durable_under_concurrency_and_failure() {
    concurrent_writers_never_error_or_corrupt();
    a_failed_save_does_not_truncate_the_existing_config();
    platform_level_tables_survive_a_workspace_save();
}

fn concurrent_writers_never_error_or_corrupt() {
    let path = sandbox("concurrent");

    // Each writer starts from the CURRENT disk state, mutates one distinct
    // thing, and saves — exactly what the nine call sites do.
    let ok = Arc::new(AtomicUsize::new(0));
    let errs = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut handles = Vec::new();
    for i in 0..8u64 {
        let ok = ok.clone();
        let errs = errs.clone();
        let thread_path = path.clone();
        handles.push(std::thread::spawn(move || {
            // Deliberately yield so the threads interleave inside the
            // read-modify-write window.
            std::thread::yield_now();
            for _ in 0..3 {
                let disk = std::fs::read_to_string(&thread_path).expect("read");
                let mut ws = workspace_from(&disk);
                ws.instances.push(entry(&format!("P{i}-USDT")));
                match config_models::save_workspace(&ws) {
                    Ok(()) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => errs.lock().unwrap().push(format!("{e:?}")),
                }
                std::thread::yield_now();
            }
        }));
    }
    for h in handles {
        h.join().expect("join");
    }

    assert_eq!(
        errs.lock().unwrap().len(),
        0,
        "no writer may fail: {:?}",
        errs.lock().unwrap()
    );
    assert_eq!(ok.load(Ordering::Relaxed), 24, "every save must succeed");

    // The surviving file must be PARSEABLE and internally consistent — that is
    // the property that matters for the operator (the daemon cannot start on an
    // unparseable config).
    let raw = std::fs::read_to_string(&path).expect("read final");
    let final_ws = workspace_from(&raw);
    assert!(
        !final_ws.instances.is_empty(),
        "the final config must not be empty — an empty [workspace.instances] \
         would respawn nothing and silently drop every pair"
    );

    // No stray temp files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "every temp file must be renamed or cleaned up, found {leftovers:?}"
    );
}

fn a_failed_save_does_not_truncate_the_existing_config() {
    // Read-only the DIRECTORY, not just the file: POSIX `rename` only needs
    // write permission on the containing directory, so a read-only config.toml
    // would still be replaced successfully.
    let path = sandbox("readonly");
    let dir = path.parent().expect("dir").to_path_buf();
    let original = std::fs::read_to_string(&path).expect("read");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&dir).expect("meta").permissions();
        let mut locked = perms.clone();
        locked.set_mode(0o555);
        std::fs::set_permissions(&dir, locked).expect("chmod dir");

        let mut ws = WorkspaceConfig::default();
        ws.instances.push(entry("BTC-USDT"));
        let result = config_models::save_workspace(&ws);

        let mut restore = perms;
        restore.set_mode(0o755);
        let _ = std::fs::set_permissions(&dir, restore);

        assert!(
            result.is_err(),
            "an unwritable directory must produce an error, not a silent success"
        );
    }

    let after = std::fs::read_to_string(&path).expect("read after");
    assert_eq!(
        after, original,
        "a failed save must leave the existing config byte-identical"
    );
}

fn platform_level_tables_survive_a_workspace_save() {
    let path = sandbox("platform");
    // Add a platform-level table the workspace writer does not know about.
    let mut raw = std::fs::read_to_string(&path).expect("read");
    raw.push_str("\n[server]\nport = 4242\nbind = \"127.0.0.1\"\n");
    std::fs::write(&path, raw).expect("seed server");

    let mut ws = WorkspaceConfig::default();
    ws.instances.push(entry("ETH-USDT"));
    config_models::save_workspace(&ws).expect("save");

    let after = std::fs::read_to_string(&path).expect("read after");
    assert!(
        after.contains("port = 4242"),
        "the platform-level [server] table must be preserved, got:\n{after}"
    );
}
