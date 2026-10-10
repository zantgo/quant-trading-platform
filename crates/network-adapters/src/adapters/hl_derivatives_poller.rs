//! Hyperliquid derivatives telemetry poller.
//!
//! Hyperliquid does not push mark price, open interest, or funding rate
//! updates over the public WebSocket. Instead, the `/info` REST endpoint
//! with `{"type":"metaAndAssetCtxs"}` returns the per-asset context for the
//! entire universe in a single request. We poll this endpoint on a timer
//! (default 60s) per active pair and emit one `OpenInterestEvent`, one
//! `FundingRateEvent`, and one `MarkPriceEvent` per successful round-trip.
//!
//! Resilience: exponential backoff (1s→60s, ±20% jitter) on transient REST
//! errors; permanent disable after 30 consecutive failures; HTTP 429 triggers
//! a 300s cooldown. Error logging is rate-limited after 5 consecutive failures
//! to avoid terminal noise flooding.
//!
//! Cancellation: the supplied `CancellationToken` cleanly stops the loop on
//! instance shutdown.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use core_domain::normalized::NormalizedEvent;

use crate::adapters::hyperliquid_rest::{
    derivatives_ctx_to_events, fetch_meta_and_asset_ctxs, HlDerivativesCtx,
};

const INITIAL_BACKOFF_SECS: u64 = 5;
const MAX_BACKOFF_SECS: u64 = 60;
const HTTP_429_COOLDOWN_SECS: u64 = 300;
const JITTER_PCT: f64 = 0.2;
const LOG_SUPPRESS_INTERVAL: u32 = 10;

fn apply_jitter(secs: u64) -> Duration {
    let range = (secs as f64 * JITTER_PCT) as i64;
    let jitter = if range > 0 {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let hash = RandomState::new().build_hasher().finish();
        ((hash as i64).unsigned_abs() % (2 * range as u64 + 1)) as i64 - range
    } else {
        0
    };
    let ms = ((secs as f64) * 1000.0) as i64 + jitter * 1000 / secs.max(1) as i64;
    Duration::from_millis((ms.max(500)) as u64)
}

fn compute_backoff(attempt: u32) -> u64 {
    let base = INITIAL_BACKOFF_SECS.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
    base.min(MAX_BACKOFF_SECS)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_hl_derivatives_poller(
    raw_symbol: String,
    internal_symbol: String,
    info_url: String,
    event_tx: Sender<NormalizedEvent>,
    cancel: CancellationToken,
    poll_ms: u64,
    max_consecutive_failures: u32,
    dex: Option<String>,
) {
    println!(
        "💹 HL Derivatives Poller: Started for {} ({}ms cadence)",
        raw_symbol, poll_ms
    );

    // Tracks the **USD-converted** previous OI so the analyzer sees a
    // consistent USD-notional series across polls. (Hyperliquid's
    // `openInterest` is in base-asset units; we multiply by `markPx`
    // inside `derivatives_ctx_to_events`.)
    let mut prev_oi_usd: Option<Decimal> = None;
    let mut consecutive_failures: u32 = 0;
    let poll_duration = Duration::from_millis(poll_ms.max(1000));

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!(
                    "🛑 HL Derivatives Poller: {} cancelled, shutting down.",
                    raw_symbol
                );
                break;
            }
            _ = tokio::time::sleep(poll_duration) => {}
        }

        match fetch_meta_and_asset_ctxs(&info_url, dex.as_deref()).await {
            Ok(map) => {
                if consecutive_failures > 5 {
                    println!(
                        "✅ HL Derivatives Poller: {} recovered after {} consecutive failures.",
                        raw_symbol, consecutive_failures
                    );
                }
                consecutive_failures = 0;
                if let Some(ctx) = lookup_ctx(&map, &raw_symbol) {
                    // Compute this poll's USD OI so we can hand the
                    // analyzer a USD-converted series on the next tick.
                    let this_oi_usd = match (ctx.open_interest, ctx.mark_px) {
                        (Some(oi), Some(mark)) if oi > Decimal::ZERO && mark > Decimal::ZERO => {
                            Some(oi * mark)
                        }
                        _ => None,
                    };
                    let events = derivatives_ctx_to_events(&internal_symbol, ctx, prev_oi_usd);
                    if this_oi_usd.is_some() {
                        prev_oi_usd = this_oi_usd;
                    }
                    for ev in events {
                        if event_tx.send(ev).await.is_err() {
                            eprintln!("⚠️  HL Derivatives Poller: {} event_tx closed", raw_symbol);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                consecutive_failures = consecutive_failures.saturating_add(1);

                if consecutive_failures >= max_consecutive_failures.max(1) {
                    eprintln!(
                        "🛑 HL Derivatives Poller: {} permanently disabled after {} consecutive failures.",
                        raw_symbol, consecutive_failures
                    );
                    break;
                }

                let suppressed =
                    consecutive_failures > 5 && consecutive_failures % LOG_SUPPRESS_INTERVAL != 0;
                if !suppressed {
                    eprintln!(
                        "⚠️  HL Derivatives Poller: {} failed ({} consecutive): {}",
                        raw_symbol, consecutive_failures, e
                    );
                }

                let backoff_secs = if e.contains("HTTP 429") || e.contains("Too Many Requests") {
                    HTTP_429_COOLDOWN_SECS
                } else {
                    compute_backoff(consecutive_failures)
                };
                let delay = apply_jitter(backoff_secs);

                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(delay) => {},
                }
            }
        }
    }
}

pub fn lookup_ctx<'a>(
    map: &'a HashMap<String, HlDerivativesCtx>,
    raw_symbol: &str,
) -> Option<&'a HlDerivativesCtx> {
    if let Some(c) = map.get(raw_symbol) {
        return Some(c);
    }
    if let Some(c) = map.get(&raw_symbol.to_uppercase()) {
        return Some(c);
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_hl_derivatives_poller(
    raw_symbol: String,
    internal_symbol: String,
    info_url: String,
    event_tx: Arc<Sender<NormalizedEvent>>,
    cancel: CancellationToken,
    poll_ms: u64,
    max_consecutive_failures: u32,
    dex: Option<String>,
) -> Option<tokio::task::JoinHandle<()>> {
    if raw_symbol.is_empty() || info_url.is_empty() {
        return None;
    }
    Some(tokio::spawn(async move {
        run_hl_derivatives_poller(
            raw_symbol,
            internal_symbol,
            info_url,
            event_tx.as_ref().clone(),
            cancel,
            poll_ms,
            max_consecutive_failures,
            dex,
        )
        .await;
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_ctx_handles_case_variants() {
        let mut m = HashMap::new();
        m.insert("BTC".to_string(), HlDerivativesCtx::default());
        assert!(lookup_ctx(&m, "BTC").is_some());
        assert!(lookup_ctx(&m, "btc").is_some());
        assert!(lookup_ctx(&m, "ETH").is_none());
    }
}
