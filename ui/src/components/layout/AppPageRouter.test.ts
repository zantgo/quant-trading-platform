// @vitest-environment jsdom
//
// v11.9 (N2): the Market Monitor Settings tab is the ONLY settings
// surface — General settings when no instance is selected (Fees &
// Leverage / Share Config), Workspace settings when one is.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import AppPageRouter from './AppPageRouter.svelte';
import { useAppStore } from '../../state.svelte';
import { makeTerms } from '../../tests/makeTerms';

const originalFetch = globalThis.fetch;

beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async (url: string) => ({
        ok: true,
        status: 200,
        json: async () => (String(url).includes('/api/config') ? {} : {}),
        text: async () => '',
        headers: new Headers({ 'content-type': 'application/json' }),
    })) as unknown as typeof fetch);
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.initInstance('BTC');
    app.instancesMap['BTC-USDT'].terms = makeTerms();
    app.instancesMap['BTC-USDT'].instanceId = 'inst_test';
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    (globalThis as any).fetch = originalFetch;
});

function renderRouter(overrides: Record<string, unknown> = {}) {
    const app = useAppStore();
    const pair = app.instancesMap['BTC-USDT'];
    return render(AppPageRouter, {
        props: {
            currentEngine: 'market_monitor',
            middleTab: 'settings',
            selectedInstance: null,
            activePair: undefined,
            activeTab: 'BTC-USDT',
            wssMap: {},
            onrequestConfirm: () => {},
            errorMessage: null,
            ...overrides,
        },
    });
}

describe('AppPageRouter — unified settings (v11.12)', () => {
    it('no instance selected still renders the unified Settings surface', async () => {
        const { container } = renderRouter();
        await tick();
        const text = container.textContent ?? '';
        // v11.12: ONE Settings surface — internal navbar, workspace-level
        // editors available even with zero instances.
        expect(text).toContain('Settings');
        expect(text).toContain('Workspace');
        expect(text).toContain('Instance');
        expect(text).toContain('General');
        expect(text).toContain('Timeframes');
    });

    it('instance selected renders the same unified Settings surface', async () => {
        const app = useAppStore();
        const { container } = renderRouter({
            selectedInstance: 'BTC-USDT',
            activePair: app.instancesMap['BTC-USDT'],
        });
        await tick();
        const text = container.textContent ?? '';
        expect(text).toContain('Timeframes');
        // General content stays MOUNTED (hidden) so drafts survive tab
        // switches — reach it via the internal navbar.
        const generalTab = screen.getByText('General') as HTMLButtonElement;
        expect(generalTab).toBeTruthy();
    });
});
