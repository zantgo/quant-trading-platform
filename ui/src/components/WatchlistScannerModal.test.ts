// @vitest-environment jsdom
// Tests for the Watchlist Scanner modal — three phases (input, running,
// done) over a single dialog. The modal's async dependencies
// (`createInstance`, `waitForModelState`, `connectWsForInstance`) are
// mocked at the module level so the test never hits the network.
//
// CR-15: the modal no longer deletes anything it added. It used to remove a
// pair whose wait window closed without a READY trade call — a rule reading
// `trade_readiness` and `directional_guidance`, both of which are gone. With
// the inputs gone every pair resolved DELETE and the scanner removed everything
// it had just added. The tests below pin the replacement: a timeout and an
// unresolved model state are NOTES on a pair that stays.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AlignmentMatrix } from '../types';
import { useAppStore } from '../state.svelte';

const mockCreateInstance = vi.fn();
const mockDeleteInstanceById = vi.fn();
const mockWaitForModelState = vi.fn();
const mockConnectWsForInstance = vi.fn();

vi.mock('../lib/api.svelte', () => ({
    createInstance: (...args: unknown[]) => mockCreateInstance(...args),
    deleteInstanceById: (...args: unknown[]) => mockDeleteInstanceById(...args),
    waitForModelState: (...args: unknown[]) => mockWaitForModelState(...args),
}));

vi.mock('../lib/websocket.svelte', () => ({
    connectWsForInstance: (...args: unknown[]) => mockConnectWsForInstance(...args),
}));

import WatchlistScannerModal from './WatchlistScannerModal.svelte';

function makeAlignment(overrides: Partial<AlignmentMatrix> = {}): AlignmentMatrix {
    return {
        mtf_overall_score: 42,
        mtf_stance: 'bull',
        ...overrides,
    } as AlignmentMatrix;
}

beforeEach(() => {
    mockCreateInstance.mockReset();
    mockDeleteInstanceById.mockReset();
    mockWaitForModelState.mockReset();
    mockConnectWsForInstance.mockReset();
    mockDeleteInstanceById.mockResolvedValue(true);

    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.sessionActive = true;
    app.sessionCurrency = 'USDT';
    app.sessionExchange = 'Hyperliquid';
});

afterEach(() => {
    cleanup();
    vi.useRealTimers();
});

describe('WatchlistScannerModal — input phase', () => {
    it('renders input textarea + Continue button when open and session active', () => {
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        expect(screen.getByLabelText('Watchlist symbols')).toBeTruthy();
        expect(screen.getByText('Continue')).toBeTruthy();
        expect(screen.getByText('Cancel')).toBeTruthy();
    });

    it('does not render when isOpen is false', () => {
        const { container } = render(WatchlistScannerModal, {
            props: { isOpen: false, wssMap: {}, onclose: () => {} },
        });
        expect(container.querySelector('[role="dialog"]')).toBeNull();
    });

    it('disables Continue when no symbols are parsed', async () => {
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const continueBtn = screen.getByText('Continue') as HTMLButtonElement;
        expect(continueBtn.disabled).toBe(true);
    });

    it('renders the wait-window input defaulting to 5 minutes', () => {
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const waitInput = screen.getByLabelText('Wait window (minutes)') as HTMLInputElement;
        expect(waitInput).toBeTruthy();
        expect(waitInput.value).toBe('5');
        expect(waitInput.min).toBe('1');
        expect(waitInput.max).toBe('60');
    });

    it('passes the configured wait window (minutes → ms) to waitForModelState', async () => {
        mockCreateInstance.mockResolvedValue({ ok: true, instanceId: 'inst_btc' });
        mockWaitForModelState.mockResolvedValue({ status: 'TIMEOUT', waitedMs: 420_000 });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const waitInput = screen.getByLabelText('Wait window (minutes)') as HTMLInputElement;
        await fireEvent.input(waitInput, { target: { value: '7' } });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        // 7 minutes → 420,000 ms window.
        expect(mockWaitForModelState).toHaveBeenCalledWith(
            expect.anything(),
            expect.any(String),
            420_000,
        );
    });

    it('shows session-not-ready banner when session is inactive', () => {
        const app = useAppStore();
        app.sessionActive = false;
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        expect(screen.getByText(/Initialize a session/)).toBeTruthy();
    });

    it('disables Continue when session is inactive', () => {
        const app = useAppStore();
        app.sessionActive = false;
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const continueBtn = screen.getByText('Continue') as HTMLButtonElement;
        expect(continueBtn.disabled).toBe(true);
    });
});

