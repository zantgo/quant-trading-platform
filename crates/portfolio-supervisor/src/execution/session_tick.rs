//! The shared per-tick session body — the structural parity contract
//! between the live session, paper trading, and the Backtesting Engine.
//!
//! Every tick of the TAE loop runs the **same** sequence through the
//! **same** code, no matter where the snapshot came from:
//!
//! ```text
//! mark_to_market (if a position is open)
//!   → fills branch (live: poll+apply external; paper/backtest: evaluate)
//!   → SetupExecutor::tick
//! ```
//!
//! The daemon drives this with live WS snapshots; the backtest runner
//! drives it with recorded/archived snapshots. Fills, sizing, safety
//! ladder and fees are the same functions on the same config — so a
//! backtest result equals the paper result by construction (live differs
//! only by venue fills/latency). See
//! `docs/engines/backtesting-engine/08-04-parity-contract.md`.

use core_domain::models::MarketSnapshot;
use rust_decimal::Decimal;
use std::sync::Arc;

use super::engine::{CloseOutcome, ExecutionEngine};
use crate::setup_executor::{SetupExecutor, TickContext};

/// What the tick did (extension point for call-site bookkeeping; the
/// execution itself is identical everywhere).
#[derive(Debug, Default, Clone)]
pub struct SessionTickOutcome {
    /// The candle timestamp the executor saw (`max` over the snapshots).
    pub candle_ts: u64,
    /// Equity after the tick (ledger only; call sites add unrealized).
    pub equity: f64,
    /// The close outcome observed between fills and the executor tick
    /// (only populated when `capture_last_close` is set — the backtest
    /// reads it before the executor's own consumption).
    pub last_close: Option<CloseOutcome>,
    /// Direction of the closed position (for the backtest trade log).
    pub last_close_direction: Option<config_models::Direction>,
    /// Size of the closed position (for the backtest trade log).
    pub last_close_size: Option<Decimal>,
    /// Entry price of the closed position (for the backtest trade log).
    pub last_close_entry: Option<Decimal>,
    /// v10: entry timestamp of the closed position (ms; for the enriched
    /// backtest trade schema).
    pub last_close_entry_ts_ms: Option<u64>,
    /// v10: MFE% / MAE% over the hold (signed by direction).
    pub last_close_mfe_pct: Option<f64>,
    pub last_close_mae_pct: Option<f64>,
}

