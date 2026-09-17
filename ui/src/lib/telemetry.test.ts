// @vitest-environment jsdom
//
// Unit tests for the EMA ribbon helpers in `ui/src/lib/telemetry.ts`.
// Single source of truth verification: every consumer (on-screen cell,
// export body, chart overlay) must read the same record via these helpers.

import { describe, expect, it } from 'vitest';
import { distFromPrice, readEmaValues, emaSpreadPct, buildEmaRibbonView, buildEmaRibbonCellView, fmtPctSigned, EMA_ROLES } from './telemetry';

describe('distFromPrice', () => {
    it('returns positive when price is above the EMA', () => {
        expect(distFromPrice(100, 99)).toBeCloseTo(0.01, 10);
    });

    it('returns negative when price is below the EMA', () => {
        expect(distFromPrice(100, 101)).toBeCloseTo(-0.01, 10);
    });

    it('returns zero when price equals the EMA', () => {
        expect(distFromPrice(100, 100)).toBe(0);
    });

    it('returns null when either operand is missing or non-finite', () => {
        expect(distFromPrice(null, 99)).toBeNull();
        expect(distFromPrice(100, null)).toBeNull();
        expect(distFromPrice(undefined, 99)).toBeNull();
        expect(distFromPrice(100, undefined)).toBeNull();
        expect(distFromPrice(NaN, 99)).toBeNull();
        expect(distFromPrice(100, NaN)).toBeNull();
    });

    it('returns null when close is 0 (would divide by zero)', () => {
        expect(distFromPrice(0, 99)).toBeNull();
    });
});

describe('readEmaValues', () => {
    it('returns all four null when the indicator map is missing', () => {
        const r = readEmaValues(undefined);
        expect(r).toEqual({ fast: null, medium: null, slow: null, long: null });
    });

    it('returns all four null when the ema_stack entry is missing', () => {
        const r = readEmaValues({});
        expect(r).toEqual({ fast: null, medium: null, slow: null, long: null });
    });

    it('returns the four values when present', () => {
        const r = readEmaValues({
            ema_stack: {
                values: { fast: 64018.2, medium: 64110.0, slow: 63980.4, long: 63845.0 },
            },
        } as any);
        expect(r.fast).toBe(64018.2);
        expect(r.medium).toBe(64110.0);
        expect(r.slow).toBe(63980.4);
        expect(r.long).toBe(63845.0);
    });

    it('fills missing sub-values with null', () => {
        const r = readEmaValues({
            ema_stack: { values: { fast: 64000, long: 63800 } },
        } as any);
        expect(r.fast).toBe(64000);
        expect(r.medium).toBeNull();
        expect(r.slow).toBeNull();
        expect(r.long).toBe(63800);
    });
});

describe('emaSpreadPct', () => {
    it('returns positive when fast is above long', () => {
        expect(emaSpreadPct({ fast: 64018, long: 63845 }, 64000)).toBeCloseTo((64018 - 63845) / 64000, 10);
    });

    it('returns negative when fast is below long', () => {
        expect(emaSpreadPct({ fast: 63845, long: 64018 }, 64000)).toBeCloseTo((63845 - 64018) / 64000, 10);
    });

    it('returns null when either line or close is missing/invalid', () => {
        expect(emaSpreadPct({ fast: null, long: 63845 }, 64000)).toBeNull();
        expect(emaSpreadPct({ fast: 64018, long: null }, 64000)).toBeNull();
        expect(emaSpreadPct({ fast: 64018, long: 63845 }, null)).toBeNull();
        expect(emaSpreadPct({ fast: 64018, long: 63845 }, 0)).toBeNull();
    });
});

