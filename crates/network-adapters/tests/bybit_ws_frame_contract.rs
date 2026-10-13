//! Bybit V5 WebSocket frame contract — decoded from frames captured LIVE.
//!
//! ## Why this test exists
//!
//! v11.13 shipped Bybit support with a fully green suite and **zero live
//! trades**. The adapter declared the frame timestamps as `Option<String>`
//! while Bybit sends them as bare JSON **integers**:
//!
//! ```text
//! "T" => number 1791857245203     // publicTrade
//! ```
//!
//! `serde_json::from_value` is all-or-nothing per frame, so that single type
//! mismatch discarded the **entire** payload, and the `Err(_) => continue`
//! arm swallowed it without a log line. Because sub-minute durations have no
//! REST fallback (HFP-03 bypasses them by design), `1s`/`3s`/`5s`/`15s`/`30s`
//! received no events at all and produced no candles, while the `>=60s` half
//! of the ladder kept rendering stale REST-warmed data — which is exactly how
//! the defect presented in the dashboard.
//!
//! Every pre-existing Bybit test fed **hand-built structs** into the decoder
//! or exercised REST only, so none of them ever saw the wire encoding that
//! broke. That is the gap this file closes: the fixtures below are VERBATIM
//! payloads captured from `wss://stream.bybit.com/v5/public/linear`, and the
//! assertions run the real decode path.
//!
//! Regenerate the fixtures by re-running the capture described in the module
//! docs of `bybit.rs` if Bybit ever changes its schema.

use core_domain::normalized::{Exchange, LiquidationSide, NormalizedEvent, TradeSide};
use network_adapters::adapters::bybit::{de_ts_ms, BybitBook, BybitTrade};
use network_adapters::adapters::bybit_derivatives::{
    pub_liquidation_to_event, BybitLiquidationData,
};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Verbatim `publicTrade.BTCUSDT` payload (captured 2026-10). `T` is an
/// integer; `p` / `v` / `S` / `i` are strings — the mixed encoding that broke.
const LIVE_TRADE_PAYLOAD: &str = r#"[{"T":1791857245203,"s":"BTCUSDT","S":"Sell","v":"0.184","p":"82429.10","L":"ZeroMinusTick","i":"829d0cef-93fd-5bf6-88c3-0dbd10694dff","BT":false,"RPI":false,"seq":824664710425}]"#;

/// Verbatim `orderbook.50.BTCUSDT` payload. Note there is NO `ts` inside
/// `data` (measured keys: `s`, `b`, `a`, `u`, `seq`) — the stamp lives on the
/// envelope, so the adapter legitimately falls back to local arrival time.
const LIVE_BOOK_PAYLOAD: &str = r#"{"s":"BTCUSDT","b":[["82429.10","2.900"],["82429.00","0.244"],["82428.00","1.100"]],"a":[["82429.20","3.979"],["82429.30","0.003"],["82430.00","2.500"]],"u":232016316,"seq":824664704802}"#;

/// Verbatim `allLiquidation` payload, from Bybit's own documented response
/// example (`T` typed `number` there too).
const LIVE_LIQUIDATION_PAYLOAD: &str =
    r#"[{"T":1739502302929,"s":"ROSEUSDT","S":"Sell","v":"20000","p":"0.04499"}]"#;

fn trades(payload: &str) -> Vec<BybitTrade> {
    serde_json::from_str(payload).expect("publicTrade payload must decode")
}

// =============================================================================
// The regression itself
// =============================================================================

#[test]
fn live_trade_frame_decodes_and_carries_the_integer_timestamp() {
    let decoded = trades(LIVE_TRADE_PAYLOAD);
    assert_eq!(
        decoded.len(),
        1,
        "the captured frame holds exactly one trade"
    );
    let t = &decoded[0];
    assert_eq!(
        t.ts_ms,
        Some(1_791_857_245_203),
        "integer T must survive the decode"
    );
    assert_eq!(t.price.as_deref(), Some("82429.10"));
    assert_eq!(t.size.as_deref(), Some("0.184"));
    assert_eq!(t.side.as_deref(), Some("Sell"));
    assert_eq!(
        t.trade_id.as_deref(),
        Some("829d0cef-93fd-5bf6-88c3-0dbd10694dff")
    );
}

