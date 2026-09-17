// Unit tests for the OPPORTUNITY SUMMARY paragraph generator — locks the
// grammar: awaiting fallback, no-clear fallback, conviction bands,
// opportunity prose labels, quality/horizon wording, and profile counts.

import { describe, expect, it } from 'vitest';
import type { OpportunityMatrix } from '../types';
import { buildOpportunitySummary, highlightOpportunitySummary, opportunityProseLabel, OPPORTUNITY_SUMMARY_LABEL } from './opportunitySummary';

function opp(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'Breakout',
        opportunity_score: 62,
        setup_quality: 'Moderate',
        profiles: [],
        forecast_confidence: 0.4,
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
        long_expected_rr_internal: 0,
        short_expected_rr_internal: 0,
        time_horizon: 'Intraday',
        confluent_entry_levels: [],
        confluent_target_levels: [],
        confluent_invalidation_levels: [],
        ...overrides,
    };
}

describe('buildOpportunitySummary', () => {
    it('renders the awaiting fallback when the matrix is null', () => {
        expect(buildOpportunitySummary(null)).toContain('Awaiting opportunity data');
    });

    it('renders the awaiting fallback when the matrix is undefined', () => {
        expect(buildOpportunitySummary(undefined)).toContain('Awaiting opportunity data');
    });

    it('renders the no-clear fallback when the primary is NoClearOpportunity', () => {
        const s = buildOpportunitySummary(opp({ primary_opportunity: 'NoClearOpportunity' }));
        expect(s).toContain('No clear opportunity is present');
    });

    it('composes the three-sentence grammar with conviction + quality + horizon', () => {
        const s = buildOpportunitySummary(
            opp({ primary_opportunity: 'Breakout', opportunity_score: 62, setup_quality: 'Moderate', time_horizon: 'Intraday' }),
        );
        expect(s).toContain('moderate-conviction breakout phase');
        expect(s).toContain('Setup quality is rated Moderate over an intraday horizon');
    });

    it('maps the conviction bands correctly (high / strong / low)', () => {
        expect(buildOpportunitySummary(opp({ opportunity_score: 90 }))).toContain('high-conviction');
        expect(buildOpportunitySummary(opp({ opportunity_score: 72 }))).toContain('strong-conviction');
        expect(buildOpportunitySummary(opp({ opportunity_score: 12 }))).toContain('low-conviction');
    });

    it('counts evaluated profiles and quotes the strongest qualifying one', () => {
        const s = buildOpportunitySummary(
            opp({
                profiles: [
                    { opportunity_type: 'TrendContinuation', score: 55, preconditions_met: 2, preconditions_total: 3, notes: '' } as any,
                    { opportunity_type: 'Pullback', score: 71, preconditions_met: 3, preconditions_total: 3, notes: '' } as any,
                ],
            }),
        );
        expect(s).toContain('2 candidate profiles evaluated');
        expect(s).toContain('strongest scoring 71 with 3/3 preconditions met');
    });

    it('reports zero qualifying profiles without a best score', () => {
        const s = buildOpportunitySummary(
            opp({
                profiles: [
                    { opportunity_type: 'TrendContinuation', score: 55, preconditions_met: 0, preconditions_total: 3, notes: '' } as any,
                ],
            }),
        );
        expect(s).toContain('none currently meeting their preconditions');
    });
});

describe('highlightOpportunitySummary', () => {
    it('wraps conviction tiers in strong spans', () => {
        expect(highlightOpportunitySummary('The market is in a strong-conviction breakout phase.')).toBe(
            'The market is in a <strong>strong-conviction</strong> breakout phase.',
        );
        expect(highlightOpportunitySummary('a moderate-conviction pullback phase')).toContain(
            '<strong>moderate-conviction</strong>',
        );
        expect(highlightOpportunitySummary('a high-conviction trend phase')).toContain('<strong>high-conviction</strong>');
    });

    it('wraps the quality rating and the strongest score figure', () => {
        const s = highlightOpportunitySummary(
            'Setup quality is rated Prime over an intraday horizon. 2 candidate profiles evaluated, the strongest scoring 78 with 3/3 preconditions met.',
        );
        expect(s).toContain('rated <strong>Prime</strong>');
        expect(s).toContain('strongest scoring <strong>78</strong>');
    });

    it('never nests strong spans (conviction token stays atomic)', () => {
        const s = highlightOpportunitySummary('a moderate-conviction breakout phase rated Moderate');
        expect(s).toBe(
            'a <strong>moderate-conviction</strong> breakout phase rated <strong>Moderate</strong>',
        );
    });

    it('leaves fallback prose untouched', () => {
        expect(highlightOpportunitySummary('Awaiting opportunity data — this summary will describe the active opportunity landscape once the opportunity matrix populates.')).not.toContain('<strong>');
    });
});

describe('opportunityProseLabel', () => {
    it('turns PascalCase tokens into hyphenated prose', () => {
        expect(opportunityProseLabel('TrendContinuation')).toBe('trend-continuation');
        expect(opportunityProseLabel('MeanReversion')).toBe('mean-reversion');
        expect(opportunityProseLabel('LiquiditySqueeze')).toBe('liquidity-squeeze');
    });

    it('falls back for empty tokens', () => {
        expect(opportunityProseLabel('')).toBe('opportunity');
    });
});

describe('OPPORTUNITY_SUMMARY_LABEL', () => {
    it('is the unified [Subject] Summary token', () => {
        expect(OPPORTUNITY_SUMMARY_LABEL).toBe('SUMMARY');
    });
});
