import { describe, it, expect } from 'vitest';
import { biasColor, riskDangerColor, qualityColor, directionColor, directionLabel, signalLabel, signalQualityBucket, rrColor, scoreColor, formatRR, asciiBar, DIRECTION_COLORS, directionColorFor, directionBackgroundFor, type DirectionMode } from './dashboardColors';

describe('biasColor', () => {
    it('StrongBullish -> good green', () => {
        expect(biasColor('StrongBullish')).toBe('#22c55e');
    });
    it('Bullish -> bull green', () => {
        expect(biasColor('Bullish')).toBe('#4ade80');
    });
    it('Neutral -> amber', () => {
        expect(biasColor('Neutral')).toBe('#f59e0b');
    });
    it('Bearish -> red', () => {
        expect(biasColor('Bearish')).toBe('#f87171');
    });
    it('StrongBearish -> dark red', () => {
        expect(biasColor('StrongBearish')).toBe('#dc2626');
    });
    it('null -> amber', () => {
        expect(biasColor(null)).toBe('#f59e0b');
    });
    it('SCREAMING_SNAKE wire shape derives correctly', () => {
        expect(biasColor('STRONG_BULLISH')).toBe('#22c55e');
        expect(biasColor('STRONG_BEARISH')).toBe('#dc2626');
        expect(biasColor('BULLISH')).toBe('#4ade80');
        expect(biasColor('BEARISH')).toBe('#f87171');
    });
});

describe('riskDangerColor', () => {
    it('80+ -> extreme red (canonical level band)', () => {
        expect(riskDangerColor(85)).toBe('#ef4444');
        expect(riskDangerColor(80)).toBe('#ef4444');
    });
    it('60-79 -> high red (canonical level band)', () => {
        expect(riskDangerColor(60)).toBe('#f87171');
        expect(riskDangerColor(75)).toBe('#f87171');
    });
    it('40-59 -> moderate amber (canonical level band)', () => {
        expect(riskDangerColor(40)).toBe('#f59e0b');
        expect(riskDangerColor(45)).toBe('#f59e0b');
        expect(riskDangerColor(59)).toBe('#f59e0b');
    });
    it('<40 -> low green (canonical level band)', () => {
        expect(riskDangerColor(20)).toBe('#22c55e');
        expect(riskDangerColor(39)).toBe('#22c55e');
    });
    it('null -> inactive', () => {
        expect(riskDangerColor(null)).toBe('rgba(255,255,255,0.25)');
    });
});

describe('qualityColor', () => {
    it('v10.1 3-band: green ≥70, amber 50-69, grey below', () => {
        expect(qualityColor(85)).toBe('#22c55e');
        expect(qualityColor(60)).toBe('#f59e0b');
        expect(qualityColor(40)).toBe('rgba(255,255,255,0.35)');
        expect(qualityColor(20)).toBe('rgba(255,255,255,0.35)');
    });
});

describe('directionColor', () => {
    it('LONG -> bull green', () => {
        expect(directionColor('LONG')).toBe('#4ade80');
    });
    it('SHORT -> red', () => {
        expect(directionColor('SHORT')).toBe('#f87171');
    });
    it('NEUTRAL -> amber', () => {
        expect(directionColor('NEUTRAL')).toBe('#f59e0b');
    });
    it('null -> amber', () => {
        expect(directionColor(null)).toBe('#f59e0b');
    });
});

describe('directionLabel', () => {
    it('long guidance variants collapse to LONG', () => {
        expect(directionLabel('StrongLong')).toBe('LONG');
        expect(directionLabel('Long')).toBe('LONG');
        expect(directionLabel('LONG')).toBe('LONG');
    });
    it('short guidance variants collapse to SHORT', () => {
        expect(directionLabel('StrongShort')).toBe('SHORT');
        expect(directionLabel('Short')).toBe('SHORT');
    });
    it('Neutral -> NEUTRAL', () => {
        expect(directionLabel('Neutral')).toBe('NEUTRAL');
    });
    it('null -> NEUTRAL', () => {
        expect(directionLabel(null)).toBe('NEUTRAL');
    });
});

describe('signalLabel', () => {
    it('BUY for LONG', () => {
        expect(signalLabel('Long')).toBe('BUY');
        expect(signalLabel('StrongLong')).toBe('BUY');
    });
    it('SELL for SHORT', () => {
        expect(signalLabel('Short')).toBe('SELL');
        expect(signalLabel('StrongShort')).toBe('SELL');
    });
    it('WAIT for Neutral', () => {
        expect(signalLabel('Neutral')).toBe('WAIT');
        expect(signalLabel(null)).toBe('WAIT');
    });
});

