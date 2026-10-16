# Process-global state and test-binary merging

Twelve of these integration tests touch **process-global** state, so each keeps
its own binary:

    config_recharge_report   config_round_trip      full_ladder_recharge
    keys_api                 mode_at_creation       mode_payload_api
    non_ascii_symbol_add     quit_persists          save_recharge_cycle
    session_init_api         session_recovery_mode  session_ui_gate

## The two globals

**1. `MARKET_MONITOR_CONFIG`** (an environment variable, set by ten of the
above). `config-models` reads it on every config load, so **readers are exposed
too**, not just writers -- a test can silently resolve the variable while a
concurrent test has it pointed at a different temp directory.

**2. `MASTER_KEY`** in `database-storage::crypto`:

    static MASTER_KEY: Mutex<Option<[u8; 32]>> = Mutex::new(None);

set by `crypto::init_master_key`, which `keys_api` calls while building its
`AppState`. Two tests using different master secrets produce rows that cannot
be decrypted with the current key, surfacing as `409 CONFLICT` on
`/api/keys/rotate`.

## Why these cannot simply be merged behind a lock

The twelve modules declare their own `static CONFIG_ENV_LOCK`, `CONFIG_FILE_LOCK`
and `COUNTER`. Those are **per-module**, so after merging into one binary each
module holds a *different* lock -- which contains nothing, because the state
they guard is process-wide. This is not hypothetical: `quit_session_persists_empty_workspace_to_disk`
failed with `fixture must seed 2 instances` (saw 1) purely from interference
with `save_recharge_cycle`, and `keys_crud_encryption_rotation_and_backup`
failed with `409` purely from interference with `mode_at_creation`.

A shared lock was also rejected:

- The tests are a mix of synchronous `#[test]` and `#[tokio::test]`.
- `std::sync::MutexGuard` is **not `Send`**, so holding one across an `.await`
  in a `#[tokio::test]` does not compile.
- A `tokio::sync::Mutex` cannot be locked from a synchronous `#[test]` without
  standing up a runtime, and its guard cannot then be held across the body.

Serialising them inside one binary would also have been a false economy: cargo
already runs test binaries **one at a time**, so isolating them costs no wall
clock at all.

## Result

**18 binaries, down from 28.** Sixteen process-clean modules are merged into six
grouped binaries (`api_routes_core`, `api_routes_market`, `api_routes_analytics`,
`api_ws_lifecycle`, `api_observation`, `api_pme_risk`); the twelve above each
keep one.

## If you add a test

If it calls `init_master_key`, sets `MARKET_MONITOR_CONFIG`, or otherwise
mutates process-global state, give it its own file in `tests/`. Do not merge it
into a grouped binary.
