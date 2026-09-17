// @vitest-environment jsdom
//
// syncInstanceIdsFromList — v11.9 `active_secs` mapping contract:
//   • each `/api/instances` entry's `active_secs: number[]` maps onto
//     `InstanceState.activeDurations` (canonical ascending order),
//   • a missing/empty `active_secs` leaves the store's full-pool default
//     untouched (defensive no-op),
//   • `/api/config`'s `timeframes` seeds the settings store.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/svelte';
import { useAppStore } from '../state.svelte';
import { syncInstanceIdsFromList, applyConfigToStore } from './api.svelte';
import { DURATIONS } from '../types';

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

describe('syncInstanceIdsFromList — active_secs → activeDurations', () => {
    it('maps the ACTIVE durations onto activeDurations in ascending order', async () => {
        const app = useAppStore();
        app.initInstance('BTC');
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({
            instances: [
                { id: 'inst_1', pair: 'BTC-USDT', mode: 'observe', active_secs: [1, 3, 5, 15, 30] },
            ],
        })));
        await syncInstanceIdsFromList(app);
        expect(app.instancesMap['BTC-USDT'].activeDurations).toEqual(DURATIONS.slice(0, 5));
    });

    it('keeps the full-pool default when the payload omits active_secs', async () => {
        const app = useAppStore();
        app.initInstance('BTC');
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({
            instances: [{ id: 'inst_1', pair: 'BTC-USDT', mode: 'observe' }],
        })));
        await syncInstanceIdsFromList(app);
        expect(app.instancesMap['BTC-USDT'].activeDurations).toEqual([...DURATIONS]);
    });
});

describe('applyConfigToStore — timeframes → settings store', () => {
    it('seeds settings.timeframes from the workspace payload (ascending, pool-filtered)', () => {
        const app = useAppStore();
        applyConfigToStore(app, {
            symbols: ['BTC'],
            instances: [],
            timeframes: [300, 1, 3600],
        });
        expect(app.settings.timeframes).toEqual([1, 300, 3600]);
    });

    it('falls back to the workspace sub-object shape and leaves the default when absent', () => {
        const app = useAppStore();
        applyConfigToStore(app, {
            symbols: ['BTC'],
            instances: [],
            workspace: { timeframes: [60, 900] },
        });
        expect(app.settings.timeframes).toEqual([60, 900]);

        const app2 = useAppStore();
        app2.settings.timeframes = [1, 3];
        applyConfigToStore(app2, { symbols: ['BTC'], instances: [] });
        expect(app2.settings.timeframes).toEqual([1, 3]);
    });
});
