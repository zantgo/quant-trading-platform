// @vitest-environment jsdom
//
// TimeframeSettings — v11.2 active-timeframes knob + active-slot cards:
//   • a compact "Active timeframes" numeric selector (1–10) at the top,
//     seeded from the settings store,
//   • the per-TF cards render only the instance's ACTIVE slots,
//   • the existing Apply flow POSTs `active_timeframes` to /api/config
//     alongside the per-slot indicator overrides (same save, no new path).
import { tick } from 'svelte';
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
    app.settings.activeSlotsList = null;
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
    // Realistic default: the v11.2 fastest-5 ladder (what a default boot runs).
    pair.activeSlots = (activeSlots ?? ['micro1', 'micro2', 'fast1', 'fast2', 'slow1']) as never;
    return pair;
}

function renderTab(pair: ReturnType<typeof seedPair>) {
    return render(TimeframeSettings, { props: { pair, tabKey: 'inst_test' } });
}

describe('TimeframeSettings — Active timeframes toggles', () => {
    function toggleButtons(container: HTMLElement): HTMLButtonElement[] {
        // Toggle buttons carry the slot label (MICRO1..LONGTERM2) + duration.
        return Array.from(container.querySelectorAll('button')).filter((b) =>
            ['Micro1','Micro2','Fast1','Fast2','Slow1','Slow2','Macro1','Macro2','Longterm1','Longterm2']
                .some((lbl) => b.textContent?.includes(lbl)),
        );
    }

    it('renders 10 toggles, fastest-5 on by default', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        await tick();
        const toggles = toggleButtons(container);
        expect(toggles.length).toBe(10);
        const on = toggles.filter((b) => b.getAttribute('aria-pressed') === 'true');
        expect(on.length).toBe(5);
        expect(container.textContent).toContain('Active timeframes');
        expect(container.textContent).toContain('saving recharges running instances');
    });

    it('toggling a slot on marks the save and posts active_slots to /api/config', async () => {
        const pair = seedPair();
        const { container } = renderTab(pair);
        await tick();
        const slow2 = toggleButtons(container).find((b) => b.textContent?.includes('Slow2'))!;
        expect(slow2.getAttribute('aria-pressed')).toBe('false');
        await fireEvent.click(slow2);
        expect(slow2.getAttribute('aria-pressed')).toBe('true');
        const button = Array.from(container.querySelectorAll('button'))
            .find((b) => b.textContent?.includes('Apply Workspace Configuration'))!;
        expect(button).toBeTruthy();
        await fireEvent.click(button);
        await waitFor(() => {
            const cfgCall = fetchCalls.find((c) => c.url === '/api/config');
            expect(cfgCall).toBeTruthy();
            expect(cfgCall!.body).toEqual({
                active_slots: ['micro1', 'micro2', 'fast1', 'fast2', 'slow1', 'slow2'],
            });
        });
        // The per-slot instance-config save still runs through the SAME flow.
        const instCall = fetchCalls.find((c) => c.url.includes('/api/instances/inst_test/config'));
        expect(instCall).toBeTruthy();
        expect(Object.keys(instCall!.body)).toContain('micro1');
    });

    it('never allows deactivating the last active timeframe', async () => {
        const pair = seedPair(['micro1']);
        const { container } = renderTab(pair);
        await tick();
        const micro1 = toggleButtons(container).find((b) => b.textContent?.includes('Micro1'))!;
        expect(micro1.getAttribute('aria-pressed')).toBe('true');
        await fireEvent.click(micro1);
        // Still on — the min-1 guard refused the toggle.
        expect(micro1.getAttribute('aria-pressed')).toBe('true');
    });
});

describe('TimeframeSettings — active-slot cards', () => {
    it('renders one card per ACTIVE slot (not the full 10)', async () => {
        const pair = seedPair(['micro1', 'fast1', 'slow2']);
        const { container } = renderTab(pair);
        await tick();
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
        await tick();
        const titles = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent ?? '');
        for (const slot of TIMEFRAME_SLOT_KINDS) {
            expect(titles.some((t) => t.includes(TIMEFRAME_SLOT_LABELS[slot]))).toBe(true);
        }
    });
});
