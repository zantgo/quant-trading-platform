/**
 * Browser console debug dump for candle + indicator-overlay verification.
 *
 * Fires on **every completed candle** (is_completed === true) for any
 * (instance, duration). Payload aggregates **all** instances × ACTIVE durations,
 * including background timeframes, so a single log line gives a full
 * cross-section of warmup + rolling buffer state.
 *
 * Intended usage:
 *   - Default DISABLED (opt-in only — Console stays clean per user request).
 *   - Enable at runtime: `window.__CANDLE_DEBUG_ENABLED__ = true` or
 *     `localStorage.setItem('candleDebug','1')` (then reload)
 *   - Disable via `window.__CANDLE_DEBUG_ENABLED__ = false` or `localStorage.setItem('candleDebug','0')`
 *
 * Log format is JSON so it can be copied/piped to jq/pandas.
 * One `console.log` per completed candle: `[CANDLE_DEBUG] <json>`
 */
import type { AppStore } from '../state.svelte';
import { activeDurations } from './terms';
import { getResolvedHistory } from './indicatorHistory';

declare global {
    interface Window {
        __CANDLE_DEBUG_ENABLED__?: boolean;
    }
}

function isDebugEnabled(): boolean {
    // Option A: opt-in only — default OFF so Console stays clean.
    // Enable via `window.__CANDLE_DEBUG_ENABLED__ = true` or `localStorage.setItem('candleDebug','1')` (then reload).
    // Explicit opt-in required; no logs unless operator asks.
    if (typeof window !== 'undefined' && window.__CANDLE_DEBUG_ENABLED__ === true) return true;
    try {
        if (typeof localStorage !== 'undefined' && localStorage.getItem('candleDebug') === '1') return true;
        if (typeof localStorage !== 'undefined' && localStorage.getItem('candleDebug') === 'true') return true;
        // Legacy support: window flag set, but localStorage '0' still wins as explicit off
        if (typeof window !== 'undefined' && window.__CANDLE_DEBUG_ENABLED__ === false) return false;
        if (typeof localStorage !== 'undefined' && localStorage.getItem('candleDebug') === '0') return false;
        if (typeof localStorage !== 'undefined' && localStorage.getItem('candleDebug') === 'false') return false;
    } catch {}
    return false;
}

export interface CandleDebugOverlayDump {
    key: string; // e.g. "ema_stack.fast" or "bollinger.upper"
    values: Array<number | null>; // aligned to times[] (null = WARMING / missing)
    lastValue: number | null;
    count: number;
}

export interface CandleDebugTimeframe {
    slot: number;
    timeframe_secs: number;
    barDurationSec: number;
    pipelineState: string | undefined;
    candleCount: number; // candles.candleTimes.length
    timesCount: number; // history.times.length (should equal candleCount when warm)
    bufferLen: number;
    // Full candles currently in memory for this slot (up to HIST_BUFFER_MAX=1000)
    candles: Array<{
        time: number; // epoch seconds
        open: number;
        high: number;
        low: number;
        close: number;
        volume: number;
        reconstructed?: string;
    }>;
    // Every indicator overlay value series currently cached for this slot
    // (keys = history.values keys, each array capped at 1000 and aligned to times[])
    indicatorOverlays: Record<string, Array<number | null>>;
    // Convenience: last overlay values for quick overlay-works check
    lastOverlayValues: Record<string, number | null>;
    // Snapshot-level view (what WS just delivered)
    latestSnapshot: {
        timestamp: number | null;
        is_completed: boolean | undefined;
        close: number | null;
        open: number | null;
        high: number | null;
        low: number | null;
        volume: number | null;
        indicators: Record<string, { raw_value: number; normalized: number; state_label: string; values?: Record<string, number> | null }>;
        indicatorLifecycle: Record<string, unknown> | null;
    } | null;
    historyTimes: number[]; // epoch seconds of history.times (full, up to 1000)
    // Cross-check: do all value arrays align to times length?
    alignmentOk: boolean;
}

export interface CandleDebugInstance {
    pairKey: string; // e.g. BTC-USDT
    exchange: string;
    instanceId?: string;
    isConnected: boolean;
    timeframes: CandleDebugTimeframe[];
}

export interface CandleDebugPayload {
    event: 'completed_candle';
    emittedAt: string; // ISO
    trigger: {
        pairKey: string;
        slot: number;
        timeframe_secs: number;
        timestamp: number | null;
        close: number | null;
    };
    instances: CandleDebugInstance[];
    // Summary for fast human scan
    summary: {
        totalInstances: number;
        totalTimeframes: number;
        maxCandlesPerTf: number;
        minCandlesPerTf: number;
        warmupOk_300: boolean; // all >=60s TFs have >=300 ? (indicator floor)
        bootstrapOk_500: boolean; // all >=60s TFs have >=500 ? (initial 500)
        cappedAt_1000: boolean; // all TFs <=1000
    };
}

