// Analysis tab builder — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome). Adds
// signal_lean_hero block, sign-prefixed display strings, and indicator
// key/period separation.

import type { AnalysisMatrix, AlignmentMatrix } from '../../types';
import { DURATIONS, tfLabel } from '../../types';
import { buildPriceBlock, buildHeaderBlock, type MetaEnvelope, type HeaderBlock, type InstanceTermsLike } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { prettifyPhase, highlightKeywords } from '../prettifyPhase';
import { computeAnalysisLean } from '../analysisLean';

// ── Payload types ────────────────────────────────────────────────────────

export interface DecomposedSignal {
  key: string;
  period: number | null;
  display_name: string;
  timeframe: string;
  score: number | null;
  score_display: string;
  regime: string;
  signals_count: number | null;
  /** Screen cell for `signals_count` — "—" when the count is absent. */
  signals_count_display: string;
  raw: string;
}

export interface AnalysisSignalsBlock {
  supporting: DecomposedSignal[];
  contradicting: DecomposedSignal[];
  /** The exact merged, timeframe-sorted list the screen renders, with the
   *  source bucket annotated on each row. */
  list: Array<DecomposedSignal & { bucket: 'supporting' | 'contradicting' }>;
  lean: {
    label: string;
    bullish: number;
    bearish: number;
    tone: 'bull' | 'bear' | 'split';
  };
}

export interface SignalLeanHeroBlock {
  label_html: string;
  meta_html: string;
  bullish_pct: number;
  bearish_pct: number;
  tone: 'bull' | 'bear' | 'split';
}

export interface QualitativeAssessmentBlock {
  trend: string;
  momentum: string;
  structure: string;
  volatility: string;
  volume: string;
  cycle_phase: string;
  /** v6.12: numeric companions — the exact 0-100 alignment dimension
   *  scores each assessment is bucketed from (the badges on the
   *  Analysis cards; the disaggregated siblings of `market_quality_score`).
   *  `null` on the empty sentinel. */
  trend_score: number | null;
  trend_score_display: string;
  momentum_score: number | null;
  momentum_score_display: string;
  structure_score: number | null;
  structure_score_display: string;
  volatility_score: number | null;
  volatility_score_display: string;
  volume_score: number | null;
  volume_score_display: string;
}

export interface PerTimeframeAlignmentRow {
  name: string;
  active: boolean;
  trend: number;
  trend_display: string;
  momentum: number;
  momentum_display: string;
  overall: number;
  overall_display: string;
  regime: string;
}

export interface AnalysisPayload {
  source_tab: 'analysis';
  meta: MetaEnvelope;
  header: HeaderBlock;
  body: AnalysisBodyBlock;
  signal_lean_hero: SignalLeanHeroBlock | null;
  signals: AnalysisSignalsBlock;
  qualitative_assessment: QualitativeAssessmentBlock;
  per_timeframe_alignment: PerTimeframeAlignmentRow[];
  interpretation: string;
  /** Marked-up HTML mirroring the panel's keyword-bolded interpretation. */
  interpretation_display: string;
  rationale: string;
  /** v6.10.18 (I-9): the L3 regime-input values (representative map —
   *  first-TF-wins) the rationale quotes. Exported so a quant can trace
   *  the L3 regime derivation from the data itself; `null` when the
   *  snapshot did not carry them. */
  representative_bbwp: number | null;
  representative_adx: number | null;
  /** KEY METRICS parity (audit C2): the panel's KEY METRICS row renders
   *  Overall Score (`alignment.mtf_overall_score`), Timeframe Agreement
   *  (`trend_agreement_pct` + `timeframes_present`), and Total Signals
   *  (`signal_cross_tf_count`) — all three were missing from the export.
   *  Sources mirror AnalysisPanel.svelte:36-42 (alignment-first with
   *  analysis fallback for the TF count). */
  key_metrics: {
    mtf_overall_score: number | null;
    trend_agreement_pct: number | null;
    timeframes_present: number | null;
    signal_cross_tf_count: number | null;
  };
}

// Also surface the original analysis header for analysts who want it
export interface AnalysisBodyBlock {
  bias: string;
  confidence_pct: number;
  state_confidence: number;
  market_regime: string;
  market_quality: string;
  cycle_phase: string;
}

// ── Helpers ──────────────────────────────────────────────────────────────

function splitIndicatorKey(rawKey: string): { key: string; period: number | null } {
  // "rsi_14" → { key: "rsi", period: 14 }
  // "macd_12_26_9" → { key: "macd", period: null }  (multi-period stays joined)
  // "vwap" → { key: "vwap", period: null }
  const parts = rawKey.split('_');
  if (parts.length >= 2) {
    const last = parts[parts.length - 1];
    const lastNum = parseInt(last, 10);
    if (Number.isFinite(lastNum) && String(lastNum) === last) {
      return { key: parts.slice(0, -1).join('_'), period: lastNum };
    }
  }
  return { key: rawKey, period: null };
}

