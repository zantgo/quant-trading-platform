// Opportunity tab builder — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome) and the
// new R:R availability helper. Adds the missing visual blocks
// (directional bars, no-clear strip, hold scenario note, viability).

import { normalizeViability } from '../viability';
import type { OpportunityMatrix, OpportunityProfile, AnalysisMatrix, AdvisoryMatrix, DecisionContext, MarketBias } from '../../types';
import { computeDecisionRank, profileSummary, resolveActiveRr, selectProfileSide, sideBracketSummary, topQualifyingProfile, neutralBracketSummary, type SideBracketSummary, type NeutralBracketSummary } from '../../lib/decisionRank';
import { buildPriceBlock, buildHeaderBlock, buildRrBlock, type MetaEnvelope, type HeaderBlock, type InstanceTermsLike } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { computeOpportunityBars, rankSectionsByCount } from '../../lib/opportunityBars';
import { LEVEL_SOURCE_ABBREV } from '../levelSourceAbbrev';
import { confluenceStrengthLabel } from '../confluenceStrength';
import { buildOpportunitySummary, highlightOpportunitySummary, OPPORTUNITY_SUMMARY_LABEL } from '../opportunitySummary';
import { computeConfluentRr, fmtConfluentRrMagnitude } from '../confluentRr';

// ── Payload types ────────────────────────────────────────────────────────

/** v6.10.19b (C2): one always-present Trade Setups section — the
 *  Opportunities panel renders the folders in RANKED order (the folder
 *  with the most setups first — same relevance ordering as the
 *  conviction bars), top-ranked first within each. The export mirrors
 *  the panel 1:1.
 *  v6.10.21 (NBR): reference brackets ride INSIDE their directional
 *  section as rows (a section that hosts no qualifying setups carries
 *  its aggregated bracket / neutral range frame). */
export interface TradeSetupSection {
  section: 'NEUTRAL' | 'BULL' | 'BEAR';
  label: string;
  /** Every value of each individual setup in this section. */
  setups: TradeSetupRow[];
}

export interface TradeSetupRow {
  opportunity_type: string;
  viability: string;
  badge_text: string;
  /** v6.10.21: quality band of the DISPLAYED score (PRIME/STRONG/MODERATE/
   *  MARGINAL/NONE) — mirrors the per-card pill. `null` on reference
   *  rows (the panel renders no pill there). */
  quality: string | null;
  /** v6.10.19 (T1): precondition-scaled display score (0 when inactive). */
  score_display: number;
  side: 'LONG' | 'SHORT' | 'NEUTRAL';
  /** v6.10.19b (C1): the Trade Setups section the card renders in —
   *  'NEUTRAL', 'BULL' (LONG) or 'BEAR' (SHORT). */
  section: 'NEUTRAL' | 'BULL' | 'BEAR';
  rank_idx: number;
  is_top: boolean;
  geometry_consistent: boolean;
  /** v6.10.21: State D flag — the reference bracket's R:R is below the
   *  1.0 actionable floor or its geometry is inconsistent. */
  below_floor: boolean;
  entry_mid: number | null;
  entry_zone: { low: number; high: number } | null;
  tp1: number;
  tp2: number;
  invalidation: number | null;
  rr_available: boolean;
  rr_value: number | null;
  rr_reason: string | null;
  score: number;
  preconditions_met: number;
  preconditions_total: number;
  notes: string;
}

export interface RrInternalBlock {
  expected_rr_available: boolean;
  expected_rr_value: number | null;
  /** v6.10.19 (P5): the GROSS geometric R:R (pre-cost) — the net lives
   *  in `expected_rr_value`; the gross stays for offline analysis. */
  gross_rr_value: number | null;
  expected_rr_reason: string | null;
  time_horizon: string;
}

