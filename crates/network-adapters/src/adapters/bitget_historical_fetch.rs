//! Bitget implementation of the [`HistoricalFetchPolicy`] trait.
//!
//! Per `docs/engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md`
//! (HFP-01, HFP-04, HFP-05, HFP-06, HFP-07, HFP-08, HFP-10).
//!
//! ## Pagination — structurally identical to Hyperliquid
//!
//! Backward `endTime` cursors. The implementation issues repeated
//! `candles` requests with `endTime = earliest_in_page - duration_ms` until
//! `target_count` candles are collected, or the page comes back empty /
//! shorter than `BITGET_PAGE_LIMIT`, or `fetch_timeout_ms` elapses.
//!
//! The loop body mirrors `hyperliquid_historical_fetch.rs::fetch` line-for-line.
//! The only exchange-specific facts that differ are:
//!   - **Page cap:** `BITGET_PAGE_LIMIT = 200` (HL: 1000)
//!   - **Anchor function:** `page.iter().min()` (HL: `page.first()`)
//!     because Bitget returns newest-first within a page.
//!   - **Short-page signal:** break when the raw page length is below
//!     `BITGET_PAGE_LIMIT` (HL has no equivalent short-page behaviour).
//!
//! Behaviour in the v6.4 baseline was a single 200-row request with no
//! pagination — HFP-06 introduces the loop; HFP-04/HFP-05 tighten cursor
//! semantics so the loop actually reaches `target_count` instead of
//! terminating after page 1.
//!
//! ## Testability
//!
//! Production wires the loop to `fetch_historical_candles_page` via a
//! type-erased `PageFetcher` closure. Tests construct a `BitgetHistoricalFetch`
//! with a custom `new_with_pager` constructor so the pagination logic can
//! be exercised without a live HTTP server.

use async_trait::async_trait;
use core_domain::normalized::{Exchange, NormalizedCandle, ReconstructionMethod};
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use super::bitget_rest::fetch_historical_candles_page;
use super::historical_fetch::{
    HistoricalFetchError, HistoricalFetchPolicy, HistoricalFetchRequest,
};

/// Bitget hardcoded per-page limit (HFP-06).
const BITGET_PAGE_LIMIT: u32 = 200;

