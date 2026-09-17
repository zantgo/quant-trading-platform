// Trade Plan builders — pure helpers for the L4/L6 surfaces.
//
// `deriveTradePlan()` consumes the same wire-format payloads the existing
// RecommendationPanel, OpportunitiesPanel, and StructuralAnchorsStrip already read,
// and produces a single `TradePlan` view that the Metrics tab and the
// Decision tab can render without re-implementing the math.
//
// The plan is **strategy-agnostic**: it is shape (entry/target/stop) not
// authorisation. The Trade Automation Engine decides whether to dispatch.
// Tools like `applyPlanToConsole()` pre-fill the BottomConsole bracket
// creator for manual confirmation.

import type { AdvisoryMatrix, AnalysisMatrix, ConfluentLevel, DecisionContext, MarketContext, OpportunityMatrix, TimeframeTelemetry } from '../types';
import { resolveActiveRr } from './decisionRank';

export type SourceTag = 'FIB' | 'VP' | 'PP' | 'SR' | 'LIQ' | 'ATR' | 'NONE';

export interface TradeTarget {
    label: string;          // "TP1" / "TP2" / "TP3"
    price: number;
    sizePct: number;         // 0..100
    rrRatio: number | null; // R:R for this target vs entry mid
    source: 'L4_TARGET_ZONE' | 'FIB_EXT_1618' | 'FIB_EXT_2618' | 'CONFLUENT' | 'NONE';
    confluenceCount?: number;
}

export interface TradeStop {
    price: number;
    distancePct: number;     // 0..100  pct from entry mid
    method: 'STRUCTURE_BASED' | 'VOLATILITY_BASED' | 'ATR_BASED' | 'SR_BASED' | 'NONE';
    fallbackPrice?: number;
    source: 'L4_INVALIDATION' | 'CONFLUENT' | 'PCT_FALLBACK' | 'NONE';
    evidenceNote?: string;
}

export interface TradePlan {
    symbol: string;
    direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    setupType: string;          // "TrendContinuation" / "Pullback" / etc
    setupScore: number;         // 0..100
    setupQuality: string;       // "PRIME" / "STRONG" / ...
    timeHorizon: 'SCALP' | 'INTRADAY' | 'SWING' | 'POSITION';
    readiness: 'READY' | 'FORMING' | 'WATCH' | 'STAND_ASIDE' | 'UNKNOWN';
    confidencePct: number;      // 0..100  (terminal confidence_assessment)
    rrRatio: number;            // expected_reward_risk_ratio
    riskDiscount: number;       // overall_risk.score
    entryMid: number;           // midpoint of entry zone
    entryZone: { low: number; high: number };
    entrySources: { tag: SourceTag; price: number; strength: number }[];
    targets: TradeTarget[];
    stop: TradeStop | null;
    entryGuidance: string;
    exitGuidance: string;
    targetStrategy: string;
    protectionStrategy: string;
    contributors: string[];
    actionable: boolean;
    actionabilityReason: string;
}

// ───────────────────────────── formatters ─────────────────────────────

function fmtPrice(num: number): number {
    if (!isFinite(num) || num <= 0) return 0;
    return Math.round(num * 100) / 100;
}

function clampPct(n: number): number {
    if (!isFinite(n)) return 50;
    return Math.min(100, Math.max(0, n));
}

/**
 * Direction-aware reward:risk ratio. Uses absolute distances so the
 * sign convention doesn't flip the result. Returns null when the math
 * is degenerate (zero risk or reward) rather than a misleading
 * positive number.
 */
function rr(entryMid: number, target: number, stop: number, isLong: boolean): number | null {
    if (entryMid <= 0 || stop <= 0 || target <= 0) return null;
    const reward = isLong ? target - entryMid : entryMid - target;
    const risk = isLong ? entryMid - stop : stop - entryMid;
    if (reward <= 0 || risk <= 0) return null;
    return Math.round((reward / risk) * 100) / 100;
}

function horizonFactor(horizon: string): { targets: number; stopPct: number } {
    const h = (horizon ?? '').toUpperCase();
    if (h === 'SCALP') return { targets: 1, stopPct: 0.4 };
    if (h === 'INTRADAY') return { targets: 2, stopPct: 1.0 };
    if (h === 'POSITION') return { targets: 3, stopPct: 4.0 };
    return { targets: 3, stopPct: 2.0 }; // SWING default
}

/**
 * Select the per-side L4 zones for the active direction. Returns null when
 * the L4 has no usable zones for that side (e.g. Neutral sentinel where
 * every level pins to close).
 */
