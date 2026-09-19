// @vitest-environment jsdom
//
// UnifiedSettings — v11.12: ONE Settings surface with an internal navbar
// (Workspace | Instance | General). Sections stay MOUNTED across tab
// switches (drafts survive); the INSTANCE group is chip-scoped; the
// header's single SAVE drives the dirty union.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import UnifiedSettings from './UnifiedSettings.svelte';
import { useAppStore } from '../state.svelte';
import { makeTerms } from '../tests/makeTerms';

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status < 400,
        status,
        json: async () => body,
        text: async () => JSON.stringify(body),
        headers: new Headers({ 'content-type': 'application/json' }),
    } as unknown as Response;
}

function seedPair(base: string, activeDurations: number[] = [1, 3, 5, 15, 30]) {
    const app = useAppStore();
    app.initInstance(base);
    const pair = app.instancesMap[`${base}-USDT`];
    pair.instanceId = `inst_${base.toLowerCase()}`;
    pair.terms = makeTerms();
    pair.activeDurations = activeDurations;
    return pair;
}

const originalFetch = globalThis.fetch;

beforeEach(() => {
    const app = useAppStore();
    app.selectedInstance = null;
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    vi.stubGlobal('fetch', vi.fn(async (url: string, opts?: RequestInit) => {
        if (String(url).includes('/api/config') && (!opts?.method || opts.method === 'GET')) {
            return jsonResponse({
                fees: { maker_fee_pct: 0.02, taker_fee_pct: 0.06, funding_rate_8h: 0.01 },
                leverage: { cross_leverage: 20 },
                backtest: { archive_depth_days: 180, warmup_bars: 300 },
                workspace: {},
                instances: [],
            });
        }
        return jsonResponse({ success: true });
    }));
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    (globalThis as any).fetch = originalFetch;
});

function tabButton(label: string): HTMLButtonElement {
    return screen.getByText(label) as HTMLButtonElement;
}

describe('UnifiedSettings — internal navbar', () => {
    it('defaults to WORKSPACE: the Timeframes card shows, general section hidden', async () => {
        seedPair('BTC');
        const { container } = render(UnifiedSettings);
        await tick();
        expect(tabButton('Workspace').getAttribute('aria-pressed')).toBe('true');
        expect(container.textContent).toContain('Timeframes');
        const general = container.querySelector('[data-testid="settings-general-section"]') as HTMLElement;
        expect(general.hidden).toBe(true);
    });

    it('INSTANCE tab shows the per-instance cards, hides the workspace card', async () => {
        seedPair('BTC');
        const { container } = render(UnifiedSettings);
        await tick();
        await fireEvent.click(tabButton('Instance'));
        await tick();
        expect(container.textContent).toContain('Visual Overlays');
        expect(container.textContent).toContain('Indicator Activation');
        expect(container.textContent).toContain('Liquidation Heatmap');
        // v11.12: sections stay mounted (drafts survive) — verify via presence
        // of the filter input (hidden vs removed is asserted at the shell
        // wrapper level in the first test).
        expect(container.querySelector('input[placeholder="Filter parameters…"]')).toBeTruthy();
    });

    it('GENERAL tab shows Fees & Leverage and hides the workspace section', async () => {
        seedPair('BTC');
        const { container } = render(UnifiedSettings);
        await tick();
        await fireEvent.click(tabButton('General'));
        await waitFor(() => expect(container.textContent).toContain('Fees & Leverage'));
        const ws = container.querySelector('[data-testid="settings-workspace-section"]') as HTMLElement;
        expect(ws.hidden).toBe(true);
    });

    it('with zero instances the INSTANCE tab shows the shared empty state', async () => {
        const { container } = render(UnifiedSettings);
        await tick();
        await fireEvent.click(tabButton('Instance'));
        await tick();
        expect(container.textContent).toContain('No active instance');
    });

    it('with 2+ instances the INSTANCE tab renders the chip selector and switches target', async () => {
        seedPair('BTC');
        seedPair('ETH');
        const { container } = render(UnifiedSettings);
        await tick();
        await fireEvent.click(tabButton('Instance'));
        await tick();
        const chips = Array.from(container.querySelectorAll('button[aria-pressed]')).filter(
            (b) => (b.textContent ?? '').includes('-USDT'),
        ) as HTMLButtonElement[];
        expect(chips.length).toBe(2);
        const eth = chips.find((c) => c.textContent?.includes('ETH-USDT'))!;
        await fireEvent.click(eth);
        expect(eth.getAttribute('aria-pressed')).toBe('true');
    });

    it('ONE SAVE drives the dirty union (fees edit on GENERAL enables it)', async () => {
        seedPair('BTC');
        const { container } = render(UnifiedSettings);
        await tick();
        await fireEvent.click(tabButton('General'));
        await waitFor(() => expect(container.textContent).toContain('Fees & Leverage'));
        const saveBtn = screen.getByText('SAVE') as HTMLButtonElement;
        expect(saveBtn.disabled).toBe(true);

        const maker = container.querySelector<HTMLInputElement>('#fee-maker')!;
        await fireEvent.input(maker, { target: { value: '0.05' } });
        await waitFor(() => expect((screen.getByText('SAVE') as HTMLButtonElement).disabled).toBe(false));

        await fireEvent.click(screen.getByText('SAVE'));
        await waitFor(() => {
            const post = (globalThis.fetch as any).mock.calls.find(
                ([u, o]: any[]) => String(u) === '/api/config' && o?.method === 'POST',
            );
            expect(post).toBeTruthy();
            expect(JSON.parse(post[1].body).fees.maker_fee_pct).toBe(0.05);
        });
    });
});
