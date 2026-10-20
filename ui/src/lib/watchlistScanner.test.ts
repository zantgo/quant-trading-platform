// @vitest-environment node
// Unit tests for the Watchlist Scanner pure helpers
// (`ui/src/lib/watchlistScanner.ts`). These cover the parsing rules, the
// strict decision matrix, the reason-mapping, and the summary aggregation
// that drives the modal's done-phase cards.

import { describe, it, expect } from 'vitest';
import { clampWaitMinutes, decide, detectBackendErrorKind, parseSymbols, parseSymbolsWithReasons, reasonFor, reasonLabel, summarize, watchlistIsClean, WAIT_WINDOW_DEFAULT, WAIT_WINDOW_MAX, WAIT_WINDOW_MIN, type PairOutcome, type PairOutcomeReason } from './watchlistScanner';
import { MAX_SYMBOL_CHARS as MAX_SYMBOL_LEN } from './symbol';
import type { AlignmentMatrix } from '../types';

function makeAlignment(overrides: Partial<AlignmentMatrix> = {}): AlignmentMatrix {
    return {
        mtf_overall_score: 42,
        mtf_stance: 'bull',
        ...overrides,
    } as AlignmentMatrix;
}

describe('parseSymbols', () => {
    it('accepts whitespace-separated tokens', () => {
        expect(parseSymbols('BTC ETH SOL')).toEqual(['BTC', 'ETH', 'SOL']);
    });

    it('v11.8: no longer accepts commas or # prefixes (spaces only)', () => {
        // Spaces are the only separator, so these are single literal tokens —
        // and they now fail the shared ticker rule instead of being POSTed to
        // the backend verbatim and coming back as a generic error.
        expect(parseSymbols('BTC,ETH')).toEqual([]);
        expect(parseSymbols('#BTC')).toEqual([]);
        const { rejected } = parseSymbolsWithReasons('BTC,ETH #BTC');
        expect(rejected.map((r) => r.token)).toEqual(['BTC,ETH', '#BTC']);
        expect(rejected[0].reason).toMatch(/letters or numbers/);
    });

    it('uppercases tokens', () => {
        expect(parseSymbols('btc eth')).toEqual(['BTC', 'ETH']);
    });

    it('collapses repeated whitespace', () => {
        expect(parseSymbols('BTC   ETH')).toEqual(['BTC', 'ETH']);
    });

    // The budget is shared with the wizard (v11.12.26) so the scanner cannot
    // reject a ticker the add-instance step would accept, or vice versa.
    it('drops tokens longer than the shared character budget', () => {
        expect(MAX_SYMBOL_LEN).toBe(20);
        expect(parseSymbols(`BTC ${'X'.repeat(MAX_SYMBOL_LEN + 1)}`)).toEqual(['BTC']);
        expect(parseSymbols(`BTC ${'X'.repeat(MAX_SYMBOL_LEN)}`)).toEqual([
            'BTC',
            'X'.repeat(MAX_SYMBOL_LEN),
        ]);
        // A non-ASCII ticker is measured in characters, so it is NOT silently
        // dropped for being "too long" the way it used to be.
        expect(parseSymbols('BTC 龙虾合约')).toEqual(['BTC', '龙虾合约']);
    });

    it('dedupes while preserving order', () => {
        expect(parseSymbols('BTC ETH BTC SOL ETH')).toEqual(['BTC', 'ETH', 'SOL']);
    });

    it('returns empty for empty input', () => {
        expect(parseSymbols('')).toEqual([]);
        expect(parseSymbols('   ')).toEqual([]);
    });

    it('accepts an explicit HIP-3 dex:BASE qualifier', () => {
        expect(parseSymbols('xyz:TLT')).toEqual(['xyz:TLT']);
        expect(parseSymbols('XYZ:TLT mkts:USBOND')).toEqual([
            'xyz:TLT',
            'mkts:USBOND',
        ]);
        // The dex half is normalized to the venue's lowercase.
        expect(parseSymbols('XYZ:TLT')).toEqual(['xyz:TLT']);
    });

    it('rejects a malformed qualifier with a reason, and never forwards it', () => {
        // `xyz:` has no base; `bad-dex:TLT` has a `-` in the dex half.
        const { symbols, rejected } = parseSymbolsWithReasons('xyz: bad-dex:TLT BTC');
        expect(symbols).toEqual(['BTC']);
        expect(rejected.map((r) => r.token).sort()).toEqual(['BAD-DEX:TLT', 'XYZ:']);
    });

    it('an unknown dex name is well-formed here and refused by the venue', () => {
        // Syntax is valid locally, so the local gate lets it through and the
        // backend's resolver decides — it must not be dropped client-side.
        expect(parseSymbols('NOTADEX:TLT')).toEqual(['notadex:TLT']);
    });

    it('names the real ticker for the reported S&P500 case', () => {
        const { symbols, rejected } = parseSymbolsWithReasons('BTC S&P500');
        expect(symbols).toEqual(['BTC']);
        expect(rejected).toHaveLength(1);
        expect(rejected[0].token).toBe('S&P500');
        expect(rejected[0].reason).toContain('SP500');
    });

    it('reports every HIP-3 tickers as clean', () => {
        expect(watchlistIsClean('GOLD SP500 EUR CL BRENTOIL SILVER COPPER')).toBe(true);
        expect(watchlistIsClean('SPCX NVDA TSLA AAPL GOOGL')).toBe(true);
        expect(watchlistIsClean('PUMP HYPE ZEC LIT DOGE TAO TLT')).toBe(true);
    });
});

