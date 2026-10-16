//! Consolidated integration tests: 2 original binaries merged into one.
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
#[path = "_parts/pme_portfolio_api.rs"]
mod pme_portfolio_api;

#[path = "_parts/recharge_abort_safety.rs"]
mod recharge_abort_safety;
