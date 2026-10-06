// Shared ticker-symbol validation (v11.12.26).
//
// The platform talks to real venues, and venue tickers are NOT ASCII-only —
// `龙虾-USDT` is a legitimate listing. Every input gate used to be an explicit
// `[A-Z0-9]` class or a byte-length check, which rejected those symbols at the
// first step (the launch wizard, where the operator types them) and again on
// the API, so a valid ticker could never reach a pipeline.
//
// This module is the ONE definition, so the wizard, the BTE launcher, the
// workspace panel, the tab header and the watchlist scanner cannot drift apart
// again — they previously carried four divergent copies of the same rule.

/**
 * Maximum symbol length, in CHARACTERS (not bytes).
 *
 * The previous limit was 10 *bytes* enforced with `str::len()`, which is why a
 * four-character CJK ticker (12 bytes) was rejected as "Symbol too long" while
 * a ten-character ASCII one was accepted — the limit never described anything
 * real. 20 characters is a superset of every ASCII symbol previously accepted,
 * so this widens what is allowed without changing any existing behaviour, and
 * it fits the longest names venues actually publish.
 */
export const MAX_SYMBOL_CHARS = 20;

/**
 * A base ticker must be 1–20 characters, each a Unicode letter, a Unicode
 * number, or `_`.
 *
 * `\p{L}` / `\p{N}` under the `u` flag covers Latin, Cyrillic, Greek, CJK,
 * Hangul and Arabic, while still rejecting the characters that would actually
 * break a pair key, a URL or a filename: whitespace, `-`, `/`, `:`, `|`, `.`
 * and the remaining punctuation/symbol classes. The previous `[A-Z0-9]{2,10}`
 * accepted only ASCII and rejected a one-character symbol that is perfectly
 * valid.
 *
 * `_` is admitted because venues publish tickers like `LUNA2_USDC`, and this is
 * the exact counterpart of `core_domain::symbol_rules::is_valid_symbol` (whose
 * `char::is_alphanumeric` is this regex's `\p{L}\p{N}`). Keep the two in step —
 * gate G18 requires CLI and GUI to agree on what a symbol is.
 */
const BASE_SYMBOL_RE = /^[\p{L}\p{N}_]{1,20}$/u;

/** Quote-currency symbols stay strict ASCII (USDT, USDC, BTC …). */
const QUOTE_SYMBOL_RE = /^[A-Z0-9]{1,20}$/;

/**
 * Normalize user input for a BASE symbol: trim, and uppercase.
 *
 * `toUpperCase()` is locale-independent (unlike `toLocaleUpperCase()`), and is
 * a no-op for scripts without case — so CJK is preserved exactly while Latin
 * tickers normalize the way the venues expect.
 */
export function normalizeBaseSymbol(raw: string): string {
    return raw.trim().toUpperCase();
}

/** True when `raw` is a usable base ticker (see {@link BASE_SYMBOL_RE}). */
export function isValidBaseSymbol(raw: string): boolean {
    return BASE_SYMBOL_RE.test(normalizeBaseSymbol(raw));
}

/** True when `raw` is a usable quote currency. */
export function isValidQuoteSymbol(raw: string): boolean {
    return QUOTE_SYMBOL_RE.test(raw.trim().toUpperCase());
}

/**
 * Character count of the normalized base symbol — the number the error
 * messages and the backend limit are expressed in.
 */
export function baseSymbolLength(raw: string): number {
    return [...normalizeBaseSymbol(raw)].length;
}

/** The message the wizard and the launcher both show for a rejected ticker. */
export const INVALID_TICKER_MESSAGE =
    `Invalid ticker. Use 1-${MAX_SYMBOL_CHARS} letters or numbers (any language — e.g. BTC, 龙虾).`;