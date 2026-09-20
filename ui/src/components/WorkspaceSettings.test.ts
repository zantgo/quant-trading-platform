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
import styles from './WorkspaceSettings.module.css';
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
        // v11.12.18: the pane sub-title is the duration label alone.
        const subTitle = container.querySelector(`.${styles.tfCardSubTitle}`);
        expect(subTitle?.textContent).toBe('15s — INDICATOR PARAMETERS');
    });

    it('v11.12.18: pane/rail labels render once — no pipe, no `· secs` sub-label', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        // Two-pane header reads TIMEFRAME / INDICATOR (the decorative `|` is gone).
        const headers = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent?.trim());
        expect(headers).toContain('TIMEFRAME');
        expect(headers).toContain('INDICATOR');
        expect(headers.some((h) => h?.includes('|'))).toBe(false);
        // Rail + pane sub-title show the duration label alone — no `15s · 15s`
        // / `3m · 180s` repetition (a timeframe IS its duration).
        const text = container.textContent ?? '';
        expect(text).not.toContain('15s · 15s');
        expect(text).not.toContain('3m · 180s');
        expect(text).not.toContain('1m · 60s');
    });

    it('v11.11: toggling is draft-only — UI updates, no POST until SAVE', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const oneMinute = railSwitches(container).find(
            (c) => c.getAttribute('aria-label') === '1m activation',
        )!;
        expect(oneMinute.getAttribute('aria-pressed')).toBe('false');
        await fireEvent.click(oneMinute);
        await tick();
        // Draft: the switch, tag and counter update…
        expect(oneMinute.getAttribute('aria-pressed')).toBe('true');
        expect(container.textContent).toContain('Active: 6 / 14');
        // …but the live ladder and the network stay untouched.
        expect(pair.activeDurations).toEqual([1, 3, 5, 15, 30]);
        const configPosts = (globalThis.fetch as any).mock.calls.filter(
            ([u, o]: any[]) => String(u) === '/api/config' && o?.method === 'POST',
        );
        expect(configPosts.length).toBe(0);
    });

    it('v11.11: SAVE applies the ladder once (config POST + canonical instance POST)', async () => {
        const { pair } = seedPair();
        const { container } = render(WorkspaceSettings, { props: { pair, tabKey: 'BTC-USDT' } });
        await tick();
        const oneMinute = railSwitches(container).find(
            (c) => c.getAttribute('aria-label') === '1m activation',
        )!;
        await fireEvent.click(oneMinute);
        const saveBtn = await waitFor(() => {
            const btn = screen.getByText('SAVE') as HTMLButtonElement;
            expect(btn.disabled).toBe(false);
            return btn;
        });
        await fireEvent.click(saveBtn);
        await waitFor(() => {
            expect(pair.activeDurations).toEqual([1, 3, 5, 15, 30, 60]);
        });
        const configPost = (globalThis.fetch as any).mock.calls.find(
            ([u, o]: any[]) => String(u) === '/api/config' && o?.method === 'POST',
        );
        expect(configPost).toBeTruthy();
        expect(JSON.parse(configPost[1].body)).toEqual({ timeframes: [1, 3, 5, 15, 30, 60] });
        const instancePost = (globalThis.fetch as any).mock.calls.find(
            ([u]: any[]) => String(u).includes('/api/instances/inst_test/config'),
        );
        expect(instancePost).toBeTruthy();
        const body = JSON.parse(instancePost[1].body);
        // v11.9 contract: per-duration overrides nested under `timeframes`,
        // keyed by seconds — top-level numeric keys are rejected by the
        // backend's `deny_unknown_fields`.
        expect(Object.keys(body.timeframes).sort()).toEqual(['1', '15', '3', '30', '5', '60']);
        expect(body.timeframes['60'].candles).toEqual({ duration_seconds: 60 });
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
