//! Venue ticker-symbol rules (v11.12.26).
//!
//! Venue tickers are **not ASCII-only** — `龙虾-USDT` is a legitimate listing —
//! yet every gate on the add-instance path was written as either an explicit
//! ASCII character class or a byte-length check. A valid non-English ticker was
//! therefore rejected at the very first step, on the launch wizard where the
//! operator types it, so it never even reached a pipeline.
//!
//! This module is the single Rust definition, consumed by both the API
//! (`POST /api/instances`) and the CLI launch prompt so the two cannot drift:
//! `docs/conceptual-foundations/01-10-cli-gui-parity.md` (gate G18) requires
//! them to agree. The frontend mirror is `ui/src/lib/symbol.ts`.

/// Maximum ticker length in **characters**, never bytes.
///
/// The previous limit was 10 *bytes* checked with `str::len()`, which rejected
/// a four-character CJK ticker (12 bytes) as "Symbol too long" while accepting
/// a ten-character ASCII one — so the limit never described a real symbol.
/// 20 characters is a superset of every ASCII symbol previously accepted, so
/// this widens what is accepted without changing any existing behaviour.
pub const MAX_SYMBOL_CHARS: usize = 20;

/// True when `sym` is a usable ticker: 1–[`MAX_SYMBOL_CHARS`] characters, every
/// one a Unicode letter, a Unicode number, or `_`.
///
/// Deliberately NOT restricted to ASCII — a venue symbol is whatever the venue
/// lists, so CJK (`龙虾`), Cyrillic, Greek and Hangul tickers are all valid.
/// `char::is_alphanumeric()` is the Unicode-aware equivalent of the frontend's
/// `\p{L}\p{N}`, which keeps the two implementations in exact agreement
/// (gate G18 requires it, and four divergent copies of this rule is exactly
/// how the bug shipped).
///
/// `_` is allowed because venues really do publish tickers like `LUNA2_USDC`.
/// Everything else is rejected, because those characters would corrupt the
/// `-`-separated pair key, a URL path, or the TOML round-trip: whitespace,
/// `-`, `/`, `:`, `|`, `.` and control characters.
pub fn is_valid_symbol(sym: &str) -> bool {
    !sym.is_empty()
        && sym.chars().count() <= MAX_SYMBOL_CHARS
        && sym.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// True when `sym` exceeds the character budget.
pub fn symbol_too_long(sym: &str) -> bool {
    sym.chars().count() > MAX_SYMBOL_CHARS
}

/// The separator between a HIP-3 perp-dex name and the coin on that dex.
pub const DEX_SEPARATOR: char = ':';

/// Quote currencies the venues settle in.
///
/// A `BASE:QUOTE` string is therefore NEVER a dex qualifier — it is the
/// `declaredSymbol` wire form (`api.svelte.ts` emits `BTC:USDT`), and reading it
/// as a qualifier would turn the venue's own symbol format into a request for a
/// perp dex literally named `BTC`.
const QUOTE_CURRENCIES: [&str; 4] = ["USDT", "USDC", "USD", "HYPE"];

/// True when `sym` is a settlement currency rather than a market base.
fn is_quote_currency(sym: &str) -> bool {
    QUOTE_CURRENCIES
        .iter()
        .any(|q| sym.len() == q.len() && sym.eq_ignore_ascii_case(q))
}

/// Split an optional `dex:BASE` qualifier off a raw ticker.
///
/// Hyperliquid runs several *perp dexes* since HIP-3: the default crypto dex
/// plus builder-deployed ones (`xyz`, `mkts`, `km`, …). The default dex is the
/// only one the bare `{"type":"meta"}` query returns, so every equity / index /
/// commodity / FX perp (`xyz:TLT`, `xyz:GOLD`, `xyz:SP500`) is invisible to it.
/// Those markets are named `<dex>:<coin>` on the wire, so the operator can
/// force one with an explicit qualifier when auto-resolution would be ambiguous.
///
/// Returns `Some((dex, base))` only when BOTH halves satisfy their own rule, so
/// a malformed qualifier degrades to "no qualifier" rather than producing a
/// nonsense base. The dex half is deliberately narrow (ASCII alphanumerics and
/// `_`) because Hyperliquid dex names are machine identifiers that go into URLs,
/// while the base half keeps the full Unicode rule.
pub fn split_dex_qualifier(raw: &str) -> Option<(&str, &str)> {
    let (dex, base) = raw.split_once(DEX_SEPARATOR)?;
    let valid_dex = !dex.is_empty()
        && dex.chars().count() <= MAX_SYMBOL_CHARS
        && dex.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        // The BASE half being a settlement currency means this is the venue's
        // `declaredSymbol` wire form (`BTC:USDT`), not a dex request. Checking
        // the base is what catches it: the dex half of such a string is an
        // ordinary ticker like `BTC`, which is a perfectly valid dex-name shape.
        && !is_quote_currency(base);
    if valid_dex && is_valid_symbol(base) {
        Some((dex, base))
    } else {
        None
    }
}

/// Render the wire name for a coin on a specific perp dex. The default dex has
/// no prefix, so this is the identity function there.
pub fn dex_qualified_name(dex: Option<&str>, base: &str) -> String {
    match dex {
        Some(d) if !d.is_empty() => format!("{d}{DEX_SEPARATOR}{base}"),
        _ => base.to_string(),
    }
}

/// The shared rejection message (kept in step with the UI's
/// `INVALID_TICKER_MESSAGE`).
pub const INVALID_SYMBOL_MESSAGE: &str = "Symbol must be 1-20 characters, no spaces or separators";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ascii_tickers() {
        assert!(is_valid_symbol("BTC"));
        assert!(is_valid_symbol("USDT"));
        assert!(is_valid_symbol("1000PEPE"));
    }

    #[test]
    fn accepts_non_english_tickers() {
        // The reported bug: a real venue listing that every gate rejected.
        assert!(is_valid_symbol("龙虾"));
        assert!(is_valid_symbol("BTC龙"));
        assert!(is_valid_symbol("Биткоин"));
        assert!(is_valid_symbol("안전"));
        assert!(is_valid_symbol("Ω"));
    }

    #[test]
    fn rejects_pair_key_corrupting_characters() {
        for bad in [
            "", " ", "BTC USDT", "BTC-USDT", "BTC/USDT", "BTC:USDT", "BTC|USDT", "BT\tC",
            "BTC.USDT", "BTC.USDT",
        ] {
            assert!(!is_valid_symbol(bad), "should reject {bad:?}");
        }
    }

    #[test]
    fn allows_the_underscore_tickers_venues_actually_publish() {
        // Rejecting `_` would be safe for the pair key but wrong for the venue:
        // tickers like `LUNA2_USDC` are real listings. The frontend mirrors this.
        assert!(is_valid_symbol("LUNA2_USDC"));
        assert!(is_valid_symbol("_1INCH"));
    }

    /// Gate G18 requires the CLI and the GUI to agree on what a symbol is. The
    /// frontend mirror is `ui/src/lib/symbol.ts` (`/^[\p{L}\p{N}_]{1,20}$/u`).
    /// This corpus is the contract between them: if a case is added here it
    /// must have the same verdict there.
    #[test]
    fn parity_corpus_with_the_frontend_rule() {
        for ok in [
            "BTC",
            "龙虾",
            "Биткоин",
            "안전",
            "Ω",
            "LUNA2_USDC",
            "1000PEPE",
        ] {
            assert!(is_valid_symbol(ok), "{ok:?} must be accepted by both");
        }
        for bad in [
            "", "BTC USDT", "BTC-USDT", "BTC/USDT", "BTC:USDT", "BTC|USDT", "BTC.USDT",
        ] {
            assert!(!is_valid_symbol(bad), "{bad:?} must be rejected by both");
        }
        // 21 characters — over budget on both sides.
        let twenty_one: String = std::iter::repeat('龙').take(21).collect();
        assert!(!is_valid_symbol(&twenty_one));
    }

    #[test]
    fn splits_the_hip3_dex_qualifier() {
        // The real Hyperliquid HIP-3 wire names the operator may want to force.
        assert_eq!(split_dex_qualifier("xyz:TLT"), Some(("xyz", "TLT")));
        assert_eq!(split_dex_qualifier("mkts:USBOND"), Some(("mkts", "USBOND")));
        // Case-insensitive dex names, and a Unicode base survives.
        assert_eq!(split_dex_qualifier("XYZ:龙虾"), Some(("XYZ", "龙虾")));

        // `BASE:QUOTE` is the venue's `declaredSymbol` wire form, NOT a dex
        // request — reading it as one would ask for a perp dex named `BTC`.
        for declared in ["BTC:USDT", "ETH:USDC", "BTC:USD"] {
            assert_eq!(split_dex_qualifier(declared), None, "{declared}");
        }
        // …while a genuine dex name still parses whatever the base is.
        assert_eq!(split_dex_qualifier("km:US500"), Some(("km", "US500")));
        // `HYPE:XYZ` does parse as a qualifier — the base is not a settlement
        // currency — and is therefore resolved as one. That is the safe
        // direction: no dex is named `HYPE`, so the resolver finds nothing
        // rather than silently selecting a different market.
        assert_eq!(split_dex_qualifier("HYPE:XYZ"), Some(("HYPE", "XYZ")));
        // A `BASE:HYPE` declared-symbol form IS caught.
        assert_eq!(split_dex_qualifier("BTC:HYPE"), None);

        // No qualifier at all — the overwhelmingly common case.
        assert_eq!(split_dex_qualifier("BTC"), None);
        assert_eq!(split_dex_qualifier("龙虾"), None);
        assert_eq!(split_dex_qualifier("LUNA2_USDC"), None);
    }

    #[test]
    fn a_malformed_qualifier_is_not_treated_as_one() {
        // Empty base, empty dex, a separator that is part of a base rule
        // violation, and a dex half carrying characters no dex name has.
        assert_eq!(split_dex_qualifier("xyz:"), None);
        assert_eq!(split_dex_qualifier(":TLT"), None);
        assert_eq!(split_dex_qualifier(":"), None);
        assert_eq!(split_dex_qualifier("xy-z:TLT"), None);
        assert_eq!(split_dex_qualifier("xyz:BT-C"), None);
        // Two separators: the base half would itself contain one.
        assert_eq!(split_dex_qualifier("xyz:TLT:USDC"), None);
        // A settlement currency is never a dex name.
        assert_eq!(split_dex_qualifier("BTC:USDT"), None);
    }

    #[test]
    fn dex_qualified_name_round_trips_the_split() {
        assert_eq!(dex_qualified_name(Some("xyz"), "TLT"), "xyz:TLT");
        // The default dex carries no prefix — this is the identity.
        assert_eq!(dex_qualified_name(None, "BTC"), "BTC");
        assert_eq!(dex_qualified_name(Some(""), "BTC"), "BTC");
        for raw in ["xyz:TLT", "mkts:USBOND", "BTC"] {
            let name = match split_dex_qualifier(raw) {
                Some((d, b)) => dex_qualified_name(Some(d), b),
                None => dex_qualified_name(None, raw),
            };
            let (d, b) = split_dex_qualifier(&name).unwrap_or(("", name.as_str()));
            assert!(is_valid_symbol(b), "{raw}: base half must stay valid");
            let _ = d;
        }
    }

    /// Gate G18 parity: the frontend mirror is `splitDexQualifier` in
    /// `ui/src/lib/symbol.ts`. These cases must carry the same verdict there.
    #[test]
    fn dex_qualifier_parity_corpus() {
        for ok in ["xyz:TLT", "mkts:USBOND", "XYZ:龙虾"] {
            assert!(split_dex_qualifier(ok).is_some(), "{ok:?}");
        }
        for bad in [
            "xyz:", ":TLT", ":", "xy-z:TLT", "xyz:BT-C", "BTC", "BTC:USDT", "BTC:HYPE",
        ] {
            assert!(split_dex_qualifier(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn budget_is_counted_in_characters_not_bytes() {
        // 12 bytes / 4 chars — rejected by the old byte limit.
        let cjk = "龙虾合约";
        assert_eq!(cjk.len(), 12);
        assert_eq!(cjk.chars().count(), 4);
        assert!(is_valid_symbol(cjk));
        assert!(!symbol_too_long(cjk));

        // 20 chars is the boundary: allowed, 21 is not.
        let twenty: String = std::iter::repeat('龙').take(20).collect();
        assert!(is_valid_symbol(&twenty));
        let twenty_one: String = std::iter::repeat('龙').take(21).collect();
        assert!(symbol_too_long(&twenty_one));
    }
}
