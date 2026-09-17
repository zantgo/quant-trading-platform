// Regression tests for the Overview export (v6.10.16 FIX-O1/O3):
// the per-asset signal token must agree with the hero's validity gate,
// the row R:R must come from the shared resolver, and the risk rows
// must never contradict the L5 panels.

import { describe, it, expect } from 'vitest';
import { buildOverviewTabExport } from './overviewTab';
import type { LayerHeaderSpec } from '../layerHeader';
import type { InstanceState, OpportunityMatrix, AnalysisMatrix, DecisionContext, AdvisoryMatrix, RiskMatrix } from '../../types';
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

function makeAdvisory(overrides: Partial<AdvisoryMatrix> = {}): AdvisoryMatrix {
  return {
    symbol: 'BTC-USDC',
    directional_guidance: 'Neutral',
    confidence_assessment: 25,
    final_recommendation: 'Neutral bias.',
    strategy_environment: 'MeanReversion',
    market_stance: 'Cautious',
    ...overrides,
  } as unknown as AdvisoryMatrix;
}

function makeAnalysis(bias = 'Neutral'): AnalysisMatrix {
  return { symbol: 'BTC-USDC', bias, market_quality: 'Average' } as unknown as AnalysisMatrix;
}

function makeRisk(score: number): RiskMatrix {
  return { overall_risk: { score, level: 'Moderate', state: 'Stable', confidence: 44, evidence: [] } } as unknown as RiskMatrix;
}

function makeDecisionContext(overrides: Partial<DecisionContext> = {}): DecisionContext {
  return {
    score: 0,
    bias: 'Neutral',
    score_confidence: 0,
    expected_reward_risk_ratio: 0,
    trade_readiness: 'WATCH',
    long_probability: 21,
    short_probability: 2,
    hold_probability: 77,
    net_bias_pct: 19,
    entry_danger: { score: 44.9, level: 'Moderate', state: 'Stable', confidence: 44, evidence: [] },
    contributing_indicators: [],
    ...overrides,
  } as unknown as DecisionContext;
}

function makeOpportunity(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
  return {
    symbol: 'BTC-USDC',
    primary_opportunity: 'Pullback',
    opportunity_score: 60.24,
    setup_quality: 'Moderate',
    profiles: [
      {
        opportunity_type: 'Pullback',
        score: 60.24,
        preconditions_met: 2,
        preconditions_total: 2,
        notes: 'Pullback',
        direction_family: 'NEUTRAL',
        trade_viability: 'DIRECTIONAL_NEUTRAL',
        long_expected_rr_internal: 0,
        short_expected_rr_internal: 0,
      } as never,
    ],
    long_expected_rr_internal: 0,
    short_expected_rr_internal: 0,
    forecast_confidence: 0.44,
    ...overrides,
  } as unknown as OpportunityMatrix;
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
    risk: makeRisk(43),
    advisory: makeAdvisory(),
    decisionContext: makeDecisionContext(),
    opportunity: makeOpportunity(),
    ...overrides,
  } as unknown as InstanceState;
}

describe('buildOverviewTabExport — FIX-O1 signal/validity gate', () => {
  it('a directional guidance WITHOUT an Actionable+READY setup renders WAIT (never BUY under "no READY trade")', () => {
    const inst = makeInstance({
      advisory: makeAdvisory({ directional_guidance: 'Long' }),
      decisionContext: makeDecisionContext({ bias: 'Bullish', trade_readiness: 'WATCH' }),
    });
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    // Hero gate: zero READY trades.
    expect(p.hero.state).toBe('WAIT');
    expect(p.cards.trade_opportunities.actionable_count).toBe(0);
    // Row gate: the LONG guidance must NOT surface as BUY.
    const row = p.asset_rankings.rows[0];
    expect(row.signal).toBe('WAIT');
    expect(row.direction).toBe('LONG');
  });

  it('an Actionable+READY setup renders BUY and the hero agrees (TRADE)', () => {
    const inst = makeInstance({
      advisory: makeAdvisory({ directional_guidance: 'Long' }),
      analysis: makeAnalysis('Bullish'),
      decisionContext: makeDecisionContext({ bias: 'Bullish', trade_readiness: 'READY', long_probability: 62, short_probability: 2, hold_probability: 36, net_bias_pct: 60 }),
      opportunity: makeOpportunity({
        profiles: [{
          opportunity_type: 'Breakout',
          score: 65.72,
          preconditions_met: 2,
          preconditions_total: 2,
          notes: 'Breakout',
          direction_family: 'TREND_RIDING',
          trade_viability: 'ACTIONABLE',
          long_expected_rr_internal: 2.54,
        } as never],
      }),
    });
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    expect(p.hero.state).toBe('TRADE');
    expect(p.cards.trade_opportunities.valid_setups).toBe(1);
    const row = p.asset_rankings.rows[0];
    expect(row.signal).toBe('BUY');
  });

  it('FIX-O1: row R:R comes from the shared resolver — neutral bias renders "—", never a legacy scalar', () => {
    const inst = makeInstance({
      advisory: makeAdvisory({ directional_guidance: 'Neutral' }),
      decisionContext: makeDecisionContext({ trade_readiness: 'WATCH' }),
    });
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    const row = p.asset_rankings.rows[0];
    // No directional bias → the resolver reports N/A, exactly like the
    // L4/L6 panels — the row cannot invent an R:R from the legacy scalar.
    expect(row.rr).toBeNull();
    expect(row.rr_display).toBe('—');
  });
});

