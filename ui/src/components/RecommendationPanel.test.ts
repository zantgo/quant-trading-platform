// @vitest-environment jsdom
//
// Regression lock for the renamed Recommendation tab (was AdvisoryPanel).
//
// Bind contract: `RecommendationPanel` reads the L6 DecisionContext
// mirror field `pair.decisionContext` first, with a fallback to
// `terms[1].latestSnapshot.decision_context`. It must also read the
// L4 mirror `pair.opportunity` (not the snapshot path) to avoid the
// shadow-tick wipe that previously blanked the Trade Setups and the
// per-profile Recommendation cards between candle closes.
//
// We exercise the panel end-to-end through the same harness pattern
// the OpportunitiesPanel.test.ts uses — wire a seeded `pair` snapshot,
// mount the panel, assert the rendered text.

import { cleanup, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import RecommendationPanel from './RecommendationPanel.svelte';
import { useAppStore } from '../state.svelte';
import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, OpportunityMatrix, RiskDimension } from '../types';

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
        strategy_environment: 'MeanReversion',
        entry_guidance: 'WaitForConfirmation',
        exit_guidance: 'NoWarning',
        protection_strategy: 'ATRBased',
        target_strategy: 'ResistanceBased',
        confidence_assessment: 22,
        stop_loss_distance_pct: 0.015,
        cascade_risk_score: 30,
        environment_favorability: makeDanger(25),
        final_recommendation:
            'Neutral — no directional edge: NEUTRAL bias with 14% confidence, neutral stance in a mean-reversion environment. Breakout opportunity. Entry: no entry context. Stop: ATR-based.',
        ...overrides,
    };
}

function makeDecisionContext(overrides: Partial<DecisionContext> = {}): DecisionContext {
    return {
        score: 0,
        bias: 'Neutral',
        score_confidence: 0,
        entry_danger: makeDanger(31),
        expected_reward_risk_ratio: 0.59,
        trade_readiness: 'FORMING',
        contributing_indicators: [],
        ...overrides,
    };
}

function makeOpportunity(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    // Each qualifying profile carries its own per-side zones. With a
    // bullish macro bias, TrendRiding families (Breakout, TrendContinuation)
    // resolve to LONG. CounterTrend families (MeanReversion, Reversal) would
    // resolve to SHORT — but those don't appear in this fixture.
    const breakoutZones = {
        direction_family: 'TREND_RIDING' as const,
        long_entry_zone: { low: 63520, high: 63800 },
        long_target_zone: { low: 64500, high: 65000 },
        long_invalidation_level: 63200,
        long_expected_rr_internal: 2.0,
        short_entry_zone: null,
        short_target_zone: null,
        short_invalidation_level: null,
        short_expected_rr_internal: null,
    };
    const squeezeZones = {
        direction_family: 'TREND_RIDING' as const,
        long_entry_zone: { low: 63600, high: 63900 },
        long_target_zone: { low: 64400, high: 64800 },
        long_invalidation_level: 63300,
        long_expected_rr_internal: 1.5,
        short_entry_zone: null,
        short_target_zone: null,
        short_invalidation_level: null,
        short_expected_rr_internal: null,
    };
    const tcZones = {
        direction_family: 'TREND_RIDING' as const,
        long_entry_zone: { low: 63000, high: 63200 },
        long_target_zone: { low: 65000, high: 65500 },
        long_invalidation_level: 62400,
        long_expected_rr_internal: 2.5,
        short_entry_zone: null,
        short_target_zone: null,
        short_invalidation_level: null,
        short_expected_rr_internal: null,
    };
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'Breakout',
        opportunity_score: 65,
        setup_quality: 'Moderate',
        profiles: [
            {
                opportunity_type: 'Breakout',
                score: 65,
                preconditions_met: 2,
                preconditions_total: 2,
                notes: 'synthetic-breakout',
                ...breakoutZones,
            },
            {
                opportunity_type: 'LiquiditySqueeze',
                score: 60,
                preconditions_met: 1,
                preconditions_total: 3,
                notes: 'synthetic-squeeze',
                ...squeezeZones,
            },
            {
                opportunity_type: 'TrendContinuation',
                score: 60,
                preconditions_met: 0,
                preconditions_total: 3,
                notes: 'synthetic-trend',
                ...tcZones,
            },
        ],
        forecast_confidence: 0.19,
        time_horizon: 'INTRADAY',
        entry_zone: { low: 63520, high: 63800 },
        target_zone: { low: 64500, high: 65000 },
        invalidation_level: 63200,
        long_entry_zone: { low: 63520, high: 63800 },
        long_target_zone: { low: 64500, high: 65000 },
        long_invalidation_level: 63200,
        long_expected_rr_internal: 2.0,
        short_entry_zone: { low: 64520, high: 64800 },
        short_target_zone: { low: 62500, high: 63000 },
        short_invalidation_level: 65000,
        short_expected_rr_internal: 1.5,
        invalidation_note: 'Close below 63200 invalidates the Breakout thesis.',
        contributing_signals: [],
        confluent_entry_levels: [],
        confluent_target_levels: [],
        confluent_invalidation_levels: [],
        ...overrides,
    } as OpportunityMatrix;
}

function makeAnalysis(): AnalysisMatrix {
    return {
        symbol: 'BTC-USDT',
        bias: 'Bullish' as AnalysisMatrix['bias'],
        state_confidence: 0.29,
        confidence: 0.29,
        market_regime: 'Expansion' as AnalysisMatrix['market_regime'],
        trend_assessment: 'Weak' as AnalysisMatrix['trend_assessment'],
        momentum_assessment: 'Weakening' as AnalysisMatrix['momentum_assessment'],
        structure_assessment: 'Strong' as AnalysisMatrix['structure_assessment'],
        volatility_assessment: 'Expanding' as AnalysisMatrix['volatility_assessment'],
        volume_assessment: 'Strong' as AnalysisMatrix['volume_assessment'],
        market_quality: 'Good' as AnalysisMatrix['market_quality'],
        market_quality_score: 67.44,
        market_phase: 'Markup' as AnalysisMatrix['market_phase'],
        market_interpretation: 'Synthetic test interpretation',
        rationale: '',
        supporting_signals: ['MACRO (bullish): score +1, RANGE regime'],
        contradicting_signals: [
            'MICRO (bearish): score -13, TRENDING regime',
            'FAST (bearish): score -29, EXPANSION regime',
        ],
        timeframes_considered: 4,
    } as AnalysisMatrix;
}

