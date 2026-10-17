// ReconciledView — pure, stateless derivation of historical + live
// Senior FE third-structure: no Map, no $state, no side-effects. Derives view from two inputs.
// Used by PriceChart to get the correct historical (500) + live tail without mutating either store.

import type { IndicatorFlatHistory } from '../indicatorHistory';

export interface RawHistoryIndicator {
    raw?: Array<number | null>;
    normalized?: Array<number | null>;
    state_label?: Array<string | null>;
    values?: Record<string, Array<number | null>>;
}

export interface RawHistory {
    times?: number[];
    indicators?: Record<string, RawHistoryIndicator>;
}

export interface RawHistoryCandle {
    time: number;
    open: string;
    high: string;
    low: string;
    close: string;
    volume: string;
    reconstructed?: string | null;
}

export interface RawResponse {
    prices?: string[];
    candles?: RawHistoryCandle[];
    indicator_history?: RawHistory | null;
    stateBands?: Record<string, unknown>;
    volume_profiles?: Record<string, unknown>;
}

function toNumberArray(arr: ReadonlyArray<number | null | string | undefined>): Array<number | null> {
    if (!arr) return [];
    return arr.map((v) => {
        if (v == null) return null;
        if (typeof v === 'number') return Number.isFinite(v) ? v : null;
        const n = Number(v);
        return Number.isFinite(n) ? n : null;
    });
}

/// Normalize raw wire response to IndicatorFlatHistory — extracted from indicatorHistory for reuse
export function normalizeHistoryForStore(raw: RawResponse): IndicatorFlatHistory {
    const times = (raw.indicator_history?.times ?? []).map((t) => Number(t));
    const values: Record<string, Array<number | null>> = {};

    const ih = raw.indicator_history?.indicators ?? {};
    for (const [key, dto] of Object.entries(ih)) {
        if (!dto) continue;
        const labels = Array.isArray(dto.state_label) ? dto.state_label : null;
        const masked = (i: number) => labels != null && labels[i] === 'WARMING';
        if (Array.isArray(dto.raw)) {
            values[key] = toNumberArray(dto.raw).map((v, i) => (masked(i) ? null : v));
        }
        if (dto.values) {
            for (const [sub, arr] of Object.entries(dto.values)) {
                if (!Array.isArray(arr)) continue;
                values[`${key}.${sub}`] = toNumberArray(arr).map((v, i) => (masked(i) ? null : v));
            }
        }
    }

    const candleRows = raw.candles ?? [];
    const candleTimes: number[] = [];
    const candleReconstructed: Array<string | undefined> = [];
    const candles = { open: [] as number[], high: [] as number[], low: [] as number[], close: [] as number[], volume: [] as number[] };
    for (const c of candleRows) {
        if (!c || c.time == null) continue;
        candleTimes.push(Math.floor(c.time / 1000));
        candles.open.push(parseFloat(c.open) || 0);
        candles.high.push(parseFloat(c.high) || 0);
        candles.low.push(parseFloat(c.low) || 0);
        candles.close.push(parseFloat(c.close) || 0);
        candles.volume.push(parseFloat(c.volume) || 0);
        candleReconstructed.push(c.reconstructed && typeof c.reconstructed === 'string' ? c.reconstructed : undefined);
    }

    return {
        times,
        values,
        candleTimes,
        candles,
        candleReconstructed,
        prices: raw.prices ?? undefined,
        stateBands: raw.stateBands ?? undefined,
        volumeProfiles: raw.volume_profiles ?? undefined,
        fetchedAtMs: Date.now(),
    };
}

