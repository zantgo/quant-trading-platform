// Export-consistency fixtures — a rich, realistic synthetic store state that
// exercises every display branch of the Market Monitoring panels.
//
// The panels read from the real `useAppStore()` singleton (same as the
// component regression tests in `src/components/*.test.ts`).

import { useAppStore } from '../../state.svelte';
import type { AlignmentMatrix, AnalysisMatrix, BandBasis, IndicatorLifecycleStatus, IndicatorMeta, IndicatorDto, MarketContext, StateBand, StateBandMatrix, TimeframeTelemetry, VolumeProfileSnapshot } from '../../types';

export const PAIR = 'BTC-USDT';
export const MARK_PRICE = 63390.0;

export function seedRichInstance(): void {
  const app = useAppStore();
  for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
  app.initInstance('BTC');
  const entry = app.instancesMap[PAIR];

  entry.terms[1].priceText = String(MARK_PRICE);
  entry.terms[1].barDurationSec = 60;
  entry.terms[1].isCompleted = true;
  entry.terms[5].priceText = '63395.00';
  entry.terms[5].barDurationSec = 180;
  entry.terms[5].isCompleted = true;
  entry.terms[30].priceText = '63380.00';
  entry.terms[30].barDurationSec = 300;
  entry.terms[30].isCompleted = true;
  entry.terms[900].priceText = '63360.00';
  entry.terms[900].barDurationSec = 900;
  entry.terms[900].isCompleted = true;
  entry.lastCompletedClose = String(MARK_PRICE);

  entry.terms[1].latestSnapshot = {
    timestamp: 1_700_000_000,
    mid_price: MARK_PRICE,
    prev_day_px: 62000,
    exchange: 'Hyperliquid',
    is_completed: true,
    // CR-10: the SNAPSHOT-path forced-close flow + cluster matrix. The
    // snapshot is where the band set lives too, because a band set is
    // computed per completed candle and the tf-level field carries the last
    // one forward across shadow ticks.
    state_bands: makeStateBands(),
  } as unknown as Record<string, unknown>;
  entry.terms[5].latestSnapshot = { timestamp: 1_700_000_000, mid_price: 63395, is_completed: true } as unknown as Record<string, unknown>;
  entry.terms[30].latestSnapshot = { timestamp: 1_700_000_000, mid_price: 63380, is_completed: true } as unknown as Record<string, unknown>;
  entry.terms[900].latestSnapshot = { timestamp: 1_700_000_000, mid_price: 63360, is_completed: true } as unknown as Record<string, unknown>;

  // CR-15: the fixture sets the model's two matrices only. The strategy
  // matrices (`risk` / `opportunity` / `advisory` / `decisionContext`) and their
  // builders are gone from the wire, so a fixture that still populated them
  // would be testing an export path that can no longer exist.
  entry.alignment = makeAlignment();
  entry.analysis = makeAnalysis();

  entry.terms[1].indicators = makeMicroIndicators();
  entry.terms[1].indicatorLifecycle = makeMicroLifecycle() as unknown as Record<string, IndicatorLifecycleStatus>;
  entry.terms[1].context = makeContext();
  entry.terms[1].volumeProfile = makeVolumeProfile();
  entry.terms[1].stateBands = makeStateBands();
  entry.terms[5].indicators = makeMicroIndicators();
  entry.terms[5].indicators['rsi_14'] = {
    raw_value: 63.5,
    normalized: 0.31,
    state_label: 'LIVE',
    confidence: 0.7,
    values: null,
    signals: [],
  } as IndicatorDto;
  entry.terms[30].indicators = makeMicroIndicators();
  entry.terms[900].indicators = makeMicroIndicators();

  app.indicatorRegistry = makeRegistry();
}

