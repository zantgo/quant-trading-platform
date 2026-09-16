import type { AppStore } from '../state.svelte';
import type { IndicatorMap, TimeframeSlotKind, TimeframeTelemetry } from '../types';
import { getTerm } from './terms';

export type ChartSlot = TimeframeSlotKind;

export interface ChartCoalescer {
    effect: () => void;
    destroy: () => void;
}

function deepUnwrap<T>(value: T): T {
    if (value == null || typeof value !== 'object') return value;
    try {
        return JSON.parse(JSON.stringify(value)) as T;
    } catch {
        if (Array.isArray(value)) {
            return (value as unknown[]).map((v) => deepUnwrap(v)) as unknown as T;
        }
        const out: Record<string, unknown> = {};
        for (const key of Object.keys(value as object)) {
            try {
                out[key] = deepUnwrap((value as Record<string, unknown>)[key]);
            } catch {
                out[key] = (value as Record<string, unknown>)[key];
            }
        }
        return out as T;
    }
}

export function makeChartCoalescer(
    app: AppStore,
    pairKey: string | (() => string),
    slot: ChartSlot | (() => ChartSlot),
    tick: (
        snap: { timestamp: number; open?: unknown; high?: unknown; low?: unknown; close?: unknown; volume?: unknown; indicators?: IndicatorMap | null },
        tfPlain: TimeframeTelemetry,
    ) => void,
): ChartCoalescer {
    let pending = false;
    let destroyed = false;

    const getPairKey = (): string =>
        typeof pairKey === 'function' ? (pairKey as () => string)() : pairKey;
    const getSlot = (): ChartSlot =>
        typeof slot === 'function' ? (slot as () => ChartSlot)() : slot;

    function readTf(): { snap: { timestamp: number; open?: unknown; high?: unknown; low?: unknown; close?: unknown; volume?: unknown; indicators?: IndicatorMap | null }; tfPlain: TimeframeTelemetry } | null {
        const curSlot = getSlot();
        const pairVal = app.instancesMap[getPairKey()];
        if (!pairVal) return null;
        const tfVal = getTerm(pairVal, curSlot);

        const rawSnap = tfVal?.latestSnapshot;
        if (!rawSnap) return null;

        const snap = deepUnwrap(rawSnap) as { timestamp: number; open?: unknown; high?: unknown; low?: unknown; close?: unknown; volume?: unknown; indicators?: IndicatorMap | null };

        const ts = Number(snap.timestamp ?? 0);
        if (!Number.isFinite(ts) || ts <= 0) return null;
        snap.timestamp = ts;

        let plainIndicators: IndicatorMap | null = null;
        try {
            const raw = tfVal.indicators;
            plainIndicators = raw ? JSON.parse(JSON.stringify(raw)) as IndicatorMap : null;
        } catch {
            plainIndicators = null;
        }
        snap.indicators = plainIndicators;

        let tfPlain: TimeframeTelemetry;
        try {
            tfPlain = {
                ...JSON.parse(JSON.stringify(tfVal)),
                indicators: plainIndicators,
            } as TimeframeTelemetry;
        } catch {
            tfPlain = { indicators: plainIndicators } as TimeframeTelemetry;
        }

        return { snap, tfPlain };
    }

    return {
        effect: () => {
            if (destroyed || pending) return;
            const first = readTf();
            if (!first) return;
            pending = true;
            requestAnimationFrame(() => {
                pending = false;
                if (destroyed) return;
                const next = readTf();
                if (!next) return;
                tick(next.snap, next.tfPlain);
            });
        },
        destroy: () => { destroyed = true; },
    };
}