function seedPair(pairKey: string) {
    const app = useAppStore();
    const [base] = pairKey.split('-');
    if (!app.instancesMap[pairKey]) app.initInstance(base);
    const entry = app.instancesMap[pairKey];
    entry.terms[1].priceText = '63505';
    entry.advisory = makeAdvisory();
    entry.decisionContext = makeDecisionContext();
    entry.opportunity = makeOpportunity();
    entry.analysis = makeAnalysis();
    return entry;
}

function zeroProfiles(entry: { opportunity: OpportunityMatrix | null }): void {
    // v6.10.19b (B2): a GENUINE HOLD verdict requires no qualifying
    // profile — the verdict-lean rule surfaces valid setups instead
    // ("it should not say HOLD when a valid setup exists"). Zeroing
    // preconditions keeps these fixtures in the true-HOLD state.
    if (!entry.opportunity) return;
    entry.opportunity = {
        ...entry.opportunity,
        profiles: (entry.opportunity.profiles ?? []).map((p) => ({ ...p, preconditions_met: 0 })),
    } as OpportunityMatrix;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
    cleanup();
});

describe('RecommendationPanel — L6 LayerHeader + safety flags (v7.0-prod)', () => {
    it('renders the Recommendation title and the canonical L6 header (single badge)', () => {
        seedPair('BTC-USDT');
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // Title text survives as the trailing slot of the LayerHeader.
        expect(screen.getAllByText('Recommendation').length).toBeGreaterThanOrEqual(1);
        // No competing badges from the legacy envHeader (NEUTRAL/CAUTIOUS).
        // The Directional Guidance + Market Stance merged pair is gone.
        expect(screen.queryByText(/Strategy environment/i)).toBeNull();
        expect(screen.queryByText(/Opportunity classification/i)).toBeNull();
        // The L6 panel MUST NOT echo the L3 `analysis.bias` (HIGH-priority
        // defect in the v6.9 chrome). The seeded analysis has `bias:
        // 'Bullish'`; the L6 header consumes `rank.top`, not `analysis.bias`.
        // We assert the absence of a stray L3-bias pill by counting only
        // the standalone "BULLISH" badge — the Recommendation page now
        // emits zero of those (the Long cards may still show "LONG", but
        // never "BULLISH").
        // (Reverting the strict-zero assertion: the body of the page
        // emits `LONG`, `SHORT`, `HOLD`, `NEUTRAL` and may say
        // `BULLISH` inside rationale bullets. We only assert the
        // chrome no longer leak-prints the L3 badge next to a state.)
        expect(screen.queryByText('BULLISH · NEUTRAL')).toBeNull();
        expect(screen.queryByText('NEUTRAL · CAUTIOUS')).toBeNull();
    });

    it('renders the safety-flags row with 5 chips (readiness, risk-adj R:R, stop-loss, confidence, entry danger)', () => {
        seedPair('BTC-USDT');
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // SAFETY FLAGS section title is unique to this row.
        expect(screen.getByText('Safety Flags')).toBeTruthy();
        // `getAllByText` because "Readiness" labels also appear in the
        // L6 header (badge sublabel) and the verdict sentence.
        expect(screen.getAllByText(/Readiness/i).length).toBeGreaterThanOrEqual(2);
        // The legacy "Internal R:R" KPI was removed in v6.9 along with
        // the matrix-level `expected_rr_internal` field; the active-side
        // R:R is now reflected via the per-side fields and the
        // Risk-Adjusted Reward-to-Risk KPI. We assert the legacy label is gone.
        expect(screen.queryByText(/Internal R:R/i)).toBeNull();
        // v6.10.19d D: the header chip is gone. v6.17: the phrase also
        // appears once more inside the Verdict & Rationale card's third
        // Why bullet ("Risk-adjusted reward-to-risk: …") — the KPI label
        // itself is still unique.
        expect(screen.getAllByText(/Risk-Adjusted Reward-to-Risk/i).length).toBe(2);
        // R7: the KPI is the advisory's ATR-derived stop-distance guide —
        // relabelled to not collide with the Top Setup card's geometric SL.
        expect(screen.getByText('ATR Stop Guide')).toBeTruthy();
        // v6.10.28: the L6 header Confidence chip was removed — the chip
        // label renders with a colon ("Confidence:"), so its absence proves
        // the header no longer duplicates the Safety Flags KPI.
        expect(screen.queryByText('Confidence:')).toBeNull();
        expect(screen.getByText('Confidence')).toBeTruthy();
        // v7.0-prod: Entry Danger moves into the safety-flags row so
        // the mirror bind contract is observable from the panel chrome.
        expect(screen.getByText('Entry Danger')).toBeTruthy();
    });

    it('v6.11: renders the Quality/Risk KPI chip from the advisory ratio', () => {
        seedPair('BTC-USDT');
        const entry = useAppStore().instancesMap['BTC-USDT'];
        entry.advisory = makeAdvisory({ quality_to_risk_ratio: 3.2 });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        expect(screen.getByText('Quality/Risk')).toBeTruthy();
        expect(screen.getByText('3.20')).toBeTruthy();
    });

    it('colors the Risk-Adjusted R:R KPI from the displayed fallback value', () => {
        // Audit regression (M3-UI): the KPI number reads
        // `riskAdjRrDisplay.value` — which falls back to
        // `topSetup.rr × (1 − overall_risk/100)` when the raw
        // `expected_reward_risk_ratio` is zero — but the color previously
        // read the raw value, so a good adjusted ratio rendered in red.
        // Color must track the resolved number.
        seedPair('BTC-USDT');
        const entry = useAppStore().instancesMap['BTC-USDT'];
        entry.decisionContext = makeDecisionContext({ expected_reward_risk_ratio: 0 });
        entry.risk = {
            ...(entry.risk ?? ({} as any)),
            overall_risk: makeDanger(1),
        };
        entry.opportunity = makeOpportunity({
            profiles: [{
                opportunity_type: 'Breakout',
                score: 65,
                preconditions_met: 2,
                preconditions_total: 2,
                notes: 'synthetic-breakout',
                direction_family: 'TREND_RIDING',
                long_entry_zone: { low: 63520, high: 63800 },
                long_target_zone: { low: 64500, high: 65000 },
                long_invalidation_level: 63200,
                long_expected_rr_internal: 3.0,
                short_entry_zone: null,
                short_target_zone: null,
                short_invalidation_level: null,
                short_expected_rr_internal: null,
                trade_viability: 'ACTIONABLE',
            }],
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });

        // Locate the KPI value span by its inline color style.
        const kpiDiv = screen
            .getAllByText(/Risk-Adjusted Reward-to-Risk/i)
            .map((el) => el.closest('div'))
            .find((d) => d && d.querySelector('[style*="color"]'));
        const value = kpiDiv?.querySelector('[style*="color"]') as HTMLElement | null;
        // 3.0 × (1 − 0.01) = 2.97 → green.
        expect(value?.textContent).toBe('2.97');
        expect(value?.style.color).toBe('rgb(34, 197, 94)');
    });

    it('v6.11: Quality/Risk chip renders an em-dash when the ratio is absent', () => {
        seedPair('BTC-USDT');
        const entry = useAppStore().instancesMap['BTC-USDT'];
        entry.advisory = makeAdvisory({ quality_to_risk_ratio: null });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        expect(screen.getByText('Quality/Risk')).toBeTruthy();
        expect(screen.getByText('—')).toBeTruthy();
    });
});