describe('decide', () => {
    it('KEEP once the model has a cross-duration alignment', () => {
        expect(decide(makeAlignment())).toBe('KEEP');
    });

    it('KEEP on a StrongBullish stance', () => {
        expect(decide(makeAlignment({ mtf_stance: 'strong_bull' }))).toBe('KEEP');
    });

    it('KEEP on a StrongBearish stance', () => {
        expect(decide(makeAlignment({ mtf_stance: 'strong_bear' }))).toBe('KEEP');
    });

    it('CR-15: KEEP on a Neutral stance — a flat read is still a reading', () => {
        expect(decide(makeAlignment({ mtf_stance: 'neutral' }))).toBe('KEEP');
    });

    it('WAIT when there is no alignment yet', () => {
        expect(decide(null)).toBe('WAIT');
        expect(decide(undefined)).toBe('WAIT');
    });

    it('WAIT while the cross-duration alignment is still NO_DATA', () => {
        expect(decide(makeAlignment({ mtf_stance: 'unmeasured' }))).toBe('WAIT');
    });

    it('CR-15: never returns DELETE — no input can remove a pair', () => {
        const shapes: Array<AlignmentMatrix | null | undefined> = [
            null,
            undefined,
            makeAlignment({ mtf_stance: 'neutral' }),
            makeAlignment({ mtf_stance: 'unmeasured' }),
        ];
        for (const al of shapes) {
            expect(decide(al)).not.toBe('DELETE');
        }
    });
});

describe('reasonFor', () => {
    it('returns KEEP when verdict is KEEP', () => {
        expect(reasonFor('KEEP', makeAlignment())).toBe('KEEP');
    });

    it('returns NO_MODEL_STATE when the alignment has not landed', () => {
        expect(reasonFor('WAIT', null)).toBe('NO_MODEL_STATE');
        expect(reasonFor('WAIT', undefined)).toBe('NO_MODEL_STATE');
    });

    it('returns NO_CROSS_TF when the alignment is NO_DATA', () => {
        expect(reasonFor('WAIT', makeAlignment({ mtf_stance: 'unmeasured' })))
            .toBe('NO_CROSS_TF');
    });

    it('CR-15: never reports a strategy removal reason', () => {
        expect(reasonFor('WAIT', null)).not.toBe('NO_DECISION');
        expect(reasonFor('WAIT', makeAlignment({ mtf_stance: 'neutral' })))
            .not.toBe('DIRECTION_NEUTRAL');
    });
});

