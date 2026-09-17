// Alignment tab builder — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome). Adds
// `label_display` for consensus, sign-prefixed display strings for
// axes and per-TF scores.
//
// v6.10.20 (C): the consensus hero is a two-container row — the dial
// verdict renders as a header + sub-label, and the "polarization"
// term is retired (field renamed `consensus.axes`).

import type { AlignmentMatrix, AlignmentDimension, TfAlignmentInfo } from '../../types';
import { buildPriceBlock, buildHeaderBlock, type MetaEnvelope, type HeaderBlock, type InstanceTermsLike } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { mLabel } from '../layerHeader';

// ── Payload types ────────────────────────────────────────────────────────

export const ALIGNMENT_DIMENSION_NAMES = [
  'Trend', 'Momentum', 'Volume', 'Volatility',
  'Structure', 'Signal', 'Regime', 'Confidence', 'Liquidity', 'Tradability',
] as const;

export interface AlignmentHeroBlock {
  mtf_overall_score: number;
  mtf_overall_label: string;
  mtf_overall_label_display: string;
  timeframes_present: number;
  signal_cross_tf_count: number;
  trend_agreement_pct: number;
}

export interface AlignmentDimensionRow {
  name: string;
  score: number;
  state: string;
  confidence: number;
}

export interface AlignmentConsensusBlock {
  /** Raw agreement percent (float) — screen text uses `toFixed(0)`, the
   *  consensus bar width uses `toFixed(1)`; null when no alignment yet
   *  (screen renders the "—%" placeholder). */
  trend_agreement_pct: number | null;
  label: 'strong_consensus' | 'partial_consensus' | 'conflict' | null;
  label_display: string;
  // v6.10.20 (C): renamed from `polarization` — the axis values are the
  // four blend dimensions (signed axes [−1, 1]) feeding the consensus.
  axes: Array<{
    key: 'Trend' | 'Momentum' | 'Volume' | 'Volatility';
    label: string;
    value: number;
    value_display: string;
  }>;
}

export interface AlignmentPerTimeframeRow {
  timeframe: string;
  trend_score: number;
  trend_score_display: string;
  momentum_score: number;
  momentum_score_display: string;
  overall_score: number;
  overall_score_display: string;
  regime: string;
  active_signals: number;
}

export interface AlignmentScoreCalcBlock {
  weights: Array<{
    key: 'Trend' | 'Momentum' | 'Volume' | 'Volatility';
    label: string;
    pct: number;
    color: string;
    value: number;
    value_display: string;
    contribution: number;
    contribution_display: string;
  }>;
}

export interface AlignmentPayload {
  source_tab: 'alignment';
  /** v11.4: mirrors the TfStatusTable (per-ACTIVE-slot badges). */
  timeframe_status: AlignmentTimeframeStatusRow[];
  meta: MetaEnvelope;
  header: HeaderBlock;
  hero: AlignmentHeroBlock;
  dimensions: AlignmentDimensionRow[];
  consensus: AlignmentConsensusBlock;
  per_timeframe: AlignmentPerTimeframeRow[];
  score_calculation: AlignmentScoreCalcBlock;
  interpretation: string;
  /** v7.1: the whisper footnote under the interpretation paragraph —
   *  composition weights spelled in full words (data-auditing parity with
   *  the panel's footnote). Null when no alignment data exists yet. */
  composition_note: string | null;
  consensus_conflict_banner: string;
}

// ── Constants ────────────────────────────────────────────────────────────

const SLOT_RANK: Record<string, number> = { MICRO: 0, FAST: 1, SLOW: 2, MACRO: 3 };

