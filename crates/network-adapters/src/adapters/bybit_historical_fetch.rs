//! Bybit implementation of the [`HistoricalFetchPolicy`] trait.
//!
//! Per `docs/engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md`
//! (HFP-01, HFP-04, HFP-05, HFP-07, HFP-08, HFP-10) and HFP-11 (Bybit).
//!
//! ## Pagination — structurally identical to Bitget
//!
//! Backward `end` cursors with the **minimum** `start_time_ms` as the anchor,
//! because `/v5/market/kline` returns rows newest-first. The loop body is
//! line-for-line the Bitget one; the exchange-specific facts that differ are:
//!
//!   - **Page cap:** `BYBIT_PAGE_LIMIT = 1000` (Bitget: 200, HL: 1000)
//!   - **No window clamp:** Bitget rejects any candle window wider than 90
//!     days and therefore clamps its initial span to `BITGET_MAX_WINDOW_MS`.
//!     Bybit has no such limit — measured retention is ~2,425 days on every
//!     interval — so the requested window is used as-is.
//!   - **No short-page detector:** Bybit signals "no more history" with an
//!     empty list, not a short one, so the loop terminates on emptiness.
//!
//! ## Testability
//!
//! Production wires the loop to [`super::bybit_rest::fetch_historical_candles_page`]
//! through a type-erased [`PageFetcher`] closure; tests construct the policy
//! with [`BybitHistoricalFetch::new_with_pager`] so the pagination loop can be
//! driven with canned pages and no live HTTP server.

use async_trait::async_trait;
use core_domain::normalized::{Exchange, NormalizedCandle, ReconstructionMethod};
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use super::bybit_rest::{fetch_historical_candles_page, BYBIT_PAGE_LIMIT};
use super::historical_fetch::{
    HistoricalFetchError, HistoricalFetchPolicy, HistoricalFetchRequest,
};