describe('RecommendationPanel — Top Setup card', () => {
    it('renders only the top-scored qualifying profile as the headline (Breakout, score 65)', () => {
        seedPair('BTC-USDT');
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // v6.10.19b (B1): the unified TOP SETUP section shows ONE verdict-
        // consistent headline. Breakout (65) is the top LONG-side
        // qualifying profile; the second (LiquiditySqueeze, 60) rides as
        // an informational alternate (it always appears on Opportunities).
        expect(screen.getByText('TOP SETUP')).toBeTruthy();
        // The headline card carries the 2/2 preconditions anchor.
        expect(screen.getByText('2/2')).toBeTruthy();
        // The 1/3 anchor for LiquiditySqueeze appears ONLY in the
        // alternate note — never as a second setup card.
        expect(screen.getAllByText(/1\/3/).length).toBeGreaterThanOrEqual(1);
        // Top setup shows per-profile LONG zones (entry low=63520, high=63800).
        expect(screen.getAllByText(/63520/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/63800/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/64500/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/65000/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/63200/).length).toBeGreaterThan(0);
        // The invalidation thesis is bound to the card's own stop-loss
        // (63200) with the LONG direction word ("below").
        expect(screen.getByText('A close below $63200 on the completed candle invalidates the Breakout thesis.')).toBeTruthy();
    });

    it('v6.14: the headline card renders the backend display_score (precondition-scaled)', () => {
        // A 1/3-precondition top profile: the local legacy rule would
        // compute round(65 × 1/3) = 22, but the wire carries the
        // authoritative 33 — the card (and its section-meta caption) must
        // render the wire value so the Recommendation and Opportunities
        // panels can never disagree.
        const entry = seedPair('BTC-USDT');
        entry.opportunity = makeOpportunity({
            profiles: [
                {
                    opportunity_type: 'Breakout',
                    score: 65,
                    preconditions_met: 1,
                    preconditions_total: 3,
                    display_score: 33,
                    notes: 'synthetic-breakout',
                    direction_family: 'TREND_RIDING',
                    trade_viability: 'ACTIONABLE',
                    long_entry_zone: { low: 63520, high: 63800 },
                    long_target_zone: { low: 64500, high: 65000 },
                    long_invalidation_level: 63200,
                    long_expected_rr_internal: 2.0,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    short_expected_rr_internal: null,
                },
            ],
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        expect(screen.getByText('TOP SETUP')).toBeTruthy();
        // Card-face score + the section-meta "score 33 · INTRADAY" caption.
        expect(screen.getAllByText('33').length).toBeGreaterThanOrEqual(1);
        expect(screen.getByText(/score 33/)).toBeTruthy();
    });

    it('Top Setup label matches the Opportunities panel top profile', () => {
        // Consistency contract: both panels must show the same top profile.
        // Build an opportunity where TrendContinuation (score 80) is the
        // top, with Breakout (score 70) and LiquiditySqueeze (score 60).
        const entry = seedPair('BTC-USDT');
        entry.opportunity = makeOpportunity({
            primary_opportunity: 'TrendContinuation',
            opportunity_score: 80,
            profiles: [
                {
                    opportunity_type: 'TrendContinuation',
                    score: 80,
                    preconditions_met: 3,
                    preconditions_total: 3,
                    notes: '',
                    direction_family: 'TREND_RIDING',
                    long_entry_zone: { low: 63000, high: 63200 },
                    long_target_zone: { low: 65000, high: 65500 },
                    long_invalidation_level: 62400,
                    long_expected_rr_internal: 2.5,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    short_expected_rr_internal: null,
                },
                {
                    opportunity_type: 'Breakout',
                    score: 70,
                    preconditions_met: 2,
                    preconditions_total: 2,
                    notes: '',
                    direction_family: 'TREND_RIDING',
                    long_entry_zone: { low: 63520, high: 63800 },
                    long_target_zone: { low: 64500, high: 65000 },
                    long_invalidation_level: 63200,
                    long_expected_rr_internal: 2.0,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    short_expected_rr_internal: null,
                },
                {
                    opportunity_type: 'LiquiditySqueeze',
                    score: 60,
                    preconditions_met: 1,
                    preconditions_total: 3,
                    notes: '',
                    direction_family: 'TREND_RIDING',
                    long_entry_zone: { low: 63600, high: 63900 },
                    long_target_zone: { low: 64400, high: 64800 },
                    long_invalidation_level: 63300,
                    long_expected_rr_internal: 1.5,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    short_expected_rr_internal: null,
                },
            ],
        } as Partial<OpportunityMatrix>);
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // Top Setup must show TrendContinuation (3/3 preconditions, score 80).
        expect(screen.getByText('3/3')).toBeTruthy();
        // And the per-profile LONG zones for TrendContinuation.
        expect(screen.getAllByText(/63000/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/63200/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/65000/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/65500/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/62400/).length).toBeGreaterThan(0);
    });

    it('renders the clean "No Active Setup" container when no profile qualifies', () => {
        const entry = seedPair('BTC-USDT');
        // Zero out profiles so every preconditions_met is 0.
        entry.opportunity = makeOpportunity({
            primary_opportunity: 'NoClearOpportunity',
            profiles: [
                {
                    opportunity_type: 'NoClearOpportunity',
                    score: 30,
                    preconditions_met: 0,
                    preconditions_total: 1,
                    notes: '',
                    direction_family: 'NEUTRAL',
                    long_entry_zone: null,
                    long_target_zone: null,
                    long_invalidation_level: null,
                    long_expected_rr_internal: null,
                    short_entry_zone: null,
                    short_target_zone: null,
                    short_invalidation_level: null,
                    short_expected_rr_internal: null,
                },
            ],
        } as Partial<OpportunityMatrix>);
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // v6.10.19c (D3): the clean empty container — no badges, no
        // no-clear card, no banner; the four fields render as placeholders.
        expect(screen.getByText('No Active Setup')).toBeTruthy();
        expect(screen.queryByText(/No Clear Setup/i)).toBeNull();
        expect(screen.queryByText(/NO CLEAR SETUP/i)).toBeNull();
        expect(screen.queryByText('No active setup — verdict is HOLD.')).toBeNull();
        // The four empty fields are present.
        expect(screen.getByText('ENTRY')).toBeTruthy();
        expect(screen.getByText('Take-Profit')).toBeTruthy();
        expect(screen.getByText('Stop-Loss')).toBeTruthy();
        expect(screen.getByText('Reward-to-Risk')).toBeTruthy();
        // No directional thesis → no invalidation subtitle.
        expect(screen.queryByText(/invalidates the .* thesis/)).toBeNull();
    });
});

describe('RecommendationPanel — bind contract', () => {
    // The recent mirror fix moved the read source from
    // `terms[1].latestSnapshot.decision_context` to `pair.decisionContext`.
    // The Recommendation tab must read from the mirror — not from the
    // shadow-wiped snapshot — so the headline R:R stays visible between
    // candle closes.
    it('reads entry_danger.score from pair.decisionContext mirror, not from the snapshot fallback', () => {
        const app = useAppStore();
        const [base] = 'BTC-USDT'.split('-');
        if (!app.instancesMap['BTC-USDT']) app.initInstance(base);
        const entry = app.instancesMap['BTC-USDT'];
        entry.terms[1].priceText = '63505';
        // Mirror has the real value: danger score 31
        entry.decisionContext = makeDecisionContext({ entry_danger: makeDanger(31) });
        // Snapshot path deliberately carries a different value to expose
        // any regression back to the snapshot read.
        entry.terms[1].latestSnapshot = {
            timestamp: 1_700_000_000,
            decision_context: makeDecisionContext({ entry_danger: makeDanger(75) }),
        } as unknown as Record<string, unknown>;
        entry.opportunity = makeOpportunity();
        entry.analysis = makeAnalysis();
        entry.advisory = makeAdvisory();

        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // v7.0-prod: the danger score surfaced on the legacy envHeader
        // (e.g. "Entry danger 31") is now hosted by the Safety Flags
        // row under the "Entry Danger" KPI. Mirror wins → 31 should be
        // visible. We assert against the literal number adjacent to
        // the Entry Danger label to keep the bind contract observable.
        expect(screen.getByText('Safety Flags')).toBeTruthy();
        expect(screen.getByText('Entry Danger')).toBeTruthy();
        // "31" must appear in the Safety Flags row (mirror value), not
        // 75 (snapshot fallback).
        const matches31 = screen.queryAllByText(/31/);
        expect(matches31.length).toBeGreaterThan(0);
    });
});

// ─────────────────────────────────────────────────────────────────────────
// Gauge geometry — lock against SVG arc-flag regressions.
//
// The active bias arc must be a geometrically congruent segment of the
// Dome curve:
//   - same radius (r=70)
//   - same center (cx=100, cy=105)
//   - bulging OUTWARD (away from the circle center) — the same
//     curvature direction as the Dome itself
//
// The earlier implementation had `sweepFlag = sweep > 0 ? 1 : 0` which
// inverted the arc on both sides (SHORT bulged inward on the right of
// the chord; LONG bulged inward on the left). These tests assert the
// correct geometry for SHORT, LONG, NEUTRAL, and the ±100 extremes.
// ─────────────────────────────────────────────────────────────────────────
describe('gauge geometry — active arc is a Dome segment', () => {
    function mountWithNetBias(netBias: number) {
        // Build a 100% probability split so netBias is exactly `netBias`
        // (long_probability - short_probability = netBias when hold=0).
        // Note: rank.long.probability prefers the wire value when present.
        const long = 50 + netBias / 2;
        const short = 50 - netBias / 2;
        const hold = 100 - long - short;
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: long,
            short_probability: short,
            hold_probability: hold,
            net_bias_pct: netBias,
        });
        return render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
    }

    function parseActiveArc(d: string): {
        startX: number;
        startY: number;
        endX: number;
        endY: number;
        sweepFlag: number;
        largeFlag: number;
    } | null {
        // SVG arc: M sx sy A rx ry x-axis-rot large-flag sweep-flag ex ey
        // 9 numeric groups: M-x M-y rx ry rot large sweep ex ey
        const m = /^M\s+([-\d.]+)\s+([-\d.]+)\s+A\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)/.exec(d);
        if (!m) return null;
        return {
            startX: Number(m[1]),
            startY: Number(m[2]),
            largeFlag: Number(m[6]),
            sweepFlag: Number(m[7]),
            endX: Number(m[8]),
            endY: Number(m[9]),
        };
    }

    function getActiveArc(): { startX: number; startY: number; endX: number; endY: number; sweepFlag: number; largeFlag: number } | null {
        // Scope to the gauge SVG only — the LayerHeader icon and other
        // chrome also contain `<path>` elements that we don't want.
        const gaugeSvg = document.querySelector('svg.gauge, [class*="gauge"] svg');
        const paths = gaugeSvg ? gaugeSvg.querySelectorAll('path') : document.querySelectorAll('svg path');
        // The active arc starts at the top-center (100, 35); the Dome
        // starts at (30, 105). Both live in the gauge card.
        for (const p of Array.from(paths)) {
            const d = p.getAttribute('d') ?? '';
            if (!d.startsWith('M 100 35')) continue;
            return parseActiveArc(d);
        }
        return null;
    }

    it('arc starts at top-center (100, 35) — the Dome apex', () => {
        mountWithNetBias(0);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        expect(arc!.startX).toBeCloseTo(100, 2);
        expect(arc!.startY).toBeCloseTo(35, 2);
    });

    it('SHORT bias uses sweepFlag=0 (counterclockwise from top, bulges UP-LEFT along Dome)', () => {
        mountWithNetBias(-50);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        expect(arc!.sweepFlag).toBe(0);
        expect(arc!.largeFlag).toBe(0);
        // Endpoint is on the LEFT side of the Dome (x < 100).
        expect(arc!.endX).toBeLessThan(100);
    });

    it('LONG bias uses sweepFlag=1 (clockwise from top, bulges UP-RIGHT along Dome)', () => {
        mountWithNetBias(50);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        expect(arc!.sweepFlag).toBe(1);
        expect(arc!.largeFlag).toBe(0);
        // Endpoint is on the RIGHT side of the Dome (x > 100).
        expect(arc!.endX).toBeGreaterThan(100);
    });

    it('arc endpoint lies on the Dome circle (distance from (100,105) == 70)', () => {
        // Sample across SHORT, NEUTRAL, LONG.
        for (const nb of [-75, -25, 0, 25, 75]) {
            mountWithNetBias(nb);
            const arc = getActiveArc();
            expect(arc).not.toBeNull();
            const dx = arc!.endX - 100;
            const dy = arc!.endY - 105;
            const dist = Math.sqrt(dx * dx + dy * dy);
            expect(dist).toBeCloseTo(70, 1);
        }
    });

    it('-100% net bias terminates exactly at the Dome LEFT extreme (30, 105)', () => {
        mountWithNetBias(-100);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        expect(arc!.endX).toBeCloseTo(30, 1);
        expect(arc!.endY).toBeCloseTo(105, 1);
        expect(arc!.sweepFlag).toBe(0); // counterclockwise → LEFT half of Dome
    });

    it('+100% net bias terminates exactly at the Dome RIGHT extreme (170, 105)', () => {
        mountWithNetBias(100);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        expect(arc!.endX).toBeCloseTo(170, 1);
        expect(arc!.endY).toBeCloseTo(105, 1);
        expect(arc!.sweepFlag).toBe(1); // clockwise → RIGHT half of Dome
    });

    it('arc bulges OUTWARD — midpoint y is above the chord midpoint y', () => {
        // For SHORT (-50%): chord goes from (100, 35) to roughly (50, 55).
        // The Dome-segment midpoint (halfway along the arc) should sit
        // ABOVE the chord's midpoint — outward, not inward.
        mountWithNetBias(-50);
        const arc = getActiveArc();
        expect(arc).not.toBeNull();
        const chordMidY = (arc!.startY + arc!.endY) / 2;
        // For an arc bulging outward on the Dome (center at cy=105),
        // a point halfway along the 45° arc has y < chordMidY
        // (i.e., closer to the top of the circle, further from cy).
        // The midpoint angle is halfway between 90° and 135° = 112.5°.
        const halfAngle = (Math.PI / 2 + (3 * Math.PI / 4)) / 2;
        const arcMidY = 105 - 70 * Math.sin(halfAngle);
        expect(arcMidY).toBeLessThan(chordMidY);
    });

    it('renders a needle line from pivot (100, 109) to the active arc terminus', () => {
        // The needle is a thin straight line connecting the pivot to
        // the active arc's endpoint. It must terminate at the same
        // (needleX, needleY) the arc terminates at, so the two
        // indicators visually align.
        for (const nb of [-75, -25, 25, 75]) {
            mountWithNetBias(nb);
            const arc = getActiveArc();
            expect(arc).not.toBeNull();
            const gaugeSvg = document.querySelector('svg.gauge, [class*="gauge"] svg');
            const lines = gaugeSvg ? gaugeSvg.querySelectorAll('line') : [];
            const needle = Array.from(lines).find((l) =>
                l.getAttribute('x1') === '100' && l.getAttribute('y1') === '109'
            );
            expect(needle).toBeTruthy();
            expect(Number(needle!.getAttribute('x2'))).toBeCloseTo(arc!.endX, 1);
            expect(Number(needle!.getAttribute('y2'))).toBeCloseTo(arc!.endY, 1);
            expect(needle!.getAttribute('stroke-width')).toBe('2');
            expect(needle!.getAttribute('stroke-linecap')).toBe('round');
        }
    });

    it('needle is visible at neutral with amber color (netBias = 0)', () => {
        // The needle is the always-on directional indicator: at neutral
        // it renders as a thin amber vertical line straight up. The
        // active arc is the magnitude indicator and stays hidden at
        // neutral — but the needle must remain visible so the operator
        // sees the "no lean" pointer.
        mountWithNetBias(0);
        const gaugeSvg = document.querySelector('svg.gauge, [class*="gauge"] svg');
        const lines = gaugeSvg ? gaugeSvg.querySelectorAll('line') : [];
        const needle = Array.from(lines).find((l) =>
            l.getAttribute('x1') === '100' && l.getAttribute('y1') === '109'
        );
        expect(needle).toBeTruthy();
        expect(needle!.getAttribute('opacity')).toBe('0.95');
        expect(needle!.getAttribute('stroke')).toBe('#f59e0b');
        // Tip lands at the top center (gaugeAngle = π/2).
        expect(Number(needle!.getAttribute('x2'))).toBeCloseTo(100, 1);
        expect(Number(needle!.getAttribute('y2'))).toBeCloseTo(35, 1);
    });
});