function displayNameForKey(rawKey: string): string {
  const { key, period } = splitIndicatorKey(rawKey);
  return period != null ? `${key.toUpperCase()} ${period}` : key.toUpperCase();
}

function signedStr(n: number, decimals: number): string {
  // Screen renders `(p.score >= 0 ? '+' : '') + p.score` — zero gets '+'.
  const s = n.toFixed(decimals);
  return n >= 0 ? '+' + s : s;
}

/** Timeframe sort rank — mirrors `AnalysisPanel.svelte::timeframeRank`. */
function timeframeRank(signal: string): number {
  const s = (signal || '').toUpperCase();
  // v11.9 duration-keyed ladder order: derived duration labels, fastest →
  // slowest. Longer labels (15M) must match before their prefixes (1M/5M).
  const DURATION_ORDER = ['1S','3S','5S','15S','30S','1M','3M','5M','15M','30M','1H','4H','12H','1D'];
  for (let i = 0; i < DURATION_ORDER.length; i++) {
      if (new RegExp(`(^|[^0-9A-Z])${DURATION_ORDER[i]}([^0-9A-Z]|$)`).test(s)) return i;
  }
  if (s.includes('MICRO')) return 0;
  if (s.includes('FAST')) return 2;
  if (s.includes('SLOW')) return 4;
  if (s.includes('MACRO')) return 6;
  if (s.includes('DAY')) return 3;
  return 4;
}

function decomposeSignal(raw: string): DecomposedSignal {
  const t = raw || '';
  let timeframe = 'GLOBAL';
  const tfMatch = t.match(/\[?(1S|3S|5S|15S|30S|1M|3M|5M|15M|30M|1H|4H|12H|1D|MICRO|FAST|SLOW|MACRO)\]?/i);
  if (tfMatch) timeframe = tfMatch[1].toUpperCase();
  let score: number | null = null;
  const scoreMatch = t.match(/score\s+([+\-]?\d+)/i);
  if (scoreMatch) score = parseInt(scoreMatch[1], 10);
  let regime = 'UNKNOWN';
  const regimeMatch = t.match(/([a-zA-Z\-_]+)\s+regime/i);
  if (regimeMatch) regime = regimeMatch[1].toUpperCase();
  let signalsCount: number | null = null;
  const sigMatch = t.match(/(\d+)\s+signals?/i);
  if (sigMatch) signalsCount = parseInt(sigMatch[1], 10);
  // Extract a key/period from the raw text if present
  const keyMatch = t.match(/\b([a-z][a-z0-9_]+)\b(?=\s*(?:score|signals?))/i);
  const rawKey = keyMatch ? keyMatch[1] : 'unknown';
  const { key, period } = splitIndicatorKey(rawKey);
  return {
    key,
    period,
    display_name: displayNameForKey(rawKey),
    timeframe,
    score,
    // Screen renders "—" when the score is absent.
    score_display: score != null ? signedStr(score, 0) : '\u2014',
    regime,
    signals_count: signalsCount,
    signals_count_display: signalsCount != null ? String(signalsCount) : '\u2014',
    raw: t,
  };
}

function prettifyBias(bias: string): string {
  // "StrongBullish" → "Strong Bullish"
  return bias.replace(/([a-z])([A-Z])/g, '$1 $2');
}

function buildAnalysisBodyBlock(analysis: AnalysisMatrix | null): AnalysisBodyBlock {
  return {
    bias: prettifyBias(analysis?.bias ?? ''),
    confidence_pct: analysis ? Math.round(analysis.confidence * 100) : 0,
    state_confidence: analysis?.state_confidence ?? 0,
    market_regime: analysis?.market_regime ?? '',
    market_quality: analysis?.market_quality ?? '',
    cycle_phase: prettifyPhase(analysis?.market_phase ?? 'UNKNOWN'),
  };
}

function signalDirection(text: string): 'bullish' | 'bearish' | 'neutral' {
  const dir = text.match(/\((bullish|bearish|neutral)\)/i)?.[1]?.toLowerCase();
  if (dir === 'bullish') return 'bullish';
  if (dir === 'bearish') return 'bearish';
  if (/\bBULLISH\b/i.test(text)) return 'bullish';
  if (/\bBEARISH\b/i.test(text)) return 'bearish';
  return 'neutral';
}

