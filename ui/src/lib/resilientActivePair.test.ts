// @vitest-environment jsdom
// Regression lock for the helper that backs `App.svelte::resilientActivePair`.
//
// The original implementation kept the cache as `$state(null)` and
// mutated it from inside a `$derived.by`. Svelte 5's `set()` throws
// `state_unsafe_mutation` whenever called from inside a reactive
// context whose `current_sources` doesn't include the source being
// mutated — `sources.js:152-163` is the contract. Wrapping the cache
// in `$state(...)` violated the contract; the dashboard then broke
// on every panel close / row click because the derived re-ran and
// threw, marking itself errored.
//
// The fix: extract the cache update into a pure helper and store
// the cache as a plain (non-reactive) variable. These tests lock the
// helper's behaviour across every branch.

import { describe, it, expect } from 'vitest';
import { makeTerms } from '../tests/makeTerms';
import { readFile } from 'node:fs/promises';
import { applyResilientCache, type PairCacheEntry } from './resilientActivePair';
import type { InstanceState } from '../types';

function makePair(symbol: string, priceText = '50000.00'): InstanceState {
    return {
        symbol,
        exchange: 'Hyperliquid',
        isConnected: true,
        terms: makeTerms({ micro1: { slot: 'micro1', symbol, exchange: 'Hyperliquid', barDurationSec: 60, indicators: {}, priceText, volText: '0', avgVolText: '0', showPatterns: true, isCompleted: false, latestSnapshot: null, historyPrices: [], showEmas: false, showBb: false, showVwap: true, showVolume: false, showAdx: false, showAtr: false, showRsi: false, showMacd: false, showSqueeze: false, showBbwp: false, showFib: false, showRvol: false, showStochastic: false, showChandeMo: false, showSupertrend: false, showKeltner: false, showDonchian: false, showIchimoku: false, showPsar: false, showStddevChan: false, showObv: false, showCmf: false, showMfi: false, showHv: false, showAroon: false, showChoppiness: false, showLinregSlope: false, showZscore: false, showLiqHeatmap: false, heatmapLeverageTiers: [10], showVolumeProfile: false, showWilliamsR: false, showCci: false, showForceIdx: false, showFunding: false, showOpenInterest: false, showOiDelta: false, showOrderFlowDepth: false, showDerivativeRibbon: true, showPivotPoints: false, showSupportResistance: false, showSmcStructure: false, showSmcLiquidity: false, showFvgZones: false, showOrderBlocks: false, showAnchoredVwap: false, showSpread: false, showAwesome: false, emaFastVal: 10, emaMediumVal: 50, emaSlowVal: 100, emaLongVal: 200, rsiPeriodVal: 14, macdFastVal: 12, macdSlowVal: 26, macdSignalVal: 9, adxPeriodVal: 14, atrPeriodVal: 14, squeezePeriodVal: 20, bbwpPeriodVal: 20, bbwpLookbackVal: 252, stochKPeriodVal: 18, stochDPeriodVal: 5, stochSPeriodVal: 9, chandemoPeriodVal: 12, supertrendPeriodVal: 10, supertrendMultiplierVal: 3.0, keltnerEmaPeriodVal: 20, keltnerAtrPeriodVal: 10, keltnerMultiplierVal: 2.0, donchianPeriodVal: 20, obvSmoothingVal: 20, cmfPeriodVal: 20, mfiPeriodVal: 14, hvPeriodVal: 20, aroonPeriodVal: 25, chopPeriodVal: 14, linregPeriodVal: 20, zscorePeriodVal: 20, macdExtremeHighVal: 1000, macdExtremeLowVal: -1000, macdContractionVal: 0.30, adxTrendThresholdVal: 20, adxExhaustionThresholdVal: 40, adxSlopeLookbackVal: 3, squeezeMinDurationVal: 5, squeezeBbPeriodVal: 20, squeezeBbStdDevVal: 2.0, squeezeKcPeriodVal: 20, squeezeKcAtrMultVal: 1.5, atrMultiplierVal: 2.0, atrTargetRRVal: 2.5, volumeAvgPeriodVal: 20, rvolInstitutionalVal: 1.5, rvolClimaxVal: 3.0 } }),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null, analysis: null, risk: null, advisory: null,
        decisionContext: null, opportunity: null,
        lastMatrixTimestampBySlot: {}, lastCompletedClose: null,
        automationEnabled: false, automationIntervalMode: 'interval',
        automationIntervalValue: 900, automationIntervalUnit: 'seconds',
        priceLineMode: false, slowIntervalSecs: 900, normalIntervalSecs: 300,
        fastIntervalSecs: 60, showEmaFast: false, showEmaMedium: false,
        showEmaSlow: false, showEmaLong: false,
    };
}

