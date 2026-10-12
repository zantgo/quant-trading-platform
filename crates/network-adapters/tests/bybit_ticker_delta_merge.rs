//! Bybit V5 `tickers` snapshot+delta semantics and funding-interval scaling.
//!
//! These are the two traps that make a straight port of the Bitget ticker
//! handler silently wrong on Bybit. Both were measured against the live venue
//! on 2026-10 and both are pinned here.
//!
//! ## Trap 1 — `tickers` sends partial deltas, not full snapshots
//!
//! Measured on `BTCUSDT` over a 20-second window: **132 frames = 1 snapshot +
//! 131 deltas**, each delta carrying only the fields that changed. Reading any
//! single frame in isolation yields `None` for `markPrice`, `openInterest`,
//! `fundingRate` and most of the rest.
//!
//! A naive port of the Bitget handler (which receives a complete `ticker` push
//! every time) would emit a stream of `MarkPrice` / `OpenInterest` /
//! `FundingRate` events with **missing fields** — corrupting the mark-price,
//! OI and funding series that the 52-indicator set, the L2.5 cluster estimator
//! and the L5 cascade risk all read. Nothing crashes; the numbers are just
//! wrong. The fix is `TickerState`, which folds frames into a retained
//! snapshot and reports only what moved.
//!
//! ## Trap 2 — the funding interval is per-symbol
//!
//! `fundingRate` is quoted for the symbol's own `fundingIntervalHour`, which
//! is **not** always 8. Across 904 listed linear instruments: 444 at 8h,
//! **419 at 4h**, 1 at 1h, 40 with no interval at all.
//!
//! Every downstream consumer assumes per-8h semantics — the funding tracker
//! annualizes with x1095 (`funding.rs`: "365 days x 3 periods/day"), the
//! L2.5 cluster estimator reads the same series, and `FUNDING_EXTREME` is
//! thresholded at `funding_extreme_pct` (default 0.0005) per 8h. Reading a 4h
//! rate raw would **double** it and false-fire the signal.

use core_domain::normalized::NormalizedEvent;
use network_adapters::adapters::bybit_derivatives::{
    normalize_funding_to_8h, BybitTickerFrame, TickerState,
};
use rust_decimal::Decimal;
use std::str::FromStr;

fn frame(json: &str) -> BybitTickerFrame {
    serde_json::from_str(json).expect("valid BybitTickerFrame")
}

fn snapshot() -> &'static str {
    r#"{"symbol":"BTCUSDT","markPrice":"83909.40","indexPrice":"83947.05",
        "prevPrice24h":"83005.90","openInterest":"55221.716",
        "openInterestValue":"4633621056.53","fundingRate":"-0.00004138",
        "fundingIntervalHour":"8","bid1Price":"83907.90","ask1Price":"83908.00"}"#
}

// ── Trap 1: partial deltas ────────────────────────────────────────────────

