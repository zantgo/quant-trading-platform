// @vitest-environment jsdom
//
// MarketAlignmentCard — system-wide cross-timeframe alignment
// synthesis. Reads three optional fields from `OverviewMatrix`:
//   1. `alignment_distribution` (count of symbols per label)
//   2. `alignment_consensus_index` (mean of mtf_overall_score)
//   3. `multi_tf_agreement_pct` (mean of trend_agreement_pct)
//
// These tests cover the empty state (no data yet), a populated
// bullish market, a populated bearish market, and the neutral
// "zero consensus" path. The component is intentionally driven from
// the global app store (`useAppStore()`) rather than props so the
// source of truth remains `app.overviewMatrix` — the same path the
// live dashboard uses.

import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import MarketAlignmentCard from './MarketAlignmentCard.svelte';
import { useAppStore } from '../../state.svelte';
import type { OverviewMatrix } from '../../types';

function makeOverview(overrides: Partial<OverviewMatrix> = {}): OverviewMatrix {
    return {
        global_market_bias: 'Bullish',
        market_breadth: 'Positive',
        axis_distribution: {},
        opportunity_distribution: {},
        risk_distribution: { low_pct: 50, moderate_pct: 40, high_pct: 10, risk_environment: 'LOW_RISK' },
        asset_ranking: [],
        market_synchronization: 'Synchronized',
        market_health: 'Healthy',
        global_summary: '',
        instance_count: 5,
        active_symbols: [],
        ...overrides,
    } as OverviewMatrix;
}

beforeEach(() => {
    const app = useAppStore();
    app.overviewMatrix = null;
});

afterEach(() => {
    cleanup();
});

describe('MarketAlignmentCard — empty state', () => {
    it('renders the awaiting-data placeholder when no overview is loaded', () => {
        render(MarketAlignmentCard);
        expect(screen.getByText(/Awaiting alignment data/i)).toBeTruthy();
    });

    it('renders the awaiting-data placeholder when overview has no alignment fields', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview();
        render(MarketAlignmentCard);
        expect(screen.getByText(/Awaiting alignment data/i)).toBeTruthy();
    });
});

describe('MarketAlignmentCard — populated states', () => {
    it('renders the bullish distribution + positive consensus + strong agreement', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview({
            alignment_distribution: {
                strong_bull: 3,
                bull: 1,
                neutral: 1,
            },
            alignment_consensus_index: 45,
            multi_tf_agreement_pct: 78,
        });
        render(MarketAlignmentCard);
        expect(screen.getByText('MARKET ALIGNMENT')).toBeTruthy();
        expect(screen.getByText(/Distribution \(5 pairs\)/)).toBeTruthy();
        expect(screen.getByText('+45')).toBeTruthy();
        expect(screen.getByText('Bullish')).toBeTruthy();
        expect(screen.getByText('78%')).toBeTruthy();
        expect(screen.getByText('Strong consensus')).toBeTruthy();
    });

    it('renders the bearish distribution + negative consensus + partial agreement', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview({
            alignment_distribution: {
                bear: 2,
                strong_bear: 1,
                neutral: 1,
            },
            alignment_consensus_index: -35,
            multi_tf_agreement_pct: 60,
        });
        render(MarketAlignmentCard);
        expect(screen.getByText('-35')).toBeTruthy();
        expect(screen.getByText('Bearish')).toBeTruthy();
        expect(screen.getByText('60%')).toBeTruthy();
        expect(screen.getByText('Partial consensus')).toBeTruthy();
    });

    it('renders the conflicted bucket for low agreement', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview({
            alignment_distribution: { bull: 2, bear: 2 },
            alignment_consensus_index: 5,
            multi_tf_agreement_pct: 30,
        });
        render(MarketAlignmentCard);
        expect(screen.getByText('30%')).toBeTruthy();
        expect(screen.getByText('Conflicted')).toBeTruthy();
        expect(screen.getByText('Neutral')).toBeTruthy();
    });

    it('strongly bearish consensus gets the Strongly Bearish label', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview({
            alignment_distribution: { strong_bear: 4 },
            alignment_consensus_index: -72,
            multi_tf_agreement_pct: 85,
        });
        render(MarketAlignmentCard);
        expect(screen.getByText('-72')).toBeTruthy();
        expect(screen.getByText('Strongly Bearish')).toBeTruthy();
    });

    it('strongly bullish consensus gets the Strongly Bullish label', () => {
        const app = useAppStore();
        app.overviewMatrix = makeOverview({
            alignment_distribution: { strong_bull: 4 },
            alignment_consensus_index: 70,
            multi_tf_agreement_pct: 90,
        });
        render(MarketAlignmentCard);
        expect(screen.getByText('+70')).toBeTruthy();
        expect(screen.getByText('Strongly Bullish')).toBeTruthy();
    });
});
