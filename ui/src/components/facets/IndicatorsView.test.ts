// @vitest-environment jsdom
//
// Tests for the WARMING-placeholder display behavior in IndicatorsView.svelte.
//
// Regression background: the analyzer inserts a zero-valued `WARMING` placeholder
// (`raw_value = 0.0`, `normalized = 0.0`, `state_label = "WARMING"`) for every
// registered key that hasn't produced a real reading yet. The previous UI
// rendered that placeholder as `Raw 0.00 / Norm 0.00`, which misled traders
// into reading a real value out of an unread entry. The fix: when the entry
// is a WARMING placeholder, render Raw `--` and Norm `--` (regardless of
// `normalization_mode`) so the row reads as "no data yet" until a real
// reading replaces the placeholder.

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import IndicatorsView from './IndicatorsView.svelte';
import type { IndicatorDto, IndicatorMeta, IndicatorLifecycleMap, TimeframeTelemetry } from '../../types';

function makeTf(overrides: Partial<TimeframeTelemetry> = {}): TimeframeTelemetry {
    return {
        slot: 1,
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        barDurationSec: 60,
        indicators: {},
        priceText: '50000.00',
        volText: '0',
        avgVolText: '0',
        showPatterns: true,
        isCompleted: true,
        latestSnapshot: null,
        historyPrices: [],
        ...overrides,
    } as TimeframeTelemetry;
}

function warmingPlaceholder(): IndicatorDto {
    // Mirrors the placeholder the analyzer constructs at
    // `crates/market-analyzer/src/indicators/normalized/all.rs:1742`.
    return {
        raw_value: 0,
        normalized: 0,
        state_label: 'WARMING',
        values: null,
        signals: [],
        confidence: 0,
    };
}

function realReading(raw: number, normalized: number, stateLabel: string): IndicatorDto {
    return {
        raw_value: raw,
        normalized,
        state_label: stateLabel,
        values: null,
        signals: [],
        confidence: 0.5,
    };
}

function lifecycle(entries: Record<string, { state: 'Live' | 'Loading' | 'Stale' | 'Failed'; barsSeen: number; barsRequired: number; feed_state?: 'Live' | 'WaitingFeed' | 'Silent' | 'Stale' }>): IndicatorLifecycleMap {
    const out: IndicatorLifecycleMap = {};
    for (const [k, v] of Object.entries(entries)) {
        out[k] = {
            state: v.state,
            bars_seen: v.barsSeen,
            bars_required: v.barsRequired,
            stale_threshold_secs: 300,
            ...(v.feed_state !== undefined ? { feed_state: v.feed_state } : {}),
        };
    }
    return out;
}

function meta(key: string, overrides: Partial<IndicatorMeta> = {}): IndicatorMeta {
    return {
        key,
        display_name: key,
        group: 'Momentum',
        class: 'Leading',
        render: 'Pane',
        directional: true,
        supports_divergence: false,
        signal_types: [],
        default_weight: 1.0,
        default_enabled: true,
        config_params: [],
        value_format: 'decimals2',
        value_source: 'raw',
        color: '#000000',
        guide_section: '',
        ...overrides,
    };
}

beforeEach(() => {
    (globalThis as any).__appStore = { instancesMap: {} };
});

afterEach(() => {
    cleanup();
});

/**
 * The table layout renders column headers AND row cells with the same class
 * names (`colRaw`, `colNorm`, `colState`). Row cells live inside
 * `[class*="rowWrap"]` elements; headers do not. Use this helper to grab
 * only the row cells and skip the header row.
 */
function rowCells(container: HTMLElement, classFragment: string): string[] {
    const rows = container.querySelectorAll('[class*="rowWrap"]');
    const out: string[] = [];
    rows.forEach((row) => {
        const cell = row.querySelector(`[class*="${classFragment}"]`);
        if (cell?.textContent != null) out.push(cell.textContent.trim());
    });
    return out;
}

