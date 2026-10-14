//! AUDIT-M8: workspace mutations must be atomic against each other.
//!
//! Nine call sites mutate the workspace — `POST /api/config`,
//! `POST /api/instances/:id/config`, `add_instance`, `delete_instance`,
//! `recharge_instance`, the strategy upsert, `POST /api/account/capital`,
//! `quit_session`, and the background boot-spawn retry loop. Each did
//! `config()` -> mutate -> `save_workspace` -> `set_config`: every step is
//! individually locked, but the SEQUENCE is not.
//!
//! `WorkspaceState::config()` returns a whole-value clone, so the read happens
//! outside any lock. Two mutators interleaving produced either a LOST UPDATE
//! (A's ladder change silently overwritten by B's instance write, with A
//! returning 200) or a spurious 500 from a rename race.
//!
//! This is not theoretical: the boot-spawn loop retries for up to ~10 minutes
//! PER entry, serially, so it is routinely still writing while an operator
//! changes the timeframe ladder in another tab.
//!
//! `mutate_and_persist` holds the write lock across read -> mutate -> persist
//! -> publish, closing the window entirely.

use std::sync::Arc;

use config_models::{InstanceEntry, InstanceStatus, WorkspaceConfig};
use portfolio_supervisor::workspace_state::WorkspaceState;

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
        // 1% each so a 33-instance workspace stays under the
        // `Σ allocation_pct <= 100` validation gate that `save_workspace`
        // enforces.
        allocation_pct: Some(1.0),
        weight_overrides: None,
        activation: None,
    }
}

/// `save_workspace` re-reads `config.toml` to preserve the platform-level
/// tables, so the file must exist for a mutation to persist. Point the loader at
/// a per-process sandbox and return a seeded `WorkspaceState`.
fn seeded() -> Arc<WorkspaceState> {
    let dir = std::env::temp_dir().join(format!("wsatomic_ok_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("sandbox");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[workspace]\nid = \"main\"\nname = \"T\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n",
    )
    .expect("seed config");
    std::env::set_var("MARKET_MONITOR_CONFIG", &path);

    let mut cfg = WorkspaceConfig::default();
    cfg.instances.push(entry("BTC-USDT"));
    Arc::new(WorkspaceState::new(cfg))
}

/// The headline property: many concurrent appends, every one of them survives.
///
/// Under the old read-clone -> mutate -> save sequence the vast majority were
/// lost, because each thread read the same pre-mutation snapshot.
/// AUDIT-M8: all four scenarios run sequentially in ONE test.
///
/// `MARKET_MONITOR_CONFIG` is a PROCESS-GLOBAL env var, and
/// `save_workspace` resolves the config path through it. Separate `#[test]`
/// functions run in parallel threads, so they would overwrite each other's
/// sandbox path — a false pass or a spurious failure depending on timing. (The
/// same lesson as `config-models/tests/config_write_concurrency.rs`.)
///
/// The per-scenario tokio runtimes below still use multiple worker threads, so
/// the CONCURRENCY under test is real.
#[test]
fn workspace_mutations_are_atomic_against_each_other() {
    run(concurrent_appends_all_survive());
    run(a_ladder_change_racing_an_append());
    run(a_declined_mutation_changes_nothing());
    run(memory_published_only_after_write());
}

fn run(f: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f);
}

async fn concurrent_appends_all_survive() {
    let ws = seeded();

    let mut handles = Vec::new();
    for i in 0..32u32 {
        let ws = ws.clone();
        handles.push(tokio::spawn(async move {
            let sym = format!("P{i}-USDT");
            ws.mutate_and_persist(move |cfg| {
                cfg.instances.push(entry(&sym));
                Some(cfg.clone())
            })
            .await
            .expect("append must succeed");
        }));
    }
    for h in handles {
        h.await.expect("join");
    }

    let final_cfg = ws.config().await;
    // 1 seeded + 32 appended. Lost updates would leave far fewer.
    assert_eq!(
        final_cfg.instances.len(),
        33,
        "every concurrent append must be preserved; lost updates would leave \
         fewer than 33 entries"
    );
    for i in 0..32u32 {
        assert!(
            final_cfg
                .instances
                .iter()
                .any(|e| e.symbol == format!("P{i}-USDT")),
            "P{i}-USDT was lost"
        );
    }
}

/// A ladder change and an instance append racing must BOTH land.
async fn a_ladder_change_racing_an_append() {
    let ws = seeded();
    let ladder = vec![60u64, 300, 900];

    let a = {
        let ws = ws.clone();
        let ladder = ladder.clone();
        tokio::spawn(async move {
            ws.mutate_and_persist(move |cfg| {
                cfg.timeframes = ladder;
                Some(cfg.clone())
            })
            .await
        })
    };
    let b = {
        let ws = ws.clone();
        tokio::spawn(async move {
            ws.mutate_and_persist(move |cfg| {
                cfg.instances.push(entry("ETH-USDT"));
                Some(cfg.clone())
            })
            .await
        })
    };
    a.await.expect("join").expect("ladder save");
    b.await.expect("join").expect("append save");

    let final_cfg = ws.config().await;
    assert_eq!(
        final_cfg.timeframes, ladder,
        "the ladder change must survive a concurrent instance append"
    );
    assert!(
        final_cfg.instances.iter().any(|e| e.symbol == "ETH-USDT"),
        "the instance append must survive a concurrent ladder change"
    );
}

/// Returning `None` aborts the mutation without persisting or publishing.
async fn a_declined_mutation_changes_nothing() {
    let ws = seeded();
    let before = ws.config().await;

    let out = ws
        .mutate_and_persist(|cfg| {
            cfg.instances.clear();
            None // decline
        })
        .await
        .expect("a declined mutation is not an error");

    assert_eq!(
        out.instances.len(),
        before.instances.len(),
        "the stored config must be untouched"
    );
    assert_eq!(ws.config().await.instances.len(), before.instances.len());
}

/// A mutation must publish to memory only AFTER it is on disk.
async fn memory_published_only_after_write() {
    // Point config_models at an unwritable directory so the write fails.
    let dir = std::env::temp_dir().join(format!("wsatomic_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("sandbox");
    let cfg_path = dir.join("config.toml");
    std::fs::write(
        &cfg_path,
        "[workspace]\nid = \"main\"\nname = \"T\"\ndefault_currency = \"USDC\"\ndefault_exchange = \"Hyperliquid\"\n",
    )
    .expect("seed");

    let ws = seeded();
    let before = ws.config().await;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&dir).expect("meta").permissions();
        let mut locked = perms.clone();
        locked.set_mode(0o555);
        std::fs::set_permissions(&dir, locked).expect("chmod");

        std::env::set_var("MARKET_MONITOR_CONFIG", &cfg_path);
        let res = ws
            .mutate_and_persist(|cfg| {
                cfg.instances.clear();
                Some(cfg.clone())
            })
            .await;

        let mut restore = perms;
        restore.set_mode(0o755);
        let _ = std::fs::set_permissions(&dir, restore);
        std::env::remove_var("MARKET_MONITOR_CONFIG");

        assert!(
            res.is_err(),
            "an unwritable config must surface as an error"
        );
        assert_eq!(
            ws.config().await.instances.len(),
            before.instances.len(),
            "memory must NOT be published when the write failed — otherwise \\
             memory and disk diverge permanently"
        );
    }
}
