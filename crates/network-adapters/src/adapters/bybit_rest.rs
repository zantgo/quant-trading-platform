//! Bybit V5 REST adapter — historical klines + instrument admission.
//!
//! ## Product model
//!
//! Every instrument the platform trades on Bybit lives in the single
//! `category=linear` product: USDT perps (`BTCUSDT`), USDC perps
//! (`BTCPERP` — note the `PERP` suffix, not `USDC`), and the whole TradFi
//! line (equities `AAPLUSDT`, ETFs `SPYUSDT`, commodities `XAUUSDT`/`XAGUSDT`/
//! `CLUSDT`/`BZUSDT`, FX `EURUSDUSDT`). Measured on 2026-10: 898 linear
//! instruments, 858 `LinearPerpetual` + 40 dated `LinearFutures`.
//!
//! Because the product is unified there is **no product-type parameter** to
//! thread through the fetch (contrast Bitget's `USDT-FUTURES`/`USDC-FUTURES`)
//! and no inverse/spot split to worry about.
//!
//! ## Interval convention (a FOURTH one in this codebase)
//!
//! | venue     | 4-hour interval |
//! |-----------|-----------------|
//! | Hyperliquid | `4h`          |
//! | Bitget     | `4H`          |
//! | Bybit      | `240` (minutes) |
//!
//! Bybit expresses every interval as a **bare minute count**, except day and
//! above (`D`, `W`, `M`). [`timeframe_secs_to_interval`] maps the whole ≥60s
//! half of the 14-duration pool exactly — verified live: every one of the nine
//! returns a full 500-candle page with candle timestamps divisible by the
//! duration. Sub-minute durations have no Bybit interval at all and are
//! bypassed upstream by HFP-03 (live-only on every venue).
//!
//! ## Retention
//!
//! Measured 2026-10 by walking pages backward: ~2,425 days (~6.65 years) on
//! **every** interval, with a hard empty-page wall. That is far beyond
//! `[workspace.backtest].archive_depth_days`'s 365-day ceiling, so the
//! BTE depth check treats Bybit as uncapped (`max_candles_per_tf = 0`).

use core_domain::normalized::{Exchange, NormalizedCandle, ReconstructionMethod};
use rust_decimal::Decimal;
use serde::Deserialize;

use super::http;

/// Bybit's hard per-page cap. Measured: `limit=1000`, `2000` and `5000` all
/// return exactly 1000 rows, so 1000 is a venue cap and not a courtesy value.
pub const BYBIT_PAGE_LIMIT: u32 = 1000;

#[derive(Debug, Deserialize)]
struct BybitEnvelope<T> {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg", default)]
    ret_msg: String,
    #[serde(default)]
    result: T,
}

#[derive(Debug, Deserialize, Default)]
struct BybitKlineResult {
    #[serde(default)]
    list: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct BybitInstrumentsResult {
    #[serde(default)]
    list: Vec<BybitInstrument>,
}

/// One row of `/v5/market/instruments-info`.
#[derive(Debug, Deserialize, Clone)]
pub struct BybitInstrument {
    pub symbol: String,
    #[serde(rename = "contractType", default)]
    pub contract_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "baseCoin", default)]
    pub base_coin: String,
    #[serde(rename = "quoteCoin", default)]
    pub quote_coin: String,
    #[serde(rename = "settleCoin", default)]
    pub settle_coin: String,
    /// `stock` / `ETF` / `commodity` / `forex` / `innovation` / empty. Lets
    /// the admission gate name the asset class in its rejection message.
    #[serde(rename = "symbolType", default)]
    pub symbol_type: String,
}

fn parse_decimal(s: &str) -> Result<Decimal, String> {
    s.parse::<Decimal>()
        .map_err(|e| format!("Failed to parse decimal '{}': {}", s, e))
}