describe('applyResilientCache — helper for the top-bar price block', () => {
    const GRACE_MS = 2000;

    it('returns activePair immediately and seeds the cache', () => {
        const pair = makePair('BTC');
        const cache: PairCacheEntry | null = null;
        const { result, nextCache } = applyResilientCache(pair, 'BTC-USDT', cache, GRACE_MS, 1000);
        expect(result).toBe(pair);
        expect(nextCache).toEqual({ pair, pairKey: 'BTC-USDT', capturedAt: 1000 });
    });

    it('returns the cached pair when activePair is undefined and the cache is fresh', () => {
        const pair = makePair('BTC', '49000.00');
        const cache: PairCacheEntry = { pair, pairKey: 'BTC-USDT', capturedAt: 1000 };
        const { result, nextCache } = applyResilientCache(undefined, 'BTC-USDT', cache, GRACE_MS, 1500);
        expect(result).toBe(pair);
        expect(nextCache, 'fresh cache should be returned by reference, not re-allocated').toBe(cache);
    });

    it('returns undefined once the cache is older than the grace window', () => {
        const pair = makePair('BTC');
        const cache: PairCacheEntry = { pair, pairKey: 'BTC-USDT', capturedAt: 1000 };
        const { result, nextCache } = applyResilientCache(undefined, 'BTC-USDT', cache, GRACE_MS, 1000 + GRACE_MS);
        expect(result).toBeUndefined();
        // The cache is preserved (not cleared) — the next activePair
        // arrival will simply overwrite it.
        expect(nextCache).toBe(cache);
    });

    it('overwrites a stale cache with a new activePair', () => {
        const oldPair = makePair('BTC', '49000.00');
        const staleCache: PairCacheEntry = { pair: oldPair, pairKey: 'BTC-USDT', capturedAt: 0 };
        const newPair = makePair('BTC', '51000.00');
        const { result, nextCache } = applyResilientCache(newPair, 'BTC-USDT', staleCache, GRACE_MS, 5000);
        expect(result).toBe(newPair);
        expect(nextCache).not.toBeNull();
        expect(nextCache!.pair).toBe(newPair);
        expect(nextCache!.capturedAt).toBe(5000);
    });

    it('does NOT mutate the input cache reference (immutability)', () => {
        const pair = makePair('BTC');
        const cache: PairCacheEntry = { pair, pairKey: 'BTC-USDT', capturedAt: 1000 };
        const before = JSON.stringify(cache);
        const { nextCache } = applyResilientCache(undefined, 'BTC-USDT', cache, GRACE_MS, 1500);
        expect(JSON.stringify(cache)).toBe(before);
        // When the helper returns the same reference, the caller can
        // safely identity-check against the previous cache. When it
        // allocates a new one, the old reference is untouched.
        expect(nextCache).toBe(cache);
    });

    it('handles the full toggle cycle: undefined → defined → undefined → fresh cache', () => {
        // Simulates the user's exact flow:
        //   1. open the panel
        //   2. click a row → selectedInstance = 'BTC-USDT'
        //   3. delete the instance → instancesMap['BTC-USDT'] gone
        //   4. the top bar still has a price for 2 s (grace window)
        const pair = makePair('BTC', '50000.00');
        let cache: PairCacheEntry | null = null;

        // Step 2: activePair becomes defined.
        let r = applyResilientCache(pair, 'BTC-USDT', cache, GRACE_MS, 1000);
        expect(r.result).toBe(pair);
        cache = r.nextCache;
        expect(cache?.capturedAt).toBe(1000);

        // Step 3: activePair becomes undefined (delete).
        r = applyResilientCache(undefined, 'BTC-USDT', cache, GRACE_MS, 1500);
        expect(r.result, 'should return cached pair within grace window').toBe(pair);
        cache = r.nextCache;

        // Step 4: still undefined, but past grace — fallback.
        r = applyResilientCache(undefined, null, cache, GRACE_MS, 1000 + GRACE_MS + 1);
        expect(r.result).toBeUndefined();
        cache = r.nextCache;

        // Step 5: a new pair becomes selected.
        const ethPair = makePair('ETH', '3200.00');
        r = applyResilientCache(ethPair, 'ETH-USDT', cache, GRACE_MS, 5000);
        expect(r.result).toBe(ethPair);
        expect(r.nextCache).not.toBeNull();
        expect(r.nextCache!.pair).toBe(ethPair);
    });

    it('boundary: cache exactly at graceMs is considered stale', () => {
        const pair = makePair('BTC');
        const cache: PairCacheEntry = { pair, pairKey: 'BTC-USDT', capturedAt: 1000 };
        // 1000 + 2000 = 3000, which is exactly the boundary. The
        // helper uses strict `<`, so the boundary itself is treated as
        // stale. This locks the contract.
        const { result } = applyResilientCache(undefined, 'BTC-USDT', cache, GRACE_MS, 3000);
        expect(result).toBeUndefined();
    });
});

