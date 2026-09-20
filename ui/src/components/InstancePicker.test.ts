// @vitest-environment jsdom
// Regression + feature lock for the InstancePicker (Market Monitor →
// Workspace instance list).
//
// What these tests assert:
//   - rows render from GET /api/instances
//   - the search bar live-filters rows case-insensitively ("btc" finds
//     "BTC-USDT"; a slash-form "btc/usdt" query still matches because the
//     query is normalized to the canonical hyphen pair)
//   - a filter with no matches shows the dedicated empty state with a
//     clear-filter button
//   - the per-row Delete button fires `onrequestConfirm(id, 'delete', pair)`
//     WITHOUT navigating (stopPropagation), matching the right panel
//   - clicking the row body enters the instance (`selectedInstance` +
//     `activeEngineTab`), matching the previous behavior
//   - the inline `errorMessage` prop renders above the list (shared
//     delete-error state with the right panel)
//   - the list refetches when `sessionInstanceCount` changes (delete
//     sync) and the mount $effect does NOT throw `state_unsafe_mutation`

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import InstancePicker from './InstancePicker.svelte';
import { useAppStore } from '../state.svelte';

function mockFetch(instances: Array<{ id: string; pair: string; status: string; symbol?: string }>) {
    return vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ instances }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
}

/** Real API rows carry `symbol` (base) + `pair` (full key). */
function row(pairKey: string, status = 'running'): { id: string; pair: string; status: string; symbol: string } {
    const base = pairKey.split('-')[0];
    return { id: `inst_${base.toLowerCase()}`, pair: pairKey, status, symbol: base };
}

function seedInstance(pairKey: string) {
    const app = useAppStore();
    const [base, quote] = pairKey.split('-');
    app.initInstance(base);
    const entry = app.instancesMap[pairKey];
    if (entry) {
        entry.instanceId = `inst_${base.toLowerCase()}`;
        entry.terms[1].priceText = '50000.00';
    }
}

function renderPicker(props: Record<string, unknown> = {}) {
    return render(InstancePicker, {
        props: {
            onrequestConfirm: () => {},
            errorMessage: null,
            ...props,
        },
    });
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.sessionCurrency = 'USDT';
    app.sessionExchange = 'Hyperliquid';
    app.middleTab = 'workspace';
    app.selectedInstance = null;
    app.activeEngineTab = 'overview';
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

describe('InstancePicker — list rendering', () => {
    it('renders instance rows from the instances API', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
            row('ETH-USDT', 'paused'),
        ]));
        seedInstance('BTC-USDT');

        renderPicker();

        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());
        expect(screen.getByText('ETH-USDT')).toBeTruthy();
        // One Delete button per row.
        expect(screen.getAllByTitle('Delete')).toHaveLength(2);
    });

    it('renders the empty state when no instances exist', async () => {
        vi.stubGlobal('fetch', mockFetch([]));
        renderPicker();
        await waitFor(() =>
            expect(screen.getByText('No active instances. Open the Instances panel (top-right) to create one.')).toBeTruthy());
    });

    it('renders an inline error when errorMessage is set (shared with right panel)', async () => {
        vi.stubGlobal('fetch', mockFetch([]));
        renderPicker({ errorMessage: 'Cannot delete BTC-USDT: HTTP 500' });
        expect(await screen.findByText('Cannot delete BTC-USDT: HTTP 500')).toBeTruthy();
    });
});