export function makeRegistry(): IndicatorMeta[] {
  const base = {
    render: 'Pane' as const,
    signal_types: [],
    default_weight: 1,
    default_enabled: true,
    config_params: [],
    color: '#fff',
    guide_section: 'test',
    value_source: 'raw',
    value_format: 'decimals2',
  };
  return [
    { ...base, key: 'rsi_14', display_name: 'RSI (14)', group: 'Momentum', class: 'Leading', directional: true, supports_divergence: true, signal_capability: 'AlwaysActive', updates_on_shadow: true },
    { ...base, key: 'macd_12_26_9', display_name: 'MACD (12,26,9)', group: 'Momentum', class: 'Leading', directional: true, supports_divergence: true, signal_capability: 'AlwaysActive', updates_on_shadow: false },
    { ...base, key: 'squeeze', display_name: 'TTM Squeeze', group: 'Volatility', class: 'Leading', directional: true, value_format: 'onoff', signal_capability: 'Conditional', updates_on_shadow: true },
    { ...base, key: 'vwap', display_name: 'VWAP', group: 'Institutional', class: 'Lagging', directional: true, value_format: 'price', signal_capability: 'AlwaysActive', updates_on_shadow: true },
    { ...base, key: 'support_resistance', display_name: 'S/R Engine', group: 'Structure', class: 'Lagging', directional: false, signal_capability: 'Conditional', updates_on_shadow: false },
    { ...base, key: 'fibonacci', display_name: 'Fibonacci', group: 'Structure', class: 'Lagging', directional: true, signal_capability: 'AlwaysActive', updates_on_shadow: false },
    { ...base, key: 'smc_fvg', display_name: 'SMC FVG', group: 'Institutional', class: 'Leading', directional: true, signal_capability: 'Conditional', updates_on_shadow: false },
    { ...base, key: 'pivot_points', display_name: 'Pivot Points', group: 'Structure', class: 'Lagging', directional: false, signal_capability: 'Conditional', updates_on_shadow: false },
  ] as unknown as IndicatorMeta[];
}

