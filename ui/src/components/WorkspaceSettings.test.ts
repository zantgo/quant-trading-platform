// @vitest-environment jsdom
//
// WorkspaceSettings — v11.8 Timeframes CRUD (presence = activation):
//   • 10 slot chips render in ladder order, active = present in activeSlots
//   • toggling an inactive slot persists active_slots via POST /api/config
//   • the last active timeframe cannot be deactivated (min-1 guard)
//   • the Identity and Automation Scheduler containers are removed

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import WorkspaceSettings from './WorkspaceSettings.svelte';
import { useAppStore } from '../state.svelte';
import { makeTerms } from '../tests/makeTerms';
import { TIMEFRAME_SLOT_KINDS } from '../types';

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status < 400,
        status,
        json: async () => body,
        text: async () => JSON.stringify(body),
        headers: new Headers({ 'content-type': 'application/json' }),
    } as unknown as Response;
}

function seedPair(activeSlots: string[] = ['micro1', 'micro2', 'fast1', 'fast2', 'slow1']) {
    const app = useAppStore();
    app.initInstance('BTC');
    const pair = app.instancesMap['BTC-USDT'];
    pair.instanceId = 'inst_test';
    pair.terms = makeTerms();
    pair.activeSlots = activeSlots as never;
    return { app, pair };
}

function crudChips(container: HTMLElement): HTMLButtonElement[] {
    return Array.from(container.querySelectorAll('button[aria-pressed]')).filter((b) =>
        ['Micro1','Micro2','Fast1','Fast2','Slow1','Slow2','Macro1','Macro2','Longterm1','Longterm2']
            .some((lbl) => b.textContent?.includes(lbl)),
    );
}

const originalFetch = globalThis.fetch;

beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async (url: string, opts?: RequestInit) => {
        if (String(url).includes('/api/config') && (!opts?.method || opts.method === 'GET')) {
            return jsonResponse({
                backtest: { archive_depth_days: 180, warmup_bars: 300 },
                workspace: {},
                instances: [],
            });
        }
        if (String(url).includes('/api/instances/') && String(url).includes('/config')) {
            return jsonResponse({ success: true });
        }
        return jsonResponse({});
    }));
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    (globalThis as any).fetch = originalFetch;
});

describe('WorkspaceSettings — Timeframes CRUD (v11.8)', () => {
    it('renders 10 chips, 5 active by default (fastest-5 fixture)', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const chips = crudChips(container);
        expect(chips.length).toBe(10);
        expect(chips.filter((c) => c.getAttribute('aria-pressed') === 'true').length).toBe(5);
        expect(container.textContent).toContain('Timeframes');
    });

    it('activating a slot POSTs active_slots and updates pair.activeSlots', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const slow2 = crudChips(container).find((c) => c.textContent?.includes('Slow2'))!;
        expect(slow2.getAttribute('aria-pressed')).toBe('false');
        await fireEvent.click(slow2);
        await waitFor(() => {
            expect(pair.activeSlots).toEqual(['micro1', 'micro2', 'fast1', 'fast2', 'slow1', 'slow2']);
        });
        const post = (globalThis.fetch as any).mock.calls.find(
            ([u, o]: any[]) => String(u) === '/api/config' && o?.method === 'POST',
        );
        expect(JSON.parse(post[1].body)).toEqual({
            active_slots: ['micro1', 'micro2', 'fast1', 'fast2', 'slow1', 'slow2'],
        });
    });

    it('cannot deactivate the last active timeframe', async () => {
        const { pair } = seedPair(['micro1']);
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const micro1 = crudChips(container).find((c) => c.textContent?.includes('Micro1'))!;
        expect(micro1.disabled).toBe(true);
        await fireEvent.click(micro1);
        expect(pair.activeSlots).toEqual(['micro1']);
    });

    it('Identity and Automation Scheduler containers are removed', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        expect(container.textContent).not.toContain('Identity');
        expect(container.textContent).not.toContain('Automation Scheduler');
    });
});
