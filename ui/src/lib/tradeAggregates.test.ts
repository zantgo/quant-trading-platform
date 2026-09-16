import { describe, it, expect } from 'vitest';
import { makeTerms } from '../tests/makeTerms';
import {
    profileDirection,
    profileRR,
    collectActiveSetups,
    computeHeroState,
    pickBestOpportunity,
    aggregateRR,
    aggregateConfidence,
    aggregateRisk,
    aggregateDirections,
    aggregateSignalQuality,
} from './tradeAggregates';
import type {
    InstanceState, OpportunityMatrix, OpportunityProfile, RiskMatrix,
} from '../types';

function makeProfile(overrides: Partial<OpportunityProfile> = {}): OpportunityProfile {
    return {
        opportunity_type: 'TrendContinuation',
        score: 80,
        preconditions_met: 3,
        preconditions_total: 4,
        notes: '',
        direction_family: 'TREND_RIDING',
        long_entry_zone: null,
        long_target_zone: null,
        long_invalidation_level: null,
        short_entry_zone: null,
        short_target_zone: null,
        short_invalidation_level: null,
        long_expected_rr_internal: 2.5,
        short_expected_rr_internal: null,
        trade_viability: 'ACTIONABLE',
        ...overrides,
    } as OpportunityProfile;
}

function makeOpportunity(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'TrendContinuation',
        opportunity_score: 80,
        setup_quality: 'STRONG',
        profiles: [],
        forecast_confidence: 0,
        contributing_signals: [],
        invalidation_note: '',
        entry_zone: { low: 0, high: 0 },
        target_zone: { low: 0, high: 0 },
        invalidation_level: 0,
        long_entry_zone: { low: 0, high: 0 },
        long_target_zone: { low: 0, high: 0 },
        long_invalidation_level: 0,
        short_entry_zone: { low: 0, high: 0 },
        short_target_zone: { low: 0, high: 0 },
        short_invalidation_level: 0,
        long_expected_rr_internal: 2.5,
        short_expected_rr_internal: 2.5,
        time_horizon: '',
        confluent_entry_levels: [],
        confluent_target_levels: [],
        confluent_invalidation_levels: [],
        ...overrides,
    } as OpportunityMatrix;
}

function makeInstance(overrides: Partial<InstanceState> = {}): InstanceState {
    return {
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        isConnected: true,
        terms: makeTerms(),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null,
        analysis: null,
        risk: null,
        advisory: null,
        decisionContext: null,
        opportunity: null,
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
        ...overrides,
    };
}

describe('profileDirection', () => {
    it('TrendRiding + Bullish = LONG', () => {
        expect(profileDirection(makeProfile({ direction_family: 'TREND_RIDING' }), 'Bullish')).toBe('LONG');
    });
    it('TrendRiding + Bearish = SHORT', () => {
        expect(profileDirection(makeProfile({ direction_family: 'TREND_RIDING' }), 'Bearish')).toBe('SHORT');
    });
    it('TrendRiding + Neutral = NEUTRAL', () => {
        expect(profileDirection(makeProfile({ direction_family: 'TREND_RIDING' }), 'Neutral')).toBe('NEUTRAL');
    });
    it('CounterTrend + Bullish = SHORT', () => {
        expect(profileDirection(makeProfile({ direction_family: 'COUNTER_TREND' }), 'Bullish')).toBe('SHORT');
    });
    it('CounterTrend + Bearish = LONG', () => {
        expect(profileDirection(makeProfile({ direction_family: 'COUNTER_TREND' }), 'Bearish')).toBe('LONG');
    });
    it('Neutral family -> NEUTRAL regardless of bias', () => {
        expect(profileDirection(makeProfile({ direction_family: 'NEUTRAL' }), 'Bullish')).toBe('NEUTRAL');
    });
    it('null family + bullish = NEUTRAL', () => {
        expect(profileDirection(makeProfile({ direction_family: null }), 'Bullish')).toBe('NEUTRAL');
    });
    it('null macro bias -> NEUTRAL', () => {
        expect(profileDirection(makeProfile({ direction_family: 'TREND_RIDING' }), null)).toBe('NEUTRAL');
    });
});

describe('profileRR', () => {
    it('returns long_expected_rr_internal when direction is LONG', () => {
        expect(profileRR(makeProfile({ long_expected_rr_internal: 2.5 }), 'LONG', 1.5)).toBe(2.5);
    });
    it('returns short_expected_rr_internal when direction is SHORT', () => {
        expect(profileRR(makeProfile({ short_expected_rr_internal: 1.8 }), 'SHORT', 1.5)).toBe(1.8);
    });
    it('falls back to aggregated when per-side is null', () => {
        expect(profileRR(makeProfile({ long_expected_rr_internal: null }), 'LONG', 1.5)).toBe(1.5);
    });
    it('falls back to aggregated when per-side is 0', () => {
        expect(profileRR(makeProfile({ long_expected_rr_internal: 0 }), 'LONG', 1.5)).toBe(1.5);
    });
    it('returns 0 when direction is NEUTRAL and aggregated is null', () => {
        expect(profileRR(makeProfile(), 'NEUTRAL', null)).toBe(0);
    });
});

