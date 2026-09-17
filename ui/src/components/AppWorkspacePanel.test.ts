// @vitest-environment jsdom
// Regression lock for the simplified right-panel UI.
//
// The dashboard model is now binary — an instance is either Running or
// it doesn't exist. There are no Pause / Start / Stop buttons, no
// stop-then-delete sequence, no Stopping chip. Each row carries only
// a Delete button, and clicking the row opens the instance (sets
// `selectedInstance` AND `middleTab` so the Workspace view actually
// renders — Bug 3's regression had `enterInstance` leaving middleTab
// stuck on whatever it was before, which is why the click looked like
// it did nothing).
//
// What these tests assert:
//   - the row click sets BOTH `selectedInstance` and `middleTab = 'workspace'`
//     so the page actually enters the instance view
//   - the row click fires `onrequestConfirm('delete', ...)` via the Delete button
//   - inline `errorMessage` renders above the list when set
//   - empty state + create bar still work
//   - the panel's `fetchWorkspaces` $effect does NOT throw
//     `state_unsafe_mutation` when the session-count dependency fires —
//     the original bug threw the error and Svelte marked the effect as
//     errored, freezing the panel until full reload.
//   - the polling backstop continues to refresh while the panel is open
//     even if the reactive effect chain ever breaks again.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AppWorkspacePanel from './layout/AppWorkspacePanel.svelte';
import { useAppStore } from '../state.svelte';

function mockFetch(instances: Array<{ id: string; pair: string; status: string }>) {
    return vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ instances }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
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

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.sessionCurrency = 'USDT';
    app.sessionExchange = 'Hyperliquid';
    // Make sure no stale `middleTab` bleeds between tests.
    app.middleTab = 'overview';
    app.selectedInstance = null;
});

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

describe('AppWorkspacePanel — simplified UI', () => {
    it('renders a single Delete button per row (no pause/start/stop)', async () => {
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        const { container } = render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        // Exactly one Delete button per row.
        const deleteBtn = container.querySelector('[title="Delete"]');
        expect(deleteBtn).toBeTruthy();
        // No Pause, Start, Stop buttons.
        expect(container.querySelector('[title="Pause"]')).toBeNull();
        expect(container.querySelector('[title="Start"]')).toBeNull();
        expect(container.querySelector('[title="Stop"]')).toBeNull();
    });

    it('clicking the row opens the instance AND sets middleTab to workspace', async () => {
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        const app = useAppStore();
        app.middleTab = 'overview'; // start somewhere other than workspace
        app.selectedInstance = null;

        const onclose = vi.fn();
        const { container } = render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose,
                onrequestConfirm: () => {},
            },
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        // Click the row body (the `<a>`). Using the status dot's
        // neighbour — the pair text — keeps us off the Delete button
        // which has its own click handler.
        const row = container.querySelector('a') as HTMLElement;
        expect(row).toBeTruthy();
        await fireEvent.click(row);

        // enterInstance set selectedInstance; the panel ALSO sets
        // middleTab = 'workspace' so the page actually navigates.
        expect(app.selectedInstance).toBe('BTC-USDT');
        expect(app.middleTab, 'row click must flip middleTab to workspace').toBe('workspace');
        expect(onclose).toHaveBeenCalled();
    });

    it('clicking the Delete button fires onrequestConfirm and does NOT navigate', async () => {
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        const app = useAppStore();
        app.middleTab = 'overview';
        app.selectedInstance = null;

        const onrequestConfirm = vi.fn();
        const onclose = vi.fn();
        const { container } = render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose,
                onrequestConfirm,
            },
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        const deleteBtn = container.querySelector('[title="Delete"]') as HTMLElement | null;
        expect(deleteBtn).toBeTruthy();
        await fireEvent.click(deleteBtn!);

        expect(onrequestConfirm).toHaveBeenCalledWith('inst_btc', 'delete', 'BTC-USDT');
        // Click on the Delete button must NOT navigate (the panel
        // calls `e.stopPropagation()`).
        expect(app.selectedInstance).toBeNull();
        expect(onclose).not.toHaveBeenCalled();
    });

    it('renders an inline error when errorMessage is set', async () => {
        vi.stubGlobal('fetch', mockFetch([]));
        render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: 'Cannot delete BTC/USDT: HTTP 500',
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });
        expect(await screen.findByText('Cannot delete BTC/USDT: HTTP 500')).toBeTruthy();
    });

    it('renders the empty-state message when the workspace has no instances', async () => {
        vi.stubGlobal('fetch', mockFetch([]));
        render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });
        await waitFor(() => expect(screen.getByText('No active instances. Create one above.')).toBeTruthy());
    });
});

