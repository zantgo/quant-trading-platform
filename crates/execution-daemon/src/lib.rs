//! Library exports for the `execution-daemon` crate.
//!
//! The crate is a Cargo workspace member that ships both a library
//! (`lib.rs`) and a binary (`main.rs`). The library surface is small:
//! other crates depend on it only for the periodic
//! snapshot-export task implementation
//! (`snapshot_export::{run_snapshot_exporter, tick_once, ...}`). The
//! shared DTOs (`SnapshotExportRuntime`, `ALL_TABS`, ...) live in
//! `core_domain::snapshot_export` to avoid circular dependencies
//! between `api-gateway` and `execution-daemon`.

pub mod cli_renderer;
pub mod ds_exporter;
pub mod memory_governor;
pub mod snapshot_export;
