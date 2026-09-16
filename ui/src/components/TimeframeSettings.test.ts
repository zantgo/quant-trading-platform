// @vitest-environment jsdom
//
// TimeframeSettings — v11.2 active-timeframes knob + active-slot cards:
//   • a compact "Active timeframes" numeric selector (1–10) at the top,
//     seeded from the settings store,
//   • the per-TF cards render only the instance's ACTIVE slots,
//   • the existing Apply flow POSTs `active_timeframes` to /api/config
//     alongside the per-slot indicator overrides (same save, no new path).
import { cleanup, render, fireEvent, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import TimeframeSettings from './TimeframeSettings.svelte';
import { useAppStore } from '../state.svelte';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_LABELS } from '../types';
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
    app.settings.activeTimeframes = 5;
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

function seedPair(activeSlots?: string[]) {
    const app = useAppStore();
    app.initInstance('BTC');
    const pair = app.instancesMap['BTC-USDT'];
    pair.instanceId = 'inst_test';
    pair.terms = makeTerms();
    if (activeSlots) pair.activeSlots = activeSlots as never;
    return pair;
}

function renderTab(pair: ReturnType<typeof seedPair>) {
    return render(TimeframeSettings, { props: { pair, tabKey: 'inst_test' } });
}

describe('TimeframeSettings — Active timeframes selector', () => {
    it('renders the numeric selector seeded from the settings store', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        const input = container.querySelector('#tf-active-count') as HTMLInputElement;
        expect(input).toBeTruthy();
        expect(input.value).toBe('5');
        expect(container.textContent).toContain('Active timeframes');
        expect(container.textContent).toContain('saving recharges running instances');
    });

    it('changing the selector marks the save and posts active_timeframes to /api/config', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        const input = container.querySelector('#tf-active-count') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: '3' } });
        const button = Array.from(container.querySelectorAll('button'))
            .find((b) => b.textContent?.includes('Apply Workspace Configuration'))!;
        expect(button).toBeTruthy();
        await fireEvent.click(button);
        await waitFor(() => {
            const cfgCall = fetchCalls.find((c) => c.url === '/api/config');
            expect(cfgCall).toBeTruthy();
            expect(cfgCall!.body).toEqual({ active_timeframes: 3 });
        });
        // The per-slot instance-config save still runs through the SAME flow.
        const instCall = fetchCalls.find((c) => c.url.includes('/api/instances/inst_test/config'));
        expect(instCall).toBeTruthy();
        expect(Object.keys(instCall!.body)).toContain('micro1');
    });
});

describe('TimeframeSettings — active-slot cards', () => {
    it('renders one card per ACTIVE slot (not the full 10)', async () => {
        const pair = seedPair(['micro1', 'fast1', 'slow2']);
        const { container } = renderTab(pair);
        const titles = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent ?? '');
        const expected = ['micro1', 'fast1', 'slow2'].map((s) => TIMEFRAME_SLOT_LABELS[s as typeof TIMEFRAME_SLOT_KINDS[number]]);
        for (const label of expected) {
            expect(titles.some((t) => t.includes(label))).toBe(true);
        }
        expect(titles.some((t) => t.includes('Longterm2'))).toBe(false);
    });

    it('renders all 10 cards when every slot is active', async () => {
        const pair = seedPair([...TIMEFRAME_SLOT_KINDS]);
        const { container } = renderTab(pair);
        const titles = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent ?? '');
        for (const slot of TIMEFRAME_SLOT_KINDS) {
            expect(titles.some((t) => t.includes(TIMEFRAME_SLOT_LABELS[slot]))).toBe(true);
        }
    });
});
