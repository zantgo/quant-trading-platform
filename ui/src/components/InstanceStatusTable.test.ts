// @vitest-environment jsdom
//
// InstanceStatusTable — Overview-tab per-instance status (v11.2):
//   • one COLLAPSED row per instance, ordered by the L7 asset_ranking
//     symbol order when present (fallback: map insertion order),
//   • the decision badge is the SAME derivation the chart/Recommendation
//     view uses — `computeDecisionRank` + the `buildL6DecisionHeader`
//     palette — rendered with its probability percentage,
//   • STAND ASIDE override when HOLD + readiness STAND_ASIDE,
//   • expanded rows show one sub-row per ACTIVE slot with the same
//     badge + pipeline pill the TfStatusTable renders,
//   • empty instances render the grey `—` badge and loading pills.
import { cleanup, render, fireEvent, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import InstanceStatusTable from './InstanceStatusTable.svelte';
import headerStyles from './LayerHeader.module.css';
import { useAppStore } from '../state.svelte';
import { computeDecisionRank } from '../lib/decisionRank';
import type {
    AdvisoryMatrix, DecisionContext, InstanceState, OpportunityMatrix, OverviewMatrix,
} from '../types';
import { makeTerms } from '../tests/makeTerms';
import type { WsState } from '../lib/websocket.svelte';

function makeAdvisory(overrides: Partial<AdvisoryMatrix> = {}): AdvisoryMatrix {
    return {
        symbol: 'BTC-USDT',
        directional_guidance: 'Short',
        market_stance: 'Cautious',
        opportunity_classification: 'Breakout',
        strategy_environment: 'TrendFollowing',
        entry_guidance: 'Immediate',
        exit_guidance: 'NoWarning',
        protection_strategy: 'ATRBased',
        target_strategy: 'ResistanceBased',
        confidence_assessment: 70,
        stop_loss_distance_pct: 0.015,
        cascade_risk_score: 30,
        environment_favorability: { score: 25, level: 'Low', state: 'Stable', confidence: 50, evidence: [] },
        final_recommendation: 'Short setup.',
        ...overrides,
    };
}

function makeDecisionContext(overrides: Partial<DecisionContext> = {}): DecisionContext {
    return {
        score: -55,
        bias: 'Bearish',
        score_confidence: 0.8,
        entry_danger: { score: 30, level: 'Low', state: 'Stable', confidence: 50, evidence: [] },
        expected_reward_risk_ratio: 2.0,
        trade_readiness: 'READY',
        contributing_indicators: [],
        long_probability: 20,
        short_probability: 45,
        hold_probability: 35,
        ...overrides,
    };
}

function makeInstance(overrides: Partial<InstanceState> = {}): InstanceState {
    return {
        symbol: 'BTC',
        exchange: 'Hyperliquid',
        isConnected: true,
        mode: 'observe',
        activeSlots: ['micro1', 'fast1'],
        terms: makeTerms(),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null,
        analysis: null,
        risk: null,
        advisory: makeAdvisory(),
        decisionContext: makeDecisionContext(),
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
    } as unknown as InstanceState;
}

beforeEach(() => {
    (globalThis as any).WebSocket = { OPEN: 1, CLOSED: 3 };
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.overviewMatrix = null;
});

afterEach(() => {
    cleanup();
});

function seed(key: string, inst: InstanceState) {
    const app = useAppStore();
    app.instancesMap[key] = inst;
}

function renderTable(wssMap: Record<string, WsState> = {}) {
    return render(InstanceStatusTable, { props: { wssMap } });
}

describe('InstanceStatusTable — rows & ordering', () => {
    it('renders one collapsed row per instance in asset_ranking order', () => {
        seed('BTC-USDT', makeInstance({ symbol: 'BTC' }));
        seed('ETH-USDT', makeInstance({ symbol: 'ETH' }));
        const app = useAppStore();
        app.overviewMatrix = {
            asset_ranking: [
                { symbol: 'ETH-USDT', score: 90, bias: 'BULLISH', confidence: 80, regime: 'TRENDING', risk_level: 'LOW' },
                { symbol: 'BTC-USDT', score: 40, bias: 'BEARISH', confidence: 60, regime: 'RANGE', risk_level: 'MODERATE' },
            ],
        } as OverviewMatrix;
        const { container } = renderTable();
        const table = screen.getByLabelText('Per-instance status');
        const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
        // Collapsed → exactly one row per instance (no sub-rows).
        expect(bodyRows.length).toBe(2);
        expect(bodyRows[0].textContent).toContain('ETH-USDT');
        expect(bodyRows[1].textContent).toContain('BTC-USDT');
        expect(container).toBeTruthy();
    });

    it('falls back to map insertion order without a ranking', () => {
        seed('BTC-USDT', makeInstance({ symbol: 'BTC' }));
        seed('ETH-USDT', makeInstance({ symbol: 'ETH' }));
        renderTable();
        const bodyRows = Array.from(screen.getByLabelText('Per-instance status').querySelectorAll('tbody tr'));
        expect(bodyRows[0].textContent).toContain('BTC-USDT');
        expect(bodyRows[1].textContent).toContain('ETH-USDT');
    });
});

describe('InstanceStatusTable — decision badge', () => {
    it('matches computeDecisionRank (label + percentage) and the direction palette', () => {
        const inst = makeInstance();
        seed('BTC-USDT', inst);
        renderTable();
        const rank = computeDecisionRank({
            advisory: inst.advisory,
            decisionContext: inst.decisionContext,
            opportunity: inst.opportunity,
            analysis: inst.analysis,
        });
        const badge = screen.getByLabelText(/Decision badge/);
        const expectedLabel = rank.top === 'SHORT' ? 'SHORT' : rank.top;
        expect(badge.textContent).toContain(expectedLabel);
        expect(badge.textContent).toContain(`${Math.round(rank.top_prob)}%`);
        // Direction-color discipline: SHORT wears the bearish red (jsdom
        // normalizes #f87171 → rgb(248, 113, 113)).
        const style = (badge.getAttribute('style') ?? '').replace(/\s+/g, '').toLowerCase();
        expect(style.includes('#f87171') || style.includes('rgb(248,113,113)')).toBe(true);
    });

    it('STAND ASIDE overrides a HOLD verdict when readiness is STAND_ASIDE', () => {
        seed('BTC-USDT', makeInstance({
            advisory: makeAdvisory({ directional_guidance: 'Neutral' }),
            decisionContext: makeDecisionContext({
                score: 0,
                bias: 'Neutral',
                long_probability: 33,
                short_probability: 33,
                hold_probability: 34,
                trade_readiness: 'STAND_ASIDE',
            }),
        }));
        renderTable();
        const badge = screen.getByLabelText(/Decision badge/);
        expect(badge.textContent).toContain('STAND ASIDE');
    });

    it('renders the L/H/S probability chips when probabilities exist', () => {
        seed('BTC-USDT', makeInstance());
        const { container } = renderTable();
        expect(container.textContent).toContain('L 20%');
        expect(container.textContent).toContain('H 35%');
        expect(container.textContent).toContain('S 45%');
    });
});

describe('InstanceStatusTable — expand / collapse', () => {
    it('expands to one sub-row per ACTIVE slot; collapse-all hides them again', async () => {
        seed('BTC-USDT', makeInstance({ activeSlots: ['micro1', 'fast1'] }));
        const { container } = renderTable();
        const table = screen.getByLabelText('Per-instance status');
        expect(table.querySelectorAll('tbody tr').length).toBe(1);

        const toggle = screen.getByLabelText(/Expand BTC/);
        await fireEvent.click(toggle);
        expect(table.querySelectorAll('tbody tr').length).toBe(1 + 2);
        const subText = table.textContent!;
        expect(subText).toContain('MICRO1');
        expect(subText).toContain('FAST1');
        expect(subText).toContain('1s');
        expect(subText).toContain('5s');

        // The single expanded row makes `allExpanded` true → the toolbar
        // button now offers collapse; click it and everything folds.
        const expandAll = screen.getByLabelText('Collapse all rows');
        await fireEvent.click(expandAll);
        expect(table.querySelectorAll('tbody tr').length).toBe(1);
        expect(screen.getByLabelText('Expand all rows')).toBeTruthy();
    });

    it('expand-all opens every instance row at once', async () => {
        seed('BTC-USDT', makeInstance({ symbol: 'BTC', activeSlots: ['micro1'] }));
        seed('ETH-USDT', makeInstance({ symbol: 'ETH', activeSlots: ['micro1', 'micro2', 'fast1'] }));
        renderTable();
        const table = screen.getByLabelText('Per-instance status');
        await fireEvent.click(screen.getByLabelText('Expand all rows'));
        expect(table.querySelectorAll('tbody tr').length).toBe(2 + (1 + 3));
    });
});

describe('InstanceStatusTable — empty instance', () => {
    it('renders the grey — decision badge and per-TF grey badges + loading pills', async () => {
        seed('BTC-USDT', makeInstance({
            advisory: null,
            decisionContext: null,
            opportunity: null,
        }));
        const { container } = renderTable();
        const badge = screen.getByLabelText(/Decision badge/);
        expect(badge.textContent).toContain('\u2014');
        expect(badge.className).toContain(headerStyles.badgeEmpty);

        const toggle = screen.getByLabelText(/Expand BTC/);
        await fireEvent.click(toggle);
        const badges = container.querySelectorAll(`tbody .${headerStyles.badge}`);
        expect(badges.length).toBe(3); // 1 instance badge + 2 per-TF badges
        const pills = container.querySelectorAll('tbody [aria-live="polite"]');
        for (const p of pills) expect(p.textContent).toContain('loading');
    });
});
