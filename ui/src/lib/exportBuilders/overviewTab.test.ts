// Regression tests for the Overview export (v6.10.16 FIX-O1/O3):
// the per-asset signal token must agree with the hero's validity gate,
// the row R:R must come from the shared resolver, and the risk rows
// must never contradict the L5 panels.

import { describe, it, expect } from 'vitest';
import { buildOverviewTabExport } from './overviewTab';
import type { LayerHeaderSpec } from '../layerHeader';
import type { AlignmentMatrix, AnalysisMatrix, InstanceState } from '../../types';
import { DURATIONS } from '../../types';
import { makeTerms } from '../../tests/makeTerms';

const headerSpec: LayerHeaderSpec = {
  layerNumber: 7,
  layerName: 'Market Overview',
  badge: { label: 'BULLISH', color: '#22c55e', background: 'rgba(34,197,94,0.08)', state: 'valid' },
  meta: [],
  status: 'live',
};

const tf = {
  latestSnapshot: { timestamp: 1786752300 },
  priceText: '63006.0',
} as unknown as InstanceState['terms'][number];

function makeAnalysis(bias = 'Neutral'): AnalysisMatrix {
  return { symbol: 'BTC-USDC', bias, market_quality: 'Average' } as unknown as AnalysisMatrix;
}

function makeAlignment(overrides: Record<string, unknown> = {}): AlignmentMatrix {
  return {
    symbol: 'BTC-USDC',
    mtf_overall_score: 42,
    mtf_overall_label: 'WEAK_BULL_MTF',
    ...overrides,
  } as unknown as AlignmentMatrix;
}

function makeInstance(overrides: Partial<InstanceState> = {}): InstanceState {
  return {
    symbol: 'BTC-USDC',
    exchange: 'Hyperliquid',
    isConnected: true,
    instanceId: 'inst_1',
    terms: makeTerms(Object.fromEntries(DURATIONS.map((slot) => [slot, tf]))),
    historyLatestClose: '63006.0',
    currentView: { timeframe: 'micro60', symbol: 'BTC-USDC', layer: 'overview' } as never,
    alignment: null,
    analysis: makeAnalysis(),
    ...overrides,
  } as unknown as InstanceState;
}

describe('buildOverviewTabExport — I-10 KPI demotion parity', () => {
  it('demotes the KPI bias token + suffix under low coverage (parity with header)', () => {
    const inst = {
      symbol: 'BTC-USDC',
      instanceId: 'inst_btc',
      microTerm: tf,
      analysis: makeAnalysis('StrongBullish'),
    } as unknown as InstanceState;

    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: {
        global_market_bias: 'STRONG_BULLISH',
        market_health: 'HEALTHY',
        market_synchronization: 'SYNCHRONIZED',
        instance_count: 1,
        low_coverage: true,
        breadth_pct: 100,
        asset_ranking: [],
        axis_distribution: {},
        risk_distribution: { low_pct: 0, moderate_pct: 0, high_pct: 0, risk_environment: 'LOW_RISK' },
        active_symbols: ['BTC-USDC'],
      } as unknown as import('../../types').OverviewMatrix,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));

    expect(p.kpis.market_bias.value).toBe('BULLISH');
    expect(p.kpis.market_bias.sub).toContain('(1 pair)');
    // The raw wire value stays intact for data consumers.
    expect(p.overview_matrix.global_market_bias).toBe('STRONG_BULLISH');
  });

  it('keeps the STRONG token when coverage is sufficient', () => {
    const inst = {
      symbol: 'BTC-USDC',
      instanceId: 'inst_btc',
      microTerm: tf,
      analysis: makeAnalysis('StrongBullish'),
    } as unknown as InstanceState;

    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: {
        global_market_bias: 'STRONG_BULLISH',
        market_health: 'HEALTHY',
        market_synchronization: 'SYNCHRONIZED',
        instance_count: 5,
        low_coverage: false,
        breadth_pct: 100,
        asset_ranking: [],
        axis_distribution: {},
        risk_distribution: { low_pct: 0, moderate_pct: 0, high_pct: 0, risk_environment: 'LOW_RISK' },
        active_symbols: ['BTC-USDC'],
      } as unknown as import('../../types').OverviewMatrix,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));

    expect(p.kpis.market_bias.value).toBe('STRONG_BULLISH');
  });
});

