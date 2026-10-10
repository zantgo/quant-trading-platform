import { describe, it, expect } from 'vitest';
import {
    MAX_QUALIFIED_SYMBOL_CHARS,
    MAX_SYMBOL_CHARS,
    baseOf,
    baseSymbolLength,
    invalidTickerMessage,
    isValidBaseSymbol,
    isValidQuoteSymbol,
    normalizeBaseSymbol,
    splitDexQualifier,
    withDexQualifier,
    INVALID_TICKER_MESSAGE,
} from './symbol';

describe('symbol validation (v11.12.26)', () => {
    it('accepts ordinary ASCII tickers', () => {
        for (const sym of ['BTC', 'ETH', 'SOL', '1000PEPE', 'XBT']) {
            expect(isValidBaseSymbol(sym)).toBe(true);
        }
    });

    it('accepts non-English venue tickers — the reported bug', () => {
        // `龙虾` is a real listing that the old `[A-Z0-9]` class rejected before
        // anything was POSTed, so it could not even be entered on the welcome
        // screen.
        for (const sym of ['龙虾', 'BTC龙', 'Биткоин', '안전', 'Ω', 'Χρύσος']) {
            expect(isValidBaseSymbol(sym), sym).toBe(true);
        }
    });

    it('accepts a one-character ticker (the old rule required two)', () => {
        expect(isValidBaseSymbol('龙')).toBe(true);
        expect(isValidBaseSymbol('X')).toBe(true);
    });

    it('rejects characters that would corrupt a pair key', () => {
        for (const bad of [
            '',
            ' ',
            'BTC USDT',
            'BTC-USDT',
            'BTC/USDT',
            'BTC:USDT',
            'BTC|USDT',
            'BT\tC',
            '!!',
            'BTC.USDT',
        ]) {
            expect(isValidBaseSymbol(bad), bad).toBe(false);
        }
    });

    it('counts the budget in characters, not UTF-16 units or bytes', () => {
        const twenty = '龙'.repeat(MAX_SYMBOL_CHARS);
        const twentyOne = '龙'.repeat(MAX_SYMBOL_CHARS + 1);
        expect(isValidBaseSymbol(twenty)).toBe(true);
        expect(isValidBaseSymbol(twentyOne)).toBe(false);
        // Four CJK characters is 12 UTF-8 bytes — the old `maxlength=10` /
        // `str::len() > 10` pair rejected it.
        expect(baseSymbolLength('龙虾合约')).toBe(4);
        expect('龙虾合约'.length).toBe(4);
    });

    it('normalizes case but preserves scripts without case', () => {
        expect(normalizeBaseSymbol('  btc ')).toBe('BTC');
        expect(normalizeBaseSymbol('龙虾')).toBe('龙虾');
    });

    it('keeps quote currencies strict ASCII', () => {
        expect(isValidQuoteSymbol('USDT')).toBe(true);
        expect(isValidQuoteSymbol('usdc')).toBe(true);
        expect(isValidQuoteSymbol('US D')).toBe(false);
    });

    it('names the any-language rule in the rejection message', () => {
        expect(INVALID_TICKER_MESSAGE).toContain('any language');
    });
});

describe('underscore tickers (parity with Rust is_valid_symbol)', () => {
    // Venues publish tickers like LUNA2_USDC; rejecting them would be safe for
    // the pair key but wrong for the venue.
    it('accepts underscore tickers', () => {
        expect(isValidBaseSymbol('LUNA2_USDC')).toBe(true);
        expect(isValidBaseSymbol('_1INCH')).toBe(true);
    });

    it('still rejects separators that would corrupt a - pair key', () => {
        for (const bad of ['BTC-USDT', 'BTC/USDT', 'BTC:USDT', 'BTC|USDT', 'BTC.USDT', 'BTC USDT']) {
            expect(isValidBaseSymbol(bad)).toBe(false);
        }
    });
});

