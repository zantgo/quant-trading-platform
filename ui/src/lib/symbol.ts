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
 * The longest input the add-instance fields must accept: an optional HIP-3 dex
 * qualifier plus a full-length base. Capping the field at
 * {@link MAX_SYMBOL_CHARS} would silently truncate a qualified ticker before
 * validation ever saw it.
 */
export const MAX_QUALIFIED_SYMBOL_CHARS = MAX_SYMBOL_CHARS * 2 + 1;

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
 * A HIP-3 perp-dex name — `xyz`, `mkts`, `km`. Deliberately narrower than the
 * base rule: dex names are machine identifiers that end up in URLs and venue
 * requests, so they are ASCII alphanumerics and `_` only.
 *
 * Counterpart of the dex half of `core_domain::symbol_rules::split_dex_qualifier`.
 */
const DEX_NAME_RE = /^[A-Za-z0-9_]{1,20}$/;

/**
 * Settlement currencies the venues use.
 *
 * The BASE half being one of these means the string is the venue's
 * `declaredSymbol` wire form (`BTC:USDT`), not a HIP-3 dex request — its dex
 * half is an ordinary ticker like `BTC`, which is a valid dex-name shape. Exact
 * counterpart of `QUOTE_CURRENCIES` in `core_domain::symbol_rules` (gate G18).
 */
const QUOTE_CURRENCIES = ['USDT', 'USDC', 'USD', 'HYPE'];
const isQuoteCurrency = (s: string): boolean =>
    QUOTE_CURRENCIES.some((q) => q.length === s.length && q === s.toUpperCase());

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

/**
 * The rejection message for a specific input.
 *
 * `S&P500` is the reported case: `&` is not alphanumeric, so the rule refuses it
 * — but the venue's real ticker is `SP500`. A generic "invalid ticker" left the
 * operator with no way to proceed; naming the real symbol turns a dead end into
 * a one-character fix.
 */
export function invalidTickerMessage(raw: string): string {
    const stripped = raw.replace(/[^\p{L}\p{N}_]/gu, '').toUpperCase();
    // Only the punctuation that actually shows up in an index name. A bare
    // "all-caps letters and digits" test is far too broad — it fires on
    // `BTC-USDC` and `SP500.1`, which is not helpful advice.
    const looksLikeIndex = /[&.]/.test(raw) && stripped !== raw.toUpperCase();
    if (looksLikeIndex && stripped) {
        return `${INVALID_TICKER_MESSAGE} The venue ticker has no punctuation — the S&P 500 perpetual is SP500, not S&P500.`;
    }
    return INVALID_TICKER_MESSAGE;
}
/**
 * Split an optional HIP-3 `dex:BASE` qualifier off a raw ticker.
 *
 * Hyperliquid runs several perp dexes since HIP-3: the default crypto dex plus
 * builder-deployed ones. The bare `{"type":"meta"}` query returns ONLY the
 * default dex, so every equity / index / commodity / FX perp (`xyz:TLT`,
 * `xyz:GOLD`, `xyz:SP500`) lives on a builder dex and was invisible to the old
 * availability check — which is why those tickers were rejected while BTC and
 * ETH worked. Typing `xyz:TLT` names the market exactly when auto-resolution
 * would be ambiguous.
 *
 * Returns `undefined` when there is no VALID qualifier, so a malformed one
 * degrades to "no qualifier" rather than producing a nonsense base. Exact
 * counterpart of `core_domain::symbol_rules::split_dex_qualifier` (gate G18).
 */
export function splitDexQualifier(
    raw: string,
): { dex: string; base: string } | undefined {
    const trimmed = raw.trim();
    const idx = trimmed.indexOf(':');
    if (idx < 0) return undefined;
    const dex = trimmed.slice(0, idx);
    const base = trimmed.slice(idx + 1);
    if (!DEX_NAME_RE.test(dex)) return undefined;
    if (!BASE_SYMBOL_RE.test(base.toUpperCase())) return undefined;
    if (isQuoteCurrency(base)) return undefined;
    return { dex, base };
}

/** The base half of a possibly-qualified ticker — what the pair key uses. */
export function baseOf(raw: string): string {
    return splitDexQualifier(raw)?.base ?? raw.trim();
}

/** Reassemble a qualified ticker for display (`xyz:TLT`). */
export function withDexQualifier(raw: string): string {
    const s = splitDexQualifier(raw);
    return s ? `${s.dex}:${s.base}` : normalizeBaseSymbol(raw);
}
