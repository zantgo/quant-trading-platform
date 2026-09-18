// @vitest-environment jsdom
//
// WorkspaceSettings — v11.8 Timeframes CRUD (presence = activation):
//   • 14 duration chips render in ladder order, active = present in activeDurations
//   • toggling an inactive duration persists timeframes via POST /api/config
//   • the last active timeframe cannot be deactivated (min-1 guard)
//   • the Identity and Automation Scheduler containers are removed

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import WorkspaceSettings from './WorkspaceSettings.svelte';
import { useAppStore } from '../state.svelte';
import { makeTerms } from '../tests/makeTerms';
import { DURATIONS, tfLabel } from '../types';

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status < 400,
        status,
        json: async () => body,
        text: async () => JSON.stringify(body),
        headers: new Headers({ 'content-type': 'application/json' }),
    } as unknown as Response;
}

function seedPair(activeDurations: number[] = [1, 3, 5, 15, 30]) {
    const app = useAppStore();
    app.initInstance('BTC');
    const pair = app.instancesMap['BTC-USDT'];
    pair.instanceId = 'inst_test';
    pair.terms = makeTerms();
    pair.activeDurations = activeDurations;
    return { app, pair };
}

/// v11.11: activation lives in the merged rail — one switch per duration
/// (`aria-label "<label> activation"`, `aria-pressed`).
function railSwitches(container: HTMLElement): HTMLButtonElement[] {
    return Array.from(
        container.querySelectorAll('button[aria-label$="activation"]'),
    ) as HTMLButtonElement[];
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
    it('renders 14 rail switches, 5 active by default (fastest-5 fixture)', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const switches = railSwitches(container);
        expect(switches.length).toBe(DURATIONS.length);
        expect(switches.filter((c) => c.getAttribute('aria-pressed') === 'true').length).toBe(5);
        // v11.11 merged editor: ACTIVE tags + grouped parameter pane + footer.
        expect(container.textContent).toContain('ACTIVE');
        expect(container.textContent).toContain('TREND & VOLATILITY CHANNELS');
        expect(container.textContent).toContain('Active: 5 / 14');
        expect(container.textContent).toContain('Instance Memory: Allocated');
    });

    it('filters the parameter pane via the filter box', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const filter = container.querySelector('input[placeholder="Filter parameters…"]') as HTMLInputElement;
        expect(filter).toBeTruthy();
        await fireEvent.input(filter, { target: { value: 'keltner' } });
        await tick();
        const text = container.textContent ?? '';
        expect(text).toContain('Keltner EMA');
        expect(text).not.toContain('RSI Window');
    });

    it('clicking a rail row selects the target duration', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const rows = Array.from(container.querySelectorAll('button')).filter(
            (b) => b.textContent?.includes('15s') && !b.getAttribute('aria-label'),
        ) as HTMLButtonElement[];
        expect(rows.length).toBeGreaterThan(0);
        await fireEvent.click(rows[0]);
        await tick();
        expect(container.textContent).toContain('15s · 15s — INDICATOR PARAMETERS');
    });

    it('activating a duration POSTs timeframes and updates pair.activeDurations', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const oneMinute = railSwitches(container).find(
            (c) => c.getAttribute('aria-label') === '1m activation',
        )!;
        expect(oneMinute.getAttribute('aria-pressed')).toBe('false');
        await fireEvent.click(oneMinute);
        await waitFor(() => {
            expect(pair.activeDurations).toEqual([1, 3, 5, 15, 30, 60]);
        });
        const post = (globalThis.fetch as any).mock.calls.find(
            ([u, o]: any[]) => String(u) === '/api/config' && o?.method === 'POST',
        );
        expect(JSON.parse(post[1].body)).toEqual({
            timeframes: [1, 3, 5, 15, 30, 60],
        });
    });

    it('cannot deactivate the last active timeframe', async () => {
        const { pair } = seedPair([1]);
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const one = railSwitches(container).find(
            (c) => c.getAttribute('aria-label') === '1s activation',
        )!;
        expect(one.disabled).toBe(true);
        await fireEvent.click(one);
        expect(pair.activeDurations).toEqual([1]);
    });

    it('Identity and Automation Scheduler containers are removed', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        expect(container.textContent).not.toContain('Identity');
        expect(container.textContent).not.toContain('Automation Scheduler');
    });
});
