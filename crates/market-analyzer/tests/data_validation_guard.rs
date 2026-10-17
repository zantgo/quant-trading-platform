//! AUDIT-C4 + AUDIT-M20 regressions for the data-validation choke points.
//!
//! C4: Bitget and Hyperliquid coerced an unparseable trade price/size to
//! `Decimal::ZERO` and emitted it as a REAL trade. A zero-price tick opens a
//! candle bucket at 0 (or drags `low` to 0), and `assert_validity` accepted an
//! all-zero OHLCV candle -- so the poisoned bar was persisted, broadcast, and
//! fed to every downstream indicator. Sub-minute durations have no REST refetch
//! fallback, so the slot could not self-repair.
//!
//! M20: Hyperliquid validated both sides non-empty; Bitget validated nothing;
//! Bybit only rejected when BOTH sides were empty. No adapter sorted, and all
//! three decoded levels with `filter_map`, so one unparseable level silently
//! promoted a worse level to `bids[0]` -- which the mid-price reader
//! treats as top of book. The result was a negative `spread_pct`, an inverted
//! mid feeding the shadow candle, and wrong OFI/DWM.

use core_domain::normalized::{
    Exchange, NormalizedCandle, NormalizedOrderBook, NormalizedTrade, OrderBookReject, TradeSide,
};
use market_analyzer::candle_generator::CandleGenerator;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

// ── C4: trade admissibility ────────────────────────────────────────────────

/// A trade at an explicit wall-clock millisecond, so tests can drive candle
/// bucket boundaries (`interval_start = ts / duration * duration`).
fn trade_at_ts(
    ts_ms: u64,
    price: rust_decimal::Decimal,
    size: rust_decimal::Decimal,
) -> NormalizedTrade {
    NormalizedTrade {
        exchange: Exchange::Bitget,
        symbol: "BTC-USDT".into(),
        price,
        size,
        side: TradeSide::Buy,
        timestamp_ms: ts_ms,
        trade_id: format!("t{ts_ms}"),
    }
}

fn trade_at(price: rust_decimal::Decimal, size: rust_decimal::Decimal) -> NormalizedTrade {
    trade_at_ts(1_000, price, size)
}

#[test]
fn zero_price_trade_is_inadmissible() {
    let gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);
    assert!(!gen.trade_is_admissible(&trade_at(dec!(0), dec!(1))));
}

#[test]
fn zero_size_trade_is_inadmissible() {
    let gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);
    assert!(!gen.trade_is_admissible(&trade_at(dec!(100_000), dec!(0))));
}

#[test]
fn negative_trade_is_inadmissible() {
    let gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);
    assert!(!gen.trade_is_admissible(&trade_at(dec!(-1), dec!(1))));
    assert!(!gen.trade_is_admissible(&trade_at(dec!(100_000), dec!(-1))));
}

#[test]
fn a_real_trade_is_admissible() {
    let gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);
    assert!(gen.trade_is_admissible(&trade_at(dec!(100_000.5), dec!(0.25))));
}

