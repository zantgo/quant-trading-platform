// Per-duration telemetry accessor for the duration-keyed ladder (v11.9).
//
// Every `InstanceState` carries `terms: Record<number, TimeframeTelemetry>`
// keyed by the duration in seconds (a member of `DURATIONS`). Chart
// components bind one duration each via `slot: number` and resolve their
// telemetry through this helper — the duration IS the slot identity and
// the label is derived via `tfLabel(secs)`.
import type { InstanceState, TimeframeTelemetry } from '../types';
import { DURATIONS } from '../types';

export function getTerm(
    pair: InstanceState | null | undefined,
    slotSecs: number,
): TimeframeTelemetry | undefined {
    return pair?.terms?.[slotSecs];
}

/**
 * v11.12.20: the timeframe that owns a pair's SHARED overlay-flag state —
 * the FASTEST ACTIVE duration (`terms[1]` only when 1s actually runs; the
 * first active duration otherwise). The overlay pills sync their flags
 * across every active duration, so reading/writing a hardcoded 1s slot
 * silently froze the pills on ladders without 1s. Mirrors `AppStore.micro()`.
 */
export function overlayRepTerm(
    pair: InstanceState | null | undefined,
): TimeframeTelemetry | undefined {
    if (!pair) return undefined;
    const fastest = pair.activeDurations?.[0] ?? 1;
    return pair.terms[fastest] ?? pair.terms[1];
}

/**
 * The ACTIVE durations of an instance's ladder, in canonical order
 * (fastest → slowest). v11.9: the active set is an arbitrary subset of
 * the 14-duration pool (`[workspace].timeframes`); the backend publishes
 * `active_secs` on `GET /api/instances` and the store mirrors it as
 * `InstanceState.activeDurations`. Inactive durations are INERT — they
 * never emit snapshots and their WS sockets are never served — so every
 * "walk the ladder" loop must iterate this helper instead of `DURATIONS`.
 * Falls back to the full 14-duration pool until the payload arrives (or
 * on malformed/empty data).
 */
export function activeDurations(
    pair: InstanceState | null | undefined,
): number[] {
    const active = pair?.activeDurations;
    if (!Array.isArray(active) || active.length === 0) return [...DURATIONS];
    const set = new Set(active);
    return DURATIONS.filter((secs) => set.has(secs));
}

/**
 * Heal a wire `active_secs` list: keep supported durations only,
 * duplicates collapse, and the result is re-ordered to the canonical
 * ascending (fastest → slowest) order regardless of the wire order.
 */
export function durationsFromSecs(secs: readonly number[]): number[] {
    const wanted = new Set(secs);
    return DURATIONS.filter((d) => wanted.has(d));
}

/**
 * The fastest N durations of the supported pool — the derivation behind
 * count-style knobs. Counts are clamped into 1..=14 exactly like the
 * backend.
 */
export function withActiveDurations(count: number): number[] {
    const n = Math.min(Math.max(Math.trunc(count) || 1, 1), DURATIONS.length);
    return DURATIONS.slice(0, n);
}
