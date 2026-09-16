// @vitest-environment jsdom
//
// Polling-and-fetch lifecycle for the L7 OverviewMatrix endpoint.
// Validates that `fetchOverview` is idempotent, tolerates transient
// network failures, and reacts to `startOverviewPolling` /
// `stopOverviewPolling` correctly.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from '../state.svelte';
import type { OverviewMatrix } from '../types';

const sampleOverview: OverviewMatrix = {
    global_market_bias: 'BULLISH',
    market_breadth: 'POSITIVE',
    low_coverage: false,
    breadth_pct: 50,
    regime_distribution: { TRENDING: 0.7, RANGE: 0.3 },
    opportunity_distribution: { BREAKOUT: 2, TREND_CONTINUATION: 1 },
    risk_distribution: { low_pct: 60, moderate_pct: 30, high_pct: 10, risk_environment: 'LOW_RISK' },
    asset_ranking: [],
    market_synchronization: 'SYNCHRONIZED',
    market_health: 'HEALTHY',
    global_summary: 'Test summary',
    instance_count: 3,
    active_symbols: ['BTC-USDT', 'ETH-USDT'],
} as OverviewMatrix;

beforeEach(() => {
    const app = useAppStore();
    app.overviewMatrix = null;
    // Ensure no leftover timer from a previous test.
    app.stopOverviewPolling();
    vi.useRealTimers();
});

afterEach(() => {
    const app = useAppStore();
    app.stopOverviewPolling();
    vi.restoreAllMocks();
});

describe('AppStore.fetchOverview', () => {
    it('assigns overviewMatrix on a successful fetch', async () => {
        const app = useAppStore();
        const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
            ok: true,
            json: async () => sampleOverview,
        } as Response);

        await app.fetchOverview();

        expect(spy).toHaveBeenCalledWith('/api/overview', expect.objectContaining({
            headers: expect.objectContaining({ Accept: 'application/json' }),
        }));
        expect(app.overviewMatrix).toMatchObject(sampleOverview);
    });

    it('keeps the previous matrix on a transient network failure', async () => {
        const app = useAppStore();
        app.overviewMatrix = sampleOverview;
        vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('NetworkError'));

        await app.fetchOverview();

        expect(app.overviewMatrix).toMatchObject(sampleOverview);
    });

    it('keeps the previous matrix on a non-2xx response', async () => {
        const app = useAppStore();
        app.overviewMatrix = sampleOverview;
        vi.spyOn(globalThis, 'fetch').mockResolvedValue({
            ok: false,
            status: 500,
        } as Response);

        await app.fetchOverview();

        expect(app.overviewMatrix).toMatchObject(sampleOverview);
    });

    it('is idempotent on concurrent calls (no in-flight races)', async () => {
        const app = useAppStore();
        let resolveFn: (v: Response) => void = () => {};
        const pending = new Promise<Response>((resolve) => { resolveFn = resolve; });
        const spy = vi.spyOn(globalThis, 'fetch').mockReturnValue(pending);

        const p1 = app.fetchOverview();
        const p2 = app.fetchOverview();
        const p3 = app.fetchOverview();

        // Only one network call should have been kicked off.
        expect(spy).toHaveBeenCalledTimes(1);

        resolveFn({
            ok: true,
            json: async () => sampleOverview,
        } as Response);

        await Promise.all([p1, p2, p3]);
        expect(app.overviewMatrix).toEqual(sampleOverview);
    });
});

describe('AppStore.startOverviewPolling', () => {
    // Phase 4: the FIRST tick fires after a one-off ±250 ms module-level
    // jitter (TAB_JITTER_MS) so multiple browser tabs poll out of phase;
    // every later cycle stays exactly `intervalMs` apart. These fixtures
    // therefore wait out the worst-case jitter (250 ms) before asserting.
    const JITTER_BUDGET_MS = 400;

    it('fires the first fetch after the startup jitter and then on every interval tick', async () => {
        const app = useAppStore();
        const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
            ok: true,
            json: async () => sampleOverview,
        } as Response);

        // Phase A — first fetch: exactly one call once the ≤ 250 ms
        // jitter window has elapsed. Interval is 60 s so the 400 ms
        // wait cannot catch a second tick.
        app.startOverviewPolling(60_000);
        await new Promise((r) => setTimeout(r, JITTER_BUDGET_MS));
        expect(spy).toHaveBeenCalledTimes(1);

        // Phase B — restart with a fast interval: the loop keeps
        // ticking every `intervalMs` after the first jittered tick.
        app.stopOverviewPolling();
        const callsAfterFirst = spy.mock.calls.length;
        app.startOverviewPolling(60);
        await new Promise((r) => setTimeout(r, JITTER_BUDGET_MS + 120));
        expect(spy.mock.calls.length).toBeGreaterThanOrEqual(callsAfterFirst + 2);
    });

    it('does not double-start when called twice', async () => {
        const app = useAppStore();
        const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
            ok: true,
            json: async () => sampleOverview,
        } as Response);

        app.startOverviewPolling(60_000);
        app.startOverviewPolling(60_000);
        app.startOverviewPolling(60_000);

        // Single first fetch (after the jitter) — the duplicate calls
        // did not stack timers.
        await new Promise((r) => setTimeout(r, JITTER_BUDGET_MS));
        expect(spy).toHaveBeenCalledTimes(1);
    });

    it('stopOverviewPolling cancels the interval', async () => {
        const app = useAppStore();
        const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
            ok: true,
            json: async () => sampleOverview,
        } as Response);

        app.startOverviewPolling(60);
        await new Promise((r) => setTimeout(r, 20));
        app.stopOverviewPolling();
        const callsAfterStop = spy.mock.calls.length;
        await new Promise((r) => setTimeout(r, 200));
        expect(spy.mock.calls.length).toBe(callsAfterStop);
    });
});
