// @vitest-environment jsdom
// ChartToggles overlay-flag propagation + the v11.12.20 fix.
//
// The overlay pills sync their flags across all ACTIVE durations. Both the
// flip source and the pill's own light must come from the FASTEST ACTIVE
// duration (`terms[1]` only when 1s actually runs) — a hardcoded 1s slot
// froze the pills on ladders that deactivated 1s (the light never flipped
// because `syncAll` only writes active slots).

import { describe, it, expect, beforeEach } from 'vitest';
import { DURATIONS } from '../types';
import { activeDurations, overlayRepTerm } from '../lib/terms';
import { saveChartOverlays, applyChartOverlays } from '../lib/chartOverlays';
import { makeTerms } from '../tests/makeTerms';
import type { InstanceState } from '../types';

beforeEach(() => {
    (globalThis as any).__appStore = {
        instancesMap: {},
    };
    localStorage.clear();
});

function makeInstance(ladder: number[] = [...DURATIONS]): InstanceState {
    return {
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        isConnected: true,
        activeDurations: ladder,
        terms: makeTerms(Object.fromEntries(DURATIONS.map((slot) => [slot, makeTf(slot)]))),
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

function makeTf(slot: number) {
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

/// Mirrors the ChartToggles handler exactly: flip from the fastest ACTIVE
/// term, then sync every active slot (including that term).
function simulateToggle(inst: InstanceState, flag: string): boolean {
    const rep = overlayRepTerm(inst)!;
    const v = !(rep as any)[flag];
    for (const slot of activeDurations(inst)) (inst.terms[slot] as any)[flag] = v;
    return v;
}

describe('ChartToggles overlay state propagation', () => {
    it('defaults showLiqHeatmap to false on every timeframe', () => {
        const inst = makeInstance();
        for (const slot of DURATIONS) expect(inst.terms[slot].showLiqHeatmap).toBe(false);
    });

    it('defaults showVolumeProfile to false on every timeframe', () => {
        const inst = makeInstance();
        for (const slot of DURATIONS) expect(inst.terms[slot].showVolumeProfile).toBe(false);
    });

    it('LIQ HEATMAP toggle flips all four timeframes in sync', () => {
        const inst = makeInstance();
        simulateToggle(inst, 'showLiqHeatmap');

        expect(inst.terms[1].showLiqHeatmap).toBe(true);
        expect(inst.terms[5].showLiqHeatmap).toBe(true);
        expect(inst.terms[30].showLiqHeatmap).toBe(true);
        expect(inst.terms[900].showLiqHeatmap).toBe(true);
    });

    it('VOL PROFILE toggle flips all four timeframes in sync', () => {
        const inst = makeInstance();
        simulateToggle(inst, 'showVolumeProfile');

        expect(inst.terms[1].showVolumeProfile).toBe(true);
        expect(inst.terms[5].showVolumeProfile).toBe(true);
        expect(inst.terms[30].showVolumeProfile).toBe(true);
        expect(inst.terms[900].showVolumeProfile).toBe(true);
    });

    it('toggling LIQ HEATMAP off syncs the off state too', () => {
        const inst = makeInstance();
        simulateToggle(inst, 'showLiqHeatmap');
        expect(inst.terms[900].showLiqHeatmap).toBe(true);
        simulateToggle(inst, 'showLiqHeatmap');
        expect(inst.terms[1].showLiqHeatmap).toBe(false);
        expect(inst.terms[900].showLiqHeatmap).toBe(false);
    });

    it('LIQ HEATMAP and VOL PROFILE toggles are independent', () => {
        const inst = makeInstance();
        inst.terms[1].showLiqHeatmap = true;
        // Volume profile should not be affected.
        expect(inst.terms[1].showVolumeProfile).toBe(false);
        inst.terms[1].showVolumeProfile = true;
        expect(inst.terms[1].showLiqHeatmap).toBe(true);
    });
});

describe('ChartToggles — fastest-active flag source (v11.12.20)', () => {
    it('uses terms[1] while 1s runs', () => {
        const inst = makeInstance([1, 3, 5, 15]);
        expect(overlayRepTerm(inst)).toBe(inst.terms[1]);
    });

    it('uses the fastest ACTIVE duration on a ladder without 1s', () => {
        const inst = makeInstance([3, 5, 15, 30, 60]);
        expect(overlayRepTerm(inst)).toBe(inst.terms[3]);
    });

    it('a ladder without 1s lights and flips every active slot (regression)', () => {
        const inst = makeInstance([3, 5, 15, 30]);
        simulateToggle(inst, 'showLiqHeatmap');

        // The pill's light source flipped...
        expect(inst.terms[3].showLiqHeatmap).toBe(true);
        // ...every active slot flipped...
        expect(inst.terms[5].showLiqHeatmap).toBe(true);
        expect(inst.terms[15].showLiqHeatmap).toBe(true);
        expect(inst.terms[30].showLiqHeatmap).toBe(true);
        // ...and the inactive 1s slot was never written.
        expect(inst.terms[1].showLiqHeatmap).toBe(false);

        // Toggling again turns every active slot off.
        simulateToggle(inst, 'showLiqHeatmap');
        expect(inst.terms[3].showLiqHeatmap).toBe(false);
        expect(inst.terms[30].showLiqHeatmap).toBe(false);
    });

    it('persists and restores overlay flags with a ladder without 1s', () => {
        const inst = makeInstance([3, 5, 15, 30]);
        simulateToggle(inst, 'showLiqHeatmap');
        saveChartOverlays('BTC-USDT', inst);

        const restored = makeInstance([3, 5, 15, 30]);
        applyChartOverlays('BTC-USDT', restored);

        // The light source reads the saved state...
        expect(overlayRepTerm(restored)?.showLiqHeatmap).toBe(true);
        expect(restored.terms[3].showLiqHeatmap).toBe(true);
        expect(restored.terms[30].showLiqHeatmap).toBe(true);
    });
});