/// The behavioural guard: a verbatim frame from the live venue must decode.
///
/// This is written to fail at RUNTIME (a serde panic), not at compile time, so
/// that it still guards the bug if the field type is ever changed again. Under
/// the v11.13 declaration (`T: Option<String>`) this test fails with
/// `invalid type: integer, expected a string` — which is precisely how the
/// entire trade stream vanished without a single log line.
#[test]
fn a_verbatim_live_frame_never_fails_to_decode() {
    assert!(
        serde_json::from_str::<Vec<BybitTrade>>(LIVE_TRADE_PAYLOAD).is_ok(),
        "a real captured publicTrade frame must decode"
    );
    assert!(
        serde_json::from_str::<Vec<BybitLiquidationData>>(LIVE_LIQUIDATION_PAYLOAD).is_ok(),
        "a real captured allLiquidation frame must decode"
    );
    assert!(
        serde_json::from_str::<BybitBook>(LIVE_BOOK_PAYLOAD).is_ok(),
        "a real captured orderbook frame must decode"
    );
}

#[test]
fn a_live_trade_becomes_a_normalized_event() {
    // The full decode path the adapter runs: wire -> struct -> NormalizedEvent.
    // Before the fix this returned nothing at all for every frame.
    let t = trades(LIVE_TRADE_PAYLOAD).remove(0);
    let price = Decimal::from_str(t.price.as_deref().unwrap()).expect("price");
    let size = Decimal::from_str(t.size.as_deref().unwrap()).expect("size");
    assert!(price > Decimal::ZERO && size > Decimal::ZERO);
    let side = match t.side.as_deref() {
        Some(s) if s.eq_ignore_ascii_case("buy") => TradeSide::Buy,
        _ => TradeSide::Sell,
    };
    assert_eq!(side, TradeSide::Sell);
    // The venue stamp is authoritative — no local-clock fallback when present.
    assert_eq!(t.ts_ms.filter(|ms| *ms > 0).unwrap(), 1_791_857_245_203);
}

#[test]
fn live_liquidation_frame_decodes_and_emits_an_event() {
    let rows: Vec<BybitLiquidationData> =
        serde_json::from_str(LIVE_LIQUIDATION_PAYLOAD).expect("allLiquidation payload must decode");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].updated_time_ms, Some(1_739_502_302_929));

    let ev = pub_liquidation_to_event("BTC-USDT", &rows[0]).expect("event");
    let NormalizedEvent::Liquidation(l) = ev else {
        panic!("expected a Liquidation event");
    };
    assert_eq!(l.exchange, Exchange::Bybit);
    assert_eq!(l.symbol, "BTC-USDT");
    assert_eq!(l.timestamp_ms, 1_739_502_302_929);
    assert_eq!(
        l.side,
        LiquidationSide::Short,
        "S: Sell == liquidated SHORT"
    );
    assert_eq!(l.price, Decimal::from_str("0.04499").unwrap());
}

#[test]
fn buy_side_liquidation_is_a_liquidated_long() {
    // The documented inversion (contrast Bitget's `fill` channel) must survive
    // the timestamp change.
    let payload = r#"[{"T":1739502302929,"s":"ROSEUSDT","S":"Buy","v":"20000","p":"0.04499"}]"#;
    let rows: Vec<BybitLiquidationData> = serde_json::from_str(payload).expect("decode");
    let NormalizedEvent::Liquidation(l) =
        pub_liquidation_to_event("BTC-USDT", &rows[0]).expect("event")
    else {
        panic!("expected a Liquidation event");
    };
    assert_eq!(l.side, LiquidationSide::Long);
}

