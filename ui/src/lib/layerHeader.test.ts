// @vitest-environment node
//
// layerHeader.spec — the canonical "single badge per MME tab" contract.
//
// These tests pin the v7.0-prod spec rules:
//   • exactly one authoritative primary badge per layer
//   • empty / zero / error are visually distinct (discriminated by `state`)
//   • L5 risk-zero is GREEN (zero risk = meaningful good news)
//   • L4 opportunity-zero (no clear setup) is AMBER NEUTRAL
//   • L3 regime chip is hidden when redundant with the bias
//   • L6 never reads L3 bias (high-priority v6.9 regression guard)
//   • L7 status discriminates live / stale / error from fetch state
//
// The builders are tested in isolation (pure functions) so every property
// above is observable directly from the `LayerHeaderSpec` shape the
// component consumes.

import { describe, it, expect } from 'vitest';
import { buildL1MetricsHeader, buildL1MtfHeader, metricsBadgeFor, buildL2AlignmentHeader, buildL3AnalysisHeader, buildL4OpportunityHeader, buildL5RiskHeader, buildL6DecisionHeader, buildL7OverviewHeader, chip, emptyBadge, hexToRgba, type LayerHeaderSpec } from './layerHeader';
import { DASHBOARD_COLORS, biasColor, riskDangerColor, scoreColor } from './dashboardColors';
import { COLORS } from './scoreStyles';
import type {
    AdvisoryMatrix,
    AlignmentMatrix,
    AnalysisMatrix,
    ContextDimension,
    DecisionContext,
    MarketContext,
    OpportunityMatrix,
    OverviewMatrix,
    RiskDimension,
    RiskMatrix,
} from '../types';

// ── Tiny fixture builders ────────────────────────────────────────────────

function ctx(overrides: Partial<MarketContext> = {}): MarketContext {
    const makeDim = (label: string): ContextDimension => ({ score: 50, confidence: 50, label });
    return {
        trend: makeDim('NEUTRAL'),
        momentum: makeDim('NEUTRAL'),
        volatility: makeDim('NORMAL'),
        volume: makeDim('NORMAL'),
        liquidity: makeDim('ADEQUATE'),
        regime: 'RANGE',
        overall_score: 50,
        overall_label: 'NEUTRAL',
        ...overrides,
    };
}

function tfStub(context: MarketContext | null) {
    return {
        slot: 1,
        context,
        indicators: {},
        isCompleted: context != null,
        pipelineState: context ? 'LIVE' : 'LOADING',
    } as any;
}

function alignmentStub(overrides: Partial<AlignmentMatrix> = {}): AlignmentMatrix {
    return {
        symbol: 'BTC-USDT',
        timeframes_present: 4,
        dimensions: [],
        mtf_trend_alignment: 0,
        mtf_momentum_alignment: 0,
        mtf_volume_alignment: 0,
        mtf_volatility_alignment: 0,
        mtf_overall_score: 0,
        mtf_overall_label: 'NEUTRAL_MTF',
        timeframe_alignments: [],
        signal_cross_tf_count: 0,
        trend_agreement_pct: 0,
        ...overrides,
    };
}

function analysisStub(overrides: Partial<AnalysisMatrix> = {}): AnalysisMatrix {
    return {
        symbol: 'BTC-USDT',
        bias: 'Neutral',
        confidence: 0,
        state_confidence: 0,
        market_regime: 'Range',
        trend_assessment: 'Developing',
        momentum_assessment: 'Increasing',
        structure_assessment: 'Healthy',
        volatility_assessment: 'Normal',
        volume_assessment: 'Normal',
        market_quality: 'Average',
        market_quality_score: 50,
        market_phase: 'ACCUMULATION',
        market_interpretation: '',
        rationale: '',
        supporting_signals: [],
        contradicting_signals: [],
        timeframes_considered: 4,
        ...overrides,
    } as AnalysisMatrix;
}

function opportunityStub(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'Pullback',
        opportunity_score: 60,
        setup_quality: 'Moderate',
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
        long_expected_rr_internal: 1.5,
        short_expected_rr_internal: 1.0,
        time_horizon: 'SWING',
        confluent_entry_levels: [],
        confluent_target_levels: [],
        confluent_invalidation_levels: [],
        ...overrides,
    } as OpportunityMatrix;
}

function riskStub(overrides: Partial<RiskDimension> = {}): RiskDimension {
    return {
        score: 35,
        level: 'Low',
        state: 'Stable',
        confidence: 70,
        evidence: [],
        ...overrides,
    };
}

function riskMatrixStub(overall: RiskDimension): RiskMatrix {
    return {
        symbol: 'BTC-USDT',
        market_risk: riskStub({ score: 30 }),
        volatility_risk: riskStub({ score: 35 }),
        execution_liquidity_risk: riskStub({ score: 25 }),
        structure_risk: riskStub({ score: 40 }),
        momentum_risk: riskStub({ score: 30 }),
        signal_risk: riskStub({ score: 20 }),
        execution_risk: riskStub({ score: 25 }),
        cascade_risk: riskStub({ score: 30 }),
        overall_risk: overall,
    };
}

function decisionCtxStub(overrides: Partial<DecisionContext> = {}): DecisionContext {
    return {
        score: 30,
        bias: 'Bullish',
        confidence: 0.7,
        score_confidence: 0.7,
        entry_danger: riskStub({ score: 30 }),
        expected_reward_risk_ratio: 1.8,
        trade_readiness: 'READY',
        contributing_indicators: [],
        ...overrides,
    } as DecisionContext;
}