#[test]
fn partial_delta_does_not_erase_the_retained_snapshot() {
    let mut st = TickerState::default();

    assert!(st.apply(&frame(snapshot())).any());

    // A typical delta: bid/ask only. Nothing the platform tracks moved.
    let changes = st.apply(&frame(r#"{"bid1Price":"83910.00","ask1Size":"3.5"}"#));
    assert!(
        !changes.any(),
        "a bid/ask-only delta must not report a tracked change"
    );
    // THE regression: the retained values must survive a partial frame. If the
    // adapter replaced rather than merged, these would be None here and every
    // downstream mark/OI/funding read would see a hole.
    assert_eq!(
        st.mark_price(),
        Some(Decimal::from_str("83909.40").unwrap())
    );
    assert_eq!(
        st.index_price(),
        Some(Decimal::from_str("83947.05").unwrap())
    );
    assert_eq!(
        st.open_interest_usd(),
        Some(Decimal::from_str("4633621056.53").unwrap())
    );
    assert_eq!(
        st.funding_rate_8h(),
        Some(Decimal::from_str("-0.00004138").unwrap())
    );
}

#[test]
fn a_snapshot_then_ninety_nine_deltas_yields_one_event_per_moving_field() {
    // Replays the measured ratio (1 snapshot : ~131 deltas) and asserts the
    // adapter emits only for fields that genuinely moved — never one partial
    // event per frame.
    let mut st = TickerState::default();
    st.apply(&frame(snapshot()));

    let mut emitted = 0usize;
    let mut quiet = 0usize;
    for i in 0..100u32 {
        // Real deltas alternate between a moving mark and a bid/ask-only
        // update. The bid/ask frames are the ones that must stay silent.
        let delta = if i % 3 == 1 {
            format!(r#"{{"bid1Price":"{}"}}"#, 83907.0 + i as f64)
        } else {
            format!(r#"{{"markPrice":"{}"}}"#, 83910.0 + i as f64)
        };
        let changes = st.apply(&frame(&delta));
        let n = st.events_for("BTC-USDT", changes, 1_700_000_000_000).len();
        if i % 3 == 1 {
            assert_eq!(n, 0, "a bid-only delta must emit nothing (i={})", i);
            quiet += 1;
        } else {
            assert_eq!(n, 1, "a changed mark must emit exactly one event (i={})", i);
            emitted += 1;
        }
    }
    assert_eq!(emitted + quiet, 100);
    assert!(
        quiet > 0,
        "the silent-frame path must actually be exercised"
    );
}

#[test]
fn identical_repeats_emit_nothing() {
    let mut st = TickerState::default();
    assert!(st.apply(&frame(snapshot())).any());
    assert!(
        !st.apply(&frame(snapshot())).any(),
        "a byte-identical repeat must not re-emit"
    );
}

#[test]
fn events_carry_the_workspace_symbol_not_the_venue_name() {
    let mut st = TickerState::default();
    let changes = st.apply(&frame(snapshot()));
    let events = st.events_for("BTC-USDT", changes, 1_700_000_000_000);
    assert!(!events.is_empty());
    for ev in &events {
        let sym = match ev {
            NormalizedEvent::AssetContext(a) => &a.symbol,
            NormalizedEvent::MarkPrice(m) => &m.symbol,
            NormalizedEvent::OpenInterest(o) => &o.symbol,
            NormalizedEvent::FundingRate(f) => &f.symbol,
            other => panic!("unexpected event {:?}", other),
        };
        assert_eq!(sym, "BTC-USDT", "must use the workspace pair key");
    }
}

// ── Trap 2: per-symbol funding interval ───────────────────────────────────

#[test]
fn four_hour_symbols_are_rescaled_to_the_per_8h_convention() {
    // 419 of 904 listed instruments fund every 4h. Reading raw would double
    // the rate against the x1095 annualizer and the 0.0005 extreme threshold.
    assert_eq!(
        normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(4)),
        Some(Decimal::from_str("0.0002").unwrap())
    );
    // 8h (the majority) is a pass-through, and 1h scales up by 8.
    assert_eq!(
        normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(8)),
        Some(Decimal::from_str("0.0001").unwrap())
    );
    assert_eq!(
        normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(1)),
        Some(Decimal::from_str("0.0008").unwrap())
    );
}

#[test]
fn a_missing_interval_is_dropped_rather_than_guessed() {
    // 40 instruments (dated futures / settlements) publish no interval. A
    // default of "8" here would be a silent fabrication.
    assert_eq!(
        normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), None),
        None
    );
    assert_eq!(
        normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(0)),
        None
    );
}

#[test]
fn a_four_hour_symbol_rescales_end_to_end_through_the_state() {
    // XAUUSDT is a real 4h-funding commodity perp; this replays its actual
    // wire payload.
    let mut st = TickerState::default();
    let changes = st.apply(&frame(
        r#"{"symbol":"XAUUSDT","markPrice":"4210.29","indexPrice":"4209.27",
            "prevPrice24h":"4197.39","openInterestValue":"119262131.30",
            "fundingRate":"0.00005","fundingIntervalHour":"4"}"#,
    ));
    assert!(changes.funding_rate);
    // Raw is 0.00005 per 4h -> 0.0001 per 8h.
    assert_eq!(
        st.funding_rate_8h(),
        Some(Decimal::from_str("0.0001").unwrap()),
        "a 4h rate must be doubled into the per-8h series"
    );
    // Mark / OI / prev-day ride along unchanged.
    assert!(changes.mark_price && changes.open_interest && changes.prev_day_price);
    assert_eq!(
        st.open_interest_usd(),
        Some(Decimal::from_str("119262131.30").unwrap())
    );
}

#[test]
fn blank_funding_fields_do_not_zero_the_series() {
    let mut st = TickerState::default();
    st.apply(&frame(snapshot()));
    // A delta with empty funding fields (mid-settlement) must leave the last
    // known rate intact, not overwrite it with zero.
    st.apply(&frame(r#"{"fundingRate":"","fundingIntervalHour":""}"#));
    assert_eq!(
        st.funding_rate_8h(),
        Some(Decimal::from_str("-0.00004138").unwrap())
    );
}

#[test]
fn open_interest_prefers_the_usd_field_and_falls_back_to_base_times_mark() {
    // Bybit publishes openInterestValue already in USD — no mark multiply
    // needed (contrast Bitget's base-unit holdingAmount).
    let mut st = TickerState::default();
    st.apply(&frame(
        r#"{"markPrice":"100","openInterest":"12.5","openInterestValue":"1250"}"#,
    ));
    assert_eq!(
        st.open_interest_usd(),
        Some(Decimal::from_str("1250").unwrap())
    );

    // If only the base-unit field is present, the state derives USD itself.
    let mut st2 = TickerState::default();
    st2.apply(&frame(r#"{"markPrice":"100","openInterest":"12.5"}"#));
    assert_eq!(
        st2.open_interest_usd(),
        Some(Decimal::from_str("1250").unwrap())
    );
}
