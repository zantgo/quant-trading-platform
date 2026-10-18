// Watchlist Scanner — pure helpers used by `WatchlistScannerModal.svelte`.
//
// These functions are intentionally free of any Svelte/fetch/DOM
// dependencies so they can be unit-tested at the lib boundary and so the
// modal component stays a thin orchestration layer over the existing
// `createInstance` / `deleteInstanceById` / `waitForAdvisory` helpers.
//
// CR-15: the scanner is now wired against the MODEL, not the Decision Matrix.
//
// It used to keep only pairs where `decisionContext.trade_readiness` was READY
// AND `advisory.directional_guidance` was a real bias, and DELETE every other
// pair from the workspace. Both inputs were L5/L6 strategy: a statement about
// what to do. They no longer cross the wire, and an observe build has no
// business deleting an instance because the market has not yet produced a
// trade call — that is an execution decision, and the charter stops at the
// model.
//
// The scanner's rule is therefore inverted: a pair is kept once the model has
// actually said something about it, and it is NEVER removed for having said
// too little. The wait window still has meaning — it waits for the model to
// resolve — but a timeout is a note, not a deletion.
//
//   - `analysis.bias` ∈ MarketBias — the model's resolved direction (or Neutral)
//   - `alignment.mtf_overall_score` / `mtf_stance` — whether the
//     durations agree, and whether an alignment exists at all
//
// The DELETE outcomes that remain are the operator's own doing: a duplicate
// pair, an invalid ticker, or a venue that does not list it. Those are input
// errors, not judgements about the market.

import type { AlignmentMatrix, AnalysisMatrix, MarketBias } from '../types';

// v11.12.26: the scanner used to drop any token over 10 CHARACTERS silently
// (no error at all), which was the worst failure mode for a non-ASCII ticker —
// it vanished with no feedback. The budget is now shared with the wizard.
import {
    MAX_QUALIFIED_SYMBOL_CHARS,
    MAX_SYMBOL_CHARS,
    invalidTickerMessage,
    isValidBaseSymbol,
    splitDexQualifier,
} from './symbol';
export { MAX_SYMBOL_CHARS as MAX_SYMBOL_LEN };

/// Wait-window bounds for the scanner's recommendation grace period — a
/// pair is kept when a recommendation to any side appears within the
/// window, deleted otherwise (default 5 minutes, 1–60).
export const WAIT_WINDOW_MIN = 1;
export const WAIT_WINDOW_MAX = 60;
export const WAIT_WINDOW_DEFAULT = 5;

// CR-15: `DELETE` is gone from this union. The scanner no longer removes a
// pair for what the model said; only the caller's input-level checks do.
export type DecisionVerdict = 'KEEP' | 'WAIT';

export type PairOutcomeReason =
    | 'KEEP'
    | 'NO_MODEL_STATE'
    | 'NO_CROSS_TF'
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
    /** The model's resolved bias (when KEEP). */
    guidance?: string | null;
    /** The cross-duration agreement label (when KEEP). */
    mtfLabel?: string | null;
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

/** Why a token in a watchlist paste was rejected. */
export interface SymbolRejection {
    token: string;
    reason: string;
}

/** Parse a free-text watchlist string into a deduped, ordered list of base
 *  symbols, plus the tokens that were rejected and why.
 *
 *  The character budget alone was the whole rule here, so every malformed token
 *  (`S&P500`, `BTC-USDC`, `#SOL`) was POSTed to the backend verbatim and came
 *  back as a generic failure. Validation now runs the SHARED ticker rule, and
 *  the reason is surfaced instead of vanishing — the same failure mode the
 *  v11.12.26 fix addressed in the wizard.
 *
 *  An explicit HIP-3 `dex:BASE` qualifier is accepted and kept on the token;
 *  the base half is what the pair key uses. */
export function parseSymbolsWithReasons(text: string): {
    symbols: string[];
    rejected: SymbolRejection[];
} {
    if (!text) return { symbols: [], rejected: [] };
    const seen = new Set<string>();
    const symbols: string[] = [];
    const rejected: SymbolRejection[] = [];
    for (const raw of text.split(/\s+/g)) {
        const tok = raw.trim().toUpperCase();
        if (!tok) continue;
        const qualifier = splitDexQualifier(tok);
        if (qualifier && isValidBaseSymbol(qualifier.base)) {
            const qualified = `${qualifier.dex.toLowerCase()}:${qualifier.base}`;
            if (seen.has(qualified)) continue;
            seen.add(qualified);
            symbols.push(qualified);
            continue;
        }
        if (!isValidBaseSymbol(tok)) {
            rejected.push({ token: tok, reason: invalidTickerMessage(tok) });
            continue;
        }
        if (seen.has(tok)) continue;
        seen.add(tok);
        symbols.push(tok);
    }
    return { symbols, rejected };
}

