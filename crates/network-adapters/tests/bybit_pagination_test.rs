//! Bybit V5 kline pagination + instrument admission.
//!
//! Three venue facts are pinned:
//!
//! 1. **The page cap is a hard venue maximum.** `limit=1000`, `2000` and
//!    `5000` all return exactly 1000 rows, so `BYBIT_PAGE_LIMIT` is a venue
//!    cap and not a courtesy value. The fetcher clamps so a misconfigured
//!    `backtest.bybit.page_cap` cannot ask for rows that never arrive.
//!
//! 2. **Rows come back newest-first**, so the backward cursor anchors on the
//!    MINIMUM `start_time_ms` in the page — the same anchor Bitget uses, and
//!    the opposite of Hyperliquid's `page.first()`.
//!
//! 3. **There is no window clamp.** Bitget rejects any candle window wider
//!    than 90 days and clamps its initial span accordingly; Bybit's retention
//!    is ~2,425 days on every interval (measured), so no clamp applies and the
//!    widest pool request (500 x 1d = 500 days) is served in full.

use core_domain::normalized::{Exchange, NormalizedCandle};
use network_adapters::adapters::bybit_historical_fetch::BybitHistoricalFetch;
use network_adapters::adapters::bybit_historical_fetch::PageFetcher;
use network_adapters::adapters::bybit_rest::{
    interval_duration_ms, is_tradeable_perpetual, timeframe_secs_to_interval, BybitInstrument,
    BYBIT_PAGE_LIMIT,
};
use network_adapters::adapters::historical_fetch::{
    HistoricalFetchError, HistoricalFetchPolicy, HistoricalFetchRequest,
};
use rust_decimal::Decimal;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

fn candle(start_ms: u64, secs: u64) -> NormalizedCandle {
    NormalizedCandle {
        exchange: Exchange::Bybit,
        symbol: "BTC-USDT".into(),
        start_time_ms: start_ms,
        duration_ms: secs * 1000,
        open: Decimal::ONE,
        high: Decimal::ONE,
        low: Decimal::ONE,
        close: Decimal::ONE,
        volume: Decimal::ONE,
        trades_count: 0,
        reconstructed: None,
    }
}

/// A page ordered newest-first, exactly as `/v5/market/kline` returns it.
fn page_newest_first(base_ms: u64, secs: u64, count: u64) -> Vec<NormalizedCandle> {
    let mut v: Vec<NormalizedCandle> = (0..count)
        .map(|i| candle(base_ms + i * secs * 1000, secs))
        .collect();
    v.sort_by_key(|c| std::cmp::Reverse(c.start_time_ms));
    v
}

fn request(secs: u64, target: usize, end_ts: u64) -> HistoricalFetchRequest {
    HistoricalFetchRequest {
        exchange_symbol: "BTCUSDT".into(),
        internal_symbol: "BTC-USDT".into(),
        timeframe_secs: secs,
        target_count: target,
        end_ts,
        product_type: None,
        fetch_timeout_ms: 30_000,
    }
}

#[test]
fn interval_table_is_exact_and_self_consistent() {
    // Every >=60s duration in the 14-duration pool maps exactly, and the
    // derived duration_ms agrees with the requested duration. A mismatch
    // here would silently misalign every REST candle.
    for secs in [60u64, 180, 300, 900, 1800, 3600, 14400, 43200, 86400] {
        let iv = timeframe_secs_to_interval(secs);
        assert_eq!(
            interval_duration_ms(iv),
            secs * 1000,
            "interval {} must carry {}ms",
            iv,
            secs * 1000
        );
    }
}

#[test]
fn page_cap_is_the_venue_maximum() {
    // Measured: the venue returns 1000 rows whatever `limit` says.
    assert_eq!(BYBIT_PAGE_LIMIT, 1000);
}

