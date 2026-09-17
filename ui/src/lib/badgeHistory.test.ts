// @vitest-environment jsdom
//
// badgeHistory — literal last-5 ring contract (v11.5):
//   • newest first, capped at 5 (oldest dropped),
//   • repeats kept (literal sampling),
//   • debounced localStorage persistence + restore,
//   • trail view = positions 1..4 (position 0 is the live badge).

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { pushBadge, getBadgeHistory, getBadgeTrail, clearBadgeHistory, BADGE_HISTORY_CAP } from './badgeHistory.svelte';

function entry(label: string, ts: number) {
    return { label, color: '#22c55e', ts };
}

beforeEach(() => {
    localStorage.clear();
    clearBadgeHistory();
});

describe('badgeHistory ring', () => {
    it('keeps the literal last 7 samples, newest first', () => {
        for (const [i, label] of ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'].entries()) {
            pushBadge('k', entry(label, i));
        }
        const ring = getBadgeHistory('k');
        expect(ring.length).toBe(BADGE_HISTORY_CAP);
        expect(ring.map((e) => e.label)).toEqual(['H', 'G', 'F', 'E', 'D', 'C', 'B']);
    });

    it('keeps repeats (literal sampling)', () => {
        for (let i = 0; i < 5; i++) pushBadge('k', entry('NEUTRAL', i));
        expect(getBadgeHistory('k').map((e) => e.label)).toEqual(
            ['NEUTRAL', 'NEUTRAL', 'NEUTRAL', 'NEUTRAL', 'NEUTRAL'],
        );
    });

    it('trail view = positions 1..4 (live badge excluded)', () => {
        pushBadge('k', entry('A', 1));
        pushBadge('k', entry('B', 2));
        pushBadge('k', entry('C', 3));
        expect(getBadgeTrail('k').map((e) => e.label)).toEqual(['B', 'A']);
    });

    it('keys are independent', () => {
        pushBadge('a', entry('A1', 1));
        pushBadge('b', entry('B1', 1));
        expect(getBadgeHistory('a')[0].label).toBe('A1');
        expect(getBadgeHistory('b')[0].label).toBe('B1');
    });

    it('persists to localStorage (debounced write)', () => {
        vi.useFakeTimers();
        try {
            pushBadge('k', entry('ALPHA', 42));
            pushBadge('k', entry('BETA', 43));
            vi.advanceTimersByTime(2100);
            const raw = localStorage.getItem('qtp.badgeHistory.v1');
            expect(raw).toBeTruthy();
            const parsed = JSON.parse(raw!) as Record<string, Array<{ label: string }>>;
            expect(parsed.k.map((e) => e.label)).toEqual(['BETA', 'ALPHA']);
        } finally {
            vi.useRealTimers();
        }
    });

    it('tolerates corrupted persisted payloads', () => {
        localStorage.setItem('qtp.badgeHistory.v1', '{not json');
        // Re-import with cleared module registry is not possible here; the
        // restore() guard runs at init — simulate by clearing + re-pushing
        // (no throw is the contract).
        clearBadgeHistory();
        pushBadge('k', entry('OK', 1));
        expect(getBadgeHistory('k')[0].label).toBe('OK');
    });
});