describe('signalQualityBucket', () => {
    it('STRONG >= 70', () => {
        expect(signalQualityBucket(70)).toBe('STRONG');
        expect(signalQualityBucket(95)).toBe('STRONG');
    });
    it('MODERATE 40-69', () => {
        expect(signalQualityBucket(40)).toBe('MODERATE');
        expect(signalQualityBucket(69)).toBe('MODERATE');
    });
    it('WEAK < 40', () => {
        expect(signalQualityBucket(39)).toBe('WEAK');
        expect(signalQualityBucket(0)).toBe('WEAK');
        expect(signalQualityBucket(null)).toBe('WEAK');
    });
});

describe('rrColor', () => {
    it('>= 2.0 -> good green', () => {
        expect(rrColor(2.5)).toBe('#22c55e');
    });
    it('1.0-1.99 -> amber', () => {
        expect(rrColor(1.5)).toBe('#f59e0b');
    });
    it('< 1.0 -> amber (v10.1 — red is SHORT only)', () => {
        expect(rrColor(0.5)).toBe('#f59e0b');
    });
    it('null -> muted', () => {
        expect(rrColor(null)).toBe('rgba(255,255,255,0.55)');
    });
    it('0 -> muted', () => {
        expect(rrColor(0)).toBe('rgba(255,255,255,0.55)');
    });
});

describe('scoreColor', () => {
    it('85+ -> good green', () => {
        expect(scoreColor(95)).toBe('#22c55e');
    });
    it('50-84 -> amber (v10.1 3-band)', () => {
        expect(scoreColor(75)).toBe('#f59e0b');
    });
    it('<50 -> grey (v10.1 — red is SHORT only)', () => {
        expect(scoreColor(40)).toBe('rgba(255,255,255,0.35)');
        expect(scoreColor(20)).toBe('rgba(255,255,255,0.35)');
    });
});

describe('formatRR', () => {
    it('formats 1:2.50', () => {
        expect(formatRR(2.5)).toBe('1 : 2.50');
    });
    it('formats 1:1.00', () => {
        expect(formatRR(1.0)).toBe('1 : 1.00');
    });
    it('null -> —', () => {
        expect(formatRR(null)).toBe('—');
    });
    it('0 -> —', () => {
        expect(formatRR(0)).toBe('—');
    });
    it('negative -> —', () => {
        expect(formatRR(-1)).toBe('—');
    });
});

describe('asciiBar', () => {
    it('0% -> empty bar', () => {
        expect(asciiBar(0)).toBe('░░░░░░░░░░');
    });
    it('100% -> full bar', () => {
        expect(asciiBar(100)).toBe('██████████');
    });
    it('50% -> half bar', () => {
        expect(asciiBar(50)).toBe('█████░░░░░');
    });
    it('clamps > 100', () => {
        expect(asciiBar(150)).toBe('██████████');
    });
    it('clamps < 0', () => {
        expect(asciiBar(-10)).toBe('░░░░░░░░░░');
    });
    it('respects custom width', () => {
        expect(asciiBar(50, 4)).toBe('██░░');
    });
});

// ── v7.0-prod direction-vocabulary palette ─────────────────────────────
//
// The top badge on every MME tab is one of four canonical foreground
// colours:
//   green   — long / bullish
//   red     — short / bearish
//   amber   — sideways / neutral / hold / wait / stand aside
//   gray    — disconnected / no data

describe('DIRECTION_COLORS palette', () => {
    const expectedHex: Record<DirectionMode, string> = {
        long: '#22c55e',
        short: '#ef4444',
        sideways: '#f59e0b',
        nodata: 'rgba(255, 255, 255, 0.35)',
    };
    const expectedRgba: Record<DirectionMode, string> = {
        long: 'rgba(34, 197, 94, 0.10)',
        short: 'rgba(239, 68, 68, 0.10)',
        sideways: 'rgba(245, 158, 11, 0.10)',
        nodata: 'rgba(255, 255, 255, 0.04)',
    };

    for (const mode of ['long', 'short', 'sideways', 'nodata'] as DirectionMode[]) {
        it(`${mode} foreground = ${expectedHex[mode]}`, () => {
            expect(DIRECTION_COLORS[mode].hex).toBe(expectedHex[mode]);
            expect(directionColorFor(mode)).toBe(expectedHex[mode]);
        });
        it(`${mode} background = ${expectedRgba[mode]}`, () => {
            expect(DIRECTION_COLORS[mode].rgba).toBe(expectedRgba[mode]);
            expect(directionBackgroundFor(mode)).toBe(expectedRgba[mode]);
        });
    }

    it('green and red are reserved for long / short only', () => {
        // Cross-check that no other mode can leak into green/red —
        // enforces "green = long, red = short" the operator requires.
        expect(DIRECTION_COLORS.long.hex).toBe('#22c55e');
        expect(DIRECTION_COLORS.short.hex).toBe('#ef4444');
        expect(DIRECTION_COLORS.sideways.hex).not.toBe('#22c55e');
        expect(DIRECTION_COLORS.sideways.hex).not.toBe('#ef4444');
        expect(DIRECTION_COLORS.nodata.hex).not.toBe('#22c55e');
        expect(DIRECTION_COLORS.nodata.hex).not.toBe('#ef4444');
    });
});
