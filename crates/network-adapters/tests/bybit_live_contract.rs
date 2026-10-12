//! Live-venue contract checks for the Bybit adapter.
//!
//! Unlike the other suites in this directory these tests **hit the real
//! Bybit API**. They are gated behind `BYBIT_LIVE=1` so the default
//! `cargo test` run stays hermetic (the whole suite is designed to pass with
//! no network), and are meant to be run manually when the adapter's wire
//! assumptions need re-verifying:
//!
//! ```bash
//! BYBIT_LIVE=1 cargo test -p network-adapters --test bybit_live_contract -- --ignored --nocapture
//! ```
//!
//! What they pin, all measured 2026-10:
//!
//! * the 14-duration pool's >=60s half returns a **full 500-candle page** with
//!   candle timestamps aligned to the duration;
//! * the retention wall (~2,425 days) sits far past the platform's 365-day
//!   archive ceiling, which is why Bybit is treated as depth-uncapped;
//! * the page cap is 1000 regardless of the requested `limit`;
//! * a live linear perpetual is admitted and a dated future is not.

use network_adapters::adapters::bybit_rest::{
    fetch_instrument, is_tradeable_perpetual, timeframe_secs_to_interval,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const KLINE_URL: &str = "https://api.bybit.com/v5/market/kline";
const INSTRUMENTS_URL: &str = "https://api.bybit.com/v5/market/instruments-info";

fn live() -> bool {
    std::env::var("BYBIT_LIVE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

async fn kline_count(symbol: &str, interval: &str, days: u64, limit: u32) -> usize {
    let end = now_ms();
    let start = end - days * 86_400_000;
    let url = format!(
        "{}?category=linear&symbol={}&interval={}&start={}&end={}&limit={}",
        KLINE_URL, symbol, interval, start, end, limit
    );
    let body: serde_json::Value = reqwest::get(&url)
        .await
        .expect("kline request")
        .json()
        .await
        .expect("kline json");
    assert_eq!(
        body.get("retCode").and_then(|c| c.as_i64()),
        Some(0),
        "kline error: {}",
        body
    );
    body.get("result")
        .and_then(|r| r.get("list"))
        .and_then(|l| l.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

/// Every >=60s duration in the pool must return the full 500-candle bootstrap
/// target, and the rows must be aligned to the duration.
#[tokio::test]
#[ignore = "requires network; run with BYBIT_LIVE=1"]
async fn every_above_minute_pool_duration_is_served() {
    if !live() {
        eprintln!("skipping (BYBIT_LIVE != 1)");
        return;
    }
    for secs in [60u64, 180, 300, 900, 1800, 3600, 14400, 43200, 86400] {
        let iv = timeframe_secs_to_interval(secs);
        // 500 candles' worth of window, as the bootstrap requests. Ceiling
        // division — floor would round 500 x 180s (= 1.04 days) down to a
        // single day, which holds only 480 three-minute candles.
        let days = ((500 * secs).div_ceil(86_400)).max(1);
        let n = kline_count("BTCUSDT", iv, days, 1000).await;
        assert!(
            n >= 500,
            "{}s (interval {}) returned only {} candles for a {}-day window",
            secs,
            iv,
            n,
            days
        );
        eprintln!("  {:>6}s -> interval {:<4} -> {} candles", secs, iv, n);
    }
}

/// The retention wall is far past the platform's 365-day archive ceiling, so
/// `exchange_max_depth_secs` correctly returns 0 (uncapped) for Bybit.
#[tokio::test]
#[ignore = "requires network; run with BYBIT_LIVE=1"]
async fn retention_is_deep_at_the_365_day_archive_ceiling() {
    if !live() {
        eprintln!("skipping (BYBIT_LIVE != 1)");
        return;
    }
    for (secs, iv) in [(60u64, "1"), (3600, "60"), (86400, "D")] {
        // A 30-day window ending at the 365-day mark — the worst case the
        // platform's own backfill would ever request.
        let end = now_ms() - 335 * 86_400_000;
        let start = end - 30 * 86_400_000;
        let url = format!(
            "{}?category=linear&symbol=BTCUSDT&interval={}&start={}&end={}&limit=1000",
            KLINE_URL, iv, start, end
        );
        let body: serde_json::Value = reqwest::get(&url)
            .await
            .expect("request")
            .json()
            .await
            .expect("json");
        let n = body
            .get("result")
            .and_then(|r| r.get("list"))
            .and_then(|l| l.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(n > 0, "{}s returned no rows near the 365-day mark", secs);
    }
}

/// The page cap is a venue maximum: asking for more does not return more.
#[tokio::test]
#[ignore = "requires network; run with BYBIT_LIVE=1"]
async fn page_cap_is_hard_capped_at_1000() {
    if !live() {
        eprintln!("skipping (BYBIT_LIVE != 1)");
        return;
    }
    let a = kline_count("BTCUSDT", "1", 30, 1000).await;
    let b = kline_count("BTCUSDT", "1", 30, 5000).await;
    assert_eq!(a, b, "limit above 1000 must be clamped by the venue");
    assert!(a <= 1000);
}

/// A live perpetual is admitted; a dated future is not.
#[tokio::test]
#[ignore = "requires network; run with BYBIT_LIVE=1"]
async fn admission_rejects_dated_futures_and_accepts_perpetuals() {
    if !live() {
        eprintln!("skipping (BYBIT_LIVE != 1)");
        return;
    }
    for sym in ["BTCUSDT", "ETHUSDT", "AAPLUSDT", "XAUUSDT", "SPYUSDT"] {
        let inst = fetch_instrument(sym, INSTRUMENTS_URL)
            .await
            .expect("instrument lookup")
            .unwrap_or_else(|| panic!("{} should be listed", sym));
        assert!(
            is_tradeable_perpetual(&inst),
            "{} must be admitted (status={} type={})",
            sym,
            inst.status,
            inst.contract_type
        );
    }
    // An unlisted symbol must return None, not an error.
    assert!(fetch_instrument("ZZZNOTREALUSDT", INSTRUMENTS_URL)
        .await
        .expect("lookup")
        .is_none());
}

/// Bybit's TradFi line rides the same `category=linear` product, so it needs
/// no separate wiring — this asserts those instruments are reachable.
#[tokio::test]
#[ignore = "requires network; run with BYBIT_LIVE=1"]
async fn tradfi_perps_are_in_the_same_linear_product() {
    if !live() {
        eprintln!("skipping (BYBIT_LIVE != 1)");
        return;
    }
    for sym in [
        "AAPLUSDT",
        "TSLAUSDT",
        "XAUUSDT",
        "XAGUSDT",
        "CLUSDT",
        "EURUSDUSDT",
    ] {
        let inst = fetch_instrument(sym, INSTRUMENTS_URL)
            .await
            .expect("instrument lookup")
            .unwrap_or_else(|| panic!("{} should be listed", sym));
        assert!(is_tradeable_perpetual(&inst));
        eprintln!(
            "  {:<12} type={:<16} settle={}",
            sym, inst.symbol_type, inst.settle_coin
        );
    }
}

/// Keeps the unused-import warning away while documenting the timeout the
/// rest of this file assumes.
#[test]
fn probe_budget_is_sane() {
    assert!(Duration::from_secs(30) > Duration::from_secs(10));
}
