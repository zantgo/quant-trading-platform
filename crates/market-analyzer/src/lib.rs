//! # Market Analyzer Crate
//!
//! Owns the Market Monitoring Engine (MME): the indicator registry, the
//! per-duration pipeline that emits `MarketSnapshot`s, and the three
//! descriptive reductions (`timeframe_state` → `cross_tf_state` →
//! `cross_pair_state`) that form the model envelope.

#![allow(clippy::too_many_arguments)]

pub mod active_set;
pub mod analyzer;
pub mod candle_aggregator;
pub mod candle_builder;
pub mod candle_generator;
// event_detector module removed (v6.4 — dormant, superseded by signal system)
pub mod indicators;
pub mod leg;
pub mod market_context_synth;
pub mod model;
pub mod replay;
pub mod sr_engine;
pub mod strategy_params;
pub mod synthesis;