describe('InstancePicker — live search filter', () => {
    it('filters rows live and case-insensitively on lowercase query', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
            row('ETH-USDT'),
        ]));
        seedInstance('BTC-USDT');
        seedInstance('ETH-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const input = screen.getByLabelText('Filter instances by name') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: 'btc' } });

        expect(screen.getByText('BTC-USDT')).toBeTruthy();
        expect(screen.queryByText('ETH-USDT')).toBeNull();
    });

    it('matches on UPPERCASE query against the stored symbol (symbols are always caps)', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
            row('ETH-USDT'),
        ]));
        seedInstance('BTC-USDT');
        seedInstance('ETH-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const input = screen.getByLabelText('Filter instances by name') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: 'BTC' } });

        expect(screen.getByText('BTC-USDT')).toBeTruthy();
        expect(screen.queryByText('ETH-USDT')).toBeNull();
    });

    it('v11.12.18: a slash-form query still matches the canonical hyphen pair', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
            row('ETH-USDT'),
        ]));
        seedInstance('BTC-USDT');
        seedInstance('ETH-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const input = screen.getByLabelText('Filter instances by name') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: 'btc/usdt' } });

        expect(screen.getByText('BTC-USDT')).toBeTruthy();
        expect(screen.queryByText('ETH-USDT')).toBeNull();
    });

    it('shows the no-match state and clears the filter from it', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
        ]));
        seedInstance('BTC-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const input = screen.getByLabelText('Filter instances by name') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: 'sol' } });

        expect(screen.queryByText('BTC-USDT')).toBeNull();
        expect(screen.getByText(/No instances match/)).toBeTruthy();

        await fireEvent.click(screen.getByText('Clear filter'));
        expect(screen.getByText('BTC-USDT')).toBeTruthy();
    });

    it('renders the count chip showing filtered / total when a filter is active', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
            row('ETH-USDT'),
        ]));
        seedInstance('BTC-USDT');
        seedInstance('ETH-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const input = screen.getByLabelText('Filter instances by name') as HTMLInputElement;
        await fireEvent.input(input, { target: { value: 'btc' } });

        // The chip renders "1 / 2" as a single element whose full text
        // content spans the strong "1" and the " / 2" remainder.
        const chip = screen.getAllByText((content, el) =>
            el instanceof HTMLElement && el.textContent?.trim() === '1 / 2');
        expect(chip.length).toBeGreaterThan(0);
    });
});

describe('InstancePicker — row interactions', () => {
    it('clicking the row body enters the instance (same behavior as before)', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
        ]));
        seedInstance('BTC-USDT');

        const app = useAppStore();
        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        // v11.4: rows are semantic <a> links (right-click → open in a new
        // tab); a plain click still prevents default and enters the instance.
        const rowEl = screen.getByText('BTC-USDT').closest('a') as HTMLAnchorElement;
        expect(rowEl).toBeTruthy();
        expect(rowEl.getAttribute('href')).toContain('instance/BTC-USDT');
        await fireEvent.click(rowEl);

        expect(app.selectedInstance).toBe('BTC-USDT');
        expect(app.activeEngineTab).toBe('instance');
    });

    it('clicking the Delete button fires onrequestConfirm and does NOT navigate', async () => {
        vi.stubGlobal('fetch', mockFetch([
            row('BTC-USDT'),
        ]));
        seedInstance('BTC-USDT');

        const app = useAppStore();
        const onrequestConfirm = vi.fn();
        renderPicker({ onrequestConfirm });
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

        const deleteBtn = screen.getByTitle('Delete');
        await fireEvent.click(deleteBtn);

        expect(onrequestConfirm).toHaveBeenCalledWith('inst_btc', 'delete', 'BTC-USDT');
        expect(app.selectedInstance, 'delete click must not enter the instance').toBeNull();
    });
});

describe('InstancePicker — sync after delete', () => {
    it('refetches the list when sessionInstanceCount changes', async () => {
        let instances = [
            row('BTC-USDT'),
            row('ETH-USDT'),
        ];
        const fetchSpy = vi.fn(async () => new Response(
            JSON.stringify({ instances }),
            { status: 200, headers: { 'content-type': 'application/json' } },
        ));
        vi.stubGlobal('fetch', fetchSpy);
        seedInstance('BTC-USDT');
        seedInstance('ETH-USDT');

        renderPicker();
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());
        expect(screen.getByText('ETH-USDT')).toBeTruthy();

        // Simulate the App-level delete: instance removed server-side,
        // `executeDelete` bumps the session instance count.
        instances = [row('BTC-USDT')];
        useAppStore().sessionInstanceCount = 1;
        await waitFor(() => expect(screen.queryByText('ETH-USDT')).toBeNull());
        await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());
    });

    it('mount effect does not throw state_unsafe_mutation', async () => {
        const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
        try {
            vi.stubGlobal('fetch', mockFetch([
                row('BTC-USDT'),
            ]));
            seedInstance('BTC-USDT');

            renderPicker();
            await waitFor(() => expect(screen.getByText('BTC-USDT')).toBeTruthy());

            const unsafe = consoleErrorSpy.mock.calls.some((args: unknown[]) => {
                const msg = (args as unknown[]).map((a: unknown) => (typeof a === 'string' ? a : String(a))).join(' ');
                return msg.includes('state_unsafe_mutation');
            });
            expect(unsafe, 'initial mount must not throw state_unsafe_mutation').toBe(false);
        } finally {
            consoleErrorSpy.mockRestore();
        }
    });
});