describe('buildEmaRibbonView', () => {
    it('builds the unified view: 4 values + 4 distances + spread', () => {
        const view = buildEmaRibbonView(
            {
                indicators: {
                    ema_stack: {
                        values: { fast: 64018.2, medium: 64110.0, slow: 63980.4, long: 63845.0 },
                    },
                },
            } as any,
            64000,
        );
        expect(view.values.fast).toBe(64018.2);
        expect(view.distance.fast).toBeCloseTo(-0.00028, 4);
        expect(view.distance.long).toBeCloseTo(0.00241, 4);
        expect(view.spread).toBeCloseTo((64018.2 - 63845.0) / 64000, 10);
    });

    it('returns all-null distances when close is missing', () => {
        const view = buildEmaRibbonView(
            {
                indicators: {
                    ema_stack: {
                        values: { fast: 64018.2, medium: 64110.0, slow: 63980.4, long: 63845.0 },
                    },
                },
            } as any,
            null,
        );
        expect(view.distance.fast).toBeNull();
        expect(view.distance.medium).toBeNull();
        expect(view.distance.slow).toBeNull();
        expect(view.distance.long).toBeNull();
        expect(view.spread).toBeNull();
    });

    it('handles cold-start (no ema_stack entry)', () => {
        const view = buildEmaRibbonView({ indicators: {} } as any, 64000);
        expect(view.values).toEqual({ fast: null, medium: null, slow: null, long: null });
        expect(view.distance).toEqual({ fast: null, medium: null, slow: null, long: null });
        expect(view.spread).toBeNull();
    });
});

describe('fmtPctSigned', () => {
    it('formats positive values with explicit + sign', () => {
        expect(fmtPctSigned(0.0009, 2)).toBe('+0.09%');
    });

    it('formats negative values without explicit - (rendered by minus glyph)', () => {
        // Math: −0.0027 * 100 = -0.27 → toFixed(2) = '-0.27', sign already embedded.
        expect(fmtPctSigned(-0.0027, 2)).toBe('-0.27%');
    });

    it('formats zero with leading space (alignment)', () => {
        expect(fmtPctSigned(0, 2)).toBe(' 0.00%');
    });

    it('returns -- for null / non-finite', () => {
        expect(fmtPctSigned(null)).toBe('--');
        expect(fmtPctSigned(undefined)).toBe('--');
        expect(fmtPctSigned(NaN)).toBe('--');
    });
});

describe('buildEmaRibbonCellView (on-screen micro-grid)', () => {
    it('emits 4 rows in the canonical F/M/S/L order plus a spread label', () => {
        const cell = buildEmaRibbonCellView(
            {
                indicators: {
                    ema_stack: {
                        values: { fast: 64018.2, medium: 64110.0, slow: 63980.4, long: 63845.0 },
                    },
                },
            } as any,
            64000,
        );
        expect(cell.rows.map(r => r.role)).toEqual([...EMA_ROLES]);
        expect(cell.rows.find(r => r.role === 'fast')!.label).toBe('I');
        expect(cell.rows.find(r => r.role === 'medium')!.label).toBe('F');
        expect(cell.rows.find(r => r.role === 'slow')!.label).toBe('M');
        expect(cell.rows.find(r => r.role === 'long')!.label).toBe('S');
        // 5-digit prices render 1 decimal; sub-$ assets render 8 decimals;
        // the existing fmtPrice scale (per ui/src/lib/telemetry.ts:32-40).
        expect(cell.rows.find(r => r.role === 'fast')!.valueText).toBe('64018.2');
        expect(cell.ready).toBe(true);
        expect(cell.spreadText).toMatch(/^[-+ ]?/);
    });

    it('reports !ready when any line is missing (UI shows -- across the grid)', () => {
        const cell = buildEmaRibbonCellView(
            {
                indicators: {
                    ema_stack: { values: { fast: 64018.2 } },
                },
            } as any,
            64000,
        );
        expect(cell.ready).toBe(false);
        expect(cell.rows.find(r => r.role === 'medium')!.valueText).toBe('--');
        expect(cell.spreadText).toBe('--');
    });

    it('formatting precision adapts to the reference price (6-decimal sub-$ assets)', () => {
        const cell = buildEmaRibbonCellView(
            {
                indicators: {
                    ema_stack: { values: { fast: 0.012345, medium: 0.012350, slow: 0.012340, long: 0.012330 } },
                },
            } as any,
            0.012340,
        );
        // For sub-$ references the existing fmtPrice uses 8 decimals; that's
        // intentionally different from price-displayed-on-chart at cents scale.
        expect(cell.rows.find(r => r.role === 'fast')!.valueText).toMatch(/^0\.01234500$/);
    });
});