function advisoryStub(overrides: Partial<AdvisoryMatrix> = {}): AdvisoryMatrix {
    return {
        symbol: 'BTC-USDT',
        directional_guidance: 'Long',
        market_stance: 'Constructive',
        opportunity_classification: 'Pullback',
        strategy_environment: 'TrendFollowing',
        entry_guidance: 'WaitForConfirmation',
        exit_guidance: 'NoWarning',
        protection_strategy: 'ATRBased',
        target_strategy: 'ResistanceBased',
        confidence_assessment: 60,
        stop_loss_distance_pct: 0.015,
        cascade_risk_score: 30,
        environment_favorability: riskStub({ score: 25 }),
        final_recommendation: '',
        ...overrides,
    } as AdvisoryMatrix;
}

function overviewStub(overrides: Partial<OverviewMatrix> = {}): OverviewMatrix {
    return {
        global_market_bias: 'BULLISH',
        market_breadth: 'POSITIVE',
        regime_distribution: {},
        opportunity_distribution: {},
        risk_distribution: { low_pct: 0, moderate_pct: 0, high_pct: 0, risk_environment: 'LOW' },
        asset_ranking: [],
        market_synchronization: 'SYNCHRONIZED',
        market_health: 'HEALTHY',
        global_summary: '',
        instance_count: 3,
        active_symbols: [],
        ...overrides,
    } as OverviewMatrix;
}

// ── Helpers ──────────────────────────────────────────────────────────────

function expectEmpty(spec: LayerHeaderSpec) {
    expect(spec.badge.label).toBe('\u2014');
    expect(spec.badge.state).toBe('empty');
    expect(spec.meta).toEqual([]);
}

describe('layerHeader: discriminated state helpers', () => {
    it('emptyBadge returns a dash in italic grey (state=empty)', () => {
        const b = emptyBadge();
        expect(b.label).toBe('\u2014');
        expect(b.state).toBe('empty');
        expect(b.color).toBe(DASHBOARD_COLORS.inactive);
    });

    it('chip returns `empty` when raw value is null/undefined/""', () => {
        for (const v of [null, undefined, '']) {
            const c = chip('X', v, null, null);
            expect(c.state).toBe('empty');
            expect(c.value).toBe('\u2014');
        }
    });

    it('chip returns `neutral` (amber) for numeric 0 by default', () => {
        const c = chip('Score', 0, 0, scoreColor);
        expect(c.state).toBe('neutral');
        expect(c.color).toBe(COLORS.neutral);
    });

    it('chip returns `valid` (green) for numeric 0 when zeroIsGood=true', () => {
        const c = chip('Risk', 0, 0, riskDangerColor, false, { zeroIsGood: true });
        expect(c.state).toBe('valid');
        expect(c.color).toBe('#22c55e');
    });

    it('chip returns `valid` with the colourFn result for non-zero numeric', () => {
        const c = chip('Score', 85, 85, scoreColor);
        expect(c.state).toBe('valid');
        expect(c.color).toBe(scoreColor(85));
    });

    it('hexToRgba converts #abc123 → rgba with alpha', () => {
        const out = hexToRgba('#22c55e', 0.08);
        expect(out).toBe('rgba(34, 197, 94, 0.08)');
    });

    it('hexToRgba tolerates malformed hex by returning neutral rgba', () => {
        const out = hexToRgba('#zzz', 0.1);
        expect(out).toBe('rgba(255,255,255,0.1)');
    });
});

// ── L1 — Metrics ────────────────────────────────────────────────────────

describe('buildL1MetricsHeader (L1 single-TF)', () => {
    it('returns empty badge + loading when no context present', () => {
        const spec = buildL1MetricsHeader(tfStub(null));
        expectEmpty(spec);
        expect(spec.status).toBe('loading');
        expect(spec.layerNumber).toBe(1);
    });

    it('badge = overall_label, sublabel suppressed when regime is implied by label', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'TRENDING', overall_score: 80 })));
        expect(spec.badge.label).toBe('STRONG BULL');
        // regime 'TRENDING' is implied by 'BULLISH' label → suppressed
        expect(spec.badge.sublabel).toBeUndefined();
        expect(spec.status).toBe('live');
        expect(spec.meta.some((c) => c.label === 'Score')).toBe(true);
    });

    it('badge sublabel = regime when regime is NOT implied', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'CONTRACTION', overall_score: 50 })));
        expect(spec.badge.label).toBe('STRONG BULL');
        expect(spec.badge.sublabel).toBe('CONTRACTION');
    });

    it('zero score is rendered neutral amber, not error', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'NEUTRAL', regime: 'RANGE', overall_score: 0 })));
        const scoreChip = spec.meta.find((c) => c.label === 'Score')!;
        expect(scoreChip.state).toBe('neutral');
        expect(scoreChip.color).toBe(COLORS.neutral);
    });

    it('M-4: status flows through tfStatusFrom — ws closed → error, pipeline STALE → stale, shadow tick → live, pipeline LOADING → loading', () => {
        // The node test env has no global WebSocket — polyfill the two
        // constants tfStatusFrom reads.
        (globalThis as any).WebSocket = { OPEN: 1, CLOSED: 3 };
        const closedWs = { sockets: { 1: { readyState: 3 /* CLOSED */ } as WebSocket } };
        expect(buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL' })), closedWs).status).toBe('error');
        const stale = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        stale.pipelineState = 'STALE';
        expect(buildL1MetricsHeader(stale).status).toBe('stale');
        // v6.13: a shadow tick (isCompleted=false) with a LIVE pipeline
        // stays live — `pipeline_state` is authoritative and the old
        // `!tf.isCompleted → loading` rule flashed "loading" between
        // candle closes on every healthy stream.
        const shadow = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        shadow.isCompleted = false;
        expect(buildL1MetricsHeader(shadow).status).toBe('live');
        const failed = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        failed.pipelineState = 'FAILED';
        expect(buildL1MetricsHeader(failed).status).toBe('error');
        const loading = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        loading.pipelineState = 'LOADING';
        expect(buildL1MetricsHeader(loading).status).toBe('loading');
        // Healthy completed tick stays live.
        expect(buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL' }))).status).toBe('live');
    });
});

