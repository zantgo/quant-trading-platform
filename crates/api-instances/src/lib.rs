//! HTTP handler group extracted from `api-gateway` (v11.14.2 build-time split).
//!
//! These handlers are addressed through the `api_gateway::handlers::`
//! namespace, which `api-gateway` re-assembles from the group crates so
//! that no consumer import path changed.

pub mod config;
pub mod instances;
pub mod keys;
pub mod profiles;
pub mod session;
pub mod strategies;
