// Recommendation tab builder — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome) and the
// new R:R availability helper (replaces `rr: null` with `{available, value, reason}`).

import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, OpportunityMatrix, RiskDimension } from '../../types';
import { computeDecisionRank, resolveActiveRr, riskAdjRrExplanation, topSetupSummary, entryDangerLevel, buildVerdictSentence, type AlternateSetupInfo } from '../../lib/decisionRank';
import { buildPriceBlock, buildHeaderBlock, buildRrBlock, type MetaEnvelope, type HeaderBlock, type RrBlock } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { emptyProjection, type ProjectionState } from '../projection';

// ── Payload types ────────────────────────────────────────────────────────

export interface RecommendationEnvironmentBlock {
  directional_guidance: string;
  market_stance: string;
  strategy_environment: string;
  opportunity_classification: string;
  confidence_pct: number;
  readiness: string;
  entry_danger_score: number;
  entry_danger_level: string;
  /** v6.11: setup-efficiency metric — market quality ÷ overall risk.
   *  `null` until the advisory carries a value (or overall risk = 0). */
  quality_to_risk_ratio: number | null;
  quality_to_risk_ratio_display: string;
}

export interface RecommendationVerdictBlock {
  top: 'LONG' | 'SHORT' | 'HOLD';
  long_probability: number;
  short_probability: number;
  hold_probability: number;
}

export interface TopSetupBlock {
  opportunity_type: string;
  viability: string;
  badge_text: string;
  score: number;
  /** v6.14: the screen card's number — backend `display_score` when
   *  present (precondition-scaled), raw `score` on legacy payloads.
   *  Verbatim copy of the card face; `score` above stays raw. */
  score_display: number;
  preconditions_met: number;
  preconditions_total: number;
  direction_label: string;
  entry_zone: { low: number; high: number } | null;
  target_zone: { low: number; high: number } | null;
  invalidation: number | null;
  /** Display strings — verbatim copies of the screen card values. */
  entry_zone_display: string;
  target_zone_display: string;
  invalidation_display: string;
  rr_display: string;
  rr_available: boolean;
  rr_value: number | null;
  rr_reason: string | null;
  rationale: string;
  /** v6.10.19b (B3): the unified SETUP block also carries the horizon
   *  (previously only in `price_levels`). */
  horizon: string | null;
  /** v6.10.19b (B1): qualifying setups that did not make the verdict-
   *  consistent headline (counter-bias / any setup under HOLD) — they
   *  always appear on the Opportunities panel. */
  alternate_qualifying_setups: AlternateSetupInfo[];
  /** v6.10.19b (B3): the unified SETUP note under a HOLD verdict
   *  (null under directional verdicts). */
  hold_placeholder: string | null;
}

function viabilityBadgeText(viability: string, top: string, belowFloor = false): string {
  // Mirrors `RecommendationPanel.svelte` — emits the same literal badge
  // text the screen shows. Empty string when the screen shows nothing
  // (e.g. Actionable + HOLD verdict).
  // v6.10.19 (T3): a sub-1.0 reference bracket is NEVER framed as a trade.
  if (belowFloor) return 'R:R BELOW ACTIONABLE FLOOR';
  if (viability === 'Actionable' && top !== 'HOLD') return 'ACTIONABLE';
  if (viability === 'Qualifying') return 'QUALIFYING';
  if (viability === 'DirectionalNeutral') return 'HOLD · NO DIRECTIONAL EDGE';
  if (viability === 'GeometryInverted') return 'GEOMETRY INVERTED';
  if (viability === 'NoClear') return 'NO CLEAR SETUP';
  return '';
}

export interface SafetyFlagsBlock {
  readiness: string;
  rr_available: boolean;
  rr_value: number | null;
  rr_reason: string | null;
  /** The first-class R:R discount explanation (RR-008) — the same sentence
   *  the L6 header chip tooltip renders. `null` when there is no real
   *  risk-adjusted R:R. */
  risk_adj_rr_explanation: string | null;
  stop_loss_pct: number;
  confidence_pct: number;
  entry_danger_score: number;
  entry_danger_level: string;
  /** Display strings — verbatim copies of the screen KPI chips. */
  rr_display: string;
  stop_loss_display: string;
  confidence_display: string;
  entry_danger_display: string;
  /** v6.11: Quality/Risk KPI chip (mirrors the Environment block value). */
  quality_to_risk_ratio: number | null;
  quality_to_risk_ratio_display: string;
}

