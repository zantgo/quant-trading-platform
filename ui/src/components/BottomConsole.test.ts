// Component tests for `BottomConsole.svelte` — exercises the export
// pipeline through the rendered UI. These tests mount the component
// with a mocked `AppStore`, simulate clicking the export button for
// each sub-tab, and assert the JSON payload matches the schema.
//
// Uses the Svelte 5 `mount` API and the `navigator.clipboard` mock
// pattern. 

// @vitest-environment jsdom

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount, tick } from 'svelte';
import BottomConsole from './BottomConsole.svelte';

// Mock the AppStore before importing the Svelte component.
const mockApp = {
    activeTab: 'BTC-USDT',
    priceText: '65000.00',
    paperDirection: 'LONG',
    paperLeverage: 10,
    paperMarginUsed: 320,
    paperUnrealizedPnl: 50,
    paperUnrealizedRoi: 15.63,
    paperTotalAccountValue: 10000,
    paperCashBalance: 9500,
    paperInitialUSD: 10000,
    paperAllocationPct: 20,
    paperAutoExecute: false,
    paperBreakEvenTrailEnabled: false,
    activePaperPosition: {
        symbol: 'BTC-USDT',
        size: 0.05,
        entry_price: 64000,
        opened_at: 1753950000,
    },
    paperHistory: [],
    openOrders: [],
    activeDurations: [],
    activeEntryOrders: [],
    positionBrackets: [],
    paper: { openOrders: [] as Record<string, unknown>[] },
    activePlan: null,
    activeConsoleOpen: false,
    activeConsoleTab: 'positions' as 'positions' | 'orders' | 'history' | 'plan',
    fullscreenChart: null,
};

vi.mock('../state.svelte', () => ({
    useAppStore: () => mockApp,
}));

// Capture clipboard.writeText calls so we can assert on the JSON.
const clipboardWrites: string[] = [];
const clipboardMock = vi.fn(async (text: string) => {
    clipboardWrites.push(text);
});

beforeEach(() => {
    clipboardWrites.length = 0;
    mockApp.activeConsoleTab = 'positions';
    mockApp.paperDirection = 'LONG';
    Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: clipboardMock },
        writable: true,
        configurable: true,
    });
});

afterEach(() => {
    document.body.innerHTML = '';
    vi.restoreAllMocks();
});

describe('BottomConsole export pipeline', () => {
    it('renders the export button with a context-aware label per tab', async () => {
        const component = mount(BottomConsole, {
            target: document.body,
            props: { activeConsoleTab: 'positions' },
        });
        await tick();
        const btn = document.body.querySelector('button[title*="JSON"]');
        expect(btn).not.toBeNull();
        expect(btn?.textContent?.trim()).toContain('Export Positions JSON');
        unmount(component);
    });

    it('exports a positions payload when clicked on the positions tab', async () => {
        const component = mount(BottomConsole, {
            target: document.body,
            props: { activeConsoleTab: 'positions' },
        });
        await tick();
        const btn = document.body.querySelector('button[title*="JSON"]') as HTMLButtonElement;
        btn.click();
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        expect(clipboardWrites.length).toBe(1);
        const payload = JSON.parse(clipboardWrites[0]);
        expect(payload.source_tab).toBe('positions');
        expect(payload.symbol).toBe('BTC-USDT');
        expect(payload.position).not.toBeNull();
        expect(payload.position.direction).toBe('LONG');
        unmount(component);
    });

    it('exports an orders payload when clicked on the orders tab', async () => {
        const component = mount(BottomConsole, {
            target: document.body,
            props: { activeConsoleTab: 'orders' },
        });
        await tick();
        const btn = document.body.querySelector('button[title*="JSON"]') as HTMLButtonElement;
        btn.click();
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        expect(clipboardWrites.length).toBe(1);
        const payload = JSON.parse(clipboardWrites[0]);
        expect(payload.source_tab).toBe('orders');
        expect(Array.isArray(payload.open_orders)).toBe(true);
        unmount(component);
    });

    it('exports a history payload when clicked on the history tab', async () => {
        const component = mount(BottomConsole, {
            target: document.body,
            props: { activeConsoleTab: 'history' },
        });
        await tick();
        const btn = document.body.querySelector('button[title*="JSON"]') as HTMLButtonElement;
        btn.click();
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        expect(clipboardWrites.length).toBe(1);
        const payload = JSON.parse(clipboardWrites[0]);
        expect(payload.source_tab).toBe('history');
        expect(Array.isArray(payload.history)).toBe(true);
        unmount(component);
    });

    it('exports a plan payload when clicked on the plan tab', async () => {
        const component = mount(BottomConsole, {
            target: document.body,
            props: { activeConsoleTab: 'plan' },
        });
        await tick();
        const btn = document.body.querySelector('button[title*="JSON"]') as HTMLButtonElement;
        btn.click();
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        expect(clipboardWrites.length).toBe(1);
        const payload = JSON.parse(clipboardWrites[0]);
        expect(payload.source_tab).toBe('plan');
        expect(payload.plan_visible).toBe(false);
        expect(payload.targets).toEqual([]);
        expect(payload.stop).toBeNull();
        unmount(component);
    });
});
