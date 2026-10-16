//! HTTP handler group extracted from `api-gateway` (v11.14.2 build-time split).
//!
//! These handlers are addressed through the `api_gateway::handlers::`
//! namespace, which `api-gateway` re-assembles from the group crates so
//! that no consumer import path changed.

pub mod cluster_status;
pub mod connection_quality;
pub mod history;
pub mod monitor;
pub mod overview;
pub mod snapshot_export;
