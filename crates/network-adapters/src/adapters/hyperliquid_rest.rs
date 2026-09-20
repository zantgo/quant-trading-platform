use core_domain::normalized::{
    Exchange, FundingRateEvent, MarkPriceEvent, NormalizedCandle, NormalizedEvent,
    OpenInterestEvent, ReconstructionMethod,
};
use rust_decimal::Decimal;
use serde::Deserialize;

use super::http;

#[derive(Debug, Deserialize)]
struct CandleSnapshot {
    #[serde(rename = "t")]
    start_time_ms: u64,
    #[allow(dead_code)]
    #[serde(rename = "T")]
    end_time_ms: u64,
    #[allow(dead_code)]
    #[serde(rename = "s")]
    coin: String,
    #[allow(dead_code)]
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "n")]
    trades_count: u64,
}

fn parse_decimal(s: &str) -> Result<Decimal, String> {
    s.parse::<Decimal>()
        .map_err(|e| format!("Failed to parse decimal '{}': {}", s, e))
}

/// Fetch historical candles from the Hyperliquid Info REST API.
///
/// `rest_url` is the derived `/info` endpoint (e.g. `https://api.hyperliquid.xyz/info`).
/// `symbol` is the raw exchange coin name (e.g. `"BTC"`).
/// `interval` is the Hyperliquid interval string (e.g. `"1m"`, `"5m"`, `"15m"`, `"1h"`).
/// `start_time_ms` and `end_time_ms` bound the candle range.
pub async fn fetch_historical_candles(
    symbol: &str,
    internal_symbol: &str,
    interval: &str,
    start_time_ms: u64,
    end_time_ms: u64,
    rest_url: &str,
) -> Result<Vec<NormalizedCandle>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let request_body = serde_json::json!({
        "type": "candleSnapshot",
        "req": {
            "coin": symbol,
            "interval": interval,
            "startTime": start_time_ms,
            "endTime": end_time_ms,
        }
    });

    // v9: transport-level retry — deep backfills page dozens of requests;
    // a single transient send error must not abort the whole run.
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err: Option<String> = None;
    let mut response = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match client.post(rest_url).json(&request_body).send().await {
            Ok(res) => {
                response = Some(res);
                last_err = None;
                break;
            }
            Err(e) => {
                last_err = Some(format!(
                    "REST request failed for {} {}: {}",
                    symbol, interval, e
                ));
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(400 * attempt as u64))
                        .await;
                }
            }
        }
    }
    let response = response.ok_or_else(|| last_err.unwrap_or_default())?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "REST endpoint returned HTTP {} for {} {}: {}",
            status, symbol, interval, body
        ));
    }

    let snapshots: Vec<CandleSnapshot> = response.json().await.map_err(|e| {
        format!(
            "Failed to parse candle snapshot JSON for {} {}: {}",
            symbol, interval, e
        )
    })?;

    snapshots
        .into_iter()
        .map(|cs| {
            let start = cs.start_time_ms;
            let duration = cs.end_time_ms.saturating_sub(cs.start_time_ms);
            Ok(NormalizedCandle {
                exchange: Exchange::Hyperliquid,
                symbol: internal_symbol.to_string(),
                start_time_ms: start,
                duration_ms: duration,
                open: parse_decimal(&cs.open)?,
                high: parse_decimal(&cs.high)?,
                low: parse_decimal(&cs.low)?,
                close: parse_decimal(&cs.close)?,
                volume: parse_decimal(&cs.volume)?,
                trades_count: cs.trades_count,
                reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct HlAsset {
    name: String,
}

#[derive(Debug, Deserialize)]
struct HlMeta {
    universe: Vec<HlAsset>,
}

/// Verify that a Hyperliquid perpetual coin exists by querying the `meta`
/// endpoint and checking the asset universe.
///
/// v11.12.19: the check retries TRANSPORT errors (3 attempts, 30 s per
/// attempt + connect timeout, 1 s/2 s backoff) — slow networks (cold DNS can
/// take seconds through a VPN) used to fail the single 10 s request. A
/// definitive venue answer is never retried.
///
/// Returns `Ok(true)` if the coin is listed, `Ok(false)` if not, and `Err(..)`
/// only on transport/parse failures.
pub async fn symbol_exists(coin: &str, info_url: &str) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(http::PROBE_CONNECT_TIMEOUT)
        .timeout(http::PROBE_TOTAL_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut last_err = String::new();
    for attempt in 0..http::PROBE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(http::probe_backoff(attempt)).await;
        }
        match client
            .post(info_url)
            .json(&serde_json::json!({ "type": "meta" }))
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    return Err(format!(
                        "Hyperliquid meta endpoint returned HTTP {}",
                        response.status()
                    ));
                }

                let meta: HlMeta = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse Hyperliquid meta JSON: {}", e))?;

                let target = coin.to_uppercase();
                let ok = meta
                    .universe
                    .iter()
                    .any(|a| a.name.to_uppercase() == target);
                return Ok(ok);
            }
            Err(e) => {
                last_err = http::describe_reqwest_error(&e);
                eprintln!(
                    "Symbol check attempt {}/{} failed for {}: {}",
                    attempt + 1,
                    http::PROBE_ATTEMPTS,
                    coin,
                    last_err
                );
            }
        }
    }
    Err(format!(
        "Symbol check request failed for {} after {} attempts: {}",
        coin,
        http::PROBE_ATTEMPTS,
        last_err
    ))
}

