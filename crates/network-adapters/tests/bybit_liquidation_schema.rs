//! Bybit V5 liquidation-stream contract.
//!
//! Two venue-specific behaviours are pinned here, both of which would fail
//! *silently* — producing a plausible-looking chart with inverted meaning:
//!
//! 1. **Side semantics.** Per the Bybit docs, on `allLiquidation` a `S` of
//!    `Buy` means **a LONG was liquidated**. This is the OPPOSITE of Bitget's
//!    `fill` channel (`buy` = aggressor bought = a SHORT closed) and the SAME
//!    as Bitget's public `liquidation` channel. Porting either side blindly
//!    would render the heatmap's real layer backwards.
//!
//! 2. **Completeness.** `allLiquidation` pushes EVERY forced close at 500 ms
//!    — strictly better than Bitget's 1 Hz aggregated public channel (top-1
//!    per side per second). That is why the UI's "no public liquidation feed"
//!    caveat (which keys on `exchange === 'Hyperliquid'`) never applies here.

use core_domain::normalized::{Exchange, LiquidationSide, NormalizedEvent};
use network_adapters::adapters::bybit_derivatives::{
    pub_liquidation_to_event, BybitLiquidationData,
};
use rust_decimal::Decimal;
use std::str::FromStr;

fn row(side: &str) -> BybitLiquidationData {
    // `T` is typed `u64` — Bybit sends it as a bare JSON integer on
    // `allLiquidation` (docs type it `number`), so the fixture must mirror the
    // real encoding rather than the string form the adapter used to declare.
    BybitLiquidationData {
        updated_time_ms: Some(1_739_502_302_929),
        symbol: Some("BTCUSDT".into()),
        side: Some(side.into()),
        size: Some("20000".into()),
        price: Some("0.04499".into()),
    }
}

fn side_of(side: &str) -> LiquidationSide {
    let ev = pub_liquidation_to_event("BTC-USDT", &row(side)).expect("event");
    match ev {
        NormalizedEvent::Liquidation(l) => l.side,
        _ => panic!("expected a Liquidation event for side {}", side),
    }
}

#[test]
fn buy_update_is_a_liquidated_long() {
    // The documented mapping: "When you receive a Buy update, this means that
    // a long position has been liquidated".
    assert_eq!(side_of("Buy"), LiquidationSide::Long);
}

#[test]
fn sell_update_is_a_liquidated_short() {
    assert_eq!(side_of("Sell"), LiquidationSide::Short);
}

#[test]
fn side_is_case_insensitive_and_unknown_values_are_dropped() {
    assert_eq!(side_of("buy"), LiquidationSide::Long);
    assert_eq!(side_of("SELL"), LiquidationSide::Short);
    // Anything the venue did not document is dropped rather than guessed —
    // guessing would put a forced close on the wrong side of the heatmap.
    for bad in ["", "Sideways", "unknown"] {
        assert!(
            pub_liquidation_to_event("BTC-USDT", &row(bad)).is_none(),
            "side {:?} must not produce an event",
            bad
        );
    }
    assert!(pub_liquidation_to_event("BTC-USDT", &row("")).is_none());
}

#[test]
fn event_carries_the_venue_stamp_and_workspace_symbol() {
    let ev = pub_liquidation_to_event("BTC-USDT", &row("Buy")).expect("event");
    let NormalizedEvent::Liquidation(l) = ev else {
        panic!("expected Liquidation");
    };
    assert_eq!(l.exchange, Exchange::Bybit);
    // The workspace pair key travels, never the venue wire name — the same
    // contract Bitget and Hyperliquid satisfy.
    assert_eq!(l.symbol, "BTC-USDT");
    assert_eq!(l.timestamp_ms, 1_739_502_302_929);
    assert_eq!(l.price, Decimal::from_str("0.04499").unwrap());
    assert_eq!(l.size, Decimal::from_str("20000").unwrap());
}

#[test]
fn zero_and_negative_payloads_are_dropped() {
    let mut r = row("Buy");
    r.size = Some("0".into());
    assert!(pub_liquidation_to_event("BTC-USDT", &r).is_none());
    r.size = Some("1".into());
    r.price = Some("0".into());
    assert!(pub_liquidation_to_event("BTC-USDT", &r).is_none());
    // A blank numeric field (Bybit omits values for instruments where they do
    // not apply) must not parse as zero and must be dropped, not emitted.
    r.price = Some("".into());
    assert!(pub_liquidation_to_event("BTC-USDT", &r).is_none());
}