/// Pure reconcile: historical (durable, maybe null) + live (mutable ring, maybe null) → view
/// - If historical is null/empty and live exists → live-only (sub-minute case)
/// - If historical exists and live tail newer than serverLast → historical + tail
/// - If historical exists and no newer tail → historical
/// Caps at 1000, keeps indicator alignment. Does NOT mutate inputs (creates new object when merging).
export function reconcile(
    historical: IndicatorFlatHistory | null,
    live: IndicatorFlatHistory | null,
): IndicatorFlatHistory | null {
    if (!historical && !live) return null;
    if (!historical) {
        // Live-only: clone to avoid mutating live store when caller slices
        return live ? cloneHistory(live) : null;
    }
    if (!live || live.times.length === 0) {
        return cloneHistory(historical);
    }
    // Both exist: check if live has tail newer than historical
    const serverLast = historical.times[historical.times.length - 1] ?? -Infinity;
    const tailIdx = live.times.findIndex((t) => t > serverLast);
    if (tailIdx === -1) {
        // No newer tail — historical is authoritative
        return cloneHistory(historical);
    }
    // Merge: historical + live tail
    const merged: IndicatorFlatHistory = cloneHistory(historical);
    const tailTimes = live.times.slice(tailIdx);
    const tailCandles = live.candleTimes.slice(tailIdx);
    // Merge values
    for (const [k, arr] of Object.entries(live.values)) {
        const serverArr = merged.values[k];
        if (!serverArr) {
            merged.values[k] = Array(merged.times.length).fill(null).concat(arr.slice(tailIdx));
        } else {
            merged.values[k] = serverArr.concat(arr.slice(tailIdx));
        }
    }
    // For keys in historical but not in live tail, need to extend with nulls already handled via tail length?
    // Ensure all historical keys have tail extension if live didn't have them
    // Actually historical keys missing in live tail should get nulls for tail length — but live.values already has nulls for those keys via ingest
    // So we are good.

    merged.times = merged.times.concat(tailTimes);
    merged.candleTimes = merged.candleTimes.concat(tailCandles);
    merged.candles.open = merged.candles.open.concat(live.candles.open.slice(tailIdx));
    merged.candles.high = merged.candles.high.concat(live.candles.high.slice(tailIdx));
    merged.candles.low = merged.candles.low.concat(live.candles.low.slice(tailIdx));
    merged.candles.close = merged.candles.close.concat(live.candles.close.slice(tailIdx));
    merged.candles.volume = merged.candles.volume.concat(live.candles.volume.slice(tailIdx));
    if (merged.candleReconstructed && live.candleReconstructed) {
        merged.candleReconstructed = (merged.candleReconstructed as Array<string | undefined>).concat(
            live.candleReconstructed.slice(tailIdx),
        );
    }
    merged.fetchedAtMs = Date.now();

    // Cap
    if (merged.times.length > 1000) {
        const trim = merged.times.length - 1000;
        merged.times.splice(0, trim);
        merged.candleTimes.splice(0, trim);
        merged.candles.open.splice(0, trim);
        merged.candles.high.splice(0, trim);
        merged.candles.low.splice(0, trim);
        merged.candles.close.splice(0, trim);
        merged.candles.volume.splice(0, trim);
        if (merged.candleReconstructed) (merged.candleReconstructed as Array<string | undefined>).splice(0, trim);
        for (const arr of Object.values(merged.values)) arr.splice(0, trim);
    }

    // Merge stateBands/volumeProfiles from live if historical missing them
    if (!merged.stateBands && live.stateBands) merged.stateBands = live.stateBands;
    if (!merged.volumeProfiles && live.volumeProfiles) merged.volumeProfiles = live.volumeProfiles;

    return merged;
}

function cloneHistory(h: IndicatorFlatHistory): IndicatorFlatHistory {
    return {
        times: [...h.times],
        values: Object.fromEntries(Object.entries(h.values).map(([k, v]) => [k, [...v]])),
        candleTimes: [...h.candleTimes],
        candles: {
            open: [...h.candles.open],
            high: [...h.candles.high],
            low: [...h.candles.low],
            close: [...h.candles.close],
            volume: [...h.candles.volume],
        },
        candleReconstructed: h.candleReconstructed ? [...h.candleReconstructed] : undefined,
        prices: h.prices ? [...h.prices] : undefined,
        stateBands: h.stateBands ? { ...h.stateBands } : undefined,
        volumeProfiles: h.volumeProfiles ? { ...h.volumeProfiles } : undefined,
        fetchedAtMs: h.fetchedAtMs,
    };
}
