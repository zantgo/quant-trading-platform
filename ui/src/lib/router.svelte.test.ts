// @vitest-environment jsdom
// Router contract tests for `lib/router.svelte.ts`.
//
// These exercise the round-trip between URLs and `RouteParams` and the
// subtle back-button path that broke the dashboard in Bug 3. The
// runtime consumers (`App.svelte::applyRoute`) layer additional
// semantics on top — `middleTab` defaults to `overview` when the URL
// omits it, and `currentView` defaults to `terminal` — but those are
// covered by the integration scenarios in `deleteInstance.test.ts`
// and the manual regression checklist.

import { describe, expect, it, beforeEach, vi } from 'vitest';
import { buildEngineHash, parseEngineHash, hashEquals, applyRouteToStore, currentHashFor, writeHash } from './router.svelte';
import { createAppStore, type AppStore } from '../state.svelte';

describe('buildEngineHash / parseEngineHash round-trip', () => {
    it('round-trips a bare engine', () => {
        const hash = buildEngineHash('market_monitor');
        expect(hash).toBe('#/engine/market_monitor');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({ engine: 'market_monitor' });
    });

    it('round-trips engine + middleTab', () => {
        const hash = buildEngineHash('market_monitor', 'workspace');
        expect(hash).toBe('#/engine/market_monitor/workspace');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({ engine: 'market_monitor', middleTab: 'workspace' });
    });

    it('round-trips engine + middleTab + instance + view', () => {
        const hash = buildEngineHash('market_monitor', 'workspace', 'BTC-USDT', 'monitor');
        expect(hash).toBe('#/engine/market_monitor/workspace/instance/BTC-USDT/view/monitor');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({
            engine: 'market_monitor',
            middleTab: 'workspace',
            instance: 'BTC-USDT',
            view: 'monitor',
        });
    });

    it('round-trips the full grammar incl. tf and run segments (v2)', () => {
        const hash = buildEngineHash(
            'market_monitor', 'workspace', 'BTC-USDT', 'terminal', 'micro3',
        );
        expect(hash).toBe('#/engine/market_monitor/workspace/instance/BTC-USDT/view/terminal/tf/micro3');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({
            engine: 'market_monitor',
            middleTab: 'workspace',
            instance: 'BTC-USDT',
            view: 'terminal',
            tf: 'micro3',
        });
        expect(buildEngineHash(parsed!.engine, parsed!.middleTab, parsed!.instance, parsed!.view, parsed!.tf, parsed!.run)).toBe(hash);
    });

    it('round-trips a BTE deep link with instance + run', () => {
        const hash = buildEngineHash('backtesting', 'study', 'BTC-USDC', undefined, undefined, '42');
        expect(hash).toBe('#/engine/backtesting/study/instance/BTC-USDC/run/42');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({
            engine: 'backtesting',
            middleTab: 'study',
            instance: 'BTC-USDC',
            run: '42',
        });
        expect(buildEngineHash(parsed!.engine, parsed!.middleTab, parsed!.instance, parsed!.view, parsed!.tf, parsed!.run)).toBe(hash);
    });

    it('round-trips a run segment without an instance (standalone run deep link)', () => {
        const hash = buildEngineHash('performance', 'history', undefined, undefined, undefined, '7');
        expect(hash).toBe('#/engine/performance/history/run/7');
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({ engine: 'performance', middleTab: 'history', run: '7' });
    });

    it('parses keyed segments in any order after the middleTab', () => {
        const parsed = parseEngineHash('#/engine/backtesting/study/run/9/instance/ETH-USDC/tf/30s');
        expect(parsed).toEqual({
            engine: 'backtesting',
            middleTab: 'study',
            instance: 'ETH-USDC',
            tf: '30s',
            run: '9',
        });
    });

    it('ignores dangling keyed segments (no value after the key)', () => {
        // A trailing `/run` with no id is not a keyed segment — the
        // catch-all treats it as a middleTab instead of throwing.
        const parsed = parseEngineHash('#/engine/backtesting/study/run');
        expect(parsed).toEqual({ engine: 'backtesting', middleTab: 'run' });
    });

    it('handles a URL with engine + view but no middleTab (legacy / direct-link)', () => {
        // The user can hand-edit a URL like `#/engine/market_monitor/instance/BTC-USDT/view/monitor`
        // (skipping middleTab) — the parser must still surface the
        // instance and view, and the round-trip must remain stable.
        const hash = '#/engine/market_monitor/instance/BTC-USDT/view/monitor';
        const parsed = parseEngineHash(hash);
        expect(parsed).toEqual({
            engine: 'market_monitor',
            instance: 'BTC-USDT',
            view: 'monitor',
        });
        // Re-build from the parsed params and compare — should match the
        // canonical form once middleTab is supplied.
        const rebuilt = buildEngineHash(parsed!.engine, parsed!.middleTab, parsed!.instance, parsed!.view);
        expect(rebuilt).toBe('#/engine/market_monitor/instance/BTC-USDT/view/monitor');
    });

    it('returns null for an empty hash', () => {
        expect(parseEngineHash('')).toBeNull();
        expect(parseEngineHash('#')).toBeNull();
        expect(parseEngineHash('#/')).toBeNull();
    });

    it('returns null for a non-engine hash', () => {
        expect(parseEngineHash('#/random/path')).toBeNull();
        expect(parseEngineHash('#something')).toBeNull();
    });
});

