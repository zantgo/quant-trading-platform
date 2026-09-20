//! Hyperliquid implementation of the [`HistoricalFetchPolicy`] trait.
//!
//! Per `docs/engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md`
//! (HFP-01, HFP-04, HFP-05, HFP-07, HFP-08, HFP-10).
//!
//! Pagination: backward `startTime` cursors. The implementation issues repeated
//! `candleSnapshot` requests with `endTime = previous_start - 1` until either
//! `target_count` candles are collected, or the response is empty, or
//! `fetch_timeout_ms` elapses.

use async_trait::async_trait;
use core_domain::normalized::{Exchange, NormalizedCandle, ReconstructionMethod};
use std::time::{Duration, Instant};

use super::historical_fetch::{
    HistoricalFetchError, HistoricalFetchPolicy, HistoricalFetchRequest,
};
use super::hyperliquid_rest::fetch_historical_candles;

/// Max candles per Hyperliquid REST page. The exchange doesn't document an
/// explicit cap but empirically returns at most a few hundred per request;
/// 1000 is the conservative upper bound used by the v6.5 pagination loop.
const HL_PAGE_CAP: usize = 1000;

pub struct HyperliquidHistoricalFetch {
    /// Base REST URL of the `/info` endpoint (e.g. `https://api.hyperliquid.xyz/info`).
    pub info_url: String,
}

impl HyperliquidHistoricalFetch {
    pub fn new(info_url: String) -> Self {
        Self { info_url }
    }
}

#[async_trait]
impl HistoricalFetchPolicy for HyperliquidHistoricalFetch {
    fn exchange(&self) -> Exchange {
        Exchange::Hyperliquid
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

        let interval = super::hyperliquid_rest::timeframe_secs_to_interval(request.timeframe_secs);
        let started = Instant::now();
        let timeout = Duration::from_millis(request.fetch_timeout_ms);

        let mut collected: Vec<NormalizedCandle> = Vec::with_capacity(request.target_count);
        let mut end_ts = request.end_ts;
        let mut pages = 0u32;

        while collected.len() < request.target_count {
            if started.elapsed() >= timeout {
                // v11.12.20: keep the pages already fetched on timeout
                // (partial warm beats a blank chart); an empty result still
                // reports Timeout so the cold-start path is unchanged.
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

            // Compute the desired page size: either the page cap, or whatever
            // remains to reach target_count — whichever is smaller.
            let remaining = request.target_count - collected.len();
            let desired = remaining.min(HL_PAGE_CAP);

            // Hyperliquid's REST API doesn't accept a `limit` parameter; we
            // bound the page by computing a `startTime = end_ts - N * duration_ms`
            // and asking the exchange for that window. Whatever it returns is
            // what we get; we filter the open candle and continue.
            let start_ts = end_ts.saturating_sub((desired as u64) * request.timeframe_secs * 1000);

            // v11.12.20: retry a failed page (slow networks) before giving
            // up — 2 retries with 1 s / 2 s backoff.
            let mut fetched: Option<Vec<NormalizedCandle>> = None;
            let mut fetch_err = String::new();
            for attempt in 0..3u32 {
                if attempt > 0 {
                    tokio::time::sleep(Duration::from_millis(1000 << (attempt - 1))).await;
                }
                match fetch_historical_candles(
                    &request.exchange_symbol,
                    &request.internal_symbol,
                    interval,
                    start_ts,
                    end_ts,
                    &self.info_url,
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
            let mut page = fetched.ok_or_else(|| HistoricalFetchError::Http {
                status: 0,
                attempts: pages + 1,
                body: fetch_err,
            })?;

            // HFP-07: drop currently-open candles.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(request.end_ts);
            let duration_ms = request.timeframe_secs * 1000;
            page.retain(|c| c.start_time_ms + duration_ms <= now_ms);

            if page.is_empty() {
                // Exchange has no more history; stop paginating.
                break;
            }

            // Anchor the next cursor to the earliest candle we just got.
            end_ts = page.first().map(|c| c.start_time_ms).unwrap_or(end_ts);

            // Defensive: if the page didn't advance, we'd loop forever — bail.
            if !collected.is_empty() && collected.last().map(|c| c.start_time_ms) <= Some(end_ts) {
                break;
            }

            collected.extend(page);
            pages += 1;

            // If we received fewer than `desired`, the exchange has no more history.
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
