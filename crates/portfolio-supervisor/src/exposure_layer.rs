use core_domain::portfolio::{CorrelationMap, ExposureMatrix, PositionMatrix};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

pub const MAX_SINGLE_PAIR_EXPOSURE_PCT: f64 = 20.0;
pub const MAX_PORTFOLIO_EXPOSURE_PCT: f64 = 50.0;
pub const MAX_CORRELATION: f64 = 0.8;

pub struct ConcentrationLimits {
    pub max_single_pair_pct: f64,
    pub max_portfolio_pct: f64,
    pub max_correlation: f64,
}

impl Default for ConcentrationLimits {
    fn default() -> Self {
        Self {
            max_single_pair_pct: MAX_SINGLE_PAIR_EXPOSURE_PCT,
            max_portfolio_pct: MAX_PORTFOLIO_EXPOSURE_PCT,
            max_correlation: MAX_CORRELATION,
        }
    }
}

impl ConcentrationLimits {
    /// v7.3: limits from `[workspace.risk_limits]` config — the displayed
    /// cap and the enforced cap are the same number.
    pub fn from_config(cfg: &config_models::RiskLimitsConfig) -> Self {
        Self {
            max_single_pair_pct: cfg.max_single_pair_exposure_pct,
            max_portfolio_pct: cfg.max_portfolio_exposure_pct,
            max_correlation: cfg.max_correlation,
        }
    }
}

/// Classify a market for the sector-concentration breakdown.
///
/// The table is crypto-only, so before HIP-3 every non-crypto ticker collapsed
/// into a single `"Other"` bucket — which made the sector concentration limit
/// meaningless for exactly the markets that share exposure (six equities in one
/// bucket, all commodities in another). Since the venue now resolves equity,
/// index, commodity and FX perps, those get their own buckets.
///
/// `venue_coin` is the resolved wire name (`BTC`, `xyz:TLT`). Its HIP-3 prefix is
/// what distinguishes a builder-deployed market from a same-named default-dex
/// crypto market: `xyz:TAO` is an equity-perp proxy, `TAO` is Bittensor.
pub fn assign_sector(symbol: &str, venue_coin: Option<&str>) -> &'static str {
    // A bare base may still carry a qualifier when the caller passes the raw
    // symbol; otherwise read it from the resolved venue name.
    // Fall back to the pair key when no resolved venue name is supplied. The
    // quote half is stripped FIRST — `xyz:GOLD-USDC` would otherwise fail the
    // qualifier parse (its base half contains the `-` separator) and lose the
    // dex.
    let key_base = symbol.split('-').next().unwrap_or(symbol);
    let (key_dex, key_base) = match core_domain::symbol_rules::split_dex_qualifier(key_base) {
        Some((d, b)) => (Some(d.to_string()), b.to_uppercase()),
        None => (None, key_base.to_uppercase()),
    };
    let (dex, base) = match venue_coin {
        Some(v) => match core_domain::symbol_rules::split_dex_qualifier(v) {
            Some((d, b)) => (Some(d.to_string()), b.to_uppercase()),
            None => (key_dex, v.to_uppercase()),
        },
        None => (key_dex, key_base),
    };

    // Non-crypto perps on a HIP-3 dex. The dex prefix IS the "this is not
    // crypto" signal (those markets exist nowhere else), and it must be
    // consulted BEFORE the crypto table: `xyz:TAO` is an equity proxy while a
    // bare `TAO` is Bittensor.
    if dex.is_some() {
        return classify_non_crypto(&base).unwrap_or("Equity");
    }

    // Crypto classification first — unchanged behaviour for every venue.
    if let Some(sector) = classify_crypto(&base) {
        return sector;
    }

    // Bybit's TradFi line lives in the SAME `category=linear` product as its
    // crypto perps and carries NO dex qualifier, so the branch above never
    // applied to it: `AAPLUSDT` reduced to the base `AAPLUSDT` and fell
    // through to "Other", collapsing Bybit's entire headline catalogue (206
    // equity / 56 ETF / 4 commodity / 3 FX perps) into one bucket and defeating
    // the sector-concentration limits for exactly the markets that share the
    // most exposure. Bybit's wire name is `{BASE}USDT`, so strip the quote and
    // classify the real ticker against BOTH tables — crypto first, so
    // `BTCUSDT` still reduces to `BTC` and stays `Base Chain`.
    if let Some(stripped) = base.strip_suffix("USDT") {
        if let Some(sector) = classify_crypto(stripped).or_else(|| classify_non_crypto(stripped)) {
            return sector;
        }
    }

    "Other"
}

