//! # Core Domain Crate
//!
//! Stateless DTOs, JSON-RPC schemas, and shared types used across all
//! downstream crates in the workspace. Pure leaf crate — has no dependency
//! on any other workspace crate.

pub mod advisory;
pub mod alignment;
pub mod analysis;
pub mod decision_context;
pub mod decision_params;
pub mod indicator_dtos;
pub mod jsonrpc;
pub mod jsonrpc_methods;
pub mod latency;
pub mod liquidity;
pub mod market_context;
pub mod models;
pub use models::{
    duration_from_label, duration_label, duration_label_upper, is_supported_duration,
    slot_label_for, SUPPORTED_DURATIONS,
};
pub mod normalized;
pub mod opportunity;
pub mod overview;
pub mod overview_panel;
pub mod performance;
pub mod portfolio;
pub mod risk;
pub mod risk_reward;
pub mod snapshot_export;
pub mod state_matrix;
pub mod statistics;
pub mod timeframe_category;
pub mod volume_profile;

pub use latency::{LatencySnapshot, LatencyTracker, SharedLatencyTracker};
pub use normalized::TriggerType;
