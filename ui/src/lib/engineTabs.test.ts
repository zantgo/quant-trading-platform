// engineTabs — v7.3 tab-spec regression lock.
//
// Every engine × mode combination must render exactly the documented tab
// set: `[Overview] → [L1..Ln layers] → [Settings last]`, Settings ALWAYS
// present in every mode, and every mode set ≥ 3 tabs.
import { describe, expect, it } from 'vitest';
import { ENGINE_TABS, tabsForMode, resolveEngineTabForMode } from './engineTabs';

function keys(engine: Parameters<typeof tabsForMode>[0], mode?: string): string[] {
    return tabsForMode(engine, mode).map((t) => t.key);
}

describe('engineTabs v7.3 spec', () => {
    it('Settings is always the last tab in every mode for TAE/PME/PAE', () => {
        for (const engine of ['trade_automation', 'portfolio', 'performance'] as const) {
            for (const mode of ['observe', 'paper', 'live'] as const) {
                const tabs = tabsForMode(engine, mode);
                expect(tabs[tabs.length - 1].key).toBe('settings');
            }
        }
    });

    it('every mode set has at least three tabs', () => {
        for (const engine of ['trade_automation', 'portfolio', 'performance'] as const) {
            for (const mode of ['observe', 'paper', 'live'] as const) {
                expect(tabsForMode(engine, mode).length).toBeGreaterThanOrEqual(3);
            }
        }
        // DIE / MME are mode-agnostic and already ≥ 3.
        expect(ENGINE_TABS.data_infra.length).toBeGreaterThanOrEqual(3);
        expect(ENGINE_TABS.market_monitor.length).toBeGreaterThanOrEqual(3);
    });

    it('TAE navbar per mode', () => {
        expect(keys('trade_automation', 'observe')).toEqual(['overview', 'activity', 'settings']);
        expect(keys('trade_automation', 'paper')).toEqual(['overview', 'orders', 'activity', 'history', 'settings']);
        expect(keys('trade_automation', 'live')).toEqual(['overview', 'orders', 'activity', 'history', 'settings']);
    });

    it('PME navbar per mode (v10.1: Portfolio Overview merged into Overview)', () => {
        expect(keys('portfolio', 'observe')).toEqual(['overview', 'safety', 'settings']);
        expect(keys('portfolio', 'paper')).toEqual(['overview', 'positions', 'exposure', 'capital', 'safety', 'settings']);
        expect(keys('portfolio', 'live')).toEqual(['overview', 'positions', 'exposure', 'capital', 'safety', 'settings']);
    });

    it('PAE navbar per mode (L1→L5, cross-cutting last)', () => {
        // v8: the Backtesting tab moved to the Backtesting Engine.
        expect(keys('performance', 'observe')).toEqual(['overview', 'comparison', 'history', 'methodology', 'settings']);
        expect(keys('performance', 'paper')).toEqual(['overview', 'trades', 'strategy', 'risk', 'performance', 'comparison', 'history', 'methodology', 'settings']);
        expect(keys('performance', 'live')).toEqual(['overview', 'trades', 'strategy', 'risk', 'performance', 'comparison', 'history', 'methodology', 'settings']);
    });

    it('BTE navbar v10.1: trader flow first, details demoted, Settings last', () => {
        expect(ENGINE_TABS.backtesting.map((t) => t.key)).toEqual([
            'overview', 'study', 'chart', 'history', 'die', 'mme', 'tae', 'pme', 'pae', 'settings',
        ]);
    });

    it('DIE / MME are mode-agnostic', () => {
        expect(keys('data_infra', 'observe')).toEqual(keys('data_infra', 'live'));
        expect(keys('market_monitor', 'observe')).toEqual(['overview', 'workspace', 'settings']);
        // v10.1: Connection Settings (the [workspace.api_failover] editor,
        // moved from the Home page) sits at the far right.
        expect(keys('data_infra', 'observe')).toEqual([
            'overview', 'exchange_status', 'connectivity', 'market_data',
            'clock_monitor', 'data_quality', 'distribution', 'settings',
        ]);
    });

    it('unknown mode falls back to the full (paper/live) tab set', () => {
        expect(keys('trade_automation', undefined)).toEqual(keys('trade_automation', 'paper'));
        expect(keys('performance', 'weird')).toEqual(keys('performance', 'paper'));
    });
});

describe('resolveEngineTabForMode (v7.3)', () => {
    it('resolves within the mode-appropriate tab set', () => {
        expect(resolveEngineTabForMode('trade_automation', 'orders', 'observe')).toBe('overview');
        expect(resolveEngineTabForMode('trade_automation', 'orders', 'paper')).toBe('orders');
        expect(resolveEngineTabForMode('portfolio', 'capital', 'observe')).toBe('overview');
        expect(resolveEngineTabForMode('performance', 'trades', 'observe')).toBe('overview');
        expect(resolveEngineTabForMode('performance', 'settings', 'observe')).toBe('settings');
    });

    it('stale middleTab values fall back to the engine default', () => {
        expect(resolveEngineTabForMode('data_infra', 'bogus', 'observe')).toBe('overview');
        expect(resolveEngineTabForMode('trade_automation', 'bogus', 'paper')).toBe('overview');
    });
});