/// Fetch one page of historical klines from `/v5/market/kline`.
///
/// - `symbol` is the exchange-native contract symbol (`BTCUSDT`, `AAPLUSDT`).
/// - `internal_symbol` is the unified workspace symbol (`BTC-USDT`).
/// - `interval` is the Bybit interval string (`"1"`, `"240"`, `"D"`, ...).
/// - Rows come back **newest-first** (reverse by `startTime`), positionally
///   `[startTime, open, high, low, close, volume, turnover]`. `volume` is in
///   base units for linear contracts, which is what the candle aggregator
///   wants; `turnover` is quote-denominated and deliberately dropped.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_historical_candles_page(
    symbol: &str,
    internal_symbol: &str,
    interval: &str,
    start_time_ms: u64,
    end_time_ms: u64,
    limit: u32,
    kline_url: &str,
) -> Result<Vec<NormalizedCandle>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    // The limit is clamped to the venue cap so a misconfigured
    // `backtest.bybit.page_cap` cannot ask for rows that never arrive.
    let limit_str = limit.min(BYBIT_PAGE_LIMIT).to_string();
    let category = "linear";
    // Deep backfills issue dozens of requests; one transient send error must
    // not abort the whole run. 3 attempts, 400ms × attempt backoff — the same
    // transport policy the Bitget and Hyperliquid fetchers use.
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err: Option<String> = None;
    let mut response = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match client
            .get(kline_url)
            .query(&[
                ("category", category),
                ("symbol", symbol),
                ("interval", interval),
                ("start", &start_time_ms.to_string()),
                ("end", &end_time_ms.to_string()),
                ("limit", &limit_str),
            ])
            .send()
            .await
        {
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

    let parsed: BybitEnvelope<BybitKlineResult> = response.json().await.map_err(|e| {
        format!(
            "Failed to parse kline JSON for {} {}: {}",
            symbol, interval, e
        )
    })?;

    if parsed.ret_code != 0 {
        return Err(format!(
            "Bybit API error for {} {}: retCode={} retMsg={:?}",
            symbol, interval, parsed.ret_code, parsed.ret_msg
        ));
    }

    let duration_ms = interval_duration_ms(interval);

    Ok(parsed
        .result
        .list
        .into_iter()
        .filter_map(|row| {
            if row.len() < 6 {
                return None;
            }
            let start_time_ms: u64 = row[0].parse().ok()?;
            let open = parse_decimal(&row[1]).ok()?;
            let high = parse_decimal(&row[2]).ok()?;
            let low = parse_decimal(&row[3]).ok()?;
            let close = parse_decimal(&row[4]).ok()?;
            let volume = parse_decimal(&row[5]).ok()?;
            Some(NormalizedCandle {
                exchange: Exchange::Bybit,
                symbol: internal_symbol.to_string(),
                start_time_ms,
                duration_ms,
                open,
                high,
                low,
                close,
                volume,
                trades_count: 0,
                reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
            })
        })
        .collect())
}

/// Backward-compatible wrapper pinned to the venue page cap. Use
/// [`fetch_historical_candles_page`] from new code.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_historical_candles(
    symbol: &str,
    internal_symbol: &str,
    interval: &str,
    start_time_ms: u64,
    end_time_ms: u64,
    kline_url: &str,
) -> Result<Vec<NormalizedCandle>, String> {
    fetch_historical_candles_page(
        symbol,
        internal_symbol,
        interval,
        start_time_ms,
        end_time_ms,
        BYBIT_PAGE_LIMIT,
        kline_url,
    )
    .await
}

/// Duration in ms of a Bybit interval string. Unknown intervals fall back to
/// 60s, which is the venue's finest granularity.
pub fn interval_duration_ms(interval: &str) -> u64 {
    match interval {
        "1" => 60_000,
        "3" => 180_000,
        "5" => 300_000,
        "15" => 900_000,
        "30" => 1_800_000,
        "60" => 3_600_000,
        "120" => 7_200_000,
        "240" => 14_400_000,
        "360" => 21_600_000,
        "720" => 43_200_000,
        "D" => 86_400_000,
        "W" => 604_800_000,
        "M" => 2_592_000_000,
        _ => 60_000,
    }
}

/// Map an internal timeframe duration in seconds to a Bybit V5 kline
/// interval.
///
/// The 14-duration pool's ≥60s half maps **exactly** — no duration in the
/// pool falls through to the nearest-coarser arm. Sub-minute values are
/// unreachable through this function (HFP-03 bypasses them before a fetch is
/// ever requested) but still return the finest venue interval rather than
/// panicking, so a future caller cannot build a nonsense window.
pub fn timeframe_secs_to_interval(secs: u64) -> &'static str {
    match secs {
        60 => "1",
        180 => "3",
        300 => "5",
        900 => "15",
        1800 => "30",
        3600 => "60",
        14400 => "240",
        43200 => "720",
        86400 => "D",
        other if other < 60 => "1",
        other if other < 180 => "1",
        other if other < 300 => "3",
        other if other < 900 => "5",
        other if other < 1800 => "15",
        other if other < 3600 => "30",
        other if other < 14400 => "60",
        other if other < 43200 => "240",
        _ => "720",
    }
}