function buildSignalLeanHero(
  analysis: AnalysisMatrix | null,
): SignalLeanHeroBlock | null {
  // The screen ALWAYS renders the hero — including the "No signals" /
  // "Waiting for cross-TF consensus" placeholders when the analysis is
  // absent. Emit the same strings instead of null.
  const supporting = analysis?.supporting_signals ?? [];
  const contradicting = analysis?.contradicting_signals ?? [];
  const allTexts = [...supporting, ...contradicting];
  // v6.10.18 (I-7): the hero vote uses the bias machinery's filter —
  // COMPRESSION windows and flat TFs (|score| ≤ 10) do not vote. The
  // placeholder logic keys on the RAW text presence (empty lists → "No
  // signals"; neutral TFs → "Neutral signals · no directional lean").
  const voteTexts = allTexts;
  const bull = voteTexts.filter((t) => signalDirection(t) === 'bullish').length;
  const bear = voteTexts.filter((t) => signalDirection(t) === 'bearish').length;
  // v6.10.16 (FIX-O2): the shared bias-aware lean — under a Neutral market
  // bias a directional TF vote renders with a neutral (amber) tone and a
  // "market bias neutral" qualifier instead of a green bull hero under the
  // NEUTRAL badge. Raw counts stay visible.
  const lean = computeAnalysisLean(analysis?.bias, bull, bear, allTexts.length);
  const total = bull + bear;
  return {
    label_html: lean.callHtml,
    meta_html: lean.metaHtml,
    bullish_pct: total > 0 ? Math.round((bull / total) * 100) : 0,
    bearish_pct: total > 0 ? Math.round((bear / total) * 100) : 0,
    tone: lean.tone,
  };
}

function buildSignalsBlock(analysis: AnalysisMatrix | null): AnalysisSignalsBlock {
  const supporting = analysis?.supporting_signals ?? [];
  const contradicting = analysis?.contradicting_signals ?? [];
  const allTexts = [...supporting, ...contradicting];
  // v6.10.19c (C): the hero counts ALL timeframe lines present — a
  // display choice over the raw data; the bias engine's LEAN-tier vote
  // definition is unchanged.
  const voteTexts = allTexts;
  const bull = voteTexts.filter((t) => signalDirection(t) === 'bullish').length;
  const bear = voteTexts.filter((t) => signalDirection(t) === 'bearish').length;
  let lean: AnalysisSignalsBlock['lean'];
  // AN-2 + FIX-O2: mirrors the panel via the shared bias-aware helper —
  // empty lists are the pre-warmup placeholder; all-neutral signals surface
  // the honest neutral lean; a directional vote under a Neutral market bias
  // carries the "market bias neutral" qualifier.
  const computed = computeAnalysisLean(analysis?.bias, bull, bear, allTexts.length);
  lean = {
    label: computed.label,
    bullish: computed.bullish,
    bearish: computed.bearish,
    tone: computed.tone,
  };
  return {
    supporting: supporting.map((s) => decomposeSignal(s)),
    contradicting: contradicting.map((c) => decomposeSignal(c)),
    list: [...supporting.map((s) => ({ bucket: 'supporting' as const, sig: s })),
           ...contradicting.map((c) => ({ bucket: 'contradicting' as const, sig: c }))]
      .sort((a, b) => timeframeRank(a.sig) - timeframeRank(b.sig))
      .map((e) => ({ ...decomposeSignal(e.sig), bucket: e.bucket })),
    lean,
  };
}

function buildQualitativeBlock(analysis: AnalysisMatrix | null): QualitativeAssessmentBlock {
  // v6.12: per-card numeric companions — mirror the panel badges'
  // rounded-integer + '%' formatting; '\u2014' when absent (empty sentinel).
  const scorePairs = (v: number | null | undefined): { score: number | null; display: string } => ({
    score: v ?? null,
    display: v != null ? `${Math.round(v)}%` : '\u2014',
  });
  const trendScore = scorePairs(analysis?.trend_score);
  const momentumScore = scorePairs(analysis?.momentum_score);
  const structureScore = scorePairs(analysis?.structure_score);
  const volatilityScore = scorePairs(analysis?.volatility_score);
  const volumeScore = scorePairs(analysis?.volume_score);
  return {
    // Screen renders "—" for missing assessments.
    trend: analysis?.trend_assessment ?? '\u2014',
    momentum: analysis?.momentum_assessment ?? '\u2014',
    structure: analysis?.structure_assessment ?? '\u2014',
    volatility: analysis?.volatility_assessment ?? '\u2014',
    volume: analysis?.volume_assessment ?? '\u2014',
    // Screen renders "—" when the analysis is absent, prettified otherwise.
    cycle_phase: analysis ? prettifyPhase(analysis.market_phase ?? '') : '\u2014',
    // v6.12: per-card dimension-score badges.
    trend_score: trendScore.score,
    trend_score_display: trendScore.display,
    momentum_score: momentumScore.score,
    momentum_score_display: momentumScore.display,
    structure_score: structureScore.score,
    structure_score_display: structureScore.display,
    volatility_score: volatilityScore.score,
    volatility_score_display: volatilityScore.display,
    volume_score: volumeScore.score,
    volume_score_display: volumeScore.display,
  };
}

