// Regression tests for the v7.0-audit Analysis tab export.

import { describe, it, expect } from 'vitest';
import { buildAnalysisTabExport } from './analysisTab';
import type { LayerHeaderSpec } from '../layerHeader';
import type { AnalysisMatrix, AlignmentMatrix } from '../../types';

const headerSpec: LayerHeaderSpec = {
  layerNumber: 3,
  layerName: 'Analysis',
  badge: { label: 'Strong Bullish', color: '#22c55e', background: 'rgba(34,197,94,0.08)', state: 'valid' },
  meta: [],
  status: 'live',
};

function makeAnalysis(overrides: Partial<AnalysisMatrix> = {}): AnalysisMatrix {
  return {
    bias: 'StrongBullish',
    confidence: 0.78,
    state_confidence: 0.78,
    market_regime: 'TrendingBull',
    market_quality: 'Good',
    market_phase: 'Markup',
    timeframes_considered: 10,
    supporting_signals: [
      'MICRO1 (bullish): score +35, TRENDING regime, 3 signals',
      'FAST1 (bullish): score +25, TRENDING regime, 2 signals',
    ],
    contradicting_signals: [
      'SLOW1 (bearish): score -30, RANGE regime, 1 signal',
    ],
    trend_assessment: 'Developing',
    momentum_assessment: 'Increasing',
    structure_assessment: 'Healthy',
    volatility_assessment: 'Expanding',
    volume_assessment: 'Strong',
    market_interpretation: 'Market is in a strong bullish phase.',
    rationale: 'Composite confluence score 78.',
    ...overrides,
  } as unknown as AnalysisMatrix;
}

function makeAlignment(): AlignmentMatrix {
  return {
    mtf_overall_score: 75,
    mtf_overall_label: 'STRONG_BULLISH',
    timeframes_present: 10,
    signal_cross_tf_count: 3,
    trend_agreement_pct: 75,
    mtf_trend_alignment: 0.45,
    mtf_momentum_alignment: 0.3,
    mtf_volume_alignment: 0.1,
    mtf_volatility_alignment: 0.2,
    timeframe_alignments: [
      { timeframe: 'MICRO1', trend_score: 0.42, momentum_score: 0.3, overall_score: 1.0, regime: 'TRENDING', active_signals: 5 },
      { timeframe: 'FAST1', trend_score: 0.5, momentum_score: 0.2, overall_score: 0.7, regime: 'EXPANSION', active_signals: 3 },
    ],
  } as unknown as AlignmentMatrix;
}