export interface GaugeBlock {
  net_bias_pct: number;
  bias_direction: string;
  long_pct: number;
  short_pct: number;
  hold_pct: number;
  /** v6.10.19 (P6): the graded-lean floors adjusted this split — the
   *  directional read is structurally boosted (LEAN annotation). */
  lean_floor_applied: boolean;
  /** Verbatim copy of the gauge needle label ("+37%" / "0%" / "-14%").
   *  v6.10.17: the needle is verdict-consistent — neutral only when the
   *  top action is HOLD; a directional lean gated by STAND ASIDE keeps
   *  its real net-bias display. */
  net_bias_display: string;
}

export interface RecommendationPayload {
  source_tab: 'recommendation';
  meta: MetaEnvelope;
  header: HeaderBlock;
  gauge: GaugeBlock;
  environment: RecommendationEnvironmentBlock;
  verdict: RecommendationVerdictBlock;
  top_setup: TopSetupBlock | null;
  /** Verbatim section-meta caption shown when no qualifying setup exists
   *  ("no qualifying setup yet") — null when a setup renders. */
  top_setup_empty_text: string | null;
  safety_flags: SafetyFlagsBlock;
  why_note: string | null;
  why: string[];
  price_levels: {
    side: 'long' | 'short' | 'hold';
    entry_zone: { low: number; high: number } | null;
    target_zone: { low: number; high: number } | null;
    invalidation: number | null;
    horizon: string;
    hold_placeholder: string | null;
  };
  strategy: {
    entry: string;
    exit: string;
    protection: string;
    target: string;
  };
  /** Verdict-consistent final verdict (HOLD verdict → verdict sentence). */
  final_verdict: string;
  /** Advisory environment guidance rendered below the verdict under HOLD. */
  final_verdict_guidance: string | null;
  /** v7.0: Project Risk and Return drawer state — `configured: false`
   *  with null numerics until the operator runs a what-if calculation on
   *  the Recommendation panel; populated verbatim from the drawer once
   *  configured (export/screen parity). */
  projection: ProjectionState;
}

// ── Helpers ──────────────────────────────────────────────────────────────

/**
 * v6.10.19 (T2 + T5) + v6.17: verdict-consistent environment guidance.
 * Under a genuine HOLD top the guidance sentence must say exactly what the
 * 0% needle means — no "Long bias: …" claim leading the sentence, no
 * "Entry: …. Stop: …." execution instructions. v6.17: under a DIRECTIONAL
 * verdict the guidance leads with the verdict's own read — same direction
 * and the same probability as the headline (`rank.top_prob`) — so the
 * guidance line can never contradict the verdict quote; the advisory's
 * environment tail survives verbatim, and "Entry:/Stop:" clauses are
 * stripped under every verdict. Shared by the panel and the export so
 * screen and clipboard can never disagree.
 */