/// Type-erased page fetcher. Production wraps `fetch_historical_candles_page`;
/// tests inject a mock that returns canned pages.
pub type PageFetcher = std::sync::Arc<
    dyn Fn(
            &str,
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

pub struct BitgetHistoricalFetch {
    /// Base REST URL of the V2 mix (perpetual futures) candles endpoint,
    /// e.g. `https://api.bitget.com/api/v2/mix/market/candles`.
    pub rest_url: String,
    /// Futures product type (`"USDT-FUTURES"` / `"USDC-FUTURES"`).
    pub product_type: String,
    /// Optional test override for the page fetcher. `None` (production) routes
    /// through `fetch_historical_candles_page`; `Some(pager)` (tests) routes
    /// through the closure directly.
    page_fetcher: Option<PageFetcher>,
}

impl BitgetHistoricalFetch {
    pub fn new(rest_url: String, product_type: String) -> Self {
        Self {
            rest_url,
            product_type,
            page_fetcher: None,
        }
    }

    /// Constructor that wires a custom page fetcher. Production code
    /// paths use `new()` and never call this method; integration tests
    /// in `tests/` use it to drive the pagination loop with canned
    /// pages without a live HTTP server.
    ///
    /// Always `pub` so the integration-tests crate (which compiles
    /// outside the `cfg(test)` context) can construct pagers; not
    /// advertised as a stable API.
    pub fn new_with_pager(rest_url: String, product_type: String, pager: PageFetcher) -> Self {
        Self {
            rest_url,
            product_type,
            page_fetcher: Some(pager),
        }
    }

    /// Dispatch a single page-fetch through the configured page fetcher
    /// (test override or production real one).
    #[allow(clippy::too_many_arguments)]
    async fn fetch_page(
        &self,
        symbol: &str,
        internal_symbol: &str,
        product_type: &str,
        granularity: &str,
        start_time_ms: u64,
        end_time_ms: u64,
        limit: u32,
        rest_url: &str,
    ) -> Result<Vec<NormalizedCandle>, String> {
        match &self.page_fetcher {
            Some(pager) => {
                pager(
                    symbol,
                    internal_symbol,
                    product_type,
                    granularity,
                    start_time_ms,
                    end_time_ms,
                    limit,
                    rest_url,
                )
                .await
            }
            None => {
                fetch_historical_candles_page(
                    symbol,
                    internal_symbol,
                    product_type,
                    granularity,
                    start_time_ms,
                    end_time_ms,
                    limit,
                    rest_url,
                )
                .await
            }
        }
    }
}

#[async_trait]
impl HistoricalFetchPolicy for BitgetHistoricalFetch {
    fn exchange(&self) -> Exchange {
        Exchange::Bitget
    }

    async fn fetch(
        &self,
        request: HistoricalFetchRequest,
    ) -> Result<Vec<NormalizedCandle>, HistoricalFetchError> {
        // HFP-03: sub-minute short-circuit.
        if request.timeframe_secs < 60 {
            return Err(HistoricalFetchError::SubMinuteBypassed(
                request.timeframe_secs,
            ));
        }

        let granularity = super::bitget_rest::timeframe_secs_to_interval(request.timeframe_secs);
        let started = Instant::now();
        let timeout = Duration::from_millis(request.fetch_timeout_ms);
        let duration_ms = request.timeframe_secs * 1000;

        // The earliest candle we ever want to fetch. The cursor walks
        // backward from `end_ts` toward (but never past) this boundary.
        // Mirrors hyperliquid_historical_fetch.rs — the loop logic is
        // structurally identical, only the page-cap constant and the
        // anchor function differ (Bitget returns newest-first within a
        // page; HL returns oldest-first).
        //
        // v11.12 FIX: Bitget rejects any candle window wider than 90 days
        // ("startTime and endTime interval cannot be greater than 90 days").
        // The raw target window (500 × 12h = 250 d, 500 × 1d = 500 d) blew
        // past that cap and the FIRST page 400'd — 12h/1d bootstraps got
        // zero warm data and cold-started. Clamp the initial window to the
        // venue limit; the backward cursor (≤ BITGET_PAGE_LIMIT candles per
        // page, and `end_ts` only ever moves EARLIER) keeps every later
        // page within it. A clamped window yields a partial warm
        // (≤180 × 12h / ≤90 × 1d candles) — the MIN_WARMUP_BARS gate in the
        // bootstrap already logs the shortfall.
        const BITGET_MAX_WINDOW_MS: u64 = 90 * 24 * 3600 * 1000;
        let requested_window_ms = (request.target_count as u64).saturating_mul(duration_ms);
        let start_ts = request
            .end_ts
            .saturating_sub(requested_window_ms.min(BITGET_MAX_WINDOW_MS));

        let mut collected: Vec<NormalizedCandle> = Vec::with_capacity(request.target_count);
        let mut end_ts = request.end_ts;
        let mut pages: u32 = 0;

        while collected.len() < request.target_count {
            if started.elapsed() >= timeout {
                return Err(HistoricalFetchError::Timeout(
                    started.elapsed().as_millis() as u64
                ));
            }

            // HFP-04..HFP-06: paginate, asking for at most the page cap.
            // `desired` shrinks as the buffer fills so the last page is
            // not over-fetched.
            let remaining = request.target_count - collected.len();
            let desired = remaining.min(BITGET_PAGE_LIMIT as usize);

            let page = self
                .fetch_page(
                    &request.exchange_symbol,
                    &request.internal_symbol,
                    &self.product_type,
                    granularity,
                    start_ts,
                    end_ts,
                    desired as u32,
                    &self.rest_url,
                )
                .await
                .map_err(|e| HistoricalFetchError::Http {
                    status: 0,
                    attempts: pages + 1,
                    body: e,
                })?;

            // HFP-06 / HFP-07 short-page detection: capture the raw
            // page length BEFORE the open-candle filter. Bitget caps at
            // exactly `BITGET_PAGE_LIMIT` rows when more history is
            // available, and returns fewer when it isn't.
            let page_len = page.len();
            let mut page = page;

            // HFP-07: drop currently-open candles.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(request.end_ts);
            page.retain(|c| c.start_time_ms + duration_ms <= now_ms);

            // HFP-05: empty page after filter ⇒ no more history.
            if page.is_empty() {
                break;
            }

            // HFP-06 (Bitget-specific): a "short" page (raw len < what
            // we asked for) signals the exchange has no more history in
            // this window. (HL has no equivalent short-page signal.)
            if page_len < desired {
                collected.extend(page);
                break;
            }

            // Anchor on the OLDEST candle in the page (Bitget returns
            // newest-first within a page; HL uses `page.first().start_time_ms`).
            // The next request window is `[start_ts, earliest_in_page - duration_ms]`.
            let earliest_in_page = page.iter().map(|c| c.start_time_ms).min().unwrap_or(end_ts);
            let next_end = earliest_in_page.saturating_sub(duration_ms);

            // Defensive: if the cursor didn't strictly advance backward,
            // the exchange is returning overlapping windows — bail rather
            // than loop forever.
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

        // Sort newest-first (HFP-09 convention) and trim to target.
        collected.sort_by_key(|c| std::cmp::Reverse(c.start_time_ms));
        collected.truncate(request.target_count);

        // HFP-08: tag every candle as ExchangeHistorical (idempotent — the
        // page fetcher already tags them, but we re-assert in case the call
        // path bypasses the tagger).
        for c in &mut collected {
            if c.reconstructed.is_none() {
                c.reconstructed = Some(ReconstructionMethod::ExchangeHistorical);
            }
        }

        Ok(collected)
    }
}
