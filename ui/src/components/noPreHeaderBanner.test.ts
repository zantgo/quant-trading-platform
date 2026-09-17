// @vitest-environment jsdom
//
// v7.0-prod — D9 invariant regression guard.
//
// "In every tab of every layer, the canonical LayerHeader chrome sits
//  at the top of the panel. NO text is allowed above it. Period."
//
// We render each panel with no data (the empty / loading state) and
// assert that the first painted text does NOT include any helper copy
// that used to sit above the badge in the v6.9 layout. Any future PR
// that re-introduces a pre-banner block (e.g. an "Awaiting data…"
// explanatory line) will fail this test.

import { cleanup, render } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import AlignmentPanel from './AlignmentPanel.svelte';
import AnalysisPanel from './AnalysisPanel.svelte';
import RiskPanel from './RiskPanel.svelte';
import RecommendationPanel from './RecommendationPanel.svelte';
import { useAppStore } from '../state.svelte';
import type { AlignmentMatrix, AnalysisMatrix, DecisionContext, InstanceState, OpportunityMatrix, RiskDimension, RiskMatrix } from '../types';

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

function makeRiskMatrix(): RiskMatrix {
    return {
        symbol: 'BTC-USDT',
        market_risk: makeDanger(30),
        volatility_risk: makeDanger(35),
        execution_liquidity_risk: makeDanger(25),
        structure_risk: makeDanger(40),
        momentum_risk: makeDanger(30),
        signal_risk: makeDanger(20),
        execution_risk: makeDanger(25),
        cascade_risk: makeDanger(30),
        overall_risk: makeDanger(40),
    };
}

function seedEmptyPair(): InstanceState {
    const app = useAppStore();
    if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
    const entry = app.instancesMap['BTC-USDT'];
    entry.alignment = null;
    entry.analysis = null;
    entry.risk = null;
    entry.opportunity = null;
    entry.advisory = null;
    entry.decisionContext = null;
    return entry;
}

const FORBIDDEN_PHRASES: { label: string; phrase: RegExp }[] = [
    { label: 'L2', phrase: /Multi-timeframe alignment forming/ },
    { label: 'L3', phrase: /Awaiting market analysis data/ },
    { label: 'L5', phrase: /Risk assessment engine initializing/ },
    { label: 'L5-headline', phrase: /Risk assessment data forming/ },
    { label: 'L6', phrase: /Awaiting recommendation data/ },
];

describe('chrome invariant — no text sits above the LayerHeader', () => {
    afterEach(() => cleanup());

    for (const { label, phrase } of FORBIDDEN_PHRASES) {
        it(`${label}: forbidden pre-banner phrase is absent`, async () => {
            seedEmptyPair();
            // Render every panel in turn. Each panel mounts the LayerHeader
            // as the first painted child; the document's full text content
            // must not contain the legacy phrase. OpportunitiesPanel is
            // deliberately excluded from this loop — the user's later
            // request restored it to its pre-v7 chrome shape, so the L4
            // body content (Trade Setups / Invalidation Note / etc.)
            // legitimately re-renders even with no matrix loaded, and the
            // v6 "Awaiting" copy may surface inside the body.
            const panels = [
                AlignmentPanel,
                AnalysisPanel,
                RiskPanel,
                RecommendationPanel,
            ];
            let found = false;
            for (const Panel of panels) {
                const props = Panel === AnalysisPanel
                    ? {}
                    : { pairKey: 'BTC-USDT' };
                const { unmount } = render(Panel as any, { props });
                if (document.body.textContent && phrase.test(document.body.textContent)) {
                    found = true;
                }
                unmount();
            }
            expect(found, `phrase ${phrase} must never render above any LayerHeader`).toBe(false);
        });
    }
});