export function verdictAwareGuidance(
  advisory: AdvisoryMatrix | null,
  top: 'LONG' | 'SHORT' | 'HOLD',
  verdictPct: number,
): string | null {
  if (!advisory?.final_recommendation) return null;
  let g = advisory.final_recommendation;
  // Execution instructions are never environment guidance — strip them
  // under every verdict (v6.17; previously HOLD-only).
  g = g.replace(/\s*Entry: [^.]*\.?\s*Stop: [^.]*\.?\s*$/i, '');
  g = g.replace(/\s*Entry: [^.]*\.?\s*$/i, '');
  if (top === 'HOLD') {
    // Reword the leading directional claim — under HOLD the read exists
    // but the edge does not.
    const guidance = advisory.directional_guidance ?? '';
    const conf = Math.round(advisory.confidence_assessment ?? 0);
    g = g
      .replace(/^Strong long bias:/i, `${guidance} bias at ${conf}% — no actionable directional edge;`)
      .replace(/^Long bias:/i, `BULLISH bias at ${conf}% — no actionable directional edge;`)
      .replace(/^Strong short bias:/i, `${guidance} bias at ${conf}% — no actionable directional edge;`)
      .replace(/^Short bias:/i, `BEARISH bias at ${conf}% — no actionable directional edge;`);
    // Drop the duplicated "… bias with N% confidence" fragment that follows
    // the reworded claim (the confidence already lives in the new claim).
    g = g.replace(/,?\s*[A-Za-z]+ bias with \d+% confidence/i, '');
    // v6.10.19a (D2a): the strip can leave an orphaned ":," behind
    // ("…no actionable directional edge:, cautious…") — collapse both.
    g = g.replace(/:\s*,\s*/g, ', ');
    g = g.replace(/\s*:\s*$/g, '');
    return g.trim();
  }

  // ── Directional verdict (v6.17): lead with the verdict's own read ──
  const claim = top === 'LONG' ? 'Bullish' : 'Bearish';
  const tail = g
    // legacy directional claim head: "Strong long bias:", "Long bias:", …
    .replace(/^(Strong\s+)?(Long|Short|Bearish|Bullish)\s+bias\s*:\s*/i, '')
    // neutral claim head: "Neutral — no directional edge: …"
    .replace(/^Neutral\s*—\s*no directional edge\s*:?\s*/i, '')
    // duplicated "X bias with N% confidence" fragment right after the head
    .replace(/^[A-Za-z]+\s+bias\s+with\s+\d+%\s+confidence\s*[,.]?\s*/i, '')
    // bare "N% confidence," fragment right after the head
    .replace(/^\d+%\s+confidence\s*[,.]?\s*/i, '')
    // v6.10.19a (D2a) artifact collapse
    .replace(/:\s*,\s*/g, ', ')
    .replace(/\s*:\s*$/g, '')
    .trim()
    .replace(/^[\s,.;]+/, '');
  const pct = Math.round(verdictPct);
  const head = `${claim} market bias with ${pct}% confidence`;
  return tail ? `${head}, ${tail}` : `${head}.`;
}