/** Parse a free-text watchlist string into a deduped, ordered, validated
 *  list of base symbols. Spaces are the only separator; commas and `#` prefixes
 *  are no longer accepted (they produce literal characters that fail symbol
 *  validation). Invalid tokens are dropped — use
 *  {@link parseSymbolsWithReasons} to surface why. */
export function parseSymbols(text: string): string[] {
    return parseSymbolsWithReasons(text).symbols;
}

/** True when every token in a watchlist paste is usable. */
export function watchlistIsClean(text: string): boolean {
    return parseSymbolsWithReasons(text).rejected.length === 0;
}

/** Has the model resolved a reading for this pair?
 *
 *  CR-15: this replaces a rule that required a READY trade gate AND a real
 *  directional bias, and DELETEd every pair that fell short. Both inputs were
 *  strategy, so the rule now asks only whether the model has spoken: a resolved
 *  bias (Neutral counts — a flat read is a reading) and at least one duration
 *  with a cross-duration alignment other than `NO_DATA`.
 *
 *  Note what this deliberately does NOT do: it never returns a verdict that
 *  removes a pair. Not Neutral, not a missing alignment, not a timeout. */
export function decide(
    analysis: AnalysisMatrix | null | undefined,
    alignment: AlignmentMatrix | null | undefined,
): DecisionVerdict {
    // CR-15: "resolved" means the model has produced a reading for this pair.
    // A pair is kept once that happens, and the verdict is never DELETE on the
    // strength of what the model said — only the caller removes pairs, and only
    // for input-level problems.
    if (!analysis || typeof analysis !== 'object') return 'WAIT';
    if (!hasResolvedBias(analysis.bias)) return 'WAIT';
    if (!alignment || typeof alignment !== 'object') return 'WAIT';
    if (alignment.mtf_stance === 'unmeasured') return 'WAIT';
    return 'KEEP';
}

/** A bias the model has actually resolved.
 *
 *  `Neutral` is a resolution, not an absence — a market the model reads as flat
 *  is still a reading, so it keeps the pair. What would NOT be a resolution is
 *  `undefined` / `null`, which is what a pair that has not yet produced a
 *  completed-candle analysis carries. */
function hasResolvedBias(bias: MarketBias | null | undefined): boolean {
    return bias != null;
}

/** Map a `DecisionVerdict` to the user-facing reason string on the summary
 *  card. These say what the model had NOT yet said when the wait window closed
 *  — they are notes on a pair that was kept, not reasons for a removal. */
export function reasonFor(
    verdict: DecisionVerdict,
    analysis: AnalysisMatrix | null | undefined,
    alignment: AlignmentMatrix | null | undefined,
): PairOutcomeReason {
    if (verdict === 'KEEP') return 'KEEP';
    // CR-15: the four strategy near-misses (no decision / not ready / direction
    // neutral / avoid directional) all said the same thing in different words —
    // "the model has not produced a trade call yet". There is one reason now.
    if (!analysis || typeof analysis !== 'object') return 'NO_MODEL_STATE';
    if (!hasResolvedBias(analysis.bias)) return 'NO_MODEL_STATE';
    if (!alignment || typeof alignment !== 'object') return 'NO_CROSS_TF';
    return 'NO_CROSS_TF';
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
    // CR-15: `removed` is now ALWAYS empty. Every outcome the scanner records
    // after a successful add is a pair that stayed in the workspace; the only
    // pairs that never made it in are the input-level skips (a duplicate pair, an
    // invalid ticker, a venue that does not list it). The field is kept because
    // the summary card renders three groups and an absent group would reflow the
    // card — but nothing populates it, and `added` counts the kept pairs only.
    const kept: PairOutcome[] = [];
    const removed: PairOutcome[] = [];
    const skipped: PairOutcome[] = [];
    let totalMs = 0;
    for (const r of results) {
        if (r.elapsedMs) totalMs += r.elapsedMs;
        if (
            r.reason === 'DUPLICATE'
            || r.reason === 'INVALID'
            || r.reason === 'UNAVAILABLE'
        ) skipped.push(r);
        else kept.push(r);
    }
    return {
        added: kept.length,
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
        case 'NO_MODEL_STATE': return 'Model state pending';
        case 'NO_CROSS_TF': return 'Awaiting cross-duration agreement';
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