function selectSide(
    opp: OpportunityMatrix | null,
    direction: 'LONG' | 'SHORT',
): { entryLow: number; entryHigh: number; entryMid: number; tp1: number; tp2: number; invalidation: number; confTargets: ConfluentLevel[]; confInvals: ConfluentLevel[] } | null {
    if (!opp) return null;
    const entry = direction === 'LONG' ? opp.long_entry_zone : opp.short_entry_zone;
    const target = direction === 'LONG' ? opp.long_target_zone : opp.short_target_zone;
    const inv = direction === 'LONG' ? opp.long_invalidation_level : opp.short_invalidation_level;
    if (!entry || entry.low <= 0 || entry.high <= 0 || inv <= 0) return null;
    const entryMid = (entry.low + entry.high) / 2;
    // TP1 = nearest to entry_mid, TP2 = farther (direction-agnostic ordering).
    const candidates = [target.low, target.high].filter((p) => p > 0);
    if (candidates.length === 0) return null;
    const sorted = [...candidates].sort((a, b) => Math.abs(a - entryMid) - Math.abs(b - entryMid));
    const tp1 = sorted[0];
    const tp2 = sorted.length > 1 ? sorted[1] : tp1;
    return {
        entryLow: entry.low,
        entryHigh: entry.high,
        entryMid,
        tp1,
        tp2,
        invalidation: inv,
        confTargets: opp.confluent_target_levels ?? [],
        confInvals: opp.confluent_invalidation_levels ?? [],
    };
}

// ───────────────────────────── core derivation ─────────────────────────────

export interface DeriveArgs {
    symbol: string;
    markPrice: number;
    opportunity: OpportunityMatrix | null;
    advisory: AdvisoryMatrix | null;
    analysis: AnalysisMatrix | null;
    decisionContext: DecisionContext | null;
    /** Active timeframe telemetry — used for Fibonacci extension targets. */
    tf: TimeframeTelemetry | undefined;
    microTf: TimeframeTelemetry | undefined;
    /** Optional Risk Matrix overall_risk.score for the discount display. */
    overallRisk?: number;
}

