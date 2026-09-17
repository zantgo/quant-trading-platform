//! AC-L2-1 (03-01-03 §6.1): candle close instant is exactly
//! `interval_start + duration_ms` for every completed candle, and
//! `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms`.

use core_domain::normalized::{Exchange, NormalizedTrade, TradeSide};
use market_analyzer::candle_generator::CandleGenerator;
use rust_decimal_macros::dec;

fn trade(ts_ms: u64, price: rust_decimal::Decimal) -> NormalizedTrade {
    NormalizedTrade {
        exchange: Exchange::Hyperliquid,
        symbol: "BTC-USDT".to_string(),
        price,
        size: dec!(1),
        side: TradeSide::Buy,
        timestamp_ms: ts_ms,
        trade_id: format!("t{ts_ms}"),
    }
}

#[test]
fn interval_alignment_formula() {
    // 60 s candle for a trade at 123456 ms aligns to 120000 ms (03-01-03 §3.1).
    let mut generator = CandleGenerator::new("BTC-USDT", 60, Exchange::Hyperliquid);
    let (_, live) = generator.process_trade(&trade(123_456, dec!(100)));
    assert_eq!(live.start_time_ms, 120_000);
    assert_eq!(live.duration_ms, 60_000);
}

#[test]
fn completed_candle_closes_on_integer_epoch_multiple() {
    let duration_secs = 60u64;
    let duration_ms = duration_secs * 1000;
    let mut generator = CandleGenerator::new("BTC-USDT", duration_secs, Exchange::Hyperliquid);

    // Ticks inside interval [60000, 120000).
    generator.process_trade(&trade(61_000, dec!(100)));
    generator.process_trade(&trade(90_500, dec!(101)));
    generator.process_trade(&trade(119_999, dec!(99)));

    // Boundary-crossing tick emits the completed candle.
    let (completed, _) = generator.process_trade(&trade(120_001, dec!(102)));
    let completed = completed.expect("interval crossing must emit completed candle");

    assert_eq!(completed.start_time_ms % duration_ms, 0, "open aligned");
    let close_instant = completed.start_time_ms + completed.duration_ms;
    assert_eq!(
        close_instant, 120_000,
        "close = interval_start + duration_ms"
    );
    assert_eq!(close_instant % duration_ms, 0, "close on epoch multiple");
}

#[test]
fn utc_boundary_map_for_all_default_tiers() {
    // 1m / 3m / 5m / 15m all close on exact epoch
    // multiples of their duration (03-01-03 §3.1 UTC boundary map).
    for duration_secs in [60u64, 180, 300, 900] {
        let duration_ms = duration_secs * 1000;
        let mut generator = CandleGenerator::new("BTC-USDT", duration_secs, Exchange::Hyperliquid);
        let t0 = 1_000_003_337u64; // arbitrary unaligned ms timestamp
        generator.process_trade(&trade(t0, dec!(100)));
        let (completed, _) = generator.process_trade(&trade(t0 + duration_ms, dec!(101)));
        let completed = completed.unwrap();
        assert_eq!(
            (completed.start_time_ms + completed.duration_ms) % duration_ms,
            0,
            "{duration_secs}s tier must close on an epoch multiple"
        );
    }
}

#[test]
fn identical_trade_sequences_yield_identical_candles() {
    // Determinism guarantee (03-01-03 §6).
    let ticks: Vec<NormalizedTrade> = (0..500u64)
        .map(|i| trade(i * 137, dec!(100) + rust_decimal::Decimal::from(i % 7)))
        .collect();

    let run = |ticks: &[NormalizedTrade]| {
        let mut generator = CandleGenerator::new("BTC-USDT", 60, Exchange::Hyperliquid);
        let mut completed = Vec::new();
        for t in ticks {
            if let (Some(c), _) = generator.process_trade(t) {
                completed.push(c);
            }
        }
        completed
    };

    let a = run(&ticks);
    let b = run(&ticks);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.start_time_ms, y.start_time_ms);
        assert_eq!(x.open, y.open);
        assert_eq!(x.high, y.high);
        assert_eq!(x.low, y.low);
        assert_eq!(x.close, y.close);
        assert_eq!(x.volume, y.volume);
        assert_eq!(x.trades_count, y.trades_count);
    }
}
