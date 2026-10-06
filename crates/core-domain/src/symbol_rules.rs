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