// ─────────────────────────────────────────────────────────────────────────
// R1 + GAUGE-001: the needle is verdict-consistent — a HOLD verdict (hold
// probability dominant) neutralizes the needle even when the raw net bias
// (long − short) is non-zero. The center-bottom dial label mirrors the
// needle: it renders the verdict-consistent net % (0 under HOLD), never the
// raw split. No LONG/HOLD/SHORT percentage split is rendered under the dial.
// ─────────────────────────────────────────────────────────────────────────
describe('gauge verdict consistency — needle neutralizes under HOLD', () => {
    function mountHoldVerdictWithNetBias() {
        // The user's real 1s sample: long 46 / hold 52 / short 2
        // (net bias +44%) with the hold probability dominant → HOLD
        // verdict. The raw net bias is +44 but the needle must sit
        // neutral.
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // no qualifying setup → the verdict stays a genuine HOLD
        entry.decisionContext = makeDecisionContext({
            long_probability: 46,
            short_probability: 2,
            hold_probability: 52,
            net_bias_pct: 44,
        });
        return render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
    }

    it('needle sits at neutral (top center, amber, 0%) under a HOLD verdict with a +44 net bias', () => {
        mountHoldVerdictWithNetBias();
        const gaugeSvg = document.querySelector('svg.gauge, [class*="gauge"] svg');
        const lines = gaugeSvg ? gaugeSvg.querySelectorAll('line') : [];
        const needle = Array.from(lines).find((l) =>
            l.getAttribute('x1') === '100' && l.getAttribute('y1') === '109'
        );
        expect(needle).toBeTruthy();
        // Neutralized: tip at top center, amber — NOT green at +44%.
        expect(Number(needle!.getAttribute('x2'))).toBeCloseTo(100, 1);
        expect(Number(needle!.getAttribute('y2'))).toBeCloseTo(35, 1);
        expect(needle!.getAttribute('stroke')).toBe('#f59e0b');
        // The active arc stays hidden.
        const paths = gaugeSvg ? gaugeSvg.querySelectorAll('path') : [];
        const activeArc = Array.from(paths).find((p) => (p.getAttribute('d') ?? '').startsWith('M 100 35'));
        expect(activeArc?.getAttribute('opacity')).toBe('0');
        // The center-bottom dial label mirrors the needle: 0% amber — the
        // raw +44% bias is verdict-neutralized and must NOT render.
        expect(screen.getByText('0%')).toBeTruthy();
        expect(screen.queryByText('+44%')).toBeNull();
    });

    it('GAUGE-001: no percentage split is rendered under the dial (the net label is the single final number)', () => {
        mountHoldVerdictWithNetBias();
        // The dial labels LONG/SHORT exist at the flanks, the net % label
        // sits center-bottom, but the LONG/HOLD/SHORT probability split
        // must NOT appear — the net label is the single final number
        // (0 under HOLD).
        expect(screen.getAllByText(/LONG/).length).toBeGreaterThanOrEqual(1);
        expect(screen.getAllByText(/HOLD/).length).toBeGreaterThanOrEqual(1);
        expect(screen.getAllByText(/SHORT/).length).toBeGreaterThanOrEqual(1);
        expect(screen.queryByText('46%')).toBeNull();
        expect(screen.queryByText('52%')).toBeNull();
        expect(screen.queryByText('2%')).toBeNull();
    });
});