export interface EvaluatedSetupRow {
  opportunity_type: string;
  viability: string;
  score: number;
  /** v6.10.19 (T1): precondition-scaled display score (0 when inactive). */
  score_display: number;
  preconditions_met: number;
  preconditions_total: number;
  trade_viability: string | null;
  notes: string;
}

export interface ConfluentLevelRow {
  price: number;
  sources: string[];
  strength: number;
  /** v6.15: qualitative band mirroring the panel pill (WEAK/MODERATE/
   *  STRONG/VERY STRONG) — the screen no longer renders the raw %. */
  strength_label: string;
  /** v6.10.17 (F23): LONG / SHORT / null — which side the level serves. */
  side: 'LONG' | 'SHORT' | null;
}

export interface MarketPositionBlock {
  bias: string;
  regime: string;
  trend: string;
  quality: string;
}

export interface EnvironmentBlock {
  timeframes_considered: number;
  timeframes_considered_display: string;
  confidence_pct: number;
  /** Panel-format string mirroring `Confidence: N%`. */
  confidence_display: string;
}

export interface DirectionalBarsBlock {
  bullish_pct: number;
  bearish_pct: number;
  hold_pct: number;
  sort: 'desc';
}


export interface OpportunityPayload {
  source_tab: 'opportunity';
  meta: MetaEnvelope;
  header: HeaderBlock;
  /** v7.0: the OPPORTUNITY SUMMARY natural-language paragraph — the same
   *  string the panel's top summary card renders (shared generator). */
  summary: string;
  /** v7.2: the panel-rendered (keyword-highlighted) summary paragraph —
   *  mirrors `interpretation_display` in the Analysis tab; `summary`
   *  stays raw for data consumers. */
  summary_display: string;
  /** v7.0: the [Subject] Summary label rendered on the panel card. */
  summary_label: string;
  directional_bars: DirectionalBarsBlock;
  trade_setups: TradeSetupRow[];
  /** v6.10.19b (C2): the nested sections view — NEUTRAL / BULL / BEAR,
   *  each a list of its setups with ALL values (entry zone, TPs, SL, R:R,
   *  score, preconditions, geometry, badge, notes). Always present. */
  trade_setup_sections: TradeSetupSection[];
  rr_internal: RrInternalBlock;
  /** Audit C2: the per-side "Expected Reward-to-Risk Ratio" section —
   *  the confluent-geometry LONG/SHORT R-multiplier cards the panel
   *  renders via `computeConfluentRr` (OpportunitiesPanel.svelte:741-777).
   *  Distinct from `rr_internal` (the resolved active-side R:R from the
   *  decision chain). */
  confluent_rr: {
    sides: Array<{
      side: 'LONG' | 'SHORT';
      entry_avg: number;
      target_avg: number;
      invalidation_avg: number | null;
      /** v7.3: `bracket_geometry` marks a row synthesized from the side's
       *  per-side bracket zones (midpoints + zone invalidation) because
       *  its confluent set was incomplete — the confluent-averaged rows
       *  carry `invalidation` or `market_distance` as before. */
      risk_basis: 'invalidation' | 'market_distance' | 'bracket_geometry';
      rr: number | null;
      /** The exact magnitude label the screen renders ("1.5R", "10R+"). */
      rr_display: string;
      reason: string | null;
    }>;
    /** Global N/A reason when NO side produced a row. */
    reason: string | null;
  };
  invalidation_note: string;
  evaluated_setups: EvaluatedSetupRow[];
  confluent_entry_levels: ConfluentLevelRow[];
  confluent_target_levels: ConfluentLevelRow[];
  market_position: MarketPositionBlock;
  environment: EnvironmentBlock;
}

// ── Helpers ──────────────────────────────────────────────────────────────