describe('WatchlistScannerModal — run phase', () => {
    it('advances to running phase on Continue', async () => {
        mockCreateInstance.mockResolvedValue({ ok: false, error: 'simulated' });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC ETH' } });
        const continueBtn = screen.getByText('Continue');
        await fireEvent.click(continueBtn);

        await waitFor(() => expect(screen.getByText('Watchlist scan')).toBeTruthy());
        expect(mockCreateInstance).toHaveBeenCalled();
    });

    // CR-15: the four cases below used to assert deletions. They now assert the
    // opposite — every pair that was added survives, whatever the model (or the
    // lack of it) said.
    it('keeps both pairs when both resolve a model state', async () => {
        mockCreateInstance
            .mockResolvedValueOnce({ ok: true, instanceId: 'inst_btc' })
            .mockResolvedValueOnce({ ok: true, instanceId: 'inst_eth' });
        mockWaitForModelState
            .mockResolvedValueOnce({
                status: 'READY',
                alignment: makeAlignment(),
            })
            .mockResolvedValueOnce({
                status: 'READY',
                alignment: makeAlignment(),
            });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC ETH' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(screen.getAllByText('Kept').length).toBeGreaterThanOrEqual(1);
        expect(mockDeleteInstanceById).not.toHaveBeenCalled();
    });

    it('CR-15: keeps the pair on timeout — a silent model is not a reason to delete', async () => {
        mockCreateInstance.mockResolvedValue({ ok: true, instanceId: 'inst_btc' });
        mockWaitForModelState.mockResolvedValue({ status: 'TIMEOUT' });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(
            mockDeleteInstanceById,
            'CR-15: a timeout must not delete the instance the operator added',
        ).not.toHaveBeenCalled();
        expect(screen.getAllByText('Kept').length).toBeGreaterThanOrEqual(1);
    });

    it('CR-15: keeps the pair on a bearish model state', async () => {
        mockCreateInstance.mockResolvedValue({ ok: true, instanceId: 'inst_sol' });
        mockWaitForModelState.mockResolvedValue({
            status: 'READY',
            alignment: makeAlignment({ mtf_stance: 'strong_bear' }),
        });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'SOL' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(mockDeleteInstanceById).not.toHaveBeenCalled();
    });

    it('CR-15: keeps the pair on a NEUTRAL model state', async () => {
        // This is the exact case the old rule DELETEd ("direction neutral"), on
        // the grounds that a market with no directional call is not worth a
        // workspace slot. That is a trading judgement, not an observe one.
        mockCreateInstance.mockResolvedValue({ ok: true, instanceId: 'inst_avax' });
        mockWaitForModelState.mockResolvedValue({
            status: 'READY',
            alignment: makeAlignment({ mtf_stance: 'neutral', mtf_overall_score: 0 }),
        });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'AVAX' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(
            mockDeleteInstanceById,
            'CR-15: a neutral model state must not delete the pair',
        ).not.toHaveBeenCalled();
    });

    it('skips duplicate pair without deleting anything', async () => {
        mockCreateInstance.mockResolvedValue({
            ok: false,
            error: 'Instance for pair BTC-USDT already exists',
        });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(mockDeleteInstanceById).not.toHaveBeenCalled();
        expect(mockWaitForModelState).not.toHaveBeenCalled();
    });
});

describe('WatchlistScannerModal — done phase', () => {
    it('Accept button closes the modal', async () => {
        mockCreateInstance.mockResolvedValue({ ok: false, error: 'simulated' });

        const onclose = vi.fn();
        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        await fireEvent.click(screen.getByText('Accept'));
        expect(onclose).toHaveBeenCalled();
    });

    // CR-15: this counted one kept and one removed (ETH, on timeout). Nothing is
    // removed any more, so both are kept — and the summary must not claim
    // otherwise.
    it('renders summary stats with both pairs kept', async () => {
        mockCreateInstance
            .mockResolvedValueOnce({ ok: true, instanceId: 'inst_btc' })
            .mockResolvedValueOnce({ ok: true, instanceId: 'inst_eth' });
        mockWaitForModelState
            .mockResolvedValueOnce({
                status: 'READY',
                alignment: makeAlignment(),
            })
            .mockResolvedValueOnce({ status: 'TIMEOUT' });

        render(WatchlistScannerModal, {
            props: { isOpen: true, wssMap: {}, onclose: () => {} },
        });
        const textarea = screen.getByLabelText('Watchlist symbols') as HTMLTextAreaElement;
        await fireEvent.input(textarea, { target: { value: 'BTC ETH' } });
        await fireEvent.click(screen.getByText('Continue'));

        await waitFor(() => expect(screen.getByText('Accept')).toBeTruthy());
        expect(screen.getAllByText('Kept').length).toBeGreaterThanOrEqual(1);
        expect(
            screen.queryAllByText('Removed').length,
            'CR-15: no pair is removed, so the Removed count must read zero',
        ).toBe(0);
    });
});