#[tokio::test]
async fn subminute_is_bypassed_before_any_network_call() {
    // The pager would panic if invoked — proves HFP-03 short-circuits.
    let pager: PageFetcher = std::sync::Arc::new(|_s, _i, _iv, _st, _en, _l, _u| {
        Box::pin(async { panic!("sub-minute must never reach the REST fetcher") })
    });
    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    assert!(matches!(
        f.fetch(request(1, 500, 1_700_000_000_000)).await,
        Err(HistoricalFetchError::SubMinuteBypassed(1))
    ));
    assert!(f.exchange() == Exchange::Bybit);
}

#[tokio::test]
async fn a_single_page_satisfies_the_default_bootstrap_target() {
    // The bootstrap asks for [candle_buffer] size = 500 candles.
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let page = std::sync::Arc::new(page_newest_first(1_699_999_460_000, 60, 500));
    let pager: PageFetcher = std::sync::Arc::new(move |_s, _i, _iv, _st, _en, _l, _u| {
        counter.fetch_add(1, Ordering::Relaxed);
        let p = page.clone();
        Box::pin(async move { Ok((*p).clone()) })
    });
    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    let out = f
        .fetch(request(60, 500, 1_700_000_000_000))
        .await
        .expect("fetch");
    assert_eq!(out.len(), 500);
    assert_eq!(calls.load(Ordering::Relaxed), 1, "500 candles fit one page");
    // Newest-first is the merged order (HFP-09).
    assert!(out
        .windows(2)
        .all(|w| w[0].start_time_ms > w[1].start_time_ms));
    // Every candle is tagged as exchange-sourced (HFP-08).
    assert!(out.iter().all(|c| c.reconstructed.is_some()));
}

#[tokio::test]
async fn pagination_walks_backward_from_the_oldest_candle_in_each_page() {
    // Two 400-candle pages deliver a 500-candle target; the second request's
    // window must start below the first page's MINIMUM start_time_ms (the
    // newest-first anchor), not at its first element.
    let secs = 60u64;
    let step = secs * 1000;
    let end_ts = 1_700_000_000_000u64;
    let p1 = std::sync::Arc::new(page_newest_first(end_ts - 400 * step, secs, 400));
    let p2 = std::sync::Arc::new(page_newest_first(end_ts - 800 * step, secs, 400));
    let p1_min = p1.iter().map(|c| c.start_time_ms).min().unwrap();

    let windows: std::sync::Arc<std::sync::Mutex<Vec<(u64, u64)>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = windows.clone();
    let fetches = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = fetches.clone();
    let pager: PageFetcher = std::sync::Arc::new(move |_s, _i, _iv, st, en, l, _u| {
        seen.lock().unwrap().push((st, en));
        let i = counter.fetch_add(1, Ordering::Relaxed);
        let p = if i == 0 { p1.clone() } else { p2.clone() };
        let want = l as usize;
        Box::pin(async move { Ok((*p).clone().into_iter().take(want).collect::<Vec<_>>()) })
    });

    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    let out = f.fetch(request(secs, 500, end_ts)).await.expect("fetch");
    assert_eq!(out.len(), 500);
    assert_eq!(fetches.load(Ordering::Relaxed), 2);

    let w = windows.lock().unwrap();
    assert_eq!(w.len(), 2);
    // Window 1 is the full requested span.
    assert_eq!(w[0].1, end_ts);
    assert_eq!(w[0].0, end_ts - 500 * step);
    // Window 2's cursor sits strictly BELOW the first page's minimum start.
    assert_eq!(w[1].1, p1_min - step);
    assert!(
        w[1].1 < p1_min,
        "the backward cursor must advance past the oldest candle"
    );

    // No duplicate timestamps survive the merge.
    let mut ts: Vec<u64> = out.iter().map(|c| c.start_time_ms).collect();
    ts.dedup();
    assert_eq!(ts.len(), out.len(), "merged history must be deduped");
}

#[tokio::test]
async fn an_empty_page_terminates_the_loop_without_error() {
    let counter = std::sync::Arc::new(AtomicUsize::new(0));
    let fetches = counter.clone();
    let pager: PageFetcher = std::sync::Arc::new(move |_s, _i, _iv, _st, _en, _l, _u| {
        fetches.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Vec::new()) })
    });
    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    let out = f
        .fetch(request(60, 500, 1_700_000_000_000))
        .await
        .expect("fetch");
    assert!(out.is_empty());
    assert_eq!(
        counter.load(Ordering::Relaxed),
        1,
        "must stop after the empty page"
    );
}