// ── metricsBadgeFor (single-source L1 badge — v10.2 TF status table) ────

describe('metricsBadgeFor (single-source L1 badge)', () => {
    it('returns the empty badge + loading when the TF is missing or has no context label', () => {
        for (const tf of [null, undefined, tfStub(null), tfStub(ctx({ overall_label: null as any }))]) {
            const info = metricsBadgeFor(tf, null);
            expect(info.badge.label).toBe('\u2014');
            expect(info.badge.state).toBe('empty');
            expect(info.status).toBe('loading');
        }
    });

    it('badge = prettified overall_label + biasColor + regime sublabel rule (identical to the L1 header badge)', () => {
        const info = metricsBadgeFor(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'CONTRACTION', overall_score: 80 })), null);
        expect(info.badge.label).toBe('STRONG BULL');
        expect(info.badge.color).toBe(biasColor('STRONG_BULL'));
        expect(info.badge.background).toBe(hexToRgba(biasColor('STRONG_BULL'), 0.08));
        expect(info.badge.sublabel).toBe('CONTRACTION');
        expect(info.badge.state).toBe('valid');
        // Regime implied by the label → suppressed (same rule as the header).
        const implied = metricsBadgeFor(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'TRENDING' })), null);
        expect(implied.badge.sublabel).toBeUndefined();
    });

    it('status flows through tfStatusFrom (M-4 semantics)', () => {
        (globalThis as any).WebSocket = { OPEN: 1, CLOSED: 3 };
        const closedWs = { sockets: { 1: { readyState: 3 /* CLOSED */ } as WebSocket } };
        expect(metricsBadgeFor(tfStub(ctx({ overall_label: 'STRONG_BULL' })), closedWs).status).toBe('error');
        const stale = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        stale.pipelineState = 'STALE';
        expect(metricsBadgeFor(stale, null).status).toBe('stale');
        const loading = tfStub(ctx({ overall_label: 'STRONG_BULL' }));
        loading.pipelineState = 'LOADING';
        expect(metricsBadgeFor(loading, null).status).toBe('loading');
        expect(metricsBadgeFor(tfStub(ctx({ overall_label: 'STRONG_BULL' })), null).status).toBe('live');
    });

    it('buildL1MetricsHeader delegates to metricsBadgeFor — badge + status are identical objects', () => {
        const tf = tfStub(ctx({ overall_label: 'WEAK_BEAR', regime: 'EXPANSION', overall_score: 42 }));
        const info = metricsBadgeFor(tf, null);
        const spec = buildL1MetricsHeader(tf, null);
        expect(spec.badge).toEqual(info.badge);
        expect(spec.status).toBe(info.status);
        // Empty branch too.
        const emptyInfo = metricsBadgeFor(null, null);
        const emptySpec = buildL1MetricsHeader(null, null);
        expect(emptySpec.badge).toEqual(emptyInfo.badge);
        expect(emptySpec.status).toBe(emptyInfo.status);
    });
});

describe('buildL1MtfHeader (L1 Multi-TF)', () => {
    it('renders MTF SYNC badge with Timeframes N/14 / Agreement / Cross chips', () => {
        const spec = buildL1MtfHeader(alignmentStub({ timeframes_present: 4, trend_agreement_pct: 75, signal_cross_tf_count: 3 }), 'Synchronized');
        expect(spec.badge.label).toBe('MTF SYNC');
        expect(spec.badge.sublabel).toBe('Synchronized');
        const tfs = spec.meta.find((c) => c.label === 'Timeframes')!;
        expect(tfs.value).toBe('4/14');
        const a = spec.meta.find((c) => c.label === 'Agreement')!;
        expect(a.value).toBe('75%');
        expect(spec.status).toBe('live');
    });

    it('status transitions live→stale→loading as TFs drop below 3/1/0', () => {
        const live = buildL1MtfHeader(alignmentStub({ timeframes_present: 4 })).status;
        const stale = buildL1MtfHeader(alignmentStub({ timeframes_present: 2 })).status;
        const empty = buildL1MtfHeader(null).status;
        expect(live).toBe('live');
        expect(stale).toBe('stale');
        expect(empty).toBe('loading');
    });
});

// ── L2 — Alignment ──────────────────────────────────────────────────────

