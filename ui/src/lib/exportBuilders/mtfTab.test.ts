// Regression tests for the v7.0-audit MTF tab export.

import { describe, it, expect } from 'vitest';
import { makeTerms } from '../../tests/makeTerms';
import { buildMtfExportJson } from './mtfTab';
import type { LayerHeaderSpec } from '../layerHeader';
import type { TimeframeTelemetry, IndicatorMeta, IndicatorDto } from '../../types';

const headerSpec: LayerHeaderSpec = {
  layerNumber: 1,
  layerName: 'Metrics · MTF',
  badge: { label: 'MTF SYNC', color: '#22c55e', background: 'rgba(34,197,94,0.08)', state: 'valid' },
  meta: [],
  status: 'live',
};

const registry: IndicatorMeta[] = [
  {
    key: 'rsi_14',
    display_name: 'RSI 14',
    group: 'Momentum',
    class: 'Hybrid',
    value_format: 'decimals2',
    value_source: 'raw',
    default_enabled: true,
    directional: true,
  } as unknown as IndicatorMeta,
];

function makeTf(label: string, priceText: string): TimeframeTelemetry {
  return {
    barDurationSec: 60,
    isCompleted: true,
    pipelineState: 'OK',
    priceText,
    latestSnapshot: { timestamp: Math.floor(Date.now() / 1000) - 5 },
    indicators: {
      rsi_14: { raw_value: 62.4, normalized: 0.24, confidence: 0.78, state_label: 'BULLISH', signals: [], values: {} },
    } as unknown as Record<string, IndicatorDto>,
  } as unknown as TimeframeTelemetry;
}