describe('IndicatorsView WARMING placeholder rendering', () => {
    it('renders -- for Raw and Norm when entry is a WARMING placeholder (Directional mode)', () => {
        const tf = makeTf({
            indicators: {
                smc_structure: warmingPlaceholder(),
            },
            indicatorLifecycle: lifecycle({
                smc_structure: { state: 'Loading', barsSeen: 12, barsRequired: 50 },
            }),
        });
        const registry = [meta('smc_structure', {
            group: 'Institutional',
            class: 'Leading',
            render: 'Marker',
            normalization_mode: 'Directional',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const raws = rowCells(container, 'colRaw');
        const norms = rowCells(container, 'colNorm');
        expect(raws.length).toBe(1);
        expect(norms.length).toBe(1);
        expect(raws[0]).toBe('--');
        expect(norms[0]).toBe('--');
    });

    it('renders -- for Raw and Norm when entry is a WARMING placeholder (ContextOnly mode)', () => {
        // Previously, ContextOnly mode rendered `N/A` in the Norm column even
        // for the WARMING placeholder — confusing, because `N/A` is the
        // canonical "non-directional gate" marker, not "no data yet".
        const tf = makeTf({
            indicators: {
                funding_rate: warmingPlaceholder(),
            },
            indicatorLifecycle: lifecycle({
                funding_rate: { state: 'Loading', barsSeen: 5, barsRequired: 1 },
            }),
        });
        const registry = [meta('funding_rate', {
            group: 'DerivativesData',
            value_format: 'percent1',
            normalization_mode: 'ContextOnly',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const raws = rowCells(container, 'colRaw');
        const norms = rowCells(container, 'colNorm');
        expect(raws[0]).toBe('--');
        expect(norms[0]).toBe('--');
    });

    it('renders real values when the entry is a non-WARMING reading (regression guard)', () => {
        // After the WARMING-aware fix, real readings must still render their
        // actual values; the guard must not over-trigger.
        const tf = makeTf({
            indicators: {
                rsi: realReading(62.4, 0.42, 'RSI_BULLISH_BUT_NOT_OVERBOUGHT'),
            },
            indicatorLifecycle: lifecycle({
                rsi: { state: 'Live', barsSeen: 100, barsRequired: 14 },
            }),
        });
        const registry = [meta('rsi', { group: 'Momentum', value_format: 'decimals2' })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const raws = rowCells(container, 'colRaw');
        const norms = rowCells(container, 'colNorm');
        expect(raws[0]).toBe('62.40');
        expect(norms[0]).toBe('0.42');
    });

    it('renders N/A in Norm for ContextOnly real readings (not WARMING)', () => {
        // ContextOnly mode should still render `N/A` for real entries (the
        // canonical contract: normalized is contractually 0.0).
        const tf = makeTf({
            indicators: {
                bbwp: realReading(50.0, 0.0, 'NORMAL_VOLATILITY_BULL_CYCLE'),
            },
            indicatorLifecycle: lifecycle({
                bbwp: { state: 'Live', barsSeen: 100, barsRequired: 252 },
            }),
        });
        const registry = [meta('bbwp', {
            group: 'Volatility',
            value_format: 'decimals2',
            normalization_mode: 'ContextOnly',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const raws = rowCells(container, 'colRaw');
        const norms = rowCells(container, 'colNorm');
        expect(raws[0]).toBe('50.00');
        expect(norms[0]).toBe('N/A');
    });

    it('renders Warming (X/Y) in State column when lifecycle is Loading', () => {
        const tf = makeTf({
            indicators: {
                smc_structure: warmingPlaceholder(),
            },
            indicatorLifecycle: lifecycle({
                smc_structure: { state: 'Loading', barsSeen: 12, barsRequired: 50 },
            }),
        });
        const registry = [meta('smc_structure', {
            group: 'Institutional',
            class: 'Leading',
            render: 'Marker',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const states = rowCells(container, 'colState');
        expect(states[0]).toContain('Warming');
        expect(states[0]).toContain('12/50');
    });

    it('renders WAITING FEED ⏳ in State column when lifecycle is Live but feed has not arrived (v6.6+)', () => {
        // v6.6: distinguishes "feed hasn't arrived yet" (WaitingFeed)
        // from "feed says zero" (SILENT ⚡). This is the exact symptom
        // the user reported: Bitget ticker channel's `holdingAmount`
        // was absent on cold start → OI / funding / OI-Δ / OI-Price
        // Divergence all showed SILENT ⚡ when they should have shown
        // WAITING FEED ⏳.
        const tf = makeTf({
            indicators: {
                // No value-map entry: only a placeholder so the row renders.
                // In production the entry is absent (per the analyzer's
                // WaitingFeed contract) so we pass an empty indicators map.
            },
            indicatorLifecycle: lifecycle({
                open_interest: { state: 'Live', barsSeen: 100, barsRequired: 1, feed_state: 'WaitingFeed' },
                funding_rate: { state: 'Live', barsSeen: 100, barsRequired: 1, feed_state: 'WaitingFeed' },
                oi_delta: { state: 'Live', barsSeen: 100, barsRequired: 1, feed_state: 'WaitingFeed' },
                oi_price_divergence: { state: 'Live', barsSeen: 100, barsRequired: 1, feed_state: 'WaitingFeed' },
            }),
        });
        const registry = [
            meta('open_interest', { group: 'DerivativesData', value_format: 'usd_notional' }),
            meta('funding_rate', { group: 'DerivativesData', value_format: 'percent1' }),
            meta('oi_delta', { group: 'DerivativesData', value_format: 'decimals2' }),
            meta('oi_price_divergence', { group: 'DerivativesData', value_format: 'decimals2' }),
        ];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const states = rowCells(container, 'colState');
        expect(states.length).toBe(4);
        for (const s of states) {
            expect(s).toContain('WAITING FEED');
        }
    });
});

// ── EMA Ribbon micro-grid on the `ema_stack` collapsed `raw_value` cell ──
//
// Single source of truth: the same `tf.indicators["ema_stack"].values.*`
// record drives the chart overlay, this micro-grid, and the export body's
// `body.ema` block. These tests cover the screen-side rendering.
describe('IndicatorsView EMA Ribbon micro-grid', () => {
    function realEmaReading(values: { fast: number; medium: number; slow: number; long: number }): IndicatorDto {
        return {
            raw_value: values.fast,
            normalized: 1.0,
            state_label: 'ESTABLISHED_BULLISH_STACK',
            confidence: 0.9,
            signals: [],
            values: { ...values },
        };
    }

    function partialEmaReading(values: Partial<{ fast: number; medium: number; slow: number; long: number }>): IndicatorDto {
        return {
            raw_value: values.fast ?? 0,
            normalized: 0,
            state_label: 'WARMING',
            confidence: 0,
            values: {
                ...(values.fast != null ? { fast: values.fast } : {}),
                ...(values.medium != null ? { medium: values.medium } : {}),
                ...(values.slow != null ? { slow: values.slow } : {}),
                ...(values.long != null ? { long: values.long } : {}),
            },
            signals: [],
        };
    }

    it('renders all 4 EMA lines + signed distance + spread sub-label on the `ema_stack` row', () => {
        const tf = makeTf({
            indicators: {
                ema_stack: realEmaReading({ fast: 64018.2, medium: 64110.0, slow: 63980.4, long: 63845.0 }),
            },
            priceText: '64000.00',
        });
        const registry = [meta('ema_stack', {
            group: 'Trend',
            class: 'Lagging',
            render: 'PriceOverlay',
            value_format: 'price',
            value_source: 'sub:fast',
            normalization_mode: 'Directional',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const ribbon = container.querySelector('[class*="emaRibbon"]');
        expect(ribbon).not.toBeNull();
        const text = (ribbon?.textContent ?? '').replace(/\s+/g, ' ').trim();
        // All four labels must be present (I/F/M/S).
        expect(text).toContain('I');
        expect(text).toContain('F');
        expect(text).toContain('M');
        expect(text).toContain('S');
        // All four formatted prices render (formatted by fmtPrice against
        // the refPrice 64000, which is 1-decimal scale for >=$10k prices).
        expect(text).toContain('64018.2');
        expect(text).toContain('64110.0');
        expect(text).toContain('63980.4');
        expect(text).toContain('63845.0');
        // Per-line distance_from_price (signed %): with refPrice=64000,
        //   fast  (64000 - 64018.2) / 64000 = -0.0284% → '-0.03%'
        //   medium (64000 - 64110.0) / 64000 = -0.1719% → '-0.17%'
        //   slow   (64000 - 63980.4) / 64000 = +0.0306% → '+0.03%'
        //   long   (64000 - 63845.0) / 64000 = +0.2422% → '+0.24%'
        expect(text).toContain('-0.03%');
        expect(text).toContain('-0.17%');
        expect(text).toContain('+0.03%');
        expect(text).toContain('+0.24%');
        // The spread = (64018.2 - 63845) / 64000 ≈ +0.2703% → '+0.27%'.
        expect(text).toContain('spread ↔');
        expect(text).toMatch(/0\.2[5-9]%/);
    });

    it('shows -- across the grid when the EMA ribbon has not warmed up (cold start)', () => {
        // True cold-start: no `ema_stack` entry at all → readEmaValues
        // returns all-null → every value in the micro-grid is '--' and
        // the spread is '--'.
        const tf = makeTf({
            indicators: {},
            priceText: '64000.00',
        });
        const registry = [meta('ema_stack', {
            group: 'Trend',
            class: 'Lagging',
            render: 'PriceOverlay',
            value_format: 'price',
            value_source: 'sub:fast',
            normalization_mode: 'Directional',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const ribbon = container.querySelector('[class*="emaRibbon"]');
        const text = (ribbon?.textContent ?? '').replace(/\s+/g, ' ').trim();
        // Five -- placeholders should appear (4 lines + spread suffix).
        expect((text.match(/--/g) ?? []).length).toBeGreaterThanOrEqual(5);
        expect(text).toContain('spread ↔');
    });

    it('does NOT alter rendering of any other indicator (regression)', () => {
        // The collapsed `raw_value` cell for any non-`ema_stack` indicator
        // still goes through `formatRaw()` and renders the single-scalar
        // `raw_value`. The micro-grid is `ema_stack`-only.
        const tf = makeTf({
            indicators: {
                rsi_14: realReading(62.4, 0.24, 'BULLISH'),
            },
            priceText: '64000.00',
        });
        const registry = [meta('rsi_14', { group: 'Momentum', value_format: 'decimals2' })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        // No `emaRibbon` element should appear when the only row is RSI.
        const ribbon = container.querySelector('[class*="emaRibbon"]');
        expect(ribbon).toBeNull();

        // The single-scalar 62.40 renders as the raw_value cell.
        const raws = rowCells(container, 'colRaw');
        expect(raws.length).toBe(1);
        expect(raws[0]).toBe('62.40');
    });

    it('v6.10.21: price_trend_sharpe Raw cell is tinted by the normalized sign', () => {
        // Friction fix: the unbounded annualized raw (e.g. -6.45) is now
        // color-coded via `normColor(normalized)` so an operator reads the
        // direction without cross-referencing the Norm/State columns.
        // Band mapping: |n| ≥ 0.9 → extreme (#c084fc), > 0.1 → bullish
        // (#4ade80), < -0.1 → bearish (#f87171).
        const tf = makeTf({
            indicators: {
                price_trend_sharpe: realReading(-6.45, -0.5, 'STRONG_NEGATIVE_SHARPE'),
            },
        });
        const registry = [meta('price_trend_sharpe', {
            group: 'Regime',
            class: 'Lagging',
            value_format: 'ratio2',
        })];

        const { container } = render(IndicatorsView, {
            props: { tf, registry },
        });

        const rawCell = container.querySelector('[class*="rowWrap"] [class*="colRaw"]') as HTMLElement | null;
        expect(rawCell?.textContent?.trim()).toBe('-6.45');
        expect(rawCell?.getAttribute('style')).toContain('rgb(248, 113, 113)'); // bearish #f87171

        // Positive reading tints bullish.
        const tfBull = makeTf({
            indicators: {
                price_trend_sharpe: realReading(2.4, 0.5, 'STRONG_POSITIVE_SHARPE'),
            },
        });
        const { container: c2 } = render(IndicatorsView, {
            props: { tf: tfBull, registry },
        });
        const rawCell2 = c2.querySelector('[class*="rowWrap"] [class*="colRaw"]') as HTMLElement | null;
        expect(rawCell2?.getAttribute('style')).toContain('rgb(74, 222, 128)'); // bullish #4ade80

        // Clamped-to-band normalized (-1.0 / +1.0) tints the extreme color.
        const tfExtreme = makeTf({
            indicators: {
                price_trend_sharpe: realReading(-10.42, -1, 'STRONG_NEGATIVE_SHARPE'),
            },
        });
        const { container: c4 } = render(IndicatorsView, {
            props: { tf: tfExtreme, registry },
        });
        const rawCell4 = c4.querySelector('[class*="rowWrap"] [class*="colRaw"]') as HTMLElement | null;
        expect(rawCell4?.getAttribute('style')).toContain('rgb(192, 132, 252)'); // extreme #c084fc

        // Non-sharpe rows stay untinted.
        const tfRsi = makeTf({
            indicators: { rsi_14: realReading(62.4, 0.24, 'BULLISH') },
        });
        const registryRsi = [meta('rsi_14', { group: 'Momentum', value_format: 'decimals2' })];
        const { container: c3 } = render(IndicatorsView, {
            props: { tf: tfRsi, registry: registryRsi },
        });
        const rsiCell = c3.querySelector('[class*="rowWrap"] [class*="colRaw"]') as HTMLElement | null;
        expect(rsiCell?.getAttribute('style') ?? '').not.toContain('color:');
    });
});