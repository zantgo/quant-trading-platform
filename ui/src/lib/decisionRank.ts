// Decision Rank — pure helper for the unified Decision-tab hero.
//
// `computeDecisionRank()` consumes the same wire-format payloads the Advisory
// panel already reads (AdvisoryMatrix + DecisionContext + OpportunityMatrix)
// and produces a single `DecisionRank` view containing:
//
//   - three normalized probabilities (long / short / hold) summing to 100
//   - the top action + a hero label that is always consistent with the
//     `trade_readiness` gate (so the UI never says "LONG — READY" while the
//     gate is "STAND ASIDE")
//   - a bulleted rationale list explaining the decision
//
// This is the **frontend-only consolidation layer** that resolves the
// long-standing UX bug where three competing badges (trade_readiness,
// directional_guidance, market_stance) appeared at the top of the Decision
// tab and could visually contradict each other. The three fields are
// orthogonal axes (gate vs bias vs environment), not redundant, so the bug
// is perceptual: this helper merges them into a single coherent verdict.

import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, MarketBias, NeutralBracket, OpportunityMatrix, OpportunityProfile, PriceRange } from '../types';
import { normalizeViability } from './viability';

export type DecisionAction = 'LONG' | 'SHORT' | 'HOLD';
export type DecisionState = 'READY' | 'FORMING' | 'WATCH' | 'STAND_ASIDE';

export interface RankSide {
    probability: number;          // 0..100, integer, normalised to sum 100
    reasons: string[];
}

