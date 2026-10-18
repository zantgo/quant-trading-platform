// @vitest-environment jsdom
//
// GeneralDashboard — Market Overview operator dashboard.
//
// These tests cover the three hero states (TRADE / WAIT / STAND ASIDE),
// the 4-up card row, the 15-column Asset Rankings table, and the
// Risk Distribution card's wire-side behaviour (reading from
// OverviewMatrix.risk_distribution when available, falling back to local
// aggregation when not).

import { cleanup, render, screen } from '@testing-library/svelte';
import { makeTerms } from '../tests/makeTerms';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import GeneralDashboard from './GeneralDashboard.svelte';
import { useAppStore } from '../state.svelte';
import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, InstanceState, OpportunityMatrix, OverviewMatrix, RiskDimension, RiskMatrix } from '../types';

function makeDanger(score: number, overrides: Partial<RiskDimension> = {}): RiskDimension {
    return {
        score,
        level: score >= 80 ? 'Extreme' : score >= 60 ? 'High' : score >= 40 ? 'Moderate' : score >= 20 ? 'Low' : 'VeryLow',
        state: 'Stable',
        confidence: 50,
        evidence: [],
        ...overrides,
    };
}

function makeAdvisory(overrides: Partial<AdvisoryMatrix> = {}): AdvisoryMatrix {
    return {
        symbol: 'BTC-USDT',
        directional_guidance: 'Long',
        market_stance: 'Constructive',
        opportunity_classification: 'Breakout',
        strategy_environment: 'TrendFollowing',
        entry_guidance: 'Immediate',
        exit_guidance: 'NoWarning',
        protection_strategy: 'ATRBased',
        target_strategy: 'ResistanceBased',
        confidence_assessment: 75,
        stop_loss_distance_pct: 0.015,
        cascade_risk_score: 30,
        environment_favorability: makeDanger(25),
        final_recommendation: 'Long setup actionable.',
        ...overrides,
    };
}

function makeDecisionContext(overrides: Partial<DecisionContext> = {}): DecisionContext {
    return {
        score: 50,
        bias: 'Bullish',
        score_confidence: 0.8,
        entry_danger: makeDanger(31),
        expected_reward_risk_ratio: 2.5,
        trade_readiness: 'READY',
        contributing_indicators: [],
        ...overrides,
    };
}

function makeOpportunity(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'TrendContinuation',
        opportunity_score: 80,
        setup_quality: 'STRONG',
        profiles: [
            {
                opportunity_type: 'TrendContinuation',
                score: 80,
                preconditions_met: 3,
                preconditions_total: 4,
                notes: '',
                direction_family: 'TREND_RIDING',
                long_entry_zone: { low: 60000, high: 62000 },
                long_target_zone: { low: 65000, high: 68000 },
                long_invalidation_level: 59000,
                short_entry_zone: null,
                short_target_zone: null,
                short_invalidation_level: null,
                long_expected_rr_internal: 2.5,
                short_expected_rr_internal: null,
                trade_viability: 'ACTIONABLE',
            },
        ],
        forecast_confidence: 0,
        contributing_signals: [],
        invalidation_note: '',
        entry_zone: { low: 60000, high: 62000 },
        target_zone: { low: 65000, high: 68000 },
        invalidation_level: 59000,
        long_entry_zone: { low: 60000, high: 62000 },
        long_target_zone: { low: 65000, high: 68000 },
        long_invalidation_level: 59000,
        short_entry_zone: { low: 0, high: 0 },
        short_target_zone: { low: 0, high: 0 },
        short_invalidation_level: 0,
        long_expected_rr_internal: 2.5,
        short_expected_rr_internal: 2.5,
        time_horizon: 'SWING',
        confluent_entry_levels: [],
        confluent_target_levels: [],
        confluent_invalidation_levels: [],
        ...overrides,
    } as OpportunityMatrix;
}

function makeAnalysis(overrides: Partial<AnalysisMatrix> = {}): AnalysisMatrix {
    return {
        symbol: 'BTC-USDT',
        bias: 'Bullish',
        confidence: 0.7,
        state_confidence: 0.7,
        market_regime: 'Accumulation',
        trend_assessment: 'Developing',
        momentum_assessment: 'Increasing',
        structure_assessment: 'Healthy',
        volatility_assessment: 'Normal',
        volume_assessment: 'Normal',
        market_quality: 'Good',
        market_quality_score: 70,
        market_phase: 'Markup',
        market_interpretation: '',
        rationale: '',
        supporting_signals: [],
        contradicting_signals: [],
        timeframes_considered: 4,
        ...overrides,
    } as AnalysisMatrix;
}

function makeRiskMatrix(score = 40): RiskMatrix {
    return {
        symbol: 'BTC-USDT',
        market_risk: makeDanger(score),
        volatility_risk: makeDanger(score),
        execution_liquidity_risk: makeDanger(score, { confidence: 80 }),
        structure_risk: makeDanger(score),
        momentum_risk: makeDanger(score),
        signal_risk: makeDanger(score),
        execution_risk: makeDanger(score),
        cascade_risk: makeDanger(score),
        overall_risk: makeDanger(score),
    };
}