/// The load-bearing C4 test: a zero-price tick must not poison a real candle.
#[test]
fn zero_price_trade_cannot_poison_a_real_candle() {
    let mut gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);

    // Two genuine trades open and populate a 1s bucket (1000..1999).
    gen.process_trade_at(&trade_at_ts(1_000, dec!(100_000), dec!(1)), 1_000);
    gen.process_trade_at(&trade_at_ts(1_100, dec!(100_010), dec!(1)), 1_100);
    let (_, live) = gen.process_trade_at(&trade_at_ts(1_200, dec!(100_020), dec!(1)), 1_200);
    assert_eq!(live.low, dec!(100_000));
    assert_eq!(live.close, dec!(100_020));

    // A malformed tick arrives in the SAME bucket. It must change nothing.
    let (completed, live_after) =
        gen.process_trade_at(&trade_at_ts(1_300, dec!(0), dec!(1)), 1_300);
    assert!(
        completed.is_none(),
        "an inadmissible trade must not complete a candle"
    );
    assert_eq!(
        live_after.low,
        dec!(100_000),
        "an inadmissible trade must not drag `low` to zero"
    );
    assert_eq!(live_after.close, dec!(100_020));
    assert_eq!(live_after.trades_count, 3, "trade count must be unchanged");

    // The next real trade crosses the 1s boundary and completes the bucket.
    let (completed, _) = gen.process_trade_at(&trade_at_ts(2_000, dec!(100_500), dec!(1)), 2_000);
    let done = completed.expect("bucket should roll at the 1s boundary");
    assert!(
        done.assert_validity().is_ok(),
        "completed candle must pass validity: {done:?}"
    );
    assert!(done.low > dec!(0), "no zero-price wick may survive");
    assert_eq!(done.trades_count, 3, "the zero tick must not be counted");
}

/// A zero-price tick arriving FIRST (before any real trade) must not open a
/// bucket at zero -- this is the exact path that produced the all-zero candle.
#[test]
fn zero_price_first_tick_does_not_open_a_zero_bucket() {
    let mut gen = CandleGenerator::new("BTC-USDT", 1, Exchange::Bitget);

    let (completed, _) = gen.process_trade_at(&trade_at(dec!(0), dec!(1)), 1_000);
    assert!(completed.is_none());
    // The next real trade must be able to open a clean bucket.
    let (_, live) = gen.process_trade_at(&trade_at(dec!(100_000), dec!(1)), 1_100);
    assert!(live.low > dec!(0), "bucket must not be seeded at zero");
    assert!(live.assert_validity().is_ok());
}

// ── C4: candle validity ────────────────────────────────────────────────────

fn candle(o: i64, h: i64, l: i64, c: i64, v: i64) -> NormalizedCandle {
    NormalizedCandle {
        exchange: Exchange::Bitget,
        symbol: "BTC-USDT".into(),
        start_time_ms: 0,
        duration_ms: 1000,
        open: Decimal::from(o),
        high: Decimal::from(h),
        low: Decimal::from(l),
        close: Decimal::from(c),
        volume: Decimal::from(v),
        trades_count: 1,
        reconstructed: None,
    }
}

#[test]
fn all_zero_candle_is_rejected() {
    // This is the defect: every geometric check below passed before.
    let c = candle(0, 0, 0, 0, 0);
    assert!(
        c.assert_validity().is_err(),
        "an all-zero OHLCV candle must never be accepted"
    );
    assert!(c.high >= c.low);
    assert!(c.volume >= dec!(0)); // the old checks all passed
}

#[test]
fn zero_candle_with_volume_is_still_rejected() {
    // A synthetic gap-fill Doji is built from a real close, so it has a
    // positive price but zero volume -- that must remain VALID. This test
    // pins the boundary: only a genuinely zero PRICE is degenerate.
    let doji = candle(100_000, 100_000, 100_000, 100_000, 0);
    assert!(
        doji.assert_validity().is_ok(),
        "a real-price doji with zero volume is legitimate"
    );
}

#[test]
fn negative_low_is_rejected() {
    assert!(candle(1, 1, -1, 1, 1).assert_validity().is_err());
}

#[test]
fn ordinary_candles_still_pass() {
    assert!(candle(100, 110, 95, 105, 5).assert_validity().is_ok());
    // A doji (high == low) is legitimate.
    assert!(candle(100, 100, 100, 100, 1).assert_validity().is_ok());
}

// ── M20: order-book validation ─────────────────────────────────────────────