/**
 * v6.10.19 (T1): the DISPLAY score scales the raw wire score by the
 * precondition ratio — 0/3 met → 0 (muted, a dead setup), 2/3 → 2/3 of
 * the score, 3/3 → the full score. The raw wire `score` stays untouched
 * for data-science consumers; only what the operator SEES changes, so a
 * trader can instantly separate "nearly ready" from "completely dead".
 *
 * v6.14: the backend now emits this scaled value as the profile's
 * `display_score` — `wireDisplayScore` below prefers it and keeps this
 * local rule ONLY as the legacy-payload fallback, so the export can
 * never disagree with the panel or the wire.
 */
function displayScore(score: number, met: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round(score * Math.min(1, met / total));
}

function wireDisplayScore(p: {
  score: number;
  preconditions_met: number;
  preconditions_total: number;
  display_score?: number | null;
}): number {
  return p.display_score != null
    ? p.display_score
    : displayScore(p.score, p.preconditions_met, p.preconditions_total);
}

function setupQuality(s: number): string {
  if (s >= 85) return 'PRIME';
  if (s >= 70) return 'STRONG';
  if (s >= 50) return 'MODERATE';
  if (s >= 30) return 'MARGINAL';
  return 'NONE';
}

function prettifyOpportunityType(raw: string): string {
  // "TrendContinuation" → "Trend Continuation"
  return raw
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ');
}

// v6.10.21 badge policy — mirrors `OpportunitiesPanel.svelte`
// `setupBadgeCls`: EVERY Actionable card carries the actionable badge
// (`TOP · ACTIONABLE` for the top-ranked one, plain `ACTIONABLE` for the
// rest — the HOLD-verdict gate is gone), a card with broken geometry is
// always `GEOMETRY INVERTED`, DirectionalNeutral reads `RANGE · NEUTRAL`.
function setupBadgeText(viability: string, geometryConsistent: boolean, isTopActionable: boolean): string {
  if (!geometryConsistent || viability === 'GeometryInverted') return 'GEOMETRY INVERTED';
  if (viability === 'Actionable') return isTopActionable ? 'TOP · ACTIONABLE' : 'ACTIONABLE';
  if (viability === 'Qualifying') return 'QUALIFYING';
  if (viability === 'DirectionalNeutral') return 'RANGE · NEUTRAL';
  return 'NO CLEAR';
}

const SETUP_VIABILITY_RANK: Record<string, number> = {
  Actionable: 0,
  Qualifying: 1,
  DirectionalNeutral: 2,
  GeometryInverted: 3,
  NoClear: 4,
};