// v6.10.18: keys are the full dimension names — the legacy "Vt"/"Vm"
// abbreviations bound Volume/Volatility swapped vs. the spec (V_t =
// volatility, V_m = volume, 02-01 §4.2). Full-word keys bind each weight
// to exactly one mtf_*_alignment field; legacy wire keys are normalized
// in `effective()` ("Vt" → Volume, "Vm" → Volatility, matching the
// legacy wire semantics).
const SCORE_CALC_WEIGHTS: Array<{
  label: string;
  key: 'Trend' | 'Momentum' | 'Volume' | 'Volatility';
  pct: number;
  color: string;
}> = [
  { label: 'Trend',      key: 'Trend',      pct: 50, color: '#22c55e' },
  { label: 'Momentum',   key: 'Momentum',   pct: 30, color: '#3b82f6' },
  { label: 'Volume',     key: 'Volume',     pct: 10, color: '#a78bfa' },
  { label: 'Volatility', key: 'Volatility', pct: 10, color: '#f59e0b' },
];

const WEIGHT_KEY_CANON: Record<string, string> = {
  T: 'Trend',
  M: 'Momentum',
  Vt: 'Volume',
  Vm: 'Volatility',
  Trend: 'Trend',
  Momentum: 'Momentum',
  Volume: 'Volume',
  Volatility: 'Volatility',
};

// v6.10.20 (C): label_display mirrors the dial verdict HEADER only —
// the panel renders it as a bold tier-colored header ("Strong Consensus")
// with a grey sub-label ("Timeframes are aligned.") that stays DOM-only.
const CONSENSUS_DISPLAY: Record<'strong_consensus' | 'partial_consensus' | 'conflict', string> = {
  strong_consensus: 'Strong Consensus',
  partial_consensus: 'Partial Consensus',
  conflict: 'Mixed Consensus',
};

// ── Helpers ──────────────────────────────────────────────────────────────

function shortStateLabel(state: string): string {
  // Panel parity (AlignmentPanel.svelte): normalize case + underscores so
  // both PascalCase wire ("StrongBullish", "NoData") and legacy SCREAMING
  // payloads resolve identically — the old case-sensitive startsWith
  // rendered "STRONGBULLISH"/"NODATA" for the real wire.
  const s = String(state || '').toUpperCase().replace(/_/g, ' ');
  if (s === 'NO DATA' || s === 'NODATA') return 'NO DATA';
  if (s.startsWith('STRONG')) return 'STRONG';
  return s;
}

function signedStr(n: number, decimals: number): string {
  // Screen convention (AlignmentPanel.svelte): values >= 0 render with a
  // leading '+' (so 0.00 shows as "+0.00"); only negatives are bare.
  const s = n.toFixed(decimals);
  return n >= 0 ? '+' + s : s;
}

function buildConflictBanner(alignment: AlignmentMatrix | null): string {
  if (!alignment) return '';
  if (alignment.trend_agreement_pct < 50 && alignment.timeframes_present > 0) {
    return 'TIMEFRAME MISALIGNMENT — time horizons are not working together';
  }
  return '';
}

function buildHeroBlock(alignment: AlignmentMatrix): AlignmentHeroBlock {
  return {
    mtf_overall_score: alignment.mtf_overall_score,
    mtf_overall_label: alignment.mtf_overall_label,
    mtf_overall_label_display: mLabel(alignment.mtf_overall_label),
    timeframes_present: alignment.timeframes_present,
    signal_cross_tf_count: alignment.signal_cross_tf_count,
    trend_agreement_pct: alignment.trend_agreement_pct,
  };
}

function buildDimensionsBlock(alignment: AlignmentMatrix): AlignmentDimensionRow[] {
  return alignment.dimensions.map((dim: AlignmentDimension, i: number) => ({
    name: ALIGNMENT_DIMENSION_NAMES[i] ?? `Dim ${i}`,
    score: Math.round(dim.score),
    state: shortStateLabel(dim.state ?? ''),
    // `dim.confidence` is already 0..100 on the wire (Rust alignment.rs
    // `(score / 100.0) * 100.0`); the screen renders `confidence.toFixed(0)%`.
    confidence: Math.round(dim.confidence ?? 0),
  }));
}

function classifyConsensus(pct: number): 'strong_consensus' | 'partial_consensus' | 'conflict' {
  if (pct >= 75) return 'strong_consensus';
  if (pct >= 50) return 'partial_consensus';
  return 'conflict';
}