/// Run one session tick. `dispatch` is decided by the call site
/// (observe → false; paper/backtest/live → true); `live_fills` swaps the
/// fill source for live-mode engines (the daemon polls the broker);
/// `capture_last_close` snapshots the close outcome after fills and
/// before the executor consumes it (backtest trade log).
pub async fn run_tick(
    engine: &Arc<ExecutionEngine>,
    executor: &SetupExecutor,
    instance_id: &str,
    symbol: &str,
    snaps: &[&MarketSnapshot],
    mid: Decimal,
    ctx: TickContext,
    live_fills: Option<Vec<crate::execution::backend::Fill>>,
    capture_last_close: bool,
) -> SessionTickOutcome {
    let candle_ts = snaps.iter().map(|s| s.timestamp).max().unwrap_or(0);

    let mut outcome = SessionTickOutcome {
        candle_ts,
        ..SessionTickOutcome::default()
    };

    // The closed-position context (captured before mark-to-market).
    let prev_position = engine.get_position(symbol).await;

    // Mark-to-market (live unrealized PnL for display).
    if prev_position.is_some() {
        engine.mark_to_market(symbol, mid).await;
    }

    // Fills first (entry, TP, SL), then the state machine.
    if ctx.dispatch {
        match live_fills {
            Some(fills) => {
                engine.apply_external_fills(fills).await;
            }
            None => {
                engine.evaluate_order_fills(symbol, mid).await;
            }
        }
    }

    // The backtest captures the close (TP/SL fill) before the executor's
    // tick_position consumes it; the daemon never sets this flag.
    if capture_last_close {
        if let Some(close) = engine.take_last_close(symbol).await {
            if let Some(pos) = &prev_position {
                outcome.last_close_direction = Some(pos.direction);
                outcome.last_close_size = Some(pos.size);
                outcome.last_close_entry = Some(pos.entry_price);
                outcome.last_close_entry_ts_ms = Some(pos.opened_at_ms);
                outcome.last_close_mfe_pct = Some(pos.mfe_pct);
                outcome.last_close_mae_pct = Some(pos.mae_pct);
            }
            // v8.2: feed the simulated safety ladder here — the executor's
            // own take (tick_position) now finds the slot empty.
            if let Some(safety) = &ctx.safety {
                safety.record_trade_outcome(symbol, close.is_loss).await;
            }
            outcome.last_close = Some(close);
        }
    }

    let safety_ref = ctx.safety.clone();
    executor
        .tick(instance_id, symbol, snaps.to_vec(), mid, ctx)
        .await;

    // v8.2: market closes (signal-flip) happen INSIDE the executor tick
    // and leave the close in the engine slot. Capture them here with the
    // pre-tick position context (the flipped position was still open at
    // tick start) — otherwise the ledger would record them one tick late
    // with no direction/size context.
    if capture_last_close && outcome.last_close.is_none() {
        if let Some(close) = engine.take_last_close(symbol).await {
            if let Some(pos) = &prev_position {
                outcome.last_close_direction = Some(pos.direction);
                outcome.last_close_size = Some(pos.size);
                outcome.last_close_entry = Some(pos.entry_price);
                outcome.last_close_entry_ts_ms = Some(pos.opened_at_ms);
                outcome.last_close_mfe_pct = Some(pos.mfe_pct);
                outcome.last_close_mae_pct = Some(pos.mae_pct);
            }
            if let Some(safety) = &safety_ref {
                safety.record_trade_outcome(symbol, close.is_loss).await;
            }
            outcome.last_close = Some(close);
        }
    }

    outcome.equity = engine.get_equity().await;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_trading::FeesConfig;
    use config_models::MinimalTaeConfig;
    use rust_decimal_macros::dec;

    fn tae_cfg() -> MinimalTaeConfig {
        MinimalTaeConfig {
            enabled: true,
            allocation_pct: 10.0,
            min_net_rr: 1.0,
            max_position_size_pct_of_equity: None,
            max_open_positions: 1,
            entry_mode: "zone_midpoint".to_string(),
            invalidate_on: "direction_flip".to_string(),
        }
    }

    fn plain_snapshot(ts: u64, mid: f64) -> MarketSnapshot {
        MarketSnapshot {
            symbol: "BTC-USDC".to_string(),
            timeframe_secs: 60,
            timestamp: ts,
            is_completed: Some(true),
            mid_price: Decimal::from_f64_retain(mid).unwrap_or_default(),
            bid_price: Decimal::from_f64_retain(mid).unwrap_or_default(),
            ask_price: Decimal::from_f64_retain(mid).unwrap_or_default(),
            close: Some(Decimal::from_f64_retain(mid).unwrap_or_default()),
            ..MarketSnapshot::default()
        }
    }

    #[tokio::test]
    async fn run_tick_is_deterministic_and_idempotent_across_engines() {
        // Two fresh engines driven by the same tick sequence must produce
        // byte-identical outcomes — the backtest determinism contract.
        for _ in 0..2 {
            let engine = Arc::new(ExecutionEngine::new(FeesConfig::default()));
            engine.set_initial_equity(dec!(1000)).await;
            let executor = SetupExecutor::new(engine.clone(), &tae_cfg());

            for (ts, mid) in [(1u64, 100.0f64), (2, 100.0), (3, 100.0)] {
                let snap = plain_snapshot(ts, mid);
                let _outcome = run_tick(
                    &engine,
                    &executor,
                    "test",
                    "BTC-USDC",
                    &[&snap],
                    snap.mid_price,
                    TickContext {
                        safety_allows_entry: true,
                        lifecycle_running: true,
                        market_filter_allows_entry: true,
                        entry_block_reason: None,
                        candle_ts: ts,
                        safety: None,
                        dispatch: true,
                        allocation_pct: None,
                        strategy: None,
                    },
                    None,
                    true,
                )
                .await;
            }
            assert_eq!(
                engine.get_equity().await,
                1000.0,
                "no fills without decision matrices"
            );
        }
    }

    #[tokio::test]
    async fn run_tick_respects_dispatch_gate() {
        let engine = Arc::new(ExecutionEngine::new(FeesConfig::default()));
        engine.set_initial_equity(dec!(1000)).await;
        let executor = SetupExecutor::new(engine.clone(), &tae_cfg());
        let snap = plain_snapshot(1, 100.0);
        let outcome = run_tick(
            &engine,
            &executor,
            "test",
            "BTC-USDC",
            &[&snap],
            snap.mid_price,
            TickContext {
                safety_allows_entry: true,
                lifecycle_running: true,
                market_filter_allows_entry: true,
                entry_block_reason: None,
                candle_ts: 1,
                safety: None,
                dispatch: false,
                allocation_pct: None,
                strategy: None,
            },
            None,
            false,
        )
        .await;
        assert_eq!(outcome.candle_ts, 1);
        assert_eq!(engine.get_equity().await, 1000.0);
    }
}