/// Non-crypto asset classes, keyed on the bare ticker.
///
/// Shared by the HIP-3 dex path (prefixed, e.g. `xyz:GOLD`) and Bybit's
/// un-prefixed `{BASE}USDT` line, so the two venues' TradFi catalogues land in
/// the same buckets.
fn classify_non_crypto(base: &str) -> Option<&'static str> {
    // The tickers below are generated from Bybit's own
    // `/v5/market/instruments-info` listing (measured 2026-10: 213 `stock`,
    // 57 `ETF`, 4 `commodity`, 3 `forex`) unioned with Hyperliquid's HIP-3
    // dex markets, so both venues' TradFi catalogues land in the same buckets.
    // Specific buckets are matched BEFORE the ETF and equity tables so a
    // ticker listed as both (TLT, URNM) resolves identically on either venue.
    // A ticker in none of the tables returns None -> "Other", the SAFE
    // direction: it under-claims a sector limit rather than inventing one.
    Some(match base {
        // Commodity
        "GOLD" | "SILVER" | "PLATINUM" | "PALLADIUM" | "COPPER" | "ALUMINIUM" | "NATGAS"
        | "XAU" | "XAG" => "Commodity",
        // Energy
        "CL" | "BZ" | "BRENTOIL" | "WTI" | "USOIL" | "NOK" | "TTF" | "HO" => "Energy",
        // Index
        "SP500" | "USA500" | "US500" | "XYZ100" | "USTECH" | "SMALL2000" | "VIX" | "BVIV"
        | "NIFTY" | "JP225" | "JPN225" | "KR200" | "NAS100" | "NDX" => "Index",
        // FX
        "EUR" | "GBP" | "JPY" | "KRW" | "DXY" | "EURUSD" | "GBPUSD" | "USDJPY" => "FX",
        // Rates / Bond
        "TLT" | "USBOND" | "GLDMINE" | "SHY" | "TMF" | "TBT" => "Rates / Bond",
        // Agriculture
        "WHEAT" | "CORN" | "SOY" | "URANIUM" | "URNM" => "Agriculture",
        // Bybit ETF perps (57 listed; already-checked tickers excluded)
        "ARKK" | "BITO" | "BMNU" | "BWET" | "CONL" | "CSOPSAMSUNG2L" | "CSOPSKHYNIX2L" | "DRAM"
        | "EWJ" | "EWT" | "EWY" | "EWZ" | "GDX" | "IBIT" | "INTW" | "IWM" | "KODEX200" | "KORU"
        | "KSTR" | "LYTE" | "MAGS" | "MSFU" | "MSTU" | "MUU" | "MVLL" | "NCLD" | "NVDL" | "QQQ"
        | "RAM" | "SKDD" | "SKUU" | "SMH" | "SNXX" | "SOXL" | "SOXS" | "SOXX" | "SPCH" | "SPXL"
        | "SPY" | "SQQQ" | "SSPC" | "TQQQ" | "TSLL" | "TZA" | "UVXY" | "XBI" | "XLE" | "XLF"
        | "XLK" | "XLP" | "XLU" | "XLV" | "ZCSH" => "ETF",
        // Single-name equity perps (213 listed)
        "AAL" | "AAOI" | "AAPL" | "ACN" | "ADBE" | "ADI" | "AEHR" | "ALAB" | "AMAT" | "AMC"
        | "AMDSTOCK" | "AMGN" | "AMZN" | "AMZU" | "ANET" | "APLD" | "APPSTOCK" | "ARM" | "ASML"
        | "ASTS" | "AVGO" | "AXTI" | "BABA" | "BAC" | "BBX" | "BE" | "BIIB" | "BMNR" | "BNC"
        | "BOT" | "BRKB" | "BSP" | "BTDR" | "BX" | "BYD" | "CATSTOCK" | "CBRS" | "CGNX"
        | "CIEN" | "CIFR" | "CLSK" | "COHR" | "COIN" | "CORZ" | "COST" | "CRCL" | "CRDO"
        | "CRM" | "CRWD" | "CRWV" | "CSCO" | "CVNA" | "CVXSTOCK" | "CXMT" | "CYPH" | "DDOG"
        | "DELL" | "DJT" | "DKNG" | "EBAY" | "FLEX" | "FLNC" | "FLY" | "FORMSTOCK" | "FUTU"
        | "FWDI" | "GE" | "GEV" | "GIGADEVICE" | "GILD" | "GLW" | "GMESTOCK" | "GOOGL" | "GPRO"
        | "GRAL" | "GS" | "GTLB" | "HANMI" | "HD" | "HIMS" | "HOOD" | "HORIZON" | "HPE"
        | "HUAHONG" | "HUT" | "HYUNDAI" | "IBM" | "INFQ" | "INTC" | "INTU" | "IONQ" | "IREN"
        | "ISRG" | "JD" | "JNJ" | "JPM" | "KLAC" | "KO" | "KUAISHOU" | "LAOPU" | "LENOVO"
        | "LGELECTRONICS" | "LITE" | "LLY" | "LRCX" | "LULU" | "MA" | "MARA" | "MCD" | "MDB"
        | "MEITU" | "MEITUAN" | "MELI" | "META" | "MINIMAX" | "MNST" | "MO" | "MP" | "MRK"
        | "MRNA" | "MRVL" | "MSFT" | "MSTR" | "MU" | "NAVER" | "NBIS" | "NET" | "NFLX" | "NKE"
        | "NOKIA" | "NOW" | "NUSTOCK" | "NVDA" | "NVO" | "OKLO" | "OKTA" | "ON" | "ONDS"
        | "ORCL" | "OUST" | "PANW" | "PATH" | "PBR" | "PDD" | "PENGSTOCK" | "PEP" | "PLTR"
        | "POET" | "POPMART" | "PURR" | "PYPL" | "QCOM" | "QNTX" | "RDDT" | "RDW" | "REGN"
        | "RIOT" | "RIVN" | "RKLB" | "RUM" | "SAMSUNG" | "SAMSUNGEM" | "SDGR" | "SECZ"
        | "SENSETIME" | "SHAZ" | "SHEIN" | "SHOP" | "SKHY" | "SKHYNIX" | "SMCI" | "SMIC"
        | "SNDK" | "SNOW" | "SNPS" | "SOFI" | "SONY" | "SPCX" | "SPOT" | "STXX" | "TEAM"
        | "TEM" | "TENCENT" | "TER" | "TSEM" | "TSLA" | "TSM" | "TTWO" | "TWST" | "TXN"
        | "UBER" | "UMC" | "UNH" | "UNITREE" | "USAR" | "USDEX" | "V" | "VIST" | "VKTX" | "VRT"
        | "VRTXSTOCK" | "VST" | "WDC" | "WENSTOCK" | "WMT" | "XIAOMI" | "XOM" | "YOFC" | "YPF"
        | "ZHIPU" | "ZHONGJI" | "ZM" | "ZS" => "Equity",
        _ => return None,
    })
}