describe('HIP-3 perp-dex qualifiers', () => {
    // Hyperliquid runs several perp dexes since HIP-3; the equity / index /
    // commodity / FX markets (`xyz:TLT`, `xyz:GOLD`, `xyz:SP500`) live on
    // builder-deployed dexes and were invisible to the default-dex-only
    // availability check.
    it('parses an explicit dex:BASE qualifier', () => {
        expect(splitDexQualifier('xyz:TLT')).toEqual({ dex: 'xyz', base: 'TLT' });
        expect(splitDexQualifier('mkts:USBOND')).toEqual({ dex: 'mkts', base: 'USBOND' });
        expect(splitDexQualifier('XYZ:龙虾')).toEqual({ dex: 'XYZ', base: '龙虾' });
    });

    it('a bare ticker has no qualifier', () => {
        for (const bare of ['BTC', '龙虾', 'LUNA2_USDC']) {
            expect(splitDexQualifier(bare)).toBeUndefined();
        }
    });

    // The venue's `declaredSymbol` wire form is BASE:QUOTE. Reading it as a dex
    // request would ask for a perp dex literally named `BTC`.
    it('does not mistake the venue BASE:QUOTE form for a qualifier', () => {
        for (const declared of ['BTC:USDT', 'ETH:USDC', 'BTC:USD', 'BTC:HYPE']) {
            expect(splitDexQualifier(declared), declared).toBeUndefined();
        }
    });

    it('rejects a malformed qualifier instead of inventing a base', () => {
        for (const bad of ['xyz:', ':TLT', ':', 'xy-z:TLT', 'xyz:BT-C', 'xyz:TLT:USDC']) {
            expect(splitDexQualifier(bad), bad).toBeUndefined();
        }
    });

    it('the base half is what the pair key uses', () => {
        expect(baseOf('xyz:TLT')).toBe('TLT');
        expect(baseOf('BTC')).toBe('BTC');
        // No `:` may reach a pair key, a URL path or config.toml.
        expect(baseOf('xyz:GOLD')).not.toContain(':');
    });

    it('round-trips the display form', () => {
        expect(withDexQualifier('xyz:TLT')).toBe('xyz:TLT');
        expect(withDexQualifier('BTC')).toBe('BTC');
        expect(withDexQualifier('btc')).toBe('BTC');
    });

    it('the input budget leaves room for a qualifier', () => {
        expect(MAX_QUALIFIED_SYMBOL_CHARS).toBeGreaterThan(MAX_SYMBOL_CHARS);
        expect('xyz:TLT'.length).toBeLessThanOrEqual(MAX_QUALIFIED_SYMBOL_CHARS);
    });

    it('names the venue ticker for the reported S&P500 case', () => {
        expect(invalidTickerMessage('S&P500')).toContain('SP500');
        expect(invalidTickerMessage('SP500')).toBe(INVALID_TICKER_MESSAGE);
        expect(invalidTickerMessage('BTC-USDC')).toBe(INVALID_TICKER_MESSAGE);
    });

    // Gate G18 parity corpus with `crates/core-domain/src/symbol_rules.rs`
    // (`dex_qualifier_parity_corpus`).
    it('matches the Rust verdict on the shared corpus', () => {
        for (const ok of ['xyz:TLT', 'mkts:USBOND', 'XYZ:龙虾']) {
            expect(splitDexQualifier(ok), ok).toBeDefined();
        }
        for (const bad of [
            'xyz:',
            ':TLT',
            ':',
            'xy-z:TLT',
            'xyz:BT-C',
            'BTC',
            'BTC:USDT',
            'BTC:HYPE',
        ]) {
            expect(splitDexQualifier(bad), bad).toBeUndefined();
        }
    });

    // The reported working set, end to end through the local gate.
    it('accepts every reported HIP-3 ticker', () => {
        for (const ok of [
            'GOLD', 'SP500', 'EUR', 'CL', 'BRENTOIL', 'SILVER', 'COPPER',
            'SPCX', 'NVDA', 'TSLA', 'AAPL', 'GOOGL',
            'PUMP', 'HYPE', 'ZEC', 'LIT', 'DOGE', 'TAO', 'TLT',
        ]) {
            expect(isValidBaseSymbol(ok), ok).toBe(true);
        }
    });
});