describe('buildL2AlignmentHeader (L2)', () => {
    it('renders empty + loading when no alignment', () => {
        expectEmpty(buildL2AlignmentHeader(null));
    });

    it('badge label = prettified mtf_overall_label; Timeframes N/14 chip (v11.12.4)', () => {
        const spec = buildL2AlignmentHeader(alignmentStub({
            mtf_overall_label: 'WEAK_BULL',
            mtf_overall_score: 21.2,
            trend_agreement_pct: 100,
            timeframes_present: 4,
        }));
        expect(spec.badge.label).toBe('WEAK BULL');
        expect(spec.badge.color).toBe(biasColor('WEAK_BULL'));
        expect(spec.meta.find((c) => c.label === 'Score')).toBeUndefined();
        // v11.12.4: Timeframes N/14 is the Alignment header's only chip.
        expect(spec.meta.map((m) => m.label)).toEqual(['Timeframes']);
        expect(spec.meta[0].value).toBe('4/14');
    });

    it('v6.10.19d (A): the Agreement chip is gone — the Timeframe Consensus meter lives in the panel header container', () => {
        const spec = buildL2AlignmentHeader(alignmentStub({ trend_agreement_pct: 0 }));
        expect(spec.meta.find((c) => c.label === 'Agreement')).toBeUndefined();
    });
});

// ── L3 — Analysis ───────────────────────────────────────────────────────

describe('buildL3AnalysisHeader (L3)', () => {
    it('badge reads bias; Quality + Confidence chips; Regime hidden when redundant', () => {
        // bias='BULLISH' ∧ regime='TRENDING_BULL' is one fact, not two.
        const a = analysisStub({ bias: 'Bullish', market_regime: 'TrendingBull', market_quality: 'Good', state_confidence: 0.7 });
        const spec = buildL3AnalysisHeader(a);
        expect(spec.badge.label).toBe('Bullish');
        expect(spec.meta.some((c) => c.label === 'Quality')).toBe(true);
        expect(spec.meta.some((c) => c.label === 'State Confidence')).toBe(true);
        expect(spec.meta.some((c) => c.label === 'Regime')).toBe(false);
    });

    it('Regime chip is present when regime is NOT redundant with bias', () => {
        const a = analysisStub({ bias: 'Bullish', market_regime: 'Accumulation', market_quality: 'Average', state_confidence: 0.4 });
        const spec = buildL3AnalysisHeader(a);
        expect(spec.meta.some((c) => c.label === 'Regime')).toBe(true);
        expect(spec.meta.find((c) => c.label === 'Regime')!.value).toBe('Accumulation');
    });

    it('confidence 0 renders as 0% amber (neutral)', () => {
        const a = analysisStub({ bias: 'Neutral', state_confidence: 0 });
        const spec = buildL3AnalysisHeader(a);
        const c = spec.meta.find((m) => m.label === 'State Confidence')!;
        expect(c.state).toBe('neutral');
        expect(c.value).toBe('0%');
    });
});

// ── L4 — Opportunity ───────────────────────────────────────────────────

describe('buildL4OpportunityHeader (L4)', () => {
    it('NO CLEAR SETUP is rendered neutral amber (zero opportunity is NOT good)', () => {
        const spec = buildL4OpportunityHeader(opportunityStub({ primary_opportunity: 'NoClearOpportunity', opportunity_score: 0 }));
        expect(spec.badge.label).toBe('NO CLEAR SETUP');
        expect(spec.badge.state).toBe('neutral');
        expect(spec.badge.color).toBe(COLORS.neutral);
    });

    it('badge colour = direction: LONG is green, SHORT is red', () => {
        const long = buildL4OpportunityHeader(opportunityStub({ primary_opportunity: 'Pullback', long_expected_rr_internal: 2.0, short_expected_rr_internal: 1.0 }), 'Bullish');
        const short = buildL4OpportunityHeader(opportunityStub({ primary_opportunity: 'MeanReversion', long_expected_rr_internal: 1.0, short_expected_rr_internal: 2.5 }), 'Bearish');
        expect(long.badge.color).toBe(DASHBOARD_COLORS.bullish);
        expect(short.badge.color).toBe(DASHBOARD_COLORS.bearish);
    });

    it('L3 bias override: even if LONG RR < SHORT RR, a Bullish bias forces LONG direction', () => {
        const spec = buildL4OpportunityHeader(opportunityStub({
            primary_opportunity: 'Breakout',
            long_expected_rr_internal: 1.0,
            short_expected_rr_internal: 2.5,
        }), 'Bullish');
        // The resolved direction rides the badge (green = bullish); the
        // R:R chip itself was removed in v7.3 (TRADE SETUPS owns it).
        expect(spec.badge.color).toBe(DASHBOARD_COLORS.bullish);
    });

    it('FIX-2: Neutral bias renders a NEUTRAL-tone badge (no argmax directionality)', () => {
        // The user's capture: a directionally-neutral panel (Pullback
        // DirectionalNeutral, N/A R:R) must NOT render a bear/bull tone
        // just because one per-side R:R happens to be larger.
        const spec = buildL4OpportunityHeader(opportunityStub({
            primary_opportunity: 'Pullback',
            long_expected_rr_internal: 1.0,
            short_expected_rr_internal: 2.5,
        }), 'Neutral');
        expect(spec.badge.label).toBe('Pullback');
        expect(spec.badge.color).toBe(COLORS.neutral);
        // No R:R chip under a neutral bias (v7.3: chip removed entirely).
        expect(spec.meta.find((m) => m.label === 'Reward-to-Risk Ratio')).toBeUndefined();
    });

    it('v7.3: the chip rail is grouped Score / Confidence / Horizon / Timeframes with no R:R chip', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'Breakout', opportunity_score: 58.87, time_horizon: 'INTRADAY' }),
            'Bullish',
            analysisStub({ timeframes_considered: 4, confidence: 0.26 }),
        );
        // v11.12.4: the Timeframes chip was erased from Opportunities.
        expect(spec.meta.map((m) => m.label)).toEqual([
            'Score',
            'Confidence',
            'Horizon',
        ]);
        expect(spec.meta.find((m) => m.label === 'Reward-to-Risk Ratio')).toBeUndefined();
    });

    it('Score chip renders neutral amber when opportunity_score=0 but a type IS set', () => {
        const spec = buildL4OpportunityHeader(opportunityStub({ primary_opportunity: 'Pullback', opportunity_score: 0 }));
        const score = spec.meta.find((m) => m.label === 'Score')!;
        expect(score.state).toBe('neutral');
        expect(score.value).toBe('0');
    });

    it('environment cluster: the Confidence chip rides the meta rail (Timeframes erased v11.12.4)', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'Pullback', opportunity_score: 58.87, time_horizon: 'INTRADAY' }),
            'Bullish',
            analysisStub({ timeframes_considered: 4, confidence: 0.26 }),
        );
        expect(spec.meta.find((m) => m.label === 'Timeframes')).toBeUndefined();
        const conf = spec.meta.find((m) => m.label === 'Confidence')!;
        expect(conf.value).toBe('26%');
        // The pre-existing bracket chips stay side-by-side in the same rail.
        const score = spec.meta.find((m) => m.label === 'Score')!;
        expect(score.value).toBe('58.87');
        const horizon = spec.meta.find((m) => m.label === 'Horizon')!;
        expect(horizon.value).toBe('INTRADAY');
    });

    it('environment cluster renders even in the NO CLEAR SETUP branch (always meaningful)', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'NoClearOpportunity', opportunity_score: 0 }),
            'Neutral',
            analysisStub({ timeframes_considered: 3, confidence: 0.4 }),
        );
        expect(spec.badge.label).toBe('NO CLEAR SETUP');
        expect(spec.meta.find((m) => m.label === 'Timeframes')).toBeUndefined();
        expect(spec.meta.find((m) => m.label === 'Confidence')!.value).toBe('40%');
        // No bracket-derived chips in the no-clear branch.
        expect(spec.meta.find((m) => m.label === 'Score')).toBeUndefined();
        expect(spec.meta.find((m) => m.label === 'Horizon')).toBeUndefined();
    });

    it('environment chips degrade to em-dashes without an analysis payload', () => {
        const spec = buildL4OpportunityHeader(opportunityStub(), 'Bullish', null);
        expect(spec.meta.find((m) => m.label === 'Timeframes')).toBeUndefined();
        const conf = spec.meta.find((m) => m.label === 'Confidence')!;
        expect(conf.value).toBe('—');
    });
});