export function deriveTradePlan(args: DeriveArgs): TradePlan {
    const { symbol, markPrice, opportunity, advisory, analysis, decisionContext, tf, microTf, overallRisk } = args;

    const ready: TradePlan['readiness'] = decisionContext?.trade_readiness === 'READY' ? 'READY'
        : decisionContext?.trade_readiness === 'FORMING' ? 'FORMING'
        : decisionContext?.trade_readiness === 'WATCH' ? 'WATCH'
        : decisionContext?.trade_readiness === 'STAND_ASIDE' ? 'STAND_ASIDE'
        : 'UNKNOWN';

    const direction: TradePlan['direction'] =
        advisory?.directional_guidance?.includes('Long') ? 'LONG'
        : advisory?.directional_guidance?.includes('Short') ? 'SHORT'
        : 'NEUTRAL';

    const setupType = opportunity?.primary_opportunity ?? 'NoClearOpportunity';
    const setupScore = Math.round(opportunity?.opportunity_score ?? 0);
    const setupQuality = opportunity?.setup_quality ?? '—';
    const horizonRaw = opportunity?.time_horizon ?? 'SWING';
    const timeHorizon = (['SCALP', 'INTRADAY', 'SWING', 'POSITION'].includes(horizonRaw)
        ? horizonRaw
        : 'SWING') as TradePlan['timeHorizon'];
    const horizonCfg = horizonFactor(timeHorizon);

    const confidencePct = Math.round(advisory?.confidence_assessment ?? 0);

    // Plan-level R:R — RR-005 (v6.10.12): the L6 RISK-ADJUSTED decision
    // value via the shared resolver (risk-adjusted first, geometric
    // fallback when the decision value is absent). Per-target R:R below
    // remains per-target economics (same mid-based convention).
    const planRr = resolveActiveRr(opportunity, decisionContext, analysis);
    const rrRatio = Math.round((planRr.riskAdjusted ?? planRr.value) * 100) / 100;

    // ── Select per-side L4 zones (NOT the legacy single-bias mirror) ──
    const isLong = direction === 'LONG';
    const isShort = direction === 'SHORT';
    const side = (isLong || isShort) ? selectSide(opportunity ?? null, direction) : null;
    const entryMid = side ? fmtPrice(side.entryMid) : 0;

    // ── Targets (TP1 nearest, TP2 farther; direction-aware) ──
    const targets: TradeTarget[] = [];
    if (side && horizonCfg.targets >= 1) {
        targets.push({
            label: 'TP1',
            price: fmtPrice(side.tp1),
            sizePct: 40,
            rrRatio: rr(side.entryMid, side.tp1, side.invalidation, isLong),
            source: 'L4_TARGET_ZONE',
        });
    }

    // TP2 / TP3: confluent ladder, direction-aware (LONG = desc, SHORT = asc).
    if (side && horizonCfg.targets >= 2 && targets.length === 1) {
        const directionKey = isLong ? 'desc' : 'asc';
        const sortedConf = sortedConfluents(side.confTargets, directionKey);
        // Pick the first confluent that lies on the correct side of entry_mid
        // (target above entry for LONG, below for SHORT) and differs from TP1.
        const candidate = sortedConf.find((c) =>
            c.price !== side.tp1 &&
            ((isLong && c.price > side.entryMid) || (isShort && c.price < side.entryMid)),
        );
        if (candidate) {
            targets.push({
                label: 'TP2',
                price: fmtPrice(candidate.price),
                sizePct: 40,
                rrRatio: rr(side.entryMid, candidate.price, side.invalidation, isLong),
                source: 'CONFLUENT',
                confluenceCount: candidate.confluence_count,
            });
        }
    }

    if (side && horizonCfg.targets >= 3 && targets.length === 2) {
        const directionKey = isLong ? 'desc' : 'asc';
        const sortedConf = sortedConfluents(side.confTargets, directionKey);
        const alreadyUsed = new Set([side.tp1, targets[1]?.price].filter(Boolean));
        const candidate = sortedConf.find((c) =>
            !alreadyUsed.has(c.price) &&
            ((isLong && c.price > side.entryMid) || (isShort && c.price < side.entryMid)),
        );
        if (candidate) {
            targets.push({
                label: 'TP3',
                price: fmtPrice(candidate.price),
                sizePct: 20,
                rrRatio: rr(side.entryMid, candidate.price, side.invalidation, isLong),
                source: 'CONFLUENT',
                confluenceCount: candidate.confluence_count,
            });
        }
    }

    // Backfill sizes to sum to 100 if targets overlap (sanity)
    const totalSizes = targets.reduce((s, t) => s + t.sizePct, 0);
    if (totalSizes > 0 && totalSizes !== 100 && targets.length > 0) {
        const last = targets[targets.length - 1];
        last.sizePct += 100 - totalSizes;
    }

    // ── Stop (direction-aware check) ──
    let stop: TradeStop | null = null;
    if (side) {
        // LONG: invalidation < entry_mid. SHORT: invalidation > entry_mid.
        const validStop = isLong
            ? side.invalidation < side.entryMid
            : side.invalidation > side.entryMid;
        if (validStop) {
            const distancePct = Math.abs((side.entryMid - side.invalidation) / side.entryMid) * 100;
            const method = (advisory?.protection_strategy?.includes('Structure')
                ? 'STRUCTURE_BASED'
                : advisory?.protection_strategy?.includes('Volatility')
                    ? 'VOLATILITY_BASED'
                    : advisory?.protection_strategy?.includes('ATR')
                        ? 'ATR_BASED'
                        : advisory?.protection_strategy?.includes('SR')
                            ? 'SR_BASED'
                            : 'NONE') as TradeStop['method'];

            // Confluent inval fallback: also direction-filtered.
            const sortedInvalConf = sortedConfluents(
                side.confInvals,
                isLong ? 'asc' : 'desc',
            );
            const fallbackCandidate = sortedInvalConf.find((c) =>
                isLong ? c.price < side.entryMid : c.price > side.entryMid,
            );
            const fallback = fallbackCandidate?.price ?? 0;

            // `stop_loss_distance_pct` is percent-scale on the wire
            // (2.5 = 2.5%, advisory.rs [0.5, 15.0]) — no ×100: the old
            // fraction-scale multiplication produced a LONG fallback stop
            // of `entryMid × (1 − 2.5)` = a negative price.
            const stopPctAdvisory = (advisory as any)?.stop_loss_distance_pct != null
                ? ((advisory as any).stop_loss_distance_pct as number)
                : null;
            const fallbackPrice = stopPctAdvisory != null && side.entryMid > 0
                ? side.entryMid * (1 - (isLong ? stopPctAdvisory / 100 : -stopPctAdvisory / 100))
                : (fallback > 0 ? fallback : undefined);

            stop = {
                price: fmtPrice(side.invalidation),
                distancePct: Math.round(distancePct * 100) / 100,
                method,
                fallbackPrice: fallbackPrice ? fmtPrice(fallbackPrice) : undefined,
                source: 'L4_INVALIDATION',
                evidenceNote: opportunity?.invalidation_note || undefined,
            };
        }
    }

    // ── Entry sources (confluent) ───────────────────────
    const entrySources: TradePlan['entrySources'] = (opportunity?.confluent_entry_levels ?? [])
        .slice(0, 4)
        .map((l) => ({
            tag: tagFromSources(l.sources),
            price: fmtPrice(l.price),
            strength: l.strength,
        }));

    // ── Actionability ──────────────────────────────────
    const tfConsidered = analysis?.timeframes_considered ?? 0;
    const actionable =
        side != null &&
        tfConsidered >= 1 &&
        setupScore >= 30 &&
        ready !== 'STAND_ASIDE' &&
        stop != null &&
        targets.length > 0;
    let actionabilityReason = '';
    if (!actionable) {
        if (!opportunity) actionabilityReason = 'Awaiting L4 opportunity matrix';
        else if (tfConsidered < 1) actionabilityReason = 'Awaiting cross-TF consensus';
        else if (setupScore < 30) actionabilityReason = `Setup score ${setupScore} below threshold`;
        else if (ready === 'STAND_ASIDE') actionabilityReason = 'Trade readiness: STAND ASIDE';
        else if (direction === 'NEUTRAL' || setupType === 'NoClearOpportunity')
            actionabilityReason = 'No directional bias (HOLD / No Clear Setup)';
        else if (!side) actionabilityReason = 'L4 zones not direction-consistent';
        else if (stop == null) actionabilityReason = 'No invalidation level on the correct side of entry';
        else if (targets.length === 0) actionabilityReason = 'No target levels on the correct side of entry';
        else actionabilityReason = 'Awaiting entry zone';
    } else {
        actionabilityReason = 'Actionable setup';
    }

    return {
        symbol,
        direction,
        setupType,
        setupScore,
        setupQuality,
        timeHorizon,
        readiness: ready,
        confidencePct,
        rrRatio,
        riskDiscount: Math.round(clampPct(overallRisk ?? 0)),
        entryMid,
        entryZone: {
            low:  side?.entryLow  ?? 0,
            high: side?.entryHigh ?? 0,
        },
        entrySources,
        targets,
        stop,
        entryGuidance: advisory?.entry_guidance ?? '—',
        exitGuidance: advisory?.exit_guidance ?? '—',
        targetStrategy: advisory?.target_strategy ?? '—',
        protectionStrategy: advisory?.protection_strategy ?? '—',
        contributors: decisionContext?.contributing_indicators ?? [],
        actionable,
        actionabilityReason,
    };
}