function buildConsensusBlock(alignment: AlignmentMatrix): AlignmentConsensusBlock {
  const pct = alignment.trend_agreement_pct;
  return {
    trend_agreement_pct: pct,
    label: classifyConsensus(pct),
    label_display: CONSENSUS_DISPLAY[classifyConsensus(pct)],
    axes: [
      { key: 'Trend',      label: 'Trend',      value: alignment.mtf_trend_alignment,      value_display: signedStr(alignment.mtf_trend_alignment, 2) },
      { key: 'Momentum',   label: 'Momentum',   value: alignment.mtf_momentum_alignment,   value_display: signedStr(alignment.mtf_momentum_alignment, 2) },
      { key: 'Volume',     label: 'Volume',     value: alignment.mtf_volume_alignment,     value_display: signedStr(alignment.mtf_volume_alignment, 2) },
      { key: 'Volatility', label: 'Volatility', value: alignment.mtf_volatility_alignment, value_display: signedStr(alignment.mtf_volatility_alignment, 2) },
    ],
  };
}

function buildPerTimeframeBlock(alignment: AlignmentMatrix): AlignmentPerTimeframeRow[] {
  return (alignment.timeframe_alignments ?? [])
    .slice()
    .sort((a, b) => (SLOT_RANK[a.timeframe] ?? 99) - (SLOT_RANK[b.timeframe] ?? 99))
    .map((tf: TfAlignmentInfo) => ({
      timeframe: tf.timeframe,
      trend_score: tf.trend_score,
      // Screen chips render unsigned (`Trend 0.15`) — no '+' prefix.
      trend_score_display: tf.trend_score.toFixed(2),
      momentum_score: tf.momentum_score,
      momentum_score_display: tf.momentum_score.toFixed(2),
      overall_score: tf.overall_score,
      overall_score_display: tf.overall_score.toFixed(1),
      regime: tf.regime,
      active_signals: tf.active_signals,
    }));
}

function buildScoreCalcBlock(alignment: AlignmentMatrix): AlignmentScoreCalcBlock {
  const getValue = (key: 'Trend' | 'Momentum' | 'Volume' | 'Volatility'): number => {
    if (key === 'Trend') return alignment.mtf_trend_alignment;
    if (key === 'Momentum') return alignment.mtf_momentum_alignment;
    if (key === 'Volume') return alignment.mtf_volume_alignment;
    return alignment.mtf_volatility_alignment;
  };
  // v6.10.16 (FIX-H2): the backend may apply thin-participation
  // reweighting (Trend 0.55 / Momentum 0.35 / Volume 0.05 / Volatility
  // 0.05) — consume the wire's effective weights so the export formula
  // ALWAYS balances the composite it mirrors. Legacy payloads fall back
  // to the standard static table. v6.10.18: wire keys are full dimension
  // names; legacy "T"/"M"/"Vt"/"Vm" keys are normalized via
  // WEIGHT_KEY_CANON ("Vt" → Volume, "Vm" → Volatility).
  const effective = (key: string): number => {
    const canon = WEIGHT_KEY_CANON[key] ?? key;
    const found = (alignment.blend_weights ?? []).find(([k]) => (WEIGHT_KEY_CANON[k] ?? k) === canon);
    return found ? found[1] : SCORE_CALC_WEIGHTS.find((w) => w.key === canon)!.pct / 100;
  };
  const weights = SCORE_CALC_WEIGHTS.map((w) => {
    const value = getValue(w.key);
    const pct = effective(w.key);
    return {
      key: w.key,
      label: w.label,
      pct: Math.round(pct * 100),
      color: w.color,
      value,
      value_display: signedStr(value, 2),
      contribution: value * pct,
      // Screen shows the contribution at 2 decimals (Weight chips row).
      contribution_display: signedStr(value * pct, 2),
    };
  });
  // v6.10.19d (A): the formula line was removed from the panel — the
  // export mirrors the screen (weights only).
  return { weights };
}

