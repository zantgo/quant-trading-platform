// LiveRing — append-only, mutable, live data for all TFs, authoritative for <60
// Senior FE third-structure: separates recency from durability.
// This store is the SOLE writer for live ingestion (WS is_completed). Never fetches, never purges on its own
// except per-slot on explicit reset. Caps at HIST_BUFFER_MAX=1000.
// Used for <60 live-only and as tail for >=60 reconciled view.

import type { Time } from 'lightweight-charts';
import type { CandleOHLCV, IndicatorFlatHistory } from '../indicatorHistory';

const HIST_MAX = 1000;

const liveHistory = new Map<string, IndicatorFlatHistory>();
const liveCandles = new Map<string, CandleOHLCV[]>();

function keyFor(pairKey: string, timeframe: number, slot?: number | string): string {
    return `${pairKey}@${slot ?? '?'}@${timeframe}`;
}

export function getLiveHistory(pairKey: string, timeframe: number, slot?: number | string): IndicatorFlatHistory | null {
    return liveHistory.get(keyFor(pairKey, timeframe, slot)) ?? null;
}

export function getLiveCandles(pairKey: string, timeframe: number, slot?: number | string): CandleOHLCV[] | null {
    return liveCandles.get(keyFor(pairKey, timeframe, slot)) ?? null;
}

export function clearLive(): void {
    liveHistory.clear();
    liveCandles.clear();
}

export function purgeLive(pairKey: string, timeframe: number, slot?: number | string): void {
    const k = keyFor(pairKey, timeframe, slot);
    liveHistory.delete(k);
    liveCandles.delete(k);
}

/// Live ingestion — mirrors indicatorHistory.ingestLiveSnapshot but WITHOUT cache priming
/// and WITHOUT timeframe<60 branching in fetch. This is the sole mutable path for live.
export function ingestLive(
    pairKey: string,
    timeframe: number,
    slot: number | string | undefined,
    snapshot: Record<string, unknown>,
): void {
    if (!pairKey || !timeframe) return;
    const isCompleted = snapshot.is_completed === true;
    if (!isCompleted) return;
    const tsRaw = snapshot.timestamp;
    const ts = typeof tsRaw === 'number' ? tsRaw : Number(tsRaw ?? 0);
    if (!Number.isFinite(ts) || ts <= 0) return;
    const key = keyFor(pairKey, timeframe, slot);
    let hist = liveHistory.get(key);
    if (!hist) {
        hist = {
            times: [],
            values: {},
            candleTimes: [],
            candles: { open: [], high: [], low: [], close: [], volume: [] },
            candleReconstructed: [],
            fetchedAtMs: Date.now(),
        };
        liveHistory.set(key, hist);
    }
    if (hist.times.length > 0 && hist.times[hist.times.length - 1] === ts) return;

    hist.times.push(ts);

    const open = parseFloat(String((snapshot as Record<string, unknown>).open ?? snapshot.close ?? '0')) || 0;
    const high = parseFloat(String((snapshot as Record<string, unknown>).high ?? snapshot.close ?? '0')) || 0;
    const low = parseFloat(String((snapshot as Record<string, unknown>).low ?? snapshot.close ?? '0')) || 0;
    const close = parseFloat(String((snapshot as Record<string, unknown>).close ?? '0')) || 0;
    const vol = parseFloat(String((snapshot as Record<string, unknown>).volume ?? '0')) || 0;
    const reconstructed = (snapshot as Record<string, unknown>).quality_envelope
        ? ((snapshot as Record<string, unknown>).quality_envelope as Record<string, unknown>).is_gap_filled
            ? 'SYNTHETIC'
            : undefined
        : undefined;
    hist.candleTimes.push(ts);
    hist.candles.open.push(open);
    hist.candles.high.push(high);
    hist.candles.low.push(low);
    hist.candles.close.push(close);
    hist.candles.volume.push(vol);
    if (hist.candleReconstructed) hist.candleReconstructed.push(reconstructed);
    else hist.candleReconstructed = [reconstructed];

    const incoming = (snapshot.indicators && typeof snapshot.indicators === 'object'
        ? (snapshot.indicators as Record<string, { raw_value?: number; state_label?: string; values?: Record<string, number> }>)
        : null);

    const existingKeys = Object.keys(hist.values);
    const incomingKeys = new Set<string>();
    if (incoming) {
        for (const [k, dto] of Object.entries(incoming)) {
            const isWarming = dto?.state_label === 'WARMING';
            const raw = isWarming ? null : typeof dto.raw_value === 'number' && Number.isFinite(dto.raw_value) ? dto.raw_value : null;
            const rk = k;
            incomingKeys.add(rk);
            if (!(rk in hist.values)) {
                hist.values[rk] = Array(hist.times.length - 1).fill(null);
            }
            hist.values[rk].push(raw);
            if (dto.values && typeof dto.values === 'object') {
                for (const [sub, sv] of Object.entries(dto.values)) {
                    const sk = `${k}.${sub}`;
                    incomingKeys.add(sk);
                    if (!(sk in hist.values)) {
                        hist.values[sk] = Array(hist.times.length - 1).fill(null);
                    }
                    const sval = isWarming ? null : typeof sv === 'number' && Number.isFinite(sv) ? sv : null;
                    hist.values[sk].push(sval);
                }
            }
        }
    }
    for (const ek of existingKeys) {
        if (!incomingKeys.has(ek)) {
            hist.values[ek].push(null);
        }
    }

    if (hist.times.length > HIST_MAX) {
        const trim = hist.times.length - HIST_MAX;
        hist.times.splice(0, trim);
        hist.candleTimes.splice(0, trim);
        hist.candles.open.splice(0, trim);
        hist.candles.high.splice(0, trim);
        hist.candles.low.splice(0, trim);
        hist.candles.close.splice(0, trim);
        hist.candles.volume.splice(0, trim);
        if (hist.candleReconstructed) hist.candleReconstructed.splice(0, trim);
        for (const arr of Object.values(hist.values)) {
            arr.splice(0, trim);
        }
    }
    hist.fetchedAtMs = Date.now();
}

export function appendLiveCandle(
    pairKey: string,
    timeframe: number,
    slot: number | string | undefined,
    candle: CandleOHLCV,
): void {
    if (!pairKey || !timeframe || !candle || candle.reconstructed) return;
    const t = Number(candle.time);
    if (!Number.isFinite(t) || t <= 0) return;
    const key = keyFor(pairKey, timeframe, slot);
    const existing = liveCandles.get(key) ?? [];
    if (existing.length > 0 && Number(existing[existing.length - 1].time) === t) {
        existing[existing.length - 1] = candle;
        liveCandles.set(key, existing);
        return;
    }
    if (existing.length > 0 && t < Number(existing[existing.length - 1].time)) {
        return;
    }
    existing.push(candle);
    if (existing.length > 1000) {
        existing.splice(0, existing.length - 1000);
    }
    liveCandles.set(key, existing);
}

// For facade: expose internal maps for debugging
export function _getLiveHistoryMap(): Map<string, IndicatorFlatHistory> {
    return liveHistory;
}
export function _getLiveCandleMap(): Map<string, CandleOHLCV[]> {
    return liveCandles;
}
