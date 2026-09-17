// Risk tab builder — scoped export payload mirroring the Risk panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome). Adds
// `interpretation_full` as the screen paragraph, and reformats cascade
// telemetry into typed fields.
// v7.1: the header trailing headline ("1 extreme · 1 high · …") was
// removed from the panel (title-only header, like every other tab) and
// therefore stripped from the export too — `summary_counts` carries the
// per-level distribution for data consumers.

import type { RiskMatrix, RiskDimension, RiskLevel, LiquidityFlow, LiquidationClusterMatrix } from '../../types';
import { buildPriceBlock, buildHeaderBlock, type MetaEnvelope, type HeaderBlock, type InstanceTermsLike } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';

// ── Payload types ────────────────────────────────────────────────────────

export interface RiskHeroBlock {
  overall_score: number;
  overall_level: string;
  overall_state: string;
  overall_confidence: number;
  top_severity: RiskLevel | null;
}

export interface RiskSummaryCountsBlock {
  very_low: { label: string; count: number };
  low:      { label: string; count: number };
  moderate: { label: string; count: number };
  high:     { label: string; count: number };
  extreme:  { label: string; count: number };
}

export interface RiskCascadeExtras {
  state_label: string;
  /** Null when no flow telemetry is present — mirrors the panel hiding the
   *  Intensity field (RiskPanel.svelte) instead of rendering `0.0`. */
  intensity_display: string | null;
  asymmetry_sign: string;
  asymmetry_magnitude_pct: number | null;
  asymmetry_description: string | null;
  /** The exact badge sentence the screen renders, e.g. "↑35.0% (short squeeze)". */
  asymmetry_display: string | null;
}

export interface RiskDimensionExport {
  name: string;
  key: string;
  weight: number;
  weight_pct: number;
  score: number;
  level: string;
  state: string;
  state_display: string;
  confidence: number;
  evidence: string[];
  /** Screen chip for High/Extreme dims with no recorded evidence. */
  no_evidence_text: string | null;
  /** Screen placeholder for dims whose data feed is inactive. */
  not_active_text: string | null;
  /** True when the risk matrix is absent — mirrors the screen's "AWAITING"
   *  placeholder cards (name + weight only). */
  awaiting: boolean;
  /** Verbatim badge text shown for awaiting rows ("AWAITING"). */
  awaiting_badge: string | null;
  bar_pct: number;
  weight_mark_pct: number;
  is_cascade_dim: boolean;
  not_active: boolean;
  cascade_extras: RiskCascadeExtras | null;
  /** v6.11: execution-friction gauge on the Execution Risk dimension —
   *  ATR(14) ÷ top-of-book spread. `null` when absent (all other dims). */
  execution_extras: { volatility_to_spread_ratio: number | null } | null;
}

export interface RiskDisclosureBlock {
  weights: Array<{ label: string; pct: number }>;
  note: string;
}

export interface RiskPayload {
  source_tab: 'risk';
  meta: MetaEnvelope;
  header: HeaderBlock;
  hero: RiskHeroBlock | null;
  summary_counts: RiskSummaryCountsBlock;
  dimensions: RiskDimensionExport[];
  interpretation_full: string | null;
  disclosure: RiskDisclosureBlock;
  awaiting_dimensions_text: string;
}

// ── Constants (mirrors `RiskPanel.svelte::namedDims`) ────────────────────

const RISK_DIMENSION_DEFS: ReadonlyArray<{
  name: string;
  key: keyof RiskMatrix;
  weight: number;
  isCascade: boolean;
}> = [
  { name: 'Market Risk',              key: 'market_risk',              weight: 0.14, isCascade: false },
  { name: 'Volatility Risk',          key: 'volatility_risk',          weight: 0.14, isCascade: false },
  { name: 'Execution Liquidity Risk', key: 'execution_liquidity_risk', weight: 0.14, isCascade: false },
  { name: 'Structure Risk',           key: 'structure_risk',           weight: 0.10, isCascade: false },
  { name: 'Momentum Risk',            key: 'momentum_risk',            weight: 0.14, isCascade: false },
  { name: 'Signal Risk',              key: 'signal_risk',              weight: 0.10, isCascade: false },
  { name: 'Execution Risk',           key: 'execution_risk',           weight: 0.10, isCascade: false },
  { name: 'Cascade Risk',             key: 'cascade_risk',             weight: 0.14, isCascade: true  },
];

const LEVELS: RiskLevel[] = ['VeryLow', 'Low', 'Moderate', 'High', 'Extreme'];
const LEVEL_LABELS: Record<RiskLevel, string> = {
  VeryLow: 'Very Low',
  Low: 'Low',
  Moderate: 'Moderate',
  High: 'High',
  Extreme: 'Extreme',
};

