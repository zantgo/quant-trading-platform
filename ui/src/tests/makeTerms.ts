// Test helper — build a full duration-keyed `terms` record for
// `InstanceState` fixtures. Durations not listed in `overrides` get a
// minimal placeholder telemetry bound to its pool duration.
import type { InstanceState, TimeframeTelemetry } from '../types';
import { DURATIONS } from '../types';

export function emptyTerm(slotSecs: number, symbol = 'BTC'): TimeframeTelemetry {
    return {
        slot: slotSecs,
        symbol,
        exchange: 'Hyperliquid',
        barDurationSec: slotSecs,
        indicators: {},
        priceText: '--',
        volText: '--',
        avgVolText: '--',
        showPatterns: true,
        isCompleted: false,
        latestSnapshot: null,
        historyPrices: [],
        pipelineState: 'LOADING',
        indicatorLifecycle: {},
        heatmapLeverageTiers: [10],
    } as unknown as TimeframeTelemetry;
}

export function makeTerms(
    overrides: Partial<Record<number, Partial<TimeframeTelemetry>>> = {},
    symbol = 'BTC',
): Record<number, TimeframeTelemetry> {
    return Object.fromEntries(
        DURATIONS.map((secs) => [secs, { ...emptyTerm(secs, symbol), ...(overrides[secs] ?? {}) }]),
    ) as Record<number, TimeframeTelemetry>;
}

/// Attach a full `terms` record to a partial InstanceState fixture.
export function withTerms(
    pair: Partial<InstanceState> & { symbol?: string },
    overrides: Partial<Record<number, Partial<TimeframeTelemetry>>> = {},
): InstanceState {
    return {
        ...pair,
        terms: makeTerms(overrides, pair.symbol ?? 'BTC'),
    } as unknown as InstanceState;
}
