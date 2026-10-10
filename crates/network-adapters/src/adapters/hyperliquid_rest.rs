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

/// A coin the operator asked for, resolved to the exact market it names.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCoin {
    /// The venue's wire name — `BTC`, or `xyz:TLT` on a HIP-3 dex. This is
    /// what every venue call (WS subscribe, candleSnapshot, asset index)
    /// must send verbatim.
    pub venue_coin: String,
    /// The bare base the operator typed (`TLT`), which is what the pair key
    /// and the UI display use.
    pub base: String,
    /// The perp dex that owns the market, or `None` for the default crypto dex.
    pub dex: Option<String>,
    /// Max leverage the venue publishes for this market.
    pub max_leverage: i64,
    /// True when the market is tradeable but isolated-margin-only
    /// (`onlyIsolated` / `marginMode: "noCross"`). Observe and paper are
    /// unaffected; live dispatch needs separate margin handling.
    pub isolated_only: bool,
}

#[derive(Debug, Deserialize)]
struct HlDexMetaBody {
    universe: Vec<HlMetaAsset>,
}

/// POST an info request with the v11.12.19 retry policy (transport errors only
/// — a definitive venue answer is never retried).
async fn post_info_with_retry(
    client: &reqwest::Client,
    info_url: &str,
    body: &serde_json::Value,
    what: &str,
) -> Result<serde_json::Value, String> {
    let mut last_err = String::new();
    for attempt in 0..http::PROBE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(http::probe_backoff(attempt)).await;
        }
        match client.post(info_url).json(body).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    return Err(format!(
                        "Hyperliquid {what} endpoint returned HTTP {}",
                        response.status()
                    ));
                }
                return response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse Hyperliquid {what} JSON: {e}"));
            }
            Err(e) => {
                last_err = http::describe_reqwest_error(&e);
                eprintln!(
                    "{what} attempt {}/{} failed: {}",
                    attempt + 1,
                    http::PROBE_ATTEMPTS,
                    last_err
                );
            }
        }
    }
    Err(format!(
        "Hyperliquid {what} request failed after {} attempts: {}",
        http::PROBE_ATTEMPTS,
        last_err
    ))
}

