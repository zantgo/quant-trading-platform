//! Sector classification parity across all three venues.
//!
//! `assign_sector` drives the portfolio exposure layer's per-sector
//! concentration limits. A market that lands in the wrong bucket silently
//! defeats that limit — the limit exists precisely to stop many positions in
//! *the same* risk being treated as independent.
//!
//! ## Why this test exists
//!
//! Bybit's TradFi line (206 equity / 56 ETF / 4 commodity / 3 FX perps)
//! lives in the SAME `category=linear` product as its crypto perps and carries
//! no HIP-3 dex qualifier. The classifier's non-crypto branch was gated on
//! `dex.is_some()`, so it never applied: `AAPLUSDT` reduced to the base
//! `AAPLUSDT`, missed every table, and fell to `"Other"`. Bybit's entire
//! headline catalogue — the markets with the *most* shared exposure — was
//! therefore collapsed into one bucket.
//!
//! The two venue shapes must stay distinct and must not regress into each
//! other:
//!
//! * Hyperliquid `xyz:AAPL` — dex-qualified, so the dex prefix is the
//!   "this is not crypto" signal and must be read BEFORE the crypto table
//!   (`xyz:TAO` is an equity proxy; a bare `TAO` is Bittensor).
//! * Bybit `AAPLUSDT` — no qualifier, so the `{BASE}USDT` quote suffix is
//!   stripped and the bare ticker classified (and `BTCUSDT` must still reduce
//!   to `BTC` and stay crypto).

use portfolio_supervisor::exposure_layer::assign_sector;

#[test]
fn hyperliquid_crypto_venues_are_unaffected() {
    // A bare crypto base on the default dex.
    for (coin, sector) in [
        ("BTC", "Base Chain"),
        ("ETH", "L1 Smart Contract"),
        ("SOL", "Base Chain"),
        ("DOGE", "Meme"),
        ("LINK", "AI / Oracle"),
    ] {
        assert_eq!(
            assign_sector("BTC-USDC", Some(coin)),
            sector,
            "{coin} must keep its crypto sector"
        );
    }
}

#[test]
fn hyperliquid_dex_qualified_tradfi_is_classified_by_its_ticker() {
    for (coin, sector) in [
        ("xyz:AAPL", "Equity"),
        ("xyz:TSLA", "Equity"),
        ("xyz:GOLD", "Commodity"),
        ("xyz:TLT", "Rates / Bond"),
        ("xyz:CL", "Energy"),
        ("xyz:SP500", "Index"),
        ("xyz:EUR", "FX"),
    ] {
        assert_eq!(
            assign_sector("BTC-USDC", Some(coin)),
            sector,
            "{coin} must be classified as {sector}"
        );
    }
}

#[test]
fn the_dex_prefix_is_read_before_the_crypto_table() {
    // The ambiguity the dex qualifier exists to resolve: `TAO` is Bittensor on
    // the default dex and an equity proxy on a builder-deployed one.
    assert_eq!(assign_sector("TAO-USDC", Some("TAO")), "AI / Oracle");
    assert_eq!(assign_sector("TAO-USDC", Some("xyz:TAO")), "Equity");
}

#[test]
fn bybit_crypto_perps_stay_crypto_after_the_usdt_suffix_is_stripped() {
    for (coin, sector) in [
        ("BTCUSDT", "Base Chain"),
        ("ETHUSDT", "L1 Smart Contract"),
        ("SOLUSDT", "Base Chain"),
        ("TAOUSDT", "AI / Oracle"),
    ] {
        assert_eq!(
            assign_sector("BTC-USDT", Some(coin)),
            sector,
            "{coin} must remain crypto ({sector})"
        );
    }
}

#[test]
fn bybit_tradfi_perps_are_no_longer_other() {
    // The regression: every one of these returned "Other" before v11.13.1.
    for (coin, sector) in [
        ("AAPLUSDT", "Equity"),
        ("TSLAUSDT", "Equity"),
        ("NVDAUSDT", "Equity"),
        ("SPYUSDT", "ETF"),
        ("QQQUSDT", "ETF"),
        ("XAUUSDT", "Commodity"),
        ("XAGUSDT", "Commodity"),
        ("CLUSDT", "Energy"),
        ("BRENTOILUSDT", "Energy"),
        ("EURUSDUSDT", "FX"),
        ("TLTUSDT", "Rates / Bond"),
    ] {
        assert_eq!(
            assign_sector("BTC-USDT", Some(coin)),
            sector,
            "{coin} must classify as {sector}, not \"Other\""
        );
    }
}

#[test]
fn bybit_and_hyperliquid_agree_on_the_same_underlying_ticker() {
    // The same asset must bucket identically across venues, otherwise a
    // cross-venue portfolio would split one risk across two limits.
    assert_eq!(
        assign_sector("X-USDC", Some("xyz:GOLD")),
        assign_sector("X-USDT", Some("XAUUSDT"))
    );
    assert_eq!(
        assign_sector("X-USDC", Some("xyz:TLT")),
        assign_sector("X-USDT", Some("TLTUSDT"))
    );
    assert_eq!(
        assign_sector("X-USDC", Some("xyz:CL")),
        assign_sector("X-USDT", Some("CLUSDT"))
    );
}

#[test]
fn an_unclassified_ticker_still_falls_through_to_other() {
    // The fix must not turn every unknown symbol into a TradFi bucket.
    assert_eq!(assign_sector("ZZZ-USDT", Some("ZZZUSDT")), "Other");
    assert_eq!(assign_sector("ZZZ-USDT", None), "Other");
}

#[test]
fn the_pair_key_alone_is_enough_when_no_venue_name_is_supplied() {
    assert_eq!(assign_sector("BTC-USDC", None), "Base Chain");
    assert_eq!(assign_sector("ETH-USDC", None), "L1 Smart Contract");
}
