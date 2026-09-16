// Test helper — build a full 10-slot `terms` record for `InstanceState`
// fixtures. Slots not listed in `overrides` get a minimal placeholder
// telemetry bound to the fixed-ladder duration for that slot.
import type { InstanceState, TimeframeSlotKind, TimeframeTelemetry } from '../types';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_DURATION_SECS } from '../types';

export function emptyTerm(slot: TimeframeSlotKind, symbol = 'BTC'): TimeframeTelemetry {
    return {
        slot,
        symbol,
        exchange: 'Hyperliquid',
        barDurationSec: TIMEFRAME_SLOT_DURATION_SECS[slot],
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
    overrides: Partial<Record<TimeframeSlotKind, Partial<TimeframeTelemetry>>> = {},
    symbol = 'BTC',
): Record<TimeframeSlotKind, TimeframeTelemetry> {
    return Object.fromEntries(
        TIMEFRAME_SLOT_KINDS.map((slot) => [slot, { ...emptyTerm(slot, symbol), ...(overrides[slot] ?? {}) }]),
    ) as Record<TimeframeSlotKind, TimeframeTelemetry>;
}

/// Attach a full `terms` record to a partial InstanceState fixture.
export function withTerms(
    pair: Partial<InstanceState> & { symbol?: string },
    overrides: Partial<Record<TimeframeSlotKind, Partial<TimeframeTelemetry>>> = {},
): InstanceState {
    return {
        ...pair,
        terms: makeTerms(overrides, pair.symbol ?? 'BTC'),
    } as unknown as InstanceState;
}