// ── L5 — Risk ───────────────────────────────────────────────────────────

describe('buildL5RiskHeader (L5)', () => {
    it('badge label = prettified overall.level, sublabel = state', () => {
        const r = riskMatrixStub(riskStub({ score: 42, level: 'Moderate', state: 'Increasing' }));
        const spec = buildL5RiskHeader(r);
        expect(spec.badge.label).toBe('Moderate');
        expect(spec.badge.sublabel).toBe('Increasing');
    });

    it('risk-score 0 — SCORE chip green (low risk = calm) but HEADLINE badge is BLUE', () => {
        // v7.0-prod: green is reserved for bullish setups; risk has no
        // direction. Low risk gets a BLUE badge (calm / ok). The
        // numeric Score chip keeps its severity palette.
        const r = riskMatrixStub(riskStub({ score: 0, level: 'VeryLow' }));
        const spec = buildL5RiskHeader(r);
        const score = spec.meta.find((m) => m.label === 'Score')!;
        expect(score.state).toBe('valid');
        expect(score.color).toBe('#22c55e');
        expect(spec.badge.color).toBe('#22d3ee');
    });

    it('risk-score 80 (high danger) — HEADLINE badge is AMBER (never red)', () => {
        const r = riskMatrixStub(riskStub({ score: 80, level: 'High' }));
        const spec = buildL5RiskHeader(r);
        // Red is reserved for short/bearish setups; L5 is a magnitude
        // measure — high risk is "indeterminate / be cautious", not
        // "bearish", so amber is the correct semantics.
        expect(spec.badge.color).toBe('#f59e0b');
        expect(spec.badge.color).not.toBe('#ef4444');
        expect(spec.badge.color).not.toBe('#22d3ee');
    });

    it('risk-score boundary: 39 → BLUE, 40 → AMBER (threshold at the Moderate band, v6.10.9)', () => {
        const blue = buildL5RiskHeader(riskMatrixStub(riskStub({ score: 39, level: 'Low' })));
        const amber = buildL5RiskHeader(riskMatrixStub(riskStub({ score: 40, level: 'Moderate' })));
        expect(blue.badge.color).toBe('#22d3ee');
        expect(amber.badge.color).toBe('#f59e0b');
    });

    it('Dimensions chip reports n/8 active', () => {
        const r = riskMatrixStub(riskStub({ score: 30 }));
        const spec = buildL5RiskHeader(r);
        const dims = spec.meta.find((m) => m.label === 'Dimensions')!;
        expect(dims.value).toBe('8/8');
    });

    it('Confidence chip sits between Score and Dimensions, styled as a meta chip', () => {
        const r = riskMatrixStub(riskStub({ score: 42, confidence: 78 }));
        const spec = buildL5RiskHeader(r);
        expect(spec.meta.map((m) => m.label)).toEqual(['Score', 'Confidence', 'Dimensions']);
        const conf = spec.meta[1];
        expect(conf.value).toBe('78%');
        expect(conf.state).toBe('valid');
        // v11.12.3: confidence VALUES are white (values white, labels grey).
        expect(conf.color).toBe('rgba(255,255,255,0.85)');
    });
});

