// @vitest-environment jsdom
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { buildCandleDebugPayload } from './candleDebug';
import { clearHistoryCache, ingestLiveSnapshot } from './indicatorHistory';
import type { AppStore } from '../state.svelte';
import type { TimeframeSlotKind } from '../types';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_DURATION_SECS } from '../types';
import { makeTerms } from '../tests/makeTerms';

function makeApp(instances: Record<string, { exchange: string; isConnected: boolean }>): AppStore {
    const map: Record<string, any> = {};
    for (const [pairKey, cfg] of Object.entries(instances)) {
        map[pairKey] = {
            symbol: pairKey,
            exchange: cfg.exchange,
            isConnected: cfg.isConnected,
            instanceId: `inst_${pairKey.replace('-', '_')}`,
            terms: makeTerms(
                Object.fromEntries(TIMEFRAME_SLOT_KINDS.map((slot) => [slot, { pipelineState: 'LIVE' }])),
                pairKey,
            ),
        };
    }
    return { instancesMap: map } as unknown as AppStore;
}

function makeSnapshot(ts: number, close: number, opts: { is_completed?: boolean; open?: number; high?: number; low?: number; volume?: number; indicators?: Record<string, unknown> } = {}): Record<string, unknown> {
    return {
        timestamp: ts,
        close,
        open: opts.open ?? close,
        high: opts.high ?? close,
        low: opts.low ?? close,
        volume: opts.volume ?? 10,
        is_completed: opts.is_completed ?? true,
        indicators: opts.indicators ?? {
            rsi: { raw_value: 55.5, normalized: 0.1, state_label: 'Live', values: null },
            ema_stack: { raw_value: close, normalized: 0.2, state_label: 'Live', values: { fast: close - 10, medium: close - 20 } },
        },
        indicator_lifecycle: { rsi: { state: 'Live', bars_seen: 500, bars_required: 14 } },
    };
}