describe('AppWorkspacePanel — state_unsafe_mutation regression', () => {
    // The original bug: `fetchWorkspaces()` was called from inside a
    // $effect whose sync prelude (`wsLoading = true;`) ran with
    // `current_sources` populated from the effect's tracked
    // dependencies but `wsLoading` was NOT one of them. Svelte 5
    // throws `state_unsafe_mutation` to flag the violation, marks the
    // effect as errored, and stops re-running it — so a successful
    // DELETE never refreshed the panel and the row looked "stuck".
    //
    // The fix added `await Promise.resolve()` to push the synchronous
    // state mutations past the current effect's tracking scope. These
    // tests lock that contract.

    let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
        // The Svelte error goes through `console.error`. We assert on
        // it so any future regression that re-introduces the
        // violation is caught loudly.
        consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    });

    afterEach(() => {
        consoleErrorSpy.mockRestore();
    });

    function hasUnsafeMutationError(): boolean {
        return consoleErrorSpy.mock.calls.some((args: unknown[]) => {
            const msg = (args as unknown[]).map((a: unknown) => (typeof a === 'string' ? a : String(a))).join(' ');
            return msg.includes('state_unsafe_mutation');
        });
    }

    it('does not throw state_unsafe_mutation when fetchWorkspaces runs from the $effect', async () => {
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });

        // Wait for the initial fetch to settle. The first
        // $effect-driven `fetchWorkspaces()` must NOT have thrown the
        // unsafe-mutation error.
        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());
        expect(hasUnsafeMutationError(), 'initial panel mount must not throw state_unsafe_mutation').toBe(false);

        // Drive the effect chain again by bumping sessionInstanceCount
        // and the panel's `isOpen` prop. Each trigger runs
        // `fetchWorkspaces()` synchronously; before the fix, every
        // call threw the violation.
        const app = useAppStore();
        for (let i = 0; i < 4; i++) app.sessionInstanceCount = i + 1;
        await new Promise((r) => setTimeout(r, 50));

        expect(hasUnsafeMutationError(), 'repeated effect triggers must not throw').toBe(false);
    });

    it('polling backstop issues additional fetches while the panel is open', async () => {
        // The polling backstop is a belt-and-braces against any future
        // reactivity regression in the $effect. We assert that, while
        // the panel is open, fetch() is invoked more than once over
        // time — even if the upstream `fetchWorkspaces` chain is
        // somehow broken, the timer keeps things moving.
        const fetchSpy = vi.fn(async () => new Response(
            JSON.stringify({ instances: [] }),
            { status: 200, headers: { 'content-type': 'application/json' } },
        ));
        vi.stubGlobal('fetch', fetchSpy);

        vi.useFakeTimers({ shouldAdvanceTime: true });
        try {
            render(AppWorkspacePanel, {
                props: {
                    isOpen: true,
                    wssMap: {},
                    errorMessage: null,
                    onclose: () => {},
                    onrequestConfirm: () => {},
                },
            });

            // Let the initial fetch + microtask settle.
            await vi.advanceTimersByTimeAsync(50);
            const initialCalls = fetchSpy.mock.calls.length;
            expect(initialCalls).toBeGreaterThan(0);

            // Advance ~7 s — the 3 s backstop must fire at least once.
            await vi.advanceTimersByTimeAsync(7000);
            expect(fetchSpy.mock.calls.length).toBeGreaterThan(initialCalls);
        } finally {
            vi.useRealTimers();
        }
    });

    it('stops the polling backstop when the panel closes', async () => {
        const fetchSpy = vi.fn(async () => new Response(
            JSON.stringify({ instances: [] }),
            { status: 200, headers: { 'content-type': 'application/json' } },
        ));
        vi.stubGlobal('fetch', fetchSpy);

        vi.useFakeTimers({ shouldAdvanceTime: true });
        try {
            const { rerender } = render(AppWorkspacePanel, {
                props: {
                    isOpen: true,
                    wssMap: {},
                    errorMessage: null,
                    onclose: () => {},
                    onrequestConfirm: () => {},
                },
            });

            await vi.advanceTimersByTimeAsync(50);
            const callsWhileOpen = fetchSpy.mock.calls.length;

            await rerender({
                isOpen: false,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            });

            // After closing, additional fake-timer advances must NOT
            // issue more fetches — the backstop is cleared.
            await vi.advanceTimersByTimeAsync(7000);
            expect(fetchSpy.mock.calls.length).toBe(callsWhileOpen);
        } finally {
            vi.useRealTimers();
        }
    });
});