// ── L6 — Decision (HIGH-priority regression guard) ──────────────────────

describe('buildL6DecisionHeader (L6) — must NOT consume L3 bias', () => {
    it('LONG verdict renders LONG badge in green', () => {
        const decision = decisionCtxStub({ score: 75, bias: 'Bullish', trade_readiness: 'READY', expected_reward_risk_ratio: 2.0 });
        const advisory = advisoryStub({ directional_guidance: 'Long', market_stance: 'Constructive' });
        const rank = { top: 'LONG' as const, headline: { state: 'READY' as const, confidence_pct: 80 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.badge.label).toBe('LONG');
        expect(spec.badge.color).toBe(DASHBOARD_COLORS.bullish);
    });

    it('directional verdict gated by STAND_ASIDE keeps its direction (v6.10.17)', () => {
        // v6.10.17 decoupling: a LONG verdict gated by STAND ASIDE shows
        // the direction as the badge (with the readiness as sublabel) —
        // only a HOLD top under STAND_ASIDE collapses to "STAND ASIDE".
        const decision = decisionCtxStub({ trade_readiness: 'STAND_ASIDE', score: 0 });
        const advisory = advisoryStub({ directional_guidance: 'Long' });
        const rank = { top: 'LONG' as const, headline: { state: 'STAND_ASIDE' as const, confidence_pct: 0 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.badge.label).toBe('LONG');
        expect(spec.badge.color).toBe(DASHBOARD_COLORS.bullish);
        expect(spec.badge.sublabel).toBe('STAND ASIDE');
    });

    it('flat HOLD under the STAND_ASIDE gate renders STAND ASIDE in amber', () => {
        const decision = decisionCtxStub({ trade_readiness: 'STAND_ASIDE', score: 0, bias: 'Neutral' });
        const advisory = advisoryStub({ directional_guidance: 'Neutral' });
        const rank = { top: 'HOLD' as const, headline: { state: 'STAND_ASIDE' as const, confidence_pct: 0 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.badge.label).toBe('STAND ASIDE');
        expect(spec.badge.color).toBe(COLORS.neutral);
    });

    it('L3 AnalysisPanel.bias is NOT consumed (regression: L3 leak fix)', () => {
        // Seed rank resolves to HOLD. Even if analysis.bias = Bullish,
        // the L6 badge never reads the L3 input.
        const decision = decisionCtxStub({ score: 0, bias: 'Neutral', trade_readiness: 'FORMING', expected_reward_risk_ratio: 0 });
        const advisory = advisoryStub({ directional_guidance: 'Neutral', market_stance: 'Neutral' });
        const rank = { top: 'HOLD' as const, headline: { state: 'FORMING' as const, confidence_pct: 0 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.badge.label).toBe('HOLD');
        // sublabel = readiness (not L3 bias)
        expect(spec.badge.sublabel).toBe('FORMING');
        // The header definition does not take an analysis arg at all —
        // this is structural proof that L3 cannot leak.
        expect(spec.badge.color).toBe(COLORS.neutral);
    });

    it('v6.10.19d (D) + v6.10.28: Risk-Adj R:R and Confidence header chips are removed — the Safety Flags KPI row owns both', () => {
        // The N/A sentinel case: no chip at all now.
        const decisionNA = decisionCtxStub({ score: 0, bias: 'Neutral', trade_readiness: 'WATCH', expected_reward_risk_ratio: 0 });
        const advisoryNA = advisoryStub({ directional_guidance: 'Neutral' });
        const rankNA = { top: 'HOLD' as const, headline: { state: 'WATCH' as const, confidence_pct: 0 } };
        const specNA = buildL6DecisionHeader({ rank: rankNA, decisionContext: decisionNA, advisory: advisoryNA });
        expect(specNA.meta.find((m) => m.label === 'Risk-Adj R:R')).toBeUndefined();
        expect(specNA.meta.find((m) => m.label === 'Confidence')).toBeUndefined();

        // The non-zero case: also gone from the header — the Safety
        // Flags KPI is the single Risk-Adj R:R surface.
        const decision = decisionCtxStub({ score: 0, bias: 'Neutral', trade_readiness: 'WATCH', expected_reward_risk_ratio: 1.2 });
        const advisory = advisoryStub({ directional_guidance: 'Neutral' });
        const rank = { top: 'HOLD' as const, headline: { state: 'WATCH' as const, confidence_pct: 0 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.meta.find((m) => m.label === 'Risk-Adj R:R')).toBeUndefined();
        expect(spec.meta.find((m) => m.label === 'Confidence')).toBeUndefined();
    });
});

// ── L7 — Overview ──────────────────────────────────────────────────────

describe('buildL7OverviewHeader (L7)', () => {
    const fetchState = { lastSuccessMs: 1_000_000, lastErrorMs: null, now: 1_000_500, pollIntervalMs: 3000 };

    it('badge reads global_market_bias + sublabel market_health', () => {
        const spec = buildL7OverviewHeader(overviewStub({ market_health: 'STRONG' }), fetchState);
        expect(spec.badge.label).toBe('BULLISH');
        expect(spec.badge.sublabel).toBe('STRONG');
    });

    it('status = live when lastSuccess is fresh (age < 2 × pollInterval)', () => {
        const spec = buildL7OverviewHeader(overviewStub(), { ...fetchState, now: 1_000_500 });
        expect(spec.status).toBe('live');
    });

    it('status = stale when lastSuccess is older than 2 × pollInterval', () => {
        const spec = buildL7OverviewHeader(overviewStub(), { ...fetchState, now: 1_010_000 });
        expect(spec.status).toBe('stale');
    });

    it('status = error when the LATEST attempt failed (no successful retry since)', () => {
        const spec = buildL7OverviewHeader(overviewStub(), {
            lastSuccessMs: 1_000_000,
            lastErrorMs: 1_005_000,
            now: 1_006_000,
            pollIntervalMs: 3000,
        });
        expect(spec.status).toBe('error');
    });

    it('status = loading when no fetch has ever completed', () => {
        const spec = buildL7OverviewHeader(null, { lastSuccessMs: null, lastErrorMs: null, now: 1_000_000, pollIntervalMs: 3000 });
        expect(spec.status).toBe('loading');
    });

    it('systemic_risk_score 0 renders GREEN (zero risk = good)', () => {
        const spec = buildL7OverviewHeader(overviewStub({ systemic_risk_score: 0 }), fetchState);
        const sys = spec.meta.find((m) => m.label === 'Sys Risk')!;
        expect(sys.state).toBe('valid');
        expect(sys.color).toBe('#22c55e');
    });
});

// ── Lint-y invariants across all seven builders ────────────────────────

describe('cross-layer invariants', () => {
    it('every builder returns a LayerHeaderSpec with one primary badge and zero secondary badges', () => {
        const specs: LayerHeaderSpec[] = [
            buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'TRENDING', overall_score: 60 }))),
            buildL1MtfHeader(alignmentStub({ timeframes_present: 4 }), 'Synchronized'),
            buildL2AlignmentHeader(alignmentStub({ mtf_overall_label: 'WEAK_BULL', mtf_overall_score: 10 })),
            buildL3AnalysisHeader(analysisStub({ bias: 'Bullish', market_regime: 'TrendingBull' })),
            buildL4OpportunityHeader(opportunityStub({ primary_opportunity: 'Pullback' }), 'Bullish'),
            buildL5RiskHeader(riskMatrixStub(riskStub({ score: 40, level: 'Moderate' }))),
            buildL7OverviewHeader(overviewStub({ global_market_bias: 'BULLISH' }), { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 }),
        ];
        for (const s of specs) {
            expect(s.badge).toBeDefined();
            expect(typeof s.badge.label).toBe('string');
            expect(['valid', 'neutral', 'empty', 'error']).toContain(s.badge.state);
        }
    });

    it('layerNumber is monotonic 1..7', () => {
        const l1 = buildL1MetricsHeader(tfStub(ctx())).layerNumber;
        const l2 = buildL2AlignmentHeader(alignmentStub()).layerNumber;
        const l3 = buildL3AnalysisHeader(analysisStub()).layerNumber;
        const l4 = buildL4OpportunityHeader(opportunityStub()).layerNumber;
        const l5 = buildL5RiskHeader(riskMatrixStub(riskStub())).layerNumber;
        const l7 = buildL7OverviewHeader(overviewStub(), { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 }).layerNumber;
        expect([l1, l2, l3, l4, l5, l7]).toEqual([1, 2, 3, 4, 5, 7]);
    });
});

// ── v7.0-prod direction-vocabulary invariant ───────────────────────────
//
// Every LayerHeader builder's headline badge must fall into the 4-colour
// direction vocabulary: green (long) / red (short) / amber (sideways)
// / gray (no-data).  No other colour is allowed on a tab chrome.

describe('v7.0-prod — direction-vocabulary colour invariant (L1..L7)', () => {
    const ALLOWED = new Set([
        '#22c55e',           // green · long / StrongBullish
        '#4ade80',           // green · bullish (Lighter Bullish)
        '#ef4444',           // red   · StrongBearish
        '#dc2626',           // red   · StrongBearish (darker)
        '#f87171',           // red   · bearish / SHORT (default bearish token)
        '#f59e0b',           // amber · sideways / neutral / hold / wait
        '#22d3ee',           // blue  · L5 low-risk (calm / ok — v7.0-prod)
        'rgba(255,255,255,0.30)', // gray · nodata (foreground)
    ]);

    function assertVocabulary(spec: { badge: { color?: string } }) {
        // Empty badges fall back to gray via emptyBadge() — also allowed.
        const c = spec.badge.color ?? 'rgba(255,255,255,0.30)';
        expect(
            ALLOWED.has(c),
            `headline colour ${c} must be one of ${[...ALLOWED].join(', ')}`,
        ).toBe(true);
    }

    it('L1 (per-TF bullish → green)', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'STRONG_BULL', regime: 'TRENDING', overall_score: 80 })));
        assertVocabulary(spec);
    });

    it('L1 (per-TF bearish → red)', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'BEARISH', regime: 'TRENDING', overall_score: 80 })));
        assertVocabulary(spec);
    });

    it('L1 (per-TF neutral → amber)', () => {
        const spec = buildL1MetricsHeader(tfStub(ctx({ overall_label: 'NEUTRAL', regime: 'RANGE', overall_score: 50 })));
        assertVocabulary(spec);
    });

    it('L1 (MTF bullish → green)', () => {
        const spec = buildL1MtfHeader(alignmentStub({ mtf_overall_label: 'WEAK_BULL', timeframes_present: 4 }));
        assertVocabulary(spec);
    });

    it('L2 (bullish → green)', () => {
        const spec = buildL2AlignmentHeader(alignmentStub({ mtf_overall_label: 'WEAK_BULL', mtf_overall_score: 12 }));
        assertVocabulary(spec);
    });

    it('L3 (bullish → green)', () => {
        const spec = buildL3AnalysisHeader(analysisStub({ bias: 'Bullish', market_regime: 'TrendingBull' }));
        assertVocabulary(spec);
    });

    it('L4 (LONG → green)', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'Pullback', long_expected_rr_internal: 2.0, short_expected_rr_internal: 1.0 }),
            'Bullish',
        );
        assertVocabulary(spec);
    });

    it('L4 (SHORT → red)', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'MeanReversion', long_expected_rr_internal: 1.0, short_expected_rr_internal: 2.5 }),
            'Bearish',
        );
        assertVocabulary(spec);
    });

    it('L4 (NO CLEAR SETUP → amber)', () => {
        const spec = buildL4OpportunityHeader(
            opportunityStub({ primary_opportunity: 'NoClearOpportunity', opportunity_score: 0 }),
            'Neutral',
        );
        assertVocabulary(spec);
    });

    it('L5 (score < 50 → blue, score >= 50 → amber)', () => {
        const low = buildL5RiskHeader(riskMatrixStub(riskStub({ score: 30, level: 'Low' })));
        assertVocabulary(low);
        expect(low.badge.color).toBe('#22d3ee');
        const high = buildL5RiskHeader(riskMatrixStub(riskStub({ score: 70, level: 'High' })));
        assertVocabulary(high);
        expect(high.badge.color).toBe('#f59e0b');
    });

    it('L6 (LONG → green)', () => {
        const decision = decisionCtxStub({ score: 75, bias: 'Bullish', trade_readiness: 'READY', expected_reward_risk_ratio: 2.0 });
        const advisory = advisoryStub({ directional_guidance: 'Long', market_stance: 'Constructive' });
        const rank = { top: 'LONG' as const, headline: { state: 'READY' as const, confidence_pct: 80 } };
        assertVocabulary(buildL6DecisionHeader({ rank, decisionContext: decision, advisory }));
    });

    it('L6 (SHORT → red)', () => {
        const decision = decisionCtxStub({ score: -75, bias: 'Bearish', trade_readiness: 'READY', expected_reward_risk_ratio: 2.0 });
        const advisory = advisoryStub({ directional_guidance: 'Short', market_stance: 'Cautious' });
        const rank = { top: 'SHORT' as const, headline: { state: 'READY' as const, confidence_pct: 80 } };
        assertVocabulary(buildL6DecisionHeader({ rank, decisionContext: decision, advisory }));
    });

    it('L6 (directional-gated LONG → green; flat HOLD/STAND_ASIDE → amber)', () => {
        // v6.10.17: a directional verdict gated by STAND ASIDE keeps its
        // direction colour — red/green, never amber. Amber is reserved for
        // the genuinely flat no-directional-call state.
        const decision = decisionCtxStub({ trade_readiness: 'STAND_ASIDE', score: 0 });
        const advisory = advisoryStub({ directional_guidance: 'Long', market_stance: 'Neutral' });
        const rank = { top: 'LONG' as const, headline: { state: 'STAND_ASIDE' as const, confidence_pct: 0 } };
        const spec = buildL6DecisionHeader({ rank, decisionContext: decision, advisory });
        expect(spec.badge.color).toBe(DASHBOARD_COLORS.bullish);
        assertVocabulary(spec);

        const flatDecision = decisionCtxStub({ trade_readiness: 'STAND_ASIDE', score: 0, bias: 'Neutral' });
        const flatRank = { top: 'HOLD' as const, headline: { state: 'STAND_ASIDE' as const, confidence_pct: 0 } };
        const flatSpec = buildL6DecisionHeader({ rank: flatRank, decisionContext: flatDecision, advisory });
        expect(flatSpec.badge.color).toBe('#f59e0b');
        assertVocabulary(flatSpec);
    });

    it('L7 (low coverage → STRONG demoted + pair count) (v6.10.18 I-10)', () => {
        // A single-pair market must not headline "STRONG BULLISH 100%
        // breadth" — the display token demotes one tier and the sublabel
        // carries the pair count ("BULLISH (1 pair)"). Wire value intact.
        const spec = buildL7OverviewHeader(
            overviewStub({ global_market_bias: 'STRONG_BULLISH', market_health: 'POOR', instance_count: 1, low_coverage: true }),
            { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 },
        );
        expect(spec.badge.label).toBe('BULLISH');
        expect(spec.badge.sublabel).toContain('(1 pair)');
        // A 5-pair synchronized market keeps the STRONG token.
        const strong = buildL7OverviewHeader(
            overviewStub({ global_market_bias: 'STRONG_BULLISH', market_health: 'HEALTHY', instance_count: 5 }),
            { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 },
        );
        expect(strong.badge.label).toBe('STRONG BULLISH');
        expect(strong.badge.sublabel).toBe('HEALTHY');
    });

    it('L7 (bullish → green, bearish → red)', () => {
        const bull = buildL7OverviewHeader(
            overviewStub({ global_market_bias: 'BULLISH' }),
            { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 },
        );
        expect(bull.badge.color).toBe('#4ade80');
        const bear = buildL7OverviewHeader(
            overviewStub({ global_market_bias: 'BEARISH' }),
            { lastSuccessMs: 1, lastErrorMs: null, now: 2, pollIntervalMs: 3000 },
        );
        expect(bear.badge.color).toBe('#f87171');
        assertVocabulary(bull);
        assertVocabulary(bear);
    });
});