#[test]
fn live_book_frame_decodes_with_levels_and_no_timestamp() {
    let book: BybitBook =
        serde_json::from_str(LIVE_BOOK_PAYLOAD).expect("book payload must decode");
    assert_eq!(book.bids.len(), 3);
    assert_eq!(book.asks.len(), 3);
    assert_eq!(book.bids[0], ["82429.10".to_string(), "2.900".to_string()]);
    assert_eq!(book.asks[0], ["82429.20".to_string(), "3.979".to_string()]);
    // Measured: no `ts` inside the book data, so the adapter's local-clock
    // fallback is the correct behaviour rather than a lost venue stamp.
    assert_eq!(book.ts_ms, None);
}

// =============================================================================
// Tolerance — the deserializer must never turn one odd encoding into total loss
// =============================================================================

#[derive(Debug, serde::Deserialize)]
struct TsProbe {
    #[serde(default, deserialize_with = "de_ts_ms")]
    t: Option<u64>,
}

fn ts_of(raw: &str) -> Option<u64> {
    serde_json::from_str::<TsProbe>(raw)
        .expect("timestamp must decode")
        .t
}

#[test]
fn timestamp_accepts_every_encoding_the_venue_uses() {
    // The live integer form (the case that shipped broken).
    assert_eq!(ts_of(r#"{"t":1791857245203}"#), Some(1_791_857_245_203));
    // A quoted string — the REST endpoints quote numbers, and a future venue
    // re-encoding must not reintroduce a silent total drop.
    assert_eq!(ts_of(r#"{"t":"1791857245203"}"#), Some(1_791_857_245_203));
    // Absent / null / empty all mean "no stamp", never the epoch.
    assert_eq!(ts_of(r#"{}"#), None);
    assert_eq!(ts_of(r#"{"t":null}"#), None);
    assert_eq!(ts_of(r#"{"t":""}"#), None);
}

#[test]
fn a_trade_with_an_unusable_timestamp_still_decodes_and_falls_back() {
    // Robustness: a missing stamp must cost the STAMP, never the trade.
    // Losing the whole trade is the v11.13 failure mode.
    let payload = r#"[{"T":null,"s":"BTCUSDT","S":"Buy","v":"1.5","p":"82429.10","i":"abc"}]"#;
    let decoded = trades(payload);
    assert_eq!(decoded.len(), 1, "trade must survive a null timestamp");
    assert_eq!(decoded[0].ts_ms, None);
    assert_eq!(decoded[0].price.as_deref(), Some("82429.10"));
}

#[test]
fn a_multi_trade_batch_decodes_completely() {
    // Bybit batches up to 1024 trades per message; every element must land.
    let payload = r#"[{"T":1791857245203,"s":"BTCUSDT","S":"Buy","v":"0.1","p":"100.5","i":"a"},
                     {"T":1791857245204,"s":"BTCUSDT","S":"Sell","v":"0.2","p":"100.4","i":"b"},
                     {"T":1791857245205,"s":"BTCUSDT","S":"Buy","v":"0.3","p":"100.6","i":"c"}]"#;
    let decoded = trades(payload);
    assert_eq!(decoded.len(), 3, "one bad element must not drop the batch");
    assert_eq!(decoded[1].ts_ms, Some(1_791_857_245_204));
    assert_eq!(decoded[2].trade_id.as_deref(), Some("c"));
}

#[test]
fn ids_are_unique_per_trade_so_the_venue_key_survives() {
    // The v11.13 docs claimed "100% unique trade ids" from Bybit. Pin it
    // against real frame shapes rather than a synthetic loop.
    let payload = r#"[{"T":1791857245203,"s":"BTCUSDT","S":"Buy","v":"0.1","p":"100.5","i":"829d0cef-93fd-5bf6-88c3-0dbd10694dff"},
                     {"T":1791857245204,"s":"BTCUSDT","S":"Sell","v":"0.1","p":"100.4","i":"1a2b3c4d-93fd-5bf6-88c3-0dbd10694dff"}]"#;
    let decoded = trades(payload);
    let mut ids: Vec<&str> = decoded
        .iter()
        .filter_map(|t| t.trade_id.as_deref())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        2,
        "Bybit publishes a genuine unique id per trade"
    );
}