function makeInstance(symbol: string, overrides: Partial<InstanceState> = {}): InstanceState {
    return {
        symbol,
        exchange: 'Hyperliquid',
        isConnected: true,
        terms: makeTerms({ 1: { priceText: '63505' } as any }),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null,
        analysis: makeAnalysis(),
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

function seedPair(symbol: string, overrides: Partial<InstanceState> = {}) {
    const app = useAppStore();
    const key = `${symbol}-USDT`;
    if (!app.instancesMap[key]) app.initInstance(symbol);
    const entry = app.instancesMap[key];
    entry.instanceId = entry.instanceId || `inst_test_${symbol}`;
    Object.assign(entry, makeInstance(symbol, overrides));
    return entry;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.overviewMatrix = null;
});

afterEach(() => {
    cleanup();
});

describe('GeneralDashboard — empty state', () => {
    it('renders the placeholder when no instances are configured', () => {
        const { container } = render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('Market Overview')).toBeTruthy();
        expect(screen.getByText(/Add workspaces/i)).toBeTruthy();
        // The hero is only shown when there are instances.
        expect(container.querySelector('[class*="hero"]')).toBeNull();
    });
});

describe('GeneralDashboard — instance status table (v11.2)', () => {
    it('renders the definitive order: header → rankings → instance table', () => {
        seedPair('BTC');
        seedPair('ETH');
        const { container } = render(GeneralDashboard, { props: { wssMap: {} } });
        const table = container.querySelector('[aria-label="Per-instance status"]');
        const header = container.querySelector('[class*="unifiedHeader"]');
        const rankings = container.querySelector('[class*="assetRankings"], [class*="ranking"]')
            ?? container.querySelector('table');
        expect(header).toBeTruthy();
        expect(rankings).toBeTruthy();
        expect(table).toBeTruthy();
        // CR-15: the market-status banner used to be a separate `RecommendationHero`
        // between the header and the rankings. Its verdict (TRADE / WAIT / STAND
        // ASIDE) was strategy; the status reading now lives in the unified
        // header's own badge row, so the funnel is header → rankings → instance
        // status → KPIs → cards → market health.
        expect(header!.compareDocumentPosition(rankings!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
        expect(rankings!.compareDocumentPosition(table!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
        // One collapsed row per instance.
        const bodyRows = Array.from(table!.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(2);
    });
});

describe('GeneralDashboard — asset rankings table', () => {
    // CR-15: the table was 15 columns — Symbol, Price, Entry, Take Profit,
    // Stop Loss, Bias, Signal, Direction, R:R, Score, Confidence, MTF Score,
    // MTF Stance, Risk, Updated — and eight of them were strategy. The seven that
    // remain are what the model measures.
    it('renders the 7 model columns (Symbol, Price, Bias, Score, MTF Score, MTF Stance, Updated)', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        for (const col of ['Symbol', 'Price', 'Bias', 'Score', 'MTF Score', 'MTF Stance', 'Updated']) {
            expect(screen.getAllByText(col).length, `missing column ${col}`).toBeGreaterThan(0);
        }
    });

    it('renders no strategy column', () => {
        // The regression this pins: a strategy column creeping back onto the
        // table. Each of these rendered a claim about what to do.
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        for (const col of ['Entry', 'Take Profit', 'Stop Loss', 'Signal', 'Direction', 'R:R', 'Confidence', 'Risk']) {
            expect(screen.queryAllByText(col).length, `strategy column ${col} is back`).toBe(0);
        }
    });

    it('renders the server-computed model columns from overview_rows', () => {
        seedPair('BTC');
        const app = useAppStore();
        app.overviewMatrix = {
            overview_rows: [
                {
                    symbol: 'BTC-USDT',
                    price: 63505,
                    bias: 'Bullish',
                    score: 61,
                    mtf_score: 42,
                    mtf_stance: 'bull',
                    updated_ts: 1_700_000_000,
                    active: true,
                },
            ],
        } as OverviewMatrix;
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText('WEAK BULL').length).toBeGreaterThan(0);
        expect(screen.getAllByText('61').length).toBeGreaterThan(0);
        expect(screen.getAllByText('+42').length).toBeGreaterThan(0);
    });

    it('renders one row per pair', () => {
        seedPair('BTC');
        seedPair('ETH');
        render(GeneralDashboard, { props: { wssMap: {} } });
        // `createInstanceState` stores the bare symbol (e.g. 'BTC') in
        // `inst.symbol`, so the table renders 'BTC' and 'ETH' (the
        // `-USDT` suffix is the pairKey, not the display symbol).
        expect(screen.getAllByText('BTC').length).toBeGreaterThan(0);
        expect(screen.getAllByText('ETH').length).toBeGreaterThan(0);
    });

});

describe('GeneralDashboard — UTC clock badge', () => {
    it('renders the UTC clock badge next to the title', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('UTC')).toBeTruthy();
    });
});

describe('GeneralDashboard — header KPI strip', () => {
    // CR-15: the strip was six tiles and five were strategy (valid trades, best
    // opportunity, R:R, avg risk, and a coverage ratio measured as "pairs with
    // opportunity data"). What remains are three model readings.
    it('renders the 3 model tiles', () => {
        seedPair('BTC');
        seedPair('ETH');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('MARKET BIAS')).toBeTruthy();
        expect(screen.getByText('CROSS-TF CONSENSUS')).toBeTruthy();
        expect(screen.getByText('ACTIVE PAIRS')).toBeTruthy();
    });

    it('renders no strategy tile', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        for (const gone of ['VALID TRADES', 'BEST OPPORTUNITY', 'RISK TO REWARD RATIO', 'AVG RISK', 'COVERAGE']) {
            expect(screen.queryByText(gone), `strategy tile ${gone} is back`).toBeNull();
        }
    });
});

