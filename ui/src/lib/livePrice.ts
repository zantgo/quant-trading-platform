// Header live-price picker for an `InstanceState`.
//
// Two-stage picker, exported as a pure helper so `ui/src/App.svelte` and
// the regression suite (ui/src/tests/snapshot.transform.test.ts) can
// both exercise the exact same code path.
//
// 1. **Stage 1 — Freshest of all ten slots:** walk every slot's
//    `priceText` and `latestSnapshot.timestamp`, returning the value from
//    the slot whose `timestamp` is most recent AND within 30 seconds of
//    `now`. This keeps the header honest even when one slot is mid-
//    bootstrap while the others are streaming.
// 2. **Stage 2 — Last known good:** if no slot has a fresh frame, return
//    the most recent non-placeholder price regardless of staleness. This
//    prevents the seeded `'--'` placeholder at `state.svelte.ts:26`
//    from regressing into the DOM once any WS frame has ever landed.
// 3. **Stage 3 — Dashes:** only when no slot has ever received a real
//    price, fall back to `'--'`.

import { TIMEFRAME_SLOT_KINDS } from '../types';

/// Loose type for any object that carries a `priceText` and an optional
/// `latestSnapshot`. We deliberately accept a broader shape than
/// `TimeframeTelemetry` so the test fixtures and the production store
/// can both flow through this picker without an explicit downcast.
export interface PricePickLike {
    priceText?: string | null;
    latestSnapshot?: { timestamp?: unknown } | null;
}

export interface PricePickPairLike {
    /// Per-slot telemetry record keyed by the fixed 10-slot ladder
    /// (`micro1`..`longterm2`). Partial so the full production
    /// `Record<TimeframeSlotKind, TimeframeTelemetry>` and loose test
    /// fixtures both flow through.
    terms?: {
        micro1?: PricePickLike | null;
        micro2?: PricePickLike | null;
        fast1?: PricePickLike | null;
        fast2?: PricePickLike | null;
        slow1?: PricePickLike | null;
        slow2?: PricePickLike | null;
        macro1?: PricePickLike | null;
        macro2?: PricePickLike | null;
        longterm1?: PricePickLike | null;
        longterm2?: PricePickLike | null;
    } | null;
}

const STALENESS_WINDOW_MS = 30_000;

function isNumericPrice(value: unknown): value is string {
    if (typeof value !== 'string') return false;
    if (value === '' || value === '--' || value === '0' || value === 'NaN') return false;
    const n = parseFloat(value);
    return Number.isFinite(n) && n > 0;
}

function timestampOf(snap: PricePickLike['latestSnapshot']): number {
    if (!snap) return Number.NEGATIVE_INFINITY;
    const ts = (snap as { timestamp?: unknown }).timestamp;
    const n = typeof ts === 'number' ? ts : NaN;
    return Number.isFinite(n) ? n : Number.NEGATIVE_INFINITY;
}

export function pickInstanceLivePrice(pair: PricePickPairLike, nowMs: number): string {
    const slots: Array<PricePickLike | null | undefined> = TIMEFRAME_SLOT_KINDS.map(
        (slot) => pair.terms?.[slot],
    );

    // Stage 1 — freshest within the staleness window.
    let bestText: string | null = null;
    let bestAge = Infinity;
    for (const tf of slots) {
        const p = tf?.priceText;
        if (!isNumericPrice(p)) continue;
        const ts = timestampOf(tf?.latestSnapshot);
        if (!Number.isFinite(ts)) continue;
        const age = nowMs / 1000 - ts;
        if (age < 0 || age * 1000 >= STALENESS_WINDOW_MS) continue;
        if (age < bestAge) {
            bestAge = age;
            bestText = p;
        }
    }
    if (bestText != null) return bestText;

    // Stage 2 — most recent numeric price regardless of staleness.
    let fallbackText: string | null = null;
    let fallbackTs = Number.NEGATIVE_INFINITY;
    for (const tf of slots) {
        const p = tf?.priceText;
        if (!isNumericPrice(p)) continue;
        const ts = timestampOf(tf?.latestSnapshot);
        if (ts > fallbackTs || fallbackText === null) {
            fallbackTs = ts;
            fallbackText = p;
        }
    }
    if (fallbackText != null) return fallbackText;

    // Stage 3 — every slot is still the seed placeholder.
    return '--';
}

