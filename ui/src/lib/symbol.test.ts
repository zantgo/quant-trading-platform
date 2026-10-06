import { describe, it, expect } from 'vitest';
import {
    MAX_SYMBOL_CHARS,
    baseSymbolLength,
    isValidBaseSymbol,
    isValidQuoteSymbol,
    normalizeBaseSymbol,
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
