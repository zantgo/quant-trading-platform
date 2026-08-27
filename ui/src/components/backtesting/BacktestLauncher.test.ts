// @vitest-environment jsdom
// BacktestLauncher (v8.2) — the installer-style wizard:
// step navigation, allocation-sum guard, preseeded timeframe dropdowns,
// and the Run step's progress/cancel state machine.
import { cleanup, render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import BacktestLauncher from './BacktestLauncher.svelte';

function jsonResponse(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': 'application/json' },
    });
}

let fetchMock: ReturnType<typeof vi.fn>;

function mockFetchImpl() {
    fetchMock = vi.fn((url: string, init?: any) => {
        const u = String(url);
        if (u.includes('/api/config')) {
            return Promise.resolve(jsonResponse({
                backtest: { archive_depth_days: 180, warmup_bars: 300 },
                workspace: {},
            }));
        }
        if (u.includes('/api/backtest/coverage')) {
            return Promise.resolve(jsonResponse({
                archive_depth_days: 180,
                archive: [
                    { symbol: 'BTC-USDC', timeframe_secs: 60, candle_count: 300000, earliest_secs: 1740000000, latest_secs: 1760000000, covered_span_secs: 20000000, max_lookback_secs: 15552000, max_depth_secs: 300000, coverage_pct: 100 },
                    { symbol: 'BTC-USDC', timeframe_secs: 180, candle_count: 100000, earliest_secs: 1740000000, latest_secs: 1760000000, covered_span_secs: 20000000, max_lookback_secs: 15552000, max_depth_secs: 900000, coverage_pct: 100 },
                    { symbol: 'BTC-USDC', timeframe_secs: 300, candle_count: 60000, earliest_secs: 1740000000, latest_secs: 1760000000, covered_span_secs: 20000000, max_lookback_secs: 15552000, max_depth_secs: 1500000, coverage_pct: 100 },
                    { symbol: 'BTC-USDC', timeframe_secs: 900, candle_count: 20000, earliest_secs: 1740000000, latest_secs: 1760000000, covered_span_secs: 20000000, max_lookback_secs: 15552000, max_depth_secs: 4500000, coverage_pct: 100 },
                ],
                backfill_jobs: [],
            }));
        }
        if (u.includes('/api/backtest/run')) {
            return Promise.resolve(jsonResponse({ run_id: 7, status: 'running' }));
        }
        if (u.includes('/api/backtest/progress/7')) {
            return Promise.resolve(jsonResponse({
                run_id: 7, status: 'completed', phase: 'analyzing', pct: 100,
                message: 'analysis complete', backtest_id: 42,
            }));
        }
        if (u.includes('/api/backtest/cancel/7')) {
            return Promise.resolve(jsonResponse({ run_id: 7, status: 'cancelled' }));
        }
        return Promise.resolve(jsonResponse({}));
    });
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
}

interface LauncherProps {
    bound?: { pair: string; id: string; symbol: string } | null;
    ladder?: number[];
    depthDefault?: number;
    warmupBars?: number;
    onCompleted?: (id: number) => void;
}

function renderLauncher(props: LauncherProps = {}) {
    return render(BacktestLauncher, {
        props: {
            bound: null,
            ladder: [60, 180, 300, 900],
            depthDefault: 180,
            warmupBars: 300,
            onCompleted: () => {},
            ...props,
        },
    });
}

async function goToInstancesStep() {
    // Step 1 → 2 (Strategy) → 3 (Instances).
    fireEvent.click(screen.getByText('Continue'));
    await waitFor(() => expect(screen.getAllByText('Strategy').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getByText('Continue'));
    await waitFor(() => expect(screen.getByText('Σ allocations: 0%')).toBeTruthy());
}

beforeEach(() => {
    cleanup();
    mockFetchImpl();
});

afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
});