// =============================================================================
// Derivatives Telemetry — metaAndAssetCtxs polling
// =============================================================================
//
// Hyperliquid does not expose OI, mark price, or funding rate on the public
// WebSocket. Instead, the REST endpoint `/info` with
// `{"type":"metaAndAssetCtxs"}` returns the per-asset context for the entire
// universe in a single request. We poll this endpoint on a timer
// (default 60s) per active pair and emit one `OpenInterestEvent`, one
// `FundingRateEvent`, and one `MarkPriceEvent` per successful round-trip.

#[derive(Debug, Deserialize)]
struct MetaAndAssetCtxsResponse(
    #[allow(dead_code)] serde_json::Value, // meta (asset universe)
    Vec<AssetCtxEntry>,
);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct AssetCtxEntry {
    /// Hyperliquid's `/info metaAndAssetCtxs` response doesn't include a
    /// `coin` field per entry — the coin name comes positionally from
    /// `meta.universe[i].name`. We try to deserialise it for forward
    /// compatibility with any future shape change, but make it optional
    /// so the parser never rejects the real payload.
    #[allow(dead_code)]
    #[serde(default)]
    coin: Option<String>,
    #[serde(default, rename = "markPx")]
    markPx: Option<serde_json::Value>,
    #[serde(default, rename = "oraclePx")]
    oraclePx: Option<serde_json::Value>,
    #[serde(default, rename = "openInterest")]
    openInterest: Option<serde_json::Value>,
    #[serde(default)]
    funding: Option<serde_json::Value>,
    #[serde(default, rename = "prevDayPx")]
    prevDayPx: Option<serde_json::Value>,
}

fn parse_ctx_decimal(v: &Option<serde_json::Value>) -> Option<Decimal> {
    match v {
        None => None,
        Some(serde_json::Value::String(s)) => s.parse::<Decimal>().ok(),
        Some(serde_json::Value::Number(n)) => n.as_f64().and_then(Decimal::from_f64_retain),
        _ => None,
    }
}

/// Per-coin parsed derivatives context. All-`None` if a field was absent.
#[derive(Debug, Clone, Default)]
pub struct HlDerivativesCtx {
    pub mark_px: Option<Decimal>,
    pub oracle_px: Option<Decimal>,
    pub open_interest: Option<Decimal>,
    pub funding: Option<Decimal>,
    pub prev_day_px: Option<Decimal>,
}

/// Fetch and parse the full asset-ctx universe. Returns a map keyed by raw
/// coin name (e.g. "BTC"). Errors propagate.
pub async fn fetch_meta_and_asset_ctxs(
    info_url: &str,
) -> Result<std::collections::HashMap<String, HlDerivativesCtx>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .post(info_url)
        .json(&serde_json::json!({ "type": "metaAndAssetCtxs" }))
        .send()
        .await
        .map_err(|e| format!("Hyperliquid metaAndAssetCtxs request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Hyperliquid metaAndAssetCtxs HTTP {}",
            response.status()
        ));
    }

    let parsed: MetaAndAssetCtxsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Hyperliquid metaAndAssetCtxs: {e}"))?;
    let MetaAndAssetCtxsResponse(meta_json, asset_ctxs) = parsed;

    // Recover the asset universe so each entry can be keyed by its real
    // coin name. `meta.universe` and the parallel `asset_ctxs` array are
    // positional; the i-th entry of each refers to the same coin. We
    // reuse the existing `HlMeta` struct (only `universe` is read) so the
    // JSON shape is forgiving: extra fields in `meta` are ignored.
    let meta: HlMeta = serde_json::from_value(meta_json)
        .map_err(|e| format!("Failed to parse Hyperliquid meta universe: {e}"))?;
    let universe_index_to_name: Vec<Option<String>> = meta
        .universe
        .into_iter()
        .map(|a| {
            if a.name.is_empty() {
                None
            } else {
                Some(a.name)
            }
        })
        .collect();

    let mut map = std::collections::HashMap::with_capacity(asset_ctxs.len());
    for (i, mut entry) in asset_ctxs.into_iter().enumerate() {
        let ctx = HlDerivativesCtx {
            mark_px: parse_ctx_decimal(&entry.markPx),
            oracle_px: parse_ctx_decimal(&entry.oraclePx),
            open_interest: parse_ctx_decimal(&entry.openInterest),
            funding: parse_ctx_decimal(&entry.funding),
            prev_day_px: parse_ctx_decimal(&entry.prevDayPx),
        };
        // Prefer `entry.coin` if present (forward-compat); otherwise fall
        // back to the positional `meta.universe[i].name`; otherwise invent
        // a UNKNOWN_<i> placeholder so the response is never empty.
        let key = entry
            .coin
            .take()
            .or_else(|| universe_index_to_name.get(i).and_then(|n| n.clone()))
            .unwrap_or_else(|| format!("UNKNOWN_{i}"));
        map.insert(key, ctx);
    }
    Ok(map)
}

