// Watchlist Scanner — pure helpers used by `WatchlistScannerModal.svelte`.
//
// These functions are intentionally free of any Svelte/fetch/DOM
// dependencies so they can be unit-tested at the lib boundary and so the
// modal component stays a thin orchestration layer over the existing
// `createInstance` / `deleteInstanceById` / `waitForAdvisory` helpers.
//
// The scanner is wired up against the platform's Decision Matrix:
//
//   - `decisionContext.trade_readiness` ∈ { READY, FORMING, WATCH,
//     STAND_ASIDE } (trade_readiness lives on the Decision Context, not
//     the Advisory Matrix — corrected 2026-08-17)
//   - `advisory.directional_guidance` ∈ { StrongLong, Long, Neutral,
//                                          Short, StrongShort,
//                                          AvoidDirectionalExposure }
//
// The "clear decision" filter keeps only pairs where trade_readiness is
// READY AND the directional guidance is a real bias (anything other than
// Neutral / AvoidDirectionalExposure). All other pairs are DELETE-removed
// from the workspace after evaluation.

import type { AdvisoryMatrix, DecisionContext, DirectionalGuidance } from '../types';

export const MAX_SYMBOL_LEN = 10;

/// Wait-window bounds for the scanner's recommendation grace period — a
/// pair is kept when a recommendation to any side appears within the
/// window, deleted otherwise (default 5 minutes, 1–60).
export const WAIT_WINDOW_MIN = 1;
export const WAIT_WINDOW_MAX = 60;
export const WAIT_WINDOW_DEFAULT = 5;

export type DecisionVerdict = 'KEEP' | 'DELETE';

export type PairOutcomeReason =
    | 'KEEP'
    | 'NO_DECISION'
    | 'NOT_READY'
    | 'DIRECTION_NEUTRAL'
    | 'AVOID_DIRECTIONAL'
    | 'TIMEOUT'
    | 'UNAVAILABLE'
    | 'DUPLICATE'
    | 'INVALID'
    | 'NETWORK_ERROR';

export interface PairOutcome {
    base: string;
    pairKey: string;
    status: 'pending' | 'adding' | 'waiting' | 'evaluating' | 'done';
    /** Set when status === 'done'. */
    reason?: PairOutcomeReason;
    /** Final directional guidance observed (when KEEP). */
    guidance?: DirectionalGuidance;
    /** Final trade_readiness observed (when KEEP). */
    tradeReadiness?: string;
    /** Raw error message from the backend (when UNAVAILABLE / NETWORK_ERROR). */
    error?: string;
    /** Milliseconds elapsed from `add` start to done. */
    elapsedMs?: number;
    /** Wall-clock ms when the pair's add/wait lifecycle started (running UI). */
    startedMs?: number;
}

/** Clamp a wait-window value (minutes) into the [1, 60] range; the
 *  default applies for empty / non-finite input. The scanner's running
 *  phase always uses the clamped value. */
export function clampWaitMinutes(value: number | null | undefined): number {
    if (value == null || !Number.isFinite(value)) return WAIT_WINDOW_DEFAULT;
    const v = Math.round(value);
    return Math.min(WAIT_WINDOW_MAX, Math.max(WAIT_WINDOW_MIN, v));
}

/** Parse a free-text watchlist string into a deduped, ordered, validated
 *  list of base symbols. Accepts comma-, whitespace-, and `#`-tag-separated
 *  tokens (e.g. "BTC ETH, #SOL, AVAX"). Empty tokens and tokens longer than
 *  `MAX_SYMBOL_LEN` are dropped silently. */
export function parseSymbols(text: string): string[] {
    // v11.8: spaces are the ONLY separator — commas and '#' prefixes are
    // no longer accepted (they now produce literal characters that fail
    // symbol validation downstream).
    if (!text) return [];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const raw of text.split(/\s+/g)) {
        const tok = raw.trim().toUpperCase();
        if (!tok) continue;
        if (tok.length > MAX_SYMBOL_LEN) continue;
        if (seen.has(tok)) continue;
        seen.add(tok);
        out.push(tok);
    }
    return out;
}

/** Apply the strict "clear decision" rule: keep only when the platform
 *  L6 `decisionContext` says `trade_readiness === 'READY'` AND the L4.75
 *  `advisory` matrix carries a real directional bias (Long / Short of
 *  any strength; Neutral and AvoidDirectionalExposure are excluded).
 *
 *  The two inputs are sourced from different WS frames: `decisionContext`
 *  rides the macro snapshot and `advisory` is the most recent rendering
 *  from the decision-support pipeline. Both are required: a READY gate
 *  without a directional bias is "form a position but don't take it" and
 *  a directional bias without READY is "we see a setup but the confluence
 *  isn't there yet" — neither is a "clear decision" in the scanner's
 *  sense. */
