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
        risk: makeRiskMatrix(40),
        advisory: makeAdvisory(),
        decisionContext: makeDecisionContext(),
        opportunity: makeOpportunity(),
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
    it('renders the v11.11 definitive order: header → hero → rankings → instance table', () => {
        seedPair('BTC');
        seedPair('ETH');
        const { container } = render(GeneralDashboard, { props: { wssMap: {} } });
        const table = container.querySelector('[aria-label="Per-instance status"]');
        expect(table).toBeTruthy();
        const header = container.querySelector('[class*="unifiedHeader"]');
        const hero = container.querySelector('[class*="hero"]');
        expect(header).toBeTruthy();
        expect(hero).toBeTruthy();
        // v11.11 DEFINITIVE ORDER: header → MARKET STATUS (hero) →
        // ASSET RANKINGS → INSTANCE STATUS (…→ Market Health last).
        expect(header!.compareDocumentPosition(hero!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
        expect(hero!.compareDocumentPosition(table!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
        // One collapsed row per instance.
        const bodyRows = Array.from(table!.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(2);
    });
});

describe('GeneralDashboard — hero states', () => {
    it('renders TRADE when at least one Actionable + READY setup exists', () => {
        seedPair('BTC', {
            decisionContext: makeDecisionContext({ trade_readiness: 'READY' }),
            opportunity: makeOpportunity({
                profiles: [{
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
                }],
            }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('TRADE')).toBeTruthy();
        expect(screen.getByText(/actionable setup/i)).toBeTruthy();
    });

    it('renders WAIT when only DirectionalNeutral setups exist', () => {
        seedPair('BTC', {
            decisionContext: makeDecisionContext({ trade_readiness: 'FORMING' }),
            opportunity: makeOpportunity({
                profiles: [{
                    opportunity_type: 'MeanReversion',
                    score: 50,
                    preconditions_met: 2,
                    preconditions_total: 4,
                    notes: '',
                    direction_family: 'NEUTRAL',
                    long_entry_zone: null,
                    long_target_zone: null,
                    long_invalidation_level: null,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    long_expected_rr_internal: null,
                    short_expected_rr_internal: null,
                    trade_viability: 'DIRECTIONAL_NEUTRAL',
                }],
            }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        // The hero WAIT and the asset-table WAIT cells both render.
        expect(screen.getAllByText('WAIT').length).toBeGreaterThan(0);
    });

    it('renders STAND ASIDE when no qualifying profile exists', () => {
        seedPair('BTC', {
            opportunity: makeOpportunity({ profiles: [] }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('STAND ASIDE')).toBeTruthy();
    });
});

describe('GeneralDashboard — asset rankings table', () => {
    it('renders 15 columns (Symbol, Price, Entry, Take Profit, Stop Loss, Bias, Signal, Direction, R:R, Score, Confidence, MTF Score, MTF Label, Risk, Updated) — Image 1 fidelity', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        // Verify each column header is present (use getAllByText for
        // labels that may appear in multiple cards, e.g. "Direction"
        // appears in the Trade Opportunities card AND the table).
        expect(screen.getAllByText('Symbol').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Price').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Entry').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Take Profit').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Stop Loss').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Bias').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Signal').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Direction').length).toBeGreaterThan(0);
        // R:R header renders as "R:R" (colon without slash)
        expect(screen.getAllByText('R:R').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Score').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Confidence').length).toBeGreaterThan(0);
        expect(screen.getAllByText('MTF Score').length).toBeGreaterThan(0);
        expect(screen.getAllByText('MTF Label').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Risk').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Updated').length).toBeGreaterThan(0);
    });

    it('renders the top-setup entry / target / stop levels', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        // The seeded profile carries long zones 60,000–62,000 /
        // 65,000–68,000 / 59,000 → the ENTRY / TARGET / STOP cells render
        // them (local warmup fallback through `topSetupSummary`).
        expect(screen.getAllByText('60,000–62,000').length).toBeGreaterThan(0);
        expect(screen.getAllByText('65,000–68,000').length).toBeGreaterThan(0);
        expect(screen.getAllByText('59,000').length).toBeGreaterThan(0);
    });

    it('renders the server-computed setup levels from overview_rows', () => {
        seedPair('BTC');
        const app = useAppStore();
        app.overviewMatrix = {
            overview_rows: [
                {
                    symbol: 'BTC-USDT',
                    price: 63505,
                    bias: 'Bullish',
                    signal: 'BUY',
                    direction: 'LONG',
                    rr: 2.5,
                    score: 61,
                    confidence: 75,
                    mtf_score: 42,
                    mtf_label: 'WEAK_BULL_MTF',
                    risk: 45,
                    entry_low: 63200,
                    entry_high: 63400,
                    target_low: 66000,
                    target_high: 66500,
                    invalidation: 62800,
                    updated_ts: 1_700_000_000,
                    active: true,
                },
            ],
        } as OverviewMatrix;
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText('63,200–63,400').length).toBeGreaterThan(0);
        expect(screen.getAllByText('66,000–66,500').length).toBeGreaterThan(0);
        expect(screen.getAllByText('62,800').length).toBeGreaterThan(0);
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

    it('renders BUY signal for LONG directional_guidance', () => {
        seedPair('BTC', {
            advisory: makeAdvisory({ directional_guidance: 'Long' }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        const cells = screen.getAllByText('BUY');
        expect(cells.length).toBeGreaterThan(0);
    });

    it('renders SELL signal for SHORT directional_guidance', () => {
        seedPair('BTC', {
            advisory: makeAdvisory({ directional_guidance: 'StrongShort' }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        const cells = screen.getAllByText('SELL');
        expect(cells.length).toBeGreaterThan(0);
    });

    it('renders WAIT signal for Neutral directional_guidance', () => {
        seedPair('BTC', {
            advisory: makeAdvisory({ directional_guidance: 'Neutral' }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        const cells = screen.getAllByText('WAIT');
        expect(cells.length).toBeGreaterThan(0);
    });
});

describe('GeneralDashboard — risk distribution', () => {
    it('renders the Risk Distribution card', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('RISK DISTRIBUTION')).toBeTruthy();
        expect(screen.getByText('Low')).toBeTruthy();
        expect(screen.getByText('Moderate')).toBeTruthy();
        expect(screen.getByText('High')).toBeTruthy();
    });

    it('uses OverviewMatrix.risk_distribution when present (L7 source)', () => {
        const app = useAppStore();
        app.overviewMatrix = {
            global_market_bias: 'BULLISH',
            market_breadth: 'POSITIVE',
            low_coverage: false,
            breadth_pct: 50,
            regime_distribution: {},
            opportunity_distribution: {},
            risk_distribution: {
                low_pct: 60,
                moderate_pct: 30,
                high_pct: 10,
                risk_environment: 'LOW_RISK',
            },
            asset_ranking: [],
            market_synchronization: 'SYNCHRONIZED',
            market_health: 'HEALTHY',
            global_summary: '',
            instance_count: 1,
            active_symbols: ['BTC-USDT'],
        } as OverviewMatrix;
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        // The card should show "LOW RISK" environment from L7.
        expect(screen.getByText(/LOW RISK/i)).toBeTruthy();
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
    it('renders the 6 KPI tiles', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('VALID TRADES')).toBeTruthy();
        expect(screen.getByText('BEST OPPORTUNITY')).toBeTruthy();
        expect(screen.getByText('RISK TO REWARD RATIO')).toBeTruthy();
        expect(screen.getByText('MARKET BIAS')).toBeTruthy();
        expect(screen.getByText('AVG RISK')).toBeTruthy();
        expect(screen.getByText('COVERAGE')).toBeTruthy();
    });
});

describe('GeneralDashboard — market health bars', () => {
    it('renders the 4 sub-dimension bars', () => {
        seedPair('BTC');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getByText('MARKET HEALTH')).toBeTruthy();
        expect(screen.getByText('TREND STRENGTH')).toBeTruthy();
        expect(screen.getByText('LIQUIDITY')).toBeTruthy();
        expect(screen.getByText('VOLATILITY')).toBeTruthy();
        expect(screen.getByText('SIGNAL STABILITY')).toBeTruthy();
    });
});

describe('GeneralDashboard — scan status', () => {
    it('renders the scan-status strip with pair count', () => {
        seedPair('BTC');
        seedPair('ETH');
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText(/2.*pairs/).length).toBeGreaterThan(0);
        expect(screen.getAllByText('last scan').length).toBeGreaterThan(0);
        expect(screen.getAllByText('auto-refresh').length).toBeGreaterThan(0);
    });
});

describe('GeneralDashboard — direction distribution', () => {
    it('renders Long, Short, Neutral counts', () => {
        seedPair('BTC', { advisory: makeAdvisory({ directional_guidance: 'Long' }) });
        seedPair('ETH', { advisory: makeAdvisory({ directional_guidance: 'Short' }) });
        render(GeneralDashboard, { props: { wssMap: {} } });
        // Use getAllByText because LONG/SHORT/NEUTRAL also appear in
        // the Asset Rankings table's Direction column.
        expect(screen.getAllByText('LONG').length).toBeGreaterThan(0);
        expect(screen.getAllByText('SHORT').length).toBeGreaterThan(0);
        expect(screen.getAllByText('NEUTRAL').length).toBeGreaterThan(0);
    });
});

describe('GeneralDashboard — signal quality', () => {
    it('renders Strong/Moderate/Weak buckets', () => {
        seedPair('BTC', { advisory: makeAdvisory({ confidence_assessment: 80 }) });
        seedPair('ETH', { advisory: makeAdvisory({ confidence_assessment: 50 }) });
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText('STRONG').length).toBeGreaterThan(0);
        expect(screen.getAllByText('MODERATE').length).toBeGreaterThan(0);
    });
});

describe('GeneralDashboard — trade opportunities card', () => {
    it('renders the count of valid setups', () => {
        seedPair('BTC', {
            decisionContext: makeDecisionContext({ trade_readiness: 'READY' }),
        });
        render(GeneralDashboard, { props: { wssMap: {} } });
        expect(screen.getAllByText('TRADE OPPORTUNITIES').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Best Pair').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Direction').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Best Risk/Reward').length).toBeGreaterThan(0);
        expect(screen.getAllByText('Confidence').length).toBeGreaterThan(0);
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
            regime_distribution: {},
            opportunity_distribution: {},
            risk_distribution: { low_pct: 50, moderate_pct: 40, high_pct: 10, risk_environment: 'LOW_RISK' },
            asset_ranking: [],
            market_synchronization: 'SYNCHRONIZED',
            market_health: 'HEALTHY',
            global_summary: '',
            instance_count: 4,
            active_symbols: [],
            alignment_distribution: { STRONG_BULL_MTF: 2, WEAK_BULL_MTF: 1, NEUTRAL_MTF: 1 },
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