describe('hashEquals', () => {
    it('compares hashes ignoring the leading "#" vs "#/"', () => {
        expect(hashEquals('#/engine/market_monitor', '#engine/market_monitor')).toBe(true);
        expect(hashEquals('#/engine/market_monitor', '#/engine/data_infra')).toBe(false);
        expect(hashEquals('#/engine/market_monitor', '')).toBe(false);
    });
});describe('back/forward sequences preserve engine, instance, and view', () => {
    // Bug 3 regression: when the user presses Back from a deep
    // workspace URL to a shallow engine URL, the URL parser must
    // surface every piece the runtime needs to rebuild the right
    // state. The full handler lives in `App.svelte`; this test only
    // locks the URL semantics.

    const sequence = [
        '#/engine/market_monitor',
        '#/engine/market_monitor/workspace',
        '#/engine/market_monitor/workspace/instance/BTC-USDT',
        '#/engine/market_monitor/workspace/instance/BTC-USDT/view/monitor',
        '#/engine/market_monitor/workspace/instance/BTC-USDT/view/risk',
    ];

    it('each step parses losslessly', () => {
        for (const hash of sequence) {
            const parsed = parseEngineHash(hash);
            expect(parsed, `failed to parse ${hash}`).not.toBeNull();
            // Round-trip after buildEngineHash must produce the same
            // canonical hash (modulo the optional middleTab absent in
            // the first entry).
            if (parsed!.middleTab) {
                expect(
                    buildEngineHash(parsed!.engine, parsed!.middleTab, parsed!.instance, parsed!.view),
                ).toBe(hash);
            }
        }
    });

    it('back from view/risk to view/monitor keeps the instance', () => {
        const before = parseEngineHash(sequence[3])!;
        const after = parseEngineHash(sequence[2])!;
        expect(after.engine).toBe(before.engine);
        expect(after.middleTab).toBe(before.middleTab);
        expect(after.instance).toBe(before.instance);
        expect(after.view).toBeUndefined();
    });

    it('back to a URL with no middleTab still surfaces engine', () => {
        // sequence[0] is `#/engine/market_monitor` — engine-only, no
        // middleTab, no instance. The runtime defaults middleTab to
        // 'overview' for market_monitor (see `App.svelte::applyRoute`),
        // so back-navigation to this URL must leave the dashboard on
        // the overview tab even when state thinks it's on `workspace`.
        const parsed = parseEngineHash(sequence[0])!;
        expect(parsed.engine).toBe('market_monitor');
        expect(parsed.middleTab).toBeUndefined();
        expect(parsed.instance).toBeUndefined();
        expect(parsed.view).toBeUndefined();
    });
});
// ─── Phase 2/3: state ↔ URL wiring ────────────────────────────────────

describe('currentHashFor — per-engine serialization', () => {
    it('serializes the MME pair, sub-view, and per-pair tf', () => {
        const app = createAppStore();
        app.initInstance('BTC');
        app.selectEngine('market_monitor');
        app.middleTab = 'workspace';
        app.enterInstance('BTC-USDT');
        app.instancesMap['BTC-USDT'].currentView = 'monitor';
        app.instancesMap['BTC-USDT'].activeTf = 3;
        expect(currentHashFor(app)).toBe(
            '#/engine/market_monitor/workspace/instance/BTC-USDT/view/monitor/tf/3s',
        );
    });

    it('omits the MME view/tf defaults (terminal / 1s)', () => {
        const app = createAppStore();
        app.initInstance('BTC');
        app.enterInstance('BTC-USDT');
        app.middleTab = 'workspace';
        expect(currentHashFor(app)).toBe(
            '#/engine/market_monitor/workspace/instance/BTC-USDT',
        );
    });

    it('serializes tae/pme/pae instance selection and BTE instance + run', () => {
        const app = createAppStore();
        app.initInstance('BTC');
        app.selectedInstance = 'BTC-USDT';

        app.selectEngine('trade_automation');
        expect(currentHashFor(app)).toBe('#/engine/trade_automation/overview/instance/BTC-USDT');

        app.selectEngine('portfolio');
        expect(currentHashFor(app)).toBe('#/engine/portfolio/overview/instance/BTC-USDT');

        app.selectEngine('performance');
        app.paeSelectedRunId = 7;
        expect(currentHashFor(app)).toBe('#/engine/performance/overview/instance/BTC-USDT/run/7');

        app.selectEngine('backtesting');
        app.paeSelectedRunId = null;
        app.bteRunId = 42;
        expect(currentHashFor(app)).toBe('#/engine/backtesting/overview/instance/BTC-USDT/run/42');
    });

    it('DIE defaults to Overview, not Connectivity (v11.9 N1)', () => {
        const app = createAppStore();
        app.selectEngine('data_infra');
        expect(currentHashFor(app)).toBe('#/engine/data_infra/overview');
    });

    it('removed engines (profile / exchange_settings) parse to null', () => {
        expect(parseEngineHash('#/engine/profile/settings')).toBeNull();
        expect(parseEngineHash('#/engine/exchange_settings')).toBeNull();
    });
});

