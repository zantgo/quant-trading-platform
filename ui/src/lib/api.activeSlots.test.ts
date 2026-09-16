// @vitest-environment jsdom
//
// syncInstanceIdsFromList — v11.2 `active_secs` mapping contract:
//   • each `/api/instances` entry's `active_secs: number[]` maps onto
//     `InstanceState.activeSlots` via the inverted duration table,
//   • a missing/empty `active_secs` leaves the store's all-10 default
//     untouched (defensive no-op),
//   • `/api/config`'s `active_timeframes` seeds the settings store.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/svelte';
import { useAppStore } from '../state.svelte';
import { syncInstanceIdsFromList, applyConfigToStore } from './api.svelte';
import { TIMEFRAME_SLOT_KINDS } from '../types';

function jsonResponse(body: unknown): Response {
    return {
        ok: true,
        status: 200,
        json: async () => body,
    } as unknown as Response;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

describe('syncInstanceIdsFromList — active_secs → activeSlots', () => {
    it('maps the fastest-N durations onto activeSlots in ladder order', async () => {
        const app = useAppStore();
        app.initInstance('BTC');
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({
            instances: [
                { id: 'inst_1', pair: 'BTC-USDT', mode: 'observe', active_secs: [1, 3, 5, 15, 30] },
            ],
        })));
        await syncInstanceIdsFromList(app);
        expect(app.instancesMap['BTC-USDT'].activeSlots).toEqual(TIMEFRAME_SLOT_KINDS.slice(0, 5));
    });

    it('keeps the all-10 default when the payload omits active_secs', async () => {
        const app = useAppStore();
        app.initInstance('BTC');
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({
            instances: [{ id: 'inst_1', pair: 'BTC-USDT', mode: 'observe' }],
        })));
        await syncInstanceIdsFromList(app);
        expect(app.instancesMap['BTC-USDT'].activeSlots).toEqual([...TIMEFRAME_SLOT_KINDS]);
    });
});

describe('applyConfigToStore — active_timeframes → settings store', () => {
    it('seeds settings.activeTimeframes from the workspace payload', () => {
        const app = useAppStore();
        applyConfigToStore(app, {
            symbols: ['BTC'],
            instances: [],
            active_timeframes: 7,
        });
        expect(app.settings.activeTimeframes).toBe(7);
    });

    it('falls back to the workspace sub-object shape and leaves the default when absent', () => {
        const app = useAppStore();
        applyConfigToStore(app, {
            symbols: ['BTC'],
            instances: [],
            workspace: { active_timeframes: 3 },
        });
        expect(app.settings.activeTimeframes).toBe(3);

        const app2 = useAppStore();
        app2.settings.activeTimeframes = 5;
        applyConfigToStore(app2, { symbols: ['BTC'], instances: [] });
        expect(app2.settings.activeTimeframes).toBe(5);
    });
});