describe('buildOverviewTabExport — risk rows (FIX-O3)', () => {
  it('row risk mirrors L5 overall_risk', () => {
    const inst = makeInstance({ risk: makeRisk(43) });
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    expect(p.asset_rankings.rows[0].risk).toBe(43);
    expect(p.asset_rankings.rows[0].risk_display).toBe('43');
  });
});

describe('buildOverviewTabExport — asset rankings top-setup columns', () => {
  it('row carries the top-setup entry / target / stop levels', () => {
    const inst = makeInstance({
      analysis: makeAnalysis('Bullish'),
      decisionContext: makeDecisionContext({ bias: 'Bullish', trade_readiness: 'READY' }),
      opportunity: makeOpportunity({
        profiles: [{
          opportunity_type: 'Pullback',
          score: 60,
          preconditions_met: 2,
          preconditions_total: 2,
          notes: 'Pullback',
          direction_family: 'TREND_RIDING',
          trade_viability: 'ACTIONABLE',
          long_entry_zone: { low: 60000, high: 62000 },
          long_target_zone: { low: 65000, high: 68000 },
          long_invalidation_level: 59000,
          long_expected_rr_internal: 2.5,
        } as never],
      }),
    });
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    const row = p.asset_rankings.rows[0];
    expect(row.entry_low).toBe(60000);
    expect(row.entry_display).toBe('60,000–62,000');
    expect(row.target_low).toBe(65000);
    expect(row.target_display).toBe('65,000–68,000');
    expect(row.stop_level).toBe(59000);
    expect(row.stop_display).toBe('59,000');
  });

  it('row setup columns render "—" when no bracket exists', () => {
    const inst = makeInstance(); // NEUTRAL profile, no zones
    const p = JSON.parse(buildOverviewTabExport({
      overviewMatrix: null,
      instances: [inst],
      headerSpec,
      nowMs: 1786752300000,
    }));
    const row = p.asset_rankings.rows[0];
    expect(row.entry_low).toBe(0);
    expect(row.entry_display).toBe('—');
    expect(row.target_display).toBe('—');
    expect(row.stop_display).toBe('—');
  });
});

describe('buildOverviewTabExport — I-10 KPI demotion parity', () => {
  it('demotes the KPI bias token + suffix under low coverage (parity with header)', () => {
    const inst = {
      symbol: 'BTC-USDC',
      instanceId: 'inst_btc',
      microTerm: tf,
      advisory: makeAdvisory({ directional_guidance: 'Long', confidence_assessment: 78 }),
      analysis: makeAnalysis('StrongBullish'),
      risk: makeRisk(20),
      opportunity: {
        profiles: [{ score: 82 }],
        opportunity_score: 82,
        long_expected_rr_internal: 2.5,
        short_expected_rr_internal: 0,
      } as unknown as OpportunityMatrix,
      decisionContext: makeDecisionContext({ bias: 'Bullish', trade_readiness: 'READY' }),
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
        regime_distribution: {},
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
      advisory: makeAdvisory({ directional_guidance: 'Long', confidence_assessment: 78 }),
      analysis: makeAnalysis('StrongBullish'),
      risk: makeRisk(20),
      opportunity: { profiles: [{ score: 82 }], opportunity_score: 82 } as unknown as OpportunityMatrix,
      decisionContext: makeDecisionContext({ bias: 'Bullish', trade_readiness: 'READY' }),
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
        regime_distribution: {},
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