/// Crypto asset classes, keyed on the bare ticker.
fn classify_crypto(base: &str) -> Option<&'static str> {
    Some(match base {
        "BTC" | "SOL" | "AVAX" | "ADA" | "DOT" | "ATOM" => "Base Chain",
        "ARB" | "OP" | "MATIC" | "POL" | "IMX" | "STRK" => "L2 Protocols",
        "UNI" | "AAVE" | "MKR" | "COMP" | "CRV" | "LDO" | "PENDLE" => "DeFi",
        "DOGE" | "SHIB" | "PEPE" | "WIF" | "BONK" | "FLOKI" => "Meme",
        "LINK" | "RNDR" | "FET" | "OCEAN" | "AGIX" | "WLD" => "AI / Oracle",
        "ETH" | "BNB" | "SUI" | "APT" | "SEI" | "NEAR" | "FTM" => "L1 Smart Contract",
        "TAO" => "AI / Oracle",
        _ => return None,
    })
}

pub fn compute_correlation_matrix(price_histories: &HashMap<String, Vec<f64>>) -> CorrelationMap {
    let mut pairs = HashMap::new();
    let symbols: Vec<&String> = price_histories.keys().collect();
    for i in 0..symbols.len() {
        for j in (i + 1)..symbols.len() {
            let x = &price_histories[symbols[i]];
            let y = &price_histories[symbols[j]];
            if let Some(corr) = pearson_correlation(x, y) {
                let key = format!("{}-{}", symbols[i], symbols[j]);
                pairs.insert(key, corr);
            }
        }
    }
    CorrelationMap { pairs }
}

fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len().min(y.len());
    if n < 10 {
        return None;
    }
    let x_slice = &x[x.len() - n..];
    let y_slice = &y[y.len() - n..];
    let mean_x = x_slice.iter().sum::<f64>() / n as f64;
    let mean_y = y_slice.iter().sum::<f64>() / n as f64;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for i in 0..n {
        let dx = x_slice[i] - mean_x;
        let dy = y_slice[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    if var_x == 0.0 || var_y == 0.0 {
        return None;
    }
    Some(cov / (var_x.sqrt() * var_y.sqrt()))
}

/// `venue_coins` maps a workspace pair key (`TLT-USDC`) to the venue wire name
/// the instance resolved to (`xyz:TLT`). It is what lets the sector
/// classification tell a HIP-3 market from a same-named default-dex crypto
/// market. Positions whose pair is absent fall back to the pair key.
pub fn compute_exposure_matrix(
    positions: &[PositionMatrix],
    equity: Decimal,
    venue_coins: Option<&std::collections::HashMap<String, String>>,
) -> ExposureMatrix {
    let mut long_exposure = dec!(0);
    let mut short_exposure = dec!(0);
    let mut symbol_concentration = HashMap::new();
    let mut sector_concentration = HashMap::new();

    for pos in positions {
        let notional = pos.allocated_usd;
        match pos.direction.as_str() {
            "LONG" | "Long" => long_exposure += notional,
            "SHORT" | "Short" => short_exposure += notional,
            _ => {}
        }

        let pct = if equity > dec!(0) {
            (notional / equity) * dec!(100)
        } else {
            dec!(0)
        };
        symbol_concentration.insert(pos.symbol.clone(), pct);

        let sector = assign_sector(
            &pos.symbol,
            venue_coins
                .and_then(|m| m.get(&pos.symbol))
                .map(String::as_str),
        );
        let entry = sector_concentration
            .entry(sector.to_string())
            .or_insert(dec!(0));
        *entry += pct;
    }

    let gross_exposure = long_exposure + short_exposure;
    let net_exposure = long_exposure - short_exposure;

    let net_exposure_pct = if equity > dec!(0) {
        (net_exposure / equity) * dec!(100)
    } else {
        dec!(0)
    };

    let max_single_pair = symbol_concentration
        .values()
        .max()
        .copied()
        .unwrap_or(dec!(0));

    ExposureMatrix {
        gross_exposure,
        net_exposure,
        net_exposure_pct,
        long_exposure,
        short_exposure,
        symbol_concentration,
        sector_concentration,
        max_single_pair_pct: max_single_pair,
        correlation_matrix: CorrelationMap {
            pairs: HashMap::new(),
        },
    }
}

pub fn validate_concentration(
    symbol: &str,
    proposed_size: Decimal,
    equity: Decimal,
    matrix: &ExposureMatrix,
    limits: &ConcentrationLimits,
) -> Result<(), String> {
    let proposed_notional = proposed_size;
    let current_symbol_notional = matrix
        .symbol_concentration
        .get(symbol)
        .map(|pct| {
            if equity > dec!(0) {
                (*pct / dec!(100)) * equity
            } else {
                dec!(0)
            }
        })
        .unwrap_or(dec!(0));

    let new_total = current_symbol_notional + proposed_notional;
    let single_pair_pct = if equity > dec!(0) {
        (new_total / equity * dec!(100)).to_f64().unwrap_or(100.0)
    } else {
        100.0
    };

    if single_pair_pct > limits.max_single_pair_pct {
        return Err(format!(
            "Single-pair concentration {:.1}% exceeds limit {:.1}%",
            single_pair_pct, limits.max_single_pair_pct
        ));
    }

    let new_total_exposure = (matrix.gross_exposure + proposed_notional)
        .to_f64()
        .unwrap_or(0.0);
    let equity_f64 = equity.to_f64().unwrap_or(1.0);
    let portfolio_pct = if equity_f64 > 0.0 {
        (new_total_exposure / equity_f64) * 100.0
    } else {
        100.0
    };

    if portfolio_pct > limits.max_portfolio_pct {
        return Err(format!(
            "Portfolio exposure {:.1}% exceeds limit {:.1}%",
            portfolio_pct, limits.max_portfolio_pct
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_sectors_are_unchanged() {
        assert_eq!(assign_sector("BTC-USDC", Some("BTC")), "Base Chain");
        assert_eq!(assign_sector("ETH-USDC", Some("ETH")), "L1 Smart Contract");
        assert_eq!(assign_sector("DOGE-USDC", Some("DOGE")), "Meme");
        // TAO is Bittensor on the default dex — it must not be demoted to the
        // non-crypto bucket just because an equity proxy shares its ticker.
        assert_eq!(assign_sector("TAO-USDC", Some("TAO")), "AI / Oracle");
    }

    #[test]
    fn hip3_markets_get_their_own_buckets() {
        assert_eq!(assign_sector("GOLD-USDC", Some("xyz:GOLD")), "Commodity");
        assert_eq!(assign_sector("SP500-USDC", Some("xyz:SP500")), "Index");
        assert_eq!(assign_sector("EUR-USDC", Some("xyz:EUR")), "FX");
        assert_eq!(assign_sector("CL-USDC", Some("xyz:CL")), "Energy");
        assert_eq!(assign_sector("TLT-USDC", Some("xyz:TLT")), "Rates / Bond");
        assert_eq!(assign_sector("NVDA-USDC", Some("xyz:NVDA")), "Equity");
    }

    /// The reported confusion: the SAME ticker means different things depending
    /// on the dex, so the venue name — not the pair key — must decide.
    #[test]
    fn the_same_ticker_on_different_dexes_lands_in_different_buckets() {
        assert_eq!(assign_sector("TAO-USDC", Some("TAO")), "AI / Oracle");
        assert_eq!(assign_sector("TAO-USDC", Some("xyz:TAO")), "Equity");
    }

    #[test]
    fn falls_back_to_the_pair_key_when_no_venue_name_is_supplied() {
        assert_eq!(assign_sector("BTC-USDC", None), "Base Chain");
        assert_eq!(assign_sector("xyz:GOLD-USDC", None), "Commodity");
        assert_eq!(assign_sector("SOL-USDC", None), "Base Chain");
    }

    #[test]
    fn concentration_buckets_separate_otherwise_merged_markets() {
        use rust_decimal_macros::dec;
        let mk = |symbol: &str| PositionMatrix {
            position_id: 1,
            symbol: symbol.to_string(),
            direction: "LONG".to_string(),
            size: dec!(1),
            entry_price: dec!(100),
            current_price: dec!(100),
            allocated_usd: dec!(1000),
            unrealized_pnl: dec!(0),
            roi_pct: dec!(0),
            entry_timestamp: 0,
            unrealized_pnl_after_fees: dec!(0),
            stop_loss_price: None,
            take_profit_price: None,
            invalidation_level: None,
            target_profit_ratio: None,
            position_state: Default::default(),
        };
        let equity = dec!(10000);
        let venue = std::collections::HashMap::from([
            ("GOLD-USDC".to_string(), "xyz:GOLD".to_string()),
            ("SP500-USDC".to_string(), "xyz:SP500".to_string()),
            ("EUR-USDC".to_string(), "xyz:EUR".to_string()),
        ]);
        let m = compute_exposure_matrix(
            &[mk("GOLD-USDC"), mk("SP500-USDC"), mk("EUR-USDC")],
            equity,
            Some(&venue),
        );
        // Three distinct buckets — previously all three collapsed into "Other",
        // which made the sector limit meaningless for non-crypto books.
        assert_eq!(m.sector_concentration.len(), 3);
        assert!(m.sector_concentration.contains_key("Commodity"));
        assert!(m.sector_concentration.contains_key("Index"));
        assert!(m.sector_concentration.contains_key("FX"));
        assert!(!m.sector_concentration.contains_key("Other"));
    }
}
