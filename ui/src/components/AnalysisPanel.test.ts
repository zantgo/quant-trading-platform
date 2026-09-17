// @vitest-environment jsdom
//
// AnalysisPanel — signal lean / signal-square consistency (v6.10.8):
//   AN-1: neutral signals render with the neutral (gray) square + flat
//         dash icon — never the bearish red styling and down arrow.
//   AN-2: the lean hero distinguishes "no data yet" (empty lists) from
//         "all timeframes neutral" (signals exist, no directional lean).
//   AN-3: a zero-opposing count renders "3:0", never "3:1".

import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { tick } from 'svelte';
import AnalysisPanel from './AnalysisPanel.svelte';
import { useAppStore } from '../state.svelte';
import type { AnalysisMatrix, AlignmentMatrix, IndicatorMap } from '../types';

function makeAnalysis(overrides: Partial<AnalysisMatrix> = {}): AnalysisMatrix {
    return {
        symbol: 'BTC-USDT',
        bias: 'Bullish',
        market_bias_score: 0.4,
        state_confidence: 0.6,
        confidence: 0.6,
        market_regime: 'TrendingBull',
        trend_assessment: 'Healthy',
        momentum_assessment: 'Stable',
        structure_assessment: 'Healthy',
        volatility_assessment: 'Normal',
        volume_assessment: 'Normal',
        market_quality: 'Good',
        market_quality_score: 70,
        market_phase: 'Markup',
        market_interpretation: 'Bullish market.',
        rationale: 'MTF overall score 30/100 → Bullish.',
        supporting_signals: ['MICRO (bullish): score +5, TRENDING regime, 3 signals'],
        contradicting_signals: [],
        timeframes_considered: 4,
        ...overrides,
    } as AnalysisMatrix;
}

function makeAlignment(overrides: Partial<AlignmentMatrix> = {}): AlignmentMatrix {
    return {
        symbol: 'BTC-USDT',
        timeframes_present: 4,
        dimensions: [],
        mtf_trend_alignment: 0,
        mtf_momentum_alignment: 0,
        mtf_volume_alignment: 0,
        mtf_volatility_alignment: 0,
        mtf_overall_score: 25,
        mtf_overall_label: 'BULLISH',
        timeframe_alignments: [],
        signal_cross_tf_count: 34,
        trend_agreement_pct: 100,
        ...overrides,
    } as AlignmentMatrix;
}

function seed(analysis: AnalysisMatrix) {
    const app = useAppStore();
    app.activeTab = 'BTC-USDT';
    if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
    app.instancesMap['BTC-USDT'].analysis = analysis;
    return app;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
    cleanup();
});