// ─────────────────────────────────────────────────────────────────────────
// R6: the Final Verdict block is verdict-consistent — under HOLD it shows
// the verdict sentence (not the advisory's directional "Entry: immediate"),
// with the advisory text demoted to muted environment guidance.
// ─────────────────────────────────────────────────────────────────────────
describe('RecommendationPanel — Final Verdict + Environment Guidance (R6)', () => {
    it('renders the verdict sentence + guidance under a HOLD verdict', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // keep a genuine HOLD (no qualifying setup)
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // Default fixture resolves to HOLD → verdict sentence.
        expect(screen.getByText(/HOLD — no directional call/)).toBeTruthy();
        expect(screen.getByText(/Environment guidance:/)).toBeTruthy();
    });

    it('v6.17: renders the advisory sentence under a directional verdict with the verdict-consistent lead', () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // LONG verdict → guidance leads with the verdict's OWN read
        // (same direction + probability as the headline) — the stale
        // "Neutral — no directional edge" claim can never survive.
        expect(screen.getByText(/Bullish market bias with 60% confidence/)).toBeTruthy();
        expect(screen.queryByText(/Neutral — no directional edge/)).toBeNull();
    });

    it('renders the guidance cards with tactic labels and no reference caption under HOLD (v6.10.28)', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // keep a genuine HOLD (no qualifying setup)
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        expect(screen.getByText('Environment Guidance')).toBeTruthy();
        expect(screen.getByText('Trigger Tactic')).toBeTruthy();
        expect(screen.getByText('Exit Condition')).toBeTruthy();
        expect(screen.getByText('Protection')).toBeTruthy();
        expect(screen.getByText('Target')).toBeTruthy();
        expect(screen.queryByText(/For reference — no active directional call/)).toBeNull();
    });

    it('v6.10.28 + v6.17 + v7.2: the Verdict & Rationale card accent line is amber under a HOLD verdict', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // keep a genuine HOLD (no qualifying setup)
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const card = document.querySelector('[aria-label="VERDICT & RATIONALE"]')!;
        const cls = card.className;
        expect(cls).toMatch(/accentHold/);
        expect(cls).not.toMatch(/accentLong|accentShort/);
    });

    it('v6.10.28 + v6.17 + v7.2: the Verdict & Rationale card accent line is green under a LONG verdict', () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const cls = document.querySelector('[aria-label="VERDICT & RATIONALE"]')!.className;
        expect(cls).toMatch(/accentLong/);
        expect(cls).not.toMatch(/accentShort/);
    });

    it('v6.10.28 + v6.17 + v7.2: the Verdict & Rationale card accent line is red under a SHORT verdict', () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 10,
            short_probability: 60,
            hold_probability: 30,
            net_bias_pct: -50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const cls = document.querySelector('[aria-label="VERDICT & RATIONALE"]')!.className;
        expect(cls).toMatch(/accentShort/);
        expect(cls).not.toMatch(/accentLong/);
    });

    it('v6.17: verdict, guidance and Why bullets share ONE card separated by a divider', () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // v7.0: the card lives inside the VERDICT & RATIONALE summary card
        // (the standalone section title is gone — the card label is the
        // kicker, and the old "Final Verdict"/"Why" titles never existed).
        expect(screen.getByLabelText('VERDICT & RATIONALE')).toBeTruthy();
        expect(screen.queryByText('Final Verdict')).toBeNull();
        expect(screen.queryByText('Why')).toBeNull();
        // Verdict quote + guidance + divider + bullets all live in the card.
        const card = document.querySelector('[class*="verdictCard"]')!;
        expect(card.querySelector('blockquote')!.textContent).toContain('LONG lean 60%');
        expect(card.querySelector('[class*="verdictGuidance"]')).toBeTruthy();
        expect(card.querySelector('[class*="verdictDivider"]')).toBeTruthy();
        expect(card.querySelectorAll('[class*="whyItem"]').length).toBe(3);
        // Bullets are the polished prose (no L-tokens, sentence-cased).
        expect(card.textContent).toContain('Layer 2 Tradability Dimension');
        expect(card.textContent).toContain('Trade readiness is');
    });
});