describe('applyRouteToStore', () => {
    let app: AppStore;

    beforeEach(() => {
        app = createAppStore();
        app.initInstance('BTC');
        app.initInstance('ETH');
    });

    it('applies the MME instance, view, and tf segments', () => {
        applyRouteToStore(app, parseEngineHash(
            '#/engine/market_monitor/workspace/instance/BTC-USDT/view/risk/tf/30s',
        )!);
        expect(app.currentEngine).toBe('market_monitor');
        expect(app.middleTab).toBe('workspace');
        expect(app.selectedInstance).toBe('BTC-USDT');
        expect(app.activeTab).toBe('BTC-USDT');
        expect(app.instancesMap['BTC-USDT'].currentView).toBe('risk');
        expect(app.instancesMap['BTC-USDT'].activeTf).toBe(30);
    });

    it('defaults the view to terminal and ignores an invalid tf', () => {
        applyRouteToStore(app, parseEngineHash(
            '#/engine/market_monitor/workspace/instance/BTC-USDT/tf/notASlot',
        )!);
        expect(app.instancesMap['BTC-USDT'].currentView).toBe('terminal');
        // `createInstanceState` seeds the per-pair default — an invalid
        // URL tf must leave it untouched.
        expect(app.instancesMap['BTC-USDT'].activeTf).toBe(1);
    });

    it('applies a valid tf segment onto the active pair', () => {
        applyRouteToStore(app, parseEngineHash(
            '#/engine/market_monitor/workspace/instance/BTC-USDT/tf/15s',
        )!);
        expect(app.instancesMap['BTC-USDT'].activeTf).toBe(15);
    });

    it('ignores an unknown MME pair gracefully (keeps current selection)', () => {
        app.enterInstance('BTC-USDT');
        applyRouteToStore(app, parseEngineHash(
            '#/engine/market_monitor/workspace/instance/NOPE-USDT',
        )!);
        expect(app.selectedInstance).toBe('BTC-USDT');
    });

    it('applies the shared instance for non-MME engines when the pair exists', () => {
        applyRouteToStore(app, parseEngineHash(
            '#/engine/trade_automation/overview/instance/ETH-USDT',
        )!);
        expect(app.currentEngine).toBe('trade_automation');
        expect(app.selectedInstance).toBe('ETH-USDT');
        expect(app.activeTab).toBe('ETH-USDT');
    });

    it('ignores the instance segment for a non-MME engine when the pair is unknown', () => {
        app.enterInstance('BTC-USDT');
        applyRouteToStore(app, parseEngineHash(
            '#/engine/backtesting/overview/instance/GHOST-USDT',
        )!);
        expect(app.currentEngine).toBe('backtesting');
        expect(app.selectedInstance).toBe('BTC-USDT');
    });

    it('clears the selection when back-navigating to the MME overview', () => {
        app.enterInstance('BTC-USDT');
        applyRouteToStore(app, parseEngineHash('#/engine/market_monitor/overview')!);
        expect(app.selectedInstance).toBeNull();
        expect(app.middleTab).toBe('overview');
    });

    it('restores the engine default middleTab when the URL omits it', () => {
        app.middleTab = 'workspace';
        applyRouteToStore(app, parseEngineHash('#/engine/market_monitor')!);
        expect(app.middleTab).toBe('overview');
    });

    it('loads a deep-linked BTE run into the store', async () => {
        const fetchMock = vi.fn((url: string) => {
            const u = String(url);
            if (u.endsWith('/api/backtest/42')) {
                return Promise.resolve(new Response(JSON.stringify({
                    backtest_id: 42,
                    params: { symbol: 'BTC-USDT', timeframe_secs: 60, from_secs: 1, to_secs: 2, portfolio_capital_usd: 1000 },
                    summary: { total_trades: 3, win_count: 2, loss_count: 1, win_rate: 66.7, gross_profit: 30, gross_loss: 10, profit_factor: 3, expectancy: 6.7, max_drawdown_pct: 4 },
                    stats: {},
                    trades: [],
                    equity_curve: [],
                }), { status: 200, headers: { 'content-type': 'application/json' } }));
            }
            if (u === '/api/backtest/42/portfolio') {
                return Promise.resolve(new Response(JSON.stringify({ run_id: 42, portfolio: [] }), { status: 200 }));
            }
            if (u === '/api/backtest/42/signals') {
                return Promise.resolve(new Response(JSON.stringify({ run_id: 42, count: 0, signals: [] }), { status: 200 }));
            }
            if (u === '/api/backtest/42/metrics') {
                return Promise.resolve(new Response(JSON.stringify([{ key: 'sharpe_ratio', value: '1.5' }]), { status: 200 }));
            }
            return Promise.resolve(new Response('{}', { status: 200 }));
        });
        vi.stubGlobal('fetch', fetchMock);

        applyRouteToStore(app, parseEngineHash('#/engine/backtesting/study/run/42')!);
        expect(app.bteRunId).toBe(42);
        await vi.waitFor(() => expect(app.bteResult).not.toBeNull());
        expect(app.bteResult!.backtest_id).toBe(42);
        expect(app.btePortfolio).toEqual({ run_id: 42, portfolio: [] });
        expect(app.bteSignals).toEqual({ run_id: 42, count: 0, signals: [] });
        expect(app.bteMetrics).toEqual({ sharpe_ratio: '1.5' });
        vi.unstubAllGlobals();
    });

    it('keeps the empty state and warns once for an unknown run id', async () => {
        const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
        vi.stubGlobal('fetch', vi.fn(() =>
            Promise.resolve(new Response('{"error":"not found"}', { status: 404 })),
        ));

        applyRouteToStore(app, parseEngineHash('#/engine/backtesting/study/run/999')!);
        await vi.waitFor(() => expect(app.bteError).not.toBeNull());
        expect(app.bteResult).toBeNull();
        expect(app.bteRunId).toBe(999);
        expect(warnSpy).toHaveBeenCalledTimes(1);

        // Second failure for the same id: no duplicate warning.
        await app.loadBteRun(999);
        expect(warnSpy).toHaveBeenCalledTimes(1);
        vi.unstubAllGlobals();
        warnSpy.mockRestore();
    });
});