describe('candleDebug', () => {
    beforeEach(() => {
        clearHistoryCache();
        // Reset debug flag
        if (typeof window !== 'undefined') (window as unknown as Record<string, unknown>).__CANDLE_DEBUG_ENABLED__ = undefined;
        try { localStorage.removeItem('candleDebug'); } catch {}
    });

    it('payload contains all instances × 10 slots (background TFs included)', () => {
        const app = makeApp({
            'BTC-USDT': { exchange: 'Hyperliquid', isConnected: true },
            'ETH-USDT': { exchange: 'Bitget', isConnected: true },
        });
        // Seed 5 completed candles per slot via ingestLiveSnapshot (mutates historyData)
        for (let i = 0; i < 5; i++) {
            const snap = makeSnapshot(1_700_000_000 + i, 50000 + i * 10);
            for (const slot of TIMEFRAME_SLOT_KINDS) {
                ingestLiveSnapshot('BTC-USDT', TIMEFRAME_SLOT_DURATION_SECS[slot], slot, snap);
            }
            ingestLiveSnapshot('ETH-USDT', 1, 'micro1', snap);
            ingestLiveSnapshot('ETH-USDT', 3, 'micro2', snap);
        }

        const triggerSnap = makeSnapshot(1_700_000_300, 50100);
        const payload = buildCandleDebugPayload(app, { pairKey: 'BTC-USDT', slot: 'micro1', timeframe_secs: 1, snapshot: triggerSnap });

        expect(payload.trigger.pairKey).toBe('BTC-USDT');
        expect(payload.trigger.slot).toBe('micro1');
        expect(payload.instances.length).toBe(2);
        expect(payload.summary.totalInstances).toBe(2);
        expect(payload.summary.totalTimeframes).toBe(20); // 2 instances × 10
        // Each instance should have 10 timeframes entries (the fixed ladder)
        for (const inst of payload.instances) {
            expect(inst.timeframes.length).toBe(10);
            expect(inst.timeframes.map(t => t.slot)).toEqual([...TIMEFRAME_SLOT_KINDS]);
        }
        // BTC micro1 should have 5 candles seeded
        const btcMicro = payload.instances.find(i => i.pairKey === 'BTC-USDT')!.timeframes.find(t => t.slot === 'micro1')!;
        expect(btcMicro.candleCount).toBe(5);
        expect(btcMicro.candles.length).toBe(5);
        expect(btcMicro.candles[0].close).toBe(50000);
        // Indicator overlays should be present for BTC micro1
        expect(Object.keys(btcMicro.indicatorOverlays).length).toBeGreaterThan(0);
        expect(btcMicro.indicatorOverlays['rsi']).toBeDefined();
        expect(btcMicro.indicatorOverlays['rsi'].length).toBe(5);
        expect(btcMicro.lastOverlayValues['rsi']).toBe(55.5);
        expect(btcMicro.historyTimes.length).toBe(5);
        expect(btcMicro.alignmentOk).toBe(true);
    });

    it('caps at 1000 — oldest evicted (FIFO)', () => {
        const app = makeApp({
            'BTC-USDT': { exchange: 'Hyperliquid', isConnected: true },
        });
        // Push 1100 candles — historyData should trim to 1000
        for (let i = 0; i < 1100; i++) {
            ingestLiveSnapshot('BTC-USDT', 1, 'micro1', makeSnapshot(1_700_000_000 + i, 50000 + i));
        }
        const payload = buildCandleDebugPayload(app, { pairKey: 'BTC-USDT', slot: 'micro1', timeframe_secs: 1, snapshot: makeSnapshot(1_700_001_500, 51000) });
        const micro = payload.instances[0].timeframes.find(t => t.slot === 'micro1')!;
        expect(micro.candleCount).toBe(1000);
        expect(micro.candles.length).toBe(1000);
        expect(micro.timesCount).toBe(1000);
        expect(micro.bufferLen).toBe(1000);
        // Oldest 100 should have been evicted — first timestamp should be the 101st pushed (i=100)
        expect(micro.candles[0].time).toBe(1_700_000_000 + 100);
        expect(micro.historyTimes[0]).toBe(1_700_000_000 + 100);
        expect(payload.summary.cappedAt_1000).toBe(true);
        expect(payload.summary.maxCandlesPerTf).toBe(1000);
    });

    it('includes exchange-aware payload for both Hyperliquid and Bitget', () => {
        const app = makeApp({
            'BTC-USDT': { exchange: 'Hyperliquid', isConnected: true },
            'BTC-USDT:bitget': { exchange: 'Bitget', isConnected: true },
        });
        ingestLiveSnapshot('BTC-USDT', 1, 'micro1', makeSnapshot(1_700_000_000, 50000));
        ingestLiveSnapshot('BTC-USDT:bitget', 1, 'micro1', makeSnapshot(1_700_000_000, 50000));

        const payload = buildCandleDebugPayload(app, { pairKey: 'BTC-USDT', slot: 'micro1', timeframe_secs: 1, snapshot: makeSnapshot(1_700_000_060, 50010) });
        const exchanges = payload.instances.map(i => i.exchange).sort();
        expect(exchanges).toEqual(['Bitget', 'Hyperliquid']);
    });

    it('summary warmupOk_300 and bootstrapOk_500 for ≥60s TFs', () => {
        const app = makeApp({
            'BTC-USDT': { exchange: 'Hyperliquid', isConnected: true },
        });
        // Only 10 candles seeded — below 300 and 500
        for (let i = 0; i < 10; i++) {
            ingestLiveSnapshot('BTC-USDT', 60, 'slow2', makeSnapshot(1_700_000_000 + i * 60, 50000 + i));
            ingestLiveSnapshot('BTC-USDT', 180, 'macro1', makeSnapshot(1_700_000_000 + i * 180, 50000 + i));
        }
        let payload = buildCandleDebugPayload(app, { pairKey: 'BTC-USDT', slot: 'slow2', timeframe_secs: 60, snapshot: makeSnapshot(1_700_000_600, 50010) });
        expect(payload.summary.warmupOk_300).toBe(false);
        expect(payload.summary.bootstrapOk_500).toBe(false);

        // Now seed to 500 for all ≥60s slots
        clearHistoryCache();
        const gte60: TimeframeSlotKind[] = ['slow2', 'macro1', 'macro2', 'longterm1', 'longterm2'];
        for (let i = 0; i < 500; i++) {
            for (const slot of gte60) {
                const secs = TIMEFRAME_SLOT_DURATION_SECS[slot];
                ingestLiveSnapshot('BTC-USDT', secs, slot, makeSnapshot(1_700_000_000 + i * secs, 50000 + i));
            }
        }
        payload = buildCandleDebugPayload(app, { pairKey: 'BTC-USDT', slot: 'slow2', timeframe_secs: 60, snapshot: makeSnapshot(1_700_030_000, 50500) });
        expect(payload.summary.warmupOk_300).toBe(true);
        expect(payload.summary.bootstrapOk_500).toBe(true);
        expect(payload.summary.cappedAt_1000).toBe(true);
    });
});
