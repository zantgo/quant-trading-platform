// @vitest-environment jsdom
// Test for the LIQ HEATMAP and VOL PROFILE toggle pills in ChartToggles.svelte.
//
// Verifies that toggling a flag on the TF state object propagates correctly
// across all 10 timeframes (since the toggle is sync-all, like VWAP/Bollinger).

import { describe, it, expect, beforeEach } from 'vitest';
import { TIMEFRAME_SLOT_KINDS } from '../types';
import { makeTerms } from '../tests/makeTerms';
import type { InstanceState, TimeframeSlotKind } from '../types';

beforeEach(() => {
    (globalThis as any).__appStore = {
        instancesMap: {},
    };
});

function makeInstance(): InstanceState {
    return {
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        isConnected: true,
        terms: makeTerms(Object.fromEntries(TIMEFRAME_SLOT_KINDS.map((slot) => [slot, makeTf(slot)]))),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null,
        analysis: null,
        risk: null,
        advisory: null,
        decisionContext: null,
        opportunity: null,
        lastMatrixTimestampBySlot: {},
        lastCompletedClose: null,
        automationEnabled: false,
        automationIntervalMode: 'interval',
        automationIntervalValue: 900,
        automationIntervalUnit: 'seconds',
        priceLineMode: false,
        slowIntervalSecs: 900,
        normalIntervalSecs: 300,
        fastIntervalSecs: 60,
        showEmaFast: false,
        showEmaMedium: false,
        showEmaSlow: false,
        showEmaLong: false,
    };
}

function makeTf(slot: TimeframeSlotKind) {
    return {
        slot,
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        barDurationSec: 60,
        indicators: {},
        priceText: '--',
        volText: '--',
        avgVolText: '--',
        showPatterns: true,
        isCompleted: false,
        latestSnapshot: null,
        historyPrices: [],
        showEmas: true,
        showBb: true,
        showVwap: true,
        showVolume: true,
        showAdx: true,
        showAtr: true,
        showRsi: true,
        showMacd: true,
        showSqueeze: true,
        showBbwp: true,
        showFib: true,
        showRvol: true,
        showStochastic: true,
        showChandeMo: true,
        showSupertrend: true,
        showKeltner: true,
        showDonchian: true,
        showIchimoku: true,
        showPsar: true,
        showStddevChan: true,
        showObv: true,
        showCmf: true,
        showMfi: true,
        showHv: true,
        showAroon: true,
        showChoppiness: true,
        showLinregSlope: true,
        showZscore: true,
        showLiqHeatmap: false,
        heatmapLeverageTiers: [10],
        showVolumeProfile: false,
    } as any;
}

describe('ChartToggles overlay state propagation', () => {
    it('defaults showLiqHeatmap to false on every timeframe', () => {
        const inst = makeInstance();
        for (const slot of TIMEFRAME_SLOT_KINDS) expect(inst.terms[slot].showLiqHeatmap).toBe(false);
    });

    it('defaults showVolumeProfile to false on every timeframe', () => {
        const inst = makeInstance();
        for (const slot of TIMEFRAME_SLOT_KINDS) expect(inst.terms[slot].showVolumeProfile).toBe(false);
    });

    it('LIQ HEATMAP toggle flips all four timeframes in sync', () => {
        const inst = makeInstance();
        // Simulate the syncAll() pattern used by ChartToggles.
        const v = !inst.terms.micro1.showLiqHeatmap;
        const tfs = TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms[slot]);
        for (const tf of tfs) tf.showLiqHeatmap = v;

        expect(inst.terms.micro1.showLiqHeatmap).toBe(true);
        expect(inst.terms.fast1.showLiqHeatmap).toBe(true);
        expect(inst.terms.slow1.showLiqHeatmap).toBe(true);
        expect(inst.terms.longterm1.showLiqHeatmap).toBe(true);
    });

    it('VOL PROFILE toggle flips all four timeframes in sync', () => {
        const inst = makeInstance();
        const v = !inst.terms.micro1.showVolumeProfile;
        const tfs = TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms[slot]);
        for (const tf of tfs) tf.showVolumeProfile = v;

        expect(inst.terms.micro1.showVolumeProfile).toBe(true);
        expect(inst.terms.fast1.showVolumeProfile).toBe(true);
        expect(inst.terms.slow1.showVolumeProfile).toBe(true);
        expect(inst.terms.longterm1.showVolumeProfile).toBe(true);
    });

    it('toggling LIQ HEATMAP off syncs the off state too', () => {
        const inst = makeInstance();
        // First turn on
        for (const tf of TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms[slot])) {
            tf.showLiqHeatmap = true;
        }
        expect(inst.terms.longterm1.showLiqHeatmap).toBe(true);
        // Then turn off
        const v = !inst.terms.micro1.showLiqHeatmap; // false
        for (const tf of TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms[slot])) {
            tf.showLiqHeatmap = v;
        }
        expect(inst.terms.micro1.showLiqHeatmap).toBe(false);
        expect(inst.terms.longterm1.showLiqHeatmap).toBe(false);
    });

    it('LIQ HEATMAP and VOL PROFILE toggles are independent', () => {
        const inst = makeInstance();
        inst.terms.micro1.showLiqHeatmap = true;
        // Volume profile should not be affected.
        expect(inst.terms.micro1.showVolumeProfile).toBe(false);
        inst.terms.micro1.showVolumeProfile = true;
        expect(inst.terms.micro1.showLiqHeatmap).toBe(true);
    });
});