// CR-15: the three suites above are gone, and so are the columns they tested.
//
//   - "FIX-O1 signal/validity gate" pinned a per-row BUY / SELL / WAIT token
//     gated on an "Actionable + READY" setup. The gate was a correctness feature
//     protecting a claim the model is not allowed to make, so the claim and the
//     gate went together.
//   - "risk rows (FIX-O3)" pinned the row's L5 risk mirror.
//   - "asset rankings top-setup columns" pinned the entry / target / stop
//     bracket, which was the top setup of the Opportunity Layer.
//
// What replaces them is a single pin on the row's surviving model fields —
// score, bias and cross-duration agreement — plus the absence of every strategy
// key. A strategy column creeping back is the failure this must catch.
describe('buildOverviewTabExport — asset ranking row is model-only', () => {
    function rowsOf(payload: string): any[] {
        return JSON.parse(payload).asset_rankings.rows;
    }

    it('carries price, bias, score and cross-duration agreement', () => {
        const out = buildOverviewTabExport({
            overviewMatrix: null,
            instances: [makeInstance({ alignment: makeAlignment(), analysis: makeAnalysis('Bullish') })],
            headerSpec,
            nowMs: 1_700_000_000_000,
        });
        const row = rowsOf(out)[0];
        expect(row.bias).toBe('Bullish');
        expect(row.mtf_label).toBe('WEAK_BULL_MTF');
        expect(typeof row.score).toBe('number');
        expect(typeof row.price_display).toBe('string');
    });

    it('CR-15: carries no strategy key at all', () => {
        const out = buildOverviewTabExport({
            overviewMatrix: null,
            instances: [makeInstance({ alignment: makeAlignment(), analysis: makeAnalysis('Bullish') })],
            headerSpec,
            nowMs: 1_700_000_000_000,
        });
        const row = rowsOf(out)[0];
        for (const gone of [
            'signal', 'direction', 'rr', 'rr_display',
            'confidence_pct', 'confidence_display', 'risk', 'risk_display',
            'entry_low', 'entry_display', 'target_low', 'target_display',
            'stop_level', 'stop_display',
        ]) {
            expect(row[gone], `strategy key ${gone} must not be exported`).toBeUndefined();
        }
    });

    it('CR-15: the payload carries no hero / KPI / card verdict', () => {
        const out = buildOverviewTabExport({
            overviewMatrix: null,
            instances: [makeInstance({ alignment: makeAlignment(), analysis: makeAnalysis('Bullish') })],
            headerSpec,
            nowMs: 1_700_000_000_000,
        });
        const payload = JSON.parse(out);
        expect(payload.hero).toBeUndefined();
        expect(payload.kpis.valid_trades).toBeUndefined();
        expect(payload.kpis.best_opportunity).toBeUndefined();
        expect(payload.kpis.avg_rr).toBeUndefined();
        expect(payload.kpis.avg_risk).toBeUndefined();
        expect(payload.cards.trade_opportunities).toBeUndefined();
        expect(payload.cards.risk_distribution).toBeUndefined();
        expect(payload.cards.signal_quality).toBeUndefined();
        expect(payload.cards.direction).toBeUndefined();
        // The three that remain are model readings.
        expect(payload.kpis.market_bias).toBeDefined();
        expect(payload.kpis.cross_tf_consensus).toBeDefined();
        expect(payload.kpis.active_pairs).toBeDefined();
        expect(payload.cards.market_alignment).toBeDefined();
    });
});