#[tokio::test]
async fn the_widest_pool_request_is_served_without_a_window_clamp() {
    // 500 x 1d = 500 days. Bitget would clamp this to 90 days and cold-start;
    // Bybit's ~2,425-day retention serves it whole, so the initial window
    // must be the full requested span.
    let secs = 86_400u64;
    let step = secs * 1000;
    let end_ts = 1_700_000_000_000u64;
    let captured: std::sync::Arc<std::sync::Mutex<Option<(u64, u64)>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let slot = captured.clone();
    let page = page_newest_first(end_ts - 100 * step, secs, 100);
    let page = std::sync::Arc::new(page);
    let pager: PageFetcher = std::sync::Arc::new(move |_s, _i, _iv, st, en, _l, _u| {
        // Capture only the FIRST window — the loop paginates a second time
        // because 100 < 500, and it is the initial span that must be unclamped.
        let mut slot = slot.lock().unwrap();
        if slot.is_none() {
            *slot = Some((st, en));
        }
        let p = page.clone();
        Box::pin(async move { Ok((*p).clone()) })
    });
    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    let out = f.fetch(request(secs, 500, end_ts)).await.expect("fetch");
    assert_eq!(out.len(), 100);
    let (st, en) = captured.lock().unwrap().expect("window captured");
    assert_eq!(en, end_ts);
    assert_eq!(
        st,
        end_ts - 500 * step,
        "the 500-day span must NOT be clamped the way Bitget's 90-day cap is"
    );
}

#[tokio::test]
async fn a_failing_page_surfaces_an_http_error() {
    let pager: PageFetcher = std::sync::Arc::new(|_s, _i, _iv, _st, _en, _l, _u| {
        Box::pin(async { Err("venue unreachable".to_string()) })
    });
    let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
    let err = f
        .fetch(request(60, 500, 1_700_000_000_000))
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, HistoricalFetchError::Http { .. }),
        "expected an Http error, got {:?}",
        err
    );
}

#[test]
fn only_live_perpetuals_are_admitted() {
    let mk = |status: &str, ct: &str| BybitInstrument {
        symbol: "BTCUSDT".into(),
        contract_type: ct.into(),
        status: status.into(),
        base_coin: "BTC".into(),
        quote_coin: "USDT".into(),
        settle_coin: "USDT".into(),
        symbol_type: String::new(),
    };
    // The 858 perpetual instruments all pass.
    assert!(is_tradeable_perpetual(&mk("Trading", "LinearPerpetual")));
    // The 40 dated futures never do.
    assert!(!is_tradeable_perpetual(&mk("Trading", "LinearFutures")));
    assert!(!is_tradeable_perpetual(&mk("Trading", "InversePerpetual")));
    // Pre-launch / settling / delivering / closed are all refused.
    for s in [
        "PreLaunch",
        "Settling",
        "Delivering",
        "Closed",
        "",
        "trading",
    ] {
        let inst = mk(s, "LinearPerpetual");
        let expected = s.eq_ignore_ascii_case("Trading");
        assert_eq!(
            is_tradeable_perpetual(&inst),
            expected,
            "status {:?} admission",
            s
        );
    }
}

#[test]
fn the_pager_type_is_object_safe_for_test_injection() {
    // Compile-time proof that the erased future type is usable as a boxed
    // closure (the integration tests rely on this exact shape).
    fn assert_page_fetcher(_f: PageFetcher) {}
    let f: PageFetcher = std::sync::Arc::new(
        |_s: &str,
         _i: &str,
         _iv: &str,
         _st: u64,
         _en: u64,
         _l: u32,
         _u: &str|
         -> Pin<Box<dyn Future<Output = _> + Send>> {
            Box::pin(async { Ok(Vec::<NormalizedCandle>::new()) })
        },
    );
    assert_page_fetcher(f);
}