/// Type-erased page fetcher. Production wraps
/// [`fetch_historical_candles_page`]; tests inject a mock returning canned
/// pages. The shape mirrors `bitget_historical_fetch::PageFetcher` minus the
/// `product_type` argument (Bybit has one `category=linear` product).
pub type PageFetcher = std::sync::Arc<
    dyn Fn(
            &str,
            &str,
            &str,
            u64,
            u64,
            u32,
            &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<NormalizedCandle>, String>> + Send>>
        + Send
        + Sync,
>;

pub struct BybitHistoricalFetch {
    /// Full URL of the kline endpoint,
    /// e.g. `https://api.bybit.com/v5/market/kline`.
    pub kline_url: String,
    /// Optional test override for the page fetcher. `None` (production) routes
    /// through `fetch_historical_candles_page`; `Some(pager)` (tests) routes
    /// through the closure directly.
    page_fetcher: Option<PageFetcher>,
}

impl BybitHistoricalFetch {
    pub fn new(kline_url: String) -> Self {
        Self {
            kline_url,
            page_fetcher: None,
        }
    }

    /// Constructor that wires a custom page fetcher. Production code paths
    /// use [`BybitHistoricalFetch::new`] and never call this; the integration
    /// tests in `tests/` use it to drive the pagination loop with canned pages.
    ///
    /// Always `pub` so the integration-tests crate (which compiles outside the
    /// `cfg(test)` context) can construct pagers; not advertised as a stable
    /// API.
    pub fn new_with_pager(kline_url: String, pager: PageFetcher) -> Self {
        Self {
            kline_url,
            page_fetcher: Some(pager),
        }
    }

    /// Dispatch a single page-fetch through the configured page fetcher
    /// (test override or the production REST call).
    #[allow(clippy::too_many_arguments)]
    async fn fetch_page(
        &self,
        symbol: &str,
        internal_symbol: &str,
        interval: &str,
        start_time_ms: u64,
        end_time_ms: u64,
        limit: u32,
        kline_url: &str,
    ) -> Result<Vec<NormalizedCandle>, String> {
        match &self.page_fetcher {
            Some(pager) => {
                pager(
                    symbol,
                    internal_symbol,
                    interval,
                    start_time_ms,
                    end_time_ms,
                    limit,
                    kline_url,
                )
                .await
            }
            None => {
                fetch_historical_candles_page(
                    symbol,
                    internal_symbol,
                    interval,
                    start_time_ms,
                    end_time_ms,
                    limit,
                    kline_url,
                )
                .await
            }
        }
    }
}

#[async_trait]
impl HistoricalFetchPolicy for BybitHistoricalFetch {
    fn exchange(&self) -> Exchange {
        Exchange::Bybit
    }

    async fn fetch(
        &self,
        request: HistoricalFetchRequest,
    ) -> Result<Vec<NormalizedCandle>, HistoricalFetchError> {
        // HFP-03: sub-minute short-circuit. Identical on every venue — Bybit
        // publishes no sub-minute interval, so the buffer fills from live
        // trades.
        if request.timeframe_secs < 60 {
            return Err(HistoricalFetchError::SubMinuteBypassed(
                request.timeframe_secs,
            ));
        }

        let interval = super::bybit_rest::timeframe_secs_to_interval(request.timeframe_secs);
        let started = Instant::now();
        let timeout = Duration::from_millis(request.fetch_timeout_ms);
        let duration_ms = request.timeframe_secs * 1000;

        // The earliest candle we ever want to fetch. The cursor walks backward
        // from `end_ts` toward (but never past) this boundary. Unlike Bitget
        // there is NO 90-day venue clamp: measured Bybit retention is ~2,425
        // days on every interval, so even the widest pool request
        // (500 × 1d = 500 days) is comfortably inside it and clamping here
        // would only throw away history the venue is willing to serve.
        let requested_window_ms = (request.target_count as u64).saturating_mul(duration_ms);
        let start_ts = request.end_ts.saturating_sub(requested_window_ms);

        let mut collected: Vec<NormalizedCandle> = Vec::with_capacity(request.target_count);
        let mut end_ts = request.end_ts;
        let mut pages: u32 = 0;

        while collected.len() < request.target_count {
            if started.elapsed() >= timeout {
                // v11.12.20: a timeout must NOT discard the pages already
                // fetched — keep the partial warm so the bootstrap's
                // min-warmup gate logs the shortfall rather than the pipeline
                // cold-starting. An EMPTY result still reports Timeout so the
                // caller's cold-start path is unchanged.
                if collected.is_empty() {
                    return Err(HistoricalFetchError::Timeout(
                        started.elapsed().as_millis() as u64
                    ));
                }
                eprintln!(
                    "⚠️  Historical fetch [{}] timed out after {} ms with {}/{} candles — keeping the partial history.",
                    request.internal_symbol,
                    started.elapsed().as_millis(),
                    collected.len(),
                    request.target_count,
                );
                break;
            }

            // HFP-04..HFP-06: paginate, asking for at most the venue page cap.
            // `desired` shrinks as the buffer fills so the last page is not
            // over-fetched.
            let remaining = request.target_count - collected.len();
            let desired = remaining.min(BYBIT_PAGE_LIMIT as usize);

            // v11.12.20: retry a failed page (slow networks) before giving up —
            // 2 retries with 1 s / 2 s backoff.
            let mut fetched: Option<Vec<NormalizedCandle>> = None;
            let mut fetch_err = String::new();
            for attempt in 0..3u32 {
                if attempt > 0 {
                    tokio::time::sleep(Duration::from_millis(1000 << (attempt - 1))).await;
                }
                match self
                    .fetch_page(
                        &request.exchange_symbol,
                        &request.internal_symbol,
                        interval,
                        start_ts,
                        end_ts,
                        desired as u32,
                        &self.kline_url,
                    )
                    .await
                {
                    Ok(p) => {
                        fetched = Some(p);
                        break;
                    }
                    Err(e) => fetch_err = e,
                }
            }
            let page = fetched.ok_or_else(|| HistoricalFetchError::Http {
                status: 0,
                attempts: pages + 1,
                body: fetch_err,
            })?;

            // HFP-07: drop currently-open candles.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(request.end_ts);
            let mut page = page;
            page.retain(|c| c.start_time_ms + duration_ms <= now_ms);

            // HFP-05: empty page after filter ⇒ no more history.
            if page.is_empty() {
                break;
            }

            // Anchor on the OLDEST candle in the page (Bybit returns
            // newest-first, same as Bitget; HL uses `page.first()`). The next
            // request window is `[start_ts, earliest_in_page - duration_ms]`.
            let earliest_in_page = page.iter().map(|c| c.start_time_ms).min().unwrap_or(end_ts);
            let next_end = earliest_in_page.saturating_sub(duration_ms);

            // Defensive: if the cursor didn't strictly advance backward, the
            // venue is returning overlapping windows — bail rather than loop
            // forever.
            if !collected.is_empty() && next_end >= end_ts {
                break;
            }
            end_ts = next_end;

            collected.extend(page);
            pages += 1;

            if collected.len() >= request.target_count {
                break;
            }
        }

        // HFP-09 convention: merged newest-first, deduped by start_time_ms,
        // capped at target_count.
        collected.sort_by_key(|c| std::cmp::Reverse(c.start_time_ms));
        collected.dedup_by_key(|c| c.start_time_ms);
        collected.truncate(request.target_count);

        // HFP-08: tag every candle as ExchangeHistorical (idempotent — the
        // page fetcher already tags them, re-asserted in case the call path
        // bypasses the tagger).
        for c in &mut collected {
            if c.reconstructed.is_none() {
                c.reconstructed = Some(ReconstructionMethod::ExchangeHistorical);
            }
        }

        Ok(collected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::normalized::Exchange;

    fn candle(start: u64, secs: u64) -> NormalizedCandle {
        NormalizedCandle {
            exchange: Exchange::Bybit,
            symbol: "BTC-USDT".into(),
            start_time_ms: start,
            duration_ms: secs * 1000,
            open: rust_decimal::Decimal::ONE,
            high: rust_decimal::Decimal::ONE,
            low: rust_decimal::Decimal::ONE,
            close: rust_decimal::Decimal::ONE,
            volume: rust_decimal::Decimal::ONE,
            trades_count: 0,
            reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
        }
    }

    #[tokio::test]
    async fn subminute_is_bypassed() {
        let f = BybitHistoricalFetch::new("http://unused".into());
        let req = HistoricalFetchRequest {
            exchange_symbol: "BTCUSDT".into(),
            internal_symbol: "BTC-USDT".into(),
            timeframe_secs: 1,
            target_count: 500,
            end_ts: 1_700_000_000_000,
            product_type: None,
            fetch_timeout_ms: 30_000,
        };
        assert!(matches!(
            f.fetch(req).await,
            Err(HistoricalFetchError::SubMinuteBypassed(1))
        ));
    }

    #[tokio::test]
    async fn pagination_collects_target_count_across_pages() {
        // Two pages of 500, newest-first within each page, anchored on the
        // minimum start_time_ms. Target is 800, so the loop stops mid-second-
        // page and truncates to 800.
        let step = 60_000u64;
        let base = 1_700_000_000_000u64;
        let make_page = |from_newest: u64, count: u64| {
            let mut v: Vec<NormalizedCandle> = (0..count)
                .map(|i| candle(base + (from_newest - i) * step, 60))
                .collect();
            v.sort_by_key(|c| std::cmp::Reverse(c.start_time_ms));
            v
        };
        let page1 = make_page(1_000, 500);
        let page2 = make_page(500, 500);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let pager: PageFetcher = std::sync::Arc::new(move |_s, _i, _iv, _st, _en, _l, _u| {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p = if n == 0 { page1.clone() } else { page2.clone() };
            Box::pin(async move { Ok(p) })
        });
        let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
        let req = HistoricalFetchRequest {
            exchange_symbol: "BTCUSDT".into(),
            internal_symbol: "BTC-USDT".into(),
            timeframe_secs: 60,
            target_count: 800,
            end_ts: base + 1_000 * step,
            product_type: None,
            fetch_timeout_ms: 30_000,
        };
        let got = f.fetch(req).await.expect("fetch");
        assert_eq!(got.len(), 800);
        // Newest-first ordering preserved.
        assert!(got
            .windows(2)
            .all(|w| w[0].start_time_ms > w[1].start_time_ms));
    }

    #[tokio::test]
    async fn empty_page_terminates_without_error() {
        let pager: PageFetcher =
            std::sync::Arc::new(|_s, _i, _iv, _st, _en, _l, _u| Box::pin(async { Ok(Vec::new()) }));
        let f = BybitHistoricalFetch::new_with_pager("http://unused".into(), pager);
        let req = HistoricalFetchRequest {
            exchange_symbol: "BTCUSDT".into(),
            internal_symbol: "BTC-USDT".into(),
            timeframe_secs: 60,
            target_count: 500,
            end_ts: 1_700_000_000_000,
            product_type: None,
            fetch_timeout_ms: 30_000,
        };
        assert!(f.fetch(req).await.expect("fetch").is_empty());
    }
}