function toNum(v: unknown): number | null {
    if (v == null) return null;
    const n = typeof v === 'number' ? v : parseFloat(String(v));
    return Number.isFinite(n) ? n : null;
}

/**
 * Build the full cross-instance, cross-timeframe debug payload.
 * Reads `app.instancesMap` for liveness + snapshot, and `indicatorHistory`
 * `historyData` (via getResolvedHistory) for the actual rolling candle +
 * overlay arrays that back the chart.
 */
export function buildCandleDebugPayload(
    app: AppStore,
    trigger: { pairKey: string; slot: number; timeframe_secs: number; snapshot: Record<string, unknown> },
): CandleDebugPayload {
    const instances: CandleDebugInstance[] = [];
    let globalMax = 0;
    let globalMin = Infinity;
    let allGte300 = true;
    let allGte500 = true;
    let allLte1000 = true;

    for (const [pairKey, inst] of Object.entries(app.instancesMap)) {
        // v11.9: walk the ACTIVE ladder only — inactive durations never stream
        // and would only add all-empty noise to the dump.
        const slots = activeDurations(inst);
        const tfs: CandleDebugTimeframe[] = slots.map((slot) => {
            const tf = (inst as { terms?: Record<number, import('../types').TimeframeTelemetry> }).terms?.[slot];
            const barDurationSec = tf?.barDurationSec ?? 0;
            const hist = getResolvedHistory(pairKey, barDurationSec, slot);
            const candles: CandleDebugTimeframe['candles'] = [];
            const indicatorOverlays: Record<string, Array<number | null>> = {};
            const lastOverlayValues: Record<string, number | null> = {};
            let historyTimes: number[] = [];
            let candleCount = 0;
            let timesCount = 0;
            let alignmentOk = true;

            if (hist) {
                historyTimes = [...hist.times];
                timesCount = hist.times.length;
                candleCount = hist.candleTimes.length;
                // Build candle array aligned to candleTimes (authoritative OHLCV)
                for (let i = 0; i < hist.candleTimes.length; i++) {
                    candles.push({
                        time: hist.candleTimes[i],
                        open: hist.candles.open[i] ?? 0,
                        high: hist.candles.high[i] ?? 0,
                        low: hist.candles.low[i] ?? 0,
                        close: hist.candles.close[i] ?? 0,
                        volume: hist.candles.volume[i] ?? 0,
                        reconstructed: hist.candleReconstructed?.[i],
                    });
                }
                // Capture every overlay series
                for (const [k, arr] of Object.entries(hist.values)) {
                    indicatorOverlays[k] = [...arr];
                    // last non-null value for quick check
                    let last: number | null = null;
                    for (let i = arr.length - 1; i >= 0; i--) {
                        if (arr[i] != null) { last = arr[i]; break; }
                    }
                    // If array still warming (all null), last stays null
                    lastOverlayValues[k] = last;
                    if (arr.length !== hist.times.length) alignmentOk = false;
                }
                if (hist.times.length !== hist.candleTimes.length) {
                    // times vs candleTimes can diverge by gap-fill policy; mark but not fail
                    // candleTimes is the candle truth, times is indicator axis
                }
            } else {
                // No history yet (cold start, slot not yet bootstrapped)
                candles.length = 0;
            }

            // Also consider liveCandleCache as fallback for cold TFs (sub-minute live-append)
            // — already reflected via ingestLiveSnapshot, so historyData is primary.

            const snap = tf?.latestSnapshot as Record<string, unknown> | null;
            const snapIndicators = (snap?.indicators as Record<string, { raw_value: number; normalized: number; state_label: string; values?: Record<string, number> | null }>) ?? {};
            // Normalize indicators map for JSON safety
            const snapIndicatorsJson: Record<string, { raw_value: number; normalized: number; state_label: string; values?: Record<string, number> | null }> = {};
            for (const [k, v] of Object.entries(snapIndicators)) {
                if (v && typeof v === 'object') snapIndicatorsJson[k] = {
                    raw_value: (v as { raw_value: number }).raw_value,
                    normalized: (v as { normalized: number }).normalized,
                    state_label: (v as { state_label: string }).state_label,
                    values: (v as { values?: Record<string, number> | null }).values ?? null,
                };
            }

            const latestSnapshot: CandleDebugTimeframe['latestSnapshot'] = snap
                ? {
                    timestamp: toNum((snap as Record<string, unknown>).timestamp),
                    is_completed: (snap as Record<string, unknown>).is_completed as boolean | undefined,
                    close: toNum((snap as Record<string, unknown>).close),
                    open: toNum((snap as Record<string, unknown>).open),
                    high: toNum((snap as Record<string, unknown>).high),
                    low: toNum((snap as Record<string, unknown>).low),
                    volume: toNum((snap as Record<string, unknown>).volume),
                    indicators: snapIndicatorsJson,
                    indicatorLifecycle: (snap as Record<string, unknown>).indicator_lifecycle as Record<string, unknown> | null ?? null,
                }
                : null;

            const bufferLen = Math.max(candleCount, timesCount);

            // Update summary bounds for >=60s TFs (the ones that should have warmup)
            if (barDurationSec >= 60) {
                if (candleCount < 300) allGte300 = false;
                if (candleCount < 500) {
                    // Only flag as not-ok if this instance/slot has actually started (has at least one candle)
                    // Cold sub-minute slots are ignored, but >=60s should be >=500 after bootstrap.
                    // If still <500 shortly after boot, it indicates REST/DB shortfall.
                    // We keep strict check: >=60s must eventually reach 500.
                    // For summary we treat 0 as not-ok only if instance is connected (i.e. pipeline running).
                    if (inst.isConnected || candleCount > 0) allGte500 = false;
                }
            }
            // Cap check is universal
            if (bufferLen > 1000) allLte1000 = false;
            if (bufferLen > globalMax) globalMax = bufferLen;
            if (bufferLen < globalMin) globalMin = bufferLen;

            return {
                slot,
                timeframe_secs: barDurationSec,
                barDurationSec,
                pipelineState: tf?.pipelineState,
                candleCount,
                timesCount,
                bufferLen,
                candles,
                indicatorOverlays,
                lastOverlayValues,
                latestSnapshot,
                historyTimes,
                alignmentOk,
            };
        });

        instances.push({
            pairKey,
            exchange: inst.exchange,
            instanceId: inst.instanceId,
            isConnected: inst.isConnected,
            timeframes: tfs,
        });
    }

    if (globalMin === Infinity) globalMin = 0;

    const triggerSnap = trigger.snapshot;
    const payload: CandleDebugPayload = {
        event: 'completed_candle',
        emittedAt: new Date().toISOString(),
        trigger: {
            pairKey: trigger.pairKey,
            slot: trigger.slot,
            timeframe_secs: trigger.timeframe_secs,
            timestamp: toNum(triggerSnap.timestamp),
            close: toNum(triggerSnap.close),
        },
        instances,
        summary: {
            totalInstances: instances.length,
            totalTimeframes: instances.reduce((a, i) => a + i.timeframes.length, 0),
            maxCandlesPerTf: globalMax,
            minCandlesPerTf: globalMin,
            warmupOk_300: allGte300,
            bootstrapOk_500: allGte500,
            cappedAt_1000: allLte1000,
        },
    };
    return payload;
}