describe('AnalysisPanel — signal lean (AN-1/2/3)', () => {
    it('renders a neutral signal with the neutral gray square + flat icon (AN-1)', () => {
        seed(makeAnalysis({
            supporting_signals: ['MICRO (neutral): score +0, RANGE regime, 0 signals'],
            contradicting_signals: [],
        }));
        render(AnalysisPanel, { props: {} });
        const square = screen.getByTitle('MICRO (neutral): score +0, RANGE regime, 0 signals');
        const svg = square.querySelector('svg');
        // Flat gray dash (stroke carried by the <svg> element) — NOT the
        // red down-arrow (line + polyline, red stroke).
        expect(svg?.getAttribute('stroke')).toBe('#94a3b8');
        expect(svg?.querySelectorAll('polyline').length).toBe(0);
    });

    it('still renders bull/bear squares with arrows for directional signals (AN-1)', () => {
        seed(makeAnalysis({
            supporting_signals: ['MICRO (bullish): score +5, TRENDING regime, 3 signals'],
            contradicting_signals: ['FAST (bearish): score -3, RANGING regime, 1 signal'],
        }));
        render(AnalysisPanel, { props: {} });
        const bull = screen.getByTitle('MICRO (bullish): score +5, TRENDING regime, 3 signals');
        expect(bull.querySelector('svg')?.getAttribute('stroke')).toBe('#22c55e');
        expect(bull.querySelector('svg')?.querySelector('polyline')).toBeTruthy();
        const bear = screen.getByTitle('FAST (bearish): score -3, RANGING regime, 1 signal');
        expect(bear.querySelector('svg')?.getAttribute('stroke')).toBe('#ef4444');
        expect(bear.querySelector('svg')?.querySelector('polyline')).toBeTruthy();
    });

    it('all-neutral signals render an honest neutral hero, not "No signals" (AN-2)', () => {
        // v6.10.18 (I-7): neutral TFs (score 0, |score| ≤ 10) do not vote
        // — the hero reads "Neutral signals" (no lean) exactly as before.
        seed(makeAnalysis({
            supporting_signals: ['MICRO (neutral): score +0, RANGE regime, 0 signals'],
            contradicting_signals: ['FAST (neutral): score +0, RANGING regime, 0 signals'],
        }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('Neutral signals')).toBeTruthy();
        expect(screen.getByText('No directional lean across timeframes')).toBeTruthy();
        expect(screen.queryByText('No signals')).toBeNull();
    });

    it('empty signal lists keep the pre-warmup placeholder (AN-2)', () => {
        seed(makeAnalysis({ supporting_signals: [], contradicting_signals: [] }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('No signals')).toBeTruthy();
        expect(screen.getByText('Waiting for cross-TF consensus')).toBeTruthy();
    });

    it('zero opposing signals render "3:0", never "3:1" (AN-3)', () => {
        // v6.10.18 (I-7): the hero vote uses decisive scores (|score| > 10).
        seed(makeAnalysis({
            supporting_signals: [
                'MICRO (bullish): score +35, TRENDING regime, 3 signals',
                'FAST (bullish): score +25, TRENDING regime, 2 signals',
                'SLOW (bullish): score +15, TRENDING regime, 1 signal',
            ],
            contradicting_signals: [],
        }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('3:0 signal ratio')).toBeTruthy();
        expect(screen.queryByText(/3:1/)).toBeNull();
    });

    it('renders the doc-example 2.0:1 ratio when both sides have counts (AN-3)', () => {
        seed(makeAnalysis({
            supporting_signals: [
                'MICRO (bullish): score +35, TRENDING regime, 3 signals',
                'FAST (bullish): score +25, TRENDING regime, 2 signals',
            ],
            contradicting_signals: ['SLOW (bearish): score -30, RANGING regime, 1 signal'],
        }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('2.0:1 signal ratio')).toBeTruthy();
    });

    it('v6.14: Trend card no longer renders a Trend Stability Sharpe badge (v6.11 field removed from the L3 matrix)', () => {
        // The L1→L3 traceability-evidence exception was stripped: the
        // AnalysisMatrix no longer carries the field, so the Trend card
        // must never render the old badge — even if a stale snapshot
        // smuggles the key through.
        seed(makeAnalysis({ trend_stability_sharpe: 3.85 } as Partial<AnalysisMatrix>));
        render(AnalysisPanel, { props: {} });
        expect(screen.queryByText('3.85')).toBeNull();
        expect(screen.queryByText(/Trend Stability/)).toBeNull();
    });

    it('v6.12: all five cards render their 0-100 dimension-score badges (v6.13: rounded %)', () => {
        seed(makeAnalysis({
            trend_score: 62.35,
            momentum_score: 48.72,
            structure_score: 71.4,
            volatility_score: 78.15,
            volume_score: 82.6,
        }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('62%')).toBeTruthy();
        expect(screen.getByText('49%')).toBeTruthy();
        expect(screen.getByText('71%')).toBeTruthy();
        expect(screen.getByText('78%')).toBeTruthy();
        expect(screen.getByText('83%')).toBeTruthy();
    });

    it('v6.12: score badges are tinted by band heat (≥70 strong, ≥40 mid, <40 weak)', () => {
        seed(makeAnalysis({
            trend_score: 82.0,
            momentum_score: 55.0,
            structure_score: 25.0,
            volatility_score: 50.0,
            volume_score: 75.0,
        }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('82%').className).toContain('scoreStrong');
        expect(screen.getByText('55%').className).toContain('scoreMid');
        expect(screen.getByText('25%').className).toContain('scoreWeak');
    });

    it('v6.12: Trend card renders its dimension-score badge with tooltip', () => {
        seed(makeAnalysis({ trend_score: 62.35 }));
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('62%')).toBeTruthy();
        expect(screen.getByText('62%').getAttribute('title')).toContain('agreement across timeframes');
    });

    it('v6.12: ▲ delta arrow appears when a score rises vs the previous frame', async () => {
        const app = seed(makeAnalysis({ trend_score: 60.0 }));
        render(AnalysisPanel, { props: {} });
        await tick();
        app.instancesMap['BTC-USDT'].analysis = makeAnalysis({ trend_score: 63.5 });
        await tick();
        await tick();
        const badge = screen.getByText(/64%/);
        expect(badge.textContent).toContain('▲');
        // Unchanged score → no arrow.
        app.instancesMap['BTC-USDT'].analysis = makeAnalysis({ trend_score: 63.5 });
        await tick();
        await tick();
        expect(screen.getByText(/64%/).textContent).not.toContain('▲');
    });

    it('v6.12: ▼ delta arrow appears when a score falls vs the previous frame', async () => {
        const app = seed(makeAnalysis({ momentum_score: 70.0 }));
        render(AnalysisPanel, { props: {} });
        await tick();
        app.instancesMap['BTC-USDT'].analysis = makeAnalysis({ momentum_score: 65.0 });
        await tick();
        await tick();
        const badge = screen.getByText(/65%/);
        expect(badge.textContent).toContain('▼');
    });

    it('v6.14: export qualitative_assessment no longer carries the trend-stability Sharpe', async () => {
        const app = seed(makeAnalysis({ trend_stability_sharpe: 3.85 } as Partial<AnalysisMatrix>));
        const tf = app.instancesMap['BTC-USDT'].terms[1];
        tf.indicators = {
            bbwp: { raw_value: 94.8, normalized: 0.948, state_label: 'NEUTRAL', values: null },
            adx: { raw_value: 40.06, normalized: 0.6, state_label: 'NEUTRAL', values: null },
        } as unknown as IndicatorMap;
        const writes: string[] = [];
        Object.defineProperty(navigator, 'clipboard', {
            value: { writeText: async (t: string) => { writes.push(t); return true; } },
            writable: true,
            configurable: true,
        });
        render(AnalysisPanel, { props: {} });
        const exportBtn = screen.getByTitle('Copy all Analysis data as JSON');
        await fireEvent.click(exportBtn);
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        const payload = JSON.parse(writes[0]);
        expect(payload.qualitative_assessment.trend_stability_sharpe).toBeUndefined();
        expect(payload.qualitative_assessment.trend_stability_sharpe_display).toBeUndefined();
    });

    it('v6.10.19a (D1): export traceability carries the representative BBWP/ADX raw values', async () => {
        // Regression: the panel read the transient snapshot map with the
        // wrong key (`raw` vs the wire `raw_value`) — the representative
        // fields were null on every live export. The canonical source is
        // the term-level indicator map, exactly like the Metrics tab.
        const app = seed(makeAnalysis());
        const tf = app.instancesMap['BTC-USDT'].terms[1];
        tf.indicators = {
            bbwp: { raw_value: 94.8, normalized: 0.948, state_label: 'NEUTRAL', values: null },
            adx: { raw_value: 40.06, normalized: 0.6, state_label: 'NEUTRAL', values: null },
        } as unknown as IndicatorMap;

        const writes: string[] = [];
        Object.defineProperty(navigator, 'clipboard', {
            value: { writeText: async (t: string) => { writes.push(t); return true; } },
            writable: true,
            configurable: true,
        });
        render(AnalysisPanel, { props: {} });
        const exportBtn = screen.getByTitle('Copy all Analysis data as JSON');
        await fireEvent.click(exportBtn);
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        expect(writes.length).toBe(1);
        const payload = JSON.parse(writes[0]);
        expect(payload.representative_bbwp).toBeCloseTo(94.8, 1);
        expect(payload.representative_adx).toBeCloseTo(40.06, 1);
    });

    it('v6.10.21: matrix-pinned representative BBWP/ADX win over the micro-map fallback', async () => {
        // The analysis mirror is per-slot last-writer-wins — the matrix's
        // own pinned representative fields are the exact inputs the
        // rationale quotes; the micro map must NOT override them.
        const app = seed(makeAnalysis({
            representative_bbwp: 53.0,
            representative_adx: 35.3,
        }));
        const tf = app.instancesMap['BTC-USDT'].terms[1];
        tf.indicators = {
            bbwp: { raw_value: 11.6, normalized: 0.116, state_label: 'NEUTRAL', values: null },
            adx: { raw_value: 27.48, normalized: 0.27, state_label: 'NEUTRAL', values: null },
        } as unknown as IndicatorMap;

        const writes: string[] = [];
        Object.defineProperty(navigator, 'clipboard', {
            value: { writeText: async (t: string) => { writes.push(t); return true; } },
            writable: true,
            configurable: true,
        });
        render(AnalysisPanel, { props: {} });
        const exportBtn = screen.getByTitle('Copy all Analysis data as JSON');
        await fireEvent.click(exportBtn);
        await tick();
        await new Promise((r) => setTimeout(r, 0));
        const payload = JSON.parse(writes[0]);
        expect(payload.representative_bbwp).toBeCloseTo(53.0, 1);
        expect(payload.representative_adx).toBeCloseTo(35.3, 1);
    });
});

// v6.15: unified Interpretation & Rationale card — the raw backend
// rationale line is gone; the evidence renders as a 5-column grid fed by
// the alignment matrix (score / agreement / signals) and the analysis
// matrix's pinned representative inputs (BBWP / ADX).
describe('AnalysisPanel — interpretation & rationale grid (v6.15)', () => {
    it('v7.4: renders the grid inside the KEY METRICS card BELOW Qualitative Assessment', () => {
        seed(makeAnalysis({ bias: 'Bullish', representative_bbwp: 9.2, representative_adx: 36.5 }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        const card = screen.getByLabelText('KEY METRICS');
        expect(card).toBeTruthy();
        expect(card.querySelector('[class*="rationaleGrid"]')).toBeTruthy();
        // The prose keeps its own SUMMARY card, ABOVE KEY METRICS.
        const prose = screen.getByLabelText('SUMMARY');
        expect(prose.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
        // v7.4: the card moved below the Qualitative Assessment section —
        // it must no longer precede the Signal Lean hero.
        const hero = document.querySelector('[class*="signalLeanHero"]')!;
        expect(hero.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
        const qual = Array.from(document.querySelectorAll('[class*="sectionTitle"]'))
            .find((el) => el.textContent === 'Qualitative Assessment')!;
        expect(qual.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('renders the unified card with all 5 rationale columns', () => {
        seed(makeAnalysis({
            bias: 'Bullish',
            representative_bbwp: 9.2,
            representative_adx: 36.5,
        }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('Overall Score')).toBeTruthy();
        expect(screen.getByText('Timeframe Agreement')).toBeTruthy();
        expect(screen.getByText('Volatility Percentile')).toBeTruthy();
        expect(screen.getByText('Trend Strength')).toBeTruthy();
        expect(screen.getByText('Total Signals')).toBeTruthy();
        expect(screen.getByText('+25')).toBeTruthy();
        expect(screen.getByText('(Bullish)')).toBeTruthy();
        expect(screen.getByText('100%')).toBeTruthy();
        expect(screen.getByText('4 timeframes aligned')).toBeTruthy();
        expect(screen.getByText('9.2%')).toBeTruthy();
        expect(screen.getByText('36.5')).toBeTruthy();
        expect(screen.getByText('34 Signals')).toBeTruthy();
    });

    it('colours the bias sub-label green for Bullish', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('(Bullish)').getAttribute('style')).toContain('rgb(74, 222, 128)');
    });

    it('colours the bias sub-label red for Bearish', () => {
        seed(makeAnalysis({ bias: 'Bearish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment({ mtf_overall_score: -30 });
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('-30')).toBeTruthy();
        expect(screen.getByText('(Bearish)').getAttribute('style')).toContain('rgb(248, 113, 113)');
    });

    it('renders the signed score in sign colour and its strength label (bearish)', () => {
        seed(makeAnalysis({ bias: 'Bearish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment({
            mtf_overall_score: -30,
            mtf_overall_label: 'WEAK_BEAR_MTF',
        });
        render(AnalysisPanel, { props: {} });
        const value = screen.getByText('-30');
        // Negative score renders red (rgb(239, 68, 68)).
        expect(value.getAttribute('style')).toContain('rgb(239, 68, 68)');
        // Strength label from the shared mLabel() vocabulary.
        expect(screen.getByText('WEAK BEAR')).toBeTruthy();
    });

    it('renders a positive score green with its strength label', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment({
            mtf_overall_score: 55,
            mtf_overall_label: 'STRONG_BULL_MTF',
        });
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('+55').getAttribute('style')).toContain('rgb(34, 197, 94)');
        expect(screen.getByText('STRONG BULL')).toBeTruthy();
    });

    it('renders the footnote explaining what the overall score means', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText(/Blended multi-timeframe bias score/)).toBeTruthy();
        expect(screen.getByText(/100·\(0\.5·Trend/)).toBeTruthy();
        expect(screen.getByText(/positive = net bullish/)).toBeTruthy();
    });

    it('does not render the raw backend rationale line (v6.15 cleanup)', () => {
        seed(makeAnalysis({ rationale: 'MTF overall score 25/100 → Bullish. BBWP=9.2 ADX=36.5.' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        expect(screen.queryByText(/MTF overall score/)).toBeNull();
        expect(screen.queryByText(/BBWP=/)).toBeNull();
        expect(screen.queryByText(/ADX=/)).toBeNull();
        expect(screen.queryByText(/→/)).toBeNull();
    });

    it('annotates the score cell when the bias is lifted by the grace band', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment({ mtf_overall_score: 12 });
        render(AnalysisPanel, { props: {} });
        const value = screen.getByText('+12');
        expect(value.parentElement?.getAttribute('title')).toContain('Bias lifted');
    });

    it('does not annotate the score cell for an ordinary directional read', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment({ mtf_overall_score: 55 });
        render(AnalysisPanel, { props: {} });
        expect(screen.getByText('+55').parentElement?.getAttribute('title')).toBeNull();
    });

    it('falls back to em-dashes when alignment and representative fields are absent', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        render(AnalysisPanel, { props: {} });
        expect(screen.queryByText('+25')).toBeNull();
        expect(screen.queryByText('4 timeframes aligned')).toBeNull();
        expect(screen.queryByText('34 Signals')).toBeNull();
        expect(screen.getByText((content) => content.includes('market.'))).toBeTruthy();
    });

    it('v7.4: does not render the Per-Timeframe Alignment grid (moved to the Alignment tab)', () => {
        seed(makeAnalysis({ bias: 'Bullish' }));
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].alignment = makeAlignment();
        render(AnalysisPanel, { props: {} });
        expect(screen.queryByText('Per-Timeframe Alignment')).toBeNull();
        // The gauge grid lives only on the Alignment tab now.
        expect(screen.queryByText(/OFFLINE/)).toBeNull();
    });
});