export function makeMicroIndicators(): Record<string, IndicatorDto> {
  return {
    rsi_14: {
      raw_value: 63.5,
      normalized: 0.31,
      state_label: 'LIVE',
      confidence: 0.7,
      values: null,
      signals: [
        { kind: 'Crossover', direction: 'Bullish', status: 'Confirmed', label: 'RSI crossed above 60', strength: 0.8, age_bars: 2, points: null },
        { kind: 'Divergence', direction: 'Bullish', status: 'Active', label: 'BULLISH_DIVERGENCE', strength: 0.75, age_bars: 3, points: [{ time: 1699999900, value: 30.5 }, { time: 1700000000, value: 34.2 }] },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
    macd_12_26_9: {
      raw_value: 12.4,
      normalized: 0.22,
      state_label: 'LIVE',
      confidence: 0.65,
      values: { macd_line: 12.4, signal_line: 8.2, histogram: 4.2 },
      signals: [
        { kind: 'Crossover', direction: 'Bullish', status: 'Active', label: 'MACD line crossed above signal', strength: 0.6, age_bars: 0, points: null },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
    squeeze: {
      raw_value: 0,
      normalized: 0,
      state_label: 'WARMING',
      confidence: 0,
      values: null,
      signals: [],
    } as IndicatorDto,
    vwap: {
      raw_value: 63112.55,
      normalized: 0.18,
      state_label: 'LIVE',
      confidence: 0.55,
      values: { vwap: 63112.55 },
      signals: [
        { kind: 'LevelTest', direction: 'Bullish', status: 'Active', label: 'VWAP_REJECT', strength: 0.5, age_bars: 1, points: null, value_key: 'vwap', is_range: false },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
    support_resistance: {
      raw_value: 63050.0,
      normalized: 0,
      // No state_label → lifecycle `silent: true` drives the SILENT pill.
      confidence: 0.6,
      values: null,
      signals: [
        { kind: 'LevelTest', direction: 'Bullish', status: 'Confirmed', label: 'SUPPORT_TEST', strength: 0.9, age_bars: 4, points: null },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
    fibonacci: {
      raw_value: 63100,
      normalized: 0.6,
      state_label: 'LIVE',
      confidence: 0.72,
      values: {
        gp_top: 64050, gp_bottom: 62600, ext_1618: 65500, ext_2618: 67200,
        fib_0236: 63100, fib_0382: 63250, fib_0500: 63330, fib_0618: 63400, fib_0660: 63440, fib_0786: 63500,
      },
      signals: [],
    } as IndicatorDto,
    smc_fvg: {
      raw_value: 0,
      normalized: 0.1,
      state_label: 'LIVE',
      confidence: 0.5,
      values: { smc_fvg_bottom: 63310, smc_fvg_top: 63350 },
      signals: [
        { kind: 'LevelTest', direction: 'Bullish', status: 'Active', label: 'SMC_FVG_LEVEL_TEST', strength: 0.55, age_bars: 0, points: null, is_range: true },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
    pivot_points: {
      raw_value: 0,
      normalized: 0,
      state_label: 'LIVE',
      confidence: 0.4,
      values: { r1: 64100, r2: 64800, r3: 65500, s1: 62600, s2: 61900, s3: 61200, pivot: 63350 },
      signals: [
        { kind: 'LevelTest', direction: 'Bearish', status: 'Potential', label: 'PIVOT_R2_RESISTANCE_TEST', strength: 0.7, age_bars: 2, points: null, value_key: 'r2', is_range: false },
      ] as unknown as IndicatorDto['signals'],
    } as IndicatorDto,
  };
}

export function makeMicroLifecycle(): Record<string, IndicatorLifecycleStatus> {
  const lc = (state: 'Live' | 'Loading', bars_seen: number, bars_required: number, silent = false) => ({
    state,
    bars_seen,
    bars_required,
    last_updated_at: state === 'Live' ? 1_700_000_000 : null,
    last_error: null as string | null,
    feed_state: undefined as string | undefined,
    ...(silent ? { silent: true } : {}),
  });
  return {
    rsi_14: lc('Live', 14, 14),
    macd_12_26_9: lc('Live', 26, 26),
    squeeze: lc('Loading', 5, 14),
    vwap: lc('Live', 1, 1),
    support_resistance: lc('Live', 20, 20, true),
    fibonacci: lc('Live', 20, 20),
    smc_fvg: lc('Live', 20, 20),
    pivot_points: lc('Live', 20, 20),
  } as unknown as Record<string, IndicatorLifecycleStatus>;
}

export function makeContext(): MarketContext {
  return {
    regime: 'TRENDING',
    overall_score: 0.62,
    overall_label: 'STRONG_BULL',
    trend: { score: 0.7, confidence: 0.8, label: 'STRONG_BULL' },
    momentum: { score: 0.5, confidence: 0.7, label: 'STRONG_BULL' },
    volatility: { score: -0.2, confidence: 0.6, label: 'EXPANDING' },
    volume: { score: 0.3, confidence: 0.65, label: 'HIGH' },
    liquidity: { score: 0.4, confidence: 0.6, label: 'GOOD' },
  };
}

export function makeVolumeProfile(): VolumeProfileSnapshot {
  return {
    symbol: PAIR,
    timeframe_label: '1s',
    timeframe_secs: 60,
    poc_price: 63300,
    value_area_high: 63700,
    value_area_low: 63000,
    total_volume: 12500,
    range_low: 62800,
    range_high: 63900,
    num_bins: 60,
    timestamp_ms: Date.now(),
    bins: [
      { price_low: 63000, price_high: 63020, volume: 120, buy_volume: 80, sell_volume: 40, is_poc: false, is_value_area: true },
      { price_low: 63280, price_high: 63320, volume: 400, buy_volume: 250, sell_volume: 150, is_poc: true, is_value_area: true },
      { price_low: 63680, price_high: 63720, volume: 300, buy_volume: 120, sell_volume: 180, is_poc: false, is_value_area: false },
    ],
  };
}

/**
 * CR-10's replacement for `makeCluster()` + `makeFlow()`.
 *
 * Note what is absent versus the fixture it replaces: no `leverage_assumptions`
 * (the old fixture set `source: 'FUNDING_ADAPTIVE'` with a seven-bucket power
 * law), no `estimation_confidence`, no `cascade_asymmetry`, no `dominant_leverage`
 * per row. Every one of those was either an assumption or a confidence in an
 * assumption, and an export fixture is the last place they should survive —
 * a downstream reader diffing two exports would have seen those numbers move
 * for reasons no candle could explain.
 *
 * What is here is arithmetic: the point of control, the value area, and three
 * zones with their own volume and proximity percentiles.
 */
export function makeStateBands(): StateBandMatrix {
  const band = (
    priceLow: number,
    priceHigh: number,
    basis: BandBasis,
    volumePercentile: number,
    proximityPercentile: number,
    volumeAsymmetry: number,
  ): StateBand => ({
    price_low: priceLow,
    price_high: priceHigh,
    band_atr_multiple: 0.5,
    volume_percentile: volumePercentile,
    proximity_percentile: proximityPercentile,
    intensity_pct: (volumePercentile * proximityPercentile) / 100,
    basis,
    volume_asymmetry: volumeAsymmetry,
  });

  return {
    symbol: PAIR,
    timeframe_label: '1s',
    timeframe_secs: 60,
    poc_price: MARK_PRICE,
    value_area_high: MARK_PRICE * 1.006,
    value_area_low: MARK_PRICE * 0.994,
    has_profile: true,
    has_bands: true,
    // Strongest first by intensity_pct, which is the ordering the Rust builder
    // guarantees and every consumer relies on.
    bands: [
      band(MARK_PRICE * 0.996, MARK_PRICE * 0.999, 'HighVolumeNode', 96, 88, 0.62),
      band(MARK_PRICE * 1.004, MARK_PRICE * 1.008, 'PointOfControl', 78, 71, -0.34),
      band(MARK_PRICE * 1.012, MARK_PRICE * 1.018, 'ValueAreaEdge', 41, 22, 0.08),
    ],
    summary: {
      volume_asymmetry: 0.21,
      contributing_bins: 48,
      band_count: 3,
    },
    timestamp_ms: 1700000000000,
  };
}

export function makeAlignment(): AlignmentMatrix {
  const dim = (score: number, state: string, confidence: number) => ({ score, state, confidence });
  return {
    symbol: PAIR,
    timeframes_present: 14,
    dimensions: [
      dim(75, 'STRONG_BULLISH', 78),
      dim(60, 'BULLISH', 72),
      dim(45, 'NEUTRAL', 65),
      dim(-30, 'BEARISH', 58),
      dim(70, 'STRONG_BULLISH', 75),
      dim(65, 'BULLISH', 70),
      dim(80, 'STRONG_BULLISH', 82),
      dim(70, 'BULLISH', 70),
      dim(55, 'NEUTRAL', 62),
      dim(65, 'BULLISH', 68),
    ],
    mtf_trend_alignment: 0.45,
    mtf_momentum_alignment: 0.3,
    mtf_volume_alignment: 0.1,
    mtf_volatility_alignment: -0.2,
    mtf_overall_score: 30.5,
    mtf_stance: 'bull',
    timeframe_alignments: [
      { timeframe: '1S', timeframe_secs: 1, trend_score: 0.45, momentum_score: 0.3, overall_score: 1.0, regime: 'TRENDING', active_signals: 5, price: MARK_PRICE },
      { timeframe: '3S', timeframe_secs: 3, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '5S', timeframe_secs: 5, trend_score: 0.32, momentum_score: 0.25, overall_score: 0.6, regime: 'TRENDING', active_signals: 3, price: MARK_PRICE },
      { timeframe: '15S', timeframe_secs: 15, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '30S', timeframe_secs: 30, trend_score: 0.15, momentum_score: 0.1, overall_score: 0.3, regime: 'RANGE', active_signals: 1, price: MARK_PRICE },
      { timeframe: '1M', timeframe_secs: 60, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '3M', timeframe_secs: 180, trend_score: -0.1, momentum_score: -0.05, overall_score: -0.2, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '5M', timeframe_secs: 300, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '15M', timeframe_secs: 900, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
      { timeframe: '1H', timeframe_secs: 3600, trend_score: 0.0, momentum_score: 0.0, overall_score: 0.0, regime: 'RANGE', active_signals: 0, price: MARK_PRICE },
    ],
    signal_cross_tf_count: 2,
    trend_agreement_pct: 82,
  };
}

export function makeAnalysis(): AnalysisMatrix {
  return {
    symbol: PAIR,
    bias: 'Bullish',
    confidence: 0.72,
    state_confidence: 0.72,
    market_regime: 'TrendingBull',
    trend_assessment: 'Healthy',
    momentum_assessment: 'Increasing',
    structure_assessment: 'Strong',
    volatility_assessment: 'Normal',
    volume_assessment: 'Strong',
    market_quality: 'Good',
    market_quality_score: 72,
    // v6.12: per-card dimension scores — the exact 0-100 inputs the
    // qualitative assessments above are bucketed from (badge parity).
    trend_score: 76.5,
    momentum_score: 83.2,
    structure_score: 81.4,
    volatility_score: 55.0,
    volume_score: 78.8,
    // v6.10.21: exact L3 regime inputs the rationale quotes.
    representative_bbwp: 83.3,
    representative_adx: 33.0,
    market_phase: 'Markup',
    market_interpretation: 'Price is making higher highs and higher lows on strong volume. Momentum is increasing and structure remains intact.',
    rationale: 'The market is in a healthy uptrend with broad participation across timeframes.',
    supporting_signals: [
      '1S (bullish): rsi_14 score +62, TRENDING regime, 3 signals',
      '5S (bullish): macd_12_26_9 score +45, TRENDING regime, 2 signals',
    ],
    contradicting_signals: [
      '3M (bearish): obv score -20, RANGE regime, 1 signals',
    ],
    timeframes_considered: 14,
  };
}

export type TimeframeLike = TimeframeTelemetry;