/**
 * Emit the debug payload to the browser console as a single JSON line.
 * Call this on every `is_completed === true` frame.
 */
export function emitCandleDebug(app: AppStore, trigger: { pairKey: string; slot: number; timeframe_secs: number; snapshot: Record<string, unknown> }): void {
    if (!isDebugEnabled()) return;
    try {
        const payload = buildCandleDebugPayload(app, trigger);
        // Single console.log with JSON string — copy-paste friendly, jq-parseable.
        // Prefix tag lets the operator filter in DevTools via "[CANDLE_DEBUG]".
        console.log('[CANDLE_DEBUG]', JSON.stringify(payload));
        // Also emit a human-readable summary to console.info for quick scan.
        const s = payload.summary;
        const warmTag = s.warmupOk_300 ? '✅' : '⚠️';
        const bootTag = s.bootstrapOk_500 ? '✅' : '⚠️';
        const capTag = s.cappedAt_1000 ? '✅' : '❌';
        console.info(
            `[CANDLE_DEBUG_SUMMARY] trigger=${trigger.pairKey}/${trigger.slot}@${trigger.timeframe_secs}s ` +
            `instances=${s.totalInstances} tfs=${s.totalTimeframes} ` +
            `candles min/max=${s.minCandlesPerTf}/${s.maxCandlesPerTf} ` +
            `warmup≥300 ${warmTag} bootstrap≥500 ${bootTag} cap≤1000 ${capTag} ` +
            `at ${payload.emittedAt}`,
        );
    } catch (err) {
        console.error('[CANDLE_DEBUG] failed to build payload', err);
    }
}