export interface DecisionRank {
    /** v6.10.19 (P6): the graded-lean floors adjusted this split (HOLD
     *  capped at 60% and/or the directional arm raised to 15%) — the
     *  read is structurally boosted, not a deep consensus. */
    lean_floor_applied: boolean;
    long: RankSide;
    short: RankSide;
    hold: RankSide;
    top: DecisionAction;          // argmax of (long, short, hold)
    top_prob: number;
    /** The hero label & colour. */
    headline: {
        action: DecisionAction | 'STAND_ASIDE';
        label: string;
        state: DecisionState;
        confidence_pct: number;
    };
    /** Flat bulleted list of why-the-decision strings, ordered by importance. */
    rationale: string[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry-point
// ─────────────────────────────────────────────────────────────────────────────

export interface DecisionRankInputs {
    advisory: AdvisoryMatrix | null;
    decisionContext: DecisionContext | null;
    opportunity: OpportunityMatrix | null;
    analysis: AnalysisMatrix | null;
}

export function computeDecisionRank(inputs: DecisionRankInputs): DecisionRank {
    const { advisory, decisionContext, opportunity, analysis } = inputs;

    // Defaults — empty / pre-warmup state.
    const score = decisionContext?.score ?? 0;
    const scoreConfidence = decisionContext?.score_confidence ?? 0;
    const bias = decisionContext?.bias ?? 'Neutral';
    // `decisionContext.entry_danger` is now a RiskDimension-shaped object
    // (matches the wire and the Rust struct). Defensively extract the
    // scalar — handle the legacy bare-number shape that older snapshots
    // may still send.
    const entryDangerRaw = decisionContext?.entry_danger;
    const entryDanger =
        typeof entryDangerRaw === 'number'
            ? entryDangerRaw
            : entryDangerRaw?.score ?? 50;
    const expectedRr = decisionContext?.expected_reward_risk_ratio ?? 0;
    const readiness = (decisionContext?.trade_readiness ?? 'STAND_ASIDE') as DecisionState;
    const confidence = advisory?.confidence_assessment ?? 0;
    const guidance = advisory?.directional_guidance ?? 'Neutral';
    const stance = advisory?.market_stance ?? 'Neutral';
    const supporters = decisionContext?.contributing_indicators ?? [];
    const oppScore = opportunity?.opportunity_score ?? 0;
    const setupQuality = opportunity?.setup_quality ?? '—';
    const setupType = opportunity?.primary_opportunity ?? 'NoClearOpportunity';
    const supportingSignals = analysis?.supporting_signals ?? [];
    const contradictingSignals = analysis?.contradicting_signals ?? [];

    // ── Probabilities: use backend as source of truth when available ────
    // The Rust `DecisionContext::compute()` now publishes `long_probability`,
    // `short_probability`, `hold_probability`, and `net_bias_pct` — canonical
    // percentage values that match the algorithm replicated below. When the
    // backend fields are present (server-side v6.11+), consume them directly
    // and skip the local re-computation.
    let long: number;
    let short: number;
    let hold: number;

    const hasBackendProbs =
        decisionContext?.long_probability != null &&
        decisionContext?.short_probability != null &&
        decisionContext?.hold_probability != null;

    // v6.10.19 (P6): the graded-lean floors moved this split?
    let leanFloorApplied = false;
    if (hasBackendProbs) {
        long = decisionContext!.long_probability!;
        short = decisionContext!.short_probability!;
        hold = decisionContext!.hold_probability!;
    } else {
        // ── Fallback: local computation (identical to the Rust backend) ───

        // GEOMETRIC OFFSET: If the macro trend is neutral (score=0), inspect the top-scored
        // latent setup using the top-level opportunity matrix zones as a fallback.
        let directionalOffset = 0;
        if (score === 0 && opportunity) {
            const profiles = opportunity.profiles ?? [];
            // v6.10.17 (P2 parity): the backend filter uses `preconditions_met > 0`
            // (decision_context.rs) — the legacy `>= 0` let EVERY profile
            // qualify, including 0/N warmup rows. Mirrored exactly here.
            const qualifying = profiles.filter(
                (p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity'
            );
            if (qualifying.length > 0) {
                // v6.10.17 (P2 parity): the backend `max_by` keeps the LAST
                // element on score ties — replicate with a strict-`>=` fold.
                const topProfile = qualifying.reduce((acc, p) => (p.score >= acc.score ? p : acc));

                const hasLong = opportunity.long_entry_zone && opportunity.long_entry_zone.low > 0;
                const hasShort = opportunity.short_entry_zone && opportunity.short_entry_zone.low > 0;

                let resolvedSide: 'LONG' | 'SHORT' | 'NEUTRAL' = 'NEUTRAL';
                if (hasLong && !hasShort) {
                    resolvedSide = 'LONG';
                } else if (hasShort && !hasLong) {
                    resolvedSide = 'SHORT';
                } else if (hasLong && hasShort) {
                    const longRr = opportunity.long_expected_rr_internal ?? 0;
                    const shortRr = opportunity.short_expected_rr_internal ?? 0;
                    resolvedSide = longRr >= shortRr ? 'LONG' : 'SHORT';
                }

                if (resolvedSide === 'LONG') {
                    directionalOffset = topProfile.score * 0.15;
                } else if (resolvedSide === 'SHORT') {
                    directionalOffset = -topProfile.score * 0.15;
                }
            }
        }

        const effectiveScore = score !== 0 ? score : directionalOffset;
        const effectiveConfidence = scoreConfidence || 0.5;

        // ── 1. Raw signal scores (each ∈ [0, 100]) ─────────────────────────────
        const baseLong = clamp(0, 100, Math.max(0, effectiveScore) * effectiveConfidence);
        const baseShort = clamp(0, 100, Math.max(0, -effectiveScore) * effectiveConfidence);
        const baseHold = clamp(0, 100, (entryDanger / 100) * 50);

        // ── 2. Bias / guidance / stance modulation ─────────────────────────────
        long = baseLong;
        short = baseShort;
        hold = baseHold;

        const g = guidance.toLowerCase();
        const s = stance.toLowerCase();

        if (g.includes('long')) {
            long *= 1.2;
            short *= 0.5;
        } else if (g.includes('short')) {
            short *= 1.2;
            long *= 0.5;
        }

        if (s === 'aggressive' || s === 'constructive') {
            if (g.includes('long')) long *= 1.15;
            else if (g.includes('short')) short *= 1.15;
        }
        if (s === 'avoid') {
            long *= 0.5;
            short *= 0.5;
            hold *= 1.5;
        }

        // R:R modulation. v6.10.17 (mirror of the backend): the ×0.6
        // penalty applies only when an ACTUAL sub-1.0 R:R exists — a
        // missing R:R (0) is unknown, not bad, and must not punish a
        // vote-driven lifted lean.
        if (expectedRr > 0 && expectedRr < 1.0) {
            if (g.includes('long')) long *= 0.6;
            else if (g.includes('short')) short *= 0.6;
        }

        long = clamp(0, 100, long);
        short = clamp(0, 100, short);
        hold = clamp(0, 100, hold);

        // ── 3. Renormalize to sum to 100 (largest absorbs rounding residual) ──
        let hadSignal = false;
        const sum = long + short + hold;
        if (sum <= 0) {
            long = 34;
            short = 33;
            hold = 33;
        } else {
            hadSignal = true;
            const l = Math.round((long / sum) * 100);
            const sh = Math.round((short / sum) * 100);
            const h = 100 - l - sh;
            long = l;
            short = sh;
            hold = h;
        }
        if (hadSignal) {
            const MIN_PCT = 2;
            long = Math.max(long, MIN_PCT);
            short = Math.max(short, MIN_PCT);
            hold = Math.max(hold, MIN_PCT);
            // v6.10.17 graded-lean floors (mirror of the backend): when a
            // directional read exists, HOLD caps at 60% and the directional
            // arm never sinks below 15% — the verdict can never collapse
            // into a 96% HOLD next to a minimal bearish/bullish confirmation.
            const g2 = guidance.toLowerCase();
            if (g2.includes('long') || g2.includes('short')) {
                // v6.10.19 (P6): mirror the backend flag — did the floors
                // actually move the split?
                const preHold = hold;
                const preArm = g2.includes('long') ? long : short;
                hold = Math.min(hold, 60);
                if (g2.includes('long')) long = Math.max(long, 15);
                else short = Math.max(short, 15);
                if (preHold > 60 || preArm < 15) leanFloorApplied = true;
            }
            const reSum = long + short + hold;
            long = Math.round((long / reSum) * 100);
            short = Math.round((short / reSum) * 100);
            hold = 100 - long - short;
        }
    }

    // ── 4. Top action ─────────────────────────────────────────────────────
    // Degenerate-rank guard: if the leading action is below 35% probability,
    // the data does not support a directional call. Collapse to HOLD so the
    // operator does not see a confident "LONG" headline when the math
    // actually says "all three are about equal".
    // v6.10.17: when the macro bias IS directional (incl. lifted grace/LEAN
    // reads — `decisionContext.score` carries its sign), the verdict mirrors
    // it unless HOLD is overwhelmingly dominant: the bias-side arm wins when
    // it holds ≥35% and stays within 10 points of the hold share. This is the
    // "do not say HOLD when there is a minimal bullish/bearish confirmation"
    // rule — a graded directional read beats a bare HOLD, while the readiness
    // gate still governs when it can be acted on.
    let top: DecisionAction = 'HOLD';
    let topProb = hold;
    const maxProb = Math.max(long, short, hold);
    const biasSide: DecisionAction =
        bias === 'Bullish' || bias === 'StrongBullish'
            ? 'LONG'
            : bias === 'Bearish' || bias === 'StrongBearish'
              ? 'SHORT'
              : 'HOLD';
    if (maxProb >= 35) {
        const biasArm = biasSide === 'LONG' ? long : biasSide === 'SHORT' ? short : 0;
        if (biasSide !== 'HOLD' && biasArm >= 35 && biasArm >= hold - 10) {
            top = biasSide;
            topProb = biasArm;
        } else if (long === maxProb) {
            top = 'LONG';
            topProb = long;
        } else if (short === maxProb) {
            top = 'SHORT';
            topProb = short;
        }
    }

    // v6.10.19b (B2 / G3): the verdict respects VALID SETUPS. When the
    // probability split is hold-dominant (top HOLD — the bias is not
    // decisive and the degenerate-rank guard did not fire a direction),
    // a qualifying setup with a resolvable side provides the directional
    // lean: the verdict becomes that side with its REAL (small)
    // probability, while the readiness gate still governs execution. This
    // is the "it should not say HOLD when a valid setup exists" rule —
    // e.g. a 12/2/86 split with a qualifying LONG-side setup reads
    // "LONG lean 12%" (badge consistent with the headline setup), never a
    // bare HOLD next to a directional setup card.
    if (top === 'HOLD') {
        const qualifying = (opportunity?.profiles ?? [])
            .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
            .slice()
            .sort((a, b) => b.score - a.score);
        const leanProfile = qualifying.find((p) => {
            const hasLong = p.long_entry_zone != null && p.long_entry_zone.low > 0;
            const hasShort = p.short_entry_zone != null && p.short_entry_zone.low > 0;
            return hasLong !== hasShort;
        });
        if (leanProfile) {
            const hasLong = leanProfile.long_entry_zone != null && leanProfile.long_entry_zone.low > 0;
            const lean: 'LONG' | 'SHORT' = hasLong ? 'LONG' : 'SHORT';
            top = lean;
            topProb = lean === 'LONG' ? long : short;
        }
    }

    // ── 5. Headline (gate-aware) ───────────────────────────────────────────
    // v6.10.17: the directional read is decoupled from the execution gate —
    // STAND ASIDE no longer erases a directional lean; it reports the gate
    // ("SHORT — STAND ASIDE (lean 38%)"). The flat "HOLD — STAND ASIDE"
    // headline now applies only when the top action itself is HOLD.
    let headline: DecisionRank['headline'];
    if (readiness === 'STAND_ASIDE') {
        headline = top === 'HOLD'
            ? {
                action: 'STAND_ASIDE',
                label: `HOLD — STAND ASIDE`,
                state: 'STAND_ASIDE',
                confidence_pct: Math.round(confidence),
            }
            : {
                action: top,
                label: `${top} — STAND ASIDE (lean ${topProb}%)`,
                state: 'STAND_ASIDE',
                confidence_pct: Math.round(confidence),
            };
    } else if (top === 'HOLD') {
        headline = {
            action: 'HOLD',
            label: readiness === 'READY' ? 'HOLD — READY (awaiting trigger)' : `HOLD — ${readiness}`,
            state: readiness,
            confidence_pct: Math.round(confidence),
        };
    } else {
        headline = {
            action: top,
            label: `${top} — ${readiness}`,
            state: readiness,
            confidence_pct: Math.round(confidence),
        };
    }

    // ── 6. Rationale ──────────────────────────────────────────────────────
    const rationale = buildRationale({
        score,
        scoreConfidence,
        bias,
        readiness,
        guidance,
        stance,
        entryDanger,
        expectedRr,
        confidence,
        oppScore,
        setupQuality,
        setupType,
        supporters,
        supportingSignals,
        contradictingSignals,
        top,
    });

    // v6.10.19 (P6): prefer the backend's authoritative flag.
    const floorApplied = decisionContext?.lean_floor_applied === true || leanFloorApplied;
    return {
        lean_floor_applied: floorApplied,
        long: { probability: long, reasons: [] },
        short: { probability: short, reasons: [] },
        hold: { probability: hold, reasons: [] },
        top,
        top_prob: topProb,
        headline,
        rationale,
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Setup mirroring
// ─────────────────────────────────────────────────────────────────────────────

export interface SetupLevel {
    label: 'ENTRY' | 'TP1' | 'TP2' | 'SL';
    price: number;
    note?: string;
}

export interface SymmetricSetup {
    side: 'LONG' | 'SHORT';
    active: boolean;
    entry: SetupLevel | null;
    targets: SetupLevel[];
    stop: SetupLevel | null;
    status: string;
    rrRatio: number | null;
    /**
     * Geometric-consistency flag — true when the displayed prices match
     * the trade direction implied by entry vs target vs invalidation.
     * A long with entry above target, or short with entry below target,
     * is geometrically inverted (typically means the wire's
     * `entry_zone` and `target_zone` describe the opposite direction).
     * The panel renders an informational note when this is false.
     */
    geometry_consistent: boolean;
}

export interface MirrorInputs {
    opportunity: OpportunityMatrix | null;
    markPrice: number;
    topAction: DecisionAction;
    readiness: DecisionState;
}

function level(price: number, label: SetupLevel['label'], note?: string): SetupLevel {
    return { label, price, note };
}

function fmtPx(n: number, mp: number): string {
    if (n == null || !isFinite(n) || n <= 0) return '—';
    if (mp >= 1000) return `$${n.toFixed(0)}`;
    if (mp >= 1) return `$${n.toFixed(2)}`;
    return `$${n.toFixed(4)}`;
}

function fmtDistancePct(current: number, base: number): string {
    if (base <= 0 || current <= 0) return '—';
    const pct = ((current - base) / base) * 100;
    return `${pct >= 0 ? '+' : ''}${pct.toFixed(2)}%`;
}

/**
 * Build a symmetric Long / Short setup pair from the L4 Opportunity Matrix.
 *
 * Each side reads its **per-direction** zones directly:
 *   - Long side  → `opportunity.long_entry_zone / long_target_zone / long_invalidation_level`
 *   - Short side → `opportunity.short_entry_zone / short_target_zone / short_invalidation_level`
 *
 * TP1 is the **nearest** profitable target (smallest absolute distance from
 * entry mid); TP2 is the farther target. The R:R and stop checks are
 * direction-aware so a SHORT geometry (entry below target, inval above entry)
 * cannot accidentally inherit LONG-only invariants.
 *
 * The legacy single-bias `entry_zone / target_zone / invalidation_level`
 * fields are intentionally NOT used here: they collapse to the SHORT side
 * for Neutral bias and to whichever side the active bias picks, which
 * produces the LONG-vs-SHORT geometry inversion the user reported.
 */
export function computeSymmetricSetups(inputs: MirrorInputs): {
    long: SymmetricSetup;
    short: SymmetricSetup;
} {
    const { opportunity, markPrice, topAction, readiness } = inputs;

    const empty: SymmetricSetup = {
        side: 'LONG',
        active: false,
        entry: null,
        targets: [],
        stop: null,
        status: 'inactive (no L4 opportunity)',
        rrRatio: null,
        geometry_consistent: false,
    };

    if (!opportunity || markPrice <= 0) {
        return {
            long: { ...empty, side: 'LONG' },
            short: { ...empty, side: 'SHORT' },
        };
    }

    const gate = readiness === 'STAND_ASIDE' || readiness === 'WATCH';
    const longActive = topAction === 'LONG' && !gate;
    const shortActive = topAction === 'SHORT' && !gate;

    // ── Per-side selector: reads `long_*` / `short_*` directly. Returns
    // null when the side has no usable zones (e.g. Neutral sentinel where
    // every level pins to close).
    const selectSide = (side: 'LONG' | 'SHORT') => {
        const entry = side === 'LONG' ? opportunity.long_entry_zone : opportunity.short_entry_zone;
        const target = side === 'LONG' ? opportunity.long_target_zone : opportunity.short_target_zone;
        const inv = side === 'LONG' ? opportunity.long_invalidation_level : opportunity.short_invalidation_level;
        if (!entry || entry.low <= 0 || entry.high <= 0 || inv <= 0) return null;
        const entryMid = (entry.low + entry.high) / 2;
        // Order TP1 = nearest to entry_mid, TP2 = farther. The wire layout
        // puts `target.low`/`target.high` in different visual order for LONG
        // vs SHORT (LONG: low is conservative/closer; SHORT: low is farther).
        // Sorting by absolute distance to entry_mid gives a stable
        // direction-agnostic "nearest first" ordering.
        const tpCandidates = [target.low, target.high].filter((p) => p > 0);
        if (tpCandidates.length === 0) return null;
        const sorted = [...tpCandidates].sort((a, b) =>
            Math.abs(a - entryMid) - Math.abs(b - entryMid),
        );
        const tp1 = sorted[0];
        const tp2 = sorted.length > 1 ? sorted[1] : tp1;
        return { entry, entryMid, target, inv, tp1, tp2 };
    };

    const build = (side: 'LONG' | 'SHORT', raw: ReturnType<typeof selectSide>): SymmetricSetup => {
        if (!raw) {
            return { ...empty, side };
        }
        const { entry, entryMid, target, inv, tp1, tp2 } = raw;
        const isLong = side === 'LONG';
        // Direction-aware stop check: LONG inval < entry_mid; SHORT inval > entry_mid.
        const validStop = isLong ? inv < entryMid : inv > entryMid;
        // Direction-aware reward: LONG target above entry_mid; SHORT below.
        const reward = isLong ? tp1 - entryMid : entryMid - tp1;
        const risk = isLong ? entryMid - inv : inv - entryMid;
        const stop = validStop ? level(inv, 'SL', `L4 ${side.toLowerCase()} invalidation`) : null;
        const rrRatio = validStop && reward > 0 && risk > 0
            ? Math.round((reward / risk) * 100) / 100
            : null;
        const geometry_consistent = validStop && reward > 0 && risk > 0;

        const isActive = (isLong ? longActive : shortActive);
        const status = isActive
            ? readiness
            : gate
                ? `inactive (gated: ${readiness})`
                : `inactive (top action is not ${side})`;

        const targets: SetupLevel[] = [];
        if (tp1 > 0) targets.push(level(tp1, 'TP1', 'nearest profitable target'));
        if (tp2 > 0 && tp2 !== tp1) targets.push(level(tp2, 'TP2', 'farther profitable target'));

        return {
            side,
            active: isActive,
            entry: level(
                entryMid,
                'ENTRY',
                `${fmtPx(entry.low, markPrice)}–${fmtPx(entry.high, markPrice)}`,
            ),
            targets,
            stop,
            status,
            rrRatio,
            geometry_consistent,
        };
    };

    const longRaw = selectSide('LONG');
    const shortRaw = selectSide('SHORT');

    return {
        long: build('LONG', longRaw),
        short: build('SHORT', shortRaw),
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

function clamp(lo: number, hi: number, x: number): number {
    return Math.min(hi, Math.max(lo, x));
}

interface RationaleInputs {
    score: number;
    /** `DecisionContext.score_confidence` — |unsigned blend|/100. Lets the
     * why-line report the true unsigned blend when Neutral bias zeroes the
     * signed score (v6.10.16). */
    scoreConfidence: number;
    bias: string;
    readiness: DecisionState;
    guidance: string;
    stance: string;
    entryDanger: number;
    expectedRr: number;
    confidence: number;
    oppScore: number;
    setupQuality: string;
    setupType: string;
    supporters: string[];
    supportingSignals: string[];
    contradictingSignals: string[];
    top: DecisionAction;
}

export function entryDangerLevel(d: number): 'EXTREME' | 'HIGH' | 'MODERATE' | 'LOW' | 'VERY_LOW' {
    if (d >= 80) return 'EXTREME';
    if (d >= 60) return 'HIGH';
    if (d >= 40) return 'MODERATE';
    if (d >= 20) return 'LOW';
    return 'VERY_LOW';
}

/** Sentence-case readiness label for prose surfaces (v6.17). */
export function readinessLabel(s: DecisionState): string {
    switch (s) {
        case 'READY': return 'Ready';
        case 'WATCH': return 'Watch';
        case 'FORMING': return 'Forming';
        case 'STAND_ASIDE': return 'Stand aside';
        default: return s;
    }
}

/** Sentence-case entry-danger level for prose surfaces (e.g. `Moderate`). */
export function dangerLabel(score: number): string {
    return entryDangerLevel(score)
        .toLowerCase()
        .replace(/_/g, ' ')
        .replace(/\b\w/g, (c) => c.toUpperCase());
}

/**
 * v6.17: single shared verdict headline — used by the Recommendation panel
 * AND the export so screen and clipboard can never disagree. The readiness
 * gate renders in sentence case (`Watch`, `Stand aside`) and the danger
 * level as `Entry Danger <level>`.
 */
export function buildVerdictSentence(rank: DecisionRank, dangerScore: number): string {
    const pct = Math.round(rank.top_prob);
    if (rank.top === 'HOLD') {
        return `HOLD — no directional call (readiness: ${readinessLabel(rank.headline.state)}).`;
    }
    if (rank.headline.state === 'STAND_ASIDE') {
        return `${rank.top} lean ${pct}% — Stand Aside (readiness: Stand Aside, Entry Danger ${dangerLabel(dangerScore)}).`;
    }
    if (rank.headline.state === 'READY') {
        return `${rank.top} ${pct}% — Ready (readiness: Ready).`;
    }
    return `${rank.top} lean ${pct}% — awaiting confirmation (readiness: ${readinessLabel(rank.headline.state)}).`;
}

/**
 * The top qualifying profile (preconditions_met > 0, not the
 * NoClearOpportunity fallback), ordered by score descending. This is the
 * canonical "top setup" used by the panel cards, the directional bars,
 * the L4 header, and the R:R displays — one shared definition so every
 * surface resolves the same profile.
 *
 * Ties (the scoring blend currently emits identical scores for every
 * profile) are broken by precondition ratio (02-08 §6: "in a tie, the
 * profile with the higher preconditions_met / preconditions_total ratio
 * wins"), then by primary-opportunity priority so the top card can
 * never contradict the environment's opportunity classification.
 */
export function topQualifyingProfile(
    opportunity: OpportunityMatrix | null | undefined,
): OpportunityProfile | null {
    if (!opportunity) return null;
    const profiles = opportunity.profiles ?? [];
    const qualifying = profiles
        .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
        .slice();
    if (qualifying.length === 0) return null;
    const primary = opportunity.primary_opportunity;
    qualifying.sort((a, b) => {
        if (b.score !== a.score) return b.score - a.score;
        const ar = a.preconditions_total > 0 ? a.preconditions_met / a.preconditions_total : 0;
        const br = b.preconditions_total > 0 ? b.preconditions_met / b.preconditions_total : 0;
        if (br !== ar) return br - ar;
        if (a.opportunity_type === primary && b.opportunity_type !== primary) return -1;
        if (b.opportunity_type === primary && a.opportunity_type !== primary) return 1;
        return 0;
    });
    return qualifying[0];
}

/**
 * Resolve a profile's actual trade direction from `DirectionFamily` ×
 * `Analysis.bias`. Returns `'NEUTRAL'` for Neutral families or when
 * the macro bias is itself Neutral.
 *
 * `TrendRiding` profiles follow the macro bias; `CounterTrend` profiles
 * reverse it. **Zone-presence wins first (4b):** the backend populates
 * exactly one side's zones per profile, so the populated side IS the
 * wire-side resolution (deviation-driven for CounterTrend — Z-Score for
 * MeanReversion, divergence direction for Reversal). The family × bias
 * table below is the fallback for profiles that carry no zones (legacy
 * payloads, neutral bias).
 *
 * This is the wire-side source of truth — the heuristic substring match
 * that previously lived in the RecommendationPanel is removed in favour
 * of this helper.
 */
export function selectProfileSide(
    profile: OpportunityProfile | null | undefined,
    macroBias: MarketBias | null | undefined,
): 'LONG' | 'SHORT' | 'NEUTRAL' {
    if (!profile || !macroBias) return 'NEUTRAL';
    const longZones = profile.long_entry_zone != null && profile.long_entry_zone.low > 0;
    const shortZones = profile.short_entry_zone != null && profile.short_entry_zone.low > 0;
    if (longZones !== shortZones) return longZones ? 'LONG' : 'SHORT';
    const family = profile.direction_family ?? null;
    const isBullish = macroBias === 'Bullish' || macroBias === 'StrongBullish';
    const isBearish = macroBias === 'Bearish' || macroBias === 'StrongBearish';
    switch (family) {
        case 'TREND_RIDING':
            if (isBullish) return 'LONG';
            if (isBearish) return 'SHORT';
            return 'NEUTRAL';
        case 'COUNTER_TREND':
            if (isBullish) return 'SHORT';
            if (isBearish) return 'LONG';
            return 'NEUTRAL';
        case 'NEUTRAL':
        case null:
        case undefined:
            return 'NEUTRAL';
        default:
            return 'NEUTRAL';
    }
}

export interface ProfileZones {
    side: 'LONG' | 'SHORT';
    entry: PriceRange;
    target: PriceRange;
    invalidation: number;
    /** Direction-aware R:R (positive reward / positive risk). `null` when degenerate. */
    rr: number | null;
    /** `true` iff the displayed prices match the side's geometric invariant. */
    geometry_consistent: boolean;
}

// ── R:R vocabulary (v6.10.12, RR-001…006) ──────────────────────────────
// Platform rule: R:R is owned by exactly two layers.
//   - L4 (Opportunity): the GEOMETRIC bracket R:R (reward/risk of the
//     entry/target/invalidation levels). Canonical producer is the
//     backend `compute_side_rr_v2` (target_mid − entry_mid over
//     entry_mid − invalidation).
//   - L6 (Recommendation): the RISK-ADJUSTED decision R:R
//     (geometric × (1 − overall_risk/100), `DecisionContext`).
// L1/L2/L3/L5 never surface R:R.
//
// `RR_MEANINGFUL_FLOOR` (0.1) is the shared degenerate-ratio floor; the
// zones fallback below replicates the backend formula exactly (target_mid,
// not target.low) so a bracket can never display two different R:R values.

/** Minimum economically meaningful R:R (mirror of the backend floor). */
export const RR_MEANINGFUL_FLOOR = 0.1;

export interface ZoneRrResult {
    /** R:R computed from the bracket, `null` when degenerate. */
    rr: number | null;
    /** Why the zones produce no R:R. */
    reason: 'geometry_inverted' | 'below_floor' | null;
}

/**
 * The geometric bracket R:R — a faithful frontend replica of the backend
 * `compute_side_rr_v2` (target_mid / entry_mid / invalidation, floor 0.1).
 * Used as the fallback when the wire R:R is missing or 0, so the fallback
 * always equals what the wire would have produced for the same bracket.
 *
 * v6.10.19b (A2): mirrors the remaining backend guards — the `SlAtEntry`
 * zero-risk epsilon (1e-6 × price, NOT the old 0.01% which rejected real
 * 1.5×ATR stops) and the close-aware `TargetOnWrongSide` check (guards
 * legacy payloads whose target sits on the wrong side of the market).
 */
export function geometricRrFromZones(
    entry: PriceRange,
    target: PriceRange,
    invalidation: number,
    side: 'LONG' | 'SHORT',
    close?: number,
): ZoneRrResult {
    const entryMid = (entry.low + entry.high) / 2;
    const targetMid = (target.low + target.high) / 2;
    // Backend `SlInsideEntry` guard: a stop inside the entry zone is not a
    // bracket (the backend emits R:R 0 for it — the fallback must too).
    if (invalidation >= entry.low && invalidation <= entry.high) {
        return { rr: null, reason: 'geometry_inverted' };
    }
    // Backend `SlAtEntry` guard (B1, v6.10.19b): only a genuinely
    // zero-risk stop (sub-0.0001% of price) is degenerate.
    const riskDir = side === 'LONG' ? entryMid - invalidation : invalidation - entryMid;
    if (Math.abs(riskDir) < 1e-6 * Math.max(entryMid, 1)) {
        return { rr: null, reason: 'geometry_inverted' };
    }
    // Backend `TargetOnWrongSide` guard — needs the close; skip when the
    // caller has no market reference (legacy paths rely on the wire flag).
    if (close != null && isFinite(close)) {
        if (side === 'LONG' && targetMid <= close) {
            return { rr: null, reason: 'geometry_inverted' };
        }
        if (side === 'SHORT' && targetMid >= close) {
            return { rr: null, reason: 'geometry_inverted' };
        }
    }
    const reward = side === 'LONG' ? targetMid - entryMid : entryMid - targetMid;
    const risk = side === 'LONG' ? entryMid - invalidation : invalidation - entryMid;
    if (!isFinite(reward) || !isFinite(risk) || reward <= 0 || risk <= 0) {
        return { rr: null, reason: 'geometry_inverted' };
    }
    const ratio = reward / risk;
    if (!isFinite(ratio)) {
        return { rr: null, reason: 'geometry_inverted' };
    }
    if (ratio < RR_MEANINGFUL_FLOOR) {
        return { rr: null, reason: 'below_floor' };
    }
    return { rr: Math.round(ratio * 100) / 100, reason: null };
}

export interface ResolvedRr {
    /** The geometric (L4) bracket R:R for the resolved active side. */
    value: number;
    /** `false` → surfaces render `N/A`. */
    available: boolean;
    /** Human-readable N/A reason (rendered as tooltip/sub-line). */
    reason: string | null;
    /** Which source produced the value. */
    source: 'profile_wire' | 'matrix_wire' | 'zones' | null;
    /** The L6 risk-adjusted decision R:R when real, else `null`. */
    riskAdjusted: number | null;
}

/**
 * The single R:R resolver — every surface (cards, chips, R:R Internal,
 * exports, plan strip) reads the active side's geometric R:R through this
 * chain:
 *
 *   1. top (or given) profile's wire per-side `expected_rr_internal`,
 *   2. the matrix-level per-side wire value (bias side),
 *   3. the zones fallback with the exact backend formula
 *      (`geometricRrFromZones`), so it always equals the wire value.
 *
 * The risk-adjusted L6 value (`DecisionContext.expected_reward_risk_ratio`)
 * is carried separately — it is the decision number, never a bracket
 * number.
 */
export function resolveActiveRr(
    opportunity: OpportunityMatrix | null | undefined,
    decisionContext?: DecisionContext | null,
    analysis?: AnalysisMatrix | null,
    profileOverride?: OpportunityProfile | null,
    biasOverride?: MarketBias | null,
    close?: number,
    sideOverride?: 'LONG' | 'SHORT',
): ResolvedRr {
    const riskAdjusted =
        decisionContext?.expected_reward_risk_ratio != null && decisionContext.expected_reward_risk_ratio > 0
            ? decisionContext.expected_reward_risk_ratio
            : null;
    if (!opportunity) {
        return { value: 0, available: false, reason: 'no directional bias', source: null, riskAdjusted };
    }
    const bias = biasOverride ?? decisionContext?.bias ?? analysis?.bias ?? null;
    const top = profileOverride ?? topQualifyingProfile(opportunity);
    // v6.10.19b (B1): `sideOverride` forces the resolved side (the
    // verdict-consistent headline) when the top qualifying profile's own
    // side differs (countertrend setups under a directional verdict).
    const side: 'LONG' | 'SHORT' | 'NEUTRAL' = sideOverride ?? (top
        ? selectProfileSide(top, bias)
        : bias === 'Bullish' || bias === 'StrongBullish'
          ? 'LONG'
          : bias === 'Bearish' || bias === 'StrongBearish'
            ? 'SHORT'
            : 'NEUTRAL');
    if (side === 'NEUTRAL') {
        return { value: 0, available: false, reason: 'no directional bias', source: null, riskAdjusted };
    }
    const topVal = top
        ? (side === 'LONG' ? top.long_expected_rr_internal : top.short_expected_rr_internal) ?? null
        : null;
    const matrixVal =
        side === 'LONG'
            ? opportunity.long_expected_rr_internal ?? 0
            : opportunity.short_expected_rr_internal ?? 0;
    const wireRr = topVal ?? matrixVal;
    const wireSource: 'profile_wire' | 'matrix_wire' = topVal != null ? 'profile_wire' : 'matrix_wire';
    if (wireRr >= RR_MEANINGFUL_FLOOR) {
        return { value: wireRr, available: true, reason: null, source: wireSource, riskAdjusted };
    }
    if (wireRr > 0) {
        return { value: 0, available: false, reason: 'below the 0.10 meaningfulness floor', source: wireSource, riskAdjusted };
    }
    // v6.10.19b (A2): respect the server's geometry verdict — a bracket the
    // backend flagged GeometryInverted must NOT leak a locally-recomputed
    // R:R (observed: "GEOMETRY INVERTED" cards displaying a bogus 0.75).
    const serverConsistent =
        side === 'LONG'
            ? (top?.long_geometry_consistent ?? opportunity.long_geometry_consistent)
            : (top?.short_geometry_consistent ?? opportunity.short_geometry_consistent);
    if (serverConsistent === false) {
        return { value: 0, available: false, reason: 'geometry inverted', source: 'zones', riskAdjusted };
    }
    const entry =
        side === 'LONG'
            ? (top?.long_entry_zone ?? opportunity.long_entry_zone)
            : (top?.short_entry_zone ?? opportunity.short_entry_zone);
    const target =
        side === 'LONG'
            ? (top?.long_target_zone ?? opportunity.long_target_zone)
            : (top?.short_target_zone ?? opportunity.short_target_zone);
    const inv =
        side === 'LONG'
            ? (top?.long_invalidation_level ?? opportunity.long_invalidation_level)
            : (top?.short_invalidation_level ?? opportunity.short_invalidation_level);
    if (entry && target && entry.low > 0 && entry.high > 0 && target.low > 0 && target.high > 0 && inv > 0) {
        const zone = geometricRrFromZones(entry, target, inv, side, close);
        if (zone.rr != null) {
            return { value: zone.rr, available: true, reason: null, source: 'zones', riskAdjusted };
        }
        return {
            value: 0,
            available: false,
            reason: zone.reason === 'below_floor' ? 'below the 0.10 meaningfulness floor' : 'geometry inverted',
            source: 'zones',
            riskAdjusted,
        };
    }
    return { value: 0, available: false, reason: 'no valid bracket', source: null, riskAdjusted };
}

/**
 * The human-readable R:R discount explanation (RR-008, v6.10.14), e.g.
 * `"Risk-adjusted: net R:R 12.90 × risk factor 0.15 = 1.93"`.
 * v6.10.19 (P5): the base value is the NET R:R (gross minus estimated
 * entry/exit fees + slippage) — the value the operator can actually
 * expect after costs.
 * `null` when either value is absent or the factor is trivial (values
 * identical). Shared by the L6 header chip tooltip and the recommendation
 * export so screen and JSON always carry the identical sentence.
 */
export function riskAdjRrExplanation(geometricRr: number, riskAdjustedRr: number): string | null {
    if (riskAdjustedRr <= 0 || geometricRr <= 0) return null;
    if (Math.abs(geometricRr - riskAdjustedRr) < 1e-9) return null;
    const factor = riskAdjustedRr / geometricRr;
    return `Risk-adjusted: net R:R ${geometricRr.toFixed(2)} × risk factor ${factor.toFixed(2)} = ${riskAdjustedRr.toFixed(2)}`;
}

/**
 * Read the resolved per-side zones directly from an `OpportunityProfile`.
 * Returns `null` when the profile has no zones for that side (legacy
 * payloads, Neutral family, or un-met preconditions).
 */
export function profileZones(
    profile: OpportunityProfile | null | undefined,
    side: 'LONG' | 'SHORT',
    close?: number,
): ProfileZones | null {
    if (!profile) return null;
    const entry = side === 'LONG' ? profile.long_entry_zone : profile.short_entry_zone;
    const target = side === 'LONG' ? profile.long_target_zone : profile.short_target_zone;
    const inv = side === 'LONG' ? profile.long_invalidation_level : profile.short_invalidation_level;
    if (!entry || !target || entry.low <= 0 || entry.high <= 0 || !inv || inv <= 0) {
        return null;
    }
    // B3 (v6.10.12): the zones R:R uses the SAME mid-based formula as the
    // backend wire (`geometricRrFromZones`) — the legacy target.low-based
    // recomputation could disagree with the wire for the same bracket.
    const zone = geometricRrFromZones(entry, target, inv, side, close);
    // Prefer server-side geometry flag when present; fall back to local check.
    const serverConsistent =
        side === 'LONG' ? profile.long_geometry_consistent : profile.short_geometry_consistent;
    const geometry_consistent = serverConsistent ?? zone.reason !== 'geometry_inverted';
    const rr = geometry_consistent ? zone.rr : null;
    return { side, entry, target, invalidation: inv, rr, geometry_consistent };
}

/**
 * Read the aggregated per-direction bracket from `OpportunityMatrix`.
 * Used as a **fallback** when per-profile zones are absent — the
 * aggregated fields always have a value (even the Neutral sentinel
 * pinned to close when bias is Neutral).
 */
export function aggregateZones(
    opportunity: OpportunityMatrix | null | undefined,
    side: 'LONG' | 'SHORT',
    close?: number,
): ProfileZones | null {
    if (!opportunity) return null;
    const entry = side === 'LONG' ? opportunity.long_entry_zone : opportunity.short_entry_zone;
    const target = side === 'LONG' ? opportunity.long_target_zone : opportunity.short_target_zone;
    const inv = side === 'LONG' ? opportunity.long_invalidation_level : opportunity.short_invalidation_level;
    // v6.10.x (Bug A guard): reject zones where any bound is non-positive.
    // The Rust backend now enforces this invariant on the wire
    // (`crates/market-analyzer/src/synthesis.rs` `derive_side_zones`
    // floors + `collect_candidate_levels` filters `v > 0.0` on every
    // push). The frontend guards here are a second layer of defence:
    // if a stale snapshot from an older build sneaks through, or a
    // future indicator source bypasses the Rust filter, the panel
    // degrades to `—` instead of surfacing `$0–$X`.
    if (
        !entry || !target
        || entry.low <= 0 || entry.high <= 0
        || target.low <= 0 || target.high <= 0
        || !inv || inv <= 0
    ) {
        return null;
    }
    // Prefer server-side matrix-level geometry flag when present.
    const serverConsistent =
        side === 'LONG' ? opportunity.long_geometry_consistent : opportunity.short_geometry_consistent;
    // B3 (v6.10.12): the consistency check uses the SAME mid-based formula
    // as the backend wire (delegated into profileZones → geometricRrFromZones).
    const zone = geometricRrFromZones(entry, target, inv, side, close);
    const geometry_consistent = serverConsistent ?? zone.reason !== 'geometry_inverted';
    return profileZones(
        {
            opportunity_type: '__aggregate__',
            score: opportunity.opportunity_score,
            preconditions_met: 0,
            preconditions_total: 0,
            notes: '',
            direction_family: null,
            long_entry_zone: entry,
            long_target_zone: target,
            long_invalidation_level: inv,
            long_expected_rr_internal: null,
            long_geometry_consistent: serverConsistent,
            short_entry_zone: entry,
            short_target_zone: target,
            short_invalidation_level: inv,
            short_expected_rr_internal: null,
            short_geometry_consistent: serverConsistent,
            trade_viability: null,
        } as OpportunityProfile,
        side,
        close,
    );
}

/**
 * v6.10.21 (NBR): per-direction aggregated reference bracket for one
 * folder — the matrix's per-side zones (`aggregateZones`) with the
 * shared R:R resolver (`sideOverride` pins the resolved side). Emitted
 * ONLY when the folder hosts zero qualifying setup cards, so the
 * operator always sees valid reference geometry per direction.
 * `below_floor` flags a sub-1.0 R:R (State D), `rr_reason` carries the
 * N/A cause when the bracket is geometrically invalid (State D).
 */
export interface SideBracketSummary {
    opportunity_type: 'AggregatedBracket';
    direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    zones: ProfileZones | null;
    rr: number | null;
    /** Human-readable N/A reason, `null` when R:R is available. */
    rr_reason: string | null;
    below_floor: boolean;
    rationale: string;
}

export function sideBracketSummary(
    opportunity: OpportunityMatrix | null | undefined,
    decisionContext?: DecisionContext | null,
    analysis?: AnalysisMatrix | null,
    side: 'LONG' | 'SHORT' = 'LONG',
    close?: number,
): SideBracketSummary | null {
    if (!opportunity) return null;
    const zones = aggregateZones(opportunity, side, close);
    if (!zones) return null;
    const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, null, null, close, side);
    const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
    return {
        opportunity_type: 'AggregatedBracket',
        direction: side,
        zones,
        rr,
        rr_reason: resolvedRr.available ? null : resolvedRr.reason,
        below_floor: resolvedRr.available && resolvedRr.value < 1.0,
        rationale: `aggregated bracket (informational — no qualifying ${side === 'LONG' ? 'long' : 'short'} profile)`,
    };
}

/**
 * v6.10.21 (NBR): the backend-emitted direction-agnostic range reference
 * bracket (`OpportunityMatrix.neutral_reference_bracket`) as a card
 * summary for the Range folder. `rr` trusts the wire (the backend ran
 * the same `compute_side_rr_v2` guards); `below_floor` flags a sub-1.0
 * net R:R, `rr_reason` explains an invalid frame (State D). Returns
 * `null` when the wire carries no neutral bracket.
 */
export interface NeutralBracketSummary {
    opportunity_type: 'NeutralBracket';
    direction: 'NEUTRAL';
    zones: ProfileZones | null;
    rr: number | null;
    rr_reason: string | null;
    below_floor: boolean;
    rationale: string;
}

export function neutralBracketSummary(
    opportunity: OpportunityMatrix | null | undefined,
): NeutralBracketSummary | null {
    const nb = opportunity?.neutral_reference_bracket;
    if (!nb) return null;
    if (
        !nb.entry_zone
        || !nb.target_zone
        || nb.entry_zone.low <= 0
        || nb.entry_zone.high <= 0
        || nb.target_zone.low <= 0
        || nb.target_zone.high <= 0
        || nb.invalidation_level <= 0
    ) {
        return null;
    }
    const geometry_consistent = nb.geometry_consistent !== false;
    const rrRaw = geometry_consistent && nb.expected_rr_internal > RR_MEANINGFUL_FLOOR
        ? nb.expected_rr_internal
        : null;
    return {
        opportunity_type: 'NeutralBracket',
        direction: 'NEUTRAL',
        zones: {
            side: 'LONG',
            entry: nb.entry_zone,
            target: nb.target_zone,
            invalidation: nb.invalidation_level,
            rr: rrRaw,
            geometry_consistent,
        },
        rr: rrRaw != null ? Math.round(rrRaw * 100) / 100 : null,
        rr_reason: rrRaw != null ? null : geometry_consistent ? 'below the 0.10 meaningfulness floor' : 'geometry inverted',
        below_floor: rrRaw != null && rrRaw < 1.0,
        rationale: nb.rationale,
    };
}

/**
 * Resolve the zones for the top setup, falling back from per-profile to
 * aggregated when the per-profile zones are absent. Returns:
 *   - `zones`: never null when `opportunity` is present (uses aggregate
 *     fallback so every Top Setup card carries ENTRY/TARGET/SL/R:R).
 *   - `rr`: the per-side R:R from `opportunity.long_expected_rr_internal`
 *     or `short_expected_rr_internal` (the wire-side truth); falls back
 *     to the geometric R:R derived from the zones when both are zero.
 *   - `side`: the resolved direction for the top setup, or `'NEUTRAL'`
 *     when no resolvable direction exists.
 *   - `viability`: `'Actionable' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear'`.
 *   - `rationale`: a single user-facing line (cleaned of raw/ratio).
 *
 * The Recommendation panel calls this helper so it always renders
 * ENTRY/TARGET/SL/R:R — even when the verdict is HOLD or the top
 * profile has no actionable per-side zones.
 */
export interface TopSetupSummary {
    opportunity_type: string;
    score: number;
    /** v6.14: backend-emitted precondition-scaled score — the number the
     *  operator sees everywhere (Opportunities panel, this card, exports).
     *  `null` on legacy payloads / sentinel summaries — render `score`. */
    display_score: number | null;
    preconditions_met: number;
    preconditions_total: number;
    direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    viability: 'Actionable' | 'Qualifying' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear';
    zones: ProfileZones | null;
    rr: number | null;
    /** Human-readable N/A reason — rendered as a tooltip on the card. */
    rr_reason: string | null;
    rationale: string;
    /** v6.10.19 (T3): the aggregated reference bracket's R:R is below the
     *  1.0 actionable floor — levels stay visible but the card is demoted
     *  to "Reference Bracket (Below Actionable Floor)", never "Top Setup". */
    below_floor: boolean;
    /** v6.10.19b (B1): qualifying setups that did NOT make the headline
     *  (counter-bias under a directional verdict, or any qualifying setup
     *  under a HOLD verdict) — surfaced as informational alternatives so
     *  nothing the Opportunities panel shows is ever hidden from the
     *  Recommendation. Empty when the headline already carries them. */
    alternate_setups: AlternateSetupInfo[];
    /** v6.10.19b (B3): the setup horizon (INTRADAY / SWING / POSITION). */
    horizon: string | null;
}

/** v6.10.19b (B1): a qualifying setup surfaced as an alternative to the
 *  verdict-consistent headline. */
export interface AlternateSetupInfo {
    opportunity_type: string;
    side: 'LONG' | 'SHORT' | 'NEUTRAL';
    score: number;
    /** v6.14: backend-emitted precondition-scaled score (legacy: null). */
    display_score: number | null;
    preconditions_met: number;
    preconditions_total: number;
}

export function topSetupSummary(
    opportunity: OpportunityMatrix | null | undefined,
    analysis: AnalysisMatrix | null | undefined,
    decisionContext?: DecisionContext | null,
    verdictTop?: 'LONG' | 'SHORT' | 'HOLD',
    close?: number,
): TopSetupSummary | null {
    if (!opportunity) return null;
    const horizon = opportunity?.time_horizon ?? null;
    const macroBias = decisionContext?.bias ?? analysis?.bias ?? null;
    const qualifying = (opportunity.profiles ?? [])
        .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
        .slice()
        .sort((a, b) => b.score - a.score);
    const sideOf = (p: OpportunityProfile): 'LONG' | 'SHORT' | 'NEUTRAL' =>
        selectProfileSide(p, macroBias);
    const altOf = (p: OpportunityProfile): AlternateSetupInfo => ({
        opportunity_type: p.opportunity_type,
        side: sideOf(p),
        score: p.score,
        display_score: p.display_score ?? null,
        preconditions_met: p.preconditions_met,
        preconditions_total: p.preconditions_total,
    });

    // v6.10.19b (B1) + v6.10.19c (D3): the verdict-consistency path —
    // used by the Recommendation panel/export (which always pass
    // `verdictTop`). The headline ALWAYS mirrors what the panel should
    // present:
    //   - directional verdict → best qualifying profile ON that side, else
    //     the verdict-side aggregated reference bracket (informational);
    //   - HOLD verdict + qualifying profiles → the top qualifying profile
    //     (any side, incl. NEUTRAL) headlines the container — a real
    //     setup is never hidden behind a placeholder;
    //   - HOLD verdict + no qualifying → the "No Active Setup" empty
    //     container (fields null).
    // Every qualifying profile that is not the headline rides in
    // `alternate_setups` so valid setups are never lost.
    if (verdictTop) {
        if (verdictTop === 'HOLD') {
            const headline = qualifying[0] ?? null;
            if (!headline) {
                return {
                    opportunity_type: 'NoActiveSetup',
                    score: 0,
                    display_score: null,
                    preconditions_met: 0,
                    preconditions_total: 0,
                    direction: 'NEUTRAL',
                    viability: 'NoClear',
                    zones: null,
                    rr: null,
                    rr_reason: 'no active setup',
                    // v6.10.19d (D): the "fields are placeholders" caveat
                    // was erased — the panel renders nothing extra.
                    rationale: '',
                    below_floor: false,
                    alternate_setups: [],
                    horizon,
                };
            }
            // Case B (D3/D5): the headline setup — displayed side may be
            // NEUTRAL, but the bracket (zones/R:R) comes from its
            // populated side so a NEUTRAL card never blanks its R:R.
            const displayedSide = sideOf(headline);
            const bracketSide: 'LONG' | 'SHORT' | null =
                profileZones(headline, 'LONG', close) != null
                    ? 'LONG'
                    : profileZones(headline, 'SHORT', close) != null
                      ? 'SHORT'
                      : null;
            const zones = bracketSide ? profileZones(headline, bracketSide, close) : null;
            const resolvedRr = bracketSide
                ? resolveActiveRr(opportunity, decisionContext, analysis, headline, macroBias, close, bracketSide)
                : { value: 0, available: false, reason: 'no active setup', source: null, riskAdjusted: null };
            const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
            const rrReason = resolvedRr.available ? null : resolvedRr.reason;
            const viability = normalizeViability(
                headline.trade_viability ?? (headline.preconditions_met > 0 ? 'Qualifying' : 'NoClear'),
            ) as TopSetupSummary['viability'];
            const effectiveViability =
                viability === 'Actionable' && (rr == null || rr < 1) ? 'Qualifying' : viability;
            return {
                opportunity_type: headline.opportunity_type,
                score: headline.score,
                display_score: headline.display_score ?? null,
                preconditions_met: headline.preconditions_met,
                preconditions_total: headline.preconditions_total,
                direction: displayedSide,
                viability: effectiveViability,
                zones,
                rr,
                rr_reason: rrReason,
                rationale: `${headline.opportunity_type}: preconditions ${headline.preconditions_met}/${headline.preconditions_total}`,
                below_floor: false,
                alternate_setups: qualifying.slice(1).map(altOf),
                horizon,
            };
        }
        const headline = qualifying.find((p) => sideOf(p) === verdictTop) ?? null;
        if (!headline) {
            const zones = aggregateZones(opportunity, verdictTop, close);
            const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, null, macroBias, close, verdictTop);
            const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
            const below_floor = resolvedRr.available && resolvedRr.value < 1.0;
            return {
                opportunity_type: 'AggregatedBracket',
                score: 0,
                display_score: null,
                preconditions_met: 0,
                preconditions_total: 0,
                direction: verdictTop,
                viability: 'NoClear',
                zones,
                rr,
                rr_reason: resolvedRr.available ? null : resolvedRr.reason,
                rationale: `aggregated bracket (informational — no qualifying ${verdictTop.toLowerCase()} profile)`,
                below_floor,
                alternate_setups: qualifying.map(altOf),
                horizon,
            };
        }
        const side = verdictTop;
        const zones = profileZones(headline, side, close) ?? aggregateZones(opportunity, side, close);
        const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, headline, macroBias, close, side);
        const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
        const rrReason = resolvedRr.available ? null : resolvedRr.reason;
        const viability = normalizeViability(
            headline.trade_viability ?? (headline.preconditions_met > 0 ? 'Qualifying' : 'NoClear'),
        ) as TopSetupSummary['viability'];
        const effectiveViability =
            viability === 'Actionable' && (rr == null || rr < 1) ? 'Qualifying' : viability;
        return {
            opportunity_type: headline.opportunity_type,
            score: headline.score,
            display_score: headline.display_score ?? null,
            preconditions_met: headline.preconditions_met,
            preconditions_total: headline.preconditions_total,
            direction: side,
            viability: effectiveViability,
            zones,
            rr,
            rr_reason: rrReason,
            rationale: `${headline.opportunity_type}: preconditions ${headline.preconditions_met}/${headline.preconditions_total}`,
            below_floor: false,
            alternate_setups: qualifying.filter((p) => p !== headline).map(altOf),
            horizon,
        };
    }

    // ── Legacy path (no verdictTop — unchanged behaviour) ───────────────
    const top = topQualifyingProfile(opportunity);
    if (!top) {
        // v6.10.17 (A3): no qualifying profile (e.g. No Clear — every
        // profile has 0/N preconditions) — the AGGREGATED bracket is still
        // published on the bias side so the operator always has TPs/SLs/R:R
        // to work with, explicitly marked weak/informational. The side
        // resolves from the bias first, then net_bias_pct, then NEUTRAL —
        // fixing the legacy blind-LONG default at exactly 0.
        let side: 'LONG' | 'SHORT' | 'NEUTRAL' =
            macroBias === 'Bullish' || macroBias === 'StrongBullish'
                ? 'LONG'
                : macroBias === 'Bearish' || macroBias === 'StrongBearish'
                  ? 'SHORT'
                  : 'NEUTRAL';
        if (side === 'NEUTRAL') {
            const net = decisionContext?.net_bias_pct ?? 0;
            side = net < 0 ? 'SHORT' : net > 0 ? 'LONG' : 'NEUTRAL';
        }
        const zones = side === 'NEUTRAL' ? null : aggregateZones(opportunity, side, close);
        const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, null, macroBias, close);
        const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
        // v6.10.19 (T3): a sub-1.0 bracket is NEVER framed as a trade —
        // levels remain visible for reference, but the card demotes.
        const below_floor = resolvedRr.available && resolvedRr.value < 1.0;
        return {
            opportunity_type: 'AggregatedBracket',
            score: 0,
            display_score: null,
            preconditions_met: 0,
            preconditions_total: 0,
            direction: side,
            viability: 'NoClear',
            zones,
            rr,
            rr_reason: resolvedRr.available ? null : resolvedRr.reason,
            rationale: 'aggregated bracket (informational — no qualifying profile)',
            below_floor,
            alternate_setups: [],
            horizon,
        };
    }
    // R4: the macro bias prefers the DecisionContext mirror (the
    // same-candle field the verdict/probabilities come from). The
    // analysis matrix and the decision context are written atomically
    // per frame, but partial frames / warmup can leave `analysis.bias`
    // one candle behind — which made the card direction contradict the
    // gauge (observed: NEUTRAL card under a +44% LONG gauge).
    const side = selectProfileSide(top, macroBias);
    // Try per-profile zones first; fall back to aggregated.
    let zones = side === 'NEUTRAL' ? null : profileZones(top, side, close);
    if (!zones) {
        // When no directional resolution, prefer the gauge direction
        // over a blind LONG default. This fixes the inversion bug where
        // a SHORT gauge (-14%) showed LONG entry/target geometry.
        // Uses net_bias_pct (not score) because the score is signed_confluence
        // which is 0 for Neutral bias, while net_bias_pct reflects the
        // geometric offset + modulation that drives the actual gauge.
        const fallbackSide = side === 'NEUTRAL'
            ? (decisionContext?.net_bias_pct ?? 0) < 0 ? 'SHORT' : 'LONG'
            : side;
        zones = aggregateZones(opportunity, fallbackSide, close);
    }
    // R:R via the shared resolver (RR-002, v6.10.12): profile wire →
    // matrix wire → zones fallback with the exact backend formula. The
    // resolver carries the human-readable N/A reason.
    const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, top, macroBias, close);
    const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
    const rrReason = resolvedRr.available ? null : resolvedRr.reason;
    // Viability — wire-side default to `NoClear` when missing, except
    // that a qualifying profile (preconditions met) reads QUALIFYING
    // (v6.10.17 P1 — a real bracket is never labelled "no clear setup").
    // v6.10.18 (I-5): ACTIONABLE additionally requires R:R ≥ 1.0 — a
    // legacy payload that says ACTIONABLE with a sub-1 bracket is
    // re-derated to QUALIFYING (a real bracket, no edge to act on).
    const viability = normalizeViability(
        top.trade_viability ?? (top.preconditions_met > 0 ? 'Qualifying' : 'NoClear'),
    ) as TopSetupSummary['viability'];
    const effectiveViability =
        viability === 'Actionable' && (rr == null || rr < 1) ? 'Qualifying' : viability;
    // Clean rationale (no raw/ratio debug strings).
    const rationale = `${top.opportunity_type}: preconditions ${top.preconditions_met}/${top.preconditions_total}`;
    return {
        opportunity_type: top.opportunity_type,
        score: top.score,
        display_score: top.display_score ?? null,
        preconditions_met: top.preconditions_met,
        preconditions_total: top.preconditions_total,
        direction: side,
        viability: effectiveViability,
        zones,
        rr,
        rr_reason: rrReason,
        rationale,
        below_floor: false,
        alternate_setups: [],
        horizon,
    };
}

/**
 * Resolve the zones for ANY qualifying profile, falling back from
 * per-profile to aggregated. Used by the Opportunities panel so every
 * qualifying card carries ENTRY/TARGET/SL/R:R.
 */
export function profileSummary(
    profile: OpportunityProfile | null | undefined,
    opportunity: OpportunityMatrix | null | undefined,
    analysis: AnalysisMatrix | null | undefined,
    decisionContext?: DecisionContext | null,
    close?: number,
): {
    side: 'LONG' | 'SHORT' | 'NEUTRAL';
    zones: ProfileZones | null;
    rr: number | null;
    /** Human-readable N/A reason from the shared resolver — the L4 card
     * must not fall back to a hardcoded "no_actionable_geometry" when the
     * true cause is a missing directional bias (v6.10.16 FIX-O4). */
    rr_reason: string | null;
    viability: 'Actionable' | 'Qualifying' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear';
} {
    if (!profile) {
        return { side: 'NEUTRAL', zones: null, rr: null, rr_reason: 'no directional bias', viability: 'NoClear' };
    }
    // R4: the macro bias prefers the DecisionContext mirror — see
    // `topSetupSummary` for the rationale.
    const macroBias = decisionContext?.bias ?? analysis?.bias ?? null;
    const side = selectProfileSide(profile, macroBias);
    let zones = side === 'NEUTRAL' ? null : profileZones(profile, side, close);
    if (!zones) {
        // When no directional resolution, prefer the gauge direction
        // over a blind LONG default (same fix as topSetupSummary).
        // Uses net_bias_pct (not score) — see topSetupSummary for rationale.
        const fallbackSide = side === 'NEUTRAL'
            ? (decisionContext?.net_bias_pct ?? 0) < 0 ? 'SHORT' : 'LONG'
            : side;
        zones = aggregateZones(opportunity, fallbackSide, close);
    }
    // R:R via the shared resolver (RR-002, v6.10.12) — the same chain as
    // the Top Setup card: profile wire → matrix wire → zones fallback
    // with the exact backend formula.
    const resolvedRr = resolveActiveRr(opportunity, decisionContext, analysis, profile, macroBias, close);
    const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
    const rr_reason = resolvedRr.available ? null : resolvedRr.reason;
    // v6.10.17 (P1): a profile with met preconditions but a null wire
    // viability is QUALIFYING (a real bracket), not NoClear.
    // v6.10.18 (I-5): ACTIONABLE additionally requires R:R ≥ 1.0.
    const viability = normalizeViability(
        profile.trade_viability ?? (profile.preconditions_met > 0 ? 'Qualifying' : 'NoClear'),
    ) as 'Actionable' | 'Qualifying' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear';
    const effectiveViability =
        viability === 'Actionable' && (rr == null || rr < 1) ? 'Qualifying' : viability;
    return { side, zones, rr, rr_reason, viability: effectiveViability };
}

/** Title-case a setup token for prose (`TrendContinuation` → `Trend Continuation`). */
function prettifySetupType(s: string): string {
    return s
        .replace(/([a-z])([A-Z])/g, '$1 $2')
        .trim();
}

function buildRationale(r: RationaleInputs): string[] {
    const out: string[] = [];

    // 1. Bias + score
    // v6.10.16: when bias is Neutral the signed `score` is 0 by design
    // (`DecisionContext` zeroes direction under Neutral bias) — the
    // underlying unsigned blend is NOT 0 (≈ score_confidence × 100).
    // State both so the why-line never misattributes the zero to the
    // L2/L3/L4 blend.
    const unsignedBlend =
        r.scoreConfidence != null && r.scoreConfidence > 0
            ? Math.round(r.scoreConfidence * 100)
            : null;
    const driverSuffix =
        r.bias === 'Neutral' && r.top !== 'HOLD'
            ? `; math-driven by confluence score ${r.score.toFixed(0)} (wire bias reads Neutral)`
            : '';
    const signedNote =
        r.bias === 'Neutral' && unsignedBlend != null
            ? `; signed 0 because the Neutral wire bias zeroes the directional blend (unsigned ≈ ${unsignedBlend})`
            : '';
    out.push(
        `${r.bias} market bias: confluence score of ${r.score.toFixed(0)} (derived from the Layer 2 Tradability Dimension, Layer 3 Quality Score, and Layer 4 Opportunity Score)${signedNote}${driverSuffix}`,
    );

    // 2. Setup identification — polished prose (v6.17): no L-tokens,
    // no raw camelCase setup types.
    out.push(
        `Active setup: ${prettifySetupType(r.setupType)} (Layer 4 Opportunity Score of ${Math.round(r.oppScore)}, classified as ${r.setupQuality} quality)`,
    );

    // 3. Trade readiness gate — reports the actual reason, not a
    // hardcoded threshold. STAND_ASIDE can be triggered by
    // entry_danger ≥ 70 OR confidence_assessment < 20. (v6.17: sentence
    // case + clean colons; the danger level renders as `Entry Danger`.)
    if (r.readiness === 'STAND_ASIDE') {
        if (r.entryDanger >= 70) {
            out.push(
                `Trade readiness is Stand aside: Entry Danger of ${r.entryDanger.toFixed(0)} (${dangerLabel(r.entryDanger)}) is at or above the 70 threshold`,
            );
        } else if (r.confidence < 20) {
            out.push(
                `Trade readiness is Stand aside: Confidence assessment of ${r.confidence.toFixed(0)}% is below 20`,
            );
        } else {
            out.push(
                `Trade readiness is Stand aside: Entry Danger of ${r.entryDanger.toFixed(0)} (${dangerLabel(r.entryDanger)}), confidence ${r.confidence.toFixed(0)}%`,
            );
        }
    } else if (r.readiness === 'FORMING' && r.expectedRr < 1.0) {
        // R3: when the verdict is HOLD and the risk-discounted R:R is 0,
        // the chips render N/A — the bullet must not quote a "0.00"
        // that contradicts them.
        out.push(
            r.top === 'HOLD' && r.expectedRr === 0
                ? 'Trade readiness is Forming: risk-adjusted reward-to-risk is not available; the directional score was capped by the missing ratio'
                : `Trade readiness is Forming: risk-adjusted reward-to-risk of ${r.expectedRr.toFixed(2)} (below the 1.0 floor) capped the directional score`,
        );
    } else if (r.readiness === 'FORMING') {
        out.push(
            `Trade readiness is Forming: Entry Danger of ${r.entryDanger.toFixed(0)} (${dangerLabel(r.entryDanger)}) is acceptable but the market state is mid-form`,
        );
    } else if (r.readiness === 'WATCH') {
        out.push(
            `Trade readiness is Watch: Entry Danger of ${r.entryDanger.toFixed(0)} (${dangerLabel(r.entryDanger)}) requires additional confirmation before full execution`,
        );
    } else {
        out.push(`Trade readiness is Ready: Entry Danger of ${r.entryDanger.toFixed(0)} (${dangerLabel(r.entryDanger)}) is acceptable`);
    }

    // 4. R:R
    if (r.expectedRr < 1.0) {
        out.push(
            r.top === 'HOLD' && r.expectedRr === 0
                ? 'Risk-adjusted reward-to-risk: not available; no actionable directional R:R'
                : `Risk-adjusted reward-to-risk: ${r.expectedRr.toFixed(2)} (below the 1.0 floor; ${r.top} score capped at 60% of pre-normalized total)`,
        );
    } else {
        out.push(`Risk-adjusted reward-to-risk: ${r.expectedRr.toFixed(2)}`);
    }

    // 5. Contributing indicators
    if (r.supporters.length > 0) {
        out.push(`Contributing indicators: ${r.supporters.slice(0, 6).join(', ')}`);
    }

    // 6. Supporting signals from L3
    if (r.supportingSignals.length > 0) {
        out.push(`Layer 3 supporting signals: ${r.supportingSignals.slice(0, 4).join(', ')}`);
    }

    // 7. Contradicting signals (if any)
    if (r.contradictingSignals.length > 0) {
        out.push(`Contradicting signals: ${r.contradictingSignals.slice(0, 3).join(', ')}`);
    }

    // 8. Stance × guidance
    out.push(`Directional guidance: ${r.guidance}  ·  Market stance: ${r.stance}`);

    return out;
}