describe('summarize', () => {
    function outcome(overrides: Partial<PairOutcome>): PairOutcome {
        return {
            base: 'BTC',
            pairKey: 'BTC-USDT',
            status: 'done',
            elapsedMs: 100,
            ...overrides,
        };
    }

    it('keeps every added pair and skips only the input-level failures', () => {
        // CR-15: `ETH` (awaiting the model) and `AVAX` (timeout) used to land in
        // `removed`. They are kept now — a model that has not spoken within the
        // window is not grounds for deleting an instance the operator added.
        const results = [
            outcome({ base: 'BTC', pairKey: 'BTC-USDT', reason: 'KEEP' }),
            outcome({ base: 'ETH', pairKey: 'ETH-USDT', reason: 'NO_MODEL_STATE' }),
            outcome({ base: 'SOL', pairKey: 'SOL-USDT', reason: 'DUPLICATE' }),
            outcome({ base: 'AVAX', pairKey: 'AVAX-USDT', reason: 'TIMEOUT' }),
        ];
        const s = summarize(results);
        expect(s.added).toBe(3);
        expect(s.kept.map((r) => r.base)).toEqual(['BTC', 'ETH', 'AVAX']);
        expect(s.removed).toEqual([]);
        expect(s.skipped.map((r) => r.base)).toEqual(['SOL']);
    });

    it('CR-15: no outcome reason can land in removed', () => {
        const every: PairOutcomeReason[] = [
            'KEEP', 'NO_MODEL_STATE', 'NO_CROSS_TF', 'TIMEOUT',
            'DUPLICATE', 'INVALID', 'UNAVAILABLE', 'NETWORK_ERROR',
        ];
        const s = summarize(every.map((reason, i) =>
            outcome({ base: `S${i}`, pairKey: `S${i}-USDT`, reason })));
        expect(s.removed).toEqual([]);
        expect(s.added).toBe(s.kept.length);
    });

    it('sums elapsedMs across all pairs', () => {
        const results = [
            outcome({ base: 'BTC', elapsedMs: 100 }),
            outcome({ base: 'ETH', elapsedMs: 250 }),
        ];
        expect(summarize(results).totalMs).toBe(350);
    });

    it('handles empty input', () => {
        const s = summarize([]);
        expect(s.added).toBe(0);
        expect(s.kept).toEqual([]);
        expect(s.removed).toEqual([]);
        expect(s.skipped).toEqual([]);
        expect(s.totalMs).toBe(0);
    });
});

describe('reasonLabel', () => {
    it('returns human text for each reason', () => {
        expect(reasonLabel('KEEP')).toBe('Kept');
        expect(reasonLabel('NO_MODEL_STATE')).toBe('Model state pending');
        expect(reasonLabel('NO_CROSS_TF')).toBe('Awaiting cross-duration agreement');
        expect(reasonLabel('TIMEOUT')).toBe('Timeout');
        expect(reasonLabel('UNAVAILABLE')).toBe('Unavailable');
        expect(reasonLabel('DUPLICATE')).toBe('Already in workspace');
        expect(reasonLabel('INVALID')).toBe('Invalid');
        expect(reasonLabel('NETWORK_ERROR')).toBe('Network error');
        expect(reasonLabel(undefined)).toBe('Pending');
    });
});

describe('clampWaitMinutes', () => {
    it('keeps in-range values as-is', () => {
        expect(clampWaitMinutes(5)).toBe(5);
        expect(clampWaitMinutes(WAIT_WINDOW_MIN)).toBe(WAIT_WINDOW_MIN);
        expect(clampWaitMinutes(WAIT_WINDOW_MAX)).toBe(WAIT_WINDOW_MAX);
    });

    it('clamps below the minimum to 1', () => {
        expect(clampWaitMinutes(0)).toBe(1);
        expect(clampWaitMinutes(-3)).toBe(1);
    });

    it('clamps above the maximum to 60', () => {
        expect(clampWaitMinutes(120)).toBe(60);
        expect(clampWaitMinutes(999)).toBe(WAIT_WINDOW_MAX);
    });

    it('rounds fractional values', () => {
        expect(clampWaitMinutes(5.6)).toBe(6);
        expect(clampWaitMinutes(2.4)).toBe(2);
    });

    it('falls back to the default for empty / non-finite input', () => {
        expect(clampWaitMinutes(null)).toBe(WAIT_WINDOW_DEFAULT);
        expect(clampWaitMinutes(undefined)).toBe(WAIT_WINDOW_DEFAULT);
        expect(clampWaitMinutes(Number.NaN)).toBe(WAIT_WINDOW_DEFAULT);
    });
});

describe('detectBackendErrorKind', () => {
    it('classifies duplicate-instance errors', () => {
        expect(detectBackendErrorKind('Instance for pair BTC-USDT already exists')).toBe('DUPLICATE');
    });

    it('classifies unknown-symbol errors', () => {
        expect(detectBackendErrorKind("'XYZ' isn't available on Hyperliquid (USDT perpetual futures).")).toBe('UNAVAILABLE');
        expect(detectBackendErrorKind("Couldn't verify 'XYZ' on Hyperliquid right now.")).toBe('UNAVAILABLE');
    });

    it('classifies missing-session errors', () => {
        expect(detectBackendErrorKind('No active session. Initialize a session first.')).toBe('UNAVAILABLE');
    });

    it('classifies other errors as NETWORK_ERROR', () => {
        expect(detectBackendErrorKind('fetch failed')).toBe('NETWORK_ERROR');
        expect(detectBackendErrorKind('')).toBe('NETWORK_ERROR');
        expect(detectBackendErrorKind(undefined)).toBe('NETWORK_ERROR');
    });
});