function buildInterpretation(alignment: AlignmentMatrix | null): string {
  if (!alignment || alignment.timeframes_present === 0) {
    // AL-7: the NO_DATA sentinel (and null) render the awaiting copy —
    // never a fabricated "conflict" verdict from zero data.
    return 'Awaiting alignment data — this section will synthesize a human-readable interpretation of multi-timeframe consensus once indicators populate.';
  }
  const pct = alignment.trend_agreement_pct;
  // v7.1: the prose prints the EXACT signed score string the SCORE dial
  // renders (signed integer) — the old unsigned `toFixed(1)` ("8.1"
  // beside the dial's "+8") was read as score drift.
  const overall = signedStr(alignment.mtf_overall_score, 0);
  const present = alignment.timeframes_present;
  const crossTf = alignment.signal_cross_tf_count;
  const label = mLabel(alignment.mtf_overall_label).toUpperCase();
  if (pct >= 75) {
    // v7.1: a NEUTRAL composite can never show "strong directional
    // consensus" — neutral is directionless by definition, so the copy
    // reads "moderate consensus" instead.
    const consensusPhrase = label === 'NEUTRAL' ? 'moderate consensus' : 'strong directional consensus';
    const scoreSentence = label === 'NEUTRAL'
      ? `The composite score of ${overall} is classified as <strong>NEUTRAL</strong> — the dimensions offset into a flat composite.`
      : `The composite score of ${overall} is classified as <strong>${label}</strong>.`;
    const crossLine = crossTf > 0
      ? label === 'NEUTRAL'
        ? `${crossTf} cross-timeframe signal votes detected across the aligned timeframes.`
        : `${crossTf} cross-timeframe signal votes reinforce the current bias.`
      : 'No cross-timeframe signal votes detected.';
    // Mirrors the screen paragraph verbatim — the label is the REAL
    // mtf_overall_label, never a hardcoded token.
    // AUDIT-FE-M7: no hardcoded "/4" denominator (custom ladders).
    return `Multi-timeframe alignment shows <strong>${consensusPhrase}</strong> (${pct.toFixed(0)}% agreement across ${present} timeframes). ${scoreSentence} ${crossLine}`;
  }
  if (pct >= 50) {
    return `Alignment shows <strong>partial consensus</strong> (${pct.toFixed(0)}% agreement). The composite score of ${overall} reflects <strong>${label}</strong> conditions with mixed input from ${present} timeframes.`;
  }
  return `Timeframes are in <strong>conflict</strong> (${pct.toFixed(0)}% agreement). Exercise caution — different time horizons are pulling in opposite directions. Wait for re-alignment before committing to directional bias.`;
}

// v7.1: the whisper footnote — composition weights spelled in full words,
// mirroring the panel's footnote (order Trend, Momentum, Volatility,
// Volume per the review prose). Percentages come from the live wire
// `blend_weights` (thin-participation reweight included), never stale
// constants.
const FOOTNOTE_ORDER: Record<string, number> = { Trend: 0, Momentum: 1, Volatility: 2, Volume: 3 };
const PCT_WORDS: Record<number, string> = {
  5: 'five', 10: 'ten', 30: 'thirty', 35: 'thirty-five', 50: 'fifty', 55: 'fifty-five',
};
function pctWord(pct: number): string {
  return PCT_WORDS[pct] ?? String(pct);
}
function buildCompositionNote(alignment: AlignmentMatrix): string | null {
  if (!alignment || alignment.timeframes_present === 0) return null;
  const wire = alignment.blend_weights ?? [];
  const eff = SCORE_CALC_WEIGHTS.map((w) => {
    const found = wire.find(([k]) => (WEIGHT_KEY_CANON[k] ?? k) === w.key);
    return { label: w.label, key: w.key, pct: Math.round((found ? found[1] : w.pct / 100) * 100) };
  });
  const sorted = eff.slice().sort((a, b) => (FOOTNOTE_ORDER[a.key] ?? 9) - (FOOTNOTE_ORDER[b.key] ?? 9));
  const parts = sorted.map((w) => `${w.label} (${pctWord(w.pct)} percent)`);
  const body = parts.length > 1
    ? `${parts.slice(0, -1).join(', ')}, and ${parts[parts.length - 1]}`
    : parts.join('');
  return `Composition weights: ${body}.`;
}