describe('App.svelte — lastGoodPair is NOT a $state (regression guard)', () => {
    // The bug that broke "close the right panel + click on instance":
    // `let lastGoodPair: ... = $state(null);` followed by writing to
    // it from inside `resilientActivePair`'s `$derived.by` triggered
    // Svelte 5's `state_unsafe_mutation` on every re-run. The fix is
    // to keep `lastGoodPair` as a plain `let` (no `$state`).
    //
    // The runtime tests above mount only AppWorkspacePanel and don't
    // instantiate the App.svelte derived that was throwing, so they
    // would not catch a regression. This source-level guard pins the
    // shape of the declaration so the bug cannot silently come back.

    it('App.svelte declares lastGoodPair without a $state wrapper', async () => {
        // Vitest sets `import.meta.url` to a relative path under jsdom.
        // Build an absolute path from `process.cwd()` (the ui/ dir) to
        // keep this test stable across `vitest run` and `vitest --watch`.
        const { resolve } = await import('node:path');
        const appPath = resolve(process.cwd(), 'src/App.svelte');
        const src = await readFile(appPath, 'utf8');

        // The fix: `let lastGoodPair: PairCacheEntry | null = null;`
        // The bug: `let lastGoodPair: PairCacheEntry | null = $state(null);`
        expect(
            src.match(/let\s+lastGoodPair[^\n]*=\s*\$state\(/),
            'lastGoodPair must NOT be wrapped in $state; the resilientActivePair\n' +
                '$derived.by mutates it on every re-run and $state + derived = state_unsafe_mutation.',
        ).toBeNull();

        // Positive: the helper is imported (so the deletion didn't
        // accidentally remove the wiring that makes the derived pure).
        expect(src).toMatch(/import\s*\{[^}]*applyResilientCache[^}]*\}\s*from\s*['"]\.\/lib\/resilientActivePair['"]/);

        // Positive: the derived actually uses the helper (so a future
        // refactor can't reintroduce the bug by inlining a $state
        // wrapper without also routing through the helper).
        expect(src).toMatch(/applyResilientCache\s*\(/);
    });
});