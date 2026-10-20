import { describe, it, expect } from 'vitest';
import { makeTerms } from '../tests/makeTerms';
import { aggregateHealthBars, collectHealthBarInputs, computeMarketHealth } from './marketHealth';
import type { InstanceState, OverviewMatrix } from '../types';

describe('aggregateHealthBars', () => {
    it('returns zeroed bars for empty input', () => {
        const bars = aggregateHealthBars([], []);
        expect(bars).toHaveLength(4);
        for (const b of bars) {
            expect(b.value).toBe(0);
            expect(b.available).toBe(false);
            expect(b.contributingInstances).toBe(0);
        }
    });

    it('inverts structure_risk to Trend Strength', () => {
        const bars = aggregateHealthBars(
            [{ structureRisk: 20, executionLiquidityRisk: 50, volatilityRisk: 50, signalRisk: 50 }],
            [50],
        );
        expect(bars[0].value).toBe(80);   // 100 - 20
        expect(bars[0].invert).toBe(true);
        expect(bars[0].label).toBe('TREND STRENGTH');
    });

    it('does NOT invert volatility_risk', () => {
        const bars = aggregateHealthBars(
            [{ structureRisk: 50, executionLiquidityRisk: 50, volatilityRisk: 70, signalRisk: 50 }],
            [50],
        );
        expect(bars[2].value).toBe(70);
        expect(bars[2].invert).toBe(false);
        expect(bars[2].label).toBe('VOLATILITY');
    });

    it('excludes instances with liq confidence=0 from Liquidity bucket', () => {
        const bars = aggregateHealthBars(
            [
                { structureRisk: 50, executionLiquidityRisk: 30, volatilityRisk: 50, signalRisk: 50 },
                { structureRisk: 50, executionLiquidityRisk: 80, volatilityRisk: 50, signalRisk: 50 },
            ],
            [60, 0],  // second instance has liquidity feed OFF
        );
        // Only the first instance contributes
        expect(bars[1].value).toBe(70);   // 100 - 30
        expect(bars[1].contributingInstances).toBe(1);
    });

    it('averages across multiple instances', () => {
        const bars = aggregateHealthBars(
            [
                { structureRisk: 20, executionLiquidityRisk: 30, volatilityRisk: 40, signalRisk: 30 },
                { structureRisk: 40, executionLiquidityRisk: 70, volatilityRisk: 60, signalRisk: 50 },
            ],
            [50, 50],
        );
        expect(bars[0].value).toBe(70);   // avg(80, 60)
        expect(bars[1].value).toBe(50);   // avg(70, 30)
        expect(bars[2].value).toBe(50);   // avg(40, 60)
        expect(bars[3].value).toBe(60);   // avg(70, 50)
    });

    it('mixes available and not-available buckets', () => {
        const bars = aggregateHealthBars(
            [{ structureRisk: 50, executionLiquidityRisk: 50, volatilityRisk: 50, signalRisk: 50 }],
            [0],   // liquidity feed OFF
        );
        expect(bars[0].available).toBe(true);
        expect(bars[1].available).toBe(false);
        expect(bars[2].available).toBe(true);
        expect(bars[3].available).toBe(true);
    });

    it('handles all four buckets returning available=false when all NaN', () => {
        const bars = aggregateHealthBars(
            [{ structureRisk: NaN, executionLiquidityRisk: NaN, volatilityRisk: NaN, signalRisk: NaN }],
            [0],
        );
        for (const b of bars) {
            expect(b.available).toBe(false);
            expect(b.value).toBe(0);
        }
    });
});

describe('collectHealthBarInputs', () => {
    function makeInstance(withRisk: boolean): InstanceState {
        return {
            symbol: 'BTC-USDT',
            exchange: 'Hyperliquid',
            isConnected: true,
            terms: makeTerms(),
            historyLatestClose: '0',
            currentView: 'terminal',
            alignment: null,
                lastMatrixTimestampBySlot: {},
            lastCompletedClose: null,
            automationEnabled: false,
            automationIntervalMode: 'interval',
            automationIntervalValue: 900,
            automationIntervalUnit: 'seconds',
            priceLineMode: false,
            slowIntervalSecs: 900,
            normalIntervalSecs: 300,
            fastIntervalSecs: 60,
            showEmaFast: false,
            showEmaMedium: false,
            showEmaSlow: false,
            showEmaLong: false,
        };
    }

    it('skips instances without risk matrix', () => {
        const result = collectHealthBarInputs([makeInstance(false), makeInstance(false)]);
        expect(result.inputs).toEqual([]);
        expect(result.liqConfidence).toEqual([]);
    });

    // CR-15: this used to assert the four L5 risk dimensions were extracted
    // from `inst.risk`. That matrix no longer crosses the wire, so the function
    // has no input to read — and the invariant worth pinning is that it returns
    // NOTHING rather than four zero-scored bars, which the card would render as
    // "measured, and calm".
    it('yields no bar inputs — the L5 risk matrix is gone', () => {
        const result = collectHealthBarInputs([makeInstance(true), makeInstance(false)]);
        expect(result.inputs).toEqual([]);
        expect(result.liqConfidence).toEqual([]);
    });
});

describe('computeMarketHealth', () => {
    function makeInstance(withRisk: boolean): InstanceState {
        return {
            symbol: 'BTC-USDT',
            exchange: 'Hyperliquid',
            isConnected: true,
            terms: makeTerms(),
            historyLatestClose: '0',
            currentView: 'terminal',
            alignment: null,
                lastMatrixTimestampBySlot: {},
            lastCompletedClose: null,
            automationEnabled: false,
            automationIntervalMode: 'interval',
            automationIntervalValue: 900,
            automationIntervalUnit: 'seconds',
            priceLineMode: false,
            slowIntervalSecs: 900,
            normalIntervalSecs: 300,
            fastIntervalSecs: 60,
            showEmaFast: false,
            showEmaMedium: false,
            showEmaSlow: false,
            showEmaLong: false,
        };
    }

    it('returns null overall/sync when overview is null', () => {
        const result = computeMarketHealth([makeInstance(true)], null);
        expect(result.overall).toBeNull();
        expect(result.sync).toBeNull();
        expect(result.bars).toHaveLength(4);
    });

    it('uses overview matrix for overall/sync when available', () => {
        const overview: OverviewMatrix = {
            global_market_bias: 'BULLISH',
            market_breadth: 'POSITIVE',
            low_coverage: false,
            breadth_pct: 50,
            axis_distribution: {},
            opportunity_distribution: {},
            risk_distribution: { low_pct: 60, moderate_pct: 30, high_pct: 10, risk_environment: 'LOW_RISK' },
            asset_ranking: [],
            market_synchronization: 'SYNCHRONIZED',
            market_health: 'HEALTHY',
            global_summary: '',
            instance_count: 1,
            active_symbols: ['BTC-USDT'],
        } as OverviewMatrix;
        const result = computeMarketHealth([makeInstance(true)], overview);
        expect(result.overall).toBe('HEALTHY');
        expect(result.sync).toBe('SYNCHRONIZED');
    });

    // CR-15: `activeInstanceCount` counted instances that contributed a risk
    // bar. With no bars, it is 0 — and the card says "no data" rather than
    // claiming an instance contributed.
    it('reports no contributing instances once the risk bars are gone', () => {
        const result = computeMarketHealth([makeInstance(true), makeInstance(false)], null);
        expect(result.activeInstanceCount).toBe(0);
    });
});