describe('history policy — push on user nav, replace on sync', () => {
    let app: AppStore;

    beforeEach(() => {
        app = createAppStore();
        app.initInstance('BTC');
        window.location.hash = '';
    });

    it('enterInstance marks the origin user (→ pushState)', () => {
        app.enterInstance('BTC-USDT');
        expect(app.consumeNavOrigin()).toBe('user');
        // Consume-and-reset: the next read is sync again.
        expect(app.consumeNavOrigin()).toBe('sync');
    });

    it('selectEngine / switchTab mark the origin user', () => {
        app.selectEngine('backtesting');
        expect(app.consumeNavOrigin()).toBe('user');
        app.switchTab('ETH-USDT');
        expect(app.consumeNavOrigin()).toBe('user');
    });

    it('a URL-driven apply marks the origin sync (→ replaceState)', () => {
        applyRouteToStore(app, parseEngineHash('#/engine/market_monitor/overview')!, 'sync');
        expect(app.consumeNavOrigin()).toBe('sync');
    });

    it('writeHash pushes for user origin and replaces for sync', () => {
        window.location.hash = '#/engine/profile/account';
        // Spy WITHOUT replacing the implementations — jsdom's real
        // pushState/replaceState must run so `window.location.hash`
        // actually moves (the loop guard compares against it).
        const pushSpy = vi.spyOn(history, 'pushState');
        const replaceSpy = vi.spyOn(history, 'replaceState');

        expect(writeHash('#/engine/market_monitor/workspace/instance/BTC-USDT', 'user')).toBe('push');
        expect(pushSpy).toHaveBeenCalledTimes(1);
        expect(pushSpy).toHaveBeenCalledWith(null, '', '#/engine/market_monitor/workspace/instance/BTC-USDT');
        expect(window.location.hash).toBe('#/engine/market_monitor/workspace/instance/BTC-USDT');
        expect(replaceSpy).not.toHaveBeenCalled();

        // Same hash → no-op (loop guard).
        expect(writeHash('#/engine/market_monitor/workspace/instance/BTC-USDT', 'user')).toBe('none');
        expect(pushSpy).toHaveBeenCalledTimes(1);

        expect(writeHash('#/engine/data_infra/connectivity', 'sync')).toBe('replace');
        expect(replaceSpy).toHaveBeenCalledTimes(1);
        expect(pushSpy).toHaveBeenCalledTimes(1);
    });
});
