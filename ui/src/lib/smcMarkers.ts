// Selective SMC markers — attach to a Candlestick series and push rolling
// markers on every snapshot. Only emits when the signal's confidence
// meets the threshold (default 0.7) so the price chart doesn't get
// cluttered with weak events.
//
// Marker labels:
//   BOS↑ / BOS↓    structure (Break of Structure)
//   CHoCH↑ / CHoCH↓ structure (Change of Character)
//   SWEEP↑ / SWEEP↓ liquidity sweep

import { createSeriesMarkers } from 'lightweight-charts';
import type { ISeriesApi, Time } from 'lightweight-charts';
import type { IndicatorDto, IndicatorSignal } from '../types';


export interface SmcInput {
    structure?: IndicatorDto | null;
    liquidity?: IndicatorDto | null;
}

export interface SmcMarkerController {
    push(timeSec: number, input: SmcInput): void;
    clear(): void;
}

interface LwcMarker {
    time: Time;
    position: 'aboveBar' | 'belowBar' | 'inBar';
    color: string;
    shape: 'arrowUp' | 'arrowDown' | 'circle' | 'square';
    text: string;
}

function markerFromSignal(timeSec: number, sig: IndicatorSignal): LwcMarker | null {
    if (sig.kind !== 'Breakout' && sig.kind !== 'TrendFlip' && sig.kind !== 'Threshold' && sig.kind !== 'BandTouch') {
        return null;
    }
    const label = (sig.label ?? '').toUpperCase();
    const isBullish = sig.direction === 'Bullish';
    const isBearish = sig.direction === 'Bearish';
    const isChoch = label.includes('CHOCH') || label.includes('CHANGE_OF_CHARACTER');
    const isBos = label.includes('BOS') || label.includes('BREAK_OF_STRUCTURE');
    const isSweep = label.includes('SWEEP');

    let text: string;
    if (isChoch) text = isBullish ? 'CH↑' : isBearish ? 'CH↓' : 'CH';
    else if (isBos) text = isBullish ? 'BO↑' : isBearish ? 'BO↓' : 'BO';
    else if (isSweep) text = isBullish ? 'SP↑' : isBearish ? 'SP↓' : 'SP';
    else text = isBullish ? '↑' : isBearish ? '↓' : '·';

    return {
        time: timeSec as Time,
        position: isBullish ? 'belowBar' : 'aboveBar',
        color: isBullish ? '#10b981' : isBearish ? '#ef4444' : '#f59e0b',
        shape: isBullish ? 'arrowUp' : isBearish ? 'arrowDown' : 'circle',
        text,
    };
}

export function createSmcMarkers(
    series: ISeriesApi<'Candlestick'>,
): SmcMarkerController {
    const api = createSeriesMarkers(series, []);
    const markers: LwcMarker[] = [];
    const seen = new Set<string>();
    const MAX_MARKERS = 60;

    function rebuild() {
        markers.sort((a, b) => (a.time as number) - (b.time as number));
        api.setMarkers(markers);
    }

    function ingest(timeSec: number, dto: IndicatorDto | null | undefined, namespace: string) {
        if (!dto) return;
        const sigs = (dto.signals ?? []) as IndicatorSignal[];
        // P10-E: a Dashed reading has incomplete inputs, so its signals are
        // tentative and do not become chart markers.
        if (dto.readiness === 'dashed') return;
        for (const sig of sigs) {
            const m = markerFromSignal(timeSec, sig);
            if (!m) continue;
            const key = `${timeSec}-${namespace}-${m.text}-${m.position}`;
            if (seen.has(key)) continue;
            seen.add(key);
            markers.push(m);
        }
        while (markers.length > MAX_MARKERS) {
            const dropped = markers.shift();
            if (dropped) seen.delete(`${dropped.time as number}-${dropped.text}`);
        }
    }

    return {
        push(timeSec: number, input: SmcInput) {
            ingest(timeSec, input.structure ?? null, 'struct');
            ingest(timeSec, input.liquidity ?? null, 'liq');
            rebuild();
        },
        clear() {
            markers.length = 0;
            seen.clear();
            api.setMarkers([]);
        },
    };
}
