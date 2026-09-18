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

describe('AppPageRouter — MME settings dual-mode (v11.9 N2)', () => {
    it('no instance selected renders the full General settings page (v11.11)', async () => {
        const { container } = renderRouter();
        await tick();
        const text = container.textContent ?? '';
        // v11.11: ONE stacked page — title, fee card, cost projection and
        // the share-config container all present without any switch.
        expect(text).toContain('General Settings');
        expect(text).toContain('Fees & Leverage');
        expect(text).toContain('Cost Projection');
        expect(text).toContain('Download config.toml');
        expect(screen.queryByText('Share Config')).toBeNull();
    });

    it('instance selected renders Workspace settings', async () => {
        const app = useAppStore();
        const { container } = renderRouter({
            selectedInstance: 'BTC-USDT',
            activePair: app.instancesMap['BTC-USDT'],
        });
        await tick();
        const text = container.textContent ?? '';
        expect(text).toContain('Workspace Settings');
        expect(text).toContain('Timeframes');
    });
});
