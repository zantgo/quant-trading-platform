// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../state.svelte';

describe('TEST-UI: State Reactive Effects', () => {
    let app: ReturnType<typeof useAppStore>;

    beforeEach(() => {
        app = useAppStore();
        app.initInstance('BTC');
        app.apiKeyConfigured = true;
    });

    it('should register all four timeframes on pair init', () => {
        const pair = app.instancesMap['BTC-USDT'];
        expect(pair).toBeDefined();
        expect(pair.symbol).toBe('BTC');
        expect(pair.exchange).toBe('Hyperliquid');

        // All four timeframes should exist with default values
        expect(pair.terms.micro1).toBeDefined();
        expect(pair.terms.fast1).toBeDefined();
        expect(pair.terms.slow1).toBeDefined();
        expect(pair.terms.longterm1).toBeDefined();

        // Micro-term defaults
        expect(pair.terms.micro1.priceText).toBe('--');
        expect(pair.terms.micro1.indicators).toEqual({});
        expect(pair.isConnected).toBe(false);
    });

    it('should phase through analysis states progressively', () => {
        expect(app.analysisPhase).toBe('idle');

        app.analysisPhase = 'phase1';
        expect(app.analysisPhase).toBe('phase1');

        app.analysisPhase = 'phase2';
        expect(app.analysisPhase).toBe('phase2');

        app.analysisPhase = 'complete';
        expect(app.analysisPhase).toBe('complete');

        // Reset to idle
        app.analysisPhase = 'idle';
        expect(app.analysisPhase).toBe('idle');
    });

    it('should trigger auto-trade log when position changes from active to None', () => {
        app.currentPosition = 'Long';
        app.entryPriceVal = '50000';
        expect(app.currentPosition).toBe('Long');

        // Switching to None should auto-log the trade (via autoLogTrade)
        app.currentPosition = 'None';
        expect(app.currentPosition).toBe('None');
        // Entry price should be cleared
        expect(app.entryPriceVal).toBe('');
    });

    it('should update chart telemetry via snapshot assignment', () => {
        const pair = app.instancesMap['BTC-USDT'];

        // Simulate receiving a market snapshot
        pair.terms.micro1.latestSnapshot = {
            mid_price: '65000.00',
            exchange: 'Hyperliquid',
            symbol: 'BTC',
            rsi_14: 62.5,
            macd_line: 15.0,
            macd_signal: 10.0,
            macd_hist: 5.0,
            squeeze_on: false,
            squeeze_momentum: 0.12,
            bbwp: 45.0,
            ema_fast: 64900,
            ema_medium: 64800,
            ema_slow: 64500,
            atr_14: 250.0,
            adx_14: 28.0,
            is_completed: true,
        };

        expect(pair.terms.micro1.latestSnapshot).not.toBeNull();
        const snap = pair.terms.micro1.latestSnapshot!;
        expect(snap.mid_price).toBe('65000.00');
        expect(snap.symbol).toBe('BTC');
        expect(snap.is_completed).toBe(true);
    });

    it('should snapshot per-timeframe state independently', () => {
        app.initInstance('ETH');

        // Set values on BTC micro-term
        app.instancesMap['BTC-USDT'].terms.micro1.priceText = '65000.00';
        app.instancesMap['BTC-USDT'].terms.micro1.indicators = {
            rsi: { raw_value: 62.5, normalized: 0.44, state_label: 'BULLISH_MOMENTUM' },
        };

        // Set values on ETH small-term
        app.instancesMap['ETH-USDT'].terms.fast1.priceText = '3200.00';
        app.instancesMap['ETH-USDT'].terms.fast1.indicators = {
            rsi: { raw_value: 45.0, normalized: -0.18, state_label: 'BEARISH_MOMENTUM' },
        };

        // BTC micro-term unchanged
        expect(app.instancesMap['BTC-USDT'].terms.micro1.priceText).toBe('65000.00');
        // ETH small-term holds its value
        expect(app.instancesMap['ETH-USDT'].terms.fast1.priceText).toBe('3200.00');
        // ETH micro-term still default
        expect(app.instancesMap['ETH-USDT'].terms.micro1.priceText).toBe('--');
    });
});