// ── Public builder ───────────────────────────────────────────────────────

export interface AlignmentTimeframeStatusRow {
  slot: number;
  secs: number;
  badge_label: string;
  badge_sublabel: string | undefined;
  pipeline_status: string;
}

export interface AlignmentTabInputs {
  /**
   * v11.4: mirrors the TfStatusTable rows (per-ACTIVE-slot metrics badge +
   * pipeline status) — computed by the caller (needs WS state).
   */
  timeframe_status?: AlignmentTimeframeStatusRow[] | null;
  alignment: AlignmentMatrix | null;
  symbol: string;
  exchange?: string;
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  isCompleted?: boolean;
  terms?: InstanceTermsLike;
  headerSpec: LayerHeaderSpec;
}

/**
 * Build the Alignment tab export payload. Mirrors
 * `AlignmentPanel.svelte` 1:1.
 */
export function buildAlignmentTabExport(args: AlignmentTabInputs): string {
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  const alignment = args.alignment;
  // AL-7 (v6.10.10): the backend's warmup sentinel (`AlignmentMatrix::empty` —
  // 0 TFs, NO_DATA label) must render the awaiting consensus/interpretation,
  // never a fabricated "Conflict" verdict. The dimension cards keep their
  // honest NO DATA rows.
  const hasAlignment = !!alignment && alignment.timeframes_present > 0;
  const empty: AlignmentPayload = {
    source_tab: 'alignment',
    timeframe_status: args.timeframe_status ?? [],
    meta,
    header: { ...buildHeaderBlock(args.headerSpec), summary_label: 'SUMMARY' },
    hero: {
      mtf_overall_score: 0,
      mtf_overall_label: 'NO_DATA',
      mtf_overall_label_display: '—',
      timeframes_present: 0,
      signal_cross_tf_count: 0,
      trend_agreement_pct: 0,
    },
    dimensions: alignment ? buildDimensionsBlock(alignment) : [],
    consensus: {
      // Screen renders the "—%" placeholder + em-dash verdict; JSON carries
      // the same null-state instead of fabricating a definitive verdict.
      trend_agreement_pct: null,
      label: null,
      label_display: '\u2014',
      axes: [
        { key: 'Trend',      label: 'Trend',      value: 0, value_display: signedStr(0, 2) },
        { key: 'Momentum',   label: 'Momentum',   value: 0, value_display: signedStr(0, 2) },
        { key: 'Volume',     label: 'Volume',     value: 0, value_display: signedStr(0, 2) },
        { key: 'Volatility', label: 'Volatility', value: 0, value_display: signedStr(0, 2) },
      ],
    },
    per_timeframe: [],
    score_calculation: {
      weights: SCORE_CALC_WEIGHTS.map((w) => ({
        key: w.key,
        label: w.label,
        pct: w.pct,
        color: w.color,
        value: 0,
        value_display: '\u2014',
        contribution: 0,
        contribution_display: '\u2014',
      })),
    },
    interpretation: buildInterpretation(null),
    composition_note: null,
    consensus_conflict_banner: '',
  };
  if (!hasAlignment) {
    return JSON.stringify(empty, null, 2);
  }
  const payload: AlignmentPayload = {
    source_tab: 'alignment',
    timeframe_status: args.timeframe_status ?? [],
    meta,
    header: { ...buildHeaderBlock(args.headerSpec), summary_label: 'SUMMARY' },
    hero: buildHeroBlock(alignment),
    dimensions: buildDimensionsBlock(alignment),
    consensus: buildConsensusBlock(alignment),
    per_timeframe: buildPerTimeframeBlock(alignment),
    score_calculation: buildScoreCalcBlock(alignment),
    interpretation: buildInterpretation(alignment),
    composition_note: buildCompositionNote(alignment),
    consensus_conflict_banner: buildConflictBanner(alignment),
  };
  return JSON.stringify(payload, null, 2);
}