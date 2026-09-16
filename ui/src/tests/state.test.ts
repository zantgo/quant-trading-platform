// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../state.svelte';
import { applyConfigToStore } from '../lib/api.svelte';

describe('TEST-UI: Global State Runes', () => {
    let app: ReturnType<typeof useAppStore>;

    beforeEach(() => {
        app = useAppStore();
        app.analysisPhase = 'idle';
        app.currentPosition = 'None';
        app.entryPriceVal = '';
    });

    it('should initialize with default states', () => {
        expect(app.analysisPhase).toBe('idle');
        expect(app.currentPosition).toBe('None');
    });

    it('should handle position changes and validate fields', () => {
        app.currentPosition = 'Long';
        expect(app.currentPosition).toBe('Long');

        app.entryPriceVal = '3120.50';
        expect(app.entryPriceVal).toBe('3120.50');
    });

    it('should transition analysis phases progressively', () => {
        expect(app.analysisPhase).toBe('idle');

        app.analysisPhase = 'phase1';
        expect(app.analysisPhase).toBe('phase1');

        app.analysisPhase = 'phase2';
        expect(app.analysisPhase).toBe('phase2');

        app.analysisPhase = 'complete';
        expect(app.analysisPhase).toBe('complete');
    });

    it('should initialize instancesMap with exchange-symbol key', () => {
        app.initInstance('BTC');
        expect(app.instancesMap['BTC-USDT']).toBeDefined();
        expect(app.instancesMap['BTC-USDT'].symbol).toBe('BTC');
        expect(app.instancesMap['BTC-USDT'].exchange).toBe('Hyperliquid');
        expect(app.instancesMap['BTC-USDT'].terms.micro1.priceText).toBe('--');
    });

    it('should preserve full unified symbol config entries when session currency differs', () => {
        const originalCurrency = app.sessionCurrency;
        for (const key of Object.keys(app.instancesMap)) {
            app.removeInstance(key);
        }
        app.sessionCurrency = 'USDC';

        const result = applyConfigToStore(app, {
            api_key_configured: true,
            symbols: ['BTC-USDT'],
            instances: {},
            candles: { duration_seconds: 60, analysis_limit: 100 },
            indicators: {},
            indicator_registry: [],
        } as any);

        expect(result.firstPairKey).toBe('BTC-USDT');
        expect(app.instancesMap['BTC-USDT']).toBeDefined();
        expect(app.instancesMap['BTC-USDT'].symbol).toBe('BTC-USDT');
        expect(app.instancesMap['BTC-USDC']).toBeUndefined();

        app.removeInstance('BTC-USDT');
        app.sessionCurrency = originalCurrency;
    });

    it('should route snapshot data by exchange key to correct pair', () => {
        app.initInstance('BTC');
        app.initInstance('ETH');

        app.instancesMap['BTC-USDT'].terms.micro1.priceText = '50000.00';
        app.instancesMap['BTC-USDT'].terms.micro1.latestSnapshot = { mid_price: '50000.00', exchange: 'Hyperliquid', symbol: 'BTC' };

        expect(app.instancesMap['BTC-USDT'].terms.micro1.priceText).toBe('50000.00');
        expect(app.instancesMap['ETH-USDT'].terms.micro1.priceText).toBe('--');
    });

    it('should toggle apiKeyConfigured flag', () => {
        expect(app.apiKeyConfigured).toBe(true);
        app.apiKeyConfigured = false;
        expect(app.apiKeyConfigured).toBe(false);
        app.apiKeyConfigured = true;
        expect(app.apiKeyConfigured).toBe(true);
    });

    it('should restore per-mode Level 3 view when switching Level 2 modes', () => {
        app.initInstance('BTC');
        app.activeTab = 'BTC-USDT';

        app.switchMode('user');
        expect(app.currentLevel2Mode).toBe('user');
        expect(app.currentView).toBe('positions');

        app.currentView = 'costs';
        expect(app.currentView).toBe('costs');

        app.switchMode('rule');
        expect(app.currentView).toBe('rule');

        app.currentView = 'ledger';

        app.switchMode('user');
        expect(app.currentView).toBe('costs');

        app.switchMode('rule');
        expect(app.currentView).toBe('ledger');
    });

    it('should switch Level 2 modes without altering operational state', () => {
        app.initInstance('BTC');
        app.activeTab = 'BTC-USDT';

        app.switchMode('general');
        expect(app.currentLevel2Mode).toBe('general');
        app.switchMode('user');
        expect(app.currentLevel2Mode).toBe('user');
        app.switchMode('rule');
        expect(app.currentLevel2Mode).toBe('rule');
    });
});