describe('buildMtfExportJson', () => {
  it('emits groups with display labels and indicators with key/period split', () => {
    const p = JSON.parse(buildMtfExportJson({
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
      registry,
      terms: makeTerms({
          micro1: makeTf('Micro', '63390'),
          fast1: makeTf('Fast', '63395'),
          slow1: makeTf('Slow', '63300'),
          macro1: makeTf('Macro', '63000'),
      }),
    }));
    expect(p.meta.pair).toBe('BTC-USDT');
    expect(p.header.layer_name).toBe('Metrics · MTF');
    expect(p.groups[0].label).toBe('Momentum');
    const rsi = p.indicators.find((i: { key: string }) => i.key === 'rsi');
    expect(rsi.period).toBe(14);
    expect(rsi.display_name).toBe('RSI 14');
    expect(rsi.values).toHaveLength(10);
    expect(rsi.values[0].normalized_display).toBe('+0.24');
    expect(typeof rsi.agreement).toBe('number');
    expect(['BULL', 'BEAR', 'MIXED']).toContain(rsi.agreement_label);
  });

  it('timeframes carry fibonacci summary with status strings', () => {
    const p = JSON.parse(buildMtfExportJson({
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
      registry,
      terms: makeTerms({
          micro1: makeTf('Micro', '63390'),
          fast1: makeTf('Fast', '63395'),
          slow1: makeTf('Slow', '63300'),
          macro1: makeTf('Macro', '63000'),
      }),
    }));
    expect(p.timeframes).toHaveLength(10);
    expect(p.timeframes[0].fibonacci_summary.present).toBe(false);
    expect(p.timeframes[0].fibonacci_summary.swing_direction).toBe('NEUTRAL SWING');
  });

  // Regression (D1): the cross-TF merge compared `ind.confidence_pct`
  // (0..100) against the stored `confidence` (0..1 fraction), so every
  // later TF with confidence >= 2% won and the LAST timeframe (Macro)
  // always supplied the merged values. MTF aggregates must come from the
  // HIGHEST-CONFIDENCE timeframe instead.
  it('merged aggregates come from the highest-confidence TF, not the last one', () => {
    const confRegistry: IndicatorMeta[] = [
      {
        key: 'rsi_14',
        display_name: 'RSI 14',
        group: 'Momentum',
        class: 'Hybrid',
        value_format: 'decimals2',
        value_source: 'raw',
        default_enabled: true,
        directional: true,
      } as unknown as IndicatorMeta,
    ];
    const makeTfWith = (label: string, conf: number, norm: number, signalLabel: string): TimeframeTelemetry => ({
      barDurationSec: 60,
      isCompleted: true,
      pipelineState: 'OK',
      priceText: '63000',
      latestSnapshot: { timestamp: Math.floor(Date.now() / 1000) - 5 },
      indicators: {
        rsi_14: {
          raw_value: 62.4,
          normalized: norm,
          confidence: conf,
          state_label: norm >= 0 ? 'BULLISH' : 'BEARISH',
          signals: [{
            kind: 'Threshold',
            direction: norm >= 0 ? 'Bullish' : 'Bearish',
            status: 'Active',
            label: signalLabel,
            strength: 0,
            age_bars: 0,
            display_label: signalLabel,
          }],
          values: {},
        },
      } as unknown as Record<string, IndicatorDto>,
    } as unknown as TimeframeTelemetry);

    // Micro has the HIGHEST confidence (1.00) with a bullish reading;
    // Fast/Slow/Macro have lower confidence. Pre-fix, Macro (confidence
    // 0.5 = 50%) beat the stored fraction 1.0... 50 > 1.0 → macro won.
    const p = JSON.parse(buildMtfExportJson({
      symbol: 'BTC-USDT',
      markPrice: 63000,
      headerSpec,
      registry: confRegistry,
      terms: makeTerms({
          micro1: makeTfWith('Micro', 1.0, 0.9, 'RSI_MICRO_BULLISH'),
          fast1: makeTfWith('Fast', 0.6, -0.5, 'RSI_FAST_BEARISH'),
          slow1: makeTfWith('Slow', 0.4, 0.2, 'RSI_SLOW_BULLISH'),
          macro1: makeTfWith('Macro', 0.5, -0.1, 'RSI_MACRO_BEARISH'),
      }),
    }));

    // The merged indicator must reflect the MICRO reading (conf 1.00):
    // normalized 0.9 → bullish confluence, and its signal label must be
    // the one that surfaces in signals_by_kind.
    const confluence = p.group_confluence.find((g: { group: string }) => g.group === 'Momentum');
    expect(confluence).toEqual(expect.objectContaining({ bullish: 1, bearish: 0, neutral: 0 }));

    const thSignals = p.signals_by_kind.Threshold ?? [];
    const labels = thSignals.map((s: { label: string }) => s.label);
    expect(labels).toContain('RSI_MICRO_BULLISH');
    expect(labels).not.toContain('RSI_MACRO_BEARISH');
  });
});
// Regression (D7): unlike the single-TF Metrics builder (which omits
// missing-DTO indicators from its rows), the MTF top-level indicators
// list zero-fills registry entries with no store DTO (active: false).
it('zero-fills missing-DTO registry entries in the MTF indicator list', () => {
  const wideRegistry = [
    ...registry,
    {
      key: 'smc_liquidity',
      display_name: 'SMC Liquidity',
      group: 'Institutional',
      class: 'Leading',
      value_format: 'decimals2',
      value_source: 'raw',
      default_enabled: true,
      directional: true,
    } as unknown as IndicatorMeta,
  ];
  const p = JSON.parse(buildMtfExportJson({
    symbol: 'BTC-USDT',
    markPrice: 63390,
    headerSpec,
    registry: wideRegistry,
    terms: makeTerms({
        micro1: makeTf('Micro', '63390'),
        fast1: makeTf('Fast', '63395'),
        slow1: makeTf('Slow', '63300'),
        macro1: makeTf('Macro', '63000'),
    }),
  }));
  const rows = p.indicators as Array<{ key: string; values: Array<{ active: boolean; normalized: number }> }>;
  expect(rows).toHaveLength(2);
  const smc = rows.find((i) => i.key === 'smc_liquidity');
  expect(smc).toBeDefined();
  expect(smc!.values.every((v) => v.active === false && v.normalized === 0)).toBe(true);
});

// Audit G-4: WARMING placeholders and non-Directional gates are NOT
// "available" readings — the export must mirror the screen's '--'/'N/A'
// cells instead of reporting active 0.0 values.
it('warming placeholders and gated rows are inactive in MTF cells', () => {
  const warmRegistry: IndicatorMeta[] = [
    ...registry,
    {
      key: 'bbwp',
      display_name: 'BBWP',
      group: 'Volatility',
      class: 'Leading',
      value_format: 'decimals2',
      value_source: 'raw',
      default_enabled: true,
      directional: false,
      normalization_mode: 'ContextOnly',
    } as unknown as IndicatorMeta,
  ];
  const makeWarmTf = (label: string): TimeframeTelemetry => ({
    barDurationSec: 60,
    isCompleted: true,
    pipelineState: 'OK',
    priceText: '63000',
    latestSnapshot: { timestamp: Math.floor(Date.now() / 1000) - 5 },
    indicators: {
      rsi_14: { raw_value: 62.4, normalized: 0.24, confidence: 0.78, state_label: 'WARMING', signals: [], values: {} },
      bbwp: { raw_value: 0, normalized: 0, confidence: 0, state_label: 'WARMING', signals: [], values: {} },
    } as unknown as Record<string, IndicatorDto>,
  } as unknown as TimeframeTelemetry);
  const p = JSON.parse(buildMtfExportJson({
    symbol: 'BTC-USDT',
    markPrice: 63390,
    headerSpec,
    registry: warmRegistry,
    terms: makeTerms({
        micro1: makeWarmTf('Micro'),
        fast1: makeWarmTf('Fast'),
        slow1: makeWarmTf('Slow'),
        macro1: makeWarmTf('Macro'),
    }),
  }));
  const rsi = p.indicators.find((i: { key: string }) => i.key === 'rsi');
  // WARMING → inactive, dash display, excluded from the agreement mean.
  expect(rsi.values[0].active).toBe(false);
  expect(rsi.values[0].warming).toBe(true);
  expect(rsi.values[0].normalized_display).toBe('\u2014');
  expect(rsi.agreement).toBe(0);
  expect(rsi.normalized_available).toBe(false);
  const bbwp = p.indicators.find((i: { key: string }) => i.key === 'bbwp');
  // ContextOnly gate → inactive regardless of DTO presence.
  expect(bbwp.values[0].active).toBe(false);
  expect(bbwp.values[0].gated).toBe(true);
  // Per-TF detail rows report the same.
  const microRow = p.timeframes[0].indicators.find((i: { key: string }) => i.key === 'rsi');
  expect(microRow.normalized_available).toBe(false);
});