/**
 * RK-D (v6.10.9): detect the backend's warmup sentinel — `RiskMatrix::empty`
 * fills every dimension AND the overall at exactly 50/Moderate with no
 * evidence. The panel treats that signature as "awaiting" (AWAITING cards +
 * null hero) instead of rendering fabricated "Moderate risk" data.
 */
export function isAwaitingRiskMatrix(risk: RiskMatrix | null | undefined): boolean {
  if (!risk) return true;
  const dims = [
    risk.market_risk,
    risk.volatility_risk,
    risk.execution_liquidity_risk,
    risk.structure_risk,
    risk.momentum_risk,
    risk.signal_risk,
    risk.execution_risk,
    risk.cascade_risk,
    risk.overall_risk,
  ];
  return dims.every(
    (d) =>
      !!d &&
      d.score === 50 &&
      d.level === 'Moderate' &&
      (d.evidence ?? []).length === 0,
  );
}

function levelRank(l: RiskLevel): number {
  return LEVELS.indexOf(l);
}

function normalizeLevelKey(l: string): string {
  return l ? l.toLowerCase().replace(/_/g, '') : 'moderate';
}

/** Count dimensions per severity level (lowercase keys). */
function countLevels(risk: RiskMatrix): Record<string, number> {
  const counts: Record<string, number> = {
    verylow: 0, low: 0, moderate: 0, high: 0, extreme: 0,
  };
  for (const def of RISK_DIMENSION_DEFS) {
    const dim = (risk as unknown as Record<string, RiskDimension | undefined>)[def.key];
    if (!dim) continue;
    const key = normalizeLevelKey(dim.level);
    if (key in counts) counts[key]++;
  }
  return counts;
}

// v6.10.19d (C): unified Scheme-A state tokens shared with RiskPanel —
// trend states win when present, otherwise the LEVEL maps to its token
// (Extreme→CRITICAL / High→ELEVATED / Moderate→STEADY / Low→COMPOSED /
// VeryLow→MINIMAL; trends RISING ↗ / IMPROVING ↘).
function riskToken(level: string | undefined, state: string | undefined): { token: string; icon: string } {
  const st = String(state ?? '').toLowerCase();
  if (st === 'increasing') return { token: 'RISING', icon: '↗' };
  if (st === 'improving') return { token: 'IMPROVING', icon: '↘' };
  switch (normalizeLevelKey(level ?? '')) {
    case 'verylow': return { token: 'MINIMAL', icon: '→' };
    case 'low':     return { token: 'COMPOSED', icon: '→' };
    case 'moderate':return { token: 'STEADY', icon: '→' };
    case 'high':    return { token: 'ELEVATED', icon: '↑' };
    case 'extreme': return { token: 'CRITICAL', icon: '⚠' };
    default:        return { token: 'STEADY', icon: '→' };
  }
}

function buildHeroBlock(risk: RiskMatrix): RiskHeroBlock {
  const overall = risk.overall_risk;
  const dimCounts = countLevels(risk);
  let topSeverity: RiskLevel | null = null;
  if (dimCounts.extreme > 0) topSeverity = 'Extreme';
  else if (dimCounts.high > 0) topSeverity = 'High';
  else if (dimCounts.moderate > 0) topSeverity = 'Moderate';
  else if (dimCounts.low > 0) topSeverity = 'Low';
  else topSeverity = 'VeryLow'; // mirrors RiskPanel.svelte (unconditional fallback)
  // The screen hides the "peak" chip when the top severity equals the
  // overall level — mirror that so the JSON says what the screen shows.
  if (topSeverity === overall.level) topSeverity = null;

  return {
    overall_score: Math.round(overall.score),
    overall_level: overall.level,
    overall_state: overall.state,
    overall_confidence: Math.round(overall.confidence),
    top_severity: topSeverity,
  };
}

function buildSummaryCounts(risk: RiskMatrix | null): RiskSummaryCountsBlock {
  const counts: RiskSummaryCountsBlock = {
    very_low: { label: 'Very Low', count: 0 },
    low:      { label: 'Low',      count: 0 },
    moderate: { label: 'Moderate', count: 0 },
    high:     { label: 'High',     count: 0 },
    extreme:  { label: 'Extreme',  count: 0 },
  };
  if (!risk) return counts;
  const c = countLevels(risk);
  counts.very_low.count = c.verylow;
  counts.low.count = c.low;
  counts.moderate.count = c.moderate;
  counts.high.count = c.high;
  counts.extreme.count = c.extreme;
  return counts;
}