// ───────────────────────────── helpers ─────────────────────────────

function sortedConfluents(
    arr: ConfluentLevel[] | undefined | null,
    dir: 'asc' | 'desc',
): ConfluentLevel[] {
    if (!arr) return [];
    return [...arr].sort((a, b) => dir === 'desc' ? b.price - a.price : a.price - b.price);
}

function tagFromSources(sources: string[]): SourceTag {
    for (const s of sources) {
        if (s === 'FIBONACCI') return 'FIB';
        if (s === 'VOLUME_PROFILE') return 'VP';
        if (s === 'PIVOT_POINTS') return 'PP';
        if (s === 'SUPPORT_RESISTANCE') return 'SR';
        if (s === 'LIQUIDITY_CLUSTER') return 'LIQ';
        if (s === 'ATR_FALLBACK') return 'ATR';
    }
    return 'NONE';
}

function fibValues(tf: TimeframeTelemetry | undefined): { ext1618: number | null; ext2618: number | null } {
    if (!tf) return { ext1618: null, ext2618: null };
    const v = tf.indicators?.['fibonacci']?.values as Record<string, number | undefined> | undefined;
    if (!v) return { ext1618: null, ext2618: null };
    return {
        ext1618: typeof v['ext_1618'] === 'number' ? v['ext_1618'] : null,
        ext2618: typeof v['ext_2618'] === 'number' ? v['ext_2618'] : null,
    };
}

// ───────────────────────────── console wiring ─────────────────────────────

export interface ConsoleBracket {
    label: 'TP1' | 'TP2' | 'TP3' | 'SL';
    price: number;
    sizePct: number;
}

export function planToConsoleBrackets(plan: TradePlan): ConsoleBracket[] {
    const out: ConsoleBracket[] = [];
    for (const t of plan.targets) {
        out.push({ label: t.label as 'TP1' | 'TP2' | 'TP3', price: t.price, sizePct: t.sizePct });
    }
    if (plan.stop) {
        out.push({ label: 'SL', price: plan.stop.price, sizePct: plan.stop.distancePct });
    }
    return out;
}
