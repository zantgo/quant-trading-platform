// activeDurations — the ACTIVE-ladder accessor for the duration-keyed
// pool (v11.9).
//
// The active set is an arbitrary subset of the 14-duration pool
// (`[workspace].timeframes`); the backend publishes `active_secs` on
// `GET /api/instances` and the store mirrors it as
// `InstanceState.activeDurations`. Inactive durations are INERT (no
// snapshots, no WS sockets), so every UI walk must iterate
// `activeDurations(pair)` instead of the full pool.
import { describe, expect, it } from 'vitest';
import { DURATIONS } from '../types';
import type { InstanceState } from '../types';
import { activeDurations, durationsFromSecs, withActiveDurations } from './terms';
import { makeTerms } from '../tests/makeTerms';

function pairWith(activeDurations?: number[]): InstanceState {
    return {
        symbol: 'BTC',
        exchange: 'Hyperliquid',
        isConnected: false,
        ...(activeDurations !== undefined ? { activeDurations } : {}),
        terms: makeTerms(),
    } as unknown as InstanceState;
}

describe('activeDurations', () => {
    it('returns all 14 durations in pool order when activeDurations is absent (pre-payload default)', () => {
        expect(activeDurations(null)).toEqual([...DURATIONS]);
        expect(activeDurations(undefined)).toEqual([...DURATIONS]);
        expect(activeDurations(pairWith(undefined))).toEqual([...DURATIONS]);
    });

    it('returns only the active durations, re-ordered to the canonical ascending order (fastest → slowest)', () => {
        const pair = pairWith([3600, 1]);
        expect(activeDurations(pair)).toEqual([1, 3600]);
    });

    it('ignores unknown durations and empty arrays fall back to the full pool', () => {
        expect(activeDurations(pairWith([]))).toEqual([...DURATIONS]);
        expect(activeDurations(pairWith([7, 60]))).toEqual([60]);
    });
});

describe('durationsFromSecs', () => {
    it('heals active_secs to the canonical ascending pool order', () => {
        expect(durationsFromSecs([1, 3, 5, 15, 30])).toEqual(
            DURATIONS.slice(0, 5),
        );
    });

    it('drops durations that are not in the supported pool and de-duplicates', () => {
        expect(durationsFromSecs([1, 7, 3600, 1])).toEqual([1, 3600]);
        expect(durationsFromSecs([])).toEqual([]);
    });

    it('is the identity over the full SUPPORTED pool', () => {
        expect(durationsFromSecs([...DURATIONS])).toEqual([...DURATIONS]);
    });
});

describe('withActiveDurations', () => {
    it('returns the fastest N durations for a count (the Settings-knob derivation)', () => {
        expect(withActiveDurations(3)).toEqual([1, 3, 5]);
        expect(withActiveDurations(1)).toEqual([1]);
        expect(withActiveDurations(14)).toEqual([...DURATIONS]);
    });

    it('clamps out-of-range counts into 1..=14 (mirrors the backend clamp)', () => {
        expect(withActiveDurations(0)).toEqual([1]);
        expect(withActiveDurations(99)).toEqual([...DURATIONS]);
    });
});