fn probe_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(http::PROBE_CONNECT_TIMEOUT)
        .timeout(http::PROBE_TOTAL_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// List the named HIP-3 perp dexes (`{"type":"perpDexs"}`).
///
/// The response's first element is `null` — the unnamed default crypto dex —
/// and is skipped. Dex order is the venue's own ordering and is preserved, so
/// it doubles as the deterministic tiebreak when a base is ambiguous.
pub async fn fetch_perp_dexes(info_url: &str) -> Result<Vec<String>, String> {
    let client = probe_client()?;
    let body = post_info_with_retry(
        &client,
        info_url,
        &serde_json::json!({ "type": "perpDexs" }),
        "perpDexs",
    )
    .await?;
    // Each element is `{name, fullName, deployer, …}` — or `null` for the
    // unnamed default crypto dex, which is addressed by omitting `"dex"`.
    let entries: Vec<Option<serde_json::Value>> =
        serde_json::from_value(body).map_err(|e| format!("Failed to parse perpDexs JSON: {e}"))?;
    Ok(entries
        .into_iter()
        .flatten()
        .filter_map(|d| d.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect())
}

/// One entry of a perp dex's tradable universe.
#[derive(Debug, Clone, Deserialize)]
pub struct HlMetaAsset {
    pub name: String,
    /// The venue spells this `isDelisted`. Without the rename it would silently
    /// deserialize as `None` (= "not delisted") and the filter would be a no-op.
    #[serde(default, rename = "isDelisted")]
    pub is_delisted: Option<bool>,
    #[serde(default, rename = "maxLeverage")]
    pub max_leverage: Option<i64>,
    #[serde(default, rename = "onlyIsolated")]
    pub only_isolated: Option<bool>,
    #[serde(default, rename = "marginMode")]
    pub margin_mode: Option<String>,
}

/// Fetch one dex's tradable universe. `dex = None` is the default crypto dex.
pub async fn fetch_dex_universe(
    info_url: &str,
    dex: Option<&str>,
) -> Result<Vec<HlMetaAsset>, String> {
    let client = probe_client()?;
    let body = match dex {
        Some(d) => serde_json::json!({ "type": "meta", "dex": d }),
        None => serde_json::json!({ "type": "meta" }),
    };
    let value = post_info_with_retry(&client, info_url, &body, "meta").await?;
    let meta: HlDexMetaBody =
        serde_json::from_value(value).map_err(|e| format!("Failed to parse meta JSON: {e}"))?;
    Ok(meta.universe)
}

/// Resolve an operator-typed ticker to the exact Hyperliquid market it names.
///
/// `coin` may be a bare base (`TLT`, which auto-resolves) or carry an explicit
/// `dex:BASE` qualifier (`xyz:TLT`), in which case only that dex is consulted
/// and a miss is a miss rather than a silent fallback.
///
/// Why this exists: since HIP-3 Hyperliquid runs multiple perp dexes, and the
/// bare `{"type":"meta"}` query returns ONLY the default crypto dex (234
/// assets). Every equity / index / commodity / FX perp (`xyz:TLT`, `xyz:GOLD`,
/// `xyz:SP500`) lives on a builder-deployed dex and was therefore invisible to
/// the old single-universe check — which is exactly why those tickers were
/// rejected as "isn't available" while BTC and ETH worked.
///
/// Returns `Ok(None)` when the base is genuinely not listed (or every match is
/// delisted), and `Err(..)` only on transport/parse failure so callers can tell
/// "not available" from "couldn't check".
pub async fn resolve_venue_coin(
    coin: &str,
    info_url: &str,
) -> Result<Option<ResolvedCoin>, String> {
    let explicit = core_domain::symbol_rules::split_dex_qualifier(coin);
    let (wanted_dex, base) = match explicit {
        Some((d, b)) => (Some(d.to_string()), b.to_string()),
        None => (None, coin.to_string()),
    };
    let target = base.to_uppercase();

    // A `dex:BASE` qualifier that names no dex on the venue is a typo, not an
    // auto-resolve request — surface it as unavailable rather than silently
    // resolving to some other dex's market.
    if let Some(d) = wanted_dex.as_deref() {
        let dexes = fetch_perp_dexes(info_url).await?;
        let known = dexes.iter().any(|x| x.eq_ignore_ascii_case(d));
        if !known {
            return Ok(None);
        }
        // Dex names are machine identifiers the venue publishes lowercase, so
        // normalize before querying — an operator typing `XYZ:TLT` must reach
        // the same market as `xyz:TLT` rather than an empty universe.
        let normalized_dex = d.to_ascii_lowercase();
        let universe = fetch_dex_universe(info_url, Some(&normalized_dex)).await?;
        return Ok(pick_coin(
            &universe,
            Some(&normalized_dex),
            &target,
            base.as_str(),
        ));
    }

    // Default crypto dex first, then the HIP-3 dexes in venue order. Any single
    // dex failing must not abort the sweep — a flaky builder dex should not
    // hide the markets that did answer.
    let default_universe = fetch_dex_universe(info_url, None).await?;
    if let Some(hit) = pick_coin(&default_universe, None, &target, &base) {
        return Ok(Some(hit));
    }

    let dexes = fetch_perp_dexes(info_url).await?;
    let mut best: Option<ResolvedCoin> = None;
    for d in dexes {
        let Ok(universe) = fetch_dex_universe(info_url, Some(&d)).await else {
            continue;
        };
        let Some(hit) = pick_coin(&universe, Some(&d), &target, &base) else {
            continue;
        };
        let better = match &best {
            None => true,
            Some(cur) => hit.max_leverage > cur.max_leverage,
        };
        if better {
            best = Some(hit);
        }
    }
    Ok(best)
}

/// Pick the best match for `target` inside one dex's universe.
///
/// Delisted markets are never candidates — they stay in the universe but
/// cannot be traded, and the old check accepted them (which is how `MATIC`,
/// `RNDR`, `FTM` and `MKR` came to be admitted despite being delisted).
/// Within a dex the first match wins; across dexes the caller keeps the
/// deepest market, which is the one the venue itself would route to.
fn pick_coin(
    universe: &[HlMetaAsset],
    dex: Option<&str>,
    target: &str,
    base: &str,
) -> Option<ResolvedCoin> {
    let asset = universe
        .iter()
        .find(|a| {
            let name = a.name.to_uppercase();
            // On the default dex the wire name is the bare coin; on a HIP-3
            // dex it is `<dex>:<coin>`. Compare the coin half either way.
            let coin_half = name.rsplit(':').next().unwrap_or(&name);
            coin_half == target
        })
        .filter(|a| !a.is_delisted.unwrap_or(false))?;
    Some(ResolvedCoin {
        venue_coin: asset.name.clone(),
        base: base.to_string(),
        dex: dex.map(str::to_string),
        max_leverage: asset.max_leverage.unwrap_or_default(),
        isolated_only: asset.only_isolated.unwrap_or(false)
            || asset.margin_mode.as_deref() == Some("noCross"),
    })
}

/// Backwards-compatible availability probe: `true` when the venue lists a
/// tradeable market for `coin` on ANY perp dex.
///
/// v11.12.19: the check retries TRANSPORT errors (3 attempts, 30 s per attempt
/// plus connect timeout, 1 s/2 s backoff). Slow networks (cold DNS taking
/// seconds through a VPN) used to fail the single 10 s request.
///
/// Prefer [`resolve_venue_coin`] when the caller also needs the wire name.
pub async fn symbol_exists(coin: &str, info_url: &str) -> Result<bool, String> {
    Ok(resolve_venue_coin(coin, info_url).await?.is_some())
}

/// Resolve and return only the wire name, which is what every venue call needs.
pub async fn resolve_venue_coin_name(coin: &str, info_url: &str) -> Result<Option<String>, String> {
    Ok(resolve_venue_coin(coin, info_url)
        .await?
        .map(|r| r.venue_coin))
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
    dex: Option<&str>,
) -> Result<std::collections::HashMap<String, HlDerivativesCtx>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    // HIP-3: the asset contexts come from the dex that OWNS the market, so a
    // `xyz:TLT` instance must poll `{"type":"metaAndAssetCtxs","dex":"xyz"}`.
    // Asking the default dex would return the crypto universe and no TLT row,
    // which reads as "no derivatives telemetry" rather than as an error.
    let request_body = match dex {
        Some(d) => serde_json::json!({ "type": "metaAndAssetCtxs", "dex": d }),
        None => serde_json::json!({ "type": "metaAndAssetCtxs" }),
    };

    let response = client
        .post(info_url)
        .json(&request_body)
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
    // positional; the i-th entry of each refers to the same coin. Only
    // `name` is read, so extra fields in `meta` are ignored.
    let universe: HlDexMetaBody = serde_json::from_value(meta_json)
        .map_err(|e| format!("Failed to parse Hyperliquid meta universe: {e}"))?;
    let universe_index_to_name: Vec<Option<String>> = universe
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