describe('AppWorkspacePanel — close / re-open cycle does not throw', () => {
    // Regression: closing the panel used to throw
    // `state_unsafe_mutation` because App.svelte's
    // `resilientActivePair` derived mutated a `$state` cache from
    // inside its `$derived.by` body. That throw happened on every
    // panel close + every row click, and Svelte 5's effect error
    // handling prevented the close transition from completing. The
    // fix extracted the cache update into a pure helper and made the
    // cache a plain variable.

    let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
        consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    });

    afterEach(() => {
        consoleErrorSpy.mockRestore();
    });

    it('does not throw when isOpen toggles true → false → true', async () => {
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        const { rerender } = render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        // Simulate the user closing the panel.
        await rerender({
            isOpen: false,
            wssMap: {},
            errorMessage: null,
            onclose: () => {},
            onrequestConfirm: () => {},
        });

        // Re-open it.
        await rerender({
            isOpen: true,
            wssMap: {},
            errorMessage: null,
            onclose: () => {},
            onrequestConfirm: () => {},
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        const errs = consoleErrorSpy.mock.calls
            .map((args: unknown[]) => (args as unknown[]).map((a: unknown) => (typeof a === 'string' ? a : String(a))).join(' '))
            .filter((m: string) => m.includes('state_unsafe_mutation'));
        expect(errs, 'open → close → re-open cycle must not throw').toEqual([]);
    });

    it('does not throw when selectedInstance is toggled while the panel is open', async () => {
        // Toggling `selectedInstance` is the exact dependency that
        // re-fires `resilientActivePair` (a derived in App.svelte
        // that reads `app.selectedInstance`). The bug surfaced on
        // every row click because that's the path that toggles
        // `selectedInstance` AND calls `onclose()`.
        vi.stubGlobal('fetch', mockFetch([
            { id: 'inst_btc', pair: 'BTC-USDT', status: 'running' },
        ]));
        seedInstance('BTC-USDT');

        render(AppWorkspacePanel, {
            props: {
                isOpen: true,
                wssMap: {},
                errorMessage: null,
                onclose: () => {},
                onrequestConfirm: () => {},
            },
        });

        await waitFor(() => expect(screen.getByText('BTC/USDT')).toBeTruthy());

        const app = useAppStore();
        // Toggle selectedInstance through several values; each
        // change would re-fire the resilientActivePair derived in
        // a full mount of App.svelte.
        for (const k of ['BTC-USDT', null, 'BTC-USDT', null]) {
            app.selectedInstance = k;
        }

        const errs = consoleErrorSpy.mock.calls
            .map((args: unknown[]) => (args as unknown[]).map((a: unknown) => (typeof a === 'string' ? a : String(a))).join(' '))
            .filter((m: string) => m.includes('state_unsafe_mutation'));
        expect(errs, 'selectedInstance toggles must not throw').toEqual([]);
    });
});