describe('GeneralDashboard — market health bars', () => {
    // CR-15: the four sub-dimension bars were computed from the L5 `RiskMatrix`
    // dimensions. That matrix no longer crosses the wire, so there is nothing to
    // bar.
    //
    // The card keeps its four slots and renders them UNAVAILABLE — the label
    // plus a dash, never a zero. That distinction is the point: a bar reading 0
    // looks like "measured, and this is the value", while a dash says "not
    // measured". P10 returns the bars derived from the four typed axes.
    it('shows the card with its four bars marked unavailable, not zeroed', () => {
        seedPair('BTC');
        const { container } = render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('MARKET HEALTH')).toBeTruthy();
        for (const label of ['TREND STRENGTH', 'LIQUIDITY', 'VOLATILITY', 'SIGNAL STABILITY']) {
            expect(screen.getByText(label), `sub-bar ${label} should still be present`).toBeTruthy();
        }
        // No bar may render a numeric value — every one is an em dash.
        const bars = container.querySelectorAll('[class*="barValue"], [class*="bar"]');
        const values = Array.from(bars)
            .map((b) => (b.textContent ?? '').trim())
            .filter((s) => s.length > 0);
        expect(
            values.some((v) => /^\d/.test(v)),
            `no sub-bar may render a number: ${values.join(' | ')}`,
        ).toBe(false);
    });
});

describe('GeneralDashboard — scan status', () => {
    it('renders the scan-status strip (last scan + auto-refresh)', () => {
        seedPair('BTC');
        seedPair('ETH');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText('Last scan').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Auto-refresh').length).toBeGreaterThan(0);
    });
});

describe('GeneralDashboard — market alignment card', () => {
    it('renders the MARKET ALIGNMENT card title and awaiting-data placeholder', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('MARKET ALIGNMENT')).toBeTruthy();
        expect(screen.getByText(/Awaiting alignment data/i)).toBeTruthy();
    });

    it('renders populated distribution + consensus + agreement when overview has alignment data', () => {
        const app = useAppStore();
        app.overviewMatrix = {
            global_market_bias: 'BULLISH',
            market_breadth: 'POSITIVE',
            axis_distribution: {},
            opportunity_distribution: {},
            risk_distribution: { low_pct: 50, moderate_pct: 40, high_pct: 10, risk_environment: 'LOW_RISK' },
            asset_ranking: [],
            market_synchronization: 'SYNCHRONIZED',
            market_health: 'HEALTHY',
            global_summary: '',
            instance_count: 4,
            active_symbols: [],
            alignment_distribution: { strong_bull: 2, bull: 1, neutral: 1 },
            alignment_consensus_index: 50,
            multi_tf_agreement_pct: 80,
        } as OverviewMatrix;
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText(/Distribution \(4 pairs\)/)).toBeTruthy();
        expect(screen.getByText('+50')).toBeTruthy();
        expect(screen.getByText('80%')).toBeTruthy();
        expect(screen.getByText('Strong consensus')).toBeTruthy();
    });
});

// ── v11.12.5: header row order — badges FIRST, pills under them ────────
describe('GeneralDashboard — header row order (v11.12.5)', () => {
    it('renders title row → badge + ghost trail → scan/meta pills', () => {
        seedPair('BTC');
        const { container } = render(GeneralDashboard, { props: { wssMap: {} } });
        const header = container.querySelector('[class*="unifiedHeader"]')!;
        const top = header.querySelector('[class*="headerTop"]')!;
        const badge = header.querySelector('[class*="badgeRow"]')!;
        const scan = header.querySelector('[class*="scanRow"]')!;
        expect(top).toBeTruthy();
        expect(badge).toBeTruthy();
        expect(scan).toBeTruthy();
        expect(top.compareDocumentPosition(badge) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
        expect(badge.compareDocumentPosition(scan) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });
});