function sanitizeLabel(s: string): string {
  // Screen renders "—" for falsy input — mirror it.
  if (!s) return '\u2014';
  let cleaned = s.replace(/([a-z])([A-Z])/g, '$1 $2');
  cleaned = cleaned.replace(/_/g, ' ');
  cleaned = cleaned.trim().replace(/\s+/g, ' ');
  return cleaned
    .toLowerCase()
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function prettifyEnum(s: string): string {
  // Screen renders "—" for falsy input — mirror it.
  if (!s) return '\u2014';
  let cleaned = s.replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2');
  cleaned = cleaned.replace(/([a-z])([A-Z])/g, '$1 $2');
  cleaned = cleaned.replace(/_/g, ' ');
  cleaned = cleaned.trim().replace(/\s+/g, ' ');
  cleaned = cleaned
    .toLowerCase()
    .replace(/\b\w/g, (c) => c.toUpperCase());
  cleaned = cleaned.replace(/\sBased$/i, '-Based');
  cleaned = cleaned
    .replace(/^Atr\b/i, 'ATR')
    .replace(/^Sr\b/i, 'S/R')
    .replace(/^Rr\b/i, 'R:R')
    .replace(/^Sl\b/i, 'SL');
  return cleaned;
}

function readEntryDangerScore(decisionContext: DecisionContext | null): number {
  if (!decisionContext) return 50;
  const danger = decisionContext.entry_danger;
  // Screen default: missing entry_danger reads 50 (MODERATE).
  if (typeof danger === 'number') return danger;
  return (danger as RiskDimension | null)?.score ?? 50;
}

/** Price formatter — mirrors `RecommendationPanel.svelte::fmtPriceScale`. */
function fmtPriceScale(n: number, mp: number): string {
  if (mp >= 1000) return n.toFixed(0);
  if (mp >= 1) return n.toFixed(2);
  if (mp >= 0.01) return n.toFixed(4);
  if (mp >= 0.0001) return n.toFixed(6);
  return n.toFixed(8);
}

/** Risk-Adjusted Reward-to-Risk KPI string — mirrors the screen chip exactly. */
function rrKpiDisplay(rrRaw: number, isNA: boolean): string {
  if (isNA) return '\u2014';
  if (Number.isNaN(rrRaw) || rrRaw <= 0) return '\u2014';
  const norm = rrRaw >= 9.99 ? '9.99+' : rrRaw >= 5 ? rrRaw.toFixed(1) : rrRaw.toFixed(2);
  return norm;
}

function buildEnvironmentBlock(
  advisory: AdvisoryMatrix | null,
  decisionContext: DecisionContext | null,
  readiness: string,
): RecommendationEnvironmentBlock {
  const score = readEntryDangerScore(decisionContext);
  const qualityToRisk = advisory?.quality_to_risk_ratio ?? null;
  return {
    directional_guidance: advisory?.directional_guidance ?? '',
    market_stance: advisory?.market_stance ?? '',
    strategy_environment: advisory?.strategy_environment ?? '',
    opportunity_classification: advisory?.opportunity_classification ?? '',
    confidence_pct: advisory?.confidence_assessment ?? 0,
    readiness,
    entry_danger_score: score,
    entry_danger_level: entryDangerLevel(score),
    // v6.11: mirrors the Quality/Risk KPI chip (2-dp, em-dash placeholder).
    quality_to_risk_ratio: qualityToRisk,
    quality_to_risk_ratio_display: qualityToRisk != null ? qualityToRisk.toFixed(2) : '\u2014',
  };
}

function buildVerdictBlock(
  rank: ReturnType<typeof computeDecisionRank>,
): RecommendationVerdictBlock {
  return {
    top: rank.top,
    long_probability: rank.long.probability,
    short_probability: rank.short.probability,
    hold_probability: rank.hold.probability,
  };
}

function buildGaugeBlock(
  rank: ReturnType<typeof computeDecisionRank>,
): GaugeBlock {
  const netBias = rank.long.probability - rank.short.probability;
  return {
    net_bias_pct: netBias,
    bias_direction: rank.long.probability > rank.short.probability
      ? 'LONG'
      : rank.short.probability > rank.long.probability
        ? 'SHORT'
        : 'NEUTRAL',
    long_pct: rank.long.probability,
    short_pct: rank.short.probability,
    hold_pct: rank.hold.probability,
    net_bias_display: netBias === 0 ? '0%' : `${netBias > 0 ? '+' : ''}${netBias}%`,
    lean_floor_applied: rank.lean_floor_applied === true,
  };
}

function buildTopSetupBlock(
  summary: ReturnType<typeof topSetupSummary>,
  markPrice: number,
  top: 'LONG' | 'SHORT' | 'HOLD',
): TopSetupBlock | null {
  if (!summary) return null;
  const z = summary.zones;
  const viability =
    summary.viability === 'Actionable'
      ? 'Actionable'
      : summary.viability === 'Qualifying'
        ? 'Qualifying'
        : summary.viability === 'DirectionalNeutral'
        ? 'DirectionalNeutral'
        : summary.viability === 'GeometryInverted'
          ? 'GeometryInverted'
          : 'NoClear';
  // v6.10.19 (T3): a sub-1.0 reference bracket is never framed as a trade.
  const belowFloor = summary.below_floor === true;
  const rr = buildRrBlock(summary.rr, 'no_actionable_setup');
  const entryDisplay = z
    ? `$${fmtPriceScale(z.entry.low, markPrice)}–$${fmtPriceScale(z.entry.high, markPrice)}`
    : '\u2014';
  const targetDisplay = z
    ? (z.target.low > 0
        ? `$${fmtPriceScale(z.target.low, markPrice)}–$${fmtPriceScale(z.target.high, markPrice)}`
        : `$${fmtPriceScale(z.target.high, markPrice)}`)
    : '\u2014';
  const invalidationDisplay =
    z && z.invalidation > 0
      ? `$${fmtPriceScale(z.invalidation, markPrice)}`
      : '\u2014';
  // R:R display derives from the canonical `summary.rr` (the shared
  // resolver: profile wire → matrix wire → aligned zones fallback) with
  // the same formatting as the header chip. When N/A, the resolver's
  // human-readable reason rides in `rr_reason`.
  let rrDisplay: string;
  if (summary.rr == null) {
    rrDisplay = '\u2014';
  } else {
    rrDisplay = rrKpiDisplay(summary.rr, false);
  }
  const rrReason = summary.rr != null ? null : (summary.rr_reason ?? 'no actionable setup');
  return {
    opportunity_type: sanitizeLabel(summary.opportunity_type),
    viability,
    badge_text:
      summary.opportunity_type === 'NoActiveSetup'
        ? ''
        : viabilityBadgeText(viability, top, belowFloor),
    score: summary.score,
    score_display: summary.display_score ?? summary.score,
    preconditions_met: summary.preconditions_met,
    preconditions_total: summary.preconditions_total,
    direction_label:
      summary.direction === 'LONG' ? 'LONG'
      : summary.direction === 'SHORT' ? 'SHORT' : 'NEUTRAL',
    entry_zone: z ? { low: z.entry.low, high: z.entry.high } : null,
    target_zone: z ? { low: z.target.low, high: z.target.high } : null,
    invalidation: z?.invalidation ?? null,
    entry_zone_display: entryDisplay,
    target_zone_display: targetDisplay,
    invalidation_display: invalidationDisplay,
    rr_display: rrDisplay,
    rr_available: rr.available,
    rr_value: rr.value,
    rr_reason: rrReason,
    rationale: summary.rationale,
    // v6.10.19b (B3): the unified SETUP block is the single price-levels
    // source — horizon + hold note ride here (the separate `price_levels`
    // block below is now an alias of the SAME zones so the two can never
    // disagree).
    horizon: summary.horizon,
    alternate_qualifying_setups: summary.alternate_setups,
    // v6.10.19d (D): the "fields are placeholders" caveat is gone.
    hold_placeholder: top === 'HOLD' ? 'No active setup.' : null,
  };
}

function buildSafetyFlagsBlock(
  decisionContext: DecisionContext | null,
  advisory: AdvisoryMatrix | null,
  readiness: string,
  topAction: 'LONG' | 'SHORT' | 'HOLD',
  opportunity: OpportunityMatrix | null,
  overallRisk: number | null,
  topSetup: TopSetupBlock | null,
): SafetyFlagsBlock {
  // v6.10.19c (D4): the Risk-Adj R:R is bracket-aware — whenever the
  // container has a bracket (incl. Neutral/Qualifying) the discounted
  // ratio shows: backend `expected_reward_risk_ratio` if > 0, else
  // `container bracket R:R × (1 − overall_risk/100)`. N/A only when
  // there is genuinely no bracket or the ratio < 0.10 floor.
  const rrRaw = decisionContext?.expected_reward_risk_ratio ?? 0;
  let rrValue: number | null = rrRaw > 0 ? rrRaw : null;
  let rrReason: string | null = rrRaw > 0 ? null : 'no_wire_rr';
  let rrExplanation: string | null = null;
  if (rrRaw > 0) {
    const geometricRr = resolveActiveRr(opportunity, decisionContext).value;
    rrExplanation = riskAdjRrExplanation(geometricRr, rrRaw);
  } else if (topSetup?.rr_value != null && overallRisk != null && overallRisk > 0 && overallRisk < 100) {
    const adj = topSetup.rr_value * (1 - overallRisk / 100);
    if (adj >= 0.1) {
      rrValue = Math.round(adj * 100) / 100;
      rrReason = null;
      rrExplanation = riskAdjRrExplanation(topSetup.rr_value, rrValue);
    }
  }
  const rr = buildRrBlock(rrValue, rrReason ?? 'no_wire_rr');
  const score = readEntryDangerScore(decisionContext);
  const stopLossPct = advisory?.stop_loss_distance_pct ?? 0;
  const confidence = advisory?.confidence_assessment ?? 0;
  const entryDangerLevelVal = entryDangerLevel(score);
  const rrNotApplicable = rrValue == null;
  const qualityToRisk = advisory?.quality_to_risk_ratio ?? null;
  return {
    readiness,
    rr_available: rr.available,
    rr_value: rr.value,
    rr_reason: rr.reason,
    risk_adj_rr_explanation: rrExplanation,
    stop_loss_pct: stopLossPct,
    confidence_pct: confidence,
    entry_danger_score: score,
    entry_danger_level: entryDangerLevelVal,
    // Verbatim screen chips:
    rr_display: rrKpiDisplay(rrValue ?? 0, rrNotApplicable),
    stop_loss_display: stopLossPct > 0 ? `${stopLossPct.toFixed(2)}%` : '\u2014',
    confidence_display: `${confidence.toFixed(0)}%`,
    entry_danger_display: `${score.toFixed(0)} (${entryDangerLevelVal})`,
    // v6.11: Quality/Risk KPI chip — verbatim screen value (2-dp).
    quality_to_risk_ratio: qualityToRisk,
    quality_to_risk_ratio_display: qualityToRisk != null ? qualityToRisk.toFixed(2) : '\u2014',
  };
}

function buildWhyNote(
  rank: ReturnType<typeof computeDecisionRank>,
): string | null {
  // v6.10.17: the note applies ONLY under a genuine HOLD top — a
  // directional lean (even gated by STAND ASIDE or coexisting with the
  // No Clear explanation card) has real directional meaning and must
  // never carry the "no directional edge" disclaimer.
  if (rank.top === 'HOLD') {
    return 'No directional edge — these bullets read the same across all three arms (Long/Short/Hold). They trace the data, not a trade call.';
  }
  return null;
}

// v6.10.19b (B3): `price_levels` is now an ALIAS of the unified SETUP
// block — it derives from the same verdict-consistent summary so the two
// can never disagree (the panel renders one SETUP section at the top).
function buildPriceLevelsBlock(
  summary: ReturnType<typeof topSetupSummary>,
  topAction: 'LONG' | 'SHORT' | 'HOLD',
  opportunity: OpportunityMatrix | null,
): RecommendationPayload['price_levels'] {
  const z = summary?.zones ?? null;
  const side = summary?.direction === 'LONG'
    ? 'long'
    : summary?.direction === 'SHORT'
      ? 'short'
      : 'hold';
  return {
    side: topAction === 'HOLD' ? 'hold' : side,
    entry_zone: z ? { low: z.entry.low, high: z.entry.high } : null,
    target_zone: z ? { low: z.target.low, high: z.target.high } : null,
    invalidation: z?.invalidation ?? null,
    horizon: summary?.horizon ?? opportunity?.time_horizon ?? '\u2014',
    hold_placeholder: topAction === 'HOLD' ? 'No active setup.' : null,
  };
}

function buildStrategyBlock(
  advisory: AdvisoryMatrix | null,
  noActiveCall: boolean,
): RecommendationPayload['strategy'] {
  // Entry/Exit use the sanitizeLabel title-casing the screen renders;
  // Protection/Target use prettifyEnum with the -Based / ATR / S-R /
  // R:R / SL overrides. Under a genuine HOLD verdict (no active directional
  // call) the actionable-sounding values are replaced with "—"
  // (FIX-O5 v6.10.16). v6.10.17: a directional lean gated by STAND ASIDE is
  // a REAL lean — its playbook values render real. The advisory text
  // survives in `final_verdict_guidance`.
  return {
    entry: noActiveCall ? '\u2014' : sanitizeLabel(advisory?.entry_guidance ?? ''),
    exit: noActiveCall ? '\u2014' : sanitizeLabel(advisory?.exit_guidance ?? ''),
    protection: noActiveCall ? '\u2014' : prettifyEnum(advisory?.protection_strategy ?? ''),
    target: noActiveCall ? '\u2014' : prettifyEnum(advisory?.target_strategy ?? ''),
  };
}

// ── Public builder ───────────────────────────────────────────────────────

export interface RecommendationTabInputs {
  advisory: AdvisoryMatrix | null;
  decisionContext: DecisionContext | null;
  opportunity: OpportunityMatrix | null;
  analysis: AnalysisMatrix | null;
  symbol: string;
  exchange?: string;
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  /** v6.10.19c (D4): the L5 overall risk for the bracket-aware Risk-Adj
   *  R:R fallback (instance risk matrix). */
  overallRisk?: number | null;
  /** v7.0: Project Risk and Return drawer state. Omit / leave unset to
   *  export the empty (`configured: false`) projection block. */
  projection?: ProjectionState | null;
  isCompleted?: boolean;
  terms?: import('./shared').InstanceTermsLike;
  headerSpec: LayerHeaderSpec;
}

/**
 * Build the Recommendation tab export payload. Mirrors
 * `RecommendationPanel.svelte` 1:1.
 */
export function buildRecommendationTabExport(args: RecommendationTabInputs): string {
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  const rank = computeDecisionRank({
    advisory: args.advisory,
    decisionContext: args.decisionContext,
    opportunity: args.opportunity,
    analysis: args.analysis,
  });
  // v6.10.19b (B1): the headline is VERDICT-consistent — the best
  // qualifying profile on the verdict side, else the verdict-side
  // aggregated reference bracket; counter-bias qualifying setups ride in
  // `alternate_qualifying_setups`. The summary is computed ONCE here and
  // shared with `price_levels` so the export can never disagree with
  // itself.
  const topSummary = topSetupSummary(args.opportunity, args.analysis, args.decisionContext, rank.top, args.markPrice ?? 0);
  const topSetup = buildTopSetupBlock(topSummary, args.markPrice ?? 0, rank.top);
  // FIX-4/FIX-5 (v6.10.15) + v6.10.17 decoupling: "no active directional
  // call" applies ONLY under a genuine HOLD verdict — a directional lean
  // gated by STAND ASIDE carries a real (graded) read whose sentence
  // reports both the lean and the gate.
  const noActiveCall = rank.top === 'HOLD';
  const header = { ...buildHeaderBlock(args.headerSpec), summary_label: 'VERDICT & RATIONALE' };
  const score = readEntryDangerScore(args.decisionContext);
  // v6.17: the verdict sentence is the shared builder (same string as the
  // panel — readiness in sentence case, `Entry Danger <level>` spelled).
  const verdictSentence = buildVerdictSentence(rank, score);
  const payload: RecommendationPayload = {
    source_tab: 'recommendation',
    meta,
    header,
    // v6.10.19c (D1): payload order mirrors the panel — dial → setup →
    // safety → playbook → verdict → why (bottom).
    gauge: buildGaugeBlock(rank),
    environment: buildEnvironmentBlock(args.advisory, args.decisionContext, rank.headline.state),
    verdict: buildVerdictBlock(rank),
    top_setup: topSetup,
    // Verbatim section-meta caption the panel renders when no setup exists
    // (top_setup null — opportunity matrix absent).
    top_setup_empty_text: topSetup ? null : 'no qualifying setup yet',
    safety_flags: buildSafetyFlagsBlock(args.decisionContext, args.advisory, rank.headline.state, rank.top, args.opportunity, args.overallRisk ?? null, topSetup),
    price_levels: buildPriceLevelsBlock(topSummary, rank.top, args.opportunity),
    strategy: buildStrategyBlock(args.advisory, noActiveCall),
    // R6 + FIX-4 + v6.10.17: the final verdict is the verdict — under a
    // genuine HOLD it reads "no directional call"; under a directional
    // lean it carries the graded percentage AND the gate. The advisory
    // text is carried separately as environment guidance.
    final_verdict: verdictSentence,
    // v6.10.19 (T2/T5) + v6.17: verdict-aware — under a HOLD top the
    // guidance is reworded and stripped of execution instructions; under
    // a directional verdict it leads with the verdict's own read
    // (direction + probability), never a stale neutral claim.
    final_verdict_guidance:
      verdictAwareGuidance(args.advisory, rank.top, rank.top_prob) != null
        ? `Environment guidance: ${verdictAwareGuidance(args.advisory, rank.top, rank.top_prob)}`
        : null,
    why_note: buildWhyNote(rank),
    // Top-3 bullets (panel parity — qualifying alternatives surface in
    // `top_setup.alternate_qualifying_setups` + the panel's note, so the
    // export mirrors the screen 1:1).
    why: rank.rationale.slice(0, 3),
    // v7.0: empty (`configured: false`) until the drawer runs a
    // calculation; then a verbatim copy of the drawer's projection grid.
    projection: args.projection ?? emptyProjection(),
  };
  return JSON.stringify(payload, null, 2);
}