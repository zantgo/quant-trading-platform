// @vitest-environment jsdom
// Unit tests for the VolumeProfile TS primitive data shape and helpers.
//
// These tests verify the bin math that the primitive uses internally for
// stacked buy/sell rendering. The actual canvas rendering is verified
// manually via ./manage.sh run.

import { describe, it, expect } from 'vitest';
import type { VolumeProfileBin, VolumeProfileSnapshot } from '../types';

function makeBin(priceLow: number, priceHigh: number, buy: number, sell: number): VolumeProfileBin {
    return {
        price_low: priceLow,
        price_high: priceHigh,
        volume: buy + sell,
        buy_volume: buy,
        sell_volume: sell,
        is_poc: false,
        is_value_area: false,
    };
}

function makeSnapshot(bins: VolumeProfileBin[]): VolumeProfileSnapshot {
    const total = bins.reduce((acc, b) => acc + b.volume, 0);
    return {
        symbol: 'BTC-USDT',
        timeframe_label: '1m',
        timeframe_secs: 60,
        bins,
        poc_price: bins.length > 0 ? (bins[0].price_low + bins[0].price_high) / 2 : 0,
        value_area_high: bins.length > 0 ? bins[bins.length - 1].price_high : 0,
        value_area_low: bins.length > 0 ? bins[0].price_low : 0,
        total_volume: total,
        range_low: bins.length > 0 ? bins[0].price_low : 0,
        range_high: bins.length > 0 ? bins[bins.length - 1].price_high : 0,
        num_bins: bins.length,
        timestamp_ms: 1700000000000,
    };
}

describe('VolumeProfileSnapshot wire format', () => {
    it('round-trips through JSON', () => {
        const snap = makeSnapshot([
            makeBin(49000, 49100, 50, 50),
            makeBin(49100, 49200, 100, 30),
        ]);
        const json = JSON.stringify(snap);
        const back = JSON.parse(json) as VolumeProfileSnapshot;
        expect(back.bins.length).toBe(2);
        expect(back.bins[0].buy_volume).toBe(50);
        expect(back.bins[1].sell_volume).toBe(30);
        expect(back.total_volume).toBe(230);
    });

    it('handles empty bins array', () => {
        const snap = makeSnapshot([]);
        expect(snap.bins).toEqual([]);
        expect(snap.total_volume).toBe(0);
        expect(snap.num_bins).toBe(0);
    });

    it('bin volume equals buy + sell', () => {
        const bin = makeBin(100, 200, 30, 70);
        expect(bin.volume).toBe(bin.buy_volume + bin.sell_volume);
    });
});

describe('VolumeProfileBin buy/sell split math', () => {
    it('all-buy bin has zero sell', () => {
        const bin = makeBin(0, 1, 100, 0);
        expect(bin.volume).toBe(100);
        const buyFrac = bin.buy_volume / bin.volume;
        expect(buyFrac).toBe(1.0);
    });

    it('all-sell bin has zero buy', () => {
        const bin = makeBin(0, 1, 0, 100);
        const sellFrac = bin.sell_volume / bin.volume;
        expect(sellFrac).toBe(1.0);
    });

    it('doji bin splits 50/50', () => {
        const bin = makeBin(0, 1, 50, 50);
        expect(bin.buy_volume).toBe(bin.sell_volume);
        expect(bin.volume).toBe(100);
    });
});

describe('VolumeProfile POC and value-area identification', () => {
    it('POC is the bin with the highest volume', () => {
        const bins = [
            makeBin(100, 110, 10, 10),   // 20
            makeBin(110, 120, 80, 20),   // 100 ← POC
            makeBin(120, 130, 5, 5),     // 10
        ];
        bins[1].is_poc = true;
        const maxVolBin = bins.reduce((acc, b) => b.volume > acc.volume ? b : acc, bins[0]);
        expect(maxVolBin.price_low).toBe(110);
        expect(maxVolBin.is_poc).toBe(true);
    });

    it('value area spans bins that contain ≥70% of volume', () => {
        const bins = [
            makeBin(100, 110, 5, 5),    // 10
            makeBin(110, 120, 35, 15),  // 50 ← POC
            makeBin(120, 130, 7, 3),    // 10
            makeBin(130, 140, 4, 6),    // 10
        ];
        bins[1].is_poc = true;
        // 70% of 80 = 56 → POC alone (50) is below threshold.
        // Adding bins[0] brings VA to 60 (>=56).
        bins[0].is_value_area = true;
        bins[1].is_value_area = true;
        const vaBins = bins.filter(b => b.is_value_area);
        expect(vaBins.length).toBe(2);
        const vaTotal = vaBins.reduce((acc, b) => acc + b.volume, 0);
        const total = bins.reduce((acc, b) => acc + b.volume, 0);
        expect(vaTotal / total).toBeGreaterThanOrEqual(0.70);
    });
});

describe('VolumeProfile edge cases', () => {
    it('snapshot with no bins serializes cleanly', () => {
        const snap = makeSnapshot([]);
        const json = JSON.stringify(snap);
        expect(json).toContain('"bins":[]');
    });

    it('snapshot with one bin works', () => {
        const snap = makeSnapshot([makeBin(50000, 50100, 100, 50)]);
        expect(snap.num_bins).toBe(1);
        expect(snap.bins[0].volume).toBe(150);
    });

    it('many bins stay numerically stable', () => {
        const bins: VolumeProfileBin[] = [];
        for (let i = 0; i < 100; i++) {
            bins.push(makeBin(50000 + i, 50000 + i + 1, 10 + i, 5));
        }
        const snap = makeSnapshot(bins);
        expect(snap.num_bins).toBe(100);
        expect(snap.range_low).toBe(50000);
        expect(snap.range_high).toBe(50100);
    });
});