function buildPerTimeframeBlock(
  alignment: AlignmentMatrix | null,
  activeDurations?: readonly number[],
): PerTimeframeAlignmentRow[] {
  const slots = activeDurations && activeDurations.length > 0 ? activeDurations : [...DURATIONS];
  const order = slots.map((slot) => tfLabel(slot).toUpperCase());
  const alignments = alignment?.timeframe_alignments ?? [];
  return order.map((slot) => {
    const found = alignments.find((a) => a.timeframe.toUpperCase() === slot);
    if (!found) {
      return {
        name: slot,
        active: false,
        trend: 0,
        // Screen renders "—" for inactive slots.
        trend_display: '\u2014',
        momentum: 0,
        momentum_display: '\u2014',
        overall: 0,
        overall_display: '\u2014',
        regime: 'OFFLINE',
      };
    }
    return {
      name: slot,
      active: true,
      trend: found.trend_score ?? 0,
      trend_display: signedStr(found.trend_score ?? 0, 2),
      momentum: found.momentum_score ?? 0,
      momentum_display: signedStr(found.momentum_score ?? 0, 2),
      overall: found.overall_score ?? 0,
      overall_display: signedStr(found.overall_score ?? 0, 1),
      regime: found.regime ?? 'AWAITING',
    };
  });
}

// ── Public builder ───────────────────────────────────────────────────────

export interface AnalysisTabInputs {
  analysis: AnalysisMatrix | null;
  alignment: AlignmentMatrix | null;
  /** v6.10.18 (I-9): the representative L3 regime inputs (bbwp/adx raw
   *  values from the first-TF-wins representative map) for traceability. */
  representative?: { bbwp: number | null; adx: number | null } | null;
  symbol: string;
  exchange?: string;
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  isCompleted?: boolean;
  terms?: InstanceTermsLike;
  /** v11.2 — the ACTIVE slot ladder; the per-timeframe order array and
   *  the price-block snapshot walk follow it. Absent → all 10. */
  activeDurations?: number[];
  headerSpec: LayerHeaderSpec;
}

/**
 * Build the Analysis tab export payload. Mirrors `AnalysisPanel.svelte` 1:1.
 */
export function buildAnalysisTabExport(args: AnalysisTabInputs): string {
  const activeDurations = args.activeDurations && args.activeDurations.length > 0
    ? args.activeDurations
    : [...DURATIONS];
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    activeDurations,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  // M-2 (v6.10.13): the backend's warmup sentinel (`AnalysisMatrix::empty`)
  // renders as the null-state payload — never fabricated Neutral/Poor data.
  const analysis =
    args.analysis && (args.analysis.timeframes_considered ?? 0) > 0 ? args.analysis : null;
  const payload: AnalysisPayload = {
    source_tab: 'analysis',
    meta,
    header: { ...buildHeaderBlock(args.headerSpec), summary_label: 'SUMMARY' },
    body: buildAnalysisBodyBlock(analysis),
    signal_lean_hero: buildSignalLeanHero(analysis),
    signals: buildSignalsBlock(analysis),
    qualitative_assessment: buildQualitativeBlock(analysis),
    per_timeframe_alignment: buildPerTimeframeBlock(args.alignment, activeDurations),
    interpretation: analysis?.market_interpretation ?? '',
    // Screen renders the interpretation with keyword bolding; mirror
    // the marked-up HTML in `interpretation_display` for export parity.
    interpretation_display: highlightKeywords(analysis?.market_interpretation ?? ''),
    // Screen renders "—" when the rationale is absent (AnalysisPanel.svelte:
    // `analysis?.rationale || '—'`) — `||` so an empty string also falls back.
    rationale: analysis?.rationale || '\u2014',
    representative_bbwp: args.representative?.bbwp ?? null,
    representative_adx: args.representative?.adx ?? null,
    // KEY METRICS parity (audit C2): mirror the panel's derivation
    // (AnalysisPanel.svelte:36-42) — alignment-first, analysis fallback
    // for the timeframe count.
    key_metrics: {
      mtf_overall_score: args.alignment?.mtf_overall_score ?? null,
      trend_agreement_pct: args.alignment?.trend_agreement_pct ?? null,
      timeframes_present:
        args.alignment?.timeframes_present ?? analysis?.timeframes_considered ?? null,
      signal_cross_tf_count: args.alignment?.signal_cross_tf_count ?? null,
    },
  };
  return JSON.stringify(payload, null, 2);
}