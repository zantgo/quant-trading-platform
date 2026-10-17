// @vitest-environment jsdom
// v7.2 PortfolioDashboard — live-data regression lock.
//
// The dashboard is fully API-driven (no placeholder arrays): it lists
// instances, polls `/api/instances/:id/portfolio` + `/api/instances/:id/safety`,
// and renders mode-aware panels — readiness board (observe) vs full
// accounting (paper/live). Mode is fixed at launch and displayed as a
// read-only chip; there is no toggle.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PortfolioDashboard from './PortfolioDashboard.svelte';

function jsonResponse(body: unknown): Response {
    return new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'content-type': 'application/json' },
    });
}

const portfolioPayload = {
    instance_id: 'inst_btc',
    symbol: 'BTC-USDC',
    mode: 'paper',
    portfolio_capital_usd: 1000,
    current_equity: '1012.40',
    peak_equity: '1015.00',
    max_drawdown_pct: '1.20',
    realized_pnl: '12.40',
    unrealized_pnl: '3.10',
    daily_pnl: '8.75',
    starting_session_equity: '1003.65',
    safety_state: 'NORMAL',
    safety_context: 'Normal risk mode.',
    consecutive_losses: { 'BTC-USDC': 0 },
    lifecycle: 'RUNNING',
    exposure: {
        gross_exposure: '310.00',
        net_exposure: '310.00',
        net_exposure_pct: '30.6',
        long_exposure: '310.00',
        short_exposure: '0.00',
        symbol_concentration: { 'BTC-USDC': '0.31' },
        max_single_pair_pct: '0.20',
    },
    capital: {
        available_margin: '1006.80',
        committed_margin: '15.50',
        margin_usage_ratio: '0.015',
        leverage_ratio: '0.31',
        margin_alert: null,
    },
    position_count: 1,
    positions: [
        {
            symbol: 'BTC-USDC',
            direction: 'LONG',
            size: '0.0100',
            entry_price: '30000.00',
            mark_price: '31000.00',
            unrealized_pnl: '10.00',
            roi_pct: '3.33',
            stop_loss_price: '29500.00',
            take_profit_price: '33000.00',
        },
    ],
};

const safetyPayload = {
    instance_id: 'inst_btc',
    safety_state: 'NORMAL',
    consecutive_losses: { 'BTC-USDC': 0 },
    peak_equity: '1015.00',
    current_equity: 1012.4,
    portfolio_capital_usd: 1000,
    context: 'Normal risk mode.',
    daily_pnl: '8.75',
    max_drawdown_pct: '1.20',
    margin_usage_ratio: '0.015',
};

function mockFetchImpl(portfolioOverride: Partial<typeof portfolioPayload> = {}) {
    const fetchMock = vi.fn((url: string, opts?: RequestInit) => {
        if (url === '/api/instances') {
            return Promise.resolve(
                jsonResponse({ instances: [{ id: 'inst_btc', pair: 'BTC-USDC', status: 'running' }] }),
            );
        }
        if (url === '/api/config') {
            return Promise.resolve(
                jsonResponse({
                    safety: {
                        consecutive_loss_caution: 3,
                        consecutive_loss_dropout: 5,
                        dropout_duration_hours: 8,
                        drawdown_limit_pct: 30,
                        max_daily_drawdown_pct: 5,
                    },
                }),
            );
        }
        if (url.includes('/portfolio')) {
            return Promise.resolve(jsonResponse({ ...portfolioPayload, ...portfolioOverride }));
        }
        if (url.includes('/safety/session-reset')) {
            return Promise.resolve(jsonResponse({ success: true }));
        }
        if (url.includes('/safety')) {
            return Promise.resolve(jsonResponse(safetyPayload));
        }
        return Promise.resolve(jsonResponse({}));
    });
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
}

beforeEach(() => {
    cleanup();
    mockFetchImpl();
});

afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
});

