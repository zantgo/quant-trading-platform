//! Consolidated integration tests: 5 original binaries merged into one.
//!
//! Every module below is verified free of process-global state -- no
//! `MARKET_MONITOR_CONFIG` mutation and no `init_master_key` call -- so they
//! can safely share a process. See
//! `MARKET_MONITOR_CONFIG_ISOLATION.md` for why the others cannot be merged.

#[allow(
    clippy::module_inception,
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    clippy::let_underscore_must_use,
    unused_parens,
    unused_assignments,
    clippy::needless_borrow,
    clippy::too_many_arguments,
    clippy::field_reassign_with_default,
    clippy::type_complexity,
    clippy::large_enum_variant
)]
#[path = "_parts/cluster_status_api.rs"]
mod cluster_status_api;

#[path = "_parts/history_allocation_bound.rs"]
mod history_allocation_bound;

#[path = "_parts/history_projection_parity.rs"]
mod history_projection_parity;

#[path = "_parts/slot_identity.rs"]
mod slot_identity;

#[path = "_parts/sub_minute_history.rs"]
mod sub_minute_history;