function buildTradeSetups(
  opportunity: OpportunityMatrix | null,
  analysis: AnalysisMatrix | null,
  decisionContext: DecisionContext | null,
  markPrice: number,
): TradeSetupRow[] {
  if (!opportunity) return [];
  const profiles = opportunity.profiles ?? [];
  const qualifying = profiles
    .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
    .slice()
    .sort((a, b) => b.score - a.score);
  const out: TradeSetupRow[] = [];
  qualifying.forEach((p, idx) => {
    // Mirrors the panel's `activeSetups` derivation exactly: profileSummary
    // (aggregate-bracket fallback + wire R:R preference), NEUTRAL-side
    // cards included, geometry from the resolved zones.
    const s = profileSummary(p, opportunity, analysis, decisionContext, markPrice);
    const z = s.zones;
    const tpCandidates = z ? [z.target.low, z.target.high].filter((v) => v > 0) : [];
    const sortedTp = z
      ? [...tpCandidates].sort(
          (a, b) =>
            Math.abs(a - z.entry.low - ((z.entry.high - z.entry.low) / 2)) -
            Math.abs(b - z.entry.low - ((z.entry.high - z.entry.low) / 2)),
        )
      : [];
    // v6.10.16 (FIX-O4): use the resolver's real reason — a
    // DirectionalNeutral card with consistent geometry reports
    // "no_directional_bias" (matching rr_internal and the recommendation),
    // never the hardcoded geometry fallback.
    const rr = buildRrBlock(s.rr, s.rr_reason ?? 'no_actionable_geometry');
    out.push({
      opportunity_type: prettifyOpportunityType(p.opportunity_type),
      viability: s.viability,
      badge_text: '',
      quality: null,
      side: s.side,
      section: s.side === 'LONG' ? 'BULL' : s.side === 'SHORT' ? 'BEAR' : 'NEUTRAL',
      rank_idx: idx,
      is_top: false,
      below_floor: false,
      score_display: wireDisplayScore(p),
      geometry_consistent: z?.geometry_consistent ?? false,
      entry_mid: z ? (z.entry.low + z.entry.high) / 2 : null,
      entry_zone: z ? { low: z.entry.low, high: z.entry.high } : null,
      tp1: sortedTp[0] ?? 0,
      tp2: sortedTp.length > 1 ? sortedTp[1] : sortedTp[0] ?? 0,
      invalidation: z?.invalidation ?? null,
      rr_available: rr.available,
      rr_value: rr.value,
      rr_reason: rr.reason,
      score: p.score,
      preconditions_met: p.preconditions_met,
      preconditions_total: p.preconditions_total,
      // Panel stores the raw wire notes and renders them verbatim.
      notes: p.notes,
    });
  });
  // Viability ordering (Actionable first), then score desc — panel order.
  const ranked = out.sort((a, b) => {
    const va = SETUP_VIABILITY_RANK[a.viability] ?? 3;
    const vb = SETUP_VIABILITY_RANK[b.viability] ?? 3;
    if (va !== vb) return va - vb;
    return b.score - a.score;
  });
  // v6.10.21: badge + quality assigned AFTER the viability-tier sort —
  // `TOP · ACTIONABLE` goes to the FIRST Actionable card in panel order
  // (the HOLD-verdict gate is removed), and the quality pill bands the
  // DISPLAYED score exactly like the screen.
  // Audit G-5: `rank_idx` is ALSO remapped to the final sorted index so
  // the export matches the panel's rankIdx (the M7 fix remapped the panel
  // only; the export previously kept the pre-sort index and drifted in
  // mixed-tier lists).
  const firstActionableIdx = ranked.findIndex((r) => r.viability === 'Actionable');
  ranked.forEach((r, i) => {
    r.rank_idx = i;
    r.badge_text = setupBadgeText(r.viability, r.geometry_consistent, i === firstActionableIdx);
    r.is_top = i === firstActionableIdx && r.viability === 'Actionable';
    r.quality = setupQuality(r.score_display);
  });
  // v6.10.21 (NBR): per-folder reference brackets — mirror the panel: a
  // folder mounts its aggregated bracket (long → BULLISH, short →
  // BEARISH, backend neutral range frame → RANGE) ONLY when it hosts
  // zero qualifying setup rows. `below_floor`/broken geometry demotes
  // the badge to `BELOW ACTIONABLE FLOOR` (State D).
  const hasRows = (section: 'NEUTRAL' | 'BULL' | 'BEAR') => ranked.some((r) => r.section === section);
  const referenceRow = (
    ref: SideBracketSummary | NeutralBracketSummary,
    section: 'NEUTRAL' | 'BULL' | 'BEAR',
  ): TradeSetupRow => {
    const warn = ref.below_floor === true || ref.zones?.geometry_consistent === false;
    const z = ref.zones;
    return {
      opportunity_type: ref.opportunity_type === 'NeutralBracket' ? 'Neutral Reference Bracket' : 'Aggregated Bracket',
      viability: 'NoClear',
      badge_text: warn ? 'BELOW ACTIONABLE FLOOR' : 'INFORMATIONAL',
      quality: null,
      side: ref.direction,
      section,
      rank_idx: 0,
      is_top: false,
      below_floor: warn,
      score_display: 0,
      geometry_consistent: z?.geometry_consistent ?? false,
      entry_mid: z ? (z.entry.low + z.entry.high) / 2 : null,
      entry_zone: z ? { low: z.entry.low, high: z.entry.high } : null,
      tp1: z ? (z.target.low + z.target.high) / 2 : 0,
      tp2: 0,
      invalidation: z?.invalidation ?? null,
      rr_available: ref.rr != null,
      rr_value: ref.rr,
      rr_reason: ref.rr != null ? null : (ref.rr_reason ?? 'no actionable geometry'),
      score: 0,
      preconditions_met: 0,
      preconditions_total: 0,
      notes: ref.rationale,
    };
  };
  if (!hasRows('BULL')) {
    const ref = sideBracketSummary(opportunity, decisionContext, analysis, 'LONG', markPrice);
    if (ref && ref.zones != null) ranked.push(referenceRow(ref, 'BULL'));
  }
  if (!hasRows('BEAR')) {
    const ref = sideBracketSummary(opportunity, decisionContext, analysis, 'SHORT', markPrice);
    if (ref && ref.zones != null) ranked.push(referenceRow(ref, 'BEAR'));
  }
  if (!hasRows('NEUTRAL')) {
    const ref = neutralBracketSummary(opportunity);
    if (ref && ref.zones != null) ranked.push(referenceRow(ref, 'NEUTRAL'));
  }
  return ranked;
}