describe('BacktestLauncher wizard (v8.2)', () => {
    it('G29 — wizard navigation: steps advance with Continue and rewind with Back', async () => {
        renderLauncher();
        // Step 1: Environment (appears in the nav and as the section title).
        expect(screen.getAllByText('Environment').length).toBeGreaterThanOrEqual(2);
        expect(screen.getByText('Continue')).toBeTruthy();

        // Step 2: leaving without an instance is blocked (the guard fires
        // on the Continue out of the Instances step).
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Strategy').length).toBeGreaterThanOrEqual(2));
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Instances').length).toBeGreaterThanOrEqual(1));
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getByText(/Add at least one instance/)).toBeTruthy());

        // Add an instance → Continue proceeds.
        const ticker = screen.getByPlaceholderText('BTC') as HTMLInputElement;
        fireEvent.input(ticker, { target: { value: 'BTC' } });
        fireEvent.click(screen.getByText('+ Add'));
        await waitFor(() => expect(screen.getByText('Σ allocations: 10%')).toBeTruthy());
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Historical Data').length).toBeGreaterThanOrEqual(1));

        // Step 3 → 4.
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getByText('Run Backtest')).toBeTruthy());

        // Back rewinds to step 3.
        fireEvent.click(screen.getByText('Back'));
        await waitFor(() => expect(screen.getAllByText('Historical Data').length).toBeGreaterThanOrEqual(1));
    });

    it('G33 — timeframe dropdowns are the 14 tiers, preseeded 1m/3m/5m/15m', async () => {
        renderLauncher();
        await goToInstancesStep();
        // The four slots preseed 60/180/300/900 (1m/3m/5m/15m).
        const selects = document.querySelectorAll('select') as NodeListOf<HTMLSelectElement>;
        const tfSelects = Array.from(selects).filter((s) =>
            Array.from(s.options).some((o) => o.value === '86400'),
        );
        expect(tfSelects.length).toBe(4);
        const values = tfSelects.map((s) => s.value);
        expect(values).toEqual(['60', '180', '300', '900']);
        // The full tier list is available (14 tiers, 1s → 1d).
        expect(tfSelects[0].options.length).toBe(14);
        expect(tfSelects[0].options[0].value).toBe('1');
        expect(tfSelects[0].options[13].value).toBe('86400');
    });

    it('G30 — Σ allocations > 100 % blocks the run', async () => {
        renderLauncher();
        await goToInstancesStep();
        const add = (base: string, alloc: number) => {
            const ticker = screen.getByPlaceholderText('BTC') as HTMLInputElement;
            fireEvent.input(ticker, { target: { value: base } });
            const allocInput = document.querySelector('input[type="number"]') as HTMLInputElement;
            fireEvent.input(allocInput, { target: { value: String(alloc) } });
            fireEvent.click(screen.getByText('+ Add'));
        };
        // 11 instances × 10% = 110 % → over.
        for (let i = 0; i < 11; i++) {
            add(`SYM${i}`, 10);
        }
        await waitFor(() => expect(screen.getByText('Σ allocations: 110%')).toBeTruthy());
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Historical Data').length).toBeGreaterThanOrEqual(1));
        fireEvent.click(screen.getByText('Continue'));
        // On the Run step the guard blocks the launch with the error.
        await waitFor(() => expect(screen.getByText('Run Backtest')).toBeTruthy());
        fireEvent.click(screen.getByText('Run Backtest'));
        await waitFor(() => expect(screen.getByText(/Σ allocations = 110% — must be ≤ 100%/)).toBeTruthy());
        // No run POST was issued.
        const runCalls = fetchMock.mock.calls.filter((c: unknown[]) => String(c[0]).includes('/api/backtest/run'));
        expect(runCalls.length).toBe(0);
    });

    it('G31 — the Run step posts, polls progress phases, completes, and cancels', async () => {
        let completedId: number | null = null;
        // warmupBars 10 → burnIn 1d, so clamped depth 3 satisfies warmup (4d needed for 300 would be impossible with HL 1m max 3d)
        renderLauncher({ onCompleted: (id: number) => { completedId = id; }, warmupBars: 10 });
        await goToInstancesStep();
        const ticker = screen.getByPlaceholderText('BTC') as HTMLInputElement;
        fireEvent.input(ticker, { target: { value: 'BTC' } });
        fireEvent.click(screen.getByText('+ Add'));
        await waitFor(() => expect(screen.getByText('Σ allocations: 10%')).toBeTruthy());
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Historical Data').length).toBeGreaterThanOrEqual(1));
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getByText('Run Backtest')).toBeTruthy());

        fireEvent.click(screen.getByText('Run Backtest'));
        // The run POST fired and completion was handed to the caller.
        await waitFor(() => expect(completedId).toBe(42), { timeout: 5000 });
        expect(screen.getByText(/completed — the Study Report tab/)).toBeTruthy();
        const runCalls = fetchMock.mock.calls.filter((c: unknown[]) => String(c[0]).includes('/api/backtest/run'));
        expect(runCalls.length).toBe(1);
        const body = JSON.parse(runCalls[0][1]?.body as string);
        expect(body.exchange).toBe('Hyperliquid');
        expect(body.symbols[0]).toEqual({
            symbol: 'BTC-USDC',
            timeframes: [60, 180, 300, 900],
            allocation_pct: 10,
        });
        expect(body.mode).toBe('historical');
        expect(body.from_ms).toBeTypeOf('number');
        expect(body.to_ms).toBeGreaterThan(body.from_ms);
    });

    it('G31b — the cancel button POSTs the cancel endpoint', async () => {
        renderLauncher({ warmupBars: 10 });
        await goToInstancesStep();
        const ticker = screen.getByPlaceholderText('BTC') as HTMLInputElement;
        fireEvent.input(ticker, { target: { value: 'BTC' } });
        fireEvent.click(screen.getByText('+ Add'));
        await waitFor(() => expect(screen.getByText('Σ allocations: 10%')).toBeTruthy());
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getAllByText('Historical Data').length).toBeGreaterThanOrEqual(1));
        fireEvent.click(screen.getByText('Continue'));
        await waitFor(() => expect(screen.getByText('Run Backtest')).toBeTruthy());

        fireEvent.click(screen.getByText('Run Backtest'));
        await waitFor(() => expect(screen.getByText('Cancel')).toBeTruthy(), { timeout: 5000 });
        // Wait until the run POST has fired (cancel targets a started run).
        await waitFor(() => {
            const started = fetchMock.mock.calls.filter((c: unknown[]) => String(c[0]).includes('/api/backtest/run'));
            expect(started.length).toBe(1);
        }, { timeout: 5000 });
        fireEvent.click(screen.getByText('Cancel'));
        const cancelCalls = fetchMock.mock.calls.filter((c: unknown[]) => String(c[0]).includes('/api/backtest/cancel/7'));
        expect(cancelCalls.length).toBe(1);
    });
});