#[cfg(test)]
mod flip_tests {
    use super::*;
    use crate::paper_trading::FeesConfig;
    use crate::safety::SafetyManager;
    use config_models::MinimalTaeConfig;
    use config_models::{OrderPacket, OrderSide, OrderType};
    use rust_decimal_macros::dec;

    fn tae_cfg() -> MinimalTaeConfig {
        MinimalTaeConfig {
            enabled: true,
            allocation_pct: 10.0,
            min_net_rr: 1.0,
            max_position_size_pct_of_equity: None,
            max_open_positions: 10,
            entry_mode: "zone_midpoint".to_string(),
            invalidate_on: "direction_flip".to_string(),
        }
    }

    /// v8.2 regression: a market close left in the engine slot (the
    /// signal-flip path's mechanics) must be captured in the SAME tick
    /// the run observes it — not lost or deferred to a later tick. In the
    /// real flip flow the pre-tick position context is present (the flip
    /// happens during the tick); this synthetic case closes between ticks,
    /// so only the capture timing + safety-ladder feed are asserted here.
    #[tokio::test]
    async fn market_close_during_tick_captures_position_context() {
        let engine = Arc::new(ExecutionEngine::new(FeesConfig::default()));
        engine.set_initial_equity(dec!(1000)).await;
        // Open a position directly on the engine (as if an entry filled).
        let packet = OrderPacket {
            client_order_id: "entry".into(),
            symbol: "BTC-USDC".into(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(dec!(100)),
            size: dec!(1),
            reduce_only: false,
            is_emergency_liquidation: false,
            associated_position_id: None,
            metadata: Default::default(),
        };
        let _ = engine.submit_order(packet, dec!(99)).await;

        let executor = SetupExecutor::new(engine.clone(), &tae_cfg());
        let safety = Arc::new(SafetyManager::new(3, 5, 8, 30.0, 5.0, 80.0));

        // Tick 1: nothing happens; the close slot is empty.
        let snap = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            timeframe_secs: 60,
            timestamp: 1,
            is_completed: Some(true),
            mid_price: dec!(100),
            ..MarketSnapshot::default()
        };
        let _ = run_tick(
            &engine,
            &executor,
            "test",
            "BTC-USDC",
            &[&snap],
            dec!(100),
            TickContext {
                safety_allows_entry: true,
                lifecycle_running: true,
                market_filter_allows_entry: true,
                entry_block_reason: None,
                candle_ts: 1,
                safety: None,
                dispatch: true,
                allocation_pct: None,
                strategy: None,
            },
            None,
            true,
        )
        .await;

        // Close the position AT MARKET between ticks (the flip path's
        // mechanics) — a LOSS, so the safety ladder must record it.
        let _ = engine
            .close_position("BTC-USDC", dec!(98), "invalidated_signal")
            .await;

        // Tick 2: the capture must pick up the market close WITH the
        // pre-tick position context and feed the safety ladder.
        let snap2 = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            timeframe_secs: 60,
            timestamp: 2,
            is_completed: Some(true),
            mid_price: dec!(98),
            ..MarketSnapshot::default()
        };
        let outcome = run_tick(
            &engine,
            &executor,
            "test",
            "BTC-USDC",
            &[&snap2],
            dec!(98),
            TickContext {
                safety_allows_entry: true,
                lifecycle_running: true,
                market_filter_allows_entry: true,
                entry_block_reason: None,
                candle_ts: 2,
                safety: Some(safety.clone()),
                dispatch: true,
                allocation_pct: None,
                strategy: None,
            },
            None,
            true,
        )
        .await;

        let close = outcome.last_close.expect("market close captured in-tick");
        assert_eq!(close.exit_reason, "invalidated_signal");
        // The safety ladder saw the loss (the parity contract).
        let losses = safety.consecutive_losses.read().await;
        assert_eq!(losses.get("BTC-USDC"), Some(&1));

        // Nothing stale surfaces on the next tick (the capture is
        // same-tick, never deferred).
        let snap3 = MarketSnapshot {
            symbol: "BTC-USDC".into(),
            timeframe_secs: 60,
            timestamp: 3,
            is_completed: Some(true),
            mid_price: dec!(98),
            ..MarketSnapshot::default()
        };
        let outcome3 = run_tick(
            &engine,
            &executor,
            "test",
            "BTC-USDC",
            &[&snap3],
            dec!(98),
            TickContext {
                safety_allows_entry: true,
                lifecycle_running: true,
                market_filter_allows_entry: true,
                entry_block_reason: None,
                candle_ts: 3,
                safety: None,
                dispatch: true,
                allocation_pct: None,
                strategy: None,
            },
            None,
            true,
        )
        .await;
        assert!(
            outcome3.last_close.is_none(),
            "close must not be captured a tick late"
        );
    }
}