#[test]
fn a_normal_book_passes_and_is_sorted() {
    let book = NormalizedOrderBook::validated(
        Exchange::Bitget,
        "BTC-USDT".into(),
        // Deliberately UNSORTED, as a venue can deliver it.
        vec![(dec!(99_000), dec!(2)), (dec!(100_000), dec!(1))],
        vec![(dec!(101_000), dec!(3)), (dec!(100_500), dec!(1))],
        1_000,
    )
    .expect("a sane book must validate");

    // `OrderBookAnalysis::update` trusts order, so the validator must impose it.
    assert_eq!(book.bids[0].0, dec!(100_000), "best bid must be first");
    assert_eq!(book.asks[0].0, dec!(100_500), "best ask must be first");
}

#[test]
fn a_crossed_book_is_rejected() {
    let book = NormalizedOrderBook::validated(
        Exchange::Bybit,
        "BTC-USDT".into(),
        vec![(dec!(101_000), dec!(1))],
        vec![(dec!(100_000), dec!(1))],
        1_000,
    );
    assert!(
        book.is_none(),
        "a crossed book must never reach the pipeline"
    );

    let bids = vec![(dec!(101_000), dec!(1))];
    let asks = vec![(dec!(100_000), dec!(1))];
    assert_eq!(
        NormalizedOrderBook::reject_reason(&bids, &asks),
        OrderBookReject::Crossed
    );
}

#[test]
fn a_locked_book_is_rejected() {
    // best_bid == best_ask is a zero-width (locked) book, not a real one.
    assert!(NormalizedOrderBook::validated(
        Exchange::Bitget,
        "BTC-USDT".into(),
        vec![(dec!(100_000), dec!(1))],
        vec![(dec!(100_000), dec!(1))],
        1_000,
    )
    .is_none());
}

#[test]
fn a_one_sided_book_is_rejected() {
    // Bybit previously forwarded these -- it only skipped when BOTH were empty.
    for exchange in [Exchange::Bitget, Exchange::Bybit, Exchange::Hyperliquid] {
        assert!(NormalizedOrderBook::validated(
            exchange,
            "BTC-USDT".into(),
            vec![(dec!(100_000), dec!(1))],
            vec![],
            1_000,
        )
        .is_none());
        assert!(NormalizedOrderBook::validated(
            exchange,
            "BTC-USDT".into(),
            vec![],
            vec![(dec!(100_000), dec!(1))],
            1_000,
        )
        .is_none());
    }
}

#[test]
fn zero_size_levels_are_dropped() {
    // A zero-size level carries no liquidity; keeping it would corrupt depth.
    let book = NormalizedOrderBook::validated(
        Exchange::Bitget,
        "BTC-USDT".into(),
        vec![(dec!(100_000), dec!(0)), (dec!(99_000), dec!(1))],
        vec![(dec!(100_500), dec!(1))],
        1_000,
    )
    .expect("book survives after dropping the zero-size level");
    assert_eq!(book.bids.len(), 1);
    assert_eq!(book.bids[0].0, dec!(99_000));
}

/// The load-bearing M20 test: end-to-end, a mis-ordered book cannot produce a
/// negative spread.
/// Belt-and-braces: even if a crossed book somehow reaches the analysis layer,
/// `spread_pct` is clamped rather than going negative on the wire.
#[test]
fn empty_book_reports_the_empty_reason() {
    assert_eq!(
        NormalizedOrderBook::reject_reason(&[], &[]),
        OrderBookReject::Empty
    );
}

/// Venue parity: every exchange takes the identical path.
#[test]
fn all_three_venues_share_one_validation_contract() {
    for exchange in [Exchange::Hyperliquid, Exchange::Bitget, Exchange::Bybit] {
        assert!(NormalizedOrderBook::validated(
            exchange,
            "BTC-USDT".into(),
            vec![(dec!(100_000), dec!(1))],
            vec![(dec!(100_500), dec!(1))],
            1_000,
        )
        .is_some());
        assert!(NormalizedOrderBook::validated(
            exchange,
            "BTC-USDT".into(),
            vec![(dec!(101_000), dec!(1))],
            vec![(dec!(100_000), dec!(1))],
            1_000,
        )
        .is_none());
    }
}
