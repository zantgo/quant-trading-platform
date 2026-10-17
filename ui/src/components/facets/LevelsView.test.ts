// @vitest-environment jsdom
//
// LevelsView — liquidation-magnet distance formatting (audit fix):
//   LV-1: `distance_from_mid_pct` is an absolute percentage on the wire
//         (0.6 = 0.6%), so the magnet row must render "0.60%" — never a
//         100× inflated "60.00%".
//   LV-2: non-finite distances render as an em dash.

import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import LevelsView from './LevelsView.svelte';
import type { IndicatorMeta, StateBand, TimeframeTelemetry } from '../../types';

function makeTf(overrides: Partial<TimeframeTelemetry> = {}): TimeframeTelemetry {
    return {
        symbol: 'BTC-USDT',
        slot: 'micro',
        timeframeSecs: 60,
        barDurationSec: 60,
        priceText: '50000',
        indicators: {},
        latestSnapshot: null,
        context: null,
        liquidity: null,
        cluster: null,
        volumeProfile: null,
        liquiditySignals: [],
        indicatorLifecycle: {},
        pipelineState: 'Live',
        isCompleted: false,
        exchange: null,
        ...overrides,
    } as TimeframeTelemetry;
}

/**
 * CR-10's replacement for the liquidation-magnet distance tests.
 *
 * The two originals asserted that `distance_from_mid_pct` rendered as an
 * ABSOLUTE percentage — `0.6` as `0.60%`, not `60.00%` — and that a
 * non-finite distance rendered an em dash rather than `NaN`. That was a real
 * formatting bug guard against a field that described a cluster's distance
 * from mid.
 *
 * The state-band rows render proximity as a percentile (`proximity_percentile`)
 * rather than a distance, so the absolute-percentage trap cannot recur — but
 * the second guard generalises, and it is kept: a band whose numbers arrive
 * non-finite must not print `NaN`.
 */
describe('LevelsView state-band rows', () => {
    const band = (over: Partial<StateBand> = {}): StateBand => ({
        price_low: 50100,
        price_high: 50500,
        band_atr_multiple: 0.5,
        volume_percentile: 90,
        proximity_percentile: 75,
        intensity_pct: 67.5,
        basis: 'HighVolumeNode',
        volume_asymmetry: 0.4,
        ...over,
    });

    const withBands = (bands: StateBand[]): Partial<TimeframeTelemetry> => ({
        stateBands: {
            symbol: 'BTC-USDT',
            timeframe_label: '1s',
            timeframe_secs: 60,
            poc_price: 50000,
            value_area_high: 50200,
            value_area_low: 49800,
            has_profile: true,
            has_bands: true,
            bands,
            summary: { volume_asymmetry: 0.2, contributing_bins: 40, band_count: bands.length },
            timestamp_ms: 1700000000000,
        },
    });

    it('renders proximity as a whole-number percentile, never as a percentage of price', () => {
        const tf = makeTf(withBands([band()]));
        const { container } = render(LevelsView, { tf, registry: [] as IndicatorMeta[] });
        const text = container.textContent ?? '';
        expect(text).toContain('75% near');
        // The old failure mode was a raw fraction reaching the formatter and
        // becoming a 60× number. Proximity is already 0..100, so anything past
        // three digits in a percent column is that bug returning.
        expect(text).not.toMatch(/\d{3,}\.\d{2}%/);
    });

    it('renders an em dash rather than NaN for non-finite band numbers', () => {
        const tf = makeTf(withBands([
            band({ proximity_percentile: Number.NaN, volume_percentile: Number.NaN, volume_asymmetry: Number.NaN }),
        ]));
        const { container } = render(LevelsView, { tf, registry: [] as IndicatorMeta[] });
        const text = container.textContent ?? '';
        expect(text).not.toContain('NaN');
        expect(text).toContain('—');
    });

    it('classifies the side from the band own buy/sell asymmetry', () => {
        const buy = makeTf(withBands([band({ volume_asymmetry: 0.62 })]));
        const sell = makeTf(withBands([band({ volume_asymmetry: -0.48 })]));
        const flat = makeTf(withBands([band({ volume_asymmetry: 0.01 })]));
        const textOf = (props: { tf: TimeframeTelemetry }) =>
            (render(LevelsView, { tf: props.tf, registry: [] as IndicatorMeta[] }).container.textContent ?? '');
        expect(textOf({ tf: buy })).toContain('BUY');
        expect(textOf({ tf: sell })).toContain('SELL');
        // Below the 0.05 threshold a side label would be a rounding claim.
        expect(textOf({ tf: flat })).toContain('MIX');
    });
});