export function decide(
    decisionContext: DecisionContext | null | undefined,
    advisory: AdvisoryMatrix | null | undefined,
): DecisionVerdict {
    if (!decisionContext || typeof decisionContext !== 'object') return 'DELETE';
    if (decisionContext.trade_readiness !== 'READY') return 'DELETE';
    if (!advisory || typeof advisory !== 'object') return 'DELETE';
    const directionalBiases: DirectionalGuidance[] = [
        'StrongLong', 'Long', 'Short', 'StrongShort',
    ];
    if (!directionalBiases.includes(advisory.directional_guidance)) return 'DELETE';
    return 'KEEP';
}

/** Map a `DecisionVerdict` to the user-facing reason string we surface in
 *  the summary card. `KEEP` is its own category; for `DELETE` we distinguish
 *  between the three near-miss reasons so the summary tells the user *why*
 *  a pair was removed. The decisionContext source is checked first so
 *  missing-decision reads as `NO_DECISION` and a STAND_ASIDE readiness
 *  reads as `NOT_READY` regardless of the advisory direction. */
export function reasonFor(
    verdict: DecisionVerdict,
    decisionContext: DecisionContext | null | undefined,
    advisory: AdvisoryMatrix | null | undefined,
): PairOutcomeReason {
    if (verdict === 'KEEP') return 'KEEP';
    if (!decisionContext || typeof decisionContext !== 'object') return 'NO_DECISION';
    if (decisionContext.trade_readiness !== 'READY') return 'NOT_READY';
    if (!advisory || typeof advisory !== 'object') return 'NO_DECISION';
    if (advisory.directional_guidance === 'Neutral') return 'DIRECTION_NEUTRAL';
    if (advisory.directional_guidance === 'AvoidDirectionalExposure') return 'AVOID_DIRECTIONAL';
    return 'NO_DECISION';
}

/** Aggregate a list of `PairOutcome`s into the summary card shown in the
 *  modal's done phase. The kept/removed pair arrays preserve the input
 *  order so the user can scan them in the order they entered the symbols. */
export interface ScanSummary {
    added: number;
    kept: PairOutcome[];
    removed: PairOutcome[];
    skipped: PairOutcome[];
    totalMs: number;
}

export function summarize(results: PairOutcome[]): ScanSummary {
    const kept: PairOutcome[] = [];
    const removed: PairOutcome[] = [];
    const skipped: PairOutcome[] = [];
    let totalMs = 0;
    for (const r of results) {
        if (r.elapsedMs) totalMs += r.elapsedMs;
        if (r.reason === 'KEEP') kept.push(r);
        else if (
            r.reason === 'DUPLICATE'
            || r.reason === 'INVALID'
            || r.reason === 'UNAVAILABLE'
        ) skipped.push(r);
        else removed.push(r);
    }
    return {
        added: kept.length + removed.length,
        kept,
        removed,
        skipped,
        totalMs,
    };
}

/** Human-friendly label for the per-pair outcome/reason chip. */
export function reasonLabel(reason: PairOutcomeReason | undefined): string {
    switch (reason) {
        case 'KEEP': return 'Kept';
        case 'NO_DECISION': return 'No decision';
        case 'NOT_READY': return 'Not ready';
        case 'DIRECTION_NEUTRAL': return 'Neutral bias';
        case 'AVOID_DIRECTIONAL': return 'Avoid direction';
        case 'TIMEOUT': return 'Timeout';
        case 'UNAVAILABLE': return 'Unavailable';
        case 'DUPLICATE': return 'Already in workspace';
        case 'INVALID': return 'Invalid';
        case 'NETWORK_ERROR': return 'Network error';
        default: return 'Pending';
    }
}

/** Heuristic: detect the few backend error variants the modal cares about.
 *  Anything we don't recognize bubbles up as `NETWORK_ERROR` so the summary
 *  still surfaces it. The backend emits specific phrasings for these branches
 *  (see `crates/portfolio-supervisor/src/registry/mod.rs`); we match on those
 *  substrings rather than HTTP status because the error message is the
 *  reliable signal surfaced by `createInstance`. */
export function detectBackendErrorKind(message: string | undefined): PairOutcomeReason {
    if (!message) return 'NETWORK_ERROR';
    const m = message.toLowerCase();
    if (m.includes('already exists')) return 'DUPLICATE';
    if (m.includes("isn't available") || m.includes("isn't available on") || m.includes("couldn't verify")) return 'UNAVAILABLE';
    if (m.includes('no active session')) return 'UNAVAILABLE';
    if (m.includes('symbol')) return 'UNAVAILABLE';
    return 'NETWORK_ERROR';
}