describe('buildAnalysisTabExport', () => {
  it('header chrome is present; body bias is display-formatted', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.header.layer_name).toBe('Analysis');
    expect(p.body.bias).toBe('Strong Bullish');
    expect(p.body.confidence_pct).toBe(78);
  });

  it('key_metrics mirrors the panel KEY METRICS row (audit C2 parity)', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.key_metrics).toEqual({
      mtf_overall_score: 75,
      trend_agreement_pct: 75,
      timeframes_present: 10,
      signal_cross_tf_count: 3,
    });
    // Analysis fallback for the timeframe count when alignment is absent.
    const noAlignment = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: null,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(noAlignment.key_metrics).toEqual({
      mtf_overall_score: null,
      trend_agreement_pct: null,
      timeframes_present: 10,
      signal_cross_tf_count: null,
    });
  });

  it('signal_lean_hero is emitted with raw percentage numbers', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero).not.toBeNull();
    expect(p.signal_lean_hero.bullish_pct).toBe(67);
    expect(p.signal_lean_hero.bearish_pct).toBe(33);
    expect(p.signal_lean_hero.tone).toBe('bull');
  });

  it('signals decompose into key/period/score_display', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const supporting = p.signals.supporting;
    expect(supporting.length).toBe(2);
    expect(supporting[0].score).toBe(35);
    expect(supporting[0].score_display).toBe('+35');
    expect(supporting[0].timeframe).toBe('MICRO1');
    expect(supporting[0].regime).toBe('TRENDING');
    expect(supporting[0].signals_count).toBe(3);
  });

  it('per-TF alignment uses OFFLINE for missing TFs and sign-prefixed displays', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const tfs = p.per_timeframe_alignment;
    expect(tfs).toHaveLength(10);
    const slow = tfs.find((t: { name: string }) => t.name === 'SLOW1');
    expect(slow.active).toBe(false);
    expect(slow.regime).toBe('OFFLINE');
    const micro = tfs.find((t: { name: string }) => t.name === 'MICRO1');
    expect(micro.trend_display).toBe('+0.42');
    expect(micro.overall_display).toBe('+1.0');
  });

  it('qualitative cycle_phase is display-formatted', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.qualitative_assessment.cycle_phase).toBe('MARKUP');
  });

  it('v6.14: qualitative_assessment no longer carries the trend-stability Sharpe', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: makeAnalysis(),
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.qualitative_assessment.trend_stability_sharpe).toBeUndefined();
    expect(p.qualitative_assessment.trend_stability_sharpe_display).toBeUndefined();
  });

  it('v6.12: qualitative_assessment carries the per-card dimension scores', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: {
        ...makeAnalysis(),
        trend_score: 62.35,
        momentum_score: 48.72,
        structure_score: 71.4,
        volatility_score: 78.15,
        volume_score: 82.6,
      },
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.qualitative_assessment.trend_score).toBeCloseTo(62.35, 2);
    expect(p.qualitative_assessment.trend_score_display).toBe('62%');
    expect(p.qualitative_assessment.momentum_score).toBeCloseTo(48.72, 2);
    expect(p.qualitative_assessment.momentum_score_display).toBe('49%');
    expect(p.qualitative_assessment.structure_score_display).toBe('71%');
    expect(p.qualitative_assessment.volatility_score_display).toBe('78%');
    expect(p.qualitative_assessment.volume_score_display).toBe('83%');
  });

  it('v6.12: absent dimension scores render em-dash displays', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: { ...makeAnalysis(), trend_score: null, momentum_score: null },
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    for (const key of ['trend_score', 'momentum_score', 'structure_score', 'volatility_score', 'volume_score']) {
      expect(p.qualitative_assessment[key]).toBeNull();
    }
    expect(p.qualitative_assessment.trend_score_display).toBe('\u2014');
    expect(p.qualitative_assessment.volume_score_display).toBe('\u2014');
  });

  it('split-tone hero label matches the screen ("Split signals", no parenthetical)', () => {
    const split = {
      ...makeAnalysis(),
      supporting_signals: ['MICRO1 (bullish): score +35, TRENDING regime, 1 signal'],
      contradicting_signals: ['SLOW (bearish): score -30, RANGE regime, 1 signal'],
    };
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: split,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero.tone).toBe('split');
    expect(p.signal_lean_hero.label_html).toBe('Split signals');
    expect(p.signal_lean_hero.meta_html).toBe('1↑ vs 1↓');
  });

  it('FIX-O2: a bullish TF vote under a NEUTRAL market bias renders amber with a bias qualifier', () => {
    // The user's live capture shape: NEUTRAL badge/bias with all 4 TFs
    // filed under contradicting (raw "(bullish)"). The hero must NOT be a
    // green "Net bullish (4↑ vs 0↓)" under the NEUTRAL badge.
    const neutral = {
      ...makeAnalysis(),
      bias: 'Neutral' as const,
      supporting_signals: [] as string[],
      contradicting_signals: [
        'MICRO1 (bullish): score +26, TRENDING regime, 31 signals',
        'FAST1 (bullish): score +56, TRENDING regime, 25 signals',
        'SLOW (bullish): score +43, TRENDING regime, 21 signals',
        'MACRO1 (bullish): score +17, COMPRESSION regime, 32 signals',
      ],
    };
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: neutral,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero.tone).toBe('split');
    // v6.10.19c (C): the hero counts ALL four timeframe lines — the
    // COMPRESSION macro window now counts as a bullish vote, and the
    // "TF votes: " prefix is gone.
    expect(p.signal_lean_hero.label_html).toBe('Net bullish (4↑ vs 0↓)');
    expect(p.signal_lean_hero.meta_html).toBe('4:0 signal ratio · market bias neutral');
    expect(p.signal_lean_hero.bullish_pct).toBe(100);
    // The lean chip carries the same qualifier.
    expect(p.signals.lean.label).toContain('market bias neutral');
    expect(p.signals.lean.tone).toBe('split');
  });

  it('null analysis still emits the hero placeholders the screen renders', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: null,
      alignment: null,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero).not.toBeNull();
    expect(p.signal_lean_hero.label_html).toBe('No signals');
    expect(p.signal_lean_hero.meta_html).toBe('Waiting for cross-TF consensus');
  });

  it('null analysis emits em-dash placeholders for qualitative, per-TF and rationale', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: null,
      alignment: null,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    for (const key of ['trend', 'momentum', 'structure', 'volatility', 'volume']) {
      expect(p.qualitative_assessment[key]).toBe('—');
    }
    expect(p.qualitative_assessment.cycle_phase).toBe('—');
    expect(p.rationale).toBe('—');
    expect(p.per_timeframe_alignment).toHaveLength(10);
    for (const tf of p.per_timeframe_alignment) {
      expect(tf.trend_display).toBe('—');
      expect(tf.regime).toBe('OFFLINE');
    }
  });

  it('signal score/count placeholders use "—" exactly like the screen', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: {
        ...makeAnalysis(),
        supporting_signals: ['GLOBAL (neutral): no score, awaiting feed'],
      } as unknown as AnalysisMatrix,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const row = p.signals.supporting[0];
    expect(row.score).toBeNull();
    expect(row.score_display).toBe('—');
    expect(row.signals_count).toBeNull();
    expect(row.signals_count_display).toBe('—');
  });

  it('AN-2: all-neutral signals emit the honest neutral hero (not "No signals")', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: {
        ...makeAnalysis(),
        supporting_signals: ['MICRO1 (neutral): score +0, RANGE regime, 0 signals'],
        contradicting_signals: ['FAST1 (neutral): score +0, RANGE regime, 0 signals'],
      } as unknown as AnalysisMatrix,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero.label_html).toBe('Neutral signals');
    expect(p.signal_lean_hero.meta_html).toBe('No directional lean across timeframes');
    expect(p.signal_lean_hero.tone).toBe('split');
    expect(p.signals.lean.label).toBe('Neutral signals · no directional lean');
  });

  it('AN-2: empty signal lists keep the pre-warmup placeholder', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: {
        ...makeAnalysis(),
        supporting_signals: [],
        contradicting_signals: [],
      } as unknown as AnalysisMatrix,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero.label_html).toBe('No signals');
    expect(p.signal_lean_hero.meta_html).toBe('Waiting for cross-TF consensus');
  });

  it('AN-3: zero opposing signals render "3:0", never "3:1"', () => {
    const p = JSON.parse(buildAnalysisTabExport({
      analysis: {
        ...makeAnalysis(),
        supporting_signals: [
          'MICRO1 (bullish): score +35, TRENDING regime, 3 signals',
          'FAST1 (bullish): score +25, TRENDING regime, 2 signals',
          'SLOW (bullish): score +15, TRENDING regime, 1 signal',
        ],
        contradicting_signals: [],
      } as unknown as AnalysisMatrix,
      alignment: makeAlignment(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.signal_lean_hero.meta_html).toBe('3:0 signal ratio');
    expect(p.signal_lean_hero.tone).toBe('bull');
  });
});