describe('PortfolioDashboard (v7.2 paper)', () => {
    it('renders the overview stats from the API', async () => {
        render(PortfolioDashboard);

        await waitFor(() => expect(screen.getAllByText('Peak Equity').length).toBeGreaterThan(0));
        expect(screen.getAllByText('Account Overview').length).toBeGreaterThan(0);
        expect(screen.getAllByText('PAPER').length).toBeGreaterThan(0);
        expect(screen.getAllByText('NORMAL').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Daily PnL').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Open Positions').length).toBeGreaterThan(0);
    });

    it('renders the positions panel', async () => {
        render(PortfolioDashboard, { props: { section: 'positions' } });

        await waitFor(() => expect(screen.getAllByText('LONG').length).toBeGreaterThan(0));
        expect(screen.getAllByText('Positions').length).toBeGreaterThan(0);
        expect(screen.getAllByText('BTC-USDC').length).toBeGreaterThan(0);
    });

    it('renders the exposure panel with concentration', async () => {
        render(PortfolioDashboard, { props: { section: 'exposure' } });

        await waitFor(() => expect(screen.getByText('Gross Exposure')).toBeTruthy());
        expect(screen.getByText('Symbol Concentration')).toBeTruthy();
    });

    it('renders the capital panel', async () => {
        render(PortfolioDashboard, { props: { section: 'capital' } });

        await waitFor(() => expect(screen.getByText('Available Margin')).toBeTruthy());
        expect(screen.getByText('Margin Usage')).toBeTruthy();
    });

    it('renders the safety panel and POSTs session-reset', async () => {
        render(PortfolioDashboard, { props: { section: 'safety' } });

        await waitFor(() => expect(screen.getByText('Normal risk mode.')).toBeTruthy());
        expect(screen.getByText('Consecutive Losses')).toBeTruthy();

        await fireEvent.click(screen.getAllByText(/Reset session/)[0]);
        const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
        const resetCall = calls.find((c: unknown[]) =>
            String(c[0]).includes('/safety/session-reset'),
        );
        expect(resetCall).toBeTruthy();
        expect((resetCall as unknown[])[1]).toMatchObject({ method: 'POST' });
    });

    it('shows the informational PME explainer', async () => {
        render(PortfolioDashboard);

        await waitFor(() =>
            expect(screen.getByText(/It never executes/)).toBeTruthy(),
        );
    });
});

describe('PortfolioDashboard observe readiness board (v7.2)', () => {
    it('renders the safety + capital blueprints and hides money surfaces', async () => {
        cleanup();
        mockFetchImpl({ mode: 'observe', positions: [], position_count: 0 });
        render(PortfolioDashboard);

        await waitFor(() => expect(screen.getByText('Safety Blueprint')).toBeTruthy());
        expect(screen.getAllByText('OBSERVE').length).toBeGreaterThan(0);
        expect(screen.getByText('Capital Blueprint')).toBeTruthy();
        expect(screen.getByText(/Readiness Board/i)).toBeTruthy();
        // Money surfaces are hidden in observe — no equity accounting.
        expect(screen.queryByText('Realized PnL')).toBeNull();
        expect(screen.queryByText('Reset session')).toBeNull();
    });

    it('collapses to the safety tab in observe and shows the unarmed ladder', async () => {
        cleanup();
        mockFetchImpl({ mode: 'observe', positions: [], position_count: 0 });
        render(PortfolioDashboard, { props: { section: 'safety' } });

        await waitFor(() => expect(screen.getAllByText('Consecutive Losses').length).toBeGreaterThan(0));
        expect(screen.getAllByText(/unarmed/i).length).toBeGreaterThan(0);
        // Positions/Exposure/Capital tabs are unreachable in observe — a
        // stale section falls back to the readiness overview.
        render(PortfolioDashboard, { props: { section: 'capital' } });
        await waitFor(() => expect(screen.getByText('Safety Blueprint')).toBeTruthy());
    });
});

describe('PortfolioDashboard no-instance state (v7.3)', () => {
    it('renders the SVG empty state instead of an infinite loading message', async () => {
        cleanup();
        const fetchMock = vi.fn((url: string) => {
            if (url === '/api/instances') {
                return Promise.resolve(jsonResponse({ instances: [] }));
            }
            if (url === '/api/config') {
                return Promise.resolve(jsonResponse({ safety: {} }));
            }
            return Promise.resolve(jsonResponse({}));
        });
        vi.stubGlobal('fetch', fetchMock);

        render(PortfolioDashboard);

        await waitFor(() => expect(screen.getByText('No active instance')).toBeTruthy());
        expect(screen.getByText(/Portfolio management runs per instance/)).toBeTruthy();
        expect(screen.queryByText('Loading portfolio state…')).toBeNull();
        expect(screen.getByText('NO INSTANCE')).toBeTruthy();
    });

    it('keeps the Settings tab rendered without an instance', async () => {
        cleanup();
        const fetchMock = vi.fn((url: string) => {
            if (url === '/api/instances') {
                return Promise.resolve(jsonResponse({ instances: [] }));
            }
            if (url === '/api/config') {
                return Promise.resolve(jsonResponse({
                    safety: { consecutive_loss_caution: 3, drawdown_limit_pct: 30 },
                    risk_limits: { max_single_pair_exposure_pct: 20 },
                }));
            }
            return Promise.resolve(jsonResponse({}));
        });
        vi.stubGlobal('fetch', fetchMock);

        render(PortfolioDashboard, { props: { section: 'settings' } });

        await waitFor(() => expect(screen.getByText('Safety Ladder')).toBeTruthy());
        expect(screen.getByText('Risk Limits')).toBeTruthy();
        expect(screen.queryByText('No active instance')).toBeNull();
    });
});