// ─────────────────────────────────────────────────────────────────────────
// FIX-3/FIX-4/FIX-5 (v6.10.15) + v6.10.17 decoupling: the user's real
// capture — a STAND ASIDE badge with a LONG-dominant probability verdict
// (long 62 / hold 36) — now renders the DIRECTIONAL needle (+60%), the
// graded verdict sentence ("LONG lean 62% — STAND ASIDE…"), and a REAL
// playbook (the lean is directional, only the gate is STAND ASIDE). The
// flat HOLD + STAND ASIDE state keeps the neutral needle and the
// no-directional-call sentence.
// ─────────────────────────────────────────────────────────────────────────
describe('RecommendationPanel — STAND ASIDE with a directional verdict (v6.10.17)', () => {
    function mountStandAsideWithLongVerdict() {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            trade_readiness: 'STAND_ASIDE',
            long_probability: 62,
            short_probability: 2,
            hold_probability: 36,
            net_bias_pct: 60,
        });
        return render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
    }

    it('needle shows the +60% directional read under a STAND ASIDE badge (decoupled)', () => {
        mountStandAsideWithLongVerdict();
        const gaugeSvg = document.querySelector('svg.gauge, [class*="gauge"] svg');
        const lines = gaugeSvg ? gaugeSvg.querySelectorAll('line') : [];
        const needle = Array.from(lines).find((l) =>
            l.getAttribute('x1') === '100' && l.getAttribute('y1') === '109'
        );
        expect(needle).toBeTruthy();
        // Directional: the +60% LONG tip points right (x2 > 100), green.
        expect(Number(needle!.getAttribute('x2'))).toBeGreaterThan(100);
        expect(needle!.getAttribute('stroke')).toBe('#22c55e');
        // The center-bottom dial label mirrors the needle: +60% green.
        expect(screen.getByText('+60%')).toBeTruthy();
    });

    it('renders the graded verdict sentence + guidance (LONG lean 62% — Stand Aside)', () => {
        mountStandAsideWithLongVerdict();
        expect(screen.getByText(/LONG lean 62% — Stand Aside/)).toBeTruthy();
        expect(screen.getByText(/Environment guidance:/)).toBeTruthy();
        expect(screen.queryByText(/no directional call/)).toBeNull();
    });

    it('renders the real playbook under a directional-gated verdict', () => {
        mountStandAsideWithLongVerdict();
        expect(screen.queryByText(/For reference — no active directional call/)).toBeNull();
        expect(screen.getByText(/Wait For Confirmation/)).toBeTruthy();
    });

    it('flat HOLD + STAND ASIDE keeps the neutral needle and no-directional-call sentence', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // no qualifying setup → the verdict stays a genuine flat HOLD
        entry.decisionContext = makeDecisionContext({
            trade_readiness: 'STAND_ASIDE',
            long_probability: 2,
            short_probability: 2,
            hold_probability: 96,
            net_bias_pct: 0,
            bias: 'Neutral',
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        expect(screen.getByText(/HOLD — no directional call/)).toBeTruthy();
        // The center-bottom dial label mirrors the neutral needle: 0%.
        expect(screen.getByText(/0%/)).toBeTruthy();
    });
});