describe('collectActiveSetups', () => {
    it('returns empty for empty instances', () => {
        expect(collectActiveSetups([])).toEqual([]);
    });

    it('skips instances with no opportunity', () => {
        expect(collectActiveSetups([makeInstance()])).toEqual([]);
    });

    it('skips profiles with preconditions_met = 0', () => {
        const inst = makeInstance({
            opportunity: makeOpportunity({
                profiles: [makeProfile({ preconditions_met: 0 })],
            }),
        });
        expect(collectActiveSetups([inst])).toEqual([]);
    });

    it('skips NoClearOpportunity profiles', () => {
        const inst = makeInstance({
            opportunity: makeOpportunity({
                profiles: [makeProfile({ opportunity_type: 'NoClearOpportunity', preconditions_met: 5 })],
            }),
        });
        expect(collectActiveSetups([inst])).toEqual([]);
    });

    it('collects qualifying profiles with symbol + viability + direction', () => {
        const inst = makeInstance({
            symbol: 'ETH-USDT',
            analysis: { bias: 'Bullish' } as any,
            opportunity: makeOpportunity({
                profiles: [makeProfile({ trade_viability: 'ACTIONABLE' })],
            }),
        });
        const setups = collectActiveSetups([inst]);
        expect(setups).toHaveLength(1);
        expect(setups[0].symbol).toBe('ETH-USDT');
        expect(setups[0].viability).toBe('Actionable');
        expect(setups[0].direction).toBe('LONG');
        expect(setups[0].readiness).toBe('STAND_ASIDE');  // no decision context
    });

    it('reads trade_readiness from DecisionContext', () => {
        const inst = makeInstance({
            decisionContext: { trade_readiness: 'READY' } as any,
            opportunity: makeOpportunity({
                profiles: [makeProfile({ trade_viability: 'ACTIONABLE' })],
            }),
        });
        const setups = collectActiveSetups([inst]);
        expect(setups[0].readiness).toBe('READY');
    });
});

describe('computeHeroState', () => {
    it('STAND_ASIDE for empty instances', () => {
        expect(computeHeroState([])).toBe('STAND_ASIDE');
    });

    it('STAND_ASIDE when no instance has opportunity', () => {
        expect(computeHeroState([makeInstance()])).toBe('STAND_ASIDE');
    });

    it('TRADE when at least one Actionable + READY exists', () => {
        const inst = makeInstance({
            decisionContext: { trade_readiness: 'READY' } as any,
            opportunity: makeOpportunity({
                profiles: [makeProfile({ trade_viability: 'ACTIONABLE' })],
            }),
        });
        expect(computeHeroState([inst])).toBe('TRADE');
    });

    it('TRADE requires both Actionable AND READY', () => {
        const inst = makeInstance({
            decisionContext: { trade_readiness: 'STAND_ASIDE' } as any,
            opportunity: makeOpportunity({
                profiles: [makeProfile({ trade_viability: 'ACTIONABLE' })],
            }),
        });
        expect(computeHeroState([inst])).toBe('WAIT');
    });

    it('WAIT when only DirectionalNeutral exists', () => {
        const inst = makeInstance({
            decisionContext: { trade_readiness: 'FORMING' } as any,
            opportunity: makeOpportunity({
                profiles: [makeProfile({ trade_viability: 'DIRECTIONAL_NEUTRAL' })],
            }),
        });
        expect(computeHeroState([inst])).toBe('WAIT');
    });
});

describe('pickBestOpportunity', () => {
    it('returns null when no setups', () => {
        expect(pickBestOpportunity([])).toBeNull();
    });

    it('returns highest opportunityScore among Actionable+READY', () => {
        const a = makeInstance({
            symbol: 'BTC-USDT',
            decisionContext: { trade_readiness: 'READY' } as any,
            opportunity: makeOpportunity({
                opportunity_score: 70,
                profiles: [makeProfile({ score: 70, trade_viability: 'ACTIONABLE' })],
            }),
        });
        const b = makeInstance({
            symbol: 'ETH-USDT',
            decisionContext: { trade_readiness: 'READY' } as any,
            opportunity: makeOpportunity({
                opportunity_score: 90,
                profiles: [makeProfile({ score: 90, trade_viability: 'ACTIONABLE' })],
            }),
        });
        const best = pickBestOpportunity([a, b]);
        expect(best?.symbol).toBe('ETH-USDT');
    });

    it('prefers Actionable+READY over higher-score but not ready', () => {
        const a = makeInstance({
            symbol: 'BTC-USDT',
            decisionContext: { trade_readiness: 'READY' } as any,
            opportunity: makeOpportunity({
                opportunity_score: 60,
                profiles: [makeProfile({ score: 60, trade_viability: 'ACTIONABLE' })],
            }),
        });
        const b = makeInstance({
            symbol: 'ETH-USDT',
            decisionContext: { trade_readiness: 'STAND_ASIDE' } as any,
            opportunity: makeOpportunity({
                opportunity_score: 95,
                profiles: [makeProfile({ score: 95, trade_viability: 'ACTIONABLE' })],
            }),
        });
        const best = pickBestOpportunity([a, b]);
        expect(best?.symbol).toBe('BTC-USDT');
    });
});

