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
// CR-15: `buildL4OpportunityHeader` / `buildL5RiskHeader` /
// `buildL6DecisionHeader` are gone with the strategy matrices they rendered.
import { buildL1MetricsHeader, buildL1MtfHeader, metricsBadgeFor, buildL2AlignmentHeader, buildL3AnalysisHeader, buildL7OverviewHeader, chip, emptyBadge, hexToRgba, type LayerHeaderSpec } from './layerHeader';
import { DASHBOARD_COLORS, biasColor, riskDangerColor, scoreColor } from './dashboardColors';
import { COLORS } from './scoreStyles';
import type {
    AdvisoryMatrix,
    AlignmentMatrix,
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
        mtf_stance: 'neutral',
        timeframe_alignments: [],
        signal_cross_tf_count: 0,
        trend_agreement_pct: 0,
        ...overrides,
    };
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

function overviewStub(overrides: Partial<OverviewMatrix> = {}): OverviewMatrix {
    return {
        global_market_bias: 'BULLISH',
        market_breadth: 'POSITIVE',
        axis_distribution: {},
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

    it('badge label = prettified mtf_stance; Timeframes N/14 chip (v11.12.4)', () => {
        const spec = buildL2AlignmentHeader(alignmentStub({
            mtf_stance: 'bull',
            mtf_overall_score: 21.2,
            trend_agreement_pct: 100,
            timeframes_present: 4,
        }));
        expect(spec.badge.label).toBe('WEAK BULL');
        expect(spec.badge.color).toBe(biasColor('bull'));
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

// ── L3 — Model state ────────────────────────────────────────────────────

describe('buildL3AnalysisHeader (L3)', () => {
    it('badge reads the cross-duration stance; score + TF count are the meta', () => {
        const spec = buildL3AnalysisHeader(alignmentStub({
            mtf_stance: 'bull',
            mtf_overall_score: 72,
            timeframes_present: 4,
        }));
        expect(spec.layerName).toBe('Model State');
        expect(spec.badge.label).toBe('Bullish');
        expect(spec.meta.find((c) => c.label === 'Cross-TF')!.value).toBe('72');
        expect(spec.meta.find((c) => c.label === 'TFs')!.value).toBe('4');
    });

    it('a neutral stance is a real reading, not a loading state', () => {
        const spec = buildL3AnalysisHeader(alignmentStub({
            mtf_stance: 'neutral',
            mtf_overall_score: 0,
            timeframes_present: 2,
        }));
        expect(spec.status).toBe('live');
        expect(spec.badge.label).toBe('Neutral');
    });

    it('NO_DATA renders the empty badge + loading status', () => {
        const spec = buildL3AnalysisHeader(alignmentStub({ mtf_stance: 'unmeasured', timeframes_present: 0 }));
        expect(spec.status).toBe('loading');
        expect(spec.badge.label).toBe('—');
    });
});

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

});

// ── Lint-y invariants across all seven builders ────────────────────────
// CR-15: the L4/L5/L6 cases are gone from this invariant — those badges
// rendered an opportunity, a risk level and a verdict, and the colour that
// carried a direction was doing the work of a trading signal. What remains is
// the model's own vocabulary: green bullish, red bearish, amber neutral.
describe('v7.0-prod — direction-vocabulary colour invariant (L1..L3, L7)', () => {
    const ALLOWED = new Set([
        '#22c55e',           // green · long / StrongBullish
        '#4ade80',           // green · bullish (Lighter Bullish)
        '#ef4444',           // red   · StrongBearish
        '#dc2626',           // red   · StrongBearish (darker)
        '#f87171',           // red   · bearish / SHORT (default bearish token)
        '#f59e0b',           // amber · sideways / neutral / hold / wait
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
        const spec = buildL1MtfHeader(alignmentStub({ mtf_stance: 'bull', timeframes_present: 4 }));
        assertVocabulary(spec);
    });

    it('L2 (bullish → green)', () => {
        const spec = buildL2AlignmentHeader(alignmentStub({ mtf_stance: 'bull', mtf_overall_score: 12 }));
        assertVocabulary(spec);
    });

    it('L3 (bullish → green)', () => {
        const spec = buildL3AnalysisHeader(alignmentStub({ mtf_stance: 'bull', mtf_overall_score: 72 }));
        assertVocabulary(spec);
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
