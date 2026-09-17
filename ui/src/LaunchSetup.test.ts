// @vitest-environment jsdom
//
// Launch Setup wizard tests. The wizard replaces the old WelcomeGate with a
// four-step installer:
//   1. Mode        — Observe / Simulate / Execute
//   2. Environment — exchange + settlement currency (+ capital for paper,
//                    credentials for live)
//   3. Instances   — staged drafts (the fixed 10-slot ladder is applied
//                    server-side; no per-slot TF choice exists)
//   4. Review      — summary → Launch
//
// Currency contract per exchange (unchanged from WelcomeGate):
//   - Hyperliquid settles only in USDC.
//   - Bitget's dashboard exposes only USDT-M futures.

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { cleanup, render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import { vi } from 'vitest';
import LaunchSetup from './LaunchSetup.svelte';
import { useAppStore } from './state.svelte';

beforeEach(() => {
    const app = useAppStore();
    // The wizard renders the session quote; pin the real singleton to the
    // Hyperliquid default so review rows read `BTC-USDC` deterministically.
    app.session.sessionCurrency = 'USDC';
    app.session.sessionExchange = 'Hyperliquid';
    app.session.sessionMode = 'observe';
    app.session.sessionActive = false;
    app.session.sessionInterrupted = false;
    app.session.interruptedSession = null;
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => cleanup());

/** Read the text of every radio button label so we can check availability. */
function availableCurrencies(container: HTMLElement): string[] {
    const labels = container.querySelectorAll('label');
    const currencies: string[] = [];
    labels.forEach((l) => {
        const text = l.textContent?.trim() ?? '';
        if (text.includes('USDT')) currencies.push('USDT');
        if (text.includes('USDC')) currencies.push('USDC');
    });
    return Array.from(new Set(currencies));
}

/** Read whether the `Available` / `Not available` badge says a currency is enabled. */
function isCurrencyEnabled(container: HTMLElement, currency: string): boolean {
    const radios = container.querySelectorAll<HTMLInputElement>(
        `input[type="radio"][value="${currency}"]`,
    );
    if (radios.length === 0) return false;
    return !radios[0].disabled;
}

async function goToEnvironment(container: HTMLElement) {
    await fireEvent.click(screen.getByText('Continue'));
}

async function goToInstances(container: HTMLElement) {
    await goToEnvironment(container);
    await fireEvent.click(screen.getByText('Continue'));
}

/** Advance from the Instances step (3) to Review (4). */
async function goToReviewFromInstances() {
    await fireEvent.click(screen.getByText('Continue'));
}

/** Advance from the Environment step (2) to Review (4). */
async function goToReviewFromEnvironment() {
    await fireEvent.click(screen.getByText('Continue'));
    await goToReviewFromInstances();
}

/** Full path from the Mode step straight to Review. */
async function goToReview(container: HTMLElement) {
    await goToInstances(container);
    await goToReviewFromInstances();
}

describe('Launch Setup — mode selection', () => {
    it('shows only the Observe mode card (v11.7 observe-only build)', async () => {
        const { container } = await render(LaunchSetup);
        expect(container.textContent).toContain('Observe');
        // Trading modes are hidden from the UI.
        expect(container.textContent).not.toContain('Simulate');
        expect(container.textContent).not.toContain('Execute');
        expect(container.textContent).not.toContain('Real orders');
        expect(container.textContent).not.toContain('Paper');
    });

    it('observe mode shows neither capital nor credentials', async () => {
        const { container } = await render(LaunchSetup);
        // Observe is the default mode.
        await goToEnvironment(container);
        expect(container.querySelector('#launch-capital')).toBeFalsy();
        expect(container.querySelector('#launch-wallet')).toBeFalsy();
        expect(container.querySelector('#launch-api-key')).toBeFalsy();
        expect(container.textContent).toContain('no capital and no credentials');
    });
});

describe('Launch Setup — currency contract', () => {
    it('Hyperliquid exposes only USDC', async () => {
        const { container } = await render(LaunchSetup);
        await goToEnvironment(container);
        const enabled = availableCurrencies(container);
        expect(enabled).toContain('USDC');
        expect(isCurrencyEnabled(container, 'USDC')).toBe(true);
        expect(isCurrencyEnabled(container, 'USDT')).toBe(false);
    });

    it('Bitget exposes only USDT', async () => {
        const { container } = await render(LaunchSetup);
        await goToEnvironment(container);
        const exchange = container.querySelector<HTMLSelectElement>('#launch-exchange');
        await fireEvent.change(exchange!, { target: { value: 'Bitget' } });
        const enabled = availableCurrencies(container);
        expect(enabled).toContain('USDT');
        expect(isCurrencyEnabled(container, 'USDT')).toBe(true);
        expect(isCurrencyEnabled(container, 'USDC')).toBe(false);
    });
});

describe('Launch Setup — instances step', () => {
    it('offers NO per-slot timeframe picker and displays the ACTIVE ladder', async () => {
        const { container } = await render(LaunchSetup);
        await goToInstances(container);

        // v8 fixed ladder: there is no TF choice — no per-slot <select>
        // pickers exist on this step (the exchange select lives on the
        // Environment step only).
        const selects = container.querySelectorAll<HTMLSelectElement>('select');
        expect(selects.length).toBe(0);

        // v11.2: the ACTIVE ladder is displayed instead — the fastest N
        // slots of the fixed pool (default N = 5).
        expect(container.textContent).toContain('Active ladder (5): 1s · 3s · 5s · 15s · 30s');
    });

    it('adds and removes staged instances (active ladder shown per instance)', async () => {
        const { container } = await render(LaunchSetup);
        await goToInstances(container);

        const baseInput = container.querySelector<HTMLInputElement>('#launch-base');
        await fireEvent.input(baseInput!, { target: { value: 'btc' } });
        await fireEvent.click(screen.getByText('+ Add'));

        // Normalized to uppercase and shown with the quote + the ACTIVE
        // ladder label.
        expect(container.textContent).toContain('BTC');
        expect(container.textContent).toContain('Active ladder (5): 1s · 3s · 5s · 15s · 30s');

        // Duplicate rejection.
        await fireEvent.input(baseInput!, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('+ Add'));
        expect(container.textContent).toContain('already in the instance list');

        // Invalid ticker rejection.
        await fireEvent.input(baseInput!, { target: { value: '!!' } });
        await fireEvent.click(screen.getByText('+ Add'));
        expect(container.textContent).toContain('Invalid ticker');

        // Remove.
        await fireEvent.click(container.querySelector<HTMLButtonElement>('button[aria-label="Remove BTC"]')!);
        expect(container.textContent).toContain('No instances configured yet.');
    });

    it('review step lists the staged instances', async () => {
        const { container } = await render(LaunchSetup);
        await goToInstances(container);
        const baseInput = container.querySelector<HTMLInputElement>('#launch-base');
        await fireEvent.input(baseInput!, { target: { value: 'ETH' } });
        await fireEvent.click(screen.getByText('+ Add'));

        await goToReviewFromInstances();
        expect(container.textContent).toContain('Review');
        expect(container.textContent).toContain('1 configured');
        expect(container.textContent).toContain('ETH-USDC');
    });
});

describe('Launch Setup — launch orchestration', () => {
    afterEach(() => {
        vi.unstubAllGlobals();
    });

    function mockBackend() {
        const calls: { url: string; body?: unknown }[] = [];
        const fetchMock = vi.fn((url: string, opts?: RequestInit) => {
            let body: unknown = null;
            try { body = opts?.body ? JSON.parse(String(opts.body)) : null; } catch { /* noop */ }
            calls.push({ url, body });
            if (typeof url === 'string' && url.includes('/api/session/init')) {
                return Promise.resolve(new Response(JSON.stringify({ success: true }), {
                    status: 200,
                    headers: { 'content-type': 'application/json' },
                }));
            }
            if (typeof url === 'string' && url.includes('/api/instances') && !url.includes('/config')) {
                return Promise.resolve(new Response(JSON.stringify({ id: 'inst_abc' }), {
                    status: 200,
                    headers: { 'content-type': 'application/json' },
                }));
            }
            return Promise.resolve(new Response('{}', {
                status: 200,
                headers: { 'content-type': 'application/json' },
            }));
        });
        vi.stubGlobal('fetch', fetchMock);
        return { calls, fetchMock };
    }

    it('launches an observe session with staged instances', async () => {
        const { calls } = mockBackend();
        const { container } = await render(LaunchSetup);
        await goToInstances(container);
        const baseInput = container.querySelector<HTMLInputElement>('#launch-base');
        await fireEvent.input(baseInput!, { target: { value: 'BTC' } });
        await fireEvent.click(screen.getByText('+ Add'));
        await goToReviewFromInstances();
        await fireEvent.click(screen.getByText('Launch'));

        await waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(2));

        const initCall = calls.find((c) => String(c.url).includes('/api/session/init'));
        expect(initCall?.body).toMatchObject({
            mode: 'observe',
            exchange: 'Hyperliquid',
            currency: 'USDC',
        });
        // No capital submitted in observe mode.
        expect((initCall?.body as any)?.initial_capital_usd).toBeUndefined();

        const instCall = calls.find((c) => String(c.url).endsWith('/api/instances'));
        expect(instCall?.body).toMatchObject({ base: 'BTC', quote: 'USDC' });

        // v8 fixed ladder: the launch flow POSTs no per-slot TF config —
        // the backend applies the canonical ladder itself.
        const configCall = calls.find((c) => String(c.url).includes('/config'));
        expect(configCall).toBeUndefined();

        // Review shows the ACTIVE ladder.
        expect(container.textContent).toContain('Active ladder (5): 1s · 3s · 5s · 15s · 30s');
    });

    it('review marks the ladder as ACTIVE (count + durations, no picker)', async () => {
        const { container } = await render(LaunchSetup);
        await goToReview(container);
        expect(container.textContent).toContain('Timeframes');
        expect(container.textContent).toContain('Active ladder (5)');
    });

    it('surfaces a backend error from session init', async () => {
        const fetchMock = vi.fn((url: string) => {
            if (typeof url === 'string' && url.includes('/api/session/init')) {
                return Promise.resolve(new Response(
                    JSON.stringify({ success: false, error: 'Live session requires an active Hyperliquid API key' }),
                    { status: 400, headers: { 'content-type': 'application/json' } },
                ));
            }
            return Promise.resolve(new Response('{}', { status: 200, headers: { 'content-type': 'application/json' } }));
        });
        vi.stubGlobal('fetch', fetchMock);
        const { container } = await render(LaunchSetup);
        await goToReview(container);
        await fireEvent.click(screen.getByText('Launch'));
        await waitFor(() =>
            expect(container.textContent).toContain('Live session requires an active Hyperliquid API key'),
        );
    });
});

// ── v11.7 observe-only build — single mode card ──────────────────────
describe('Launch Setup — observe-only mode step (v11.7)', () => {
    it('renders exactly one mode card: Observe (no Simulate / Execute)', async () => {
        const { container } = await render(LaunchSetup);
        expect(container.textContent).toContain('Observe');
        expect(container.textContent).toContain('Market monitor');
        expect(container.textContent).not.toContain('Simulate');
        expect(container.textContent).not.toContain('Execute');
        expect(container.textContent).not.toContain('Real orders');
    });

    it('the single card is pre-selected and the step advances', async () => {
        const { container } = await render(LaunchSetup);
        const cards = container.querySelectorAll('button');
        const observeCard = Array.from(cards).find((b) => b.textContent?.includes('Observe'))!;
        expect(observeCard).toBeTruthy();
        // Continue moves past the mode step.
        await fireEvent.click(screen.getByText('Continue'));
        expect(container.textContent).toContain('Environment');
    });
});

// ── v11.6 crash recovery — interrupted-session card ───────────────────
describe('LaunchSetup — interrupted-session recovery card', () => {
    function seedInterrupted() {
        const app = useAppStore();
        app.session.sessionInterrupted = true;
        app.session.interruptedSession = {
            id: 9,
            mode: 'paper',
            exchange: 'Hyperliquid',
            currency: 'USDC',
            started_at_ms: Date.now() - 120_000,
            instance_count: 2,
        };
        return app;
    }

    it('renders the recovery card when the previous session was interrupted', async () => {
        seedInterrupted();
        const { container } = await render(LaunchSetup);
        expect(container.textContent).toContain('Interrupted session detected');
        expect(container.textContent).toContain('Recover last session');
        expect(container.textContent).toContain('Discard & start fresh');
        expect(container.textContent).toContain('2 instances');
    });

    it('does not render the card when the previous session finalized', async () => {
        const app = useAppStore();
        app.session.sessionInterrupted = false;
        app.session.interruptedSession = null;
        const { container } = await render(LaunchSetup);
        expect(container.textContent).not.toContain('Interrupted session detected');
    });

    it('Recover calls the endpoint, refreshes status and leaves the wizard', async () => {
        const app = seedInterrupted();
        const fetchMock = vi.fn(async (url: string) => {
            if (url === '/api/session/recover') {
                return { ok: true, status: 200, json: async () => ({ success: true }) } as unknown as Response;
            }
            if (url === '/api/session/status') {
                return {
                    ok: true, status: 200, json: async () => ({
                        active: true, currency: 'USDC', exchange: 'Hyperliquid',
                        instance_count: 2, mode: 'paper', interrupted: false,
                    }),
                } as unknown as Response;
            }
            return { ok: true, status: 200, json: async () => ({}) } as unknown as Response;
        });
        vi.stubGlobal('fetch', fetchMock as unknown as typeof fetch);
        const { container } = await render(LaunchSetup);
        const btn = Array.from(container.querySelectorAll('button'))
            .find((b) => b.textContent?.includes('Recover last session'))!;
        await fireEvent.click(btn);
        await waitFor(() => expect(app.session.sessionActive).toBe(true));
        const recoverCall = fetchMock.mock.calls.filter(([u]) => String(u) === '/api/session/recover');
        expect(recoverCall.length).toBe(1);
        vi.unstubAllGlobals();
    });

    it('Discard calls the endpoint and clears the interrupted state', async () => {
        const app = seedInterrupted();
        let discardCalls = 0;
        const fetchMock = vi.fn(async (url: string) => {
            if (url === '/api/session/discard') {
                discardCalls += 1;
                return { ok: true, status: 200, json: async () => ({ success: true }) } as unknown as Response;
            }
            return { ok: true, status: 200, json: async () => ({}) } as unknown as Response;
        });
        vi.stubGlobal('fetch', fetchMock as unknown as typeof fetch);
        const { container } = await render(LaunchSetup);
        const btn = Array.from(container.querySelectorAll('button'))
            .find((b) => b.textContent?.includes('Discard & start fresh'))!;
        await fireEvent.click(btn);
        await waitFor(() => expect(discardCalls).toBe(1));
        expect(app.session.sessionInterrupted).toBe(false);
        vi.unstubAllGlobals();
    });
});