describe('RecommendationPanel — v7.0 VERDICT & RATIONALE head card', () => {
    it('renders the verdict inside the summary card above the directional gauge', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry); // keep a genuine HOLD (no qualifying setup)
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const card = screen.getByLabelText('VERDICT & RATIONALE');
        expect(card).toBeTruthy();
        expect(card.textContent).toContain('HOLD — no directional call');
        const gauge = document.querySelector('[class*="gaugeCard"]')!;
        expect(card.compareDocumentPosition(gauge) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('v7.0: the old bottom "Verdict & Rationale" section title is gone', () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry);
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        // The standalone section title element is gone — the summary card
        // kicker renders "VERDICT & RATIONALE" (uppercase label) instead.
        expect(screen.queryAllByText('Verdict & Rationale').length).toBe(0);
        expect(screen.getByText('VERDICT & RATIONALE')).toBeTruthy();
    });
});

describe('RecommendationPanel — v7.0 Project Risk and Return drawer', () => {
    it('the header action toggles the drawer with the setup geometry', async () => {
        const entry = seedPair('BTC-USDT');
        // Directional LONG verdict so a qualifying top setup exists.
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const button = screen.getByText('Project Risk and Return');
        expect(button).toBeTruthy();
        await button.click();
        const drawer = screen.getByLabelText('Project Risk and Return');
        expect(drawer).toBeTruthy();
        expect(screen.getByText('Capital Allocation')).toBeTruthy();
        expect(screen.getByText('Leverage')).toBeTruthy();
        expect(drawer.textContent).toContain('LONG');
        // Toggling again closes the drawer.
        await button.click();
        expect(screen.queryByLabelText('Project Risk and Return')).toBeNull();
    });

    it('the export carries the empty projection until the drawer is configured', async () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        const button = screen.getByText('Project Risk and Return');
        await button.click();
        const drawer = screen.getByLabelText('Project Risk and Return');
        expect(drawer).toBeTruthy();
        // The drawer prefills the LONG setup geometry into its header line.
        expect(drawer.textContent).toContain('LONG');
    });

    it('v7.3: settles the recommendation geometry into editable fields', async () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        await screen.getByText('Project Risk and Return').click();
        const drawer = screen.getByLabelText('Project Risk and Return');
        // Direction / Entry / Stop Loss / Take Profit are editable fields
        // seeded from the LONG recommendation (Breakout top profile:
        // entry 63660, invalidation 63200, target mid 64750).
        expect((screen.getByLabelText(/Direction/) as HTMLSelectElement).value).toBe('LONG');
        expect((screen.getByLabelText(/Entry/) as HTMLInputElement).value).toBe('63660');
        expect((screen.getByLabelText(/Stop Loss/) as HTMLInputElement).value).toBe('63200');
        expect((screen.getByLabelText(/Take Profit/) as HTMLInputElement).value).toBe('64750');
        // The old static string summary is gone.
        expect(drawer.textContent).not.toContain('entry $');
    });

    it('v7.3: result values carry the cost/risk color language (liq amber, fees red, pnl green)', async () => {
        const entry = seedPair('BTC-USDT');
        entry.decisionContext = makeDecisionContext({
            long_probability: 60,
            short_probability: 10,
            hold_probability: 30,
            net_bias_pct: 50,
        });
        const app = useAppStore();
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        await screen.getByText('Project Risk and Return').click();
        // Seed a resolved calculation so the results grid renders. A stale
        // debounced calc from a previous test could still be in flight —
        // force the calculating flag off so the grid is deterministic.
        app.riskCalculation = {
            position_size_units: '0.012345',
            position_notional: '800',
            margin_required: '80',
            liquidation_price: '62100.5',
            total_fees: '1.23',
            net_pnl: '42.5',
            risk_reward_ratio: '2.4',
        } as any;
        app.riskCalculating = false;
        await tick();
        const amber = document.querySelectorAll('[class*="resultValueAmber"]');
        expect(amber.length).toBe(1);
        expect(amber[0].closest('[class*="resultItem"]')!.textContent).toContain('Liquidation Price');
        expect(document.querySelectorAll('[class*="resultValueRed"]').length).toBe(3);
        expect(document.querySelectorAll('[class*="resultValuePos"]').length).toBeGreaterThanOrEqual(1);
    });

    it('v7.4: with no active setup the drawer opens as a blank manual what-if editor', async () => {
        const entry = seedPair('BTC-USDT');
        zeroProfiles(entry);
        render(RecommendationPanel, { props: { pairKey: 'BTC-USDT' } });
        await screen.getByText('Project Risk and Return').click();
        const drawer = screen.getByLabelText('Project Risk and Return');
        expect(drawer).toBeTruthy();
        expect(screen.getByText('Capital Allocation')).toBeTruthy();
        expect(screen.getByText('Leverage')).toBeTruthy();
        expect((screen.getByLabelText(/Direction/) as HTMLSelectElement).value).toBe('LONG');
        expect((screen.getByLabelText(/Entry/) as HTMLInputElement).value).toBe('');
        expect((screen.getByLabelText(/Stop Loss/) as HTMLInputElement).value).toBe('');
        expect((screen.getByLabelText(/Take Profit/) as HTMLInputElement).value).toBe('');
    });
});