// Audit G-3: the cross_tf_tables block mirrors the three screen tables.
it('cross_tf_tables carries per-TF signal tallies and totals', () => {
  // Real-style registry key ("rsi" — no period suffix, as the backend
  // emits; the `supports_divergence` flag makes it a divergence row).
  const sigRegistry: IndicatorMeta[] = [{
    key: 'rsi',
    display_name: 'RSI 14',
    group: 'Momentum',
    class: 'Hybrid',
    value_format: 'decimals2',
    value_source: 'raw',
    default_enabled: true,
    directional: true,
    supports_divergence: true,
  } as unknown as IndicatorMeta];
  const makeSigTf = (label: string, direction: 'Bullish' | 'Bearish'): TimeframeTelemetry => ({
    barDurationSec: 60,
    isCompleted: true,
    pipelineState: 'OK',
    priceText: '63000',
    latestSnapshot: { timestamp: Math.floor(Date.now() / 1000) - 5 },
    indicators: {
      rsi: {
        raw_value: 62.4, normalized: 0.24, confidence: 0.78, state_label: 'BULLISH',
        signals: [{
          kind: 'Threshold', direction, status: 'Active', label: 'RSI_THRESHOLD',
          strength: 0.5, age_bars: 2, display_label: 'TH·2',
        }],
        values: {},
      },
    } as unknown as Record<string, IndicatorDto>,
  } as unknown as TimeframeTelemetry);
  const p = JSON.parse(buildMtfExportJson({
    symbol: 'BTC-USDT',
    markPrice: 63390,
    headerSpec,
    registry: sigRegistry,
    terms: makeTerms({
        micro1: makeSigTf('Micro', 'Bullish'),
        fast1: makeSigTf('Fast', 'Bullish'),
        slow1: makeSigTf('Slow', 'Bearish'),
        macro1: makeSigTf('Macro', 'Bearish'),
    }),
  }));
  const signals = p.cross_tf_tables.signals as Array<{
    kind: string;
    per_timeframe: Array<{ timeframe: string; bull: number; bear: number; neutral: number; entries: unknown[] }>;
    totals: { bull: number; bear: number; neutral: number };
  }>;
  const th = signals.find((s) => s.kind === 'Threshold');
  expect(th).toBeDefined();
  expect(th!.per_timeframe.map((c) => [c.bull, c.bear])).toEqual([
    [1, 0], [0, 0], [1, 0], [0, 0], [0, 1], [0, 0], [0, 1], [0, 0], [0, 0], [0, 0],
  ]);
  expect(th!.totals).toEqual({ bull: 2, bear: 2, neutral: 0 });
  expect(th!.per_timeframe[0].entries).toHaveLength(1);
  expect(th!.per_timeframe[0].entries[0]).toMatchObject({ display_name: 'RSI 14', label: 'RSI_THRESHOLD' });
  // Divergence rows: the fixture has no divergences — only capable
  // indicators appear, with empty per-TF cells.
  const divs = p.cross_tf_tables.divergences as Array<{ key: string; row_count: number }>;
  const rsiDiv = divs.find((d) => d.key === 'rsi');
  expect(rsiDiv).toBeDefined();
  expect(rsiDiv!.row_count).toBe(0);
  expect(p.cross_tf_tables.totals.signal_count).toBe(4);
  expect(p.cross_tf_tables.totals.global_signal_lean).toEqual({ bull: 2, bear: 2, neutral: 0 });
});
