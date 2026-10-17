use core_domain::normalized::{Exchange, NormalizedCandle, ReconstructionMethod};
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

/// AUDIT-M21: total time budget for the HIP-3 perp-dex sweep in
/// `resolve_venue_coin`.
///
/// Without it, an ambiguous base (one listed on several builder dexs) blocks
/// symbol admission for `PROBE_ATTEMPTS(3) × PROBE_TOTAL_TIMEOUT(30s)` per dex
/// plus backoff — roughly 95 s per dex, unbounded in aggregate. Because the
/// sweep runs inside `add_instance`, and the boot loop retried every spawn error
/// 20 × 30 s, one ambiguous symbol could stall a whole workspace's boot for
/// many minutes with nothing on screen explaining why.
const DEX_SWEEP_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

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
    // AUDIT-M21: bound the sweep. `fetch_dex_universe` retries
    // `PROBE_ATTEMPTS` (3) times with a `PROBE_TOTAL_TIMEOUT` (30s) each plus
    // 1s/2s backoff, i.e. up to ~95s PER DEX, with no aggregate ceiling. The
    // venue currently runs several builder dexs, so a single ambiguous symbol
    // (e.g. `GOLD`, listed on six) could block the admission path for ~10
    // minutes — and it is called from `add_instance`, which the boot loop then
    // retried 20 times, compounding the stall.
    //
    // Total budget across the whole sweep, checked BEFORE each request so we
    // never start a call we cannot afford. A partial sweep is still useful:
    // whatever resolved best so far is returned.
    let sweep_deadline = std::time::Instant::now() + DEX_SWEEP_BUDGET;
    let mut swept: usize = 0;
    for d in dexes {
        let Some(remaining) = sweep_deadline.checked_duration_since(std::time::Instant::now())
        else {
            eprintln!(
                "⚠️  Hyperliquid dex sweep: {}s budget exhausted after {swept} dex(es) — \
                 returning the best match found so far{}. Narrow the search with a \
                 `dex:BASE` qualifier if the asset is listed on a later dex.",
                DEX_SWEEP_BUDGET.as_secs(),
                if best.is_some() { "" } else { " (none)" },
            );
            break;
        };
        swept += 1;
        // Bound the CALL, not just the loop. `fetch_dex_universe` retries
        // PROBE_ATTEMPTS(3) x PROBE_TOTAL_TIMEOUT(30s) + backoff internally
        // (~95 s), so checking only between dexes still let one wedged dex burn
        // the whole per-dex worst case before the next check ran. Wrapping each
        // fetch in the remaining budget makes the TOTAL sweep time bounded.
        let Ok(Ok(universe)) =
            tokio::time::timeout(remaining, fetch_dex_universe(info_url, Some(&d))).await
        else {
            // Either the dex wedged or the sweep budget expired mid-call; both
            // mean "stop sweeping", and a partial sweep still returns whatever
            // resolved best so far.
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