describe('aggregateRR', () => {
    it('returns zeros when no instances have R:R', () => {
        const r = aggregateRR([]);
        expect(r.avg).toBe(0);
        expect(r.best).toBe(0);
        expect(r.count).toBe(0);
    });

    it('averages long_expected_rr_internal by default (Bullish bias)', () => {
        const a = makeInstance({
            analysis: { bias: 'Bullish' } as any,
            opportunity: makeOpportunity({ long_expected_rr_internal: 2.0, short_expected_rr_internal: 4.0 }),
        });
        const b = makeInstance({
            analysis: { bias: 'Bullish' } as any,
            opportunity: makeOpportunity({ long_expected_rr_internal: 3.0, short_expected_rr_internal: 5.0 }),
        });
        const c = makeInstance({
            opportunity: null,
        });
        const r = aggregateRR([a, b, c]);
        expect(r.avg).toBe(2.5);
        expect(r.best).toBe(3.0);
        expect(r.count).toBe(2);
    });

    it('uses short_expected_rr_internal when bias is Bearish', () => {
        const a = makeInstance({
            analysis: { bias: 'Bearish' } as any,
            opportunity: makeOpportunity({ long_expected_rr_internal: 5.0, short_expected_rr_internal: 2.0 }),
        });
        const r = aggregateRR([a]);
        expect(r.avg).toBe(2.0);
        expect(r.best).toBe(2.0);
    });

    it('ignores zero R:R', () => {
        const a = makeInstance({
            opportunity: makeOpportunity({ long_expected_rr_internal: 0 }),
        });
        const r = aggregateRR([a]);
        expect(r.count).toBe(0);
    });
});

describe('aggregateConfidence', () => {
    it('returns zeros for no confidence', () => {
        const r = aggregateConfidence([]);
        expect(r).toEqual({ avg: 0, best: 0, count: 0 });
    });

    it('averages and finds best', () => {
        const a = makeInstance({ advisory: { confidence_assessment: 60 } as any });
        const b = makeInstance({ advisory: { confidence_assessment: 80 } as any });
        const r = aggregateConfidence([a, b]);
        expect(r.avg).toBe(70);
        expect(r.best).toBe(80);
    });
});

describe('aggregateRisk', () => {
    it('averages overall_risk.score', () => {
        const riskA: RiskMatrix = {
            overall_risk: { score: 40 } as any,
        } as any;
        const riskB: RiskMatrix = {
            overall_risk: { score: 60 } as any,
        } as any;
        const r = aggregateRisk([
            makeInstance({ risk: riskA }),
            makeInstance({ risk: riskB }),
        ]);
        expect(r.avg).toBe(50);
        expect(r.count).toBe(2);
    });
});

describe('aggregateDirections', () => {
    it('Long / Short / Neutral counts', () => {
        const a = makeInstance({ advisory: { directional_guidance: 'Long' } as any });
        const b = makeInstance({ advisory: { directional_guidance: 'StrongShort' } as any });
        const c = makeInstance({ advisory: { directional_guidance: 'Neutral' } as any });
        const d = makeInstance({ advisory: null });
        const r = aggregateDirections([a, b, c, d]);
        expect(r.long).toBe(1);
        expect(r.short).toBe(1);
        expect(r.neutral).toBe(2);
    });
});

describe('aggregateSignalQuality', () => {
    it('Strong (>=70) / Moderate (40-69) / Weak (<40)', () => {
        const a = makeInstance({ advisory: { confidence_assessment: 80 } as any });
        const b = makeInstance({ advisory: { confidence_assessment: 50 } as any });
        const c = makeInstance({ advisory: { confidence_assessment: 20 } as any });
        const d = makeInstance({ advisory: null });
        const r = aggregateSignalQuality([a, b, c, d]);
        expect(r.strong).toBe(1);
        expect(r.moderate).toBe(1);
        expect(r.weak).toBe(2);  // 20 and null
    });
});
