// @vitest-environment jsdom
//
// TimeframeSettings — v11.9 duration-keyed active toggles + per-duration
// cards:
//   • a "Active timeframes" toggle grid (one per supported duration, N / 14),
//   • the per-duration cards render only the instance's ACTIVE durations,
//   • the Apply flow POSTs `timeframes: [secs…]` to /api/config alongside
//     the per-duration indicator overrides (same save, no new path).
import { tick } from 'svelte';
import { cleanup, render, fireEvent, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import TimeframeSettings from './TimeframeSettings.svelte';
import { useAppStore } from '../state.svelte';
import { DURATIONS, tfLabel } from '../types';
import { makeTerms } from '../tests/makeTerms';

const fetchCalls: Array<{ url: string; body: any }> = [];

function stubFetch(ok = true) {
    fetchCalls.length = 0;
    vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
        fetchCalls.push({ url, body: init?.body ? JSON.parse(init.body as string) : null });
        return {
            ok,
            status: ok ? 200 : 500,
            headers: new Map(),
            text: async () => '',
        } as unknown as Response;
    }));
}

beforeEach(() => {
    stubFetch();
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.settings.timeframes = [1, 3, 5, 15, 30];
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

function seedPair(activeDurations?: number[]) {
    const app = useAppStore();
    app.initInstance('BTC');
    const pair = app.instancesMap['BTC-USDT'];
    pair.instanceId = 'inst_test';
    pair.terms = makeTerms();
    // Realistic default: the fastest-5 ladder (subset of the 14 pool).
    pair.activeDurations = activeDurations ?? [1, 3, 5, 15, 30];
    return pair;
}

function renderTab(pair: ReturnType<typeof seedPair>) {
    return render(TimeframeSettings, { props: { pair, tabKey: 'inst_test' } });
}

describe('TimeframeSettings — Active timeframes toggles', () => {
    function toggleButtons(container: HTMLElement): HTMLButtonElement[] {
        return Array.from(container.querySelectorAll('button')).filter(
            (b) => b.getAttribute('aria-pressed') !== null,
        );
    }

    it('renders 14 duration toggles, fastest-5 on by default', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        await tick();
        const toggles = toggleButtons(container);
        expect(toggles.length).toBe(DURATIONS.length);
        const on = toggles.filter((b) => b.getAttribute('aria-pressed') === 'true');
        expect(on.length).toBe(5);
        expect(container.textContent).toContain('Active timeframes');
        expect(container.textContent).toContain('saving recharges running instances');
    });

    it('toggling a duration on marks the save and posts timeframes to /api/config', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        await tick();
        const oneMinute = toggleButtons(container).find((b) => b.textContent?.includes('1m'))!;
        expect(oneMinute.getAttribute('aria-pressed')).toBe('false');
        await fireEvent.click(oneMinute);
        expect(oneMinute.getAttribute('aria-pressed')).toBe('true');
        const button = Array.from(container.querySelectorAll('button'))
            .find((b) => b.textContent?.includes('Apply Workspace Configuration'))!;
        expect(button).toBeTruthy();
        await fireEvent.click(button);
        await waitFor(() => {
            const cfgCall = fetchCalls.find((c) => c.url === '/api/config');
            expect(cfgCall).toBeTruthy();
            expect(cfgCall!.body).toEqual({
                timeframes: [1, 3, 5, 15, 30, 60],
            });
        });
        // The per-duration instance-config save still runs through the SAME flow.
        const instCall = fetchCalls.find((c) => c.url.includes('/api/instances/inst_test/config'));
        expect(instCall).toBeTruthy();
        expect(Object.keys(instCall!.body)).toContain('60');
    });

    it('never allows deactivating the last active timeframe', async () => {
        const pair = seedPair([1]);
        const { container } = renderTab(pair);
        await tick();
        const one = toggleButtons(container).find((b) => b.textContent?.includes('1s'))!;
        expect(one.getAttribute('aria-pressed')).toBe('true');
        await fireEvent.click(one);
        // Still on — the min-1 guard refused the toggle.
        expect(one.getAttribute('aria-pressed')).toBe('true');
    });
});

describe('TimeframeSettings — active-duration cards', () => {
    it('renders one card per ACTIVE duration (not the full pool)', async () => {
        const pair = seedPair([1, 5, 60]);
        const { container } = renderTab(pair);
        await tick();
        const titles = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent ?? '');
        for (const slot of [1, 5, 60]) {
            expect(titles.some((t) => t.includes(tfLabel(slot)))).toBe(true);
        }
        // The slowest pool duration (1d) is not active → no card for it.
        expect(titles.some((t) => t.includes(tfLabel(86400)))).toBe(false);
    });

    it('renders all 14 cards when every duration is active', async () => {
        const pair = seedPair([...DURATIONS]);
        const { container } = renderTab(pair);
        await tick();
        const titles = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent ?? '');
        for (const slot of DURATIONS) {
            expect(titles.some((t) => t.includes(tfLabel(slot)))).toBe(true);
        }
    });
});