function buildCascadeExtras(
  flow: LiquidityFlow | null,
  cluster: LiquidationClusterMatrix | null,
): RiskCascadeExtras | null {
  // The panel renders the cascade section only when flow or cluster exists
  // (RiskPanel.svelte) — emit null instead of placeholder values.
  if (!flow && !cluster) return null;
  const asym = cluster?.cascade_asymmetry;
  const sign = asym != null && asym > 0 ? '+' : asym != null && asym < 0 ? '-' : '';
  // The screen renders the magnitude as a percentage: `(asym * 100).toFixed(1)`.
  const magnitude = asym != null ? Math.abs(asym) * 100 : null;
  // v2026-08: display-band parity with LiquidityPanel / metricsTab — the
  // ±0.3 dead-band (03-02-11 L4/L5 threshold) and the canonical
  // SHORT_SQUEEZE_RISK / LONG_SQUEEZE_RISK vocabulary.
  const description =
    asym == null
      ? null
      : asym > 0.3
        ? 'SHORT_SQUEEZE_RISK'
        : asym < -0.3
          ? 'LONG_SQUEEZE_RISK'
          : 'NEUTRAL';
  const display =
    asym == null
      ? null
      : asym > 0.3
        ? `↑${(asym * 100).toFixed(1)}% (SHORT_SQUEEZE_RISK)`
        : asym < -0.3
          ? `↓${(Math.abs(asym) * 100).toFixed(1)}% (LONG_SQUEEZE_RISK)`
          : '0.0% (NEUTRAL)';
  return {
    state_label:
      !flow?.cascade_state || String(flow.cascade_state).toUpperCase() === 'NONE'
        ? '—'
        : flow.cascade_state,
    intensity_display:
      flow?.cascade_intensity != null ? flow.cascade_intensity.toFixed(1) : null,
    asymmetry_sign: sign,
    asymmetry_magnitude_pct: magnitude,
    asymmetry_description: description,
    asymmetry_display: display,
  };
}

function buildDimensionsBlock(
  risk: RiskMatrix | null,
  flow: LiquidityFlow | null,
  cluster: LiquidationClusterMatrix | null,
): RiskDimensionExport[] {
  if (!risk) {
    // Mirror the screen's awaiting placeholder cards (name + weight +
    // "AWAITING" badge + per-dimension paragraph) — 8 rows, def order.
    return RISK_DIMENSION_DEFS.map((def) => ({
      name: def.name,
      key: def.key,
      weight: def.weight,
      weight_pct: Math.round(def.weight * 100),
      score: 0,
      not_active: false,
      awaiting: true,
      awaiting_badge: 'AWAITING',
      level: 'UNKNOWN',
      state: 'UNKNOWN',
      state_display: '\u2192 UNKNOWN',
      confidence: 0,
      evidence: [],
      no_evidence_text: null,
      not_active_text: null,
      bar_pct: 0,
      weight_mark_pct: Math.round(def.weight * 100),
      is_cascade_dim: def.isCascade,
      cascade_extras: def.isCascade ? buildCascadeExtras(flow, cluster) : null,
      execution_extras: null,
    }));
  }
  const decorated = RISK_DIMENSION_DEFS.map((def) => {
    const dim = (risk as unknown as Record<string, RiskDimension | undefined>)[def.key];
    return { def, dim };
  });
  decorated.sort((a, b) => {
    const sa = a.dim?.score ?? -1;
    const sb = b.dim?.score ?? -1;
    if (sb !== sa) return sb - sa;
    const la = a.dim ? levelRank(a.dim.level) : -1;
    const lb = b.dim ? levelRank(b.dim.level) : -1;
    return lb - la;
  });

  return decorated.map(({ def, dim }) => ({
    name: def.name,
    key: def.key,
    weight: def.weight,
    weight_pct: Math.round(def.weight * 100),
    score: Math.round(dim?.score ?? 0),
    not_active: !dim,
    awaiting: false,
    awaiting_badge: null,
    level: dim?.level ?? 'UNKNOWN',
    state: dim?.state ?? 'UNKNOWN',
    // v6.10.19d (C): state_display mirrors the screen's Scheme-A badge
    // (`{icon} {TOKEN}`), e.g. "⚠ CRITICAL" or "↗ RISING".
    state_display: (() => {
      const t = riskToken(dim?.level, dim?.state);
      return `${t.icon} ${t.token}`;
    })(),
    confidence: Math.round(dim?.confidence ?? 0),
    evidence: dim?.evidence ?? [],
    // Screen chip for High/Extreme dims that carry no recorded evidence.
    no_evidence_text:
      dim && (dim.level === 'High' || dim.level === 'Extreme') && (dim.evidence ?? []).length === 0
        ? 'No evidence recorded'
        : null,
    // Screen placeholder for inactive dimensions.
    not_active_text: !dim ? 'Data feed inactive for this dimension.' : null,
    bar_pct: Math.min(dim?.score ?? 0, 100),
    weight_mark_pct: Math.round(def.weight * 100),
    is_cascade_dim: def.isCascade,
    cascade_extras: def.isCascade ? buildCascadeExtras(flow, cluster) : null,
    execution_extras:
      def.key === 'execution_risk' && dim?.volatility_to_spread_ratio != null
        ? { volatility_to_spread_ratio: dim.volatility_to_spread_ratio }
        : null,
  }));
}

