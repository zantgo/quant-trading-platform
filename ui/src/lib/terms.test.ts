// activeSlotKinds — the ACTIVE-ladder accessor for the fixed 10-slot pool.
//
// v11.2: only the fastest N slots (`[workspace].active_timeframes`, 1..=10,
// default 5) actually run — the backend publishes `active_secs` on
// `GET /api/instances` and the store mirrors it as `InstanceState.activeSlots`.
// Inactive slots are INERT (no snapshots, no WS sockets), so every UI walk
// must iterate `activeSlotKinds(pair)` instead of the full 10-slot pool.
import { describe, expect, it } from 'vitest';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_DURATION_SECS } from '../types';
import type { InstanceState, TimeframeSlotKind } from '../types';
import { activeSlotKinds, slotsFromSecs, withActiveSlots } from './terms';
import { makeTerms } from '../tests/makeTerms';

function pairWith(activeSlots?: TimeframeSlotKind[]): InstanceState {
    return {
        symbol: 'BTC',
        exchange: 'Hyperliquid',
        isConnected: false,
        ...(activeSlots !== undefined ? { activeSlots } : {}),
        terms: makeTerms(),
    } as unknown as InstanceState;
}

describe('activeSlotKinds', () => {
    it('returns all 10 slots in ladder order when activeSlots is absent (pre-payload default)', () => {
        expect(activeSlotKinds(null)).toEqual([...TIMEFRAME_SLOT_KINDS]);
        expect(activeSlotKinds(undefined)).toEqual([...TIMEFRAME_SLOT_KINDS]);
        expect(activeSlotKinds(pairWith(undefined))).toEqual([...TIMEFRAME_SLOT_KINDS]);
    });

    it('returns only the active slots, re-ordered to the canonical ladder (fastest → slowest)', () => {
        const pair = pairWith(['longterm2', 'micro1']);
        expect(activeSlotKinds(pair)).toEqual(['micro1', 'longterm2']);
    });

    it('ignores unknown slots and empty arrays fall back to the full ladder', () => {
        expect(activeSlotKinds(pairWith([]))).toEqual([...TIMEFRAME_SLOT_KINDS]);
        expect(activeSlotKinds(pairWith(['bogus' as TimeframeSlotKind, 'fast1']))).toEqual(['fast1']);
    });
});

describe('slotsFromSecs', () => {
    it('maps active_secs durations back to slot kinds, preserving the ladder order of the pool', () => {
        expect(slotsFromSecs([1, 3, 5, 15, 30])).toEqual(
            TIMEFRAME_SLOT_KINDS.slice(0, 5),
        );
    });

    it('drops durations that are not in the fixed pool and de-duplicates', () => {
        expect(slotsFromSecs([1, 7, 3600, 1])).toEqual(['micro1', 'longterm2']);
        expect(slotsFromSecs([])).toEqual([]);
    });

    it('is the exact inverse of the TIMEFRAME_SLOT_DURATION_SECS table', () => {
        const all = Object.values(TIMEFRAME_SLOT_DURATION_SECS);
        expect(slotsFromSecs(all)).toEqual([...TIMEFRAME_SLOT_KINDS]);
    });
});

describe('withActiveSlots', () => {
    it('returns the fastest N slots for a count (the Settings-knob derivation)', () => {
        expect(withActiveSlots(3)).toEqual(['micro1', 'micro2', 'fast1']);
        expect(withActiveSlots(1)).toEqual(['micro1']);
        expect(withActiveSlots(10)).toEqual([...TIMEFRAME_SLOT_KINDS]);
    });

    it('clamps out-of-range counts into 1..=10 (mirrors the backend clamp)', () => {
        expect(withActiveSlots(0)).toEqual(['micro1']);
        expect(withActiveSlots(99)).toEqual([...TIMEFRAME_SLOT_KINDS]);
    });
});