function buildTradeSetupSections(rows: TradeSetupRow[]): TradeSetupSection[] {
  const bySection = (key: 'NEUTRAL' | 'BULL' | 'BEAR') =>
    rows.filter((r) => r.section === key);
  // RANKED order — the panel ranks the folders by content count (setup
  // rows + reference), then top score, falling back to RANGE → BULL →
  // BEAR. Reference rows are merged into `rows`, so the folder count
  // here already includes them (panel parity).
  return rankSectionsByCount(
    [
      { key: 'NEUTRAL' as const, label: 'RANGE', setups: bySection('NEUTRAL') },
      { key: 'BULL' as const, label: 'BULLISH', setups: bySection('BULL') },
      { key: 'BEAR' as const, label: 'BEARISH', setups: bySection('BEAR') },
    ].map((s) => ({
      ...s,
      hasReference: false,
      scoreOf: (r: TradeSetupRow) => r.score,
    })),
  ).map((s) => ({ section: s.key, label: s.label, setups: s.setups }));
}

function buildEvaluatedSetups(opportunity: OpportunityMatrix | null): EvaluatedSetupRow[] {
  if (!opportunity?.profiles) return [];
  // The screen's Evaluated Setups list excludes the NoClearOpportunity
  // profile — it has its own placeholder strip.
  return opportunity.profiles
    .filter((p) => p.opportunity_type !== 'NoClearOpportunity')
    .map((p) => ({
      opportunity_type: prettifyOpportunityType(p.opportunity_type),
      // v6.10.17 (P1): a profile with met preconditions but a null wire
      // viability is QUALIFYING (a real bracket) — never NoClear.
      // v2026-08: normalize the SCREAMING wire token to PascalCase so this
      // field matches `trade_setups[].viability` (same export, same
      // vocabulary — previously the raw "ACTIONABLE" sat next to
      // "Actionable" rows).
      viability: p.trade_viability ? normalizeViability(p.trade_viability) : (p.preconditions_met > 0 ? 'Qualifying' : 'NoClear'),
      score: p.score,
      // v6.10.19 (T1): the operator-facing score scales by precondition
      // ratio; the raw wire value stays in `score`. v6.14: wire-first —
      // the backend's `display_score` wins, local rule only for legacy
      // payloads (screen and clipboard always agree).
      score_display: wireDisplayScore(p),
      preconditions_met: p.preconditions_met,
      preconditions_total: p.preconditions_total,
      // Keep the raw wire value so consumers can round-trip the enum;
      // panel renders verbatim.
      trade_viability: p.trade_viability ?? null,
      // Panel renders the raw wire notes verbatim.
      notes: p.notes,
    }));
}

