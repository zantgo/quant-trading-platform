//! # Market Analyzer Crate
//!
//! Owns the Market Monitoring Engine (MME): 52 indicators across 4
//! timeframes, signal detection, multi-timeframe alignment, opportunity
//! and risk scoring, and the time-frame pipeline orchestrator that emits
//! `MarketSnapshot`s.

#![allow(clippy::too_many_arguments)]

pub mod active_set;
pub mod analyzer;
pub mod candle_aggregator;
pub mod candle_builder;
pub mod candle_generator;
// event_detector module removed (v6.4 — dormant, superseded by signal system)
pub mod indicators;
pub mod market_context_synth;
pub mod sr_engine;
pub mod strategy_params;
pub mod synthesis;
