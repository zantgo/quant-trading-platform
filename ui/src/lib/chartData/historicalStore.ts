// HistoricalStore — durable, immutable history for >=60s
// Senior FE third-structure: separates durability from recency.
// This store is SINGLE-WRITER, immutable snapshot: fetch once, read many, never mutated by live ingestion.
// Key: `${pair}@${slot}@${timeframe}` slot-aware, no duration fallback.
// Used ONLY for timeframe >= 60 (PRI-08). Sub-minute (<60) never touches this store.

import { tfLabel } from '../../types';
import type { IndicatorFlatHistory } from '../indicatorHistory';
import { normalizeHistoryForStore, type RawResponse } from './reconciledView';

const HISTORY_URL = '/api/history';

/// Immutable historical cache: promise single-flight + resolved data
const historicalCache = new Map<string, Promise<IndicatorFlatHistory | null>>();
const historicalData = new Map<string, IndicatorFlatHistory>();

function keyFor(pairKey: string, timeframe: number, slot?: number | string): string {
    return `${pairKey}@${slot ?? '?'}@${timeframe}`;
}

export function fetchHistorical(
    pairKey: string,
    timeframe: number,
    slot?: number | string,
): Promise<IndicatorFlatHistory | null> {
    if (!pairKey || !timeframe) return Promise.resolve(null);
    // HistoricalStore only for >=60; <60 should use liveRing
    if (timeframe < 60) return Promise.resolve(null);
    const key = keyFor(pairKey, timeframe, slot);
    const cached = historicalCache.get(key);
    if (cached) return cached;

    const promise = (async (): Promise<IndicatorFlatHistory | null> => {
        try {
            const slotParam = slot != null ? `&slot=${encodeURIComponent(typeof slot === 'number' ? tfLabel(slot) : slot)}` : '';
            const res = await fetch(
                `${HISTORY_URL}?symbol=${encodeURIComponent(pairKey)}&timeframe_secs=${timeframe}&limit=1000${slotParam}`,
            );
            if (!res.ok) return null;
            const raw = (await res.json()) as RawResponse;
            const hist = normalizeHistoryForStore(raw);
            if (hist) {
                // Store immutable snapshot — no live mutation, no merge
                historicalData.set(key, hist);
            }
            return hist;
        } catch (err) {
            console.error('historicalStore fetch failed', err);
            return null;
        }
    })();

    historicalCache.set(key, promise);
    // Populate historicalData when resolved (if not already set)
    promise.then((h) => {
        if (!h) return;
        const cur = historicalData.get(key);
        if (!cur || cur !== h) {
            // Only set if not already overwritten by a newer fetch (force)
            if (!historicalData.has(key)) historicalData.set(key, h);
        }
    });
    return promise;
}

export function getHistorical(pairKey: string, timeframe: number, slot?: number | string): IndicatorFlatHistory | null {
    return historicalData.get(keyFor(pairKey, timeframe, slot)) ?? null;
}

export function hasHistorical(pairKey: string, timeframe: number, slot?: string): boolean {
    return historicalData.has(keyFor(pairKey, timeframe, slot));
}

export function purgeHistorical(pairKey: string, timeframe: number, slot?: number | string): void {
    const k = keyFor(pairKey, timeframe, slot);
    historicalCache.delete(k);
    historicalData.delete(k);
}

export function clearHistorical(): void {
    historicalCache.clear();
    historicalData.clear();
}

// For testing / facade
export function _getCache(): Map<string, Promise<IndicatorFlatHistory | null>> {
    return historicalCache;
}
export function _getData(): Map<string, IndicatorFlatHistory> {
    return historicalData;
}