function buildConfluentLevels(
  levels: OpportunityMatrix['confluent_entry_levels'] | undefined,
): ConfluentLevelRow[] {
  if (!levels) return [];
  return levels.map((l) => ({
    price: l.price,
    // Screen `fmtSource` defaults unknown tokens to "ATR" — mirror it.
    sources: l.sources.map((s) => LEVEL_SOURCE_ABBREV[s] ?? 'ATR'),
    strength: l.strength,
    // Panel pill band via the shared helper — screen and export agree.
    strength_label: confluenceStrengthLabel(l.strength),
    side: l.side ?? null,
  }));
}

function buildMarketPosition(analysis: AnalysisMatrix | null): MarketPositionBlock {
  // Screen renders "—" for missing fields.
  return {
    bias: analysis?.bias ?? '\u2014',
    regime: analysis?.market_regime ?? '\u2014',
    trend: analysis?.trend_assessment ?? '\u2014',
    quality: analysis?.market_quality ?? '\u2014',
  };
}

function buildEnvironment(analysis: AnalysisMatrix | null): EnvironmentBlock {
  const tf = analysis?.timeframes_considered ?? 0;
  const confidencePct = analysis ? Math.round(analysis.confidence * 100) : 0;
  return {
    timeframes_considered: tf,
    timeframes_considered_display: `${tf} Timeframes considered`,
    confidence_pct: confidencePct,
    confidence_display: analysis ? `${confidencePct}%` : '\u2014',
  };
}

function buildDirectionalBars(
  opportunity: OpportunityMatrix | null,
  bias: MarketBias | null,
): DirectionalBarsBlock {
  // L4 bracket conviction only — the export mirrors the panel: the
  // split comes from `opportunity_score` × active-side R:R, never from
  // the L6 decision-context probabilities. The panel ALWAYS renders all
  // three bars.
  const bars = computeOpportunityBars(opportunity, bias);
  return {
    bullish_pct: bars.bullish,
    bearish_pct: bars.bearish,
    hold_pct: bars.hold,
    sort: 'desc',
  };
}

// ── Public builder ───────────────────────────────────────────────────────

