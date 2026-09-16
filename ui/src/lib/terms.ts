// Per-slot telemetry accessor for the fixed 10-slot ladder.
//
// Every `InstanceState` carries `terms: Record<TimeframeSlotKind, TimeframeTelemetry>`
// (`micro1`..`longterm2`). Chart components bind one column each via
// `slot: TimeframeSlotKind` and resolve their telemetry through this helper —
// never by duration and never via 4-way ternary chains.
import type { InstanceState, TimeframeSlotKind, TimeframeTelemetry } from '../types';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_DURATION_SECS } from '../types';

export function getTerm(
    pair: InstanceState | null | undefined,
    slot: TimeframeSlotKind,
): TimeframeTelemetry | undefined {
    return pair?.terms?.[slot];
}

/**
 * The ACTIVE slots of an instance's ladder, in canonical order
 * (fastest → slowest). v11.2: only the fastest N slots of the fixed
 * 10-slot pool run (`[workspace].active_timeframes`, 1..=10, default 5);
 * the backend publishes `active_secs` on `GET /api/instances` and the
 * store mirrors it as `InstanceState.activeSlots`. Slots beyond N are
 * INERT — they never emit snapshots and their WS sockets are never
 * served — so every "walk the ladder" loop must iterate this helper
 * instead of `TIMEFRAME_SLOT_KINDS`. Falls back to the full 10-slot
 * ladder until the payload arrives (or on malformed/empty data).
 */
export function activeSlotKinds(
    pair: InstanceState | null | undefined,
): TimeframeSlotKind[] {
    const active = pair?.activeSlots;
    if (!Array.isArray(active) || active.length === 0) return [...TIMEFRAME_SLOT_KINDS];
    const set = new Set(active);
    return TIMEFRAME_SLOT_KINDS.filter((slot) => set.has(slot));
}

/**
 * Map wire `active_secs` durations back to slot kinds via the inverted
 * `TIMEFRAME_SLOT_DURATION_SECS` table. Durations outside the fixed pool
 * are dropped and duplicates collapse; the result is re-ordered to the
 * canonical ladder order regardless of the wire order.
 */
export function slotsFromSecs(secs: readonly number[]): TimeframeSlotKind[] {
    const wanted = new Set(secs);
    return TIMEFRAME_SLOT_KINDS.filter((slot) => wanted.has(TIMEFRAME_SLOT_DURATION_SECS[slot]));
}

/**
 * The fastest N slots of the fixed pool — the derivation behind the
 * `[workspace].active_timeframes` Settings knob. Counts are clamped into
 * 1..=10 exactly like the backend (`clamp(1, 10)`).
 */
export function withActiveSlots(count: number): TimeframeSlotKind[] {
    const n = Math.min(Math.max(Math.trunc(count) || 1, 1), TIMEFRAME_SLOT_KINDS.length);
    return TIMEFRAME_SLOT_KINDS.slice(0, n);
}