function buildInterpretationFull(risk: RiskMatrix | null): string | null {
  if (!risk) {
    // Verbatim screen copy — the empty-state interpretation paragraph the
    // panel renders when no risk matrix has arrived yet.
    return 'Risk synthesis is initializing — this section will provide a human-readable summary of the overall risk environment, highlighting which dimensions require attention and suggesting position-sizing guidance.';
  }
  const c = countLevels(risk);
  const overall = risk.overall_risk;
  const overallLevel = overall.level.toLowerCase().replace(/_/g, ' ');
  const confidence = Math.round(overall.confidence);
  let body: string;
  if (c.extreme > 0 || c.high > 0) {
    // Zero-count sentences are omitted on screen — mirror that exactly.
    body = `<strong>Elevated risk environment.</strong>`;
    if (c.extreme > 0) {
      body += ` ${c.extreme} dimension${c.extreme === 1 ? '' : 's'} at extreme levels.`;
    }
    if (c.high > 0) {
      body += ` ${c.high} dimension${c.high === 1 ? '' : 's'} at high levels.`;
    }
    body += ` Consider reduced position sizing and wider stops. Monitor the highest-severity dimensions for evidence of improvement before committing capital.`;
  } else if (c.moderate > 0) {
    body = `<strong>Moderate risk environment.</strong> ${c.moderate} dimension${c.moderate === 1 ? '' : 's'} at moderate levels. Standard position sizing applies, but stay alert to dimensions trending toward higher severity.`;
  } else {
    body = `<strong>Low risk environment.</strong> All dimensions are within acceptable bounds. Favorable conditions for disciplined execution with standard risk parameters.`;
  }
  return `${body} Overall composite score is <strong>${overallLevel}</strong> at ${confidence}% confidence.`;
}

// ── Public builder ───────────────────────────────────────────────────────

export interface RiskTabInputs {
  risk: RiskMatrix | null;
  flow: LiquidityFlow | null;
  cluster: LiquidationClusterMatrix | null;
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
 * Build the Risk tab export payload. Mirrors `RiskPanel.svelte` 1:1.
 */
export function buildRiskTabExport(args: RiskTabInputs): string {
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  // RK-D (v6.10.9): the warmup sentinel matrix renders as AWAITING —
  // mirror the panel's gate so the export can never claim real
  // "Moderate" data from the backend's empty matrix.
  const risk = isAwaitingRiskMatrix(args.risk) ? null : args.risk;
  // v6.16: labels mirror the segmented weight strip's full names (the
  // screen renders segment widths + hover tooltips; the export keeps the
  // structured weights for data consumers).
  const disclosure: RiskDisclosureBlock = {
    weights: [
      { label: 'Market', pct: 14 },
      { label: 'Volatility', pct: 14 },
      { label: 'Execution Liquidity', pct: 14 },
      { label: 'Structure', pct: 10 },
      { label: 'Momentum', pct: 14 },
      { label: 'Signal', pct: 10 },
      { label: 'Execution', pct: 10 },
      { label: 'Cascade', pct: 14 },
    ],
    note: 'Overall risk is a weighted sum of the eight dimension scores. Hover a segment for its full name and weight. The state chip describes the risk trend (elevating / improving / stable); it does not change the score.',
  };
  const baseHero = risk ? buildHeroBlock(risk) : null;
  // v6.10.19d (C): the "Lower is safer." caption was removed from the
  // hero — the guidance now lives on the bar's tooltip only, so the
  // payload carries the data fields and no `hint` string.
  const hero = baseHero;
  const payload: RiskPayload = {
    source_tab: 'risk',
    meta,
    header: { ...buildHeaderBlock(args.headerSpec), summary_label: 'SUMMARY' },
    hero,
    summary_counts: buildSummaryCounts(risk),
    dimensions: buildDimensionsBlock(risk, args.flow, args.cluster),
    interpretation_full: buildInterpretationFull(risk),
    disclosure,
    awaiting_dimensions_text: 'Awaiting risk assessment — this dimension will populate once market data stabilizes.',
  };
  return JSON.stringify(payload, null, 2);
}