export interface OpportunityTabInputs {
  opportunity: OpportunityMatrix | null;
  analysis: AnalysisMatrix | null;
  decisionContext: DecisionContext | null;
  advisory?: AdvisoryMatrix | null;
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
 * Build the Opportunity tab export payload. Mirrors
 * `OpportunitiesPanel.svelte` 1:1.
 */
export function buildOpportunityTabExport(args: OpportunityTabInputs): string {
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  // The panel computes its rank with the real advisory — the export must
  // pass the same inputs or lean/top/badges would drift from the screen.
  const rank = computeDecisionRank({
    advisory: args.advisory ?? null,
    decisionContext: args.decisionContext,
    opportunity: args.opportunity,
    analysis: args.analysis,
  });
  const opp = args.opportunity;
  const activeSideRr = (() => {
    if (!opp) return null;
    // RR-002 (v6.10.12): mirror the panel's R:R (Internal) block through
    // the shared resolver — the same chain (profile wire → matrix wire →
    // aligned zones fallback) the screen uses.
    const resolved = resolveActiveRr(opp, args.decisionContext, args.analysis);
    return resolved.available ? resolved.value : 0;
  })();
  // v6.10.19b (B1): resolve the active side with the SAME chain as
  // `resolveActiveRr` so the gross R:R follows the verdict/bias side —
  // the wire fields are always numbers (serde default 0.0), so a naive
  // `??` fallback never fires and exported 0.0 for valid short/neutral
  // brackets (gross_rr_side mismatch bug).
  const grossRrSide = ((): 'LONG' | 'SHORT' | 'NEUTRAL' => {
    if (!opp) return 'NEUTRAL';
    const bias = args.decisionContext?.bias ?? args.analysis?.bias ?? null;
    const top = topQualifyingProfile(opp);
    if (top) return selectProfileSide(top, bias);
    return bias === 'Bullish' || bias === 'StrongBullish'
      ? 'LONG'
      : bias === 'Bearish' || bias === 'StrongBearish'
        ? 'SHORT'
        : 'NEUTRAL';
  })();
  const grossRrValue =
    grossRrSide === 'LONG'
      ? opp?.long_gross_rr_internal ?? null
      : grossRrSide === 'SHORT'
        ? opp?.short_gross_rr_internal ?? null
        : null;
  const expectedRrBlock =
    rank.top === 'HOLD' && (activeSideRr === null || activeSideRr === 0)
      ? { available: false as const, value: null, reason: 'no_directional_bias' as string | null }
      : { available: true as const, value: activeSideRr ?? 0, reason: null as string | null };
  const tradeSetupRows = buildTradeSetups(opp, args.analysis, args.decisionContext, args.markPrice ?? 0);
  // Audit C2: the per-side confluent-geometry R:R the panel shows under
  // "Expected Reward-to-Risk Ratio" — same resolver, same markPrice.
  const confluentRr = computeConfluentRr(opp, args.markPrice ?? 0);
  const payload: OpportunityPayload = {
    source_tab: 'opportunity',
    meta,
    header: buildHeaderBlock(args.headerSpec),
    // v7.0: shared generator — the exact paragraph the panel's OPPORTUNITY
    // SUMMARY card renders (parity invariant).
    summary: buildOpportunitySummary(opp),
    // v7.2: the highlighted variant mirrors the screen rendering (the
    // panel renders `@html highlightOpportunitySummary(summary)`).
    summary_display: highlightOpportunitySummary(buildOpportunitySummary(opp)),
    summary_label: OPPORTUNITY_SUMMARY_LABEL,
    // L4 bracket conviction only — the bias arg comes from Analysis (L3),
    // never from the L6 decision context.
    directional_bars: buildDirectionalBars(opp, args.analysis?.bias ?? null),
    trade_setups: tradeSetupRows,
    trade_setup_sections: buildTradeSetupSections(tradeSetupRows),
    rr_internal: {
      expected_rr_available: expectedRrBlock.available,
      expected_rr_value: expectedRrBlock.value,
      // v6.10.19 (P5): the GROSS geometric R:R (pre-cost) for offline
      // analysis — the net lives in `expected_rr_value`. Side-resolved
      // from the verdict/bias chain (B1) so a valid SHORT/NEUTRAL bracket
      // never exports 0.0 from the long-side wire default.
      gross_rr_value: grossRrValue,
      expected_rr_reason: expectedRrBlock.reason,
      // Screen renders "—" when the horizon is absent.
      time_horizon: opp?.time_horizon ?? '\u2014',
    },
    confluent_rr: {
      sides: confluentRr.sides.map((s) => ({
        side: s.side,
        entry_avg: s.entryAvg,
        target_avg: s.targetAvg,
        invalidation_avg: s.invalidationAvg,
        risk_basis: s.riskBasis,
        rr: s.rr,
        rr_display: s.rr != null ? fmtConfluentRrMagnitude(s.rr) : 'N/A',
        reason: s.reason,
      })),
      reason: confluentRr.reason,
    },
    invalidation_note:
      opp?.invalidation_note ??
      'Assessment conditions forming — invalidation level will be calculated when structural pivot confirms.',
    evaluated_setups: buildEvaluatedSetups(opp),
    // The screen renders at most the first 4 entry + 4 target levels.
    confluent_entry_levels: buildConfluentLevels(opp?.confluent_entry_levels).slice(0, 4),
    confluent_target_levels: buildConfluentLevels(opp?.confluent_target_levels).slice(0, 4),
    market_position: buildMarketPosition(args.analysis),
    environment: buildEnvironment(args.analysis),
  };
  return JSON.stringify(payload, null, 2);
}

// Silence unused-import warning
export type { OpportunityProfile };