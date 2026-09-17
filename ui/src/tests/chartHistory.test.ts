// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import type { Time } from 'lightweight-charts';
import { fetchIndicatorHistoryOnce, clearHistoryCache, clearCandleCache, setCachedCandles, getCachedCandles, buildPaintCandles, dedupSortByTime, historyValue, pairsFromHistory, type CandleOHLCV } from '../lib/indicatorHistory';

describe('indicatorHistory (unified)', () => {
    it('fetchIndicatorHistoryOnce_allows_sub_minute_timeframes', async () => {
        // Regression: sub-minute timeframes used to return `null`
        // immediately, which prevented every chart's `setData()` from
        // running on bootstrap. Now the helper always fetches and the
        // charts handle empty / sparse responses naturally.
        //
        // The inline mock does NOT provide a response body so the fetch
        // will fail with a JSON parse error, but the critical assertion
        // is that the function does NOT return `null` — it actually
        // attempts a network request.
        const originalFetch = globalThis.fetch;
        let reqUrl = '';
        globalThis.fetch = async (input: RequestInfo | URL, _init?: RequestInit) => {
            reqUrl = typeof input === 'string' ? input : String(input);
            return new Response('', { status: 404 });
        };
        try {
            const p = fetchIndicatorHistoryOnce('BTC-USDT', 1);
            const result = await p;
            // 404 returns null from the helper because !res.ok.
            expect(result).toBeNull();
            // But the URL was formed correctly and the request was made.
            expect(reqUrl).toContain('timeframe_secs=1');
            expect(reqUrl).toContain('BTC-USDT');
        } finally {
            clearHistoryCache();
            globalThis.fetch = originalFetch;
        }
    });

    it('fetchIndicatorHistoryOnce_returns_null_for_zero_timeframe', async () => {
        expect(await fetchIndicatorHistoryOnce('BTC-USDT', 0)).toBeNull();
    });

    it('fetchIndicatorHistoryOnce_returns_null_for_empty_pairKey', async () => {
        expect(await fetchIndicatorHistoryOnce('', 60)).toBeNull();
    });

    it('clearHistoryCache_is_noop_on_empty_cache', () => {
        expect(() => clearHistoryCache()).not.toThrow();
    });

    it('dedupSortByTime_sorts_and_removes_duplicates', () => {
        const input = [
            { time: 3 as import('lightweight-charts').Time, value: 30 },
            { time: 1 as import('lightweight-charts').Time, value: 10 },
            { time: 3 as import('lightweight-charts').Time, value: 31 }, // duplicate
            { time: 2 as import('lightweight-charts').Time, value: 20 },
        ];
        const result = dedupSortByTime(input);
        expect(result).toEqual([
            { time: 1 as import('lightweight-charts').Time, value: 10 },
            { time: 2 as import('lightweight-charts').Time, value: 20 },
            { time: 3 as import('lightweight-charts').Time, value: 30 },
        ]);
    });

    it('setCachedCandles_filters_synthetic_candles', () => {
        // Regression: the persistent candle cache must never hold
        // synthetic (SYNTHETIC) candles. They render as flat horizontal
        // lines that span the entire source interval, which on a
        // sub-minute chart looks like a "line of about 1 minute" — the
        // v6.9 "derived from minute-close" render artefact. The fix is
        // a defence-in-depth filter at the cache boundary: any candle
        // carrying a truthy `reconstructed` flag is dropped before
        // being cached, even if the caller forgot to filter.
        const pairKey = 'TEST-SYN-FILTER';
        const timeframe = 3;
        try {
            const realCandle: CandleOHLCV = { time: 1 as Time, open: 100, high: 101, low: 99, close: 100.5 };
            const syntheticCandle: CandleOHLCV = {
                time: 2 as Time, open: 100, high: 100, low: 100, close: 100,
                reconstructed: 'SYNTHETIC',
            };
            const anotherReal: CandleOHLCV = { time: 3 as Time, open: 101, high: 102, low: 100.5, close: 101.5 };
            setCachedCandles(pairKey, timeframe, [realCandle, syntheticCandle, anotherReal]);
            const cached = getCachedCandles(pairKey, timeframe);
            expect(cached).not.toBeNull();
            expect(cached!.length).toBe(2);
            expect(cached!.map((c) => c.time)).toEqual([1, 3]);
            for (const c of cached!) {
                expect(c.reconstructed).toBeUndefined();
            }
        } finally {
            clearCandleCache();
        }
    });

    it('setCachedCandles_filters_PRICE_FALLBACK_synthetic_marker', () => {
        // The chart's prices-only fallback path also tags its entries
        // so they don't pollute the cache across navigation.
        const pairKey = 'TEST-FALLBACK';
        const timeframe = 5;
        try {
            setCachedCandles(pairKey, timeframe, [
                { time: 10 as Time, open: 50, high: 50, low: 50, close: 50, reconstructed: 'PRICE_FALLBACK' },
                { time: 20 as Time, open: 51, high: 51, low: 51, close: 51, reconstructed: 'PRICE_FALLBACK' },
            ]);
            const cached = getCachedCandles(pairKey, timeframe);
            // All entries are synthetic — cache should be empty.
            expect(cached).toBeNull();
        } finally {
            clearCandleCache();
        }
    });

    it('setCachedCandles_preserves_real_candles_unchanged', () => {
        const pairKey = 'TEST-REAL';
        const timeframe = 60;
        try {
            const candles: CandleOHLCV[] = [
                { time: 1 as Time, open: 100, high: 101, low: 99, close: 100.5 },
                { time: 61 as Time, open: 100.5, high: 102, low: 100, close: 101.5 },
            ];
            setCachedCandles(pairKey, timeframe, candles);
            const cached = getCachedCandles(pairKey, timeframe);
            expect(cached).toEqual(candles);
        } finally {
            clearCandleCache();
        }
    });

    it('buildPaintCandles_preserves_synthetic_doji_candles_for_painting', () => {
        // Regression (F5 history wipe): the PriceChart paint path used to
        // filter out every candle the backend marked `reconstructed`.
        // On sparse sub-minute markets those SYNTHETIC doji-fill candles
        // are the majority of the in-memory history buffer, so after a
        // full page reload (empty candle cache → history comes only from
        // `/api/history`) the price chart lost ~90% of its history and
        // rendered long flat bridges instead of the continuous series the
        // live WebSocket coalescer draws. The paint helper must hand the
        // full gap-filled series to the chart; only the persistent cache
        // (`setCachedCandles`) may drop synthetic candles.
        const candles: CandleOHLCV[] = [
            { time: 100 as Time, open: 100, high: 101, low: 99, close: 100.5 },
            { time: 103 as Time, open: 100.5, high: 102, low: 100, close: 101.5, reconstructed: 'SYNTHETIC' },
        ];
        try {
            const painted = buildPaintCandles(candles, 1);
            // The backend-marked doji survives the paint path (and the
            // helper's own gap-fill bridges the 101/102 buckets).
            expect(painted.some((c) => c.reconstructed === 'SYNTHETIC')).toBe(true);
            expect(painted.length).toBe(4);
            // The cache path still filters the synthetic candles out.
            setCachedCandles('TEST-PAINT', 1, painted);
            const cached = getCachedCandles('TEST-PAINT', 1);
            expect(cached).not.toBeNull();
            for (const c of cached!) {
                expect(c.reconstructed).toBeUndefined();
            }
        } finally {
            clearCandleCache();
        }
    });

    it('ema_lines_do_not_render_from_raw_fallback_when_closed_candles_are_insufficient', () => {
        // PRI-10 (v6.10.7): toggling the EMA overlays on a chart whose
        // closed-candle count is below the per-line gate must NOT draw the
        // lines. The `ema_stack` raw series (fast EMA) may exist while the
        // `medium/slow/long` sub-series are absent (partial ribbon); the
        // history layer must return `undefined` for the missing sub-series
        // (no fallback to the raw series), so `pairsFromHistory` yields an
        // empty series and PriceChart's `setData` guard skips it — the same
        // "no data → nothing renders" contract every other overlay follows.
        const hist = {
            times: [100, 101, 102, 103],
            values: {
                'ema_stack': [64980.0, 64985.0, 64990.0, 64995.0],
                'ema_stack.fast': [64980.0, 64985.0, 64990.0, 64995.0],
            },
            candleTimes: [],
            candles: { open: [], high: [], low: [], close: [], volume: [] },
            prices: [],
            fetchedAtMs: Date.now(),
        } as never;

        // The fast line has real sub-series data → renders.
        expect(historyValue(hist, 'ema_stack', 'fast')).toHaveLength(4);
        // medium/slow/long are absent → undefined (NOT the raw close/fast
        // series — that was the price-following-lines bug).
        expect(historyValue(hist, 'ema_stack', 'medium')).toBeUndefined();
        expect(historyValue(hist, 'ema_stack', 'slow')).toBeUndefined();
        expect(historyValue(hist, 'ema_stack', 'long')).toBeUndefined();
        // pairsFromHistory must produce NO seed points for the gated lines.
        expect(pairsFromHistory(hist, 'ema_stack', 'medium')).toEqual([]);
        expect(pairsFromHistory(hist, 'ema_stack', 'long')).toEqual([]);
        // The no-subKey lookup still reads the raw series (Metrics table).
        expect(historyValue(hist, 'ema_stack')).toHaveLength(4);
    });

    it('P0_keepalive_appendLiveCandle_preserves_history_across_tab_switch', async () => {
        const { appendLiveCandle, getResolvedHistory, ingestLiveSnapshot } = await import('../lib/indicatorHistory');
        const pairKey = 'TEST-KEEPALIVE-1S';
        const slot = 'micro';
        const tf = 1;
        try {
            clearHistoryCache();
            clearCandleCache();
            // Simulate cold 1s start: initial fetch empty (no cache).
            expect(getCachedCandles(pairKey, tf, slot)).toBeNull();
            expect(getResolvedHistory(pairKey, tf, slot)).toBeNull();
            // Live candles accumulate via WS (completed 1s bars).
            for (let i = 0; i < 5; i++) {
                const ts = 1_718_000_000 + i;
                appendLiveCandle(pairKey, tf, slot, { time: ts as Time, open: 100 + i, high: 101 + i, low: 99 + i, close: 100.5 + i });
                ingestLiveSnapshot(pairKey, tf, slot, {
                    timestamp: ts,
                    is_completed: true,
                    close: String(100.5 + i),
                    open: String(100 + i),
                    high: String(101 + i),
                    low: String(99 + i),
                    volume: '1.5',
                    indicators: {
                        rsi: { raw_value: 55 + i, state_label: 'NEUTRAL', values: null } as unknown as Record<string, unknown>,
                        ema_stack: { raw_value: 100 + i, state_label: 'CONSOLIDATED', values: { fast: 100 + i } } as unknown as Record<string, unknown>,
                    },
                    quality_envelope: { is_gap_filled: false },
                } as unknown as Record<string, unknown>);
            }
            // Both caches now have 5 live entries, surviving a tab switch.
            const cached = getCachedCandles(pairKey, tf, slot);
            expect(cached).not.toBeNull();
            expect(cached!.length).toBe(5);
            expect(cached!.map((c) => c.time)).toEqual([1_718_000_000, 1_718_000_001, 1_718_000_002, 1_718_000_003, 1_718_000_004].map((t) => t as unknown as Time));
            const hist = getResolvedHistory(pairKey, tf, slot);
            expect(hist).not.toBeNull();
            expect(hist!.times).toHaveLength(5);
            expect(hist!.candleTimes).toHaveLength(5);
            expect(historyValue(hist, 'rsi')).toHaveLength(5);
            expect(pairsFromHistory(hist, 'rsi')).toHaveLength(5);
            // fetchIndicatorHistoryOnce should now return the live-mutated history, not empty.
            const fetched = await fetchIndicatorHistoryOnce(pairKey, tf, slot);
            expect(fetched).not.toBeNull();
            expect(fetched!.times).toHaveLength(5);
            expect(fetched).toBe(hist); // same mutated reference
        } finally {
            clearHistoryCache();
            clearCandleCache();
        }
    });

    it('P0_keepalive_synthetic_candles_never_enter_candle_cache', async () => {
        const { appendLiveCandle, ingestLiveSnapshot, getResolvedHistory } = await import('../lib/indicatorHistory');
        const pairKey = 'TEST-KEEPALIVE-SYN';
        const slot = 'micro';
        const tf = 1;
        try {
            clearHistoryCache();
            clearCandleCache();
            // Synthetic doji (gap-fill heartbeat) must not pollute candleCache
            // but is kept in the paint path via indicatorHistory's reconstructed flag.
            appendLiveCandle(pairKey, tf, slot, { time: 1_718_000_010 as Time, open: 100, high: 100, low: 100, close: 100, reconstructed: 'SYNTHETIC' });
            expect(getCachedCandles(pairKey, tf, slot)).toBeNull();
            // But ingestLiveSnapshot with is_gap_filled=true goes to history's
            // candleReconstructed array, not to candleCache.
            ingestLiveSnapshot(pairKey, tf, slot, {
                timestamp: 1_718_000_011,
                is_completed: true,
                close: '100',
                open: '100',
                high: '100',
                low: '100',
                volume: '0',
                indicators: {},
                quality_envelope: { is_gap_filled: true },
            } as unknown as Record<string, unknown>);
            const hist = getResolvedHistory(pairKey, tf, slot);
            expect(hist).not.toBeNull();
            expect(hist!.candleReconstructed?.[hist!.candleReconstructed.length - 1]).toBe('SYNTHETIC');
            // A real candle after synthetic should be cached.
            appendLiveCandle(pairKey, tf, slot, { time: 1_718_000_012 as Time, open: 101, high: 102, low: 100, close: 101.5 });
            expect(getCachedCandles(pairKey, tf, slot)!.length).toBe(1);
        } finally {
            clearHistoryCache();
            clearCandleCache();
        }
    });

    it('P0_keepalive_dedup_and_cap', async () => {
        const { appendLiveCandle, getResolvedHistory, ingestLiveSnapshot } = await import('../lib/indicatorHistory');
        const pairKey = 'TEST-KEEPALIVE-CAP';
        const slot = 'micro';
        const tf = 1;
        try {
            clearHistoryCache();
            clearCandleCache();
            // Dedup: same timestamp twice = one entry (replace).
            appendLiveCandle(pairKey, tf, slot, { time: 1_718_000_100 as Time, open: 100, high: 101, low: 99, close: 100 });
            appendLiveCandle(pairKey, tf, slot, { time: 1_718_000_100 as Time, open: 101, high: 102, low: 100, close: 101 });
            expect(getCachedCandles(pairKey, tf, slot)!.length).toBe(1);
            expect(getCachedCandles(pairKey, tf, slot)![0].close).toBe(101);
            // Cap: 1001 live appends → 1000 retained (HIST_BUFFER_MAX).
            for (let i = 1; i <= 1001; i++) {
                ingestLiveSnapshot(pairKey, tf, slot, {
                    timestamp: 1_718_000_200 + i,
                    is_completed: true,
                    close: String(i),
                    open: String(i),
                    high: String(i),
                    low: String(i),
                    volume: '1',
                    indicators: { rsi: { raw_value: i, state_label: 'NEUTRAL' } } as unknown as Record<string, unknown>,
                    quality_envelope: { is_gap_filled: false },
                } as unknown as Record<string, unknown>);
            }
            const hist = getResolvedHistory(pairKey, tf, slot);
            expect(hist!.times.length).toBe(1000);
            expect(hist!.times[0]).toBe(1_718_000_202); // oldest trimmed
        } finally {
            clearHistoryCache();
            clearCandleCache();
        }
    });

    it('MAIN_PARITY_warm_remount_reseeds_overlay_series_from_live_mutated_history', async () => {
        // Regression (main-parity restore): on the main branch every
        // remount re-seeded the price overlay series (EMAs etc.) from
        // `/api/history`, so the historical overlay lines survived TF
        // tab switches. The develop warm-cache path skipped the refetch
        // and left overlays blank until the next live frame. The fix
        // re-seeds from the live-mutated history cache (`historyData`).
        // This test pins the data-layer contract: after a cold history
        // fetch + live ingestion, the resolved history carries the FULL
        // overlay tail (historical + live-appended bars) aligned to
        // `times`, which is exactly what `seedOverlaysFromHistory` feeds
        // to `setData` on a warm remount.
        const { getResolvedHistory, appendLiveCandle, ingestLiveSnapshot, alignedSeriesFromHistory } = await import('../lib/indicatorHistory');
        const pairKey = 'TEST-WARM-OVERLAY';
        const slot = 'micro';
        const tf = 60;
        const baseSec = 1_718_000_000;
        const originalFetch = globalThis.fetch;
        try {
            clearHistoryCache();
            clearCandleCache();
            // Cold boot: /api/history returns 3 completed candles + ema_stack.
            const histTimes = [baseSec - 120, baseSec - 60, baseSec];
            const histCloses = [100, 101, 102];
            const histEmas = [99.5, 100.5, 101.5];
            globalThis.fetch = async (input: RequestInfo | URL, _init?: RequestInit) => {
                const url = typeof input === 'string' ? input : String(input);
                if (url.includes('/api/history')) {
                    return new Response(JSON.stringify({
                        prices: histCloses.map(String),
                        candles: histTimes.map((t, i) => ({
                            time: t * 1000,
                            open: String(histCloses[i]),
                            high: String(histCloses[i]),
                            low: String(histCloses[i]),
                            close: String(histCloses[i]),
                            volume: '1',
                        })),
                        indicator_history: {
                            times: histTimes,
                            indicators: {
                                ema_stack: {
                                    raw: histEmas,
                                    state_label: histTimes.map(() => 'CONSOLIDATED'),
                                    values: { fast: histEmas },
                                },
                            },
                        },
                    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
                }
                return new Response('', { status: 404 });
            };
            const hist = await fetchIndicatorHistoryOnce(pairKey, tf, slot);
            expect(hist).not.toBeNull();
            expect(hist!.times).toHaveLength(3);
            // The cold bootstrap persists the fetched candles into the
            // candle cache (setCachedCandles in PriceChart.svelte).
            setCachedCandles(pairKey, tf, hist!.candleTimes.map((t, i) => ({
                time: t as Time,
                open: hist!.candles.open[i],
                high: hist!.candles.high[i],
                low: hist!.candles.low[i],
                close: hist!.candles.close[i],
            })), slot);
            // Live WS appends two more completed candles + ema values.
            for (let i = 1; i <= 2; i++) {
                const ts = baseSec + i * 60;
                appendLiveCandle(pairKey, tf, slot, { time: ts as Time, open: 102 + i, high: 103 + i, low: 101 + i, close: 102.5 + i });
                ingestLiveSnapshot(pairKey, tf, slot, {
                    timestamp: ts,
                    is_completed: true,
                    close: String(102.5 + i),
                    open: String(102 + i),
                    high: String(103 + i),
                    low: String(101 + i),
                    volume: '1',
                    indicators: {
                        ema_stack: { raw_value: 101.5 + i, state_label: 'CONSOLIDATED', values: { fast: 101.5 + i } },
                    } as unknown as Record<string, unknown>,
                    quality_envelope: { is_gap_filled: false },
                } as unknown as Record<string, unknown>);
            }
            // Third-structure: historical cache holds durable 3, liveRing holds 2, reconciled holds 5
            const cached = getCachedCandles(pairKey, tf, slot);
            expect(cached!.length).toBe(3);
            // LiveRing holds the live tail for >=60 (not polluting historical cache)
            const { getLiveCandles } = await import('../lib/chartData/liveRing');
            const liveCandles = getLiveCandles(pairKey, tf, slot);
            expect(liveCandles!.length).toBe(2);
            const resolved = getResolvedHistory(pairKey, tf, slot);
            expect(resolved).not.toBeNull();
            expect(resolved!.times).toHaveLength(5);
            // The overlay re-seed must span the FULL tail (historical +
            // live), exactly like the main branch's post-refetch seeding.
            const [emaFast] = alignedSeriesFromHistory(resolved, [['ema_stack', 'fast']]);
            expect(emaFast).toHaveLength(5);
            expect(emaFast.map((p) => p.time)).toEqual(
                [...histTimes, baseSec + 60, baseSec + 120].map((t) => t as unknown as Time),
            );
            expect(emaFast.map((p) => p.value)).toEqual([99.5, 100.5, 101.5, 102.5, 103.5]);
        } finally {
            clearHistoryCache();
            clearCandleCache();
            globalThis.fetch = originalFetch;
        }
    });
});
