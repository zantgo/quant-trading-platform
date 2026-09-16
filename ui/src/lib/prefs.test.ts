// @vitest-environment jsdom
// Phase 3: localStorage-backed preference layer (`lib/prefs.ts`).
// Round-trip, corrupted-JSON fallback, missing-key fallback, and
// storage-availability guards (SSR / privacy mode).
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { loadPref, savePref } from './prefs';

describe('prefs — loadPref / savePref', () => {
    beforeEach(() => {
        localStorage.clear();
    });

    it('round-trips a saved value', () => {
        savePref('chartOverlays.BTC-USDT', { showVwap: true, showBb: false });
        expect(loadPref('chartOverlays.BTC-USDT', {})).toEqual({ showVwap: true, showBb: false });
    });

    it('round-trips scalars, arrays, and nested objects', () => {
        savePref('consoleOpen', true);
        savePref('consoleTab', 'orders');
        savePref('list', [1, 'two', { three: 3 }]);
        expect(loadPref('consoleOpen', false)).toBe(true);
        expect(loadPref('consoleTab', 'positions')).toBe('orders');
        expect(loadPref('list', [])).toEqual([1, 'two', { three: 3 }]);
    });

    it('namespaces keys under qtp.', () => {
        savePref('sidebarOpen', true);
        expect(localStorage.getItem('qtp.sidebarOpen')).toBe('true');
    });

    it('returns the fallback for a missing key', () => {
        expect(loadPref('never.saved', 'fallback')).toBe('fallback');
    });

    it('returns the fallback for corrupted JSON', () => {
        localStorage.setItem('qtp.broken', '{not valid json');
        expect(loadPref('broken', { ok: true })).toEqual({ ok: true });
    });

    it('returns the fallback when the value was stored raw (non-JSON)', () => {
        localStorage.setItem('qtp.raw', 'plain-string-not-json');
        // JSON.parse('plain-string-not-json') throws → fallback.
        expect(loadPref('raw', 123)).toBe(123);
    });

    it('is safe when localStorage is unavailable (SSR / privacy mode)', () => {
        vi.stubGlobal('localStorage', undefined);
        expect(loadPref('anything', 'dflt')).toBe('dflt');
        expect(() => savePref('anything', { a: 1 })).not.toThrow();
        vi.unstubAllGlobals();
    });
});