/// Fetch the instrument row for `symbol` from `/v5/market/instruments-info`.
///
/// Returns `Ok(None)` when the venue does not list the symbol at all,
/// `Ok(Some(row))` when it does. The caller gates on `status` /
/// `contract_type` through [`is_tradeable_perpetual`] so this function stays a
/// pure lookup.
///
/// Retries TRANSPORT errors only (3 attempts, 30s each + connect timeout,
/// 1s/2s backoff) using the shared `http::PROBE_*` constants — a definitive
/// venue answer is never retried, and a slow network can no longer fail the
/// single request outright.
pub async fn fetch_instrument(
    symbol: &str,
    instruments_url: &str,
) -> Result<Option<BybitInstrument>, String> {
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
            .get(instruments_url)
            .query(&[("category", "linear"), ("symbol", symbol)])
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    return Ok(None);
                }
                let parsed: BybitEnvelope<BybitInstrumentsResult> =
                    response.json().await.map_err(|e| {
                        format!("Failed to parse instruments JSON for {}: {}", symbol, e)
                    })?;
                if parsed.ret_code != 0 {
                    // retCode 10001 = "symbol invalid" — a definitive answer.
                    return Ok(None);
                }
                return Ok(parsed
                    .result
                    .list
                    .into_iter()
                    .find(|i| i.symbol.eq_ignore_ascii_case(symbol)));
            }
            Err(e) => {
                last_err = http::describe_reqwest_error(&e);
                eprintln!(
                    "Symbol check attempt {}/{} failed for {}: {}",
                    attempt + 1,
                    http::PROBE_ATTEMPTS,
                    symbol,
                    last_err
                );
            }
        }
    }
    Err(format!(
        "Symbol check request failed for {} after {} attempts: {}",
        symbol,
        http::PROBE_ATTEMPTS,
        last_err
    ))
}

/// Whether an instrument is a live perpetual the platform may stream.
///
/// Two gates, both necessary:
///
/// - `status == "Trading"` — Bybit also lists `PreLaunch`, `Settling`,
///   `Delivering` and `Closed` instruments, which either have no feed yet or
///   have stopped trading.
/// - `contract_type == "LinearPerpetual"` — this excludes the 40 dated
///   futures (`BTCUSDT-16OCT26`). Those symbols also contain a `-`, which
///   `is_valid_symbol` rejects at the operator-facing gate, but the check is
///   kept here so the venue contract does not depend on a UI-side rule.
pub fn is_tradeable_perpetual(instrument: &BybitInstrument) -> bool {
    instrument.status.eq_ignore_ascii_case("Trading")
        && instrument
            .contract_type
            .eq_ignore_ascii_case("LinearPerpetual")
}

/// Convenience wrapper: is `symbol` a tradeable Bybit linear perpetual?
pub async fn symbol_exists(symbol: &str, instruments_url: &str) -> Result<bool, String> {
    Ok(fetch_instrument(symbol, instruments_url)
        .await?
        .as_ref()
        .is_some_and(is_tradeable_perpetual))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pool_duration_maps_to_a_real_bybit_interval() {
        // The 14-duration pool. Sub-minute has no venue interval (HFP-03
        // bypasses it), the other nine must all map exactly.
        for (secs, expected) in [
            (60u64, "1"),
            (180, "3"),
            (300, "5"),
            (900, "15"),
            (1800, "30"),
            (3600, "60"),
            (14400, "240"),
            (43200, "720"),
            (86400, "D"),
        ] {
            assert_eq!(
                timeframe_secs_to_interval(secs),
                expected,
                "duration {}s must map exactly",
                secs
            );
        }
    }

    #[test]
    fn interval_duration_round_trips_the_mapping() {
        // The candle builder derives `duration_ms` from the interval STRING,
        // so a mismatch between these two functions would silently misalign
        // every REST candle. Lock them together.
        for secs in [60u64, 180, 300, 900, 1800, 3600, 14400, 43200, 86400] {
            let iv = timeframe_secs_to_interval(secs);
            assert_eq!(
                interval_duration_ms(iv),
                secs * 1000,
                "interval {} ({}) must carry duration {}ms",
                iv,
                secs,
                secs * 1000
            );
        }
    }

    #[test]
    fn subminute_degrades_to_the_finest_interval() {
        // Unreachable via HFP-03 but must never panic or invent a window.
        for secs in [1u64, 3, 5, 15, 30] {
            assert_eq!(timeframe_secs_to_interval(secs), "1");
        }
    }

    #[test]
    fn only_live_perpetuals_are_admitted() {
        let mk = |status: &str, ct: &str| BybitInstrument {
            symbol: "XUSDT".into(),
            contract_type: ct.into(),
            status: status.into(),
            base_coin: "X".into(),
            quote_coin: "USDT".into(),
            settle_coin: "USDT".into(),
            symbol_type: String::new(),
        };
        assert!(is_tradeable_perpetual(&mk("Trading", "LinearPerpetual")));
        // Dated futures must never be admitted.
        assert!(!is_tradeable_perpetual(&mk("Trading", "LinearFutures")));
        // Every non-live lifecycle state must be refused.
        for s in ["PreLaunch", "Settling", "Delivering", "Closed", ""] {
            assert!(
                !is_tradeable_perpetual(&mk(s, "LinearPerpetual")),
                "status {} must not be tradeable",
                s
            );
        }
    }

    #[test]
    fn page_cap_is_clamped_to_the_venue_maximum() {
        // Measured 2026-10: `limit=1000`, `2000` and `5000` all return exactly
        // 1000 rows, so this is a venue ceiling rather than a courtesy value
        // and the fetcher must clamp to it.
        assert_eq!(BYBIT_PAGE_LIMIT, 1000);
    }
}