/// Convert a single `HlDerivativesCtx` snapshot into the normalized events
/// the analyzer expects. Each non-`None` field yields exactly one event.
/// `internal_symbol` is the unified workspace symbol (e.g. "BTC-USDT")
/// used on every emitted event.
///
/// **OI unit conversion**: Hyperliquid's `openInterest` field is in
/// **base-asset units** (e.g. 39,925 BTC for BTC perpetuals), not USD.
/// The cluster estimator downstream treats `total_oi_usd` as a USD notional,
/// so we multiply by `markPx` here. If `markPx` is missing or non-positive
/// we skip emitting an OI event rather than propagate the wrong-unit value
/// (which would poison cluster confidence — see
/// `docs/engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md`
/// §3.2 "OI unit conversion").
pub fn derivatives_ctx_to_events(
    internal_symbol: &str,
    ctx: &HlDerivativesCtx,
    prev_oi: Option<Decimal>,
) -> Vec<NormalizedEvent> {
    let mut out = Vec::with_capacity(3);
    if let (Some(oi), Some(mark)) = (ctx.open_interest, ctx.mark_px) {
        if oi > Decimal::ZERO && mark > Decimal::ZERO {
            let oi_usd = oi * mark;
            out.push(NormalizedEvent::OpenInterest(OpenInterestEvent {
                symbol: internal_symbol.to_string(),
                oi: oi_usd,
                prev_oi: prev_oi.map(|p| p * mark),
            }));
        }
    }
    if let Some(rate) = ctx.funding {
        // v6.10 (Phase 1 / A2): Hyperliquid publishes the funding rate per
        // HOUR, while every downstream consumer (Bitget adapter, funding
        // normalizer, L2.5 cluster estimator, L5 cascade risk, Phase 3
        // signals `FUNDING_EXTREME` / `FUNDING_FLIP`) assumes per-8h
        // semantics. We normalize HL's per-hour rate to per-8h here at
        // the adapter boundary by multiplying by 8. This makes the
        // `FundingRateEvent` cross-venue comparable with Bitget and keeps
        // all downstream thresholds calibrated as documented.
        let rate_per_8h = rate * Decimal::from(8);
        out.push(NormalizedEvent::FundingRate(FundingRateEvent {
            symbol: internal_symbol.to_string(),
            rate: rate_per_8h,
        }));
    }
    if let Some(mark) = ctx.mark_px {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        out.push(NormalizedEvent::MarkPrice(MarkPriceEvent {
            symbol: internal_symbol.to_string(),
            mark_px: mark,
            index_px: ctx.oracle_px,
            timestamp_ms: ts,
        }));
    }
    out
}

/// Map an internal timeframe duration in seconds to the Hyperliquid REST interval string.
pub fn timeframe_secs_to_interval(secs: u64) -> &'static str {
    match secs {
        60 => "1m",
        180 => "3m",
        300 => "5m",
        900 => "15m",
        1800 => "30m",
        3600 => "1h",
        7200 => "2h",
        14400 => "4h",
        28800 => "8h",
        43200 => "12h",
        86400 => "1d",
        other if other < 60 => "1m",
        other if other < 180 => "1m",
        other if other < 300 => "3m",
        other if other < 900 => "5m",
        other if other < 1800 => "15m",
        other if other < 3600 => "30m",
        other if other < 7200 => "1h",
        other if other < 14400 => "2h",
        other if other < 28800 => "4h",
        other if other < 43200 => "8h",
        other if other < 86400 => "12h",
        _ => "1d",
